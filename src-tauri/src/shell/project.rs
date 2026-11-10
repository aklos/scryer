//! The open project: its file watcher, its derived tree, the model at the
//! base its changes are read against, and the developer's subagent settings.
//!
//! While a project is open the app records itself in `.scryer/view/app.json`
//! with whether the developer follows agents, so the agents' MCP servers offer
//! `show`; an agent's direction lands in `.scryer/view/direction.json` and goes
//! to the window as `view-direction`.

use std::sync::Mutex;

use notify::{recommended_watcher, EventKind, RecursiveMode, Watcher};
use tauri::Emitter;

use crate::shell::state::WatcherState;

/// The direction to send the window when `path` changed: only the direction
/// file in the view directory, and only once it parses.
pub(crate) fn direction_at(path: &std::path::Path) -> Option<scryer_core::app_view::Direction> {
    if path.file_name()? != "direction.json" || !path.parent()?.ends_with("view") {
        return None;
    }
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

/// Clear the open project's record that the app has it open.
pub(crate) fn forget_open_project(state: &Mutex<WatcherState>) {
    if let Some((dir, _)) = &state.lock().unwrap().project {
        if let Some(root) = dir.parent() {
            scryer_core::app_view::mark_closed(root);
        }
    }
}

/// One agent session as its log stands: what the app needs to offer
/// following it, without deriving every worktree's tree as the landing does.
#[tauri::command]
pub(crate) fn get_session_brief(cwd: String, session: String) -> scryer_core::session::SessionBrief {
    let r = scryer_core::ModelRef::ProjectLocal(std::path::PathBuf::from(cwd));
    scryer_core::session::session_brief(&r, &session, scryer_core::session::now_secs())
}

/// Watch `{project}/.scryer/` for the tree, session logs, verdicts and agents'
/// directions changing. Replaces any previous watcher.
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

    if let Some((prev, _)) = state.project.take() {
        if let Some(root) = prev.parent() {
            scryer_core::app_view::mark_closed(root);
        }
    }

    let scryer_core::ModelRef::ProjectLocal(ref project_path) = model_ref;
    let _ = scryer_core::agent_setup::ensure_scryer_dir(project_path);
    let handle = app.clone();
    let ref_string = ref_str.clone();
    // Passive test-report ingestion: the same watcher also covers the
    // project's report directories, and any XML written under one is ingested
    // after a short settle. Only files that CHANGE while watching count — the
    // event-driven design is what guarantees no pre-existing (older-code)
    // report is ever swept in.
    let report_dirs = crate::shell::test_reports::report_dirs(project_path);
    let debounce =
        crate::shell::test_reports::ReportDebounce::new(std::time::Duration::from_millis(800));
    let project_root = project_path.clone();
    let mut watcher =
        recommended_watcher(move |res: Result<notify::Event, notify::Error>| {
            let Ok(event) = res else { return };
            if !matches!(event.kind, EventKind::Create(_) | EventKind::Modify(_)) {
                return;
            }
            for path in &event.paths {
                // An agent pointed the app somewhere.
                if let Some(direction) = direction_at(path) {
                    let _ = handle.emit("view-direction", direction);
                    continue;
                }
                // Session logs are appended by the agent's hooks and MCP server;
                // the session view re-reads the one that changed.
                if path.parent().is_some_and(|p| p.ends_with("sessions"))
                    && path.extension().is_some_and(|e| e == "jsonl")
                {
                    if let Some(id) = path.file_stem().and_then(|s| s.to_str()) {
                        let _ = handle.emit("session-changed", id.to_string());
                    }
                    continue;
                }
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
                // The tree follows structure.toml and every fresh derivation.
                if path.file_name().is_some_and(|n| n == "structure.toml" || n == ".derived_tree.json") {
                    let _ = handle.emit("model-changed", ref_string.clone());
                }
            }
        })
        .map_err(|e| e.to_string())?;

    watcher
        .watch(&target_dir, RecursiveMode::NonRecursive)
        .map_err(|e| e.to_string())?;
    // Session logs live one level down; create the directory so a session
    // that starts after the project opens is still seen.
    let _ = scryer_core::session::ensure_sessions_dir(&model_ref);
    let _ = watcher.watch(&model_ref.sessions_dir(), RecursiveMode::NonRecursive);
    // Report directories are best-effort: a vanished one must not break the
    // model watch that everything else depends on.
    for dir in crate::shell::test_reports::report_dirs(project_path) {
        let _ = watcher.watch(&dir, RecursiveMode::Recursive);
    }
    // Agents' directions, and the record that offers them the tool.
    let view_dir = scryer_core::app_view::view_dir(project_path);
    let _ = std::fs::create_dir_all(&view_dir);
    let _ = watcher.watch(&view_dir, RecursiveMode::NonRecursive);
    let _ = scryer_core::app_view::mark_open(project_path);

    state.project = Some((target_dir, watcher));
    Ok(())
}

