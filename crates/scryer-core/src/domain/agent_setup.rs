//! Agent integration for one project: the entries scryer writes into each
//! agent tool's config files, how each is recognised as scryer's, and how it
//! is added and taken out again. Pure — composition reads and writes the files.
//!
//! An entry is scryer's by its marker and nothing else: an MCP server or a
//! command whose binary is `scryer-mcp` (the `hook` / `statusline` subcommand
//! for hooks and the status line), the `mcp__scryer` allow string, and the
//! header line of `.scryer/.gitignore`. Everything without the marker belongs
//! to someone else and is never changed.

use serde::Serialize;

/// The agent CLIs scryer integrates with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Tool {
    ClaudeCode,
    Codex,
    Copilot,
}

impl Tool {
    pub const ALL: [Tool; 3] = [Tool::ClaudeCode, Tool::Codex, Tool::Copilot];

    pub fn name(self) -> &'static str {
        match self {
            Tool::ClaudeCode => "Claude Code",
            Tool::Codex => "Codex",
            Tool::Copilot => "Copilot CLI",
        }
    }

    /// The executable whose presence on PATH means the tool is installed.
    pub fn executable(self) -> &'static str {
        match self {
            Tool::ClaudeCode => "claude",
            Tool::Codex => "codex",
            Tool::Copilot => "copilot",
        }
    }

    /// Every entry `setup on` writes for this tool, with the file it lives in.
    pub fn parts(self) -> &'static [(Part, &'static str)] {
        match self {
            Tool::ClaudeCode => &[
                (Part::McpServer, MCP_JSON),
                (Part::AutoApprove, CLAUDE_SETTINGS),
                (Part::Hooks, CLAUDE_SETTINGS),
                (Part::Statusline, CLAUDE_SETTINGS),
            ],
            Tool::Codex => &[(Part::McpServer, CODEX_CONFIG), (Part::Hooks, CODEX_HOOKS_FILE)],
            Tool::Copilot => &[(Part::McpServer, MCP_JSON), (Part::Hooks, COPILOT_HOOKS_FILE)],
        }
    }

    /// The hook registrations this tool gets.
    pub fn hook_events(self) -> &'static [HookEvent] {
        match self {
            Tool::ClaudeCode => CLAUDE_HOOK_EVENTS,
            Tool::Codex => CODEX_HOOK_EVENTS,
            Tool::Copilot => COPILOT_HOOK_EVENTS,
        }
    }

    /// The hook command, with `--copilot` for the harness whose tool names and
    /// reply shape differ (the event can't be sniffed for them).
    pub fn hook_command(self, binary: &str) -> String {
        match self {
            Tool::Copilot => format!("\"{binary}\" hook --copilot"),
            _ => format!("\"{binary}\" hook"),
        }
    }
}

/// One kind of entry `setup on` writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Part {
    McpServer,
    AutoApprove,
    Hooks,
    Statusline,
    Gitignore,
}

impl Part {
    pub fn label(self) -> &'static str {
        match self {
            Part::McpServer => "mcp server",
            Part::AutoApprove => "auto-approve",
            Part::Hooks => "hooks",
            Part::Statusline => "status line",
            Part::Gitignore => "gitignore",
        }
    }
}

/// Where one entry stands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", content = "note", rename_all = "camelCase")]
pub enum EntryState {
    Installed,
    Missing,
    /// Scryer's, but not what `on` would write now; `on` rewrites it.
    Stale(String),
    /// The slot holds someone else's entry; scryer leaves it alone.
    Foreign(String),
}

impl EntryState {
    /// Whether this entry is as `on` leaves it. A foreign entry is: `on`
    /// never takes a slot from its owner.
    pub fn settled(&self) -> bool {
        matches!(self, EntryState::Installed | EntryState::Foreign(_))
    }
}

