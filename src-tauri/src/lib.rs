use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use notify::{recommended_watcher, EventKind, RecursiveMode, Watcher};
use tauri::{Emitter, Manager, path::BaseDirectory};

/// Tracks model names recently written by the UI with timestamps, so the file
/// watcher can suppress ALL events from a single UI write (atomic writes on
/// Linux fire multiple inotify events: one for the temp file, one for the rename).
struct SelfWrites(Arc<Mutex<HashMap<String, Instant>>>);

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
fn write_model(name: String, data: String, state: tauri::State<'_, SelfWrites>) -> Result<(), String> {
    state.0.lock().unwrap().insert(name.clone(), Instant::now());
    scryer_core::write_model_raw(&name, &data)
}

#[tauri::command]
fn delete_model(name: String, state: tauri::State<'_, SelfWrites>) -> Result<(), String> {
    state.0.lock().unwrap().insert(name.clone(), Instant::now());
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let self_writes = Arc::new(Mutex::new(HashMap::<String, Instant>::new()));
    let settings = scryer_core::read_settings();
    let settings_state = Arc::new(Mutex::new(settings));

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(SelfWrites(self_writes.clone()))
        .manage(SettingsState(settings_state))
        .setup(move |app| {
            let handle = app.handle().clone();
            let writes = self_writes.clone();
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
                    {
                        let mut guard = writes.lock().unwrap();
                        if let Some(written_at) = guard.get(name) {
                            if written_at.elapsed().as_millis() < 1000 {
                                continue; // written by UI recently, skip
                            }
                            // Stale entry — clean it up
                            guard.remove(name);
                        }
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
            list_templates,
            load_template,
            get_ai_settings,
            save_ai_settings,
            open_in_editor,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
