use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use notify::{recommended_watcher, EventKind, RecursiveMode, Watcher};
use tauri::{Emitter, Manager, path::BaseDirectory};

/// Managed state wrapping the AI settings.
struct SettingsState(Arc<Mutex<scryer_core::AiSettings>>);

/// Managed state for the ACP runtime (agent orchestration).
struct AcpState(Mutex<Option<scryer_acp::AcpRuntime>>);

/// Pre-sync model snapshot for diffing after agent completes.
struct SyncSnapshot(Mutex<Option<scryer_core::C4ModelData>>);

#[tauri::command]
fn list_models() -> Result<Vec<String>, String> {
    scryer_core::list_models()
}

#[tauri::command]
fn read_model(name: String) -> Result<String, String> {
    let raw = scryer_core::read_model_raw(&name)?;
    // Migrate old kind values ("function", "unit", "member") → "operation"
    // and ensure operation nodes have type "operation" (was "c4")
    let mut val: serde_json::Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    let mut migrated = false;
    if let Some(nodes) = val.get_mut("nodes").and_then(|n| n.as_array_mut()) {
        for node in nodes {
            if let Some(kind_val) = node.pointer_mut("/data/kind") {
                if let Some(kind_str) = kind_val.as_str() {
                    if kind_str == "function" || kind_str == "unit" || kind_str == "member" {
                        *kind_val = serde_json::Value::String("operation".to_string());
                        migrated = true;
                    }
                }
            }
            // Migrate node type for operation nodes
            let is_op = node.pointer("/data/kind").and_then(|k| k.as_str()) == Some("operation");
            if is_op {
                if let Some(type_val) = node.get_mut("type") {
                    if type_val.as_str() != Some("operation") {
                        *type_val = serde_json::Value::String("operation".to_string());
                        migrated = true;
                    }
                }
            }
        }
    }
    if migrated {
        let updated = serde_json::to_string_pretty(&val).map_err(|e| e.to_string())?;
        scryer_core::write_model_raw(&name, &updated)?;
        Ok(updated)
    } else {
        Ok(raw)
    }
}

#[tauri::command]
fn write_model(name: String, data: String) -> Result<(), String> {
    scryer_core::write_model_raw(&name, &data)
}

#[tauri::command]
fn delete_model(name: String) -> Result<(), String> {
    scryer_core::delete_model(&name)
}

#[tauri::command]
fn list_templates(app: tauri::AppHandle) -> Result<Vec<String>, String> {
    let dir = app.path().resolve("templates", BaseDirectory::Resource)
        .map_err(|e| e.to_string())?;
    if !dir.exists() {
        return Ok(vec![]);
    }
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .map_err(|e| e.to_string())?
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let name = entry.file_name().to_string_lossy().to_string();
            name.strip_suffix(".scry").map(|n| n.to_string())
        })
        .collect();
    names.sort();
    Ok(names)
}

