//! What the observability surfaces read: the health report over the derived
//! tree (coverage, link evidence, style conformance, dark files and every
//! claim's status) and the per-claim verdicts and probe results the Tests
//! lens marks rows with. All of it is derived from the code and the last
//! ingested reports; nothing here is stored.

/// Everything the health surfaces read, in one deterministic pass. Runs the
/// extractor, so it also refreshes the `.build_edges.json` cache.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModelHealthReport {
    health: scryer_core::health::ModelHealth,
    /// Per-node completeness (anchored primitives over all of them), keyed
    /// by node id.
    completeness: std::collections::BTreeMap<String, scryer_core::health::Completeness>,
    derived: scryer_core::build_edges::DerivedGraph,
    /// Structural violations: the code held against each container's style
    /// from structure.toml, over the same resolved import graph.
    structural: scryer_core::style_health::StyleReport,
    /// The project's style table (built-ins plus `.scryer/styles/*.json`), so
    /// the diagram draws custom styles with their own layers.
    styles: Vec<scryer_core::style::StyleDef>,
    /// Every claim's state and probe mark, and the health counts per node.
    derived_status: scryer_core::derived_tree::DerivedStatus,
}

#[tauri::command]
pub(crate) async fn get_model_health(cwd: String) -> Result<ModelHealthReport, String> {
    // The extractor parses the whole repo (seconds on a big project) — keep it
    // off the IPC thread so the UI stays responsive while the report computes.
    tauri::async_runtime::spawn_blocking(move || model_health(std::path::Path::new(&cwd)))
        .await
        .map_err(|e| format!("health task failed: {e}"))?
}

fn model_health(project: &std::path::Path) -> Result<ModelHealthReport, String> {
    let model_ref = scryer_core::ModelRef::ProjectLocal(project.to_path_buf());
    let (tree, status) = scryer_extract::test_status::derived_status(&model_ref)?;
    let (ctx, _) = scryer_extract::extract_context_with_stats(project)?;
    let edges = ctx.build_edges();
    let _ = scryer_core::build_edges::write_build_edges(project, &edges);
    let all_files = scryer_extract::list_project_files(project);
    let derived = scryer_core::build_edges::derive_graph(&tree.committed, &edges);
    let styles = scryer_core::load_styles(project);
    // The same asks the derived tree makes: no layer for files outside a
    // container's source root (examples, benches, build scripts).
    let exempt: std::collections::HashSet<&str> = tree.layer_exempt.iter().map(String::as_str).collect();
    let structural = scryer_core::style_health::check_code(&tree.committed, &styles, &derived, &edges, Some(&all_files))
        .exempting(&exempt);
    let mut health = scryer_core::health::compute_health(&tree.committed, Some(&tree.planned), Some(&all_files));
    // Darkness is test reach: the files no test reaches by import.
    for (id, node) in health.nodes.iter_mut() {
        if let Some(b) = node.boundary.as_mut() {
            b.dark_files = status.nodes.get(id).map(|n| n.dark_files.clone()).unwrap_or_default();
            b.anchored_files = b.total_files.saturating_sub(b.dark_files.len() as u32);
        }
    }
    let completeness =
        scryer_core::health::resolve_completeness(&tree.planned, &all_files, &std::collections::HashSet::new());
    Ok(ModelHealthReport {
        health,
        completeness,
        derived,
        structural,
        styles: styles.iter().cloned().collect(),
        derived_status: status,
    })
}

/// Per-claim test verdicts, each re-verified against the working tree as it
/// stands. Empty until a test report has been ingested. The tree and node
/// pages color claim rows from this.
#[tauri::command]
pub(crate) async fn get_test_statuses(
    cwd: String,
) -> Result<Vec<scryer_extract::test_status::ClaimTestStatus>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let model_ref = scryer_core::ModelRef::ProjectLocal(std::path::PathBuf::from(&cwd));
        scryer_extract::test_status::derived_status(&model_ref).map(|(_, s)| s.tests)
    })
    .await
    .map_err(|e| format!("test-status task failed: {e}"))?
}

/// Per-claim probe results — deliberate breaks a claim's test was asked to
/// catch and how many it missed, from probe rounds and mutation reports.
/// Claims nobody has probed are absent rather than zeroed.
#[tauri::command]
pub(crate) async fn get_probe_statuses(
    cwd: String,
) -> Result<Vec<scryer_extract::test_status::ClaimProbeStatus>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let model_ref = scryer_core::ModelRef::ProjectLocal(std::path::PathBuf::from(&cwd));
        scryer_extract::test_status::derived_status(&model_ref).map(|(_, s)| s.probes)
    })
    .await
    .map_err(|e| format!("probe-status task failed: {e}"))?
}

#[cfg(test)]
mod tests {
    fn write(dir: &std::path::Path, rel: &str, text: &str) {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    /// The health report reads the derived tree: claims carry their derived
    /// status, and a file no test reaches is the dark file of its node.
    #[test]
    fn health_reports_the_derived_tree_with_its_status_and_dark_files() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        write(d, "Cargo.toml", "[package]\nname = \"core\"\nversion = \"0.1.0\"\n");
        write(d, "src/lib.rs", "pub mod draft;\npub mod store;\n");
        write(d, "src/draft.rs", "pub fn save() {}\n#[cfg(test)]\nmod tests {\n    /// Saves\n    #[test]\n    fn saves() {}\n}\n");
        write(d, "src/store.rs", "pub fn put() {}\n");
        let report = super::model_health(d).unwrap();
        assert_eq!(report.derived_status.totals.claims, 1);
        assert_eq!(report.derived_status.totals.stale, 1, "no report has covered it yet");
        let dark: Vec<&String> = report.derived_status.nodes.values().flat_map(|n| n.dark_files.iter()).collect();
        assert!(dark.iter().any(|f| f.as_str() == "src/store.rs"), "{dark:?}");
    }
}
