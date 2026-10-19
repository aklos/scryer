//! Agent integration for a project, behind one switch: [`setup_on`] writes
//! scryer's entries for every detected agent tool, [`setup_off`] takes out
//! exactly those, and [`setup_status`] reads where they stand. The
//! `scryer-mcp setup` CLI and the app both call these, so the two can never
//! disagree about what is installed.
//!
//! One state per project: on when every detected tool has every entry as
//! `on` writes it, off otherwise. A stale entry — a binary that moved, an
//! event set from an earlier scryer — reads as off with a note, and `on`
//! rewrites it.

use crate::domain::agent_setup::{self as d, EntryState, Part, Tool};
use crate::infrastructure::agent_config as files;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;

/// One entry and where it stands.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupEntry {
    pub part: Part,
    /// Project-relative.
    pub file: String,
    #[serde(flatten)]
    pub state: EntryState,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolSetup {
    pub tool: Tool,
    pub name: &'static str,
    /// On PATH. `on` writes only for detected tools; `off` clears every tool.
    pub detected: bool,
    pub entries: Vec<SetupEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupStatus {
    pub on: bool,
    pub tools: Vec<ToolSetup>,
    pub gitignore: SetupEntry,
    pub notes: Vec<String>,
}

/// The agent tools installed on this machine.
pub fn detect_tools() -> Vec<Tool> {
    Tool::ALL.into_iter().filter(|t| files::on_path(t.executable())).collect()
}

/// Where scryer's integration stands in `project`, given the `detected` tools.
pub fn setup_status(project: &Path, detected: &[Tool]) -> SetupStatus {
    let mut docs = Docs::new(project);
    let exists = files::binary_exists;
    let tools: Vec<ToolSetup> = Tool::ALL
        .into_iter()
        .map(|tool| ToolSetup {
            tool,
            name: tool.name(),
            detected: detected.contains(&tool),
            entries: tool
                .parts()
                .iter()
                .map(|&(part, file)| SetupEntry { part, file: file.into(), state: docs.state(tool, part, file, &exists) })
                .collect(),
        })
        .collect();
    let gitignore = SetupEntry {
        part: Part::Gitignore,
        file: d::GITIGNORE.into(),
        state: match files::read_text(&project.join(d::GITIGNORE)) {
            Ok(text) => d::gitignore_state(text.as_deref()),
            Err(e) => EntryState::Stale(e),
        },
    };

    let mut notes = Vec::new();
    if detected.is_empty() {
        notes.push("No agent CLI (claude, codex, copilot) is on PATH, so there is nothing to integrate with.".into());
    }
    let mut any_missing = false;
    for t in tools.iter().filter(|t| t.detected) {
        for e in &t.entries {
            match &e.state {
                EntryState::Stale(why) => notes.push(format!(
                    "{} {} in {}: {why}; `setup on` rewrites it.",
                    t.name,
                    e.part.label(),
                    e.file
                )),
                EntryState::Foreign(why) => notes.push(format!("{} {} in {}: {why}.", t.name, e.part.label(), e.file)),
                EntryState::Missing => any_missing = true,
                EntryState::Installed => {}
            }
        }
    }
    match &gitignore.state {
        EntryState::Stale(why) => notes.push(format!("{}: {why}; `setup on` rewrites it.", d::GITIGNORE)),
        EntryState::Foreign(why) => notes.push(format!("{}: {why}.", d::GITIGNORE)),
        _ => {}
    }
    let installed_any = tools.iter().flat_map(|t| &t.entries).any(|e| e.state == EntryState::Installed);
    if any_missing && installed_any {
        notes.push("Partly installed; `setup on` writes the rest.".into());
    }
    let on = !detected.is_empty()
        && gitignore.state.settled()
        && tools.iter().filter(|t| t.detected).flat_map(|t| &t.entries).all(|e| e.state.settled());
    SetupStatus { on, tools, gitignore, notes }
}

/// Write every entry each `detected` tool needs, registering `binary` as
/// scryer-mcp. Entries already as `on` writes them, and slots someone else
/// holds, are left alone, so a second run changes nothing. Every file is read
/// before any is written: a file that doesn't parse stops the whole run.
pub fn setup_on(project: &Path, detected: &[Tool], binary: &str) -> Result<SetupStatus, String> {
    let before = setup_status(project, detected);
    let mut docs = Docs::new(project);
    for t in before.tools.iter().filter(|t| t.detected) {
        for e in t.entries.iter().filter(|e| !e.state.settled()) {
            docs.set(t.tool, e.part, &e.file, binary)?;
        }
    }
    let gitignore = (!before.gitignore.state.settled()).then(d::gitignore_text);
    docs.save()?;
    if let Some(text) = gitignore {
        files::write_text(&project.join(d::GITIGNORE), &text)?;
    }
    Ok(setup_status(project, detected))
}

/// Take out every scryer entry, for every tool, detected or not, and nothing
/// else. A file left empty by it is removed. `.scryer/.gitignore` stays: it
/// keeps scryer's own files out of commits whether or not agents use it.
pub fn setup_off(project: &Path, detected: &[Tool]) -> Result<SetupStatus, String> {
    let mut docs = Docs::new(project);
    for tool in Tool::ALL {
        for &(part, file) in tool.parts() {
            docs.remove(part, file)?;
        }
    }
    docs.save()?;
    Ok(setup_status(project, detected))
}

/// The readout `scryer-mcp setup` prints: the state, then per tool, per file.
pub fn render_setup_status(s: &SetupStatus) -> String {
    let state = |e: &SetupEntry| match &e.state {
        EntryState::Installed => "installed".to_string(),
        EntryState::Missing => "missing".to_string(),
        EntryState::Stale(_) => "stale".to_string(),
        EntryState::Foreign(_) => "not scryer's, left alone".to_string(),
    };
    let row = |e: &SetupEntry| format!("    {:<30} {:<13} {}\n", e.file, e.part.label(), state(e));
    let mut out = format!("Agent integration: {}\n", if s.on { "on" } else { "off" });
    // An undetected tool is listed only for what it alone has installed: the
    // `.mcp.json` it shares with a detected tool says nothing about it.
    let shown = |e: &SetupEntry| s.tools.iter().filter(|t| t.detected).flat_map(|t| &t.entries).all(|d| d.file != e.file);
    for t in &s.tools {
        if !t.detected && t.entries.iter().all(|e| e.state == EntryState::Missing || !shown(e)) {
            continue;
        }
        out.push_str(&format!("  {}{}\n", t.name, if t.detected { "" } else { " (not on PATH)" }));
        for e in &t.entries {
            out.push_str(&row(e));
        }
    }
    out.push_str("  Project\n");
    out.push_str(&row(&s.gitignore));
    for n in &s.notes {
        out.push_str(&format!("note: {n}\n"));
    }
    out
}

/// The config files one run touches, each read once on first use and written
/// once at the end, only if changed.
struct Docs<'a> {
    project: &'a Path,
    json: BTreeMap<&'static str, Loaded<serde_json::Value>>,
    toml: BTreeMap<&'static str, Loaded<toml_edit::DocumentMut>>,
}

