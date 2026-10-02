use std::sync::Mutex;

use notify::{recommended_watcher, EventKind, RecursiveMode, Watcher};
use tauri::Emitter;

use crate::shell::state::WatcherState;

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

#[tauri::command]
pub(crate) async fn read_model(ref_str: String) -> Result<String, String> {
    let model_ref = scryer_core::ModelRef::parse(&ref_str)?;
    scryer_core::read_model_raw_at(&model_ref)
}

/// Read the planned (draft) layer — the working model the canvas edits. Returns
/// the committed model's SEEDED bytes when no plan has diverged yet (planned ==
/// model, anchors cleared), so a fresh project opens with an empty plan.
#[tauri::command]
pub(crate) async fn read_planned(ref_str: String) -> Result<String, String> {
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
    scryer_core::write_hand_edited_plan_at(&model_ref, &data)
}

/// One agent session's log, newest first, for the session list.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SessionSummary {
    session: String,
    updated_at: u64,
    /// The session's first prompt, for a title.
    first_prompt: Option<String>,
}

/// Every agent session with a log in this project, most recent first.
#[tauri::command]
pub(crate) fn list_sessions(ref_str: String) -> Result<Vec<SessionSummary>, String> {
    let model_ref = scryer_core::ModelRef::parse(&ref_str)?;
    Ok(scryer_core::session::list_sessions(&model_ref)
        .into_iter()
        .map(|(session, updated_at)| SessionSummary {
            first_prompt: scryer_core::session::first_prompt(&model_ref, &session),
            session,
            updated_at,
        })
        .collect())
}

/// One session as the user reviews it: prompts, asks and where each stands,
/// files touched with the claims they reached, and edits no ask accounts for.
#[tauri::command]
pub(crate) fn read_session(
    ref_str: String,
    session: String,
) -> Result<scryer_core::session::SessionView, String> {
    let model_ref = scryer_core::ModelRef::parse(&ref_str)?;
    Ok(scryer_core::session::session_view(&model_ref, &session, |claims| {
        scryer_extract::test_status::claim_evidence(&model_ref, claims)
            .map(|m| m.into_iter().map(|(k, e)| (k, e.verified())).collect())
            .unwrap_or_default()
    }))
}

/// Read the durable committed-model history log (`.scryer/history.jsonl`),
/// returned as a JSON array of events, oldest first. Empty when the project has
/// no history yet. The frontend re-reads this whenever the model changes (every
/// event-producing agent operation also writes a `.scry` file the watcher sees).
#[tauri::command]
pub(crate) async fn read_history(ref_str: String) -> Result<String, String> {
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

        let raw = tauri::async_runtime::block_on(super::read_planned(ref_str)).unwrap();
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

        let read_back = tauri::async_runtime::block_on(super::read_planned(ref_str)).unwrap();
        assert!(read_back.contains("does the revised thing"));
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

}