pub const MCP_JSON: &str = ".mcp.json";
pub const CLAUDE_SETTINGS: &str = ".claude/settings.local.json";
pub const CODEX_CONFIG: &str = ".codex/config.toml";
pub const CODEX_HOOKS_FILE: &str = ".codex/hooks.json";
/// Copilot loads every `*.json` in `.github/hooks/` — the only project-scoped
/// hook location it reads — so scryer takes a file of its own there.
pub const COPILOT_HOOKS_FILE: &str = ".github/hooks/scryer.json";
pub const GITIGNORE: &str = ".scryer/.gitignore";

/// The server-wide allow entry: Claude Code reads a bare `mcp__<server>` as
/// "auto-approve every tool from that server".
pub const MCP_ALLOW: &str = "mcp__scryer";

/// `(event, matcher, timeout seconds)`.
pub type HookEvent = (&'static str, Option<&'static str>, u64);

/// Claude Code's registrations. Bash is bracketed (Pre marks its start, Post
/// records the files it modified) so shell edits count as the session's.
pub const CLAUDE_HOOK_EVENTS: &[HookEvent] = &[
    ("UserPromptSubmit", None, 10),
    ("PreToolUse", Some("Bash"), 10),
    ("PostToolUse", Some("Read"), 10),
    ("PostToolUse", Some("Edit|Write|NotebookEdit"), 10),
    ("PostToolUse", Some("Bash"), 10),
    ("Stop", None, 15),
];

/// Codex's: reads fire no hooks there, so the overlay rides PreToolUse on the
/// patch, native or a Bash heredoc. Timeouts stay explicit: Codex defaults to
/// 600 s.
pub const CODEX_HOOK_EVENTS: &[HookEvent] = &[
    ("UserPromptSubmit", None, 10),
    ("PreToolUse", Some("apply_patch|Bash"), 10),
    ("PostToolUse", Some("apply_patch|Bash"), 10),
    ("Stop", None, 15),
];

/// Copilot CLI's, in its own tool vocabulary. No `bash`: Copilot has a native
/// patch tool, so there is no heredoc route worth a hook on every command.
pub const COPILOT_HOOK_EVENTS: &[HookEvent] = &[
    ("UserPromptSubmit", None, 10),
    ("PostToolUse", Some("view"), 10),
    ("PostToolUse", Some("create|edit|str_replace_editor|apply_patch"), 10),
    ("Stop", None, 15),
];

/// The first line of the `.gitignore` `setup on` writes — its marker.
pub const GITIGNORE_MARKER: &str = "# Written by `scryer-mcp setup on`.";

/// `.scryer/.gitignore` as `setup on` writes it: commit only what a teammate
/// needs — `structure.toml` and the diagram layout. Everything else there
/// (session logs, verdict and derivation caches) is regenerable or personal.
pub fn gitignore_text() -> String {
    format!(
        "{GITIGNORE_MARKER} Only what a teammate needs is committed.\n\
         *\n\
         !.gitignore\n\
         !structure.toml\n\
         !layout\n\
         !layout/**\n"
    )
}

/// The `.gitignore` the model store wrote before setup owned it. Scryer wrote
/// it, so `on` may replace it; any other unmarked file is the user's.
pub const LEGACY_GITIGNORE: &str =
    "*.baseline.scry\n.sync\n.tmp.*\n.lock\n.anchors.json\n.build_edges.json\nsessions/\npreview/\nbuild-logs/\n";

pub fn gitignore_state(current: Option<&str>) -> EntryState {
    match current {
        None => EntryState::Missing,
        Some(text) if text == gitignore_text() => EntryState::Installed,
        Some(text) if text.starts_with(GITIGNORE_MARKER) || text == LEGACY_GITIGNORE => {
            EntryState::Stale("written by an earlier scryer".into())
        }
        Some(_) => EntryState::Foreign("yours, left untouched".into()),
    }
}

// --- Markers ---

