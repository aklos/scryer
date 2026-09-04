use std::sync::Mutex;

use notify::{recommended_watcher, EventKind, RecursiveMode, Watcher};
use tauri::{Emitter, Manager};

use crate::hooks;
use crate::state::WatcherState;

/// True if the given project has a `.scryer/model.scry` whose version is not
/// the current v0.3 schema. Frontend uses this to surface a clear error.
#[tauri::command]
pub(crate) fn is_legacy_model(project_path: String) -> bool {
    scryer_core::is_legacy_model(std::path::Path::new(&project_path))
}

/// Watch `{project}/.scryer/` for model changes. Replaces any previous watcher.
#[tauri::command]
pub(crate) fn watch_project(
    ref_str: String,
    app: tauri::AppHandle,
    watcher_state: tauri::State<'_, Mutex<WatcherState>>,
) -> Result<(), String> {
    let model_ref = scryer_core::ModelRef::parse(&ref_str)?;
    let mut state = watcher_state.lock().unwrap();

    let target_dir = match &model_ref {
        scryer_core::ModelRef::ProjectLocal(path) => path.join(".scryer"),
    };

    if let Some((ref current, _)) = &state.project {
        if *current == target_dir {
            return Ok(());
        }
    }

    state.project = None;

    // The session-hook endpoint follows the watched project: opening a project
    // brings it up, switching projects replaces it (the old server's Drop
    // removes its discovery file, so its hooks fall silent).
    let scryer_core::ModelRef::ProjectLocal(ref project_path) = model_ref;
    {
        let hook_state = app.state::<hooks::HookState>();
        let mut hook = hook_state.0.lock().unwrap();
        let already = hook
            .as_ref()
            .is_some_and(|s| s.project() == project_path.as_path());
        if !already {
            *hook = None; // drop the old endpoint before starting the new one
            // Touches stream to the canvas as "hook-touch" events — the live
            // "a session is working here" signal.
            let touch_handle = app.clone();
            let on_touch = move |t: &hooks::Touch| {
                let _ = touch_handle.emit("hook-touch", t);
            };
            // A close gate that fires is review work: the inbox shows its
            // needs-reconcile items live as "hook-close-gate" events.
            let gate_handle = app.clone();
            let on_close_gate = move |payload: &serde_json::Value| {
                let _ = gate_handle.emit("hook-close-gate", payload);
            };
            match hooks::start(project_path, on_touch, on_close_gate) {
                Ok(server) => {
                    eprintln!("[hooks] session endpoint on 127.0.0.1:{}", server.port);
                    *hook = Some(server);
                }
                Err(e) => eprintln!("[hooks] endpoint not started: {e}"),
            }
        }
    }

    let _ = std::fs::create_dir_all(&target_dir);
    let handle = app.clone();
    let ref_string = ref_str.clone();
    // Passive test-report ingestion: the same watcher also covers the
    // project's report directories, and any XML written under one is ingested
    // after a short settle. Only files that CHANGE while watching count — the
    // event-driven design is what guarantees no pre-existing (older-code)
    // report is ever swept in.
    let report_dirs = crate::test_reports::report_dirs(project_path);
    let debounce =
        crate::test_reports::ReportDebounce::new(std::time::Duration::from_millis(800));
    let project_root = project_path.clone();
    let mut watcher =
        recommended_watcher(move |res: Result<notify::Event, notify::Error>| {
            let Ok(event) = res else { return };
            if !matches!(event.kind, EventKind::Create(_) | EventKind::Modify(_)) {
                return;
            }
            for path in &event.paths {
                // The test-status cache lives beside the model files; an agent
                // ingesting a report mid-session must light the verdict badges
                // without waiting for the session to end.
                if path.file_name().is_some_and(|n| n == ".test-results.json") {
                    let _ = handle.emit("test-results-changed", ref_string.clone());
                    continue;
                }
                if path.extension().is_some_and(|e| e == "xml")
                    && report_dirs.iter().any(|d| path.starts_with(d))
                {
                    debounce.schedule(project_root.clone(), path.clone());
                    continue;
                }
                if path.extension().map_or(true, |e| e != "scry") {
                    continue;
                }
                let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                    continue;
                };
                if stem.ends_with(".baseline") || stem.starts_with(".tmp") {
                    continue;
                }
                let _ = handle.emit("model-changed", ref_string.clone());
            }
        })
        .map_err(|e| e.to_string())?;

    watcher
        .watch(&target_dir, RecursiveMode::NonRecursive)
        .map_err(|e| e.to_string())?;
    // Report directories are best-effort: a vanished one must not break the
    // model watch that everything else depends on.
    for dir in crate::test_reports::report_dirs(project_path) {
        let _ = watcher.watch(&dir, RecursiveMode::Recursive);
    }

    state.project = Some((target_dir, watcher));
    Ok(())
}

