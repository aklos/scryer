use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use notify::{recommended_watcher, EventKind, RecursiveMode, Watcher};
use tauri::{Emitter, Manager, path::BaseDirectory};

/// Managed state wrapping the AI settings.
struct SettingsState(Arc<Mutex<scryer_core::AiSettings>>);

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
async fn run_command(command: String, project_path: Option<String>) -> Result<serde_json::Value, String> {
    let cwd = project_path
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    let output = tokio::process::Command::new("sh")
        .arg("-c")
        .arg(&command)
        .current_dir(&cwd)
        .output()
        .await
        .map_err(|e| format!("Failed to run command: {e}"))?;

    Ok(serde_json::json!({
        "exitCode": output.status.code().unwrap_or(-1),
        "stdout": String::from_utf8_lossy(&output.stdout),
        "stderr": String::from_utf8_lossy(&output.stderr),
    }))
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
            // Fallback: xdg-open on Linux, open on macOS
            let fallback = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
            std::process::Command::new(fallback)
                .arg(&*path_str)
                .stdin(std::process::Stdio::null())
                .spawn()
                .map_err(|e| format!("Failed to open file: {e}"))?;
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

/// Resolve the Claude settings path: per-project if project_path given, otherwise global.
fn claude_settings_path(project_path: &Option<String>) -> Option<PathBuf> {
    if let Some(ref pp) = project_path {
        Some(PathBuf::from(pp).join(".claude").join("settings.json"))
    } else {
        dirs::home_dir().map(|h| h.join(".claude").join("settings.json"))
    }
}

/// Check a Claude settings file for scryer hook and permissions.
fn check_claude_settings(path: &PathBuf) -> (bool, bool) {
    let mut hook_enabled = false;
    let mut perms_enabled = false;
    if let Ok(contents) = std::fs::read_to_string(path) {
        if let Ok(root) = serde_json::from_str::<serde_json::Value>(&contents) {
            hook_enabled = root.pointer("/hooks/PostToolUse")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().any(|entry| {
                    entry["hooks"].as_array()
                        .map(|h| h.iter().any(|hk| {
                            hk["command"].as_str()
                                .map(|c| c.contains("scryer-mcp check-drift"))
                                .unwrap_or(false)
                        }))
                        .unwrap_or(false)
                }))
                .unwrap_or(false);

            perms_enabled = root.pointer("/permissions/allow")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().any(|v| {
                    v.as_str() == Some("mcp__scryer__*")
                }))
                .unwrap_or(false);
        }
    }
    (hook_enabled, perms_enabled)
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
    let mut hook_enabled = false;
    let mut perms_enabled = false;

    let mut hook_global = false;
    let mut perms_global = false;

    if has_claude {
        // Check global first to know the source
        if let Some(home) = dirs::home_dir() {
            let global_settings = home.join(".claude").join("settings.json");
            let (h, p) = check_claude_settings(&global_settings);
            hook_global = h;
            perms_global = p;
            hook_enabled = h;
            perms_enabled = p;
        }
        // Check per-project (overrides global for detection)
        if let Some(ref pp) = project_path {
            let project_settings = PathBuf::from(pp).join(".claude").join("settings.json");
            let (h, p) = check_claude_settings(&project_settings);
            if h { hook_enabled = true; }
            if p { perms_enabled = true; }
        }
    }

    let claude_mcp = project_path.as_deref().map(check_mcp_json).unwrap_or(false);
    let codex_mcp = project_path.as_deref().map(check_codex_toml).unwrap_or(false);

    serde_json::json!({
        "claude": has_claude,
        "codex": has_codex,
        "claudeHookEnabled": hook_enabled,
        "claudePermsEnabled": perms_enabled,
        "claudeHookGlobal": hook_global,
        "claudePermsGlobal": perms_global,
        "claudeMcpEnabled": claude_mcp,
        "codexMcpEnabled": codex_mcp,
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
fn setup_claude_integration(
    action: String,
    project_path: Option<String>,
) -> Result<String, String> {
    let settings_path = claude_settings_path(&project_path)
        .ok_or("Could not determine Claude settings path")?;
    if let Some(parent) = settings_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let mut root: serde_json::Value = if settings_path.exists() {
        let contents = std::fs::read_to_string(&settings_path).map_err(|e| e.to_string())?;
        serde_json::from_str(&contents).unwrap_or_else(|_| serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    match action.as_str() {
        "hook" => {
            let binary_path = find_scryer_mcp()
                .ok_or("scryer-mcp binary not found")?;

            if !root.get("hooks").is_some_and(|v| v.is_object()) {
                root["hooks"] = serde_json::json!({});
            }

            let hook_entry = serde_json::json!({
                "matcher": "Edit|Write",
                "hooks": [{
                    "type": "command",
                    "command": format!("{} check-drift", binary_path),
                }]
            });

            if let Some(arr) = root["hooks"]["PostToolUse"].as_array_mut() {
                let already = arr.iter().any(|entry| {
                    entry["hooks"]
                        .as_array()
                        .map(|h| h.iter().any(|hk| {
                            hk["command"].as_str().map(|c| c.contains("scryer-mcp check-drift")).unwrap_or(false)
                        }))
                        .unwrap_or(false)
                });
                if !already {
                    arr.push(hook_entry);
                }
            } else {
                root["hooks"]["PostToolUse"] = serde_json::json!([hook_entry]);
            }
        }
        "permissions" => {
            if !root.get("permissions").is_some_and(|v| v.is_object()) {
                root["permissions"] = serde_json::json!({});
            }
            if let Some(arr) = root["permissions"]["allow"].as_array_mut() {
                let rule = serde_json::Value::String("mcp__scryer__*".to_string());
                if !arr.contains(&rule) {
                    arr.push(rule);
                }
            } else {
                root["permissions"]["allow"] = serde_json::json!(["mcp__scryer__*"]);
            }
        }
        "remove_hook" => {
            if let Some(arr) = root.pointer_mut("/hooks/PostToolUse").and_then(|v| v.as_array_mut()) {
                arr.retain(|entry| {
                    !entry["hooks"]
                        .as_array()
                        .map(|h| h.iter().any(|hk| {
                            hk["command"].as_str().map(|c| c.contains("scryer-mcp check-drift")).unwrap_or(false)
                        }))
                        .unwrap_or(false)
                });
            }
        }
        "remove_permissions" => {
            if let Some(arr) = root.pointer_mut("/permissions/allow").and_then(|v| v.as_array_mut()) {
                arr.retain(|v| v.as_str() != Some("mcp__scryer__*"));
            }
        }
        "mcp" => {
            // Write .mcp.json for Claude Code — requires project_path
            let pp = project_path.as_deref()
                .ok_or("project_path is required for MCP setup")?;
            let binary_path = find_scryer_mcp()
                .ok_or("scryer-mcp binary not found")?;

            let mcp_path = PathBuf::from(pp).join(".mcp.json");
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
            // Write .codex/config.toml for Codex — requires project_path
            let pp = project_path.as_deref()
                .ok_or("project_path is required for MCP setup")?;
            let binary_path = find_scryer_mcp()
                .ok_or("scryer-mcp binary not found")?;

            let codex_dir = PathBuf::from(pp).join(".codex");
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
        _ => return Err(format!("Unknown action: {}", action)),
    }

    std::fs::write(&settings_path, serde_json::to_string_pretty(&root).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;

    Ok(settings_path.to_string_lossy().to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let settings = scryer_core::read_settings();
    let settings_state = Arc::new(Mutex::new(settings));

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(SettingsState(settings_state))
        .setup(move |app| {
            let handle = app.handle().clone();
            let dir = scryer_core::models_dir();
            let _ = std::fs::create_dir_all(&dir);

            // Track known model names so we can detect new models from rename events
            // (atomic writes use temp + rename, which fires Modify instead of Create)
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
                    if matches!(event.kind, EventKind::Remove(_)) {
                        known_models.remove(name);
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
            run_command,
            detect_ai_tools,
            setup_claude_integration,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
