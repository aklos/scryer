use std::path::{Path, PathBuf};

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

/// Server-wide allow entry. Claude Code treats a bare `mcp__<server>` as
/// "auto-approve every tool from that server", so this one string covers all
/// scryer tools — reads and model writes alike, plus any added later. Safe
/// because scryer tools only ever mutate the git-tracked model under `.scryer/`
/// (reviewable in scryer's own diff), never source, the shell, or the network.
const SCRYER_MCP_ALLOW: &str = "mcp__scryer";

/// Check if Claude Code has auto-approved scryer tools in project settings.
fn check_claude_approved(project_path: &str) -> bool {
    // Check both settings.local.json and settings.json
    for filename in &["settings.local.json", "settings.json"] {
        let path = PathBuf::from(project_path).join(".claude").join(filename);
        if let Ok(contents) = std::fs::read_to_string(&path) {
            if let Ok(root) = serde_json::from_str::<serde_json::Value>(&contents) {
                if let Some(allow) = root.pointer("/permissions/allow").and_then(|v| v.as_array()) {
                    if allow.iter().any(|v| v.as_str() == Some(SCRYER_MCP_ALLOW)) {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// The Claude Code hook events scryer registers, with the matcher each needs.
/// One client command serves all of them (it dispatches on the event JSON).
const SCRYER_HOOK_EVENTS: &[(&str, Option<&str>, u64)] = &[
    ("SessionStart", None, 10),
    ("PostToolUse", Some("Read"), 10),
    ("PostToolUse", Some("Edit|Write|NotebookEdit"), 10),
    ("Stop", None, 15),
];

/// The Codex hook events, served by the same client command — Codex's hook
/// payloads use the same field names as Claude Code's. Reads fire no hooks
/// there, so the intent overlay rides PreToolUse on the patch instead; the
/// matcher covers both the native apply_patch tool and the Bash heredoc
/// route (the client no-ops on Bash commands with no patch envelope).
/// Timeouts stay explicit: Codex's default is 600 s.
const SCRYER_CODEX_HOOK_EVENTS: &[(&str, Option<&str>, u64)] = &[
    ("SessionStart", None, 10),
    ("PreToolUse", Some("apply_patch|Bash"), 10),
    ("PostToolUse", Some("apply_patch|Bash"), 10),
    ("Stop", None, 15),
];

/// Does this hook entry belong to scryer? Identified by the command invoking
/// the scryer-mcp binary's `hook` subcommand — the marker `install` writes.
fn is_scryer_hook_entry(entry: &serde_json::Value) -> bool {
    entry["hooks"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|h| h["command"].as_str())
        .any(|c| c.contains("scryer-mcp") && c.trim_end().ends_with(" hook"))
}

/// Check if Claude Code has scryer's session hooks installed for the project.
fn check_claude_hooks(project_path: &str) -> bool {
    for filename in &["settings.local.json", "settings.json"] {
        let path = PathBuf::from(project_path).join(".claude").join(filename);
        if let Ok(contents) = std::fs::read_to_string(&path) {
            if let Ok(root) = serde_json::from_str::<serde_json::Value>(&contents) {
                let installed = root
                    .pointer("/hooks/SessionStart")
                    .and_then(|v| v.as_array())
                    .is_some_and(|entries| entries.iter().any(is_scryer_hook_entry));
                if installed {
                    return true;
                }
            }
        }
    }
    false
}

/// Is this `statusLine` entry ours? Same marker as the CLI's `install_statusline`
/// (mirrored here, like `is_scryer_hook_entry`): the command invokes the
/// scryer-mcp binary's `statusline` subcommand.
fn is_scryer_statusline(entry: &serde_json::Value) -> bool {
    entry["command"]
        .as_str()
        .is_some_and(|c| c.contains("scryer-mcp") && c.trim_end().ends_with(" statusline"))
}

/// The project's Claude Code `statusLine` state as `(ours, foreign)`. Unlike
/// hooks (a merging list), `statusLine` is a SINGLE slot — a whole-line
/// replacement — so a foreign entry is never clobbered: `foreign` lets the UI
/// surface it instead of offering an install that would overwrite it.
fn check_claude_statusline(project_path: &str) -> (bool, bool) {
    for filename in &["settings.local.json", "settings.json"] {
        let path = PathBuf::from(project_path).join(".claude").join(filename);
        if let Ok(contents) = std::fs::read_to_string(&path) {
            if let Ok(root) = serde_json::from_str::<serde_json::Value>(&contents) {
                match root.get("statusLine") {
                    Some(entry) if entry.is_null() => continue,
                    Some(entry) if is_scryer_statusline(entry) => return (true, false),
                    Some(_) => return (false, true),
                    None => continue,
                }
            }
        }
    }
    (false, false)
}

/// Check if Codex has scryer's session hooks installed for the project.
fn check_codex_hooks(project_path: &str) -> bool {
    let path = PathBuf::from(project_path).join(".codex").join("hooks.json");
    if let Ok(contents) = std::fs::read_to_string(&path) {
        if let Ok(root) = serde_json::from_str::<serde_json::Value>(&contents) {
            return root
                .pointer("/hooks/SessionStart")
                .and_then(|v| v.as_array())
                .is_some_and(|entries| entries.iter().any(is_scryer_hook_entry));
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

/// Check if a project has `.cursor/mcp.json` with a scryer MCP entry.
fn check_cursor_mcp(project_path: &str) -> bool {
    let path = PathBuf::from(project_path).join(".cursor").join("mcp.json");
    if let Ok(contents) = std::fs::read_to_string(&path) {
        if let Ok(root) = serde_json::from_str::<serde_json::Value>(&contents) {
            return root.pointer("/mcpServers/scryer").is_some();
        }
    }
    false
}

fn check_cursor_approved(project_path: &str) -> bool {
    scryer_core::cursor_agent::cursor_mcp_permission_enabled(Path::new(project_path))
}

fn has_cursor_cli() -> bool {
    scryer_core::cursor_agent::find_cursor_agent().is_some()
}

fn check_cursor_authenticated() -> bool {
    scryer_core::cursor_agent::find_cursor_agent()
        .is_some_and(|p| scryer_core::cursor_agent::cursor_agent_authenticated(&p))
}

#[tauri::command]
pub(crate) fn detect_ai_tools(project_path: Option<String>) -> serde_json::Value {
    let has_claude = which::which("claude").is_ok();
    let has_codex = which::which("codex").is_ok();
    let has_cursor = has_cursor_cli();
    let cursor_authenticated = has_cursor && check_cursor_authenticated();

    let claude_mcp = project_path.as_deref().map(check_mcp_json).unwrap_or(false);
    let codex_mcp = project_path.as_deref().map(check_codex_toml).unwrap_or(false);
    let cursor_mcp = project_path.as_deref().map(check_cursor_mcp).unwrap_or(false);
    let cursor_approved = project_path.as_deref().map(check_cursor_approved).unwrap_or(false);
    let claude_approved = project_path.as_deref().map(check_claude_approved).unwrap_or(false);
    let claude_hooks = project_path.as_deref().map(check_claude_hooks).unwrap_or(false);
    let codex_hooks = project_path.as_deref().map(check_codex_hooks).unwrap_or(false);
    let (claude_statusline, claude_statusline_foreign) =
        project_path.as_deref().map(check_claude_statusline).unwrap_or((false, false));

    serde_json::json!({
        "claude": has_claude,
        "codex": has_codex,
        "cursor": has_cursor,
        "cursorAuthenticated": cursor_authenticated,
        "claudeMcpEnabled": claude_mcp,
        "codexMcpEnabled": codex_mcp,
        "cursorMcpEnabled": cursor_mcp,
        "cursorApproved": cursor_approved,
        "claudeApproved": claude_approved,
        "claudeHooksEnabled": claude_hooks,
        "codexHooksEnabled": codex_hooks,
        "claudeStatuslineEnabled": claude_statusline,
        "claudeStatuslineForeign": claude_statusline_foreign,
    })
}

/// Find the scryer-mcp binary path by checking common locations.
pub(crate) fn find_scryer_mcp() -> Option<String> {
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
pub(crate) fn setup_mcp_integration(
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
        "mcp_cursor" => {
            let binary_path = find_scryer_mcp()
                .ok_or("scryer-mcp binary not found")?;

            let cursor_dir = PathBuf::from(&project_path).join(".cursor");
            scryer_core::cursor_agent::install_cursor_mcp_permission(Path::new(&project_path))?;
            let mcp_path = cursor_dir.join("mcp.json");
            let mut mcp_root: serde_json::Value = if mcp_path.exists() {
                let contents = std::fs::read_to_string(&mcp_path).map_err(|e| e.to_string())?;
                serde_json::from_str(&contents).map_err(|e| {
                    format!(
                        "{} is not valid JSON ({e}); refusing to overwrite it",
                        mcp_path.display()
                    )
                })?
            } else {
                serde_json::json!({})
            };

            if !mcp_root.get("mcpServers").is_some_and(|v| v.is_object()) {
                mcp_root["mcpServers"] = serde_json::json!({});
            }
            mcp_root["mcpServers"]["scryer"] = serde_json::json!({
                "command": binary_path,
                "args": [],
            });

            std::fs::create_dir_all(&cursor_dir).map_err(|e| e.to_string())?;
            std::fs::write(&mcp_path, serde_json::to_string_pretty(&mcp_root).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;

            return Ok(mcp_path.to_string_lossy().to_string());
        }
        "claude_approve" => {
            let claude_dir = PathBuf::from(&project_path).join(".claude");
            let settings_path = claude_dir.join("settings.local.json");

            let mut root: serde_json::Value = if settings_path.exists() {
                let contents = std::fs::read_to_string(&settings_path).map_err(|e| e.to_string())?;
                serde_json::from_str(&contents).map_err(|e| {
                    format!(
                        "{} is not valid JSON ({e}); refusing to overwrite it — fix the file and retry.",
                        settings_path.display()
                    )
                })?
            } else {
                serde_json::json!({})
            };

            if !root.pointer("/permissions/allow").is_some_and(|v| v.is_array()) {
                root["permissions"] = serde_json::json!({ "allow": [] });
            }

            let allow = root.pointer_mut("/permissions/allow").unwrap().as_array_mut().unwrap();
            if !allow.iter().any(|v| v.as_str() == Some(SCRYER_MCP_ALLOW)) {
                allow.push(serde_json::json!(SCRYER_MCP_ALLOW));
            }

            std::fs::create_dir_all(&claude_dir).map_err(|e| e.to_string())?;
            std::fs::write(&settings_path, serde_json::to_string_pretty(&root).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;

            return Ok(settings_path.to_string_lossy().to_string());
        }
        "claude_hooks" => {
            let binary_path = find_scryer_mcp().ok_or("scryer-mcp binary not found")?;
            return write_claude_hooks(&project_path, &binary_path);
        }
        "codex_hooks" => {
            let binary_path = find_scryer_mcp().ok_or("scryer-mcp binary not found")?;
            return write_codex_hooks(&project_path, &binary_path);
        }
        "claude_statusline" => {
            let binary_path = find_scryer_mcp().ok_or("scryer-mcp binary not found")?;
            return write_claude_statusline(&project_path, &binary_path);
        }
        _ => Err(format!("Unknown action: {}", action)),
    }
}

/// Explicit, per-project opt-in: write scryer's session-hook registrations
/// into the personal settings file. The registered command no-ops in
/// milliseconds unless the app has this project open, so installed hooks
/// impose nothing on sessions where the user leaves Scryer closed.
fn write_claude_hooks(project_path: &str, binary_path: &str) -> Result<String, String> {
    let claude_dir = PathBuf::from(project_path).join(".claude");
    write_scryer_hooks(
        &claude_dir,
        &claude_dir.join("settings.local.json"),
        SCRYER_HOOK_EVENTS,
        binary_path,
    )
}

/// Register scryer's status one-liner as this project's Claude Code statusLine,
/// in the personal settings file (same conventions as the hook install: absolute
/// binary path, refuse to overwrite invalid JSON). A separate opt-in from the
/// session hooks because it's the only surface that survives Scryer being closed
/// — `scryer-mcp statusline` reads the model straight off disk. Mirrors
/// `install_statusline` in the scryer-mcp crate. `statusLine` is a SINGLE slot,
/// so a foreign entry is never clobbered: the write errors and the caller (which
/// detected the foreign line via `check_claude_statusline`) surfaces it instead.
fn write_claude_statusline(project_path: &str, binary_path: &str) -> Result<String, String> {
    let claude_dir = PathBuf::from(project_path).join(".claude");
    let settings_path = claude_dir.join("settings.local.json");

    let mut root: serde_json::Value = if settings_path.exists() {
        let contents = std::fs::read_to_string(&settings_path).map_err(|e| e.to_string())?;
        serde_json::from_str(&contents).map_err(|e| {
            format!(
                "{} is not valid JSON ({e}); refusing to overwrite it — fix the file and retry.",
                settings_path.display()
            )
        })?
    } else {
        serde_json::json!({})
    };

    if root
        .get("statusLine")
        .is_some_and(|e| !e.is_null() && !is_scryer_statusline(e))
    {
        return Err(format!(
            "A status line is already configured in {} — left untouched.",
            settings_path.display()
        ));
    }

    root["statusLine"] = serde_json::json!({
        "type": "command",
        "command": format!("\"{binary_path}\" statusline"),
    });

    std::fs::create_dir_all(&claude_dir).map_err(|e| e.to_string())?;
    std::fs::write(
        &settings_path,
        serde_json::to_string_pretty(&root).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;

    Ok(settings_path.to_string_lossy().to_string())
}

/// The same opt-in for Codex: write the registrations into the project's
/// `.codex/hooks.json` (Codex merges it with any user-level hooks, and loads
/// it once the `.codex/` layer is trusted). The registered command is the
/// same `… hook` client, so the inert-while-Scryer-is-closed economics hold.
fn write_codex_hooks(project_path: &str, binary_path: &str) -> Result<String, String> {
    let codex_dir = PathBuf::from(project_path).join(".codex");
    write_scryer_hooks(
        &codex_dir,
        &codex_dir.join("hooks.json"),
        SCRYER_CODEX_HOOK_EVENTS,
        binary_path,
    )
}

/// Idempotent hook install into one file's `{"hooks": {...}}` block — both
/// harnesses use the same entry schema. Two passes because an event may get
/// two scryer entries: first strip every prior scryer entry per event (they
/// all share the `… hook` command marker; foreign hooks are kept), then
/// append the current set.
fn write_scryer_hooks(
    dir: &Path,
    file_path: &Path,
    events: &[(&str, Option<&str>, u64)],
    binary_path: &str,
) -> Result<String, String> {
    let command = format!("\"{}\" hook", binary_path);

    let mut root: serde_json::Value = if file_path.exists() {
        let contents = std::fs::read_to_string(file_path).map_err(|e| e.to_string())?;
        serde_json::from_str(&contents).map_err(|e| {
            format!(
                "{} is not valid JSON ({e}); refusing to overwrite it — fix the file and retry.",
                file_path.display()
            )
        })?
    } else {
        serde_json::json!({})
    };

    if !root.get("hooks").is_some_and(|v| v.is_object()) {
        root["hooks"] = serde_json::json!({});
    }
    for (event, _, _) in events {
        let entries = root["hooks"]
            .as_object_mut()
            .unwrap()
            .entry(event.to_string())
            .or_insert_with(|| serde_json::json!([]));
        if !entries.is_array() {
            *entries = serde_json::json!([]);
        }
        entries.as_array_mut().unwrap().retain(|e| !is_scryer_hook_entry(e));
    }
    for (event, matcher, timeout) in events {
        let mut entry = serde_json::json!({
            "hooks": [{ "type": "command", "command": command, "timeout": timeout }],
        });
        if let Some(m) = matcher {
            entry["matcher"] = serde_json::json!(m);
        }
        root["hooks"][event].as_array_mut().unwrap().push(entry);
    }

    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    std::fs::write(
        file_path,
        serde_json::to_string_pretty(&root).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;

    Ok(file_path.to_string_lossy().to_string())
}

#[cfg(test)]
mod hook_install_tests {
    use super::*;

    /// Install twice into a settings file that already has a foreign hook:
    /// the foreign entry survives, scryer entries don't duplicate, and both
    /// PostToolUse matchers (Read overlay + Edit touch) are present.
    #[test]
    fn hook_install_is_idempotent_and_preserves_foreign_hooks() {
        let dir = tempfile::tempdir().unwrap();
        let claude_dir = dir.path().join(".claude");
        std::fs::create_dir_all(&claude_dir).unwrap();
        std::fs::write(
            claude_dir.join("settings.local.json"),
            serde_json::json!({
                "permissions": { "allow": ["mcp__scryer"] },
                "hooks": {
                    "PostToolUse": [
                        { "matcher": "Bash", "hooks": [{ "type": "command", "command": "my-linter" }] }
                    ]
                }
            })
            .to_string(),
        )
        .unwrap();

        let project = dir.path().to_string_lossy().to_string();
        assert!(!check_claude_hooks(&project));
        write_claude_hooks(&project, "/opt/scryer/scryer-mcp").unwrap();
        write_claude_hooks(&project, "/opt/scryer/scryer-mcp").unwrap();
        assert!(check_claude_hooks(&project));

        let root: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(claude_dir.join("settings.local.json")).unwrap(),
        )
        .unwrap();
        // Untouched sections and foreign hooks survive.
        assert_eq!(root["permissions"]["allow"][0], "mcp__scryer");
        let post = root["hooks"]["PostToolUse"].as_array().unwrap();
        assert!(post.iter().any(|e| e["matcher"] == "Bash"), "foreign hook kept");
        // Exactly one scryer entry per registration, even after re-install.
        let scryer_post: Vec<_> = post.iter().filter(|e| is_scryer_hook_entry(e)).collect();
        assert_eq!(scryer_post.len(), 2, "Read overlay + Edit touch: {post:?}");
        assert!(scryer_post.iter().any(|e| e["matcher"] == "Read"));
        assert!(scryer_post.iter().any(|e| e["matcher"] == "Edit|Write|NotebookEdit"));
        assert_eq!(root["hooks"]["SessionStart"].as_array().unwrap().len(), 1);
        assert_eq!(root["hooks"]["Stop"].as_array().unwrap().len(), 1);
    }

    /// A malformed settings file must ERROR the install, not get silently
    /// replaced — a stray comma in the user's config must never cost them their
    /// permissions and foreign hooks.
    #[test]
    fn hook_install_refuses_to_overwrite_malformed_settings() {
        let dir = tempfile::tempdir().unwrap();
        let claude_dir = dir.path().join(".claude");
        std::fs::create_dir_all(&claude_dir).unwrap();
        let settings = claude_dir.join("settings.local.json");
        let original = r#"{ "permissions": { "allow": ["mcp__scryer",] } }"#; // trailing comma
        std::fs::write(&settings, original).unwrap();

        let project = dir.path().to_string_lossy().to_string();
        let err = write_claude_hooks(&project, "/opt/scryer/scryer-mcp").unwrap_err();
        assert!(err.contains("not valid JSON"), "{err}");
        // The user's file is left exactly as it was.
        assert_eq!(std::fs::read_to_string(&settings).unwrap(), original);
    }

    /// The statusline install writes our single-slot `statusLine` entry, is
    /// idempotent (a re-install is our own entry refreshed, not a duplicate),
    /// and detection reports it as ours — not foreign.
    #[test]
    fn statusline_install_is_idempotent_and_detected_as_ours() {
        let dir = tempfile::tempdir().unwrap();
        let claude_dir = dir.path().join(".claude");
        std::fs::create_dir_all(&claude_dir).unwrap();
        std::fs::write(
            claude_dir.join("settings.local.json"),
            serde_json::json!({ "permissions": { "allow": ["mcp__scryer"] } }).to_string(),
        )
        .unwrap();

        let project = dir.path().to_string_lossy().to_string();
        assert_eq!(check_claude_statusline(&project), (false, false));
        write_claude_statusline(&project, "/opt/scryer/scryer-mcp").unwrap();
        write_claude_statusline(&project, "/opt/scryer/scryer-mcp").unwrap();
        assert_eq!(check_claude_statusline(&project), (true, false));

        let root: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(claude_dir.join("settings.local.json")).unwrap(),
        )
        .unwrap();
        // Untouched sections survive; the entry carries the ` statusline` marker.
        assert_eq!(root["permissions"]["allow"][0], "mcp__scryer");
        assert!(is_scryer_statusline(&root["statusLine"]));
        assert_eq!(root["statusLine"]["type"], "command");
    }

    /// A FOREIGN statusLine holds the single slot: detection flags it foreign,
    /// and the install refuses to clobber it (the user's own line is preserved).
    #[test]
    fn statusline_install_refuses_to_clobber_a_foreign_line() {
        let dir = tempfile::tempdir().unwrap();
        let claude_dir = dir.path().join(".claude");
        std::fs::create_dir_all(&claude_dir).unwrap();
        let settings = claude_dir.join("settings.local.json");
        let original =
            serde_json::json!({ "statusLine": { "type": "command", "command": "my-powerline" } })
                .to_string();
        std::fs::write(&settings, &original).unwrap();

        let project = dir.path().to_string_lossy().to_string();
        assert_eq!(check_claude_statusline(&project), (false, true));
        let err = write_claude_statusline(&project, "/opt/scryer/scryer-mcp").unwrap_err();
        assert!(err.contains("already configured"), "{err}");
        // The user's line is left exactly as it was.
        assert_eq!(std::fs::read_to_string(&settings).unwrap(), original);
    }

    /// A malformed settings file must ERROR the install, not get silently
    /// replaced — same guarantee as the hook install.
    #[test]
    fn statusline_install_refuses_to_overwrite_malformed_settings() {
        let dir = tempfile::tempdir().unwrap();
        let claude_dir = dir.path().join(".claude");
        std::fs::create_dir_all(&claude_dir).unwrap();
        let settings = claude_dir.join("settings.local.json");
        let original = r#"{ "permissions": { "allow": ["mcp__scryer",] } }"#; // trailing comma
        std::fs::write(&settings, original).unwrap();

        let project = dir.path().to_string_lossy().to_string();
        let err = write_claude_statusline(&project, "/opt/scryer/scryer-mcp").unwrap_err();
        assert!(err.contains("not valid JSON"), "{err}");
        assert_eq!(std::fs::read_to_string(&settings).unwrap(), original);
    }

    /// The Codex install writes `.codex/hooks.json` with the Codex event set —
    /// PreToolUse carries the overlay there since reads fire no hooks — and is
    /// just as idempotent and foreign-preserving as the Claude one.
    #[test]
    fn codex_hook_install_is_idempotent_and_preserves_foreign_hooks() {
        let dir = tempfile::tempdir().unwrap();
        let codex_dir = dir.path().join(".codex");
        std::fs::create_dir_all(&codex_dir).unwrap();
        std::fs::write(
            codex_dir.join("hooks.json"),
            serde_json::json!({
                "hooks": {
                    "PreToolUse": [
                        { "matcher": "Bash", "hooks": [{ "type": "command", "command": "my-policy" }] }
                    ]
                }
            })
            .to_string(),
        )
        .unwrap();

        let project = dir.path().to_string_lossy().to_string();
        assert!(!check_codex_hooks(&project));
        write_codex_hooks(&project, "/opt/scryer/scryer-mcp").unwrap();
        write_codex_hooks(&project, "/opt/scryer/scryer-mcp").unwrap();
        assert!(check_codex_hooks(&project));

        let root: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(codex_dir.join("hooks.json")).unwrap(),
        )
        .unwrap();
        let pre = root["hooks"]["PreToolUse"].as_array().unwrap();
        assert!(pre.iter().any(|e| e["matcher"] == "Bash"), "foreign hook kept");
        let scryer_pre: Vec<_> = pre.iter().filter(|e| is_scryer_hook_entry(e)).collect();
        assert_eq!(scryer_pre.len(), 1, "no duplicates after re-install: {pre:?}");
        assert_eq!(scryer_pre[0]["matcher"], "apply_patch|Bash");
        assert_eq!(
            root["hooks"]["PostToolUse"].as_array().unwrap().len(),
            1,
            "Codex set has ONE PostToolUse entry (no Read overlay there)"
        );
        assert_eq!(root["hooks"]["SessionStart"].as_array().unwrap().len(), 1);
        assert_eq!(root["hooks"]["Stop"].as_array().unwrap().len(), 1);
    }
}
