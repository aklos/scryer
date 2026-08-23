/// Cheap, agent-free drift status: nodes with OUT-OF-PLAN regressions to mapped
/// code — a committed anchor that broke or changed since the last reconcile with
/// no pending plan item to explain it (see `anchors::out_of_plan_scopes`). Stays
/// quiet while the plan is being implemented (that churn is expected) and ticks
/// when code diverges from the model outside the plan. Used to nudge the user to
/// run a semantic check — it never decides the model drifted, only where to look.
#[tauri::command]
pub(crate) fn get_drift_status(cwd: String) -> Result<Vec<scryer_core::drift::DriftScope>, String> {
    let project = std::path::Path::new(&cwd);
    let model_ref = scryer_core::ModelRef::ProjectLocal(project.to_path_buf());

    // A model that has never been reconciled has no `.sync` anchor, so the
    // baseline defaults to epoch 0 and *every* file reads as "changed since
    // reconcile" — flagging every boundary-owning node as drift, forever. There
    // is no baseline to diff against, so that verdict is pure noise. Seed the
    // anchor to the current commit/time (treat the model as in-sync as of now)
    // and report nothing; real drift then surfaces once code changes after this
    // point. Models built through the MCP tools land here, since only the in-app
    // build and drift-check completion write the anchor.
    if !model_ref.sync_path().exists() {
        let _ = scryer_core::write_sync_state(
            &model_ref,
            &scryer_core::drift::SyncState::anchored_now(scryer_core::drift::head_commit(project)),
        );
        let _ = scryer_extract::anchors::write_baseline(&model_ref);
        return Ok(Vec::new());
    }

    scryer_extract::anchors::out_of_plan_scopes(&model_ref)
}

/// Everything the observability surfaces read, in one deterministic pass — no
/// agent involved, no git. Composes the per-node health rollup (computed
/// discharge + anchor coverage + boundary darkness), the anchor fingerprint
/// check (changed/broken anchors, with moved-but-unchanged symbols silently
/// re-anchored as a side effect), and the link evidence (declared-link audit +
/// unmodeled candidates). Runs the extractor, so it also refreshes the
/// `.build_edges.json` cache the MCP commit tool reads.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModelHealthReport {
    health: scryer_core::health::ModelHealth,
    /// Per-node build completeness (anchored primitives over authored ones),
    /// keyed by node id. Spans committed + planned, so it is defined from
    /// greenfield onward.
    completeness: std::collections::BTreeMap<String, scryer_core::health::Completeness>,
    /// Anchors whose code changed/broke since the last reconcile.
    anchors: Vec<scryer_extract::anchors::AnchorObservation>,
    /// Anchors silently healed this pass (symbol moved, content unchanged).
    reanchored: usize,
    derived: scryer_core::build_edges::DerivedGraph,
}

#[tauri::command]
pub(crate) async fn get_model_health(cwd: String) -> Result<ModelHealthReport, String> {
    // The extractor parses the whole repo (seconds on a big project) — keep it
    // off the IPC thread so the UI stays responsive while the report computes.
    tauri::async_runtime::spawn_blocking(move || {
        let project = std::path::Path::new(&cwd);
        let model_ref = scryer_core::ModelRef::ProjectLocal(project.to_path_buf());

        // Check (and self-heal) anchors first: re-anchoring may update sourceMap
        // line ranges, and the health/evidence below should see the healed model.
        let check = scryer_extract::anchors::check_anchors(&model_ref)?;
        let model = scryer_core::read_model_at(&model_ref)?;

        let (ctx, _) = scryer_extract::extract_context_with_stats(project)?;
        let files: std::collections::BTreeSet<String> =
            ctx.files.iter().map(|f| f.rel_path.clone()).collect();
        let edges = scryer_core::build_edges::BuildEdges {
            symbol_edges: ctx
                .symbol_edges
                .iter()
                .map(|e| scryer_core::build_edges::CachedEdge {
                    src: e.src.clone(),
                    dst: e.dst.clone(),
                })
                .collect(),
        };
        // Keep the cross-process cache fresh for the MCP commit tool. Best-effort.
        let _ = scryer_core::build_edges::write_build_edges(project, &edges);

        // Completeness spans the authored model, so resolve the plan against real
        // files. Boundary globs can own files the parser skips (config/assets), so
        // match against the full inventory, not just the parsed source set.
        let planned = scryer_core::read_planned_at(&model_ref).unwrap_or_else(|_| model.clone());
        let all_files = scryer_extract::list_project_files(project);
        let dead: std::collections::HashSet<&str> = check
            .observations
            .iter()
            .filter(|o| {
                matches!(
                    o.state,
                    scryer_extract::anchors::AnchorState::Broken
                        | scryer_extract::anchors::AnchorState::FileMissing
                )
            })
            .map(|o| o.key.as_str())
            .collect();
        let completeness =
            scryer_core::health::resolve_completeness(&model, &planned, &all_files, &dead);

        Ok(ModelHealthReport {
            health: scryer_core::health::compute_health(&model, Some(&planned), Some(&files)),
            completeness,
            anchors: check.observations,
            reanchored: check.reanchored,
            derived: scryer_core::build_edges::derive_graph(&model, &edges),
        })
    })
    .await
    .map_err(|e| format!("health task failed: {e}"))?
}

