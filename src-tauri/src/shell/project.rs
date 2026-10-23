//! The open project: its file watcher, and the derived tree's two layers as
//! the app reads them — committed (implemented claims) and planned (plus the
//! todo stubs) — and the developer's subagent settings.

use std::sync::Mutex;

use notify::{recommended_watcher, EventKind, RecursiveMode, Watcher};
use tauri::Emitter;

use crate::shell::state::WatcherState;

/// Watch `{project}/.scryer/` for the tree, session logs and verdicts
/// changing. Replaces any previous watcher.
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

    let scryer_core::ModelRef::ProjectLocal(ref project_path) = model_ref;
    let _ = std::fs::create_dir_all(&target_dir);
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

    state.project = Some((target_dir, watcher));
    Ok(())
}

/// One layer of the derived tree as JSON.
async fn layer(ref_str: String, planned: bool) -> Result<String, String> {
    let model_ref = scryer_core::ModelRef::parse(&ref_str)?;
    let project = model_ref.project_path().to_path_buf();
    tauri::async_runtime::spawn_blocking(move || {
        let tree = scryer_extract::derived_tree::derived_tree(&project)?;
        serde_json::to_string(if planned { &tree.planned } else { &tree.committed }).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The derived tree's committed layer: every implemented claim.
#[tauri::command]
pub(crate) async fn read_model(ref_str: String) -> Result<String, String> {
    layer(ref_str, false).await
}

/// The derived tree's planned layer: the committed one plus the todo stubs,
/// so the difference between them is the work still planned.
#[tauri::command]
pub(crate) async fn read_planned(ref_str: String) -> Result<String, String> {
    layer(ref_str, true).await
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
    /// The two layers read the derived tree: a todo stub is in the planned
    /// layer only.
    #[test]
    fn the_layers_differ_by_the_todo_stubs() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        std::fs::create_dir_all(d.join("src")).unwrap();
        std::fs::write(d.join("Cargo.toml"), "[package]\nname = \"core\"\nversion = \"0.1.0\"\n").unwrap();
        std::fs::write(
            d.join("src/draft.rs"),
            "pub fn save() {}\n#[cfg(test)]\nmod tests {\n    /// Saves\n    #[test]\n    fn saves() {}\n    /// Merges\n    #[test]\n    fn merges() { todo!() }\n}\n",
        )
        .unwrap();
        let ref_str = format!("project:{}", d.display());
        let committed = tauri::async_runtime::block_on(super::read_model(ref_str.clone())).unwrap();
        let planned = tauri::async_runtime::block_on(super::read_planned(ref_str)).unwrap();
        assert!(committed.contains("Saves") && !committed.contains("Merges"));
        assert!(planned.contains("Saves") && planned.contains("Merges"));
    }

    /// When a different project is opened, replaces the previous project's watcher, and when the same project is opened again, keeps the running one.
    #[test]
    #[ignore]
    fn opening_another_project_replaces_the_watcher() {
        todo!()
    }
}
