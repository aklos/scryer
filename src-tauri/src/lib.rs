mod highlight;
mod symbols;

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
///
/// The `AtomicBool` is a build-scoped cancel flag set DIRECTLY by
/// `cancel_agent_session` (not only via the runtime). Orchestrators reset it at
/// start and check it at every wave/scope boundary, so a "stop" pressed in a
/// no-session gap — or just before a queued parallel session starts — is still
/// honored. Without it, cancellation is edge-triggered on live sessions and gets
/// silently lost in those gaps.
struct AcpState(
    Mutex<Option<scryer_acp::AcpRuntime>>,
    std::sync::Arc<std::sync::atomic::AtomicBool>,
);

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
    // Serialize against agent (MCP) writes so a canvas save and a concurrent
    // model edit can't clobber each other mid read-modify-write.
    let _lock = scryer_core::lock_model(&model_ref)?;
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

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct SourceSpan {
    /// Path actually read, relative form echoed back for display.
    file: String,
    /// 1-based line number of the first returned line (includes context).
    start_line: u32,
    /// The mapped span (for highlighting), 1-based inclusive.
    focus_start: u32,
    focus_end: u32,
    /// Lines from `start_line` onward (context + focus), each a list of
    /// syntax-highlighted segments that concatenate back to the line.
    lines: Vec<Vec<highlight::Segment>>,
}

/// Language-agnostic fallback when no bundled grammar covers the file (or the
/// grammar misses the symbol): the first line that defines `symbol` (word-
/// boundary match next to a definition cue or an assignment/open-paren).
/// Returns the 1-based line.
fn text_search_symbol(lines: &[&str], symbol: &str) -> Option<u32> {
    let cues = [
        "fn ", "function", "def ", "class ", "struct ", "interface ", "enum ",
        "impl", "type ", "const ", "let ", "var ", "func ", "public", "private",
        "export", "module", "trait ", "object ", "sub ", "proc ",
    ];
    for (i, raw) in lines.iter().enumerate() {
        // word-boundary occurrence of the symbol
        let Some(pos) = raw.find(symbol) else { continue };
        let before_ok = pos == 0
            || !raw.as_bytes()[pos - 1].is_ascii_alphanumeric() && raw.as_bytes()[pos - 1] != b'_';
        let after_idx = pos + symbol.len();
        let after_ok = after_idx >= raw.len()
            || (!raw.as_bytes()[after_idx].is_ascii_alphanumeric() && raw.as_bytes()[after_idx] != b'_');
        if !(before_ok && after_ok) {
            continue;
        }
        let trimmed = raw.trim_start();
        let after = raw[after_idx..].trim_start();
        let looks_def = cues.iter().any(|c| trimmed.starts_with(c) || raw.contains(c))
            || after.starts_with('(')
            || after.starts_with('=')
            || after.starts_with(':')
            || after.starts_with('<');
        if looks_def {
            return Some(i as u32 + 1);
        }
    }
    None
}