struct Loaded<T> {
    value: T,
    changed: bool,
    /// Whether scryer removed something here — a file that leaves empty is
    /// deleted rather than left as `{}`.
    removed: bool,
}

impl<'a> Docs<'a> {
    fn new(project: &'a Path) -> Self {
        Docs { project, json: BTreeMap::new(), toml: BTreeMap::new() }
    }

    fn json(&mut self, file: &str) -> Result<&mut Loaded<serde_json::Value>, String> {
        let file = static_file(file);
        if !self.json.contains_key(file) {
            let value = files::read_json(&self.project.join(file))?.unwrap_or_else(|| serde_json::json!({}));
            if !value.is_object() {
                return Err(format!("{file} is not a JSON object; refusing to change it."));
            }
            self.json.insert(file, Loaded { value, changed: false, removed: false });
        }
        Ok(self.json.get_mut(file).expect("inserted above"))
    }

    fn toml(&mut self, file: &str) -> Result<&mut Loaded<toml_edit::DocumentMut>, String> {
        let file = static_file(file);
        if !self.toml.contains_key(file) {
            let value = files::read_toml(&self.project.join(file))?.unwrap_or_default();
            self.toml.insert(file, Loaded { value, changed: false, removed: false });
        }
        Ok(self.toml.get_mut(file).expect("inserted above"))
    }