/// The derived tree, every claim todo tests included, as JSON.
#[tauri::command]
pub(crate) async fn read_planned(ref_str: String) -> Result<String, String> {
    let model_ref = scryer_core::ModelRef::parse(&ref_str)?;
    let project = model_ref.project_path().to_path_buf();
    tauri::async_runtime::spawn_blocking(move || {
        let tree = scryer_extract::derived_tree::derived_tree(&project)?;
        serde_json::to_string(&tree.planned).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Where the project's changes are read from: HEAD on the default branch,
/// the merge base off it. `None` outside a git repository.
#[tauri::command]
pub(crate) async fn read_change_base(ref_str: String) -> Result<Option<scryer_extract::base_model::ChangeBase>, String> {
    let model_ref = scryer_core::ModelRef::parse(&ref_str)?;
    let project = model_ref.project_path().to_path_buf();
    tauri::async_runtime::spawn_blocking(move || scryer_extract::base_model::change_base(&project).ok())
        .await
        .map_err(|e| e.to_string())
}

/// The model at `commit` as JSON, derived once per git tree.
#[tauri::command]
pub(crate) async fn read_base_model(ref_str: String, commit: String) -> Result<String, String> {
    let model_ref = scryer_core::ModelRef::parse(&ref_str)?;
    let project = model_ref.project_path().to_path_buf();
    tauri::async_runtime::spawn_blocking(move || {
        let model = scryer_extract::base_model::model_at(&project, &commit)?;
        serde_json::to_string(&model).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
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
    /// When the project is a git repository, reads the commit its changes start from and the model at it; outside one, reads no base.
    #[test]
    fn reads_the_change_base_and_the_model_at_it() {
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("SCRYER_BASES_DIR", dir.path().join("bases"));
        let d = dir.path().join("core");
        std::fs::create_dir_all(d.join("src")).unwrap();
        std::fs::write(d.join("Cargo.toml"), "[package]\nname = \"core\"\nversion = \"0.1.0\"\n").unwrap();
        std::fs::write(d.join("src/lib.rs"), "pub fn save() {}\n#[cfg(test)]\nmod tests {\n    /// Saves\n    #[test]\n    fn saves() {}\n}\n").unwrap();
        let ref_str = format!("project:{}", d.display());
        assert_eq!(tauri::async_runtime::block_on(super::read_change_base(ref_str.clone())).unwrap(), None);
        let git = |args: &[&str]| {
            let ok = std::process::Command::new("git")
                .args(["-c", "user.name=t", "-c", "user.email=t@t"])
                .args(args)
                .current_dir(&d)
                .status()
                .unwrap()
                .success();
            assert!(ok, "git {args:?}");
        };
        git(&["init", "-q", "-b", "main"]);
        git(&["add", "-A"]);
        git(&["commit", "-qm", "init"]);
        std::fs::write(d.join("src/lib.rs"), "pub fn save() {}\n#[cfg(test)]\nmod tests {\n    /// Saves the note\n    #[test]\n    fn saves() {}\n}\n").unwrap();
        let base = tauri::async_runtime::block_on(super::read_change_base(ref_str.clone())).unwrap().unwrap();
        assert!(base.dirty && base.against.is_none());
        let model = tauri::async_runtime::block_on(super::read_base_model(ref_str, base.commit)).unwrap();
        assert!(model.contains("\"Saves\"") && !model.contains("Saves the note"));
    }

    /// When the direction file changes, reads the direction to send to the window, and ignores the app's own record beside it.
    #[test]
    fn reads_a_direction_only_from_the_direction_file() {
        let dir = tempfile::tempdir().unwrap();
        let d = scryer_core::app_view::Direction { sender: "fix the save".into(), node: "n".into(), at: 3, ..Default::default() };
        scryer_core::app_view::write_direction(dir.path(), &d).unwrap();
        scryer_core::app_view::mark_open(dir.path()).unwrap();
        let view = scryer_core::app_view::view_dir(dir.path());
        assert_eq!(super::direction_at(&view.join("direction.json")), Some(d));
        assert_eq!(super::direction_at(&view.join("app.json")), None);
        std::fs::write(view.join("direction.json"), "{ torn").unwrap();
        assert_eq!(super::direction_at(&view.join("direction.json")), None, "a torn write is no direction");
    }

    /// When a different project is opened, replaces the previous project's watcher, and when the same project is opened again, keeps the running one.
    #[test]
    #[ignore]
    fn opening_another_project_replaces_the_watcher() {
        todo!()
    }
}
