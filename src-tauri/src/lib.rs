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

/// Managed state for the deterministic preview server — one shared Vite dev
/// server per open project (Track B). The child's stdin is held open by the
/// handle; the sidecar exits when the pipe closes, so it can't outlive us.
struct PreviewState(tokio::sync::Mutex<Option<PreviewServer>>);

struct PreviewServer {
    cwd: String,
    url: String,
    child: tokio::process::Child,
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

/// Read the planned (draft) layer — the working model the canvas edits. Returns
/// the committed model's bytes when no plan has diverged yet (planned == model),
/// so a fresh project opens with an empty plan.
#[tauri::command]
fn read_planned(ref_str: String) -> Result<String, String> {
    let model_ref = scryer_core::ModelRef::parse(&ref_str)?;
    scryer_core::read_planned_raw_at(&model_ref)
}

/// Write the planned (draft) layer. The canvas saves here, never to `model.scry`
/// directly: the committed model only changes when the agent implements a plan
/// element and folds it (planned → model). Serialized against MCP writes, like
/// the committed-model write.
#[tauri::command]
fn write_planned(ref_str: String, data: String) -> Result<(), String> {
    let model_ref = scryer_core::ModelRef::parse(&ref_str)?;
    let _lock = scryer_core::lock_model(&model_ref)?;
    scryer_core::write_planned_raw_at(&model_ref, &data)
}

/// Read the durable committed-model history log (`.scryer/history.jsonl`),
/// returned as a JSON array of events, oldest first. Empty when the project has
/// no history yet. The frontend re-reads this whenever the model changes (every
/// event-producing agent operation also writes a `.scry` file the watcher sees).
#[tauri::command]
fn read_history(ref_str: String) -> Result<String, String> {
    let model_ref = scryer_core::ModelRef::parse(&ref_str)?;
    let events = scryer_core::history::read_history(&model_ref);
    serde_json::to_string(&events).map_err(|e| e.to_string())
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
/// shown even as line numbers drift) and bounds what we render — the whole
/// symbol body is returned with the focus lines flagged, so you read the focus
/// in full context. When only `symbol` is given, the whole definition is the
/// focus. With no symbol, the whole file is returned. The fixed-height scroll
/// viewport bounds the visual size, so the data is never truncated. Reads are
/// constrained to within `project_path`.
#[tauri::command]
fn read_source_span(
    project_path: String,
    file: String,
    symbol: Option<String>,
    line: Option<u32>,
    end_line: Option<u32>,
) -> Result<SourceSpan, String> {
    const NO_LINE_LIMIT: u32 = 40;
    const DEFAULT_SPAN: u32 = 30;

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

    // Render window: the whole enclosing symbol body (so the focus is always
    // read in full context), or the whole file when no symbol resolves. The
    // focus is always contained. The fixed-height scroll viewport on the
    // frontend bounds visual size, so we never truncate — that only ever hid
    // lines and lied about the range.
    let (start, end) = match sym_range {
        Some((ss, se)) => (ss.min(focus_start), se.max(focus_end).min(total)),
        None => (1, total),
    };

    // Syntax-highlight the whole file (line N → index N-1), falling back to
    // plain default-coloured segments for languages without a grammar, then
    // slice out the render window.
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
    "mcp__scryer__read_model",
    "mcp__scryer__search_model",
    "mcp__scryer__get_unimplemented",
    "mcp__scryer__get_rules",
    "mcp__scryer__read_codebase",
    "mcp__scryer__validate_model",
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
        .start_session(agent_binary, mode, cwd, model_name, effort, mcp_binary, prompt, vec!["mcp__scryer__*".into()], event_tx)
        .await
}

/// Does this node have real code behind it? True when it owns a boundary glob
/// or any of its responsibilities are anchored to source — i.e. it was built
/// from / maps to a codebase. False for a node the user just designed (no
/// boundary, no source anchors), which routes to strict design mode.
fn node_has_code(model: &scryer_core::ScryModel, node_id: &str) -> bool {
    if model.boundaries.get(node_id).is_some_and(|s| !s.is_empty()) {
        return true;
    }
    if let Some(node) = model.nodes.iter().find(|n| n.id == node_id) {
        if node
            .responsibilities
            .iter()
            .any(|r| model.source_map.contains_key(&r.id))
        {
            return true;
        }
    }
    model.source_map.contains_key(node_id)
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
    // Auto-select by code presence: a node backed by real code (a boundary glob,
    // or source-anchored responsibilities) is EXTRACTED from that code; a node
    // with no code behind it — greenfield design, or a not-yet-built addition —
    // is DESIGNED strictly: the agent models every relationship itself and marks
    // everything `proposed`, since there is nothing to extract.
    let prompt = if node_has_code(&model, &node_id) {
        scryer_acp::prompt::node_fill_prompt(&cwd, &node_id, &node_name, &node_kind, &model_json)
    } else {
        scryer_acp::prompt::node_design_prompt(&cwd, &node_id, &node_name, &node_kind, &model_json)
    };
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
        .start_session(agent_binary, mode, cwd, model_name, effort, mcp_binary, prompt, vec!["mcp__scryer__*".into()], event_tx)
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
        .start_session(agent_binary, mode, cwd, model_name, effort, mcp_binary, prompt, vec!["mcp__scryer__*".into()], event_tx)
        .await
}

// --- deterministic preview server (Track B) ----------------------------------

/// The preview sidecar's sources, embedded at compile time and written to
/// `{project}/.scryer/preview/server/` before launch, so the spawned `node`
/// process always runs the version matching this binary. The sidecar has no
/// npm dependencies of its own — it resolves `vite` and `typescript` from the
/// target project's node_modules.
const PREVIEW_SIDECAR: &[(&str, &str)] = &[
    ("server.mjs", include_str!("../../preview/server.mjs")),
    ("plugin.mjs", include_str!("../../preview/plugin.mjs")),
    ("props.mjs", include_str!("../../preview/props.mjs")),
];

/// Start (or reuse) the shared preview dev server for a project and return its
/// base URL. Deterministic rendering: any visual component is then viewable at
/// `{url}/__preview?file=...&export=...` with no agent involvement.
#[tauri::command]
async fn ensure_preview_server(
    cwd: String,
    state: tauri::State<'_, PreviewState>,
) -> Result<String, String> {
    use tokio::io::AsyncBufReadExt;

    #[cfg(target_os = "macos")]
    ensure_full_path();

    let mut guard = state.0.lock().await;

    // Reuse a live server for the same project; replace anything else.
    if let Some(srv) = guard.as_mut() {
        let alive = matches!(srv.child.try_wait(), Ok(None));
        if alive && srv.cwd == cwd {
            return Ok(srv.url.clone());
        }
        let _ = srv.child.kill().await;
        *guard = None;
    }

    let dir = PathBuf::from(&cwd)
        .join(".scryer")
        .join("preview")
        .join("server");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    for (name, source) in PREVIEW_SIDECAR {
        std::fs::write(dir.join(name), source).map_err(|e| e.to_string())?;
    }

    let mut child = tokio::process::Command::new("node")
        .arg(dir.join("server.mjs"))
        .arg(&cwd)
        .arg("--exit-on-stdin-close")
        .current_dir(&cwd)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("failed to launch preview server (is node installed?): {e}"))?;

    let stdout = child.stdout.take().ok_or("preview server has no stdout")?;
    let mut lines = tokio::io::BufReader::new(stdout).lines();
    let url = tokio::time::timeout(std::time::Duration::from_secs(60), async {
        while let Ok(Some(line)) = lines.next_line().await {
            if let Some(url) = line.strip_prefix("SCRYER_PREVIEW_URL=") {
                return Ok(url.trim().to_string());
            }
        }
        Err("preview server exited before reporting its URL".to_string())
    })
    .await
    .map_err(|_| "preview server startup timed out".to_string())?;
    let url = match url {
        Ok(url) => url,
        Err(e) => {
            let _ = child.kill().await;
            return Err(e);
        }
    };

    // Keep draining stdout so the sidecar never blocks (or dies on EPIPE)
    // writing logs after we stop caring about them.
    tokio::spawn(async move { while let Ok(Some(_)) = lines.next_line().await {} });

    *guard = Some(PreviewServer {
        cwd,
        url: url.clone(),
        child,
    });
    Ok(url)
}

/// Find a node's anchored source file: the node-id source-map entry (data
/// shapes) or the first responsibility's source location.
fn node_source_file(model: &scryer_core::ScryModel, node_id: &str) -> String {
    let from_node = model
        .source_map
        .get(node_id)
        .and_then(|locs| locs.first())
        .map(|loc| loc.pattern.clone());
    from_node
        .or_else(|| {
            model
                .nodes
                .iter()
                .find(|n| n.id == node_id)
                .and_then(|node| {
                    node.responsibilities
                        .iter()
                        .find_map(|r| model.source_map.get(&r.id))
                        .and_then(|locs| locs.first())
                        .map(|loc| loc.pattern.clone())
                })
        })
        .unwrap_or_default()
}

/// Repair path for a failed deterministic render (B6). The preview server
/// renders components with synthesized placeholder props; when that comes out
/// empty or crashes, this launches an agent that authors realistic data —
/// primarily a shared, type-keyed fixture set (`.scryer/preview/fixtures/`
/// `shared.tsx` + `manifest.json`) reused across every component touching a
/// type, with a per-node override file as fallback. The preview server picks
/// the files up automatically — no build step.
#[tauri::command]
async fn start_preview_fixture_session(
    cwd: String,
    model_ref: String,
    node_id: String,
    render_status: String,
    render_error: Option<String>,
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

    let source_file = node_source_file(&model, &node_id);
    let source_lines = if !source_file.is_empty() {
        let abs = PathBuf::from(&cwd).join(&source_file);
        std::fs::read_to_string(&abs).unwrap_or_default()
    } else {
        String::new()
    };

    let prompt = scryer_acp::prompt::preview_fixture_prompt(
        &cwd,
        &node_id,
        &node_name,
        &source_file,
        &source_lines,
        &render_status,
        render_error.as_deref().unwrap_or(""),
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

    let allowed_tools = vec![
        "mcp__scryer__*".into(),
        "Write".into(),
        "Edit".into(),
        "Bash".into(),
    ];

    runtime
        .start_session(agent_binary, mode, cwd, model_name, effort, mcp_binary, prompt, allowed_tools, event_tx)
        .await
}

/// Generate visual variations of a component (B6). Launches an agent that
/// writes N self-contained variant modules under
/// `.scryer/preview/variations/{nodeId}/{i}.tsx`; the always-running preview
/// server serves each as a virtual entry instantly — no build step.
/// Ephemeral — does NOT update the model on completion.
#[tauri::command]
async fn start_visual_variation_session(
    cwd: String,
    model_ref: String,
    node_id: String,
    prompt: String,
    variation_count: Option<usize>,
    base_variation_idx: Option<u32>,
    app: tauri::AppHandle,
    state: tauri::State<'_, AcpState>,
) -> Result<String, String> {
    let variation_count = variation_count.unwrap_or(3).clamp(1, 5);

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

    let source_file = node_source_file(&model, &node_id);
    let source_lines = if !source_file.is_empty() {
        let abs = PathBuf::from(&cwd).join(&source_file);
        std::fs::read_to_string(&abs).unwrap_or_default()
    } else {
        String::new()
    };

    // If iterating on a previous variation, that variant is the base; otherwise
    // a previously accepted variant (if any) is.
    let base_variant = if let Some(idx) = base_variation_idx {
        let path = PathBuf::from(&cwd)
            .join(".scryer/preview/variations")
            .join(&node_id)
            .join(format!("{idx}.tsx"));
        std::fs::read_to_string(&path).unwrap_or_default()
    } else {
        let path = PathBuf::from(&cwd)
            .join(".scryer/preview/accepted")
            .join(format!("{node_id}.tsx"));
        std::fs::read_to_string(&path).unwrap_or_default()
    };

    let agent_prompt = scryer_acp::prompt::visual_variation_prompt(
        &cwd, &node_id, &node_name, &source_file, &source_lines,
        &prompt, &base_variant, variation_count,
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

    let allowed_tools = vec![
        "mcp__scryer__*".into(),
        "Write".into(),
        "Edit".into(),
        "Bash".into(),
    ];

    runtime
        .start_session(agent_binary, mode, cwd, model_name, effort, mcp_binary, agent_prompt, allowed_tools, event_tx)
        .await
}

/// Accept a visual variation: persist the chosen variant module as the node's
/// appearance (`.scryer/preview/accepted/{nodeId}.tsx`), point the model's
/// preview metadata at it, and clean up the variation files. The accepted
/// variant is design intent — the node renders it (status `changed`) until the
/// real component code catches up.
#[tauri::command]
async fn accept_visual_variation(
    cwd: String,
    model_ref: String,
    node_id: String,
    variation_idx: u32,
) -> Result<(), String> {
    let vars_dir = PathBuf::from(&cwd).join(".scryer/preview/variations").join(&node_id);
    let variant = vars_dir.join(format!("{variation_idx}.tsx"));
    if !variant.exists() {
        return Err(format!("Variation {} not found", variation_idx));
    }

    let accepted_rel = format!(".scryer/preview/accepted/{node_id}.tsx");
    let accepted = PathBuf::from(&cwd).join(&accepted_rel);
    std::fs::create_dir_all(accepted.parent().unwrap()).map_err(|e| e.to_string())?;
    std::fs::copy(&variant, &accepted).map_err(|e| e.to_string())?;

    // Update model preview metadata
    let parsed_ref = scryer_core::ModelRef::parse(&model_ref)?;
    let _lock = scryer_core::lock_model(&parsed_ref).ok();
    if let Ok(mut m) = scryer_core::read_model_at(&parsed_ref) {
        if let Some(n) = m.nodes.iter_mut().find(|n| n.id == node_id) {
            n.appearance = Some(scryer_core::Appearance {
                status: Some(scryer_core::RenderState::Changed),
                dist_path: Some(accepted_rel),
                built_at: Some(scryer_core::drift::now_secs()),
                source_hash: None,
            });
        }
        let _ = scryer_core::write_model_at(&parsed_ref, &m);
    }

    let _ = std::fs::remove_dir_all(&vars_dir);
    Ok(())
}

/// Discard visual variations: remove the node's variation files.
#[tauri::command]
async fn discard_visual_variations(
    cwd: String,
    node_id: String,
) -> Result<(), String> {
    let vars_dir = PathBuf::from(&cwd)
        .join(".scryer/preview/variations")
        .join(&node_id);
    if vars_dir.exists() {
        std::fs::remove_dir_all(&vars_dir).map_err(|e| e.to_string())?;
    }
    Ok(())
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

/// Adapt extracted container facts to the model crate's seeding input.
fn seed_units(ctx: &scryer_extract::ProjectContext) -> Vec<scryer_core::seed::SeedUnit> {
    ctx.containers
        .iter()
        .map(|c| scryer_core::seed::SeedUnit {
            dir: c.dir.clone(),
            name: c.name.clone(),
            technology: c.technology.clone(),
            dep_dirs: c.dep_dirs.clone(),
        })
        .collect()
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

struct Wave2Job {
    id: String,
    name: String,
    evidence_json: String,
    work_units: usize,
    payload_bytes: usize,
}

/// Derive agent concurrency from the actual evidence payload rather than a
/// fixed pool. Small scopes are cheap enough to fan out; large prompts get fewer
/// concurrent sessions to avoid memory/subscription pressure.
///
/// Byte thresholds are calibrated for evidence-embedded payloads (each symbol
/// carries its source excerpt, ~10x the bare index): a typical scope lands at
/// 50–250 KB, and only a genuinely huge scope should sacrifice concurrency.
fn wave2_pool_size(jobs: &[Wave2Job]) -> usize {
    if jobs.is_empty() {
        return 1;
    }
    let average_bytes = jobs.iter().map(|job| job.payload_bytes).sum::<usize>() / jobs.len();
    let cpu_cap = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(2)
        .clamp(1, 4);
    let payload_cap = match average_bytes {
        0..=150_000 => 4,
        150_001..=450_000 => 3,
        _ => 2,
    };
    jobs.len().min(cpu_cap).min(payload_cap).max(1)
}

fn wave2_job_permits(job: &Wave2Job, pool: usize) -> u32 {
    let desired = match (job.payload_bytes, job.work_units) {
        (450_001.., _) | (_, 8_001..) => pool,
        (150_001.., _) | (_, 3_001..) => 2,
        _ => 1,
    };
    desired.min(pool).max(1) as u32
}

#[cfg(test)]
mod build_scheduling_tests {
    use super::{wave2_job_permits, wave2_pool_size, Wave2Job};

    fn jobs(count: usize, bytes: usize) -> Vec<Wave2Job> {
        (0..count)
            .map(|idx| Wave2Job {
                id: format!("node-{idx}"),
                name: format!("job-{idx}"),
                evidence_json: String::new(),
                work_units: idx,
                payload_bytes: bytes,
            })
            .collect()
    }

    #[test]
    fn large_prompts_never_get_more_concurrency_than_small_prompts() {
        let small = jobs(8, 60_000);
        let medium = jobs(8, 250_000);
        let large = jobs(8, 900_000);
        assert!(wave2_pool_size(&large) <= wave2_pool_size(&small));
        assert_eq!(wave2_pool_size(&jobs(0, 0)), 1);
        assert_eq!(wave2_pool_size(&jobs(1, 60_000)), 1);
        assert_eq!(wave2_job_permits(&small[0], 4), 1);
        assert_eq!(wave2_job_permits(&medium[0], 4), 2);
        assert_eq!(wave2_job_permits(&large[0], 4), 4);
    }
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
            vec!["mcp__scryer__*".into()],
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
/// extract the deterministic codebase context, mint the system + container
/// skeleton mechanically from manifest facts (instant — no agent), then run
/// EVERY semantic session concurrently: the system-level pass (persons,
/// externals, responsibilities, names) beside one adaptive-pool session per
/// code-bearing container (each fed its compact sliced code context with
/// embedded source evidence). Wall clock approaches max(single session)
/// instead of wave1 + slowest round. Returns immediately; progress streams via
/// "agent-event" and nodes stream onto the canvas as the agent writes them.
#[tauri::command]
async fn start_model_build(
    cwd: String,
    app: tauri::AppHandle,
    state: tauri::State<'_, AcpState>,
) -> Result<String, String> {
    let project = std::path::Path::new(&cwd);

    // 1. Deterministic context — instant, in-memory, never persisted as a model.
    let (ctx, extraction) = scryer_extract::extract_context_with_stats(project)?;
    eprintln!(
        "[build] extraction: {} source files, {} parsed, {} cache hits",
        extraction.source_files, extraction.parsed_files, extraction.cache_hits,
    );
    // Cache the deterministic symbol dependency graph so the MCP
    // `commit_container_model` tool (a separate process) wires code-level links
    // from the same edges the agent saw, instead of having the agent author
    // them by hand and fight the same-level link validator. Best-effort.
    let build_edges = scryer_core::build_edges::BuildEdges {
        symbol_edges: ctx
            .symbol_edges
            .iter()
            .map(|e| scryer_core::build_edges::CachedEdge {
                src: e.src.clone(),
                dst: e.dst.clone(),
            })
            .collect(),
    };
    if let Err(e) = scryer_core::build_edges::write_build_edges(project, &build_edges) {
        eprintln!("[build] could not cache dependency graph: {e}");
    }

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

        // No Wave 1 (A2): mint the structural skeleton mechanically — the
        // system + containers land on the canvas instantly, container coverage
        // holds by construction, and no session blocks any other.
        let minted = {
            let mint = || -> Result<(String, Vec<(String, String, String)>), String> {
                let _lock = scryer_core::lock_model(&model_ref)?;
                let mut model = scryer_core::read_model_at(&model_ref)?;
                let minted = scryer_core::seed::mint_initial_structure(
                    &mut model,
                    &ctx.project_name,
                    &seed_units(&ctx),
                );
                scryer_core::write_model_at(&model_ref, &model)?;
                Ok(minted)
            };
            match mint() {
                Ok(v) => v,
                Err(e) => {
                    let _ = app.emit(
                        "agent-event",
                        &scryer_acp::AgentEvent::Failed {
                            error: format!("Could not seed the model structure: {e}"),
                        },
                    );
                    return;
                }
            }
        };
        let (system_id, containers) = minted;

        // What the system-level semantic session works against: the minted
        // skeleton with authoritative ids.
        let structure_json = serde_json::to_string_pretty(
            &ctx.containers
                .iter()
                .zip(&containers)
                .map(|(c, (id, name, dir))| {
                    serde_json::json!({
                        "id": id, "name": name, "dir": dir,
                        "technology": c.technology, "depDirs": c.dep_dirs,
                    })
                })
                .collect::<Vec<_>>(),
        )
        .unwrap_or_default();

        // Slice each codeful container's scope up front; skip source-less units
        // (e.g. a database image). Convert to the compact indexed wire format so
        // paths and synthetic symbol keys are not repeated throughout the prompt.
        let mut jobs: Vec<Wave2Job> = Vec::new();
        for (id, name, dir) in containers {
            let scope = scryer_extract::slice_container(&ctx, &dir);
            if scope.files.is_empty() {
                continue;
            }
            let evidence = scryer_extract::compact_scope(&scope);
            if let Ok(evidence_json) = serde_json::to_string(&evidence) {
                jobs.push(Wave2Job {
                    id,
                    name,
                    payload_bytes: evidence_json.len(),
                    work_units: evidence.work_units(),
                    evidence_json,
                });
            }
        }
        // Longest-processing-time-first reduces the tail when containers vary
        // substantially in size.
        jobs.sort_by(|a, b| b.work_units.cmp(&a.work_units));
        let wave2_pool = wave2_pool_size(&jobs);
        eprintln!(
            "[build] Wave 2: {} job(s), adaptive pool {}, {} evidence bytes",
            jobs.len(),
            wave2_pool,
            jobs.iter().map(|job| job.payload_bytes).sum::<usize>(),
        );

        // Honor a stop pressed during setup (mint + slice every container): no
        // session is live here, so the runtime cancel was a no-op — this flag
        // is the only signal that survives the gap.
        if cancel_flag.load(std::sync::atomic::Ordering::SeqCst) {
            let _ = app.emit("agent-event", &scryer_acp::AgentEvent::Cancelled);
            return;
        }

        emit_msg("▶ Modeling the system and every container in parallel…".into());

        // Shared across the parallel tasks: the live active-node set (drives
        // the canvas rings — several can be amber at once), a cancel flag, and
        // a failure log we surface after the pool drains.
        let active: std::sync::Arc<tokio::sync::Mutex<std::collections::BTreeSet<String>>> =
            std::sync::Arc::new(tokio::sync::Mutex::new(std::collections::BTreeSet::new()));
        let failures: std::sync::Arc<tokio::sync::Mutex<Vec<String>>> =
            std::sync::Arc::new(tokio::sync::Mutex::new(Vec::new()));
        // Tokens summed across the parallel sessions (debug log).
        let wave2_usage: std::sync::Arc<tokio::sync::Mutex<scryer_acp::Usage>> =
            std::sync::Arc::new(tokio::sync::Mutex::new(scryer_acp::Usage::default()));
        // +1 permit: the system-level semantic session runs beside the
        // container pool without stealing a container slot.
        let sem = std::sync::Arc::new(tokio::sync::Semaphore::new(wave2_pool + 1));

        let mut handles = Vec::with_capacity(jobs.len() + 1);

        // The system-level semantic session — persons, externals, system &
        // container responsibilities, refined names, link labels — runs
        // concurrently with every container session.
        {
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
            let system_id = system_id.clone();
            handles.push(tokio::spawn(async move {
                let _permit = match sem.acquire().await {
                    Ok(p) => p,
                    Err(_) => return,
                };
                if cancelled.load(std::sync::atomic::Ordering::SeqCst) {
                    return;
                }
                {
                    let mut a = active.lock().await;
                    a.insert(system_id.clone());
                    let _ = app.emit(
                        "build-active-node",
                        a.iter().cloned().collect::<Vec<String>>(),
                    );
                }
                let s_start = std::time::Instant::now();
                let prompt = scryer_acp::prompt::enrich_system_prompt(
                    &cwd,
                    &system_id,
                    &structure_json,
                );
                let outcome = run_wave(
                    &runtime, &agent_binary, &mode, &cwd, &model_name, &effort, &mcp_binary,
                    prompt, &app,
                )
                .await;
                {
                    let mut a = active.lock().await;
                    a.remove(&system_id);
                    let _ = app.emit(
                        "build-active-node",
                        a.iter().cloned().collect::<Vec<String>>(),
                    );
                }
                match outcome {
                    Ok((WaveOutcome::Completed, usage)) => {
                        wave2_usage.lock().await.add(&usage);
                        eprintln!(
                            "[build] system semantic pass: {:.1}s, {}",
                            s_start.elapsed().as_secs_f64(),
                            fmt_usage(&usage),
                        );
                    }
                    Ok((WaveOutcome::Cancelled, _)) => {
                        cancelled.store(true, std::sync::atomic::Ordering::SeqCst);
                    }
                    Err(e) => failures
                        .lock()
                        .await
                        .push(format!("System-level semantic pass failed: {e}")),
                }
            }));
        }
        for job in jobs {
            let permit_count = wave2_job_permits(&job, wave2_pool);
            let Wave2Job {
                id,
                name,
                evidence_json,
                work_units,
                payload_bytes,
            } = job;
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
                // Large prompts consume more pool capacity so one oversized
                // container cannot run beside several other expensive sessions.
                let _permit = match sem.acquire_many(permit_count).await {
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
                    let w2 = scryer_acp::prompt::build_container_prompt(
                        &cwd,
                        &name,
                        &id,
                        &evidence_json,
                    );
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
                            "[build] Wave 2 container '{name}': {:.1}s, {} (work {}, payload {} bytes)",
                            c_start.elapsed().as_secs_f64(),
                            fmt_usage(&usage),
                            work_units,
                            payload_bytes,
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

        let failed_jobs = failures.lock().await.clone();
        if !failed_jobs.is_empty() {
            let _ = app.emit(
                "agent-event",
                &scryer_acp::AgentEvent::Failed {
                    error: format!(
                        "{} container modeling job(s) failed: {}",
                        failed_jobs.len(),
                        failed_jobs.join(" | ")
                    ),
                },
            );
            return;
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

        // The assembled model lives in the PLANNED draft: container commits land
        // their subtrees there and the system-level session writes its enrichment
        // (persons, externals, system responsibilities, labels) there ONLY. The
        // committed layer has every container's components but not that
        // enrichment, so the planned draft is the full picture to validate,
        // repair, and ultimately fold back into the committed model below.
        let mut completed_model = match scryer_core::read_planned_at(&model_ref) {
            Ok(model) => model,
            Err(e) => {
                let _ = app.emit(
                    "agent-event",
                    &scryer_acp::AgentEvent::Failed {
                        error: format!("Could not read the completed model: {e}"),
                    },
                );
                return;
            }
        };
        let validate_completed = |model: &scryer_core::ScryModel| {
            let mut warnings = scryer_core::validate::validate(model);
            warnings.extend(scryer_core::validate::validate_coverage(
                model,
                std::path::Path::new(&cwd),
            ));
            warnings
        };
        // A code-level "appears disconnected" warning is NOT worth a repair
        // session: the deterministic dependency graph is legitimately sparse
        // (data types, UI leaves, entry points connect to nothing), so firing an
        // agent to invent links between them is pure cost for no signal. The
        // validator still reports them for the canvas/SyncBar; the BUILD just
        // doesn't repair them. Everything else (bad parents, real cross-level
        // link violations, coverage gaps) still gates.
        let is_sparse_code_disconnect = |w: &str| {
            w.contains("disconnected") && (w.contains("(symbol)") || w.contains("(component)"))
        };
        let all_warnings = validate_completed(&completed_model);
        let deferred = all_warnings.iter().filter(|w| is_sparse_code_disconnect(w)).count();
        if deferred > 0 {
            emit_msg(format!(
                "ℹ {deferred} code-level node(s) have no modeled relationship — left as-is (not a build error)."
            ));
        }
        let mut warnings: Vec<String> = all_warnings
            .into_iter()
            .filter(|w| !is_sparse_code_disconnect(w))
            .collect();
        if !warnings.is_empty() {
            emit_msg(format!(
                "▶ Repairing {} model validation issue(s)…",
                warnings.len()
            ));
            let warnings_json =
                serde_json::to_string(&warnings).unwrap_or_else(|_| "[]".to_string());
            let repair_prompt = scryer_acp::prompt::repair_model_prompt(&cwd, &warnings_json);
            match run_wave(
                &runtime,
                &agent_binary,
                &mode,
                &cwd,
                &model_name,
                &effort,
                &mcp_binary,
                repair_prompt,
                &app,
            )
            .await
            {
                Ok((WaveOutcome::Completed, usage)) => total_usage.add(&usage),
                Ok((WaveOutcome::Cancelled, _)) => {
                    let _ = app.emit("agent-event", &scryer_acp::AgentEvent::Cancelled);
                    return;
                }
                Err(e) => {
                    let _ = app.emit(
                        "agent-event",
                        &scryer_acp::AgentEvent::Failed {
                            error: format!("Model validation repair failed: {e}"),
                        },
                    );
                    return;
                }
            }

            // The repair session also authors into the planned draft (its
            // add_links/update_nodes write planned only), so re-read it there.
            completed_model = match scryer_core::read_planned_at(&model_ref) {
                Ok(model) => model,
                Err(e) => {
                    let _ = app.emit(
                        "agent-event",
                        &scryer_acp::AgentEvent::Failed {
                            error: format!("Could not read the repaired model: {e}"),
                        },
                    );
                    return;
                }
            };
            warnings = validate_completed(&completed_model)
                .into_iter()
                .filter(|w| !is_sparse_code_disconnect(w))
                .collect();
            if !warnings.is_empty() {
                let _ = app.emit(
                    "agent-event",
                    &scryer_acp::AgentEvent::Failed {
                        error: format!(
                            "Model remains invalid after repair: {}",
                            warnings.join(" | ")
                        ),
                    },
                );
                return;
            }
        }
        // Fold the assembled draft into the committed model: a from-code build is
        // extracted truth, so model and planned end equal and no spurious pending
        // plan remains. This is what lands the system-level enrichment (and any
        // repair-session edits, which were authored into the planned draft) into
        // the committed model the wiki reads.
        if let Err(e) = scryer_core::write_model_at(&model_ref, &completed_model) {
            let _ = app.emit(
                "agent-event",
                &scryer_acp::AgentEvent::Failed {
                    error: format!("Could not fold the completed model into the committed layer: {e}"),
                },
            );
            return;
        }
        if let Err(e) = scryer_core::save_baseline_at(&model_ref, &completed_model) {
            emit_msg(format!("⚠ Could not save the final model baseline: {e}"));
        }

        // Anchor the reconcile point so the first drift check only examines
        // changes made AFTER the build, not the whole repo — and fingerprint
        // every anchor so the check is content-addressed, not git-dependent.
        let _ = scryer_core::write_sync_state(
            &model_ref,
            &scryer_core::drift::SyncState {
                reconciled_at: scryer_core::drift::now_secs(),
                commit: scryer_core::drift::head_commit(std::path::Path::new(&cwd)), ..Default::default() },
        );
        if let Err(e) = scryer_extract::anchors::write_baseline(&model_ref) {
            emit_msg(format!("⚠ Could not fingerprint anchors: {e}"));
        }

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
                commit: scryer_core::drift::head_commit(project), ..Default::default() },
        );
        let _ = scryer_extract::anchors::write_baseline(&model_ref);
        return Ok(Vec::new());
    }

    let sync = scryer_core::read_sync_state(&model_ref);
    Ok(scryer_core::drift::drifted_scopes(&model, project, &sync))
}

/// Everything the observability surfaces read, in one deterministic pass — no
/// agent involved, no git. Composes the per-node health rollup (computed
/// discharge + anchor coverage + boundary darkness), the anchor fingerprint
/// check (changed/broken anchors, with moved-but-unchanged symbols silently
/// re-anchored as a side effect), and the link evidence (declared-link audit +
/// unmodeled candidates). Runs the extractor, so it also refreshes the
/// `.build_edges.json` cache the MCP commit tool reads.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ModelHealthReport {
    health: scryer_core::health::ModelHealth,
    /// Anchors whose code changed/broke since the last reconcile.
    anchors: Vec<scryer_extract::anchors::AnchorObservation>,
    /// Anchors silently healed this pass (symbol moved, content unchanged).
    reanchored: usize,
    derived: scryer_core::build_edges::DerivedGraph,
}

#[tauri::command]
async fn get_model_health(cwd: String) -> Result<ModelHealthReport, String> {
    // The extractor parses the whole repo (seconds on a big project) — keep it
    // off the IPC thread so the UI stays responsive while the report computes.
    tauri::async_runtime::spawn_blocking(move || {
        let project = std::path::Path::new(&cwd);
        let model_ref = scryer_core::ModelRef::ProjectLocal(project.to_path_buf());

        // Check (and self-heal) anchors first: re-anchoring may update sourceMap
        // line ranges, and the health/evidence below should see the healed model.
        let check = scryer_extract::anchors::check_anchors(&model_ref)?;
        let model = scryer_core::read_model_at(&model_ref)?;

        let (ctx, _) = scryer_extract::extract_context_with_stats(project)?;
        let files: std::collections::BTreeSet<String> =
            ctx.files.iter().map(|f| f.rel_path.clone()).collect();
        let edges = scryer_core::build_edges::BuildEdges {
            symbol_edges: ctx
                .symbol_edges
                .iter()
                .map(|e| scryer_core::build_edges::CachedEdge {
                    src: e.src.clone(),
                    dst: e.dst.clone(),
                })
                .collect(),
        };
        // Keep the cross-process cache fresh for the MCP commit tool. Best-effort.
        let _ = scryer_core::build_edges::write_build_edges(project, &edges);

        Ok(ModelHealthReport {
            health: scryer_core::health::compute_health(&model, Some(&files)),
            anchors: check.observations,
            reanchored: check.reanchored,
            derived: scryer_core::build_edges::derive_graph(&model, &edges),
        })
    })
    .await
    .map_err(|e| format!("health task failed: {e}"))?
}

/// Dismiss the current drift nudge without running a semantic check: advance the
/// reconcile anchor to now (the same write `start_drift_check` does on
/// completion). The user is asserting "I've looked, these changes are fine" —
/// the changed scopes stop surfacing until code changes again. The cheap
/// counterpart to running the agent over them.
#[tauri::command]
fn reconcile_drift(cwd: String) -> Result<(), String> {
    let project = std::path::Path::new(&cwd);
    let model_ref = scryer_core::ModelRef::ProjectLocal(project.to_path_buf());
    scryer_core::write_sync_state(
        &model_ref,
        &scryer_core::drift::SyncState {
            reconciled_at: scryer_core::drift::now_secs(),
            commit: scryer_core::drift::head_commit(project), ..Default::default() },
    )?;
    // Re-fingerprint: "reconciled" means the anchors as they stand are the truth.
    scryer_extract::anchors::write_baseline(&model_ref).map(|_| ())
}

/// Reconcile drift for a single node and its whole subtree, without moving the
/// project-wide anchor. Records a per-node anchor (`now` / HEAD) for the node and
/// every descendant, so their boundaries' changes stop reading as drift while the
/// rest of the model keeps whatever drift it had. The user's "I looked, this part
/// is fine" verdict, scoped to what they were looking at.
#[tauri::command]
fn reconcile_drift_node(cwd: String, node_id: String) -> Result<(), String> {
    let project = std::path::Path::new(&cwd);
    let model_ref = scryer_core::ModelRef::ProjectLocal(project.to_path_buf());
    let model = scryer_core::read_model_at(&model_ref).map_err(|e| e.to_string())?;
    let mut sync = scryer_core::read_sync_state(&model_ref);
    let anchor = scryer_core::drift::NodeAnchor {
        reconciled_at: scryer_core::drift::now_secs(),
        commit: scryer_core::drift::head_commit(project),
    };
    for id in scryer_core::drift::subtree_ids(&model, &node_id) {
        sync.nodes.insert(id, anchor.clone());
    }
    scryer_core::write_sync_state(&model_ref, &sync)
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

    let (ctx, extraction) = scryer_extract::extract_context_with_stats(project)?;
    eprintln!(
        "[drift] extraction: {} source files, {} parsed, {} cache hits",
        extraction.source_files, extraction.parsed_files, extraction.cache_hits,
    );

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
                    commit: scryer_core::drift::head_commit(std::path::Path::new(&cwd)), ..Default::default() },
            );
            let _ = scryer_extract::anchors::write_baseline(&model_ref);
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
            app.manage(PreviewState(tokio::sync::Mutex::new(None)));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            watch_project,
            is_codebase,
            is_legacy_model,
            read_model,
            write_model,
            read_planned,
            write_planned,
            read_history,
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
            ensure_preview_server,
            start_preview_fixture_session,
            start_visual_variation_session,
            accept_visual_variation,
            discard_visual_variations,
            start_model_build,
            start_drift_check,
            get_drift_status,
            get_model_health,
            reconcile_drift,
            reconcile_drift_node,
            cancel_agent_session,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
