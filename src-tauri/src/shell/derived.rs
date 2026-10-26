//! The derived tree's own commands: every claim's status, the landing view,
//! and writing claims back into the code.

fn model_ref(cwd: &str) -> scryer_core::ModelRef {
    scryer_core::ModelRef::ProjectLocal(std::path::PathBuf::from(cwd))
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

/// What each agent session changed since `since` (unix seconds), across the
/// project's live git worktrees.
#[tauri::command]
pub(crate) async fn get_landing(cwd: String, since: u64) -> Result<scryer_core::derived_tree::Landing, String> {
    tauri::async_runtime::spawn_blocking(move || {
        scryer_extract::landing::landing_across_worktrees(std::path::Path::new(&cwd), since)
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

/// Every comment thread in the project, open ones first, across its live
/// worktrees.
#[tauri::command]
pub(crate) fn list_comments(cwd: String) -> Vec<scryer_core::comments::Thread> {
    scryer_core::comments::threads(std::path::Path::new(&cwd))
}

/// Open a thread on a node, written by the person at the app.
#[tauri::command]
pub(crate) fn open_comment(
    cwd: String,
    node_id: String,
    anchor: String,
    text: String,
) -> Result<scryer_core::comments::Thread, String> {
    use scryer_core::comments::{open_thread, Author};
    open_thread(std::path::Path::new(&cwd), &node_id, &anchor, Author::Human, &text)
}

/// Reply to a thread from the app; `status` closes ("addressed") or reopens
/// ("open") it.
#[tauri::command]
pub(crate) fn reply_comment(
    cwd: String,
    thread: String,
    text: String,
    status: Option<scryer_core::comments::ThreadStatus>,
) -> Result<scryer_core::comments::Thread, String> {
    use scryer_core::comments::{reply, Author};
    reply(std::path::Path::new(&cwd), &thread, Author::Human, &text, status)
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