#[tauri::command]
pub(crate) fn read_model(ref_str: String) -> Result<String, String> {
    let model_ref = scryer_core::ModelRef::parse(&ref_str)?;
    scryer_core::read_model_raw_at(&model_ref)
}

/// Read the planned (draft) layer — the working model the canvas edits. Returns
/// the committed model's SEEDED bytes when no plan has diverged yet (planned ==
/// model, anchors cleared), so a fresh project opens with an empty plan.
#[tauri::command]
pub(crate) fn read_planned(ref_str: String) -> Result<String, String> {
    let model_ref = scryer_core::ModelRef::parse(&ref_str)?;
    // Heal legacy shadow drafts before the canvas loads one: whatever the
    // frontend loads it echoes back on save, so a pre-seeding draft would keep
    // re-minting its shadow anchors forever. No-op (and lock-free) when clean.
    let _ = scryer_core::heal_shadow_draft(&model_ref);
    scryer_core::read_planned_raw_at(&model_ref)
}

/// Write the planned (draft) layer. The canvas saves here, never to `model.scry`
/// directly: the committed model only changes when the agent implements a plan
/// element and folds it (planned → model). Serialized against MCP writes, like
/// the committed-model write.
#[tauri::command]
pub(crate) fn write_planned(ref_str: String, data: String) -> Result<(), String> {
    let model_ref = scryer_core::ModelRef::parse(&ref_str)?;
    let _lock = scryer_core::lock_model(&model_ref)?;
    // A canvas save is the DEVELOPER editing the plan — intent by definition.
    // Re-stamp every signed-off change's snapshot so their edits never read
    // as the agent's amendments at the next fold. Only a plan that carries a
    // sign-off is re-serialized; otherwise the echo lands verbatim as before.
    if let Ok(mut plan) = serde_json::from_str::<scryer_core::ScryModel>(&data) {
        if plan.changes.iter().any(|c| c.signed_off.is_some()) {
            scryer_core::changes::restamp_signoffs(&mut plan, scryer_core::drift::now_secs());
            let json = serde_json::to_string_pretty(&plan).map_err(|e| e.to_string())?;
            return scryer_core::write_planned_raw_at(&model_ref, &json);
        }
    }
    scryer_core::write_planned_raw_at(&model_ref, &data)
}

/// SIGN OFF a change from the canvas: snapshot its tagged entries as the
/// developer-approved intent (see `scryer_core::changes::sign_off`). From here
/// on, a claim the agent rewords or adds under the change lands as vagrant for
/// the developer's verdict at the fold instead of folding. Returns how many
/// entries the snapshot captured.
#[tauri::command]
pub(crate) fn sign_off_change(ref_str: String, change_id: String) -> Result<usize, String> {
    let model_ref = scryer_core::ModelRef::parse(&ref_str)?;
    let _lock = scryer_core::lock_model(&model_ref)?;
    let mut plan = scryer_core::read_planned_seeded_at(&model_ref)?;
    let n = scryer_core::changes::sign_off(&mut plan, &change_id, scryer_core::drift::now_secs())?;
    scryer_core::write_planned_at(&model_ref, &plan)?;
    Ok(n)
}

/// The fold-refusal ledger: every claim `mark_implemented` last declined to
/// fold, with the missing fact it was refused for. Read by the inbox; a
/// refusal clears when the same claim folds or leaves the plan.
#[tauri::command]
pub(crate) fn read_fold_refusals(
    ref_str: String,
) -> Result<Vec<scryer_core::refusals::Refusal>, String> {
    let model_ref = scryer_core::ModelRef::parse(&ref_str)?;
    Ok(scryer_core::refusals::read_refusals(&model_ref))
}

/// Close an EMPTY open change (a stranded ledger) from the canvas. Goes through
/// core rather than the raw plan echo so the "abandoned" history record lands;
/// the plan write fires the watcher, which refreshes every surface.
#[tauri::command]
pub(crate) fn close_change(ref_str: String, change_id: String) -> Result<(), String> {
    let model_ref = scryer_core::ModelRef::parse(&ref_str)?;
    let _lock = scryer_core::lock_model(&model_ref)?;
    scryer_core::changes::close_change(&model_ref, &change_id).map(|_| ())
}