/// Per-claim test verdicts from the status cache, each re-verified against the
/// working tree as it stands (fingerprint compare with an mtime fast path, so
/// the steady state costs stats, not parses). Empty until a test report has
/// been ingested. The canvas colors claim rows from this — quiet when green,
/// amber when stale, red when failing.
#[tauri::command]
pub(crate) async fn get_test_statuses(
    cwd: String,
) -> Result<Vec<scryer_extract::test_status::ClaimTestStatus>, String> {
    // A cold check after wide edits re-hashes many files — keep it off the IPC
    // thread like the health report.
    tauri::async_runtime::spawn_blocking(move || {
        let model_ref = scryer_core::ModelRef::ProjectLocal(std::path::PathBuf::from(&cwd));
        scryer_extract::test_status::test_statuses(&model_ref)
    })
    .await
    .map_err(|e| format!("test-status task failed: {e}"))?
}

/// Dismiss the current drift nudge without running a semantic check: advance the
/// reconcile anchor to now (the same write `start_drift_check` does on
/// completion). The user is asserting "I've looked, these changes are fine" —
/// the changed scopes stop surfacing until code changes again. The cheap
/// counterpart to running the agent over them.
#[tauri::command]
pub(crate) fn reconcile_drift(cwd: String) -> Result<(), String> {
    let project = std::path::Path::new(&cwd);
    let model_ref = scryer_core::ModelRef::ProjectLocal(project.to_path_buf());
    scryer_core::write_sync_state(
        &model_ref,
        &scryer_core::drift::SyncState::anchored_now(scryer_core::drift::head_commit(project)),
    )?;
    // Re-fingerprint: "reconciled" means the anchors as they stand are the truth.
    scryer_extract::anchors::write_baseline(&model_ref).map(|_| ())
}

/// Reconcile drift for a single node and its whole subtree, without moving the
/// project-wide anchor. Records a per-node anchor (`now` / HEAD) for the node and
/// every descendant, so their boundaries' changes stop reading as drift while the
/// rest of the model keeps whatever drift it had. The user's "I looked, this part
/// is fine" verdict, scoped to what they were looking at.
#[tauri::command]
pub(crate) fn reconcile_drift_node(cwd: String, node_id: String) -> Result<(), String> {
    let project = std::path::Path::new(&cwd);
    let model_ref = scryer_core::ModelRef::ProjectLocal(project.to_path_buf());
    let model = scryer_core::read_model_at(&model_ref).map_err(|e| e.to_string())?;
    let mut sync = scryer_core::read_sync_state(&model_ref);
    // Deletions the dismissal reconciles: inventory files currently absent.
    // They stop counting for this subtree while other owners still see them.
    let missing: std::collections::BTreeSet<String> = sync
        .files
        .iter()
        .filter(|f| !project.join(f).exists())
        .cloned()
        .collect();
    let anchor =
        scryer_core::drift::NodeAnchor::now(scryer_core::drift::head_commit(project), missing);
    for id in scryer_core::drift::subtree_ids(&model, &node_id) {
        sync.nodes.insert(id, anchor.clone());
    }
    scryer_core::write_sync_state(&model_ref, &sync)
}

#[cfg(test)]
mod tests {
    use scryer_core::{ModelRef, ScryModel};

    const ALPHA: &str = "export function alpha() {\n    return 1;\n}\n";
    const BETA: &str = "export function beta() {\n    return 2;\n}\n";

