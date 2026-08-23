use std::path::{Path, PathBuf};

/// Check if a project has .mcp.json with a scryer entry.
fn check_mcp_json(project_path: &str) -> bool {
    has_mcp_scryer_entry(&PathBuf::from(project_path).join(".mcp.json"))
}

/// Copilot reads the SAME `.mcp.json` Claude Code does — the one scryer already
/// writes — so its MCP setup needs no file of its own. It also accepts a
/// committed `.github/mcp.json`, which `.mcp.json` overrides; a project wired
/// up by hand there is already set up, so detection honours both and the setup
/// offer stays quiet. Omitting `tools` is fine: Copilot defaults a server to
/// all tools, and it treats `"stdio"` as an alias of its own `"local"`.
fn check_copilot_mcp(project_path: &str) -> bool {
    let root = PathBuf::from(project_path);
    has_mcp_scryer_entry(&root.join(".mcp.json"))
        || has_mcp_scryer_entry(&root.join(".github").join("mcp.json"))
}

fn has_mcp_scryer_entry(path: &Path) -> bool {
    if let Ok(contents) = std::fs::read_to_string(path) {
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

/// The Copilot CLI hook events. Copilot accepts Claude Code's PascalCase event
/// names as an explicit compatibility mode, which also switches its payloads to
/// the snake_case field names the one hook client already reads, so the events
/// line up — but the tool VOCABULARY is its own (`view` to read, `create` /
/// `edit` / `str_replace_editor` / native `apply_patch` to write), and matchers
/// test the runtime name. Reads fire here like they do on Claude Code, so the
/// overlay rides post-read rather than the edit. `bash` is deliberately absent:
/// Copilot has a native patch tool, so unlike Codex there is no heredoc route
/// worth spawning this client for on every shell command.
const SCRYER_COPILOT_HOOK_EVENTS: &[(&str, Option<&str>, u64)] = &[
    ("SessionStart", None, 10),
    ("PostToolUse", Some("view"), 10),
    ("PostToolUse", Some("create|edit|str_replace_editor|apply_patch"), 10),
    ("Stop", None, 15),
];

/// Does the command in this hook entry invoke scryer's hook client? The marker
/// `install` writes, and the only thing that identifies an entry as ours.
fn is_scryer_hook_command(command: &str) -> bool {
    command.contains("scryer-mcp")
        && command
            .trim_end()
            .trim_end_matches(" --copilot")
            .ends_with(" hook")
}

/// Does this hook entry belong to scryer? Claude Code and Codex nest the
/// commands under a `hooks` array; Copilot puts the command on the entry
/// itself. Both shapes are checked so one predicate serves every install.
fn is_scryer_hook_entry(entry: &serde_json::Value) -> bool {
    if entry["command"]
        .as_str()
        .is_some_and(is_scryer_hook_command)
    {
        return true;
    }
    entry["hooks"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|h| h["command"].as_str())
        .any(is_scryer_hook_command)
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

/// Where Copilot's hook registration goes. `.github/hooks/` is the only
/// project-scoped location Copilot actually loads: the `hooks` key in
/// `.github/copilot/settings.local.json` is documented but inert as of 1.0.61
/// (verified against real sessions), and the user-level hooks directory is
/// global to every project rather than an opt-in for this one.
///
/// It is a COMMITTED path, unlike Claude Code's `settings.local.json` — so this
/// registration is shared with the checkout, the same way `.codex/hooks.json`
/// already is. That costs teammates nothing: the registered command exits in
/// milliseconds unless the Scryer app has this project open, so a checkout
/// without Scryer — including a CI run of the Copilot cloud agent, which reads
/// exactly this directory — sees no behaviour at all.
///
/// Scryer owns this file outright (hence its own name in a directory Copilot
/// reads whole), which is what lets it be written wholesale rather than merged.
fn copilot_hooks_path(project_path: &str) -> PathBuf {
    PathBuf::from(project_path)
        .join(".github")
        .join("hooks")
        .join("scryer.json")
}

/// Check if Copilot CLI has scryer's session hooks installed for the project.
fn check_copilot_hooks(project_path: &str) -> bool {
    if let Ok(contents) = std::fs::read_to_string(copilot_hooks_path(project_path)) {
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

#[tauri::command]
pub(crate) fn detect_ai_tools(project_path: Option<String>) -> serde_json::Value {
    let has_claude = which::which("claude").is_ok();
    let has_codex = which::which("codex").is_ok();
    let has_copilot = which::which("copilot").is_ok();

    let claude_mcp = project_path.as_deref().map(check_mcp_json).unwrap_or(false);
    let codex_mcp = project_path.as_deref().map(check_codex_toml).unwrap_or(false);
    let copilot_mcp = project_path.as_deref().map(check_copilot_mcp).unwrap_or(false);
    let claude_approved = project_path.as_deref().map(check_claude_approved).unwrap_or(false);
    let claude_hooks = project_path.as_deref().map(check_claude_hooks).unwrap_or(false);
    let codex_hooks = project_path.as_deref().map(check_codex_hooks).unwrap_or(false);
    let copilot_hooks = project_path.as_deref().map(check_copilot_hooks).unwrap_or(false);
    let (claude_statusline, claude_statusline_foreign) =
        project_path.as_deref().map(check_claude_statusline).unwrap_or((false, false));

    serde_json::json!({
        "claude": has_claude,
        "codex": has_codex,
        "copilot": has_copilot,
        "claudeMcpEnabled": claude_mcp,
        "codexMcpEnabled": codex_mcp,
        "copilotMcpEnabled": copilot_mcp,
        "claudeApproved": claude_approved,
        "claudeHooksEnabled": claude_hooks,
        "codexHooksEnabled": codex_hooks,
        "copilotHooksEnabled": copilot_hooks,
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
        "copilot_hooks" => {
            let binary_path = find_scryer_mcp().ok_or("scryer-mcp binary not found")?;
            return write_copilot_hooks(&project_path, &binary_path);
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

    let mut root = read_json_or_refuse(&settings_path)?;

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

/// The same opt-in for Copilot CLI. Copilot reads every `*.json` in
/// `.github/hooks/`, so scryer takes a file of its own and writes it whole —
/// no merge pass, and nothing of anyone else's to preserve or corrupt, which is
/// the one simplification this location buys over the shared settings files the
/// other two installs have to edit in place. Re-installing is therefore
/// idempotent by construction, and a previously corrupted file is repaired
/// rather than refused (it is only ever scryer's own).
///
/// The entry schema is Copilot's: FLAT, with the command on the entry rather
/// than nested under a `hooks` array. `timeout` is written rather than
/// Copilot's own `timeoutSec` because it normalises one to the other, keeping a
/// single vocabulary across the three installs. The registered command carries
/// `--copilot` so the client knows whose tool names and reply shape to speak.
fn write_copilot_hooks(project_path: &str, binary_path: &str) -> Result<String, String> {
    let path = copilot_hooks_path(project_path);
    let dir = path
        .parent()
        .ok_or("no parent directory for the Copilot hooks file")?
        .to_path_buf();
    let command = format!("\"{binary_path}\" hook --copilot");

    let mut hooks = serde_json::Map::new();
    for (event, matcher, timeout) in SCRYER_COPILOT_HOOK_EVENTS {
        let mut entry = serde_json::json!({
            "type": "command",
            "command": command,
            "timeout": timeout,
        });
        if let Some(m) = matcher {
            entry["matcher"] = serde_json::json!(m);
        }
        hooks
            .entry(event.to_string())
            .or_insert_with(|| serde_json::json!([]))
            .as_array_mut()
            .unwrap()
            .push(entry);
    }
    let root = serde_json::json!({ "version": 1, "hooks": hooks });

    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::fs::write(
        &path,
        serde_json::to_string_pretty(&root).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;

    Ok(path.to_string_lossy().to_string())
}

/// Read a settings file we're about to merge into. A file that doesn't parse is
/// an ERROR, never a blank slate: a stray comma in the user's config must not
/// cost them everything else the file holds.
fn read_json_or_refuse(path: &Path) -> Result<serde_json::Value, String> {
    if !path.exists() {
        return Ok(serde_json::json!({}));
    }
    let contents = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    serde_json::from_str(&contents).map_err(|e| {
        format!(
            "{} is not valid JSON ({e}); refusing to overwrite it — fix the file and retry.",
            path.display()
        )
    })
}

/// Idempotent hook install into one file's `{"hooks": {...}}` block — Claude
/// Code and Codex use the same entry schema. Two passes because an event may
/// get two scryer entries: first strip every prior scryer entry per event (they
/// all share the `… hook` command marker; foreign hooks are kept), then
/// append the current set.
fn write_scryer_hooks(
    dir: &Path,
    file_path: &Path,
    events: &[(&str, Option<&str>, u64)],
    binary_path: &str,
) -> Result<String, String> {
    let command = format!("\"{}\" hook", binary_path);

    let mut root = read_json_or_refuse(file_path)?;

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

    /// The Copilot install writes its own file in `.github/hooks/` using
    /// Copilot's FLAT entry schema (command on the entry, not nested under a
    /// `hooks` array), with Copilot's own tool names in the matchers and
    /// `--copilot` on the command. Owning the file makes re-installing
    /// idempotent by construction — and repairs a corrupted one, since there is
    /// never anything of anyone else's in it to lose.
    #[test]
    fn copilot_hook_install_owns_its_file_and_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let hooks_file = dir.path().join(".github/hooks/scryer.json");
        std::fs::create_dir_all(hooks_file.parent().unwrap()).unwrap();
        std::fs::write(&hooks_file, "{ not json at all").unwrap();

        let project = dir.path().to_string_lossy().to_string();
        assert!(!check_copilot_hooks(&project));
        write_copilot_hooks(&project, "/opt/scryer/scryer-mcp").unwrap();
        write_copilot_hooks(&project, "/opt/scryer/scryer-mcp").unwrap();
        assert!(check_copilot_hooks(&project));

        let root: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&hooks_file).unwrap()).unwrap();
        assert_eq!(root["version"], 1);
        let post = root["hooks"]["PostToolUse"].as_array().unwrap();
        assert_eq!(post.len(), 2, "read overlay + write touch, no duplicates: {post:?}");
        // Copilot's own tool vocabulary, matched on the runtime names.
        assert!(post.iter().any(|e| e["matcher"] == "view"));
        assert!(post
            .iter()
            .any(|e| e["matcher"] == "create|edit|str_replace_editor|apply_patch"));
        // Flat schema, and the client is told which harness it is serving.
        assert_eq!(post[0]["type"], "command");
        assert_eq!(post[0]["command"], "\"/opt/scryer/scryer-mcp\" hook --copilot");
        assert!(post.iter().all(is_scryer_hook_entry));
        assert_eq!(root["hooks"]["SessionStart"].as_array().unwrap().len(), 1);
        assert_eq!(root["hooks"]["Stop"].as_array().unwrap().len(), 1);
    }

    /// Sibling files in `.github/hooks/` belong to other tools — Copilot loads
    /// the whole directory — so the install must never touch them.
    #[test]
    fn copilot_hook_install_leaves_sibling_hook_files_alone() {
        let dir = tempfile::tempdir().unwrap();
        let siblings = dir.path().join(".github/hooks/my-linter.json");
        std::fs::create_dir_all(siblings.parent().unwrap()).unwrap();
        let original = serde_json::json!({
            "hooks": { "PostToolUse": [{ "type": "command", "command": "my-linter" }] }
        })
        .to_string();
        std::fs::write(&siblings, &original).unwrap();

        let project = dir.path().to_string_lossy().to_string();
        write_copilot_hooks(&project, "/opt/scryer/scryer-mcp").unwrap();
        assert_eq!(std::fs::read_to_string(&siblings).unwrap(), original);
    }

    /// A Copilot entry and a Claude/Codex entry are both recognised as ours —
    /// one predicate over two schemas — while a foreign command in either shape
    /// is left alone.
    #[test]
    fn scryer_entries_are_recognised_in_both_schemas() {
        let nested = serde_json::json!({
            "matcher": "Read",
            "hooks": [{ "type": "command", "command": "\"/opt/scryer-mcp\" hook" }],
        });
        let flat = serde_json::json!({
            "type": "command",
            "matcher": "view",
            "command": "\"/opt/scryer-mcp\" hook --copilot",
        });
        assert!(is_scryer_hook_entry(&nested));
        assert!(is_scryer_hook_entry(&flat));

        // Not ours: a foreign command, and a command that merely mentions the
        // binary without being the hook client (e.g. a wrapper running
        // `scryer-mcp check`).
        assert!(!is_scryer_hook_entry(&serde_json::json!({
            "type": "command", "command": "my-linter",
        })));
        assert!(!is_scryer_hook_entry(&serde_json::json!({
            "type": "command", "command": "\"/opt/scryer-mcp\" check",
        })));
    }

    /// `.mcp.json` detection: only a real `mcpServers.scryer` entry counts —
    /// no file, a scryer-less file, or malformed JSON all read as not set up.
    #[test]
    fn mcp_json_detection_requires_a_scryer_server_entry() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().to_string_lossy().to_string();
        let mcp = dir.path().join(".mcp.json");

        assert!(!check_mcp_json(&project), "no file");
        std::fs::write(&mcp, r#"{ "mcpServers": { "other": {} } }"#).unwrap();
        assert!(!check_mcp_json(&project), "no scryer entry");
        std::fs::write(&mcp, "{ not json").unwrap();
        assert!(!check_mcp_json(&project), "malformed reads as absent");
        std::fs::write(&mcp, r#"{ "mcpServers": { "scryer": { "type": "stdio" } } }"#).unwrap();
        assert!(check_mcp_json(&project));
    }

    /// Auto-approval detection: the server-wide `mcp__scryer` allow entry is
    /// recognised in either Claude Code settings file.
    #[test]
    fn approval_detection_finds_the_allow_entry_in_either_settings_file() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().to_string_lossy().to_string();
        let claude_dir = dir.path().join(".claude");
        std::fs::create_dir_all(&claude_dir).unwrap();

        assert!(!check_claude_approved(&project));
        std::fs::write(
            claude_dir.join("settings.json"),
            r#"{ "permissions": { "allow": ["Bash(ls:*)", "mcp__scryer"] } }"#,
        )
        .unwrap();
        assert!(check_claude_approved(&project), "shared settings.json counts");

        std::fs::remove_file(claude_dir.join("settings.json")).unwrap();
        std::fs::write(
            claude_dir.join("settings.local.json"),
            r#"{ "permissions": { "allow": ["mcp__scryer"] } }"#,
        )
        .unwrap();
        assert!(check_claude_approved(&project), "local settings count too");
    }

    /// Codex config detection: only an `[mcp_servers.scryer]` table counts.
    #[test]
    fn codex_toml_detection_requires_a_scryer_mcp_entry() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().to_string_lossy().to_string();
        let codex_dir = dir.path().join(".codex");
        std::fs::create_dir_all(&codex_dir).unwrap();
        let config = codex_dir.join("config.toml");

        assert!(!check_codex_toml(&project), "no file");
        std::fs::write(&config, "[mcp_servers.other]\ncommand = \"x\"\n").unwrap();
        assert!(!check_codex_toml(&project), "no scryer entry");
        std::fs::write(&config, "[mcp_servers.scryer]\ncommand = \"/opt/scryer-mcp\"\n").unwrap();
        assert!(check_codex_toml(&project));
    }

    /// The setup UI's one payload aggregates every per-tool check: a fully
    /// wired project reports every project-scoped flag true, and no project
    /// reports them all false.
    #[test]
    fn detection_aggregates_every_check_into_one_payload() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().to_string_lossy().to_string();
        std::fs::write(
            dir.path().join(".mcp.json"),
            r#"{ "mcpServers": { "scryer": { "type": "stdio" } } }"#,
        )
        .unwrap();
        let claude_dir = dir.path().join(".claude");
        std::fs::create_dir_all(&claude_dir).unwrap();
        std::fs::write(
            claude_dir.join("settings.local.json"),
            r#"{ "permissions": { "allow": ["mcp__scryer"] } }"#,
        )
        .unwrap();
        std::fs::create_dir_all(dir.path().join(".codex")).unwrap();
        std::fs::write(
            dir.path().join(".codex/config.toml"),
            "[mcp_servers.scryer]\ncommand = \"/opt/scryer-mcp\"\n",
        )
        .unwrap();
        write_claude_hooks(&project, "/opt/scryer/scryer-mcp").unwrap();
        write_claude_statusline(&project, "/opt/scryer/scryer-mcp").unwrap();
        write_codex_hooks(&project, "/opt/scryer/scryer-mcp").unwrap();
        write_copilot_hooks(&project, "/opt/scryer/scryer-mcp").unwrap();

        let status = detect_ai_tools(Some(project));
        for flag in [
            "claudeMcpEnabled",
            "codexMcpEnabled",
            "copilotMcpEnabled",
            "claudeApproved",
            "claudeHooksEnabled",
            "codexHooksEnabled",
            "copilotHooksEnabled",
            "claudeStatuslineEnabled",
        ] {
            assert_eq!(status[flag], true, "{flag}: {status}");
        }
        assert_eq!(status["claudeStatuslineForeign"], false);

        let none = detect_ai_tools(None);
        assert_eq!(none["claudeMcpEnabled"], false, "no project, no project flags");
        assert_eq!(none["claudeHooksEnabled"], false);
    }

    /// With no binary beside the app, the PATH lookup finds scryer-mcp.
    #[test]
    fn find_scryer_mcp_falls_back_to_the_path_lookup() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = tempfile::tempdir().unwrap();
        let fake = dir.path().join("scryer-mcp");
        std::fs::write(&fake, "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();

        let old_path = std::env::var_os("PATH").unwrap_or_default();
        let mut paths = vec![dir.path().to_path_buf()];
        paths.extend(std::env::split_paths(&old_path));
        std::env::set_var("PATH", std::env::join_paths(paths).unwrap());
        let found = find_scryer_mcp();
        std::env::set_var("PATH", &old_path);

        assert_eq!(found.as_deref(), Some(fake.to_string_lossy().as_ref()));
    }
}