/// The binary a command invokes: a quoted path, or the first word.
pub fn command_binary(command: &str) -> &str {
    let c = command.trim_start();
    match c.strip_prefix('"') {
        Some(rest) => rest.split('"').next().unwrap_or(rest),
        None => c.split_whitespace().next().unwrap_or(c),
    }
}

fn is_scryer_binary(binary: &str) -> bool {
    let name = binary.rsplit(['/', '\\']).next().unwrap_or(binary);
    name.starts_with("scryer-mcp")
}

/// Whether `command` runs scryer-mcp's `subcommand` (`hook`, `statusline`).
pub fn is_scryer_command(command: &str, subcommand: &str) -> bool {
    let rest = command.trim_end().trim_end_matches(" --copilot");
    is_scryer_binary(command_binary(command)) && rest.ends_with(&format!(" {subcommand}"))
}

/// Whether a hook entry is scryer's. Claude Code and Codex nest commands
/// under a `hooks` array; Copilot puts the command on the entry.
pub fn is_scryer_hook_entry(entry: &serde_json::Value) -> bool {
    hook_commands(entry).any(|c| is_scryer_command(c, "hook"))
}

fn hook_commands(entry: &serde_json::Value) -> impl Iterator<Item = &str> {
    entry["command"]
        .as_str()
        .into_iter()
        .chain(entry["hooks"].as_array().into_iter().flatten().filter_map(|h| h["command"].as_str()))
}

pub fn is_scryer_statusline(entry: &serde_json::Value) -> bool {
    entry["command"].as_str().is_some_and(|c| is_scryer_command(c, "statusline"))
}

// --- The MCP server entry: `.mcp.json` ---

pub fn mcp_json_state(root: &serde_json::Value, exists: &dyn Fn(&str) -> bool) -> EntryState {
    match root.pointer("/mcpServers/scryer") {
        None => EntryState::Missing,
        Some(entry) => match entry["command"].as_str() {
            Some(c) if is_scryer_binary(c) => binary_state(c, exists),
            _ => EntryState::Foreign("an mcpServers.scryer entry that isn't scryer-mcp".into()),
        },
    }
}

pub fn set_mcp_json(root: &mut serde_json::Value, binary: &str) {
    if !root.get("mcpServers").is_some_and(|v| v.is_object()) {
        root["mcpServers"] = serde_json::json!({});
    }
    root["mcpServers"]["scryer"] = serde_json::json!({ "type": "stdio", "command": binary, "args": [] });
}

pub fn remove_mcp_json(root: &mut serde_json::Value) -> bool {
    let ours = root
        .pointer("/mcpServers/scryer/command")
        .and_then(|c| c.as_str())
        .is_some_and(is_scryer_binary);
    ours && remove_key(root, "mcpServers", "scryer")
}

// --- The MCP server entry: `.codex/config.toml` ---

pub fn codex_toml_state(doc: &toml_edit::DocumentMut, exists: &dyn Fn(&str) -> bool) -> EntryState {
    let Some(entry) = doc.get("mcp_servers").and_then(|t| t.get("scryer")) else {
        return EntryState::Missing;
    };
    match entry.get("command").and_then(|c| c.as_str()) {
        Some(c) if is_scryer_binary(c) => binary_state(c, exists),
        _ => EntryState::Foreign("an mcp_servers.scryer table that isn't scryer-mcp".into()),
    }
}

pub fn set_codex_toml(doc: &mut toml_edit::DocumentMut, binary: &str) {
    if !doc.contains_table("mcp_servers") {
        let mut t = toml_edit::Table::new();
        t.set_implicit(true);
        doc["mcp_servers"] = toml_edit::Item::Table(t);
    }
    let mut server = toml_edit::Table::new();
    server.insert("command", toml_edit::value(binary));
    server.insert("args", toml_edit::value(toml_edit::Array::new()));
    doc["mcp_servers"]["scryer"] = toml_edit::Item::Table(server);
}