#[tauri::command]
fn load_template(app: tauri::AppHandle, name: String) -> Result<String, String> {
    let path = app.path().resolve(format!("templates/{}.scry", name), BaseDirectory::Resource)
        .map_err(|e| e.to_string())?;
    std::fs::read_to_string(&path).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_ai_settings(state: tauri::State<'_, SettingsState>) -> Result<serde_json::Value, String> {
    let settings = state.0.lock().unwrap().clone();
    let configured = scryer_core::ai_configured(&settings);
    // Mask API key — only send whether it's set
    Ok(serde_json::json!({
        "provider": settings.provider,
        "model": settings.model,
        "hasKey": !settings.api_key.is_empty(),
        "configured": configured,
    }))
}

#[tauri::command]
fn save_ai_settings(
    provider: String,
    api_key: String,
    model: String,
    state: tauri::State<'_, SettingsState>,
) -> Result<(), String> {
    let mut settings = state.0.lock().unwrap();
    settings.provider = provider;
    settings.model = model;
    // Empty key means "keep existing"
    if !api_key.is_empty() {
        settings.api_key = api_key;
    }
    scryer_core::write_settings(&settings)
}

#[tauri::command]
async fn fetch_models(provider: String, api_key: Option<String>, state: tauri::State<'_, SettingsState>) -> Result<Vec<String>, String> {
    let key = match api_key {
        Some(k) if !k.is_empty() => k,
        _ => state.0.lock().unwrap().api_key.clone(),
    };
    scryer_suggest::models::fetch_models(&provider, &key).await
}

#[tauri::command]
async fn get_hints(data: String, state: tauri::State<'_, SettingsState>) -> Result<String, String> {
    let settings = state.0.lock().unwrap().clone();
    if !scryer_core::ai_configured(&settings) {
        return Ok("[]".to_string());
    }

    let model: scryer_core::C4ModelData =
        serde_json::from_str(&data).map_err(|e| e.to_string())?;

    let hints = scryer_suggest::get_hints(&model, &settings).await;
    serde_json::to_string(&hints).map_err(|e| e.to_string())
}

#[tauri::command]
fn open_in_editor(file: String, line: Option<u32>, project_path: Option<String>) -> Result<(), String> {
    // Resolve absolute path
    let path = {
        let p = PathBuf::from(&file);
        if p.is_absolute() {
            p
        } else if let Some(base) = project_path {
            PathBuf::from(base).join(p)
        } else {
            std::env::current_dir()
                .map_err(|e| e.to_string())?
                .join(p)
        }
    };

    if !path.exists() {
        return Err(format!("File not found: {}", path.display()));
    }

    let path_str = path.to_string_lossy();

    // Resolve editor: $VISUAL → $EDITOR → auto-detect → fallback
    // Skip TUI editors — we're a GUI app, can't spawn them
    let is_tui = |name: &str| {
        let base = PathBuf::from(name)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| name.to_string());
        matches!(base.as_str(), "vim" | "nvim" | "vi" | "nano" | "emacs" | "helix" | "hx" | "ed" | "micro")
    };
    let editor = std::env::var("VISUAL")
        .ok()
        .filter(|v| !is_tui(v))
        .or_else(|| std::env::var("EDITOR").ok().filter(|v| !is_tui(v)))
        .or_else(|| {
            ["code", "cursor", "zed", "zeditor", "subl"]
                .iter()
                .find(|name| which::which(name).is_ok())
                .map(|s| s.to_string())
        });

    let editor = match editor {
        Some(e) => e,
        None => {
            // Fallback: open on macOS, start on Windows, xdg-open on Linux
            if cfg!(target_os = "windows") {
                std::process::Command::new("cmd")
                    .args(["/C", "start", "", &*path_str])
                    .stdin(std::process::Stdio::null())
                    .spawn()
                    .map_err(|e| format!("Failed to open file: {e}"))?;
            } else {
                let fallback = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
                std::process::Command::new(fallback)
                    .arg(&*path_str)
                    .stdin(std::process::Stdio::null())
                    .spawn()
                    .map_err(|e| format!("Failed to open file: {e}"))?;
            }
            return Ok(());
        }
    };

    // Extract the binary name for line-number format lookup
    let editor_name = PathBuf::from(&editor)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| editor.clone());

    let mut args: Vec<String> = Vec::new();

    match editor_name.as_str() {
        "code" | "cursor" => {
            args.push("--reuse-window".to_string());
            if let Some(l) = line {
                args.push("--goto".to_string());
                args.push(format!("{path_str}:{l}"));
            } else {
                args.push(path_str.to_string());
            }
        }
        "zed" | "zeditor" => {
            // -a adds the file to the currently focused workspace
            args.push("-a".to_string());
            if let Some(l) = line {
                args.push(format!("{path_str}:{l}"));
            } else {
                args.push(path_str.to_string());
            }
        }
        "subl" => {
            if let Some(l) = line {
                args.push(format!("{path_str}:{l}"));
            } else {
                args.push(path_str.to_string());
            }
        }
        _ => {
            args.push(path_str.to_string());
        }
    }

    std::process::Command::new(&editor)
        .args(&args)
        .stdin(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("Failed to launch {editor}: {e}"))?;

    Ok(())
}

#[tauri::command]
/// Check if a project has .mcp.json with a scryer entry.
fn check_mcp_json(project_path: &str) -> bool {
    let path = PathBuf::from(project_path).join(".mcp.json");
    if let Ok(contents) = std::fs::read_to_string(&path) {
        if let Ok(root) = serde_json::from_str::<serde_json::Value>(&contents) {
            return root.pointer("/mcpServers/scryer").is_some();
        }
    }
    false
}

