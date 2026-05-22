use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Mutex;

use notify::{recommended_watcher, EventKind, RecursiveMode, Watcher};
use tauri::{Emitter, Manager, path::BaseDirectory};

/// macOS GUI apps launched via Spotlight, Dock, or Finder inherit a minimal
/// PATH (`/usr/bin:/bin:/usr/sbin:/sbin`) that excludes user-installed tools.
/// Recover the user's real PATH by asking their login shell directly.
#[cfg(target_os = "macos")]
fn ensure_full_path() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());

        // Fish outputs PATH as space-separated; ask it for colon-separated
        let echo_cmd = if shell.ends_with("/fish") {
            "string join : $PATH"
        } else {
            "echo $PATH"
        };

        let Ok(output) = std::process::Command::new(&shell)
            .args(["-l", "-c", echo_cmd])
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .output()
        else {
            return;
        };

        if output.status.success() {
            let shell_path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !shell_path.is_empty() {
                std::env::set_var("PATH", &shell_path);
            }
        }
    });
}

/// Managed state for the ACP runtime (agent orchestration).
struct AcpState(Mutex<Option<scryer_acp::AcpRuntime>>);

/// Managed state for the file watcher — only the active project is watched.
struct WatcherState {
    project: Option<(PathBuf, notify::RecommendedWatcher)>,
}

/// True if the given path contains a recognizable codebase (has a manifest/etc).
#[tauri::command]
fn is_codebase(path: String) -> bool {
    scryer_core::scan::is_codebase(std::path::Path::new(&path))
}

/// True if the given project has a `.scryer/model.scry` whose version is not
/// the current v0.3 schema. Frontend uses this to surface a clear error.
#[tauri::command]
fn is_legacy_model(project_path: String) -> bool {
    scryer_core::is_legacy_model(std::path::Path::new(&project_path))
}

/// Watch `{project}/.scryer/` for model changes. Replaces any previous watcher.
#[tauri::command]
fn watch_project(
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
fn read_model(ref_str: String) -> Result<String, String> {
    let model_ref = scryer_core::ModelRef::parse(&ref_str)?;
    scryer_core::read_model_raw_at(&model_ref)
}

#[tauri::command]
fn write_model(ref_str: String, data: String) -> Result<(), String> {
    let model_ref = scryer_core::ModelRef::parse(&ref_str)?;
    scryer_core::write_model_raw_at(&model_ref, &data)
}

#[tauri::command]
fn delete_model(ref_str: String) -> Result<(), String> {
    let model_ref = scryer_core::ModelRef::parse(&ref_str)?;
    scryer_core::delete_model_at(&model_ref)
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

#[tauri::command]
fn get_active_agent() -> Result<serde_json::Value, String> {
    let launch = scryer_acp::detect_available_agent()
        .ok_or("No AI agent found. Install Claude Code or Codex.")?;
    let name = match &launch {
        scryer_acp::AgentLaunch::Cli { kind, .. } => match kind {
            scryer_acp::AgentKind::ClaudeCode => "claude-code",
            scryer_acp::AgentKind::Codex => "codex",
            scryer_acp::AgentKind::Other => "other",
        },
        scryer_acp::AgentLaunch::Acp { .. } => "acp",
    };
    Ok(serde_json::json!({
        "name": name,
        "available": true,
        "launch": launch,
    }))
}

/// Create a blank project-local model at `{project_path}/.scryer/model.scry`.
/// Returns the ModelRef string.
#[tauri::command]
fn create_blank_model(project_path: String) -> Result<String, String> {
    let project = std::path::Path::new(&project_path);
    if !project.exists() || !project.is_dir() {
        return Err(format!(
            "Project path does not exist or is not a directory: {}",
            project_path
        ));
    }
    let model_ref = scryer_core::ModelRef::ProjectLocal(project.to_path_buf());
    let model = scryer_core::ScryModel::new();
    scryer_core::write_model_at(&model_ref, &model)?;
    Ok(model_ref.to_ref_string())
}

#[tauri::command]
async fn start_initial_model_session(
    cwd: String,
    model_ref: String,
    app: tauri::AppHandle,
    state: tauri::State<'_, AcpState>,
) -> Result<String, String> {
    let mcp_binary = find_scryer_mcp()
        .ok_or("scryer-mcp binary not found")?;

    // Detect an available agent from PATH (no MCP connection needed)
    let launch = scryer_acp::detect_available_agent()
        .ok_or("No AI agent found. Install Claude Code or Codex first.")?;

    let prompt = scryer_acp::prompt::initial_model_prompt(&cwd);

    let runtime = {
        let mut rt = state.0.lock().unwrap();
        if rt.is_none() {
            *rt = Some(scryer_acp::AcpRuntime::new());
        }
        rt.clone().unwrap()
    };

    let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel();

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
        .start_session(agent_binary, mode, cwd, model_ref, mcp_binary, prompt, event_tx)
        .await
}

#[tauri::command]
async fn start_node_fill_session(
    cwd: String,
    model_ref: String,
    node_id: String,
    app: tauri::AppHandle,
    state: tauri::State<'_, AcpState>,
) -> Result<String, String> {
    let mcp_binary = find_scryer_mcp()
        .ok_or("scryer-mcp binary not found")?;

    let launch = scryer_acp::detect_available_agent()
        .ok_or("No AI agent found. Install Claude Code or Codex first.")?;

    let parsed_ref = scryer_core::ModelRef::parse(&model_ref)?;
    let model = scryer_core::read_model_at(&parsed_ref)?;
    let node = model
        .nodes
        .iter()
        .find(|n| n.id == node_id)
        .ok_or_else(|| format!("Node '{}' not found in model", node_id))?;
    let node_name = node.name.clone();
    let node_kind = serde_json::to_value(node.kind)
        .ok()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_default();

    let model_json = scryer_acp::prompt::serialize_model_for_prompt(&model);
    let prompt = scryer_acp::prompt::node_fill_prompt(
        &cwd, &node_id, &node_name, &node_kind, &model_json,
    );

    let runtime = {
        let mut rt = state.0.lock().unwrap();
        if rt.is_none() {
            *rt = Some(scryer_acp::AcpRuntime::new());
        }
        rt.clone().unwrap()
    };

    let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel();

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
        .start_session(agent_binary, mode, cwd, model_ref, mcp_binary, prompt, event_tx)
        .await
}

#[tauri::command]
async fn cancel_agent_session(
    state: tauri::State<'_, AcpState>,
) -> Result<(), String> {
    let runtime = {
        let rt = state.0.lock().unwrap();
        rt.clone().ok_or("ACP runtime not initialized")?
    };
    runtime.cancel().await?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(target_os = "macos")]
    ensure_full_path();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AcpState(Mutex::new(None)))
        .setup(move |app| {
            app.manage(Mutex::new(WatcherState { project: None }));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            watch_project,
            is_codebase,
            is_legacy_model,
            read_model,
            write_model,
            delete_model,
            list_templates,
            load_template,
            open_in_editor,
            detect_ai_tools,
            setup_mcp_integration,
            get_active_agent,
            create_blank_model,
            start_initial_model_session,
            start_node_fill_session,
            cancel_agent_session,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