    fn state(&mut self, tool: Tool, part: Part, file: &str, exists: &dyn Fn(&str) -> bool) -> EntryState {
        let unreadable = |e: String| EntryState::Stale(e);
        if file == d::CODEX_CONFIG {
            return match self.toml(file) {
                Ok(doc) => d::codex_toml_state(&doc.value, exists),
                Err(e) => unreadable(e),
            };
        }
        let root = match self.json(file) {
            Ok(doc) => &doc.value,
            Err(e) => return unreadable(e),
        };
        match part {
            Part::McpServer => d::mcp_json_state(root, exists),
            Part::AutoApprove => d::allow_state(root),
            Part::Hooks => d::hooks_state(root, tool.hook_events(), exists),
            Part::Statusline => d::statusline_state(root, exists),
            Part::Gitignore => unreachable!("not a tool's entry"),
        }
    }

    fn set(&mut self, tool: Tool, part: Part, file: &str, binary: &str) -> Result<(), String> {
        if file == d::CODEX_CONFIG {
            let doc = self.toml(file)?;
            d::set_codex_toml(&mut doc.value, binary);
            doc.changed = true;
            return Ok(());
        }
        if file == d::COPILOT_HOOKS_FILE {
            // Scryer's own file, written whole: a corrupt one is repaired, not
            // refused, since nothing in it is anyone else's.
            let file = static_file(file);
            let value = d::copilot_hooks_file(&tool.hook_command(binary));
            self.json.insert(file, Loaded { value, changed: true, removed: false });
            return Ok(());
        }
        let doc = self.json(file)?;
        match part {
            Part::McpServer => d::set_mcp_json(&mut doc.value, binary),
            Part::AutoApprove => d::set_allow(&mut doc.value),
            Part::Hooks => d::set_hooks(&mut doc.value, tool.hook_events(), &tool.hook_command(binary)),
            Part::Statusline => d::set_statusline(&mut doc.value, binary),
            Part::Gitignore => unreachable!("not a tool's entry"),
        }
        doc.changed = true;
        Ok(())
    }

    fn remove(&mut self, part: Part, file: &str) -> Result<(), String> {
        if !self.project.join(file).exists() {
            return Ok(());
        }
        if file == d::CODEX_CONFIG {
            let doc = self.toml(file)?;
            let removed = d::remove_codex_toml(&mut doc.value);
            doc.changed |= removed;
            doc.removed |= removed;
            return Ok(());
        }
        let doc = self.json(file)?;
        let removed = match part {
            Part::McpServer => d::remove_mcp_json(&mut doc.value),
            Part::AutoApprove => d::remove_allow(&mut doc.value),
            Part::Hooks => d::remove_hooks(&mut doc.value),
            Part::Statusline => d::remove_statusline(&mut doc.value),
            Part::Gitignore => unreachable!("not a tool's entry"),
        };
        // Copilot's file is scryer's: with no hooks left, its `version` key
        // alone is not worth keeping.
        if removed && file == d::COPILOT_HOOKS_FILE && doc.value.get("hooks").is_none() {
            doc.value = serde_json::json!({});
        }
        doc.changed |= removed;
        doc.removed |= removed;
        Ok(())
    }

    fn save(self) -> Result<(), String> {
        for (file, doc) in self.json.into_iter().filter(|(_, d)| d.changed) {
            let path = self.project.join(file);
            if doc.removed && doc.value.as_object().is_some_and(|o| o.is_empty()) {
                files::remove_file(self.project, &path)?;
            } else {
                let text = serde_json::to_string_pretty(&doc.value).map_err(|e| e.to_string())?;
                files::write_text(&path, &format!("{text}\n"))?;
            }
        }
        for (file, doc) in self.toml.into_iter().filter(|(_, d)| d.changed) {
            let path = self.project.join(file);
            let text = doc.value.to_string();
            if doc.removed && text.trim().is_empty() {
                files::remove_file(self.project, &path)?;
            } else {
                files::write_text(&path, &text)?;
            }
        }
        Ok(())
    }
}