pub fn remove_codex_toml(doc: &mut toml_edit::DocumentMut) -> bool {
    let ours = doc
        .get("mcp_servers")
        .and_then(|t| t.get("scryer"))
        .and_then(|e| e.get("command"))
        .and_then(|c| c.as_str())
        .is_some_and(is_scryer_binary);
    if !ours {
        return false;
    }
    let servers = doc["mcp_servers"].as_table_like_mut().expect("checked above");
    servers.remove("scryer");
    if servers.is_empty() {
        doc.remove("mcp_servers");
    }
    true
}

// --- Auto-approve: `permissions.allow` ---

pub fn allow_state(root: &serde_json::Value) -> EntryState {
    let allowed = root
        .pointer("/permissions/allow")
        .and_then(|v| v.as_array())
        .is_some_and(|a| a.iter().any(|v| v.as_str() == Some(MCP_ALLOW)));
    if allowed {
        EntryState::Installed
    } else {
        EntryState::Missing
    }
}

pub fn set_allow(root: &mut serde_json::Value) {
    if !root.get("permissions").is_some_and(|v| v.is_object()) {
        root["permissions"] = serde_json::json!({});
    }
    if !root["permissions"].get("allow").is_some_and(|v| v.is_array()) {
        root["permissions"]["allow"] = serde_json::json!([]);
    }
    let allow = root["permissions"]["allow"].as_array_mut().expect("set above");
    if !allow.iter().any(|v| v.as_str() == Some(MCP_ALLOW)) {
        allow.push(serde_json::json!(MCP_ALLOW));
    }
}

pub fn remove_allow(root: &mut serde_json::Value) -> bool {
    let Some(allow) = root.pointer_mut("/permissions/allow").and_then(|v| v.as_array_mut()) else {
        return false;
    };
    let before = allow.len();
    allow.retain(|v| v.as_str() != Some(MCP_ALLOW));
    if allow.len() == before {
        return false;
    }
    if allow.is_empty() {
        remove_key(root, "permissions", "allow");
    }
    true
}

// --- Hooks: `{"hooks": {event: [entry…]}}` ---

/// Where scryer's hooks in `root` stand against `events`. Stale when the
/// registered set differs (an event since dropped or added, a changed
/// matcher, a duplicate) or a command names a binary that is gone.
pub fn hooks_state(root: &serde_json::Value, events: &[HookEvent], exists: &dyn Fn(&str) -> bool) -> EntryState {
    let Some(map) = root.get("hooks").and_then(|h| h.as_object()) else {
        return EntryState::Missing;
    };
    let mut found: Vec<(String, Option<String>)> = Vec::new();
    let mut gone = false;
    for (event, entries) in map {
        for entry in entries.as_array().into_iter().flatten().filter(|e| is_scryer_hook_entry(e)) {
            found.push((event.clone(), entry["matcher"].as_str().map(String::from)));
            gone |= hook_commands(entry).filter(|c| is_scryer_command(c, "hook")).any(|c| !exists(command_binary(c)));
        }
    }
    if found.is_empty() {
        return EntryState::Missing;
    }
    let mut want: Vec<(String, Option<String>)> =
        events.iter().map(|(e, m, _)| (e.to_string(), m.map(String::from))).collect();
    found.sort();
    want.sort();
    if gone {
        EntryState::Stale("names a scryer-mcp that no longer exists".into())
    } else if found != want {
        EntryState::Stale("registered by an earlier scryer".into())
    } else {
        EntryState::Installed
    }
}