/// Read a span of a source file for the inspector's code view.
///
/// `file` is the `SourceLocation.pattern`. The responsibility's *focus* is the
/// explicit `line`/`end_line` range — the statements that do its work. `symbol`
/// names the enclosing definition: it's the durable anchor (so the focus can be
/// shown even as line numbers drift) and bounds the surrounding context, so the
/// focus is rendered in-situ inside its function rather than the whole file.
/// When only `symbol` is given, the whole definition is the focus. Reads are
/// constrained to within `project_path`.
#[tauri::command]
fn read_source_span(
    project_path: String,
    file: String,
    symbol: Option<String>,
    line: Option<u32>,
    end_line: Option<u32>,
) -> Result<SourceSpan, String> {
    const PAD: u32 = 4; // context lines around the focus
    const NO_LINE_LIMIT: u32 = 40;
    const DEFAULT_SPAN: u32 = 30;
    const MAX_LINES: u32 = 80; // guard against whole-symbol dumps

    let base = PathBuf::from(&project_path);
    let path = base.join(&file);

    // Constrain to the project directory (reject path traversal / absolutes).
    let canon_base = base.canonicalize().map_err(|e| e.to_string())?;
    let canon = path
        .canonicalize()
        .map_err(|e| format!("{}: {}", file, e))?;
    if !canon.starts_with(&canon_base) {
        return Err(format!("{} is outside the project", file));
    }

    let contents = std::fs::read_to_string(&canon).map_err(|e| format!("{}: {}", file, e))?;
    let all: Vec<&str> = contents.lines().collect();
    let total = all.len() as u32;
    if total == 0 {
        return Ok(SourceSpan {
            file,
            start_line: 1,
            focus_start: 1,
            focus_end: 1,
            lines: Vec::new(),
        });
    }

    // Enclosing symbol span: tree-sitter first (exact body), then a
    // language-agnostic text search (start line + a default window).
    let sym_range: Option<(u32, u32)> =
        symbol.as_deref().filter(|s| !s.is_empty()).and_then(|s| {
            symbols::resolve(&canon, &contents, s, line)
                .or_else(|| text_search_symbol(&all, s).map(|st| (st, (st + DEFAULT_SPAN).min(total))))
        });

    // Focus: the responsibility's specific lines if given, else the whole
    // enclosing symbol, else the file head.
    let (focus_start, focus_end) = match line {
        Some(l) => {
            let fs = l.clamp(1, total);
            (fs, end_line.unwrap_or(fs).clamp(fs, total))
        }
        None => match sym_range {
            Some((s, e)) => (s, e),
            None => (1, NO_LINE_LIMIT.min(total)),
        },
    };

    // Context window: a few lines around the focus, clamped to the enclosing
    // symbol so we never spill into neighbouring code, and capped so a whole-
    // symbol focus can't dump hundreds of lines.
    let mut start = focus_start.saturating_sub(PAD).max(1);
    let mut end = (focus_end + PAD).min(total);
    if let Some((ss, se)) = sym_range {
        start = start.max(ss);
        end = end.min(se).max(focus_end.min(total));
    }
    if end.saturating_sub(start) + 1 > MAX_LINES {
        end = (start + MAX_LINES - 1).min(total);
    }

    // Syntax-highlight the whole file (line N → index N-1), falling back to
    // plain default-coloured segments for languages without a grammar, then
    // slice out the context window.
    let highlighted = highlight::highlight_lines(&canon, &contents).unwrap_or_else(|| {
        all.iter()
            .map(|l| {
                vec![highlight::Segment {
                    text: l.to_string(),
                    kind: String::new(),
                }]
            })
            .collect()
    });
    let lines: Vec<Vec<highlight::Segment>> = highlighted
        .into_iter()
        .skip(start as usize - 1)
        .take((end - start + 1) as usize)
        .collect();

    Ok(SourceSpan {
        file,
        start_line: start,
        focus_start,
        focus_end,
        lines,
    })
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
    let _lock = scryer_core::lock_model(&model_ref)?;
    let model = scryer_core::ScryModel::new();
    scryer_core::write_model_at(&model_ref, &model)?;
    Ok(model_ref.to_ref_string())
}

/// Run the deterministic, parser-only extractor over the codebase and return
/// its CONTEXT map (containers + per-file symbol index + dependency graph). No
/// AI agent is involved and NOTHING is persisted — this is the map the modeling
/// orchestrator slices per scope and hands to each subagent, never a model on
/// disk. The first write to `model.scry` is the agent's first enriched node.
#[tauri::command]
fn get_codebase_context(
    project_path: String,
) -> Result<scryer_extract::ProjectContext, String> {
    let project = std::path::Path::new(&project_path);
    if !project.is_dir() {
        return Err(format!(
            "Project path does not exist or is not a directory: {}",
            project_path
        ));
    }
    scryer_extract::extract_context(project)
}

#[tauri::command]
fn get_subagent_settings() -> scryer_core::SubagentSettings {
    scryer_core::read_subagent_settings()
}

#[tauri::command]
fn set_subagent_settings(settings: scryer_core::SubagentSettings) -> Result<(), String> {
    scryer_core::write_subagent_settings(&settings)
}

/// Resolve (model, effort) for the agent kind we're about to launch from its
/// per-agent settings. An empty model means "use the agent CLI's own default".
fn config_for_launch(
    s: &scryer_core::SubagentSettings,
    launch: &scryer_acp::AgentLaunch,
) -> (String, String) {
    match launch {
        scryer_acp::AgentLaunch::Cli {
            kind: scryer_acp::AgentKind::ClaudeCode,
            ..
        } => (s.claude.model.clone(), s.claude.effort.clone()),
        scryer_acp::AgentLaunch::Cli {
            kind: scryer_acp::AgentKind::Codex,
            ..
        } => (s.codex.model.clone(), s.codex.effort.clone()),
        _ => (String::new(), "medium".to_string()),
    }
}

#[tauri::command]
async fn start_initial_model_session(
    cwd: String,
    app: tauri::AppHandle,
    state: tauri::State<'_, AcpState>,
) -> Result<String, String> {
    let mcp_binary = find_scryer_mcp()
        .ok_or("scryer-mcp binary not found")?;

    // Detect an available agent from PATH (no MCP connection needed),
    // honoring the saved agent preference.
    let settings = scryer_core::read_subagent_settings();
    let launch = scryer_acp::detect_available_agent_pref(&settings.agent)
        .ok_or("No AI agent found. Install Claude Code or Codex first.")?;

    let prompt = scryer_acp::prompt::initial_model_prompt(&cwd);
    let (model_name, effort) = config_for_launch(&settings, &launch);

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
        .start_session(agent_binary, mode, cwd, model_name, effort, mcp_binary, prompt, event_tx)
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

    let settings = scryer_core::read_subagent_settings();
    let launch = scryer_acp::detect_available_agent_pref(&settings.agent)
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
    let (model_name, effort) = config_for_launch(&settings, &launch);

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
        .start_session(agent_binary, mode, cwd, model_name, effort, mcp_binary, prompt, event_tx)
        .await
}

