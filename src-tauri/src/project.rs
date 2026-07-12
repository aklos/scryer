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
            match hooks::start(project_path, on_touch) {
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
    let mut watcher =
        recommended_watcher(move |res: Result<notify::Event, notify::Error>| {
            let Ok(event) = res else { return };
            if !matches!(event.kind, EventKind::Create(_) | EventKind::Modify(_)) {
                return;
            }
            for path in &event.paths {
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
    scryer_core::write_planned_raw_at(&model_ref, &data)
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