/// Read the durable committed-model history log (`.scryer/history.jsonl`),
/// returned as a JSON array of events, oldest first. Empty when the project has
/// no history yet. The frontend re-reads this whenever the model changes (every
/// event-producing agent operation also writes a `.scry` file the watcher sees).
#[tauri::command]
pub(crate) fn read_history(ref_str: String) -> Result<String, String> {
    let model_ref = scryer_core::ModelRef::parse(&ref_str)?;
    let events = scryer_core::history::read_history(&model_ref);
    serde_json::to_string(&events).map_err(|e| e.to_string())
}

/// Create a blank project-local model at `{project_path}/.scryer/model.scry`.
/// Returns the ModelRef string.
#[tauri::command]
pub(crate) fn create_blank_model(project_path: String) -> Result<String, String> {
    let project = std::path::Path::new(&project_path);
    if !project.exists() || !project.is_dir() {
        return Err(format!(
            "Project path does not exist or is not a directory: {}",
            project_path
        ));
    }
    let model_ref = scryer_core::ModelRef::ProjectLocal(project.to_path_buf());
    let _lock = scryer_core::lock_model(&model_ref)?;
    let model = scryer_core::ScryModel::new();
    scryer_core::write_model_at(&model_ref, &model)?;
    Ok(model_ref.to_ref_string())
}

#[tauri::command]
pub(crate) fn get_subagent_settings() -> scryer_core::SubagentSettings {
    scryer_core::read_subagent_settings()
}