/// The static name of one of the files the parts list — every file a run
/// touches is one of them.
fn static_file(file: &str) -> &'static str {
    [d::MCP_JSON, d::CLAUDE_SETTINGS, d::CODEX_CONFIG, d::CODEX_HOOKS_FILE, d::COPILOT_HOOKS_FILE]
        .into_iter()
        .find(|f| *f == file)
        .expect("a file from Tool::parts")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stand-in scryer-mcp that exists, so entries naming it read installed.
    fn fake_binary(dir: &Path) -> String {
        let bin = dir.join("bin").join("scryer-mcp");
        std::fs::create_dir_all(bin.parent().unwrap()).unwrap();
        std::fs::write(&bin, "").unwrap();
        bin.to_string_lossy().to_string()
    }

    fn read(dir: &Path, file: &str) -> Option<String> {
        std::fs::read_to_string(dir.join(file)).ok()
    }

    fn json(dir: &Path, file: &str) -> serde_json::Value {
        serde_json::from_str(&read(dir, file).unwrap()).unwrap()
    }

    const FILES: [&str; 6] =
        [d::MCP_JSON, d::CLAUDE_SETTINGS, d::CODEX_CONFIG, d::CODEX_HOOKS_FILE, d::COPILOT_HOOKS_FILE, d::GITIGNORE];

    fn snapshot(dir: &Path) -> Vec<Option<String>> {
        FILES.iter().map(|f| read(dir, f)).collect()
    }

    /// The user's own config in every file `on` touches.
    fn foreign_config(dir: &Path) {
        let write = |file: &str, text: String| {
            std::fs::create_dir_all(dir.join(file).parent().unwrap()).unwrap();
            std::fs::write(dir.join(file), text).unwrap();
        };
        write(d::MCP_JSON, serde_json::json!({ "mcpServers": { "other": { "command": "other-mcp" } } }).to_string());
        write(
            d::CLAUDE_SETTINGS,
            serde_json::json!({
                "permissions": { "allow": ["Bash(ls:*)"] },
                "hooks": { "PostToolUse": [{ "matcher": "Bash", "hooks": [{ "type": "command", "command": "my-linter" }] }] },
                "model": "opus"
            })
            .to_string(),
        );
        write(d::CODEX_CONFIG, "model = \"gpt-5.5\"\n\n[mcp_servers.other]\ncommand = \"other-mcp\"\n".into());
        write(
            d::CODEX_HOOKS_FILE,
            serde_json::json!({ "hooks": { "PreToolUse": [{ "matcher": "Bash", "hooks": [{ "command": "my-policy" }] }] } })
                .to_string(),
        );
        write(".github/hooks/my-linter.json", serde_json::json!({ "hooks": {} }).to_string());
    }

    /// When the switch goes on, every detected tool gets its MCP entry and
    /// hooks, Claude Code its auto-approve and status line, and the project its
    /// .gitignore; running it again changes no file, and the user's own
    /// entries in those files are all still there.
    #[test]
    fn setup_on_writes_every_entry_once_and_keeps_foreign_ones() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        let bin = fake_binary(p);
        foreign_config(p);

        assert!(!setup_status(p, &Tool::ALL).on);
        let status = setup_on(p, &Tool::ALL, &bin).unwrap();
        assert!(status.on, "{}", render_setup_status(&status));
        assert!(status.notes.is_empty(), "{:?}", status.notes);

        let after = snapshot(p);
        assert_eq!(setup_on(p, &Tool::ALL, &bin).unwrap(), status);
        assert_eq!(snapshot(p), after, "a second run changes nothing");

        let mcp = json(p, d::MCP_JSON);
        assert_eq!(mcp["mcpServers"]["other"]["command"], "other-mcp");
        assert_eq!(mcp["mcpServers"]["scryer"]["command"], bin.as_str());
        let claude = json(p, d::CLAUDE_SETTINGS);
        assert_eq!(claude["permissions"]["allow"], serde_json::json!(["Bash(ls:*)", "mcp__scryer"]));
        assert_eq!(claude["model"], "opus");
        let post = claude["hooks"]["PostToolUse"].as_array().unwrap();
        assert!(post.iter().any(|e| e["hooks"][0]["command"] == "my-linter"));
        assert_eq!(post.iter().filter(|e| d::is_scryer_hook_entry(e)).count(), 3);
        assert!(d::is_scryer_statusline(&claude["statusLine"]));
        let codex = read(p, d::CODEX_CONFIG).unwrap();
        assert!(codex.contains("model = \"gpt-5.5\"") && codex.contains("[mcp_servers.other]"), "{codex}");
        assert!(codex.contains("[mcp_servers.scryer]"), "{codex}");
        let codex_hooks = json(p, d::CODEX_HOOKS_FILE);
        assert_eq!(codex_hooks["hooks"]["PreToolUse"].as_array().unwrap().len(), 2, "my-policy + scryer");
        assert_eq!(json(p, d::COPILOT_HOOKS_FILE)["hooks"]["PostToolUse"][0]["command"], format!("\"{bin}\" hook --copilot"));
        assert_eq!(read(p, d::GITIGNORE).unwrap(), d::gitignore_text());
    }

    /// Off removes exactly what on wrote: every file holds again what it held
    /// before, a file scryer created is gone, and the .gitignore stays.
    #[test]
    fn setup_off_removes_exactly_scryers_entries() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        let bin = fake_binary(p);
        foreign_config(p);
        let foreign: Vec<serde_json::Value> =
            [d::MCP_JSON, d::CLAUDE_SETTINGS, d::CODEX_HOOKS_FILE].iter().map(|f| json(p, f)).collect();
        let codex = read(p, d::CODEX_CONFIG).unwrap();

        setup_on(p, &Tool::ALL, &bin).unwrap();
        let off = setup_off(p, &Tool::ALL).unwrap();
        assert!(!off.on);
        assert!(off.tools.iter().flat_map(|t| &t.entries).all(|e| e.state == EntryState::Missing));
        let after: Vec<serde_json::Value> =
            [d::MCP_JSON, d::CLAUDE_SETTINGS, d::CODEX_HOOKS_FILE].iter().map(|f| json(p, f)).collect();
        assert_eq!(after, foreign);
        assert_eq!(read(p, d::CODEX_CONFIG).unwrap().trim(), codex.trim());
        assert!(read(p, d::COPILOT_HOOKS_FILE).is_none(), "scryer's own file is gone");
        assert!(read(p, ".github/hooks/my-linter.json").is_some());
        assert_eq!(read(p, d::GITIGNORE).unwrap(), d::gitignore_text());

        let again = snapshot(p);
        setup_off(p, &Tool::ALL).unwrap();
        assert_eq!(snapshot(p), again, "a second off changes nothing");
    }

    /// In a project with no agent config at all, on then off leaves no agent
    /// config file behind.
    #[test]
    fn on_then_off_in_a_bare_project_leaves_no_config_files() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        let bin = fake_binary(p);
        setup_on(p, &Tool::ALL, &bin).unwrap();
        setup_off(p, &Tool::ALL).unwrap();
        for f in [d::MCP_JSON, d::CLAUDE_SETTINGS, d::CODEX_CONFIG, d::CODEX_HOOKS_FILE, d::COPILOT_HOOKS_FILE] {
            assert!(read(p, f).is_none(), "{f} left behind");
        }
        for dir in [".claude", ".codex", ".github"] {
            assert!(!p.join(dir).exists(), "{dir} left behind");
        }
    }

    /// Only detected tools get entries, and the state is on once they have
    /// them all; with no tool detected the project can't be on.
    #[test]
    fn on_writes_only_for_detected_tools() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        let bin = fake_binary(p);
        let status = setup_on(p, &[Tool::Codex], &bin).unwrap();
        assert!(status.on);
        assert!(read(p, d::CLAUDE_SETTINGS).is_none());
        assert!(read(p, d::MCP_JSON).is_none());
        assert!(read(p, d::CODEX_HOOKS_FILE).is_some());

        let none = setup_status(p, &[]);
        assert!(!none.on);
        assert!(none.notes[0].contains("No agent CLI"), "{:?}", none.notes);
    }

    /// An entry naming a scryer-mcp that has since gone reads as off with a
    /// note, and on rewrites it to the binary it is given.
    #[test]
    fn a_stale_entry_reads_as_off_and_on_rewrites_it() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        let bin = fake_binary(p);
        setup_on(p, &[Tool::ClaudeCode], "/gone/scryer-mcp").unwrap();
        let stale = setup_status(p, &[Tool::ClaudeCode]);
        assert!(!stale.on);
        assert!(stale.notes.iter().any(|n| n.contains("no longer exists")), "{:?}", stale.notes);

        assert!(setup_on(p, &[Tool::ClaudeCode], &bin).unwrap().on);
        assert_eq!(json(p, d::MCP_JSON)["mcpServers"]["scryer"]["command"], bin.as_str());
    }

    /// A status line of the user's holds the one slot: on leaves it, the
    /// project still reads on with a note, and off leaves it too.
    #[test]
    fn a_foreign_status_line_is_never_clobbered() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        let bin = fake_binary(p);
        std::fs::create_dir_all(p.join(".claude")).unwrap();
        let mine = serde_json::json!({ "type": "command", "command": "my-powerline" });
        std::fs::write(p.join(d::CLAUDE_SETTINGS), serde_json::json!({ "statusLine": mine }).to_string()).unwrap();

        let on = setup_on(p, &[Tool::ClaudeCode], &bin).unwrap();
        assert!(on.on);
        assert!(on.notes.iter().any(|n| n.contains("status line of yours")), "{:?}", on.notes);
        assert_eq!(json(p, d::CLAUDE_SETTINGS)["statusLine"], mine);
        setup_off(p, &[Tool::ClaudeCode]).unwrap();
        assert_eq!(json(p, d::CLAUDE_SETTINGS), serde_json::json!({ "statusLine": mine }));
    }

    /// A settings file that doesn't parse stops the run before anything is
    /// written, and is left byte for byte.
    #[test]
    fn a_malformed_file_stops_the_run_before_any_write() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        let bin = fake_binary(p);
        std::fs::create_dir_all(p.join(".claude")).unwrap();
        let broken = r#"{ "permissions": { "allow": ["x",] } }"#;
        std::fs::write(p.join(d::CLAUDE_SETTINGS), broken).unwrap();

        let err = setup_on(p, &[Tool::ClaudeCode], &bin).unwrap_err();
        assert!(err.contains("not valid JSON"), "{err}");
        assert_eq!(read(p, d::CLAUDE_SETTINGS).unwrap(), broken);
        assert!(read(p, d::MCP_JSON).is_none(), "nothing else written");
        assert!(setup_off(p, &[Tool::ClaudeCode]).is_err());
        assert_eq!(read(p, d::CLAUDE_SETTINGS).unwrap(), broken);
    }

    /// The readout names the state, each tool, and each file.
    #[test]
    fn the_readout_lists_entries_per_tool_per_file() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        let bin = fake_binary(p);
        setup_on(p, &[Tool::ClaudeCode, Tool::Copilot], &bin).unwrap();
        let text = render_setup_status(&setup_status(p, &[Tool::ClaudeCode]));
        assert!(text.starts_with("Agent integration: on\n"), "{text}");
        assert!(text.contains("  Claude Code\n"), "{text}");
        assert!(text.contains(".claude/settings.local.json") && text.contains("hooks"), "{text}");
        assert!(!text.contains("Codex"), "an undetected tool with nothing installed is not listed: {text}");
        assert!(text.contains("Copilot CLI (not on PATH)") && text.contains(".github/hooks/scryer.json"), "{text}");
        let v = serde_json::to_value(setup_status(p, &[Tool::ClaudeCode])).unwrap();
        assert_eq!(v["tools"][0]["entries"][0], serde_json::json!({ "part": "mcpServer", "file": ".mcp.json", "state": "installed" }));
    }
}