/// Fast semantic pass: enrich an already-structured subtree (the deterministic
/// extractor built the structure; this adds the meaning). Mirrors
/// `start_node_fill_session` but uses the enrich-only prompt.
#[tauri::command]
async fn start_enrich_session(
    cwd: String,
    model_ref: String,
    node_id: String,
    app: tauri::AppHandle,
    state: tauri::State<'_, AcpState>,
) -> Result<String, String> {
    let mcp_binary = find_scryer_mcp()
        .ok_or("scryer-mcp binary not found")?;

    let settings = scryer_core::read_subagent_settings();
    let launch = scryer_acp::detect_available_agent_pref(&settings.agent)
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
    let prompt = scryer_acp::prompt::enrich_subtree_prompt(
        &cwd, &node_id, &node_name, &node_kind, &model_json,
    );
    let (model_name, effort) = config_for_launch(&settings, &launch);

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
        .start_session(agent_binary, mode, cwd, model_name, effort, mcp_binary, prompt, event_tx)
        .await
}

#[tauri::command]
async fn cancel_agent_session(
    state: tauri::State<'_, AcpState>,
) -> Result<(), String> {
    // Set the durable cancel flag FIRST so orchestrators stop launching new
    // sessions even if the runtime currently has none (a wave gap) or a queued
    // session is about to start. Then best-effort kill any live sessions.
    state.1.store(true, std::sync::atomic::Ordering::SeqCst);
    let runtime = {
        let rt = state.0.lock().unwrap();
        rt.clone()
    };
    if let Some(runtime) = runtime {
        let _ = runtime.cancel().await;
    }
    Ok(())
}

/// Map a Container node back to the project-relative directory it owns, so its
/// per-container code context can be sliced. Prefers the boundary glob the agent
/// set from `boundaryDir` (validated against the real container dirs), and falls
/// back to matching the node name against the context's container facts (which
/// covers the root container, whose empty dir carries no boundary glob).
fn derive_container_dir(
    node: &scryer_core::Node,
    model: &scryer_core::ScryModel,
    ctx: &scryer_extract::ProjectContext,
) -> Option<String> {
    if let Some(sources) = model.boundaries.get(&node.id) {
        if let Some(s) = sources.first() {
            let dir = s
                .pattern
                .trim_end_matches("/**/*")
                .trim_end_matches("/**")
                .trim_end_matches("/*")
                .to_string();
            if ctx.containers.iter().any(|c| c.dir == dir) {
                return Some(dir);
            }
        }
    }
    ctx.containers
        .iter()
        .find(|c| c.name == node.name)
        .map(|c| c.dir.clone())
}

/// Pair every deterministic-context container that actually has code to the
/// model Container node the agent created for it, so Wave 2 covers all of them.
///
/// The fragile case is the project ROOT unit (empty `dir`): [`add_container`]
/// only records a boundary glob for non-empty dirs, so the root node has no glob
/// to round-trip and `name`-matching is unreliable (it often collides with the
/// system's name). So: non-empty dirs match by glob (then exact name) and are
/// CONSUMED — even code-less units like a DB image — and the single empty-dir
/// root then claims whatever model container is left unmatched. Returns the
/// `(node_id, name, dir)` triples to model, plus the labels of any codeful
/// container that could NOT be paired, so the caller surfaces them rather than
/// silently dropping coverage.
fn map_codeful_containers(
    model: &scryer_core::ScryModel,
    ctx: &scryer_extract::ProjectContext,
) -> (Vec<(String, String, String)>, Vec<String>) {
    use std::collections::HashSet;
    let nodes: Vec<&scryer_core::Node> = model
        .nodes
        .iter()
        .filter(|n| n.kind == scryer_core::Kind::Container && n.external != Some(true))
        .collect();

    // The boundary-glob directory a node round-trips to, if it carries one.
    let node_dir = |n: &scryer_core::Node| -> Option<String> {
        model.boundaries.get(&n.id).and_then(|s| s.first()).map(|s| {
            s.pattern
                .trim_end_matches("/**/*")
                .trim_end_matches("/**")
                .trim_end_matches("/*")
                .to_string()
        })
    };

    let mut used: HashSet<String> = HashSet::new();
    let mut by_dir: Vec<(String, String)> = Vec::new(); // (dir, node_id)

    // 1. Non-empty dirs: glob match, then exact-name fallback. Consume the node
    //    even for code-less units so they can't be mistaken for the root below.
    for c in ctx.containers.iter().filter(|c| !c.dir.is_empty()) {
        let pick = nodes
            .iter()
            .find(|n| !used.contains(&n.id) && node_dir(n).as_deref() == Some(c.dir.as_str()))
            .or_else(|| nodes.iter().find(|n| !used.contains(&n.id) && n.name == c.name));
        if let Some(n) = pick {
            used.insert(n.id.clone());
            by_dir.push((c.dir.clone(), n.id.clone()));
        }
    }

    // 2. The root unit (empty dir) carries no glob — give it the one model
    //    container still unclaimed, when exactly one remains (unambiguous).
    if ctx.containers.iter().any(|c| c.dir.is_empty()) {
        let remaining: Vec<&scryer_core::Node> =
            nodes.iter().copied().filter(|n| !used.contains(&n.id)).collect();
        if let [only] = remaining.as_slice() {
            used.insert(only.id.clone());
            by_dir.push((String::new(), only.id.clone()));
        }
    }

    // 3. Emit the modeling list for containers that actually have code; report
    //    any codeful container we couldn't pair.
    let mut mapped = Vec::new();
    let mut unmapped = Vec::new();
    for c in &ctx.containers {
        if scryer_extract::slice_container(ctx, &c.dir).files.is_empty() {
            continue; // nothing to model (e.g. a database image)
        }
        match by_dir.iter().find(|(d, _)| d == &c.dir) {
            Some((_, node_id)) => {
                let name = nodes
                    .iter()
                    .find(|n| &n.id == node_id)
                    .map(|n| n.name.clone())
                    .unwrap_or_default();
                mapped.push((node_id.clone(), name, c.dir.clone()));
            }
            None => unmapped.push(if c.name.is_empty() {
                format!("'{}'", c.dir)
            } else {
                c.name.clone()
            }),
        }
    }
    (mapped, unmapped)
}