    /// A project whose model anchors one symbol node per given (id, symbol,
    /// file) triple, identical in both layers.
    fn project(files: &[(&str, &str, &str)]) -> (tempfile::TempDir, ModelRef, String) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        let mut m = ScryModel::new();
        m.nodes.push(
            serde_json::from_value(
                serde_json::json!({ "id": "node-1", "kind": "system", "name": "Acme" }),
            )
            .unwrap(),
        );
        for (i, (id, symbol, file)) in files.iter().enumerate() {
            let content = if *symbol == "alpha" { ALPHA } else { BETA };
            std::fs::write(dir.path().join(file), content).unwrap();
            let mut sym: scryer_core::Node = serde_json::from_value(serde_json::json!({
                "id": id, "kind": "symbol", "name": symbol, "parentId": "node-1"
            }))
            .unwrap();
            let rid = format!("resp-{i}");
            sym.responsibilities = vec![serde_json::from_value(
                serde_json::json!({ "id": rid, "statement": format!("does {symbol}") }),
            )
            .unwrap()];
            m.nodes.push(sym);
            m.source_map.insert(
                rid,
                vec![serde_json::from_value(
                    serde_json::json!({ "pattern": file, "symbol": symbol, "line": 1, "endLine": 3 }),
                )
                .unwrap()],
            );
        }
        let r = ModelRef::ProjectLocal(dir.path().to_path_buf());
        scryer_core::write_model_at(&r, &m).unwrap();
        scryer_core::write_planned_at(&r, &m).unwrap();
        let cwd = dir.path().to_string_lossy().to_string();
        (dir, r, cwd)
    }

    /// The sync anchor is nanosecond-precise; a token gap keeps a later edit
    /// strictly after it even on coarse-clock filesystems.
    fn touch_gate() {
        std::thread::sleep(std::time::Duration::from_millis(25));
    }

    /// With no reconcile baseline, the first status check seeds the anchor to
    /// now and reports NOTHING — not everything.
    #[test]
    fn a_first_status_check_seeds_the_anchor_and_reports_nothing() {
        let (_dir, r, cwd) = project(&[("node-2", "alpha", "src/alpha.ts")]);
        assert!(!r.sync_path().exists());
        let scopes = super::get_drift_status(cwd).unwrap();
        assert!(scopes.is_empty(), "{scopes:?}");
        assert!(r.sync_path().exists(), "the anchor is seeded for next time");
    }

    /// After the seed, an out-of-plan regression to mapped code — an anchored
    /// definition gone with no plan item explaining it — is reported.
    #[test]
    fn status_reports_out_of_plan_regressions_to_mapped_code() {
        let (dir, _r, cwd) = project(&[("node-2", "alpha", "src/alpha.ts")]);
        assert!(super::get_drift_status(cwd.clone()).unwrap().is_empty());

        touch_gate();
        std::fs::write(dir.path().join("src/alpha.ts"), BETA).unwrap();
        let scopes = super::get_drift_status(cwd).unwrap();
        assert_eq!(scopes.len(), 1, "{scopes:?}");
        assert_eq!(scopes[0].node_id, "node-2");
    }

    /// Dismissing the nudge advances the reconcile anchor without any check:
    /// the same drift stops surfacing until code changes again.
    #[test]
    fn dismissing_the_nudge_advances_the_anchor_without_a_check() {
        let (dir, _r, cwd) = project(&[("node-2", "alpha", "src/alpha.ts")]);
        assert!(super::get_drift_status(cwd.clone()).unwrap().is_empty());
        touch_gate();
        std::fs::write(dir.path().join("src/alpha.ts"), BETA).unwrap();
        assert_eq!(super::get_drift_status(cwd.clone()).unwrap().len(), 1);

        super::reconcile_drift(cwd.clone()).unwrap();
        assert!(super::get_drift_status(cwd).unwrap().is_empty(), "dismissed drift stays quiet");
    }

    /// Dismissing drift for ONE node records a per-node reconcile anchor for
    /// it and every descendant — and for nothing else.
    #[test]
    fn a_node_dismissal_records_an_anchor_for_the_whole_subtree() {
        let (dir, r, cwd) =
            project(&[("node-2", "alpha", "src/alpha.ts"), ("node-3", "beta", "src/beta.ts")]);
        // Give node-2 a child so "whole subtree" is observable.
        let mut m = scryer_core::read_model_at(&r).unwrap();
        m.nodes.push(
            serde_json::from_value(
                serde_json::json!({ "id": "node-4", "kind": "symbol", "name": "inner", "parentId": "node-2" }),
            )
            .unwrap(),
        );
        scryer_core::write_model_at(&r, &m).unwrap();
        assert!(super::get_drift_status(cwd.clone()).unwrap().is_empty());
        touch_gate();
        std::fs::write(dir.path().join("src/alpha.ts"), "export function alpha() {\n    return 9;\n}\n").unwrap();

        super::reconcile_drift_node(cwd.clone(), "node-2".into()).unwrap();

        let sync = scryer_core::read_sync_state(&r);
        assert!(sync.nodes.contains_key("node-2"), "the node gets its anchor");
        assert!(sync.nodes.contains_key("node-4"), "…and so does its whole subtree");
        assert!(!sync.nodes.contains_key("node-3"), "an unrelated node is untouched");
    }

    /// The health report checks and SELF-HEALS anchors first (a moved symbol is
    /// re-anchored), then computes completeness against the healed model.
    #[test]
    fn health_self_heals_anchors_then_computes() {
        let (dir, r, cwd) = project(&[("node-2", "alpha", "src/alpha.ts")]);
        assert!(super::get_drift_status(cwd.clone()).unwrap().is_empty());
        touch_gate();
        // Push alpha down; its text is unchanged — heal, not drift.
        std::fs::write(dir.path().join("src/alpha.ts"), format!("// pad\n// pad\n{ALPHA}")).unwrap();

        let report = tauri::async_runtime::block_on(super::get_model_health(cwd)).unwrap();
        assert_eq!(report.reanchored, 1, "moved symbol healed");
        assert!(report.anchors.is_empty(), "healed, so no observations");
        assert!(!report.completeness.is_empty(), "completeness computed");
        let healed = scryer_core::read_model_at(&r).unwrap();
        assert_eq!(healed.source_map["resp-0"][0].line, Some(3), "the model saw the heal");
    }
}