/// Replace scryer's hook entries in `root` with `events`, in the nested schema
/// Claude Code and Codex share. Foreign entries are kept.
pub fn set_hooks(root: &mut serde_json::Value, events: &[HookEvent], command: &str) {
    remove_hooks(root);
    if !root.get("hooks").is_some_and(|v| v.is_object()) {
        root["hooks"] = serde_json::json!({});
    }
    for (event, matcher, timeout) in events {
        let mut entry = serde_json::json!({
            "hooks": [{ "type": "command", "command": command, "timeout": timeout }],
        });
        if let Some(m) = matcher {
            entry["matcher"] = serde_json::json!(m);
        }
        let list = root["hooks"].as_object_mut().expect("set above").entry(event.to_string()).or_insert(serde_json::json!([]));
        if !list.is_array() {
            *list = serde_json::json!([]);
        }
        list.as_array_mut().expect("set above").push(entry);
    }
}

/// Copilot's hook file, written whole: its FLAT entry schema (the command on
/// the entry), `version: 1`. `timeout` rather than `timeoutSec` — Copilot
/// normalises one to the other, and it keeps one vocabulary across tools.
pub fn copilot_hooks_file(command: &str) -> serde_json::Value {
    let mut hooks = serde_json::Map::new();
    for (event, matcher, timeout) in COPILOT_HOOK_EVENTS {
        let mut entry = serde_json::json!({ "type": "command", "command": command, "timeout": timeout });
        if let Some(m) = matcher {
            entry["matcher"] = serde_json::json!(m);
        }
        hooks
            .entry(event.to_string())
            .or_insert_with(|| serde_json::json!([]))
            .as_array_mut()
            .expect("inserted as an array")
            .push(entry);
    }
    serde_json::json!({ "version": 1, "hooks": hooks })
}

/// Take every scryer hook entry out of `root`, dropping an event list or a
/// `hooks` object this emptied. Returns whether anything was removed.
pub fn remove_hooks(root: &mut serde_json::Value) -> bool {
    let Some(map) = root.get_mut("hooks").and_then(|h| h.as_object_mut()) else {
        return false;
    };
    let mut removed = false;
    map.retain(|_, entries| {
        let Some(list) = entries.as_array_mut() else { return true };
        let before = list.len();
        list.retain(|e| !is_scryer_hook_entry(e));
        removed |= list.len() < before;
        !(list.is_empty() && before > 0)
    });
    if removed && map.is_empty() {
        root.as_object_mut().map(|o| o.remove("hooks"));
    }
    removed
}

// --- The status line: one slot ---

pub fn statusline_state(root: &serde_json::Value, exists: &dyn Fn(&str) -> bool) -> EntryState {
    match root.get("statusLine") {
        None | Some(serde_json::Value::Null) => EntryState::Missing,
        Some(e) if is_scryer_statusline(e) => binary_state(e["command"].as_str().unwrap_or_default(), exists),
        Some(_) => EntryState::Foreign("a status line of yours holds the slot, left untouched".into()),
    }
}

/// Take the status line slot unless someone else holds it. `statusLine` is a
/// whole-line replacement, so a foreign one is never overwritten.
pub fn set_statusline(root: &mut serde_json::Value, binary: &str) {
    if matches!(statusline_state(root, &|_| true), EntryState::Foreign(_)) {
        return;
    }
    root["statusLine"] = serde_json::json!({ "type": "command", "command": format!("\"{binary}\" statusline") });
}

pub fn remove_statusline(root: &mut serde_json::Value) -> bool {
    if !root.get("statusLine").is_some_and(is_scryer_statusline) {
        return false;
    }
    root.as_object_mut().map(|o| o.remove("statusLine"));
    true
}

// --- Shared ---

fn binary_state(command: &str, exists: &dyn Fn(&str) -> bool) -> EntryState {
    if exists(command_binary(command)) {
        EntryState::Installed
    } else {
        EntryState::Stale("names a scryer-mcp that no longer exists".into())
    }
}