/// Render token usage for a one-line debug log. Leads with the "fresh" tokens
/// (input + output + cache-write) and breaks out cache-read separately — a flat
/// sum is avoided on purpose: cache-read is re-read every turn and bills at 0.1×,
/// so it dwarfs the other buckets in raw count while costing almost nothing.
///
/// The dollar figure is shown ONLY as an "API-equiv" aside. It is `total_cost_usd`
/// straight from the CLI, which is the public pay-as-you-go list price for these
/// tokens — NOT what a subscription (Max/Pro) draws. On a subscription nothing is
/// billed per build; real usage is metered server-side as the session/weekly %
/// the account dashboard shows, and that number is not available here. The
/// list-price figure is still useful: it moves proportionally with the
/// subscription draw, so it's a relative gauge between builds — just not dollars.
fn fmt_usage(u: &scryer_acp::Usage) -> String {
    let fresh = u.input_tokens + u.output_tokens + u.cache_creation_input_tokens;
    let breakdown = format!(
        "in {} / out {} / cache-write {} / cache-read {}",
        u.input_tokens, u.output_tokens, u.cache_creation_input_tokens, u.cache_read_input_tokens,
    );
    if u.cost_usd > 0.0 {
        format!("{fresh} fresh tokens ({breakdown}) · ≈${:.4} API-equiv", u.cost_usd)
    } else {
        // Codex reports no cost — just the token counts.
        format!("{fresh} fresh tokens ({breakdown})")
    }
}

/// How a single agent session ended. `Failed` is carried as the `Err` arm of
/// [`run_wave`]'s result, so this only distinguishes the two non-error endings.
enum WaveOutcome {
    /// The session finished its turn — the orchestrator moves to the next wave.
    Completed,
    /// The user cancelled — the orchestrator must abort the whole build.
    Cancelled,
}