const SCRYER_READ_TOOLS: &[&str] = &[
    "mcp__scryer__list_models",
    "mcp__scryer__get_model",
    "mcp__scryer__get_node",
    "mcp__scryer__get_rules",
    "mcp__scryer__get_changes",
    "mcp__scryer__get_structure",
];

/// Check if Claude Code has auto-approved scryer read tools in project settings.
fn check_claude_read_approved(project_path: &str) -> bool {
    // Check both settings.local.json and settings.json
    for filename in &["settings.local.json", "settings.json"] {
        let path = PathBuf::from(project_path).join(".claude").join(filename);
        if let Ok(contents) = std::fs::read_to_string(&path) {
            if let Ok(root) = serde_json::from_str::<serde_json::Value>(&contents) {
                if let Some(allow) = root.pointer("/permissions/allow").and_then(|v| v.as_array()) {
                    let allowed: HashSet<&str> = allow.iter().filter_map(|v| v.as_str()).collect();
                    if SCRYER_READ_TOOLS.iter().all(|t| allowed.contains(t)) {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// Check if a project has .codex/config.toml with a scryer MCP entry.
fn check_codex_toml(project_path: &str) -> bool {
    let path = PathBuf::from(project_path).join(".codex").join("config.toml");
    if let Ok(contents) = std::fs::read_to_string(&path) {
        if let Ok(doc) = contents.parse::<toml_edit::DocumentMut>() {
            return doc.get("mcp_servers")
                .and_then(|t| t.as_table())
                .map(|t| t.contains_key("scryer"))
                .unwrap_or(false);
        }
    }
    false
}

#[tauri::command]
fn detect_ai_tools(project_path: Option<String>) -> serde_json::Value {
    let has_claude = which::which("claude").is_ok();
    let has_codex = which::which("codex").is_ok();

    let claude_mcp = project_path.as_deref().map(check_mcp_json).unwrap_or(false);
    let codex_mcp = project_path.as_deref().map(check_codex_toml).unwrap_or(false);
    let claude_read_approved = project_path.as_deref().map(check_claude_read_approved).unwrap_or(false);

    serde_json::json!({
        "claude": has_claude,
        "codex": has_codex,
        "claudeMcpEnabled": claude_mcp,
        "codexMcpEnabled": codex_mcp,
        "claudeReadApproved": claude_read_approved,
    })
}

/// Find the scryer-mcp binary path by checking common locations.
fn find_scryer_mcp() -> Option<String> {
    // Check next to scryer (same install dir)
    if let Ok(exe) = std::env::current_exe() {
        let sibling = exe.parent().map(|p| p.join("scryer-mcp"));
        if let Some(s) = sibling {
            if s.exists() {
                return Some(s.to_string_lossy().to_string());
            }
        }
    }
    // Check PATH
    which::which("scryer-mcp")
        .ok()
        .map(|p| p.to_string_lossy().to_string())
}

#[tauri::command]
fn setup_mcp_integration(
    action: String,
    project_path: String,
) -> Result<String, String> {
    match action.as_str() {
        "mcp" => {
            let binary_path = find_scryer_mcp()
                .ok_or("scryer-mcp binary not found")?;

            let mcp_path = PathBuf::from(&project_path).join(".mcp.json");
            let mut mcp_root: serde_json::Value = if mcp_path.exists() {
                let contents = std::fs::read_to_string(&mcp_path).map_err(|e| e.to_string())?;
                serde_json::from_str(&contents).unwrap_or_else(|_| serde_json::json!({}))
            } else {
                serde_json::json!({})
            };

            if !mcp_root.get("mcpServers").is_some_and(|v| v.is_object()) {
                mcp_root["mcpServers"] = serde_json::json!({});
            }
            mcp_root["mcpServers"]["scryer"] = serde_json::json!({
                "type": "stdio",
                "command": binary_path,
                "args": [],
            });

            std::fs::write(&mcp_path, serde_json::to_string_pretty(&mcp_root).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;

            return Ok(mcp_path.to_string_lossy().to_string());
        }
        "mcp_codex" => {
            let binary_path = find_scryer_mcp()
                .ok_or("scryer-mcp binary not found")?;

            let codex_dir = PathBuf::from(&project_path).join(".codex");
            let config_path = codex_dir.join("config.toml");

            let mut doc: toml_edit::DocumentMut = if config_path.exists() {
                std::fs::read_to_string(&config_path)
                    .map_err(|e| e.to_string())?
                    .parse()
                    .unwrap_or_default()
            } else {
                toml_edit::DocumentMut::new()
            };

            if !doc.contains_table("mcp_servers") {
                doc["mcp_servers"] = toml_edit::Item::Table(toml_edit::Table::new());
            }
            let mut server = toml_edit::Table::new();
            server.insert("command", toml_edit::value(&binary_path));
            server.insert("args", toml_edit::value(toml_edit::Array::new()));
            doc["mcp_servers"]["scryer"] = toml_edit::Item::Table(server);

            std::fs::create_dir_all(&codex_dir).map_err(|e| e.to_string())?;
            std::fs::write(&config_path, doc.to_string()).map_err(|e| e.to_string())?;

            return Ok(config_path.to_string_lossy().to_string());
        }
        "claude_read_approve" => {
            let claude_dir = PathBuf::from(&project_path).join(".claude");
            let settings_path = claude_dir.join("settings.local.json");

            let mut root: serde_json::Value = if settings_path.exists() {
                let contents = std::fs::read_to_string(&settings_path).map_err(|e| e.to_string())?;
                serde_json::from_str(&contents).unwrap_or_else(|_| serde_json::json!({}))
            } else {
                serde_json::json!({})
            };

            if !root.pointer("/permissions/allow").is_some_and(|v| v.is_array()) {
                root["permissions"] = serde_json::json!({ "allow": [] });
            }

            let allow = root.pointer_mut("/permissions/allow").unwrap().as_array_mut().unwrap();
            let existing: HashSet<String> = allow.iter().filter_map(|v| v.as_str().map(String::from)).collect();
            for tool in SCRYER_READ_TOOLS {
                if !existing.contains(*tool) {
                    allow.push(serde_json::json!(tool));
                }
            }

            std::fs::create_dir_all(&claude_dir).map_err(|e| e.to_string())?;
            std::fs::write(&settings_path, serde_json::to_string_pretty(&root).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;

            return Ok(settings_path.to_string_lossy().to_string());
        }
        _ => Err(format!("Unknown action: {}", action)),
    }
}

fn sync_marker_path(model_name: &str) -> PathBuf {
    scryer_core::models_dir().join(format!(".sync-{}", model_name))
}

/// Get the drift baseline for a model. Reads a stored timestamp from the sync
/// marker file. If no marker exists, initializes it from the .scry file's
/// current mtime. This avoids using the .scry mtime directly, which changes
/// on every auto-save and would falsely clear detected drift.
fn drift_baseline(model_name: &str) -> Result<std::time::SystemTime, String> {
    let marker = sync_marker_path(model_name);
    if let Ok(contents) = std::fs::read_to_string(&marker) {
        if let Ok(nanos) = contents.trim().parse::<u128>() {
            let duration = std::time::Duration::from_nanos(nanos as u64);
            return Ok(std::time::UNIX_EPOCH + duration);
        }
        // Legacy empty marker — fall back to its mtime
        if let Ok(meta) = std::fs::metadata(&marker) {
            return meta.modified().map_err(|e| e.to_string());
        }
    }
    // No sync marker yet — initialize from model file mtime
    let scry_path = scryer_core::models_dir().join(format!("{}.scry", model_name));
    let model_mtime = std::fs::metadata(&scry_path)
        .and_then(|m| m.modified())
        .map_err(|e| e.to_string())?;
    write_sync_marker(&marker, model_mtime)?;
    Ok(model_mtime)
}

fn write_sync_marker(path: &std::path::Path, time: std::time::SystemTime) -> Result<(), String> {
    let nanos = time.duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    std::fs::write(path, nanos.to_string()).map_err(|e| e.to_string())
}

#[tauri::command]
fn check_drift(model_name: String) -> Result<serde_json::Value, String> {
    let model = scryer_core::read_model(&model_name)?;
    let project_path = model.project_path.as_deref()
        .ok_or("Model has no project path set")?;

    let baseline = drift_baseline(&model_name)?;

    let report = scryer_core::drift::check_drift(&model, baseline, std::path::Path::new(project_path));

    Ok(serde_json::json!({
        "nodes": report.nodes.iter().map(|d| {
            serde_json::json!({
                "nodeId": d.node_id,
                "nodeName": d.node_name,
                "patterns": d.patterns,
            })
        }).collect::<Vec<_>>(),
        "structureChanged": report.structure_changed,
    }))
}

/// Record the current time as the drift baseline (drift was addressed).
#[tauri::command]
fn mark_synced(model_name: String) -> Result<(), String> {
    let path = sync_marker_path(&model_name);
    write_sync_marker(&path, std::time::SystemTime::now())
}

#[tauri::command]
fn get_active_agent() -> Result<serde_json::Value, String> {
    let client = scryer_acp::active_client()
        .ok_or("No agent has connected via MCP yet")?;
    let launch = scryer_acp::resolve_agent_binary(&client.name);
    Ok(serde_json::json!({
        "name": client.name,
        "version": client.version,
        "available": launch.is_some(),
        "launch": launch,
    }))
}

#[tauri::command]
async fn start_agent_session(
    cwd: String,
    model_name: String,
    app: tauri::AppHandle,
    state: tauri::State<'_, AcpState>,
    snapshot_state: tauri::State<'_, SyncSnapshot>,
) -> Result<String, String> {
    let mcp_binary = find_scryer_mcp()
        .ok_or("scryer-mcp binary not found — cannot provide MCP server to agent")?;

    // Compute drifted nodes to pass to the agent
    let model = scryer_core::read_model(&model_name)?;

    // Snapshot the model before sync so we can diff after completion
    *snapshot_state.0.lock().unwrap() = Some(model.clone());
    let project_path = model.project_path.as_deref()
        .ok_or("Model has no project path set")?;
    let baseline = drift_baseline(&model_name)?;
    let report = scryer_core::drift::check_drift(&model, baseline, std::path::Path::new(project_path));
    let drifted = report.nodes;

    // Resolve agent from the last MCP client that connected
    let client = scryer_acp::active_client()
        .ok_or("No agent has connected via MCP yet — open scryer in an AI tool first")?;
    let launch = scryer_acp::resolve_agent_binary(&client.name)
        .ok_or_else(|| format!("Agent '{}' not found on PATH", client.name))?;

    // Ensure runtime exists and clone it out of the mutex
    let runtime = {
        let mut rt = state.0.lock().unwrap();
        if rt.is_none() {
            *rt = Some(scryer_acp::AcpRuntime::new());
        }
        rt.clone().unwrap()
    };

    let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel();

    // Forward agent events to the frontend
    let handle = app.clone();
    tokio::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            let _ = handle.emit("agent-event", &event);
        }
    });

    let (agent_binary, mode) = match launch {
        scryer_acp::AgentLaunch::Cli { binary, kind } => {
            (binary, scryer_acp::runtime::LaunchMode::Cli { kind })
        }
        scryer_acp::AgentLaunch::Acp { binary } => {
            (binary, scryer_acp::runtime::LaunchMode::Acp)
        }
    };

    runtime
        .start_session(agent_binary, mode, cwd, model_name, mcp_binary, drifted, report.structure_changed, event_tx)
        .await
}

#[tauri::command]
async fn cancel_agent_session(
    model_name: String,
    state: tauri::State<'_, AcpState>,
    snapshot_state: tauri::State<'_, SyncSnapshot>,
) -> Result<(), String> {
    let runtime = {
        let rt = state.0.lock().unwrap();
        rt.clone().ok_or("ACP runtime not initialized")?
    };
    runtime.cancel().await?;

    // Restore the model to its pre-sync state
    let snapshot = snapshot_state.0.lock().unwrap().take();
    if let Some(data) = snapshot {
        let json = serde_json::to_string(&data).map_err(|e| e.to_string())?;
        scryer_core::write_model_raw(&model_name, &json)?;
    }
    Ok(())
}

/// Diff the pre-sync snapshot against the current model on disk.
/// Returns a summary like "Updated: API Server, Auth Service. Added: Logger."
/// or "No changes" if the model is identical.
#[tauri::command]
fn sync_diff(
    model_name: String,
    snapshot_state: tauri::State<'_, SyncSnapshot>,
) -> Result<String, String> {
    let baseline = snapshot_state.0.lock().unwrap().take()
        .ok_or("No sync snapshot available")?;
    let current = scryer_core::read_model(&model_name)?;

    use std::collections::HashMap;
    let base_nodes: HashMap<&str, &scryer_core::C4Node> =
        baseline.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    let curr_nodes: HashMap<&str, &scryer_core::C4Node> =
        current.nodes.iter().map(|n| (n.id.as_str(), n)).collect();

    let mut added: Vec<&str> = Vec::new();
    let mut removed: Vec<&str> = Vec::new();
    let mut modified: Vec<&str> = Vec::new();

    for n in &current.nodes {
        match base_nodes.get(n.id.as_str()) {
            None => added.push(&n.data.name),
            Some(base) => {
                if base.data.name != n.data.name
                    || base.data.description != n.data.description
                    || base.data.kind != n.data.kind
                    || base.data.technology != n.data.technology
                    || base.data.status != n.data.status
                    || base.data.contract != n.data.contract
                    || base.parent_id != n.parent_id
                {
                    modified.push(&n.data.name);
                }
            }
        }
    }
    for n in &baseline.nodes {
        if !curr_nodes.contains_key(n.id.as_str()) {
            removed.push(&n.data.name);
        }
    }

    let mut parts: Vec<String> = Vec::new();
    if !modified.is_empty() {
        parts.push(format!("Updated: {}", modified.join(", ")));
    }
    if !added.is_empty() {
        parts.push(format!("Added: {}", added.join(", ")));
    }
    if !removed.is_empty() {
        parts.push(format!("Removed: {}", removed.join(", ")));
    }

    if parts.is_empty() {
        Ok("Model is up to date".to_string())
    } else {
        Ok(parts.join(". "))
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let settings = scryer_core::read_settings();
    let settings_state = Arc::new(Mutex::new(settings));

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(SettingsState(settings_state))
        .manage(AcpState(Mutex::new(None)))
        .manage(SyncSnapshot(Mutex::new(None)))
        .setup(move |app| {
            let handle = app.handle().clone();
            let dir = scryer_core::models_dir();
            let _ = std::fs::create_dir_all(&dir);

            // Track known model names so we can detect genuinely new models.
            // On Windows, atomic rename (temp + rename) fires Remove + Create instead
            // of Modify. We intentionally keep names in the set on Remove so that a
            // subsequent Create from an atomic rename is treated as a change, not a
            // new model. True deletions are handled by list refresh in the frontend.
            let mut known_models: HashSet<String> = std::fs::read_dir(&dir)
                .into_iter()
                .flatten()
                .filter_map(|e| e.ok())
                .filter_map(|e| {
                    let p = e.path();
                    if p.extension().map_or(true, |x| x != "scry") { return None; }
                    let stem = p.file_stem()?.to_str()?;
                    if stem.ends_with(".baseline") { return None; }
                    Some(stem.to_string())
                })
                .collect();

            let mut watcher = recommended_watcher(move |res: Result<notify::Event, notify::Error>| {
                let Ok(event) = res else { return };
                if !matches!(
                    event.kind,
                    EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
                ) {
                    return;
                }
                for path in &event.paths {
                    if path.extension().map_or(true, |e| e != "scry") {
                        continue;
                    }
                    let Some(name) = path.file_stem().and_then(|s| s.to_str()) else {
                        continue;
                    };
                    if name.ends_with(".baseline") {
                        continue;
                    }
                    // Skip Remove events — don't clear from known_models so that
                    // Windows atomic rename (Remove + Create) won't falsely emit
                    // model-created. The frontend refreshes the list to detect
                    // true deletions.
                    if matches!(event.kind, EventKind::Remove(_)) {
                        continue;
                    }
                    if known_models.insert(name.to_string()) {
                        let _ = handle.emit("model-created", name.to_string());
                    }
                    let _ = handle.emit("model-changed", name.to_string());
                }
            })
            .map_err(|e| e.to_string())?;

            watcher
                .watch(&dir, RecursiveMode::NonRecursive)
                .map_err(|e| e.to_string())?;

            // Keep watcher alive for the app's lifetime
            app.manage(Mutex::new(watcher));

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_models,
            read_model,
            write_model,
            delete_model,
            get_hints,
            fetch_models,
            list_templates,
            load_template,
            get_ai_settings,
            save_ai_settings,
            open_in_editor,
            detect_ai_tools,
            setup_mcp_integration,
            check_drift,
            mark_synced,
            get_active_agent,
            start_agent_session,
            cancel_agent_session,
            sync_diff,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