/// Remove `root[parent][key]`, and `root[parent]` too if that emptied it.
fn remove_key(root: &mut serde_json::Value, parent: &str, key: &str) -> bool {
    let Some(obj) = root.get_mut(parent).and_then(|p| p.as_object_mut()) else {
        return false;
    };
    let removed = obj.remove(key).is_some();
    if removed && obj.is_empty() {
        root.as_object_mut().map(|o| o.remove(parent));
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn yes(_: &str) -> bool {
        true
    }

    /// Hooks that predate the current set, or name a binary since moved, are
    /// stale; foreign hooks alone are no install at all.
    #[test]
    fn a_hook_set_from_an_earlier_scryer_reads_as_stale() {
        let foreign = serde_json::json!({ "hooks": { "Stop": [{ "hooks": [{ "command": "my-linter" }] }] } });
        assert_eq!(hooks_state(&foreign, CLAUDE_HOOK_EVENTS, &yes), EntryState::Missing);

        let mut root = serde_json::json!({});
        set_hooks(&mut root, CLAUDE_HOOK_EVENTS, "\"/opt/scryer-mcp\" hook");
        assert_eq!(hooks_state(&root, CLAUDE_HOOK_EVENTS, &yes), EntryState::Installed);
        assert!(matches!(hooks_state(&root, CLAUDE_HOOK_EVENTS, &|_| false), EntryState::Stale(_)), "binary gone");

        let mut doubled = root.clone();
        let stop = doubled["hooks"]["Stop"][0].clone();
        doubled["hooks"]["Stop"].as_array_mut().unwrap().push(stop);
        assert!(matches!(hooks_state(&doubled, CLAUDE_HOOK_EVENTS, &yes), EntryState::Stale(_)), "duplicate");

        let mut old = root.clone();
        old["hooks"]["SessionStart"] = old["hooks"]["Stop"].clone();
        assert!(matches!(hooks_state(&old, CLAUDE_HOOK_EVENTS, &yes), EntryState::Stale(_)), "dropped event");
    }

    /// One predicate recognises scryer in both hook schemas; a foreign command,
    /// or scryer-mcp running something other than the hook client, is not it.
    #[test]
    fn scryer_entries_are_recognised_in_both_hook_schemas() {
        let nested = serde_json::json!({ "hooks": [{ "command": "\"/opt/scryer-mcp\" hook" }] });
        let flat = serde_json::json!({ "command": "\"/opt/scryer-mcp\" hook --copilot" });
        assert!(is_scryer_hook_entry(&nested));
        assert!(is_scryer_hook_entry(&flat));
        assert!(!is_scryer_hook_entry(&serde_json::json!({ "command": "my-linter" })));
        assert!(!is_scryer_hook_entry(&serde_json::json!({ "command": "\"/opt/scryer-mcp\" check" })));
        assert!(!is_scryer_hook_entry(&serde_json::json!({ "command": "/opt/not-scryer-mcp-wrapper hook" })));
    }

    /// An `mcpServers.scryer` entry that runs something else is the user's:
    /// it reads as foreign and removal leaves it.
    #[test]
    fn a_foreign_scryer_server_entry_is_left_alone() {
        let mut root = serde_json::json!({ "mcpServers": { "scryer": { "command": "docker" } } });
        assert!(matches!(mcp_json_state(&root, &yes), EntryState::Foreign(_)));
        assert!(!remove_mcp_json(&mut root));
        assert_eq!(root["mcpServers"]["scryer"]["command"], "docker");
    }

    /// When the gitignore carries the marker or is the model store's old default, treats it as scryer's, and any other file as the user's.
    #[test]
    fn the_gitignore_is_scryers_by_its_marker() {
        assert_eq!(gitignore_state(None), EntryState::Missing);
        assert_eq!(gitignore_state(Some(&gitignore_text())), EntryState::Installed);
        assert!(matches!(gitignore_state(Some(LEGACY_GITIGNORE)), EntryState::Stale(_)));
        assert!(matches!(gitignore_state(Some(&format!("{GITIGNORE_MARKER}\n*\n"))), EntryState::Stale(_)));
        assert!(matches!(gitignore_state(Some("build/\n")), EntryState::Foreign(_)));
    }
}