/// Run one modeling agent session to completion. Forwards only the session's
/// *progress* events (messages, tool calls, activity) to the frontend and
/// translates its terminal event into a [`WaveOutcome`]/`Err`.
///
/// Crucially, per-session terminal events (`Completed`/`Cancelled`/`Failed`) are
/// NOT emitted to the frontend: a build is many sessions, and the frontend tears
/// the canvas down — re-enabling write-back — on the first terminal it sees. The
/// orchestrator owns the single terminal event for the whole build; until then
/// write-back must stay suppressed or later sessions' writes get clobbered.
///
/// Sessions can run concurrently (the runtime tracks one cancel handle per
/// session); Wave 2 starts several of these at once, bounded by the orchestrator.
///
/// Returns the session's outcome alongside the token usage it reported (CLI
/// agents report a turn total; ACP mode reports none, so usage stays zero). The
/// caller sums usage across the build's many sessions to log a grand total.
#[allow(clippy::too_many_arguments)]
async fn run_wave(
    runtime: &scryer_acp::AcpRuntime,
    agent_binary: &str,
    mode: &scryer_acp::runtime::LaunchMode,
    cwd: &str,
    model_name: &str,
    effort: &str,
    mcp_binary: &str,
    prompt: String,
    app: &tauri::AppHandle,
) -> Result<(WaveOutcome, scryer_acp::Usage), String> {
    let (tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel();
    runtime
        .start_session(
            agent_binary.to_string(),
            mode.clone(),
            cwd.to_string(),
            model_name.to_string(),
            effort.to_string(),
            mcp_binary.to_string(),
            prompt,
            tx,
        )
        .await?;

    // The agent reports its turn total once (Claude's `result` event); keep the
    // last value seen rather than summing, so a cumulative reporter (Codex) can't
    // be double-counted within a single session.
    let mut usage = scryer_acp::Usage::default();
    while let Some(ev) = event_rx.recv().await {
        match &ev {
            scryer_acp::AgentEvent::Completed { .. } => return Ok((WaveOutcome::Completed, usage)),
            scryer_acp::AgentEvent::Cancelled => return Ok((WaveOutcome::Cancelled, usage)),
            scryer_acp::AgentEvent::Failed { error } => return Err(error.clone()),
            // Token totals are for the orchestrator's log, not the canvas — keep
            // them out of the forwarded stream.
            scryer_acp::AgentEvent::Usage { usage: u } => usage = *u,
            // Progress only — forward so the activity readout stays live.
            _ => {
                let _ = app.emit("agent-event", &ev);
            }
        }
    }
    // Stream closed without an explicit terminal — treat as a clean finish.
    Ok((WaveOutcome::Completed, usage))
}

/// Orchestrate a full auto-context model build with zero per-level clicking:
/// extract the deterministic codebase context, then drive top-down modeling
/// sessions — Wave 1 builds the system + containers (fed the container facts),
/// then a serial Wave 2 models each container's components + symbols (each fed
/// its sliced code context). Returns immediately; progress streams via
/// "agent-event" and nodes stream onto the canvas as the agent writes them.
#[tauri::command]
async fn start_model_build(
    cwd: String,
    app: tauri::AppHandle,
    state: tauri::State<'_, AcpState>,
) -> Result<String, String> {
    let project = std::path::Path::new(&cwd);

    // 1. Deterministic context — instant, in-memory, never persisted as a model.
    let ctx = scryer_extract::extract_context(project)?;
    let containers_json = serde_json::to_string(&ctx.containers).map_err(|e| e.to_string())?;

    // 2. Ensure a model exists so the intent tools have something to read; a
    //    blank one if absent (the agent's writes become the first real content).
    let model_ref = scryer_core::ModelRef::ProjectLocal(project.to_path_buf());
    {
        let _lock = scryer_core::lock_model(&model_ref)?;
        if scryer_core::read_model_at(&model_ref).is_err() {
            scryer_core::write_model_at(&model_ref, &scryer_core::ScryModel::new())?;
        }
    }

    // 3. Resolve the agent + its launch config.
    let mcp_binary = find_scryer_mcp().ok_or("scryer-mcp binary not found")?;
    let settings = scryer_core::read_subagent_settings();
    let launch = scryer_acp::detect_available_agent_pref(&settings.agent)
        .ok_or("No AI agent found. Install Claude Code or Codex first.")?;
    let (model_name, effort) = config_for_launch(&settings, &launch);
    let (agent_binary, mode) = match launch {
        scryer_acp::AgentLaunch::Cli { binary, kind } => {
            (binary, scryer_acp::runtime::LaunchMode::Cli { kind })
        }
        scryer_acp::AgentLaunch::Acp { binary } => {
            (binary, scryer_acp::runtime::LaunchMode::Acp)
        }
    };

    let runtime = {
        let mut rt = state.0.lock().unwrap();
        if rt.is_none() {
            *rt = Some(scryer_acp::AcpRuntime::new());
        }
        rt.clone().unwrap()
    };

    // Build-scoped cancel flag: clear it for this fresh build. The orchestrator
    // checks it at the wave boundary and every parallel task re-checks it after
    // acquiring its slot, so a "stop" set by `cancel_agent_session` stops new
    // sessions even in a no-session gap or just before a queued session starts.
    let cancel_flag = state.1.clone();
    cancel_flag.store(false, std::sync::atomic::Ordering::SeqCst);

    // 4. Background orchestrator: Wave 1, then a bounded-parallel Wave 2.
    tokio::spawn(async move {
        let emit_msg = |text: String| {
            let _ = app.emit("agent-event", &scryer_acp::AgentEvent::Message { text });
        };

        // Debug instrumentation: wall-clock for the whole build and a running
        // token total summed across every session (Wave 1 + each Wave 2
        // container). Token counts come from CLI agents (Claude Code / Codex);
        // ACP-mode agents report none, so they stay zero.
        let build_start = std::time::Instant::now();
        let mut total_usage = scryer_acp::Usage::default();
        eprintln!("[build] start: {cwd}");

        // Wave 1 — system + containers.
        emit_msg("▶ Building system and containers…".into());
        let w1 = scryer_acp::prompt::build_system_prompt(&cwd, &containers_json);
        let w1_start = std::time::Instant::now();
        match run_wave(
            &runtime, &agent_binary, &mode, &cwd, &model_name, &effort, &mcp_binary, w1, &app,
        )
        .await
        {
            Ok((WaveOutcome::Completed, usage)) => {
                total_usage.add(&usage);
                eprintln!(
                    "[build] Wave 1 (system + containers): {:.1}s, {}",
                    w1_start.elapsed().as_secs_f64(),
                    fmt_usage(&usage),
                );
            }
            Ok((WaveOutcome::Cancelled, _)) => {
                let _ = app.emit("agent-event", &scryer_acp::AgentEvent::Cancelled);
                return;
            }
            Err(e) => {
                let _ = app.emit(
                    "agent-event",
                    &scryer_acp::AgentEvent::Failed {
                        error: format!("System/container pass failed: {e}"),
                    },
                );
                return;
            }
        }

        // Determine each container's scope from the model + the context.
        let model = match scryer_core::read_model_at(&model_ref) {
            Ok(m) => m,
            Err(e) => {
                let _ = app.emit(
                    "agent-event",
                    &scryer_acp::AgentEvent::Failed {
                        error: format!("Could not read model after the container pass: {e}"),
                    },
                );
                return;
            }
        };
        let (containers, unmapped) = map_codeful_containers(&model, &ctx);
        if !unmapped.is_empty() {
            // Never drop coverage silently — tell the user which units the agent
            // didn't create a container for (so Wave 2 can't model them).
            emit_msg(format!(
                "⚠ No container node for {} — not modeled. Re-run, or add the container by hand.",
                unmapped.join(", ")
            ));
        }

        // Wave 2 — components + symbols. Up to WAVE2_POOL containers are modeled
        // at once: each is an independent agent session (its code scope is sliced
        // independently and the model-write lock serializes their commits), so
        // they're safe to run concurrently. The pool bounds how many of the
        // user's own agent processes run at once against their subscription.
        const WAVE2_POOL: usize = 3;

        // Slice each codeful container's scope up front; skip source-less units
        // (e.g. a database image) — nothing to model there.
        let mut jobs: Vec<(String, String, String)> = Vec::new();
        for (id, name, dir) in containers {
            let scope = scryer_extract::slice_container(&ctx, &dir);
            if scope.files.is_empty() {
                continue;
            }
            if let Ok(scope_json) = serde_json::to_string(&scope) {
                jobs.push((id, name, scope_json));
            }
        }

        // Honor a stop pressed during the Wave 1→2 setup (read model + slice
        // every container): no session is live here, so the runtime cancel was a
        // no-op — this flag is the only signal that survives the gap.
        if cancel_flag.load(std::sync::atomic::Ordering::SeqCst) {
            let _ = app.emit("agent-event", &scryer_acp::AgentEvent::Cancelled);
            return;
        }

        emit_msg("▶ Modeling components and symbols…".into());

        // Shared across the parallel container tasks: the live active-node set
        // (drives the canvas rings — several can be amber at once), a cancel
        // flag, and a failure log we surface after the pool drains.
        let active: std::sync::Arc<tokio::sync::Mutex<std::collections::BTreeSet<String>>> =
            std::sync::Arc::new(tokio::sync::Mutex::new(std::collections::BTreeSet::new()));
        let failures: std::sync::Arc<tokio::sync::Mutex<Vec<String>>> =
            std::sync::Arc::new(tokio::sync::Mutex::new(Vec::new()));
        // Tokens summed across the parallel container sessions (debug log).
        let wave2_usage: std::sync::Arc<tokio::sync::Mutex<scryer_acp::Usage>> =
            std::sync::Arc::new(tokio::sync::Mutex::new(scryer_acp::Usage::default()));
        let sem = std::sync::Arc::new(tokio::sync::Semaphore::new(WAVE2_POOL));

        let mut handles = Vec::with_capacity(jobs.len());
        for (id, name, scope_json) in jobs {
            let sem = sem.clone();
            let active = active.clone();
            let cancelled = cancel_flag.clone();
            let failures = failures.clone();
            let wave2_usage = wave2_usage.clone();
            let runtime = runtime.clone();
            let agent_binary = agent_binary.clone();
            let mode = mode.clone();
            let cwd = cwd.clone();
            let model_name = model_name.clone();
            let effort = effort.clone();
            let mcp_binary = mcp_binary.clone();
            let app = app.clone();
            handles.push(tokio::spawn(async move {
                // Bound how many sessions run at once.
                let _permit = match sem.acquire().await {
                    Ok(p) => p,
                    Err(_) => return,
                };
                if cancelled.load(std::sync::atomic::Ordering::SeqCst) {
                    return;
                }
                {
                    let mut a = active.lock().await;
                    a.insert(id.clone());
                    let _ = app.emit(
                        "build-active-node",
                        a.iter().cloned().collect::<Vec<String>>(),
                    );
                }
                // Re-check right before launching: closes the window between the
                // post-acquire check and the session actually starting, so a
                // cancel during the brief setup above doesn't spawn a new agent.
                let c_start = std::time::Instant::now();
                let outcome = if cancelled.load(std::sync::atomic::Ordering::SeqCst) {
                    Ok((WaveOutcome::Cancelled, scryer_acp::Usage::default()))
                } else {
                    let _ = app.emit(
                        "agent-event",
                        &scryer_acp::AgentEvent::Message {
                            text: format!("Modeling container: {name}…"),
                        },
                    );
                    let w2 =
                        scryer_acp::prompt::build_container_prompt(&cwd, &name, &id, &scope_json);
                    run_wave(
                        &runtime, &agent_binary, &mode, &cwd, &model_name, &effort, &mcp_binary,
                        w2, &app,
                    )
                    .await
                };
                {
                    let mut a = active.lock().await;
                    a.remove(&id);
                    let _ = app.emit(
                        "build-active-node",
                        a.iter().cloned().collect::<Vec<String>>(),
                    );
                }
                match outcome {
                    Ok((WaveOutcome::Completed, usage)) => {
                        wave2_usage.lock().await.add(&usage);
                        eprintln!(
                            "[build] Wave 2 container '{name}': {:.1}s, {}",
                            c_start.elapsed().as_secs_f64(),
                            fmt_usage(&usage),
                        );
                    }
                    Ok((WaveOutcome::Cancelled, _)) => {
                        cancelled.store(true, std::sync::atomic::Ordering::SeqCst);
                    }
                    // One container failing shouldn't abort the rest of the build.
                    Err(e) => failures
                        .lock()
                        .await
                        .push(format!("Container '{name}' modeling failed: {e}")),
                }
            }));
        }

        for h in handles {
            let _ = h.await;
        }

        for f in failures.lock().await.iter() {
            emit_msg(f.clone());
        }

        total_usage.add(&*wave2_usage.lock().await);

        if cancel_flag.load(std::sync::atomic::Ordering::SeqCst) {
            eprintln!(
                "[build] cancelled after {:.1}s, {} (partial)",
                build_start.elapsed().as_secs_f64(),
                fmt_usage(&total_usage),
            );
            let _ = app.emit("build-active-node", Vec::<String>::new());
            let _ = app.emit("agent-event", &scryer_acp::AgentEvent::Cancelled);
            return;
        }

        // Anchor the reconcile point so the first drift check only examines
        // changes made AFTER the build, not the whole repo.
        let _ = scryer_core::write_sync_state(
            &model_ref,
            &scryer_core::drift::SyncState {
                reconciled_at: scryer_core::drift::now_secs(),
                commit: scryer_core::drift::head_commit(std::path::Path::new(&cwd)),
            },
        );

        let elapsed = build_start.elapsed().as_secs_f64();
        eprintln!(
            "[build] complete: {:.1}s total, {}",
            elapsed,
            fmt_usage(&total_usage),
        );

        let _ = app.emit("build-active-node", Vec::<String>::new());
        let fresh = total_usage.input_tokens
            + total_usage.output_tokens
            + total_usage.cache_creation_input_tokens;
        emit_msg(format!(
            "✓ Model build complete — {elapsed:.0}s, {fresh} tokens.",
        ));
        let _ = app.emit(
            "agent-event",
            &scryer_acp::AgentEvent::Completed {
                stop_reason: "build_complete".into(),
            },
        );
    });

    Ok("started".to_string())
}

/// Cheap, agent-free drift status: which boundary-owning nodes have code changes
/// since the last reconcile. Used to nudge the user to run a semantic check on
/// open — it never decides the model drifted, only where to look.
#[tauri::command]
fn get_drift_status(cwd: String) -> Result<Vec<scryer_core::drift::DriftScope>, String> {
    let project = std::path::Path::new(&cwd);
    let model_ref = scryer_core::ModelRef::ProjectLocal(project.to_path_buf());
    let model = scryer_core::read_model_at(&model_ref).map_err(|e| e.to_string())?;

    // A model that has never been reconciled has no `.sync` anchor, so the
    // baseline defaults to epoch 0 and *every* file reads as "changed since
    // reconcile" — flagging every boundary-owning node as drift, forever. There
    // is no baseline to diff against, so that verdict is pure noise. Seed the
    // anchor to the current commit/time (treat the model as in-sync as of now)
    // and report nothing; real drift then surfaces once code changes after this
    // point. Models built through the MCP tools land here, since only the in-app
    // build and drift-check completion write the anchor.
    if !model_ref.sync_path().exists() {
        let _ = scryer_core::write_sync_state(
            &model_ref,
            &scryer_core::drift::SyncState {
                reconciled_at: scryer_core::drift::now_secs(),
                commit: scryer_core::drift::head_commit(project),
            },
        );
        return Ok(Vec::new());
    }

    let sync = scryer_core::read_sync_state(&model_ref);
    Ok(scryer_core::drift::drifted_scopes(&model, project, &sync))
}

/// Run a semantic drift check: find the boundary-owning nodes whose code changed
/// since the last reconcile, then for each, an agent compares what the code DOES
/// against the node's responsibilities and records findings via `flag_drift`
/// (undescribed behaviour → vagrant responsibilities; stale claims → `changed`).
/// Returns immediately; progress + findings stream via "agent-event". The
/// reconcile anchor advances when the check finishes so the next run sees only
/// newer changes.
#[tauri::command]
async fn start_drift_check(
    cwd: String,
    app: tauri::AppHandle,
    state: tauri::State<'_, AcpState>,
) -> Result<String, String> {
    let project = std::path::Path::new(&cwd);
    let model_ref = scryer_core::ModelRef::ProjectLocal(project.to_path_buf());
    let model = scryer_core::read_model_at(&model_ref)
        .map_err(|e| format!("No model to check for drift: {e}"))?;
    let sync = scryer_core::read_sync_state(&model_ref);
    let scopes = scryer_core::drift::drifted_scopes(&model, project, &sync);

    let mcp_binary = find_scryer_mcp().ok_or("scryer-mcp binary not found")?;
    let settings = scryer_core::read_subagent_settings();
    let launch = scryer_acp::detect_available_agent_pref(&settings.agent)
        .ok_or("No AI agent found. Install Claude Code or Codex first.")?;
    let (model_name, effort) = config_for_launch(&settings, &launch);
    let (agent_binary, mode) = match launch {
        scryer_acp::AgentLaunch::Cli { binary, kind } => {
            (binary, scryer_acp::runtime::LaunchMode::Cli { kind })
        }
        scryer_acp::AgentLaunch::Acp { binary } => {
            (binary, scryer_acp::runtime::LaunchMode::Acp)
        }
    };
    let runtime = {
        let mut rt = state.0.lock().unwrap();
        if rt.is_none() {
            *rt = Some(scryer_acp::AcpRuntime::new());
        }
        rt.clone().unwrap()
    };

    let ctx = scryer_extract::extract_context(project)?;

    let cancel_flag = state.1.clone();
    cancel_flag.store(false, std::sync::atomic::Ordering::SeqCst);

    tokio::spawn(async move {
        let emit_msg = |text: String| {
            let _ = app.emit("agent-event", &scryer_acp::AgentEvent::Message { text });
        };

        let write_anchor = || {
            let _ = scryer_core::write_sync_state(
                &model_ref,
                &scryer_core::drift::SyncState {
                    reconciled_at: scryer_core::drift::now_secs(),
                    commit: scryer_core::drift::head_commit(std::path::Path::new(&cwd)),
                },
            );
        };

        if scopes.is_empty() {
            emit_msg("✓ Model is in sync with the code — nothing changed.".into());
            write_anchor();
            let _ = app.emit(
                "agent-event",
                &scryer_acp::AgentEvent::Completed {
                    stop_reason: "in_sync".into(),
                },
            );
            return;
        }

        emit_msg(format!(
            "▶ Checking {} changed scope(s) for drift…",
            scopes.len()
        ));
        for scope in &scopes {
            // Honor a stop pressed between scopes (the gap where no session is
            // live, so the runtime cancel is a no-op).
            if cancel_flag.load(std::sync::atomic::Ordering::SeqCst) {
                let _ = app.emit("build-active-node", Vec::<String>::new());
                let _ = app.emit("agent-event", &scryer_acp::AgentEvent::Cancelled);
                return;
            }
            let dir = model
                .nodes
                .iter()
                .find(|n| n.id == scope.node_id)
                .and_then(|n| derive_container_dir(n, &model, &ctx))
                .unwrap_or_default();
            let slice = scryer_extract::slice_container(&ctx, &dir);
            let scope_json = serde_json::to_string(&slice).unwrap_or_default();
            let changed_json = serde_json::to_string(&scope.changed_files).unwrap_or_default();
            // Feed only this node's subtree (its claims), not the whole model.
            let subtree_json =
                scryer_acp::prompt::serialize_subtree_for_prompt(&model, &scope.node_id);
            let _ = app.emit("build-active-node", vec![scope.node_id.clone()]);
            emit_msg(format!("▶ Drift check: {}…", scope.node_name));
            let prompt = scryer_acp::prompt::drift_check_prompt(
                &cwd,
                &scope.node_name,
                &scope.node_id,
                &subtree_json,
                &scope_json,
                &changed_json,
            );
            match run_wave(
                &runtime, &agent_binary, &mode, &cwd, &model_name, &effort, &mcp_binary, prompt,
                &app,
            )
            .await
            {
                Ok((WaveOutcome::Completed, _)) => {}
                Ok((WaveOutcome::Cancelled, _)) => {
                    let _ = app.emit("agent-event", &scryer_acp::AgentEvent::Cancelled);
                    return;
                }
                Err(e) => emit_msg(format!("Drift check for '{}' failed: {e}", scope.node_name)),
            }
        }

        write_anchor();
        let _ = app.emit("build-active-node", Vec::<String>::new());
        emit_msg("✓ Drift check complete — review any flagged items.".into());
        let _ = app.emit(
            "agent-event",
            &scryer_acp::AgentEvent::Completed {
                stop_reason: "drift_check_complete".into(),
            },
        );
    });

    Ok("started".to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(target_os = "macos")]
    ensure_full_path();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AcpState(
            Mutex::new(None),
            std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        ))
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
            read_source_span,
            detect_ai_tools,
            setup_mcp_integration,
            get_active_agent,
            create_blank_model,
            get_codebase_context,
            get_subagent_settings,
            set_subagent_settings,
            start_initial_model_session,
            start_node_fill_session,
            start_enrich_session,
            start_model_build,
            start_drift_check,
            get_drift_status,
            cancel_agent_session,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
