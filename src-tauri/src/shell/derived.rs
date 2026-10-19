//! The derived tree (preview): the switch that serves every read from the
//! tree derived off packaging and tests instead of the authored model, and
//! the commands that only make sense there — status, the landing view, and
//! writing claims back into test files.

/// Turn the derived tree on or off for this app process. Every read the app
/// makes (model, health, tests lens) follows it, and so does report ingest.
#[tauri::command]
pub(crate) fn set_derived_tree(on: bool) {
    if on {
        std::env::set_var(scryer_core::derived_tree::DERIVED_TREE_ENV, "1");
    } else {
        std::env::remove_var(scryer_core::derived_tree::DERIVED_TREE_ENV);
    }
}

pub(crate) fn enabled() -> bool {
    scryer_core::derived_tree::derived_tree_enabled()
}

fn model_ref(cwd: &str) -> scryer_core::ModelRef {
    scryer_core::ModelRef::ProjectLocal(std::path::PathBuf::from(cwd))
}

/// One derived layer as the JSON `read_model` / `read_planned` return.
pub(crate) fn layer_json(project: &std::path::Path, planned: bool) -> Result<String, String> {
    let tree = scryer_extract::derived_tree::derived_tree(project)?;
    serde_json::to_string(if planned { &tree.planned } else { &tree.committed }).map_err(|e| e.to_string())
}

/// Every claim's status, per-node health counts and the Tests lens lists.
#[tauri::command]
pub(crate) async fn get_derived_status(cwd: String) -> Result<scryer_core::derived_tree::DerivedStatus, String> {
    tauri::async_runtime::spawn_blocking(move || {
        scryer_extract::test_status::derived_status(&model_ref(&cwd)).map(|(_, status)| status)
    })
    .await
    .map_err(|e| format!("derived-status task failed: {e}"))?
}

/// What each agent session changed since `since` (unix seconds).
#[tauri::command]
pub(crate) async fn get_landing(cwd: String, since: u64) -> Result<scryer_core::derived_tree::Landing, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let r = model_ref(&cwd);
        let (tree, status) = scryer_extract::test_status::derived_status(&r)?;
        Ok(scryer_core::derived_tree::landing_at(&r, &tree, &status, since))
    })
    .await
    .map_err(|e| format!("landing task failed: {e}"))?
}

/// Reword a claim: edits its test's description in the file. Returns the file.
#[tauri::command]
pub(crate) fn edit_claim_text(cwd: String, resp_id: String, text: String) -> Result<String, String> {
    scryer_extract::claim_files::edit_claim_text(std::path::Path::new(&cwd), &resp_id, &text)
}

/// Add a claim to a node: writes a todo stub into its test file. Returns the file.
#[tauri::command]
pub(crate) fn add_claim_stub(cwd: String, node_id: String, text: String) -> Result<String, String> {
    scryer_extract::claim_files::write_claim_stub(std::path::Path::new(&cwd), &node_id, &text)
}

/// Rename a container: sets its display name in `.scryer/structure.toml`.
#[tauri::command]
pub(crate) fn rename_container(cwd: String, node_id: String, name: String) -> Result<(), String> {
    scryer_extract::claim_files::rename_container(std::path::Path::new(&cwd), &node_id, &name)
}

#[cfg(test)]
mod tests {
    fn write(dir: &std::path::Path, rel: &str, text: &str) {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    /// The commands round-trip through the files: a stub added to a
    /// component shows as planned in the status, a reworded claim reads back
    /// reworded, and the landing view lists a session that touched it.
    #[test]
    fn claim_commands_round_trip_through_the_files() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        write(d, "Cargo.toml", "[package]\nname = \"core\"\nversion = \"0.1.0\"\n");
        write(d, "src/lib.rs", "pub mod draft;\n");
        write(d, "src/draft.rs", "pub fn save() {}\n#[cfg(test)]\nmod tests {\n    /// Saves\n    #[test]\n    fn saves() {}\n}\n");
        let cwd = d.to_string_lossy().to_string();
        let tree = scryer_extract::derived_tree::derived_tree(d).unwrap();
        let draft = tree.planned.nodes.iter().find(|n| n.name == "draft").unwrap().id.clone();

        assert_eq!(super::add_claim_stub(cwd.clone(), draft, "When offline, queues it".into()).unwrap(), "src/draft.rs");
        let status = tauri::async_runtime::block_on(super::get_derived_status(cwd.clone())).unwrap();
        let tree = scryer_extract::derived_tree::derived_tree(d).unwrap();
        let stub = tree.claims.iter().find(|c| c.description == "When offline, queues it").unwrap();
        let state = status.claims.iter().find(|c| c.resp_id == stub.id).unwrap().state;
        assert_eq!(state, scryer_core::derived_tree::ClaimState::Planned);

        let saves = tree.claims.iter().find(|c| c.identifier == "saves").unwrap().id.clone();
        super::edit_claim_text(cwd.clone(), saves, "When asked, saves".into()).unwrap();
        let source = std::fs::read_to_string(d.join("src/draft.rs")).unwrap();
        assert!(source.contains("/// When asked, saves\n"));

        let r = scryer_core::ModelRef::ProjectLocal(d.to_path_buf());
        scryer_core::session::record_touch(&r, "sess-1", "src/draft.rs").unwrap();
        let landing = tauri::async_runtime::block_on(super::get_landing(cwd, 0)).unwrap();
        assert_eq!(landing.sessions.len(), 1);
        assert!(landing.sessions[0].still_planned.iter().chain(&landing.sessions[0].newly_planned).any(|c| c.statement == "When offline, queues it"));
    }
}