#[tauri::command]
pub(crate) fn set_subagent_settings(settings: scryer_core::SubagentSettings) -> Result<(), String> {
    scryer_core::write_subagent_settings(&settings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use scryer_core::{ModelRef, ScryModel};

    fn committed_project() -> (tempfile::TempDir, ModelRef, String) {
        let dir = tempfile::tempdir().unwrap();
        let r = ModelRef::ProjectLocal(dir.path().to_path_buf());
        let mut m = ScryModel::new();
        let mut node: scryer_core::Node = serde_json::from_value(
            serde_json::json!({ "id": "node-1", "kind": "system", "name": "Acme" }),
        )
        .unwrap();
        node.responsibilities = vec![serde_json::from_value(
            serde_json::json!({ "id": "resp-1", "statement": "does the thing" }),
        )
        .unwrap()];
        m.nodes.push(node);
        m.source_map.insert(
            "resp-1".into(),
            vec![serde_json::from_value(serde_json::json!({ "pattern": "src/a.rs" })).unwrap()],
        );
        scryer_core::write_model_at(&r, &m).unwrap();
        let ref_str = r.to_ref_string();
        (dir, r, ref_str)
    }

    /// The canvas load heals a legacy shadow draft first: a plan that mirrors
    /// committed's anchors verbatim loses the shadow before the canvas can
    /// echo it back on save.
    #[test]
    fn canvas_read_heals_a_legacy_shadow_draft_first() {
        let (_dir, r, ref_str) = committed_project();
        // A pre-seeding draft: identical content, committed's source_map shadowed.
        let committed = scryer_core::read_model_at(&r).unwrap();
        scryer_core::write_planned_raw_at(&r, &serde_json::to_string(&committed).unwrap())
            .unwrap();

        let raw = super::read_planned(ref_str).unwrap();
        let plan: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert!(
            plan["sourceMap"].as_object().is_none_or(|m| m.is_empty()),
            "the shadow anchors are healed away: {}",
            plan["sourceMap"]
        );
    }

    /// The canvas save round-trips: what write_planned stores, read_planned
    /// returns.
    #[test]
    fn canvas_save_round_trips_the_planned_layer() {
        let (_dir, r, ref_str) = committed_project();
        let mut plan = scryer_core::read_model_at(&r).unwrap();
        plan.nodes[0].responsibilities[0].statement = "does the revised thing".into();
        plan.source_map.clear();
        super::write_planned(ref_str.clone(), serde_json::to_string(&plan).unwrap()).unwrap();

        let read_back = super::read_planned(ref_str).unwrap();
        assert!(read_back.contains("does the revised thing"));
    }

    /// Closing an empty open change from the canvas records it as an
    /// abandoned history entry — which the History tab then reads back.
    #[test]
    fn closing_an_empty_change_records_an_abandoned_history_entry() {
        let (_dir, r, ref_str) = committed_project();
        let mut plan = scryer_core::read_model_at(&r).unwrap();
        plan.source_map.clear();
        let stranded = scryer_core::changes::open_change(&mut plan, "never started", 100);
        scryer_core::write_planned_at(&r, &plan).unwrap();

        super::close_change(ref_str.clone(), stranded).unwrap();

        let raw = super::read_history(ref_str).unwrap();
        let events: serde_json::Value = serde_json::from_str(&raw).unwrap();
        let abandoned = events
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["driver"] == "abandoned")
            .expect("the close lands on the durable log");
        assert_eq!(abandoned["rows"][0]["text"], "never started");
    }

    /// A new project gets a blank model at `.scryer/model.scry`; a bogus path
    /// is refused.
    #[test]
    fn a_new_project_gets_a_blank_model() {
        let dir = tempfile::tempdir().unwrap();
        let ref_str =
            super::create_blank_model(dir.path().to_string_lossy().to_string()).unwrap();
        assert!(dir.path().join(".scryer/model.scry").exists());
        let r = ModelRef::parse(&ref_str).unwrap();
        assert!(scryer_core::read_model_at(&r).unwrap().nodes.is_empty());

        assert!(super::create_blank_model("/nonexistent/nowhere".into()).is_err());
    }

    /// Legacy detection: a model predating the current schema reports legacy;
    /// a current one (or no model at all) does not.
    #[test]
    fn legacy_models_are_reported_current_ones_are_not() {
        let (dir, r, _) = committed_project();
        let project = dir.path().to_string_lossy().to_string();
        assert!(!super::is_legacy_model(project.clone()), "current schema");

        std::fs::write(r.model_path(), r#"{ "version": "0.1", "nodes": [], "links": [] }"#)
            .unwrap();
        assert!(super::is_legacy_model(project));

        let empty = tempfile::tempdir().unwrap();
        assert!(!super::is_legacy_model(empty.path().to_string_lossy().to_string()));
    }

    /// Sign-off from the canvas snapshots the change; a later canvas save
    /// re-stamps it so the developer's own edit never reads as an amendment;
    /// the refusal ledger reads back through the command.
    #[test]
    fn canvas_sign_off_and_saves_keep_the_developer_as_the_author_of_intent() {
        let dir = tempfile::tempdir().unwrap();
        let r = ModelRef::ProjectLocal(dir.path().to_path_buf());
        let mut m = ScryModel::new();
        m.nodes.push(
            serde_json::from_value(serde_json::json!({ "id": "n1", "kind": "symbol", "name": "verify" }))
                .unwrap(),
        );
        scryer_core::write_model_at(&r, &m).unwrap();
        scryer_core::ensure_planned_at(&r).unwrap();
        let mut planned = scryer_core::read_planned_at(&r).unwrap();
        planned.nodes[0].responsibilities.push(
            serde_json::from_value(serde_json::json!({ "id": "r1", "statement": "Verifies tokens" })).unwrap(),
        );
        let cid = scryer_core::changes::open_change(&mut planned, "verify", 1);
        scryer_core::changes::tag(&mut planned, &["resp:r1".to_string()], &cid);
        scryer_core::write_planned_at(&r, &planned).unwrap();
        let ref_str = r.to_ref_string();

        assert_eq!(sign_off_change(ref_str.clone(), cid.clone()).unwrap(), 1);
        let planned = scryer_core::read_planned_at(&r).unwrap();
        assert!(planned.changes[0].signed_off.is_some());

        // The developer rewords on the canvas: the echo carries the old snapshot,
        // and the save re-stamps it to the new text.
        let mut echo = planned.clone();
        echo.nodes[0].responsibilities[0].statement = "Verifies tokens, the dev's way".into();
        write_planned(ref_str.clone(), serde_json::to_string(&echo).unwrap()).unwrap();
        let planned = scryer_core::read_planned_at(&r).unwrap();
        assert!(
            scryer_core::changes::classify_against_signoff(&planned, &planned.changes[0]).is_empty(),
            "a canvas edit is intent, never an amendment"
        );
        assert_eq!(
            planned.changes[0].signed_off.as_ref().unwrap().entries["resp:r1"].statement.as_deref(),
            Some("Verifies tokens, the dev's way")
        );

        assert!(read_fold_refusals(ref_str.clone()).unwrap().is_empty());
        scryer_core::refusals::update_refusals(
            &r,
            &[scryer_core::refusals::Refusal {
                resp_id: "r1".into(),
                host_id: "n1".into(),
                kind: "no-test".into(),
                reason: "no test attached".into(),
                run: vec![],
                at: 5,
            }],
            &[],
        )
        .unwrap();
        let refusals = read_fold_refusals(ref_str).unwrap();
        assert_eq!(refusals.len(), 1);
        assert_eq!(refusals[0].kind, "no-test");
    }
}
