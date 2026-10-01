//! `scryer-mcp hook` — the session-hook client for Claude Code, Codex and
//! Copilot CLI.
//!
//! The harness invokes this once per hook event with the event JSON on stdin.
//! All three name the event fields the same way — `hook_event_name`,
//! `session_id`, `cwd`, `tool_name`, `tool_input` — so one client serves them
//! all, dispatching on the event and tool names. Everything runs in-process
//! against the project's model and the session's log
//! (`.scryer/sessions/<session_id>.jsonl`); the desktop app need not be open.
//!
//! - PostToolUse read  → inject the file's governing intent (once per session
//!                       until it changes)
//! - PostToolUse edit… → record the touch, say nothing
//! - Stop              → block once with unreconciled claims
//!
//! Where they differ is the tool vocabulary and the reply shape, and neither is
//! discoverable from the event — so the install writes which harness it is
//! (`--copilot`) rather than the client sniffing for it. See [`Harness`].
//!
//! Codex reads fire no hooks at all, so there the overlay rides PreToolUse on
//! the patch (intent lands just before the edit) and touches are recorded per
//! file named in the envelope. Copilot fires post-read like Claude Code does,
//! so it gets the same post-Read overlay.
//!
//! Every failure path — no model above the working directory, malformed input,
//! an unreadable model — exits 0 with no output: a hook must never break the
//! session it observes.

use scryer_core::ModelRef;
use std::io::Read;
use std::path::{Path, PathBuf};

/// Which harness registered this hook. The event JSON is the same shape
/// everywhere, but two things about it are not, and neither can be read off the
/// event:
///
/// - **Tool vocabulary.** Claude Code and Codex call the tools scryer cares
///   about `Read` / `Edit` / `Write` / `apply_patch` / `Bash`, and name the
///   edited file in `tool_input.file_path`. Copilot calls them `view` /
///   `create` / `edit` / `str_replace_editor` / `apply_patch`, and names the
///   file in `tool_input.path`.
/// - **Where injected context goes.** Claude Code reads it out of the
///   `hookSpecificOutput` envelope; Copilot reads a top-level
///   `additionalContext` on PostToolUse (only its PreToolUse
///   accepts either). One reply can't satisfy both without guessing.
///
/// So the install records the harness in the registered command — `hook` or
/// `hook --copilot` — and the client is told rather than left to sniff.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Harness {
    /// Claude Code and Codex: same vocabulary, same envelope.
    ClaudeLike,
    Copilot,
}

/// What a tool call means to scryer, once the harness's own name for it is
/// resolved. Everything else is [`ToolKind::Other`] and costs nothing.
#[derive(PartialEq, Eq)]
enum ToolKind {
    /// Read a file — the moment to inject the intent governing it.
    Read,
    /// Write to the file named in the arguments.
    Write,
    /// Write to every file named in an `apply_patch` envelope.
    Patch,
    Other,
}

impl Harness {
    fn tool_kind(self, tool: &str) -> ToolKind {
        match (self, tool) {
            (Harness::ClaudeLike, "Read") | (Harness::Copilot, "view") => ToolKind::Read,
            (Harness::ClaudeLike, "Edit" | "Write" | "NotebookEdit") => ToolKind::Write,
            (Harness::Copilot, "create" | "edit" | "str_replace_editor") => ToolKind::Write,
            // Codex routes edits through a native `apply_patch` or a Bash
            // heredoc wrapping the same envelope; Copilot has the native tool
            // only. A Bash command with no envelope in it parses to no files
            // and costs one no-op.
            (Harness::ClaudeLike, "apply_patch" | "Bash") => ToolKind::Patch,
            (Harness::Copilot, "apply_patch") => ToolKind::Patch,
            _ => ToolKind::Other,
        }
    }

    /// Emit injected context in the shape this harness reads.
    fn emit_context(self, event_name: &str, text: &str) {
        match self {
            Harness::ClaudeLike => emit(&serde_json::json!({
                "hookSpecificOutput": {
                    "hookEventName": event_name,
                    "additionalContext": text,
                }
            })),
            Harness::Copilot => emit(&serde_json::json!({ "additionalContext": text })),
        }
    }
}

pub fn run_hook_client(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let harness = if args.iter().any(|a| a == "--copilot") {
        Harness::Copilot
    } else {
        Harness::ClaudeLike
    };

    let mut input = String::new();
    if std::io::stdin().read_to_string(&mut input).is_err() {
        return Ok(());
    }
    let Ok(event) = serde_json::from_str::<serde_json::Value>(&input) else {
        return Ok(());
    };

    let Some(r) = discover(&event) else {
        return Ok(()); // no model — stay silent
    };

    match event["hook_event_name"].as_str().unwrap_or_default() {
        "PreToolUse" => pre_tool_use(&r, &event, harness),
        "PostToolUse" => post_tool_use(&r, &event, harness),
        "Stop" => stop(&r, &event),
        _ => {}
    }
    Ok(())
}

/// Find the project's model: `$CLAUDE_PROJECT_DIR` first, then the event's
/// `cwd`, walking up so hooks fired from a subdirectory still find it.
fn discover(event: &serde_json::Value) -> Option<ModelRef> {
    let start = std::env::var("CLAUDE_PROJECT_DIR")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| event["cwd"].as_str().map(str::to_string))?;
    let mut dir = Some(PathBuf::from(start));
    while let Some(d) = dir {
        if let Some(r) = scryer_core::resolve_project_model(&d) {
            return Some(r);
        }
        dir = d.parent().map(Path::to_path_buf);
    }
    None
}

fn emit(v: &serde_json::Value) {
    println!("{}", serde_json::to_string(v).unwrap_or_default());
}

/// The file a tool call names. `file_path` is Claude Code's and Codex's key,
/// `path` Copilot's; accepting both keeps one lookup for every harness.
fn tool_file(event: &serde_json::Value) -> Option<&str> {
    event["tool_input"]["file_path"]
        .as_str()
        .or_else(|| event["tool_input"]["path"].as_str())
}

/// The session id an event carries, if any. Copilot sends none on some events;
/// an absent or empty id means "no session": no dedupe, no touch log.
fn session_id(event: &serde_json::Value) -> Option<&str> {
    event["session_id"].as_str().filter(|s| !s.is_empty())
}

/// The rendered overlay for `file`, unless the session already saw it.
fn overlay_text(r: &ModelRef, event: &serde_json::Value, file: &str) -> Option<String> {
    let overlay = scryer_core::session::overlay(r, session_id(event), file).ok()??;
    render_overlay(&overlay)
}

fn post_tool_use(r: &ModelRef, event: &serde_json::Value, harness: Harness) {
    match harness.tool_kind(event["tool_name"].as_str().unwrap_or_default()) {
        ToolKind::Read => {
            let Some(file) = tool_file(event) else { return };
            if let Some(text) = overlay_text(r, event, file) {
                harness.emit_context("PostToolUse", &text);
            }
        }
        ToolKind::Write => {
            let Some(file) = tool_file(event) else { return };
            touch(r, event, file);
        }
        ToolKind::Patch => {
            for file in patched_files(event) {
                touch(r, event, &file);
            }
        }
        ToolKind::Other => {}
    }
}

/// Record one touched file. No output: touch recording must cost the session
/// zero tokens.
fn touch(r: &ModelRef, event: &serde_json::Value, file: &str) {
    if let Some(session) = session_id(event) {
        let _ = scryer_core::session::record_touch(r, session, file);
    }
}

/// Bound the pre-edit injection on sweeping patches: past a handful of files
/// the overlay stops being "the intent for what you're touching" and becomes
/// a wall of text the agent learns to skim.
const OVERLAY_FILE_CAP: usize = 5;

/// Codex reads fire no hook events, so the intent overlay rides the edit
/// instead: just before a patch lands, inject the claims and directives
/// governing the files it names. Claude Code and Copilot never send this event
/// (scryer registers PreToolUse for neither — post-Read is the better moment,
/// and both fire it).
fn pre_tool_use(r: &ModelRef, event: &serde_json::Value, harness: Harness) {
    let sections: Vec<String> = patched_files(event)
        .iter()
        .take(OVERLAY_FILE_CAP)
        .filter_map(|file| overlay_text(r, event, file))
        .collect();
    if sections.is_empty() {
        return;
    }
    harness.emit_context("PreToolUse", &sections.join("\n\n"));
}

/// File paths named by the apply_patch envelope in this tool call, if any.
/// Codex's native `apply_patch` carries the envelope in `tool_input.command`;
/// newer Codex builds route edits through Bash as an `apply_patch <<'EOF'`
/// heredoc with the same envelope inside; Copilot's `apply_patch` is a freeform
/// tool whose whole `tool_input` IS the patch string. The envelope grammar is
/// identical in all three, so the same parse serves them — only where to look
/// for it differs, and trying the string form first covers that without needing
/// to know the harness. Envelope paths are cwd-relative — absolutized here so
/// the endpoint's project-prefix stripping works even when the session runs in
/// a subdirectory.
fn patched_files(event: &serde_json::Value) -> Vec<String> {
    let input = &event["tool_input"];
    let command = input
        .as_str()
        .or_else(|| input["command"].as_str())
        .unwrap_or_default();
    let cwd = event["cwd"].as_str().unwrap_or_default();
    envelope_files(command)
        .into_iter()
        .map(|f| absolutize(cwd, &f))
        .collect()
}

/// Parse `*** Add File:` / `*** Update File:` / `*** Delete File:` markers
/// (and a rename's `*** Move to:` target) out of an apply_patch envelope.
/// Anything without a `*** Begin Patch` line is not an envelope — that check
/// is what lets every ordinary Bash command no-op without an HTTP call.
fn envelope_files(command: &str) -> Vec<String> {
    if !command.contains("*** Begin Patch") {
        return Vec::new();
    }
    let mut files: Vec<String> = Vec::new();
    for line in command.lines() {
        let line = line.trim();
        let path = ["*** Add File:", "*** Update File:", "*** Delete File:", "*** Move to:"]
            .iter()
            .find_map(|marker| line.strip_prefix(marker));
        if let Some(p) = path {
            let p = p.trim();
            if !p.is_empty() && !files.iter().any(|f| f == p) {
                files.push(p.to_string());
            }
        }
    }
    files
}

fn absolutize(cwd: &str, file: &str) -> String {
    let p = Path::new(file);
    if p.is_absolute() || cwd.is_empty() {
        file.to_string()
    } else {
        Path::new(cwd).join(p).to_string_lossy().to_string()
    }
}

/// The compact intent overlay for one file — or `None` when the model has
/// nothing to say about it (dark files stay silent; noise here would teach
/// the agent to ignore the channel). Repeats never get here: the session log
/// already answered them with no overlay at all.
fn render_overlay(overlay: &serde_json::Value) -> Option<String> {
    let claims = overlay["claims"].as_array().cloned().unwrap_or_default();
    let pending = overlay["pending"].as_array().cloned().unwrap_or_default();
    let mut directives: Vec<String> = Vec::new();
    for d in overlay["ownDirectives"].as_array().into_iter().flatten() {
        if let Some(s) = d.as_str() {
            directives.push(s.to_string());
        }
    }
    for inh in overlay["inheritedDirectives"].as_array().into_iter().flatten() {
        let from = inh["name"].as_str().unwrap_or("ancestor");
        for d in inh["directives"].as_array().into_iter().flatten() {
            if let Some(s) = d.as_str() {
                directives.push(format!("{s} (from {from})"));
            }
        }
    }
    let placement = overlay.get("placement").filter(|p| p.is_object());
    if claims.is_empty() && directives.is_empty() && pending.is_empty() && placement.is_none() {
        return None;
    }

    let file = overlay["file"].as_str().unwrap_or("this file");
    let mut out = String::new();
    match overlay["path"].as_str() {
        Some(p) => out.push_str(&format!("[scryer] {file} — {p}\n")),
        None => out.push_str(&format!("[scryer] {file}\n")),
    }
    // The style's answer for this file, one line: its layer, what it may
    // import, where its layer lives. The table itself never appears.
    if let Some(p) = placement {
        let may: Vec<&str> = p["mayImport"].as_array().into_iter().flatten().filter_map(|v| v.as_str()).collect();
        let mut line = format!(
            "layer: {} · may import: {}",
            p["layer"].as_str().unwrap_or("?"),
            if may.is_empty() { "nothing".to_string() } else { may.join(", ") }
        );
        if let Some(d) = p["dir"].as_str() {
            line.push_str(&format!(" · path: {d}"));
        }
        out.push_str(&line);
        out.push('\n');
    }
    if !claims.is_empty() {
        out.push_str("The model claims this file:\n");
        for c in &claims {
            let host = c["hostName"].as_str().unwrap_or("?");
            let statement = c["statement"].as_str().unwrap_or("(data shape declaration)");
            let mut flags = String::new();
            if c["stale"].as_bool() == Some(true) {
                flags.push_str(" [stale — awaiting verdict]");
            }
            if c["vagrant"].as_bool() == Some(true) {
                flags.push_str(" [vagrant — awaiting adoption]");
            }
            out.push_str(&format!("- ({host}) {statement}{flags}\n"));
            for d in c["directives"].as_array().into_iter().flatten() {
                if let Some(s) = d.as_str() {
                    out.push_str(&format!("  ⚑ {s}\n"));
                }
            }
        }
    }
    if !directives.is_empty() {
        out.push_str("Binding directives:\n");
        for d in &directives {
            out.push_str(&format!("⚑ {d}\n"));
        }
    }
    if !pending.is_empty() {
        out.push_str("Pending plan work here:\n");
        for p in &pending {
            let label = p["label"].as_str().unwrap_or("?");
            let kinds: Vec<&str> = p["changes"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|c| c["type"].as_str())
                .collect();
            out.push_str(&format!("- {}: {label}\n", kinds.join("+")));
        }
    }
    out.push_str("Keep edits consistent with these claims, or update the model as you go.");
    Some(out)
}

fn stop(r: &ModelRef, event: &serde_json::Value) {
    // Never block twice: a prior block already told the agent what to do, and
    // the flag is Claude Code's own infinite-loop guard.
    if event["stop_hook_active"].as_bool() == Some(true) {
        return;
    }
    let Some(session) = session_id(event) else { return };
    let close = scryer_core::session::close_gate(r, session, |files| anchor_flags(r, files));
    let close = serde_json::to_value(&close).unwrap_or_default();

    // The close view already did the discrimination work: `needsReconcile`
    // holds only touched files whose anchor fingerprints report the modeled
    // spans changed, broken, or missing. Clean-modeled and unmodeled touches
    // owe nothing — a session that edited around the claims stops freely.
    let needs = close["needsReconcile"].as_array().cloned().unwrap_or_default();
    if needs.is_empty() {
        return;
    }

    let mut lines: Vec<String> = Vec::new();
    for f in &needs {
        let file = f["file"].as_str().unwrap_or("?");
        lines.push(format!("- {file}:"));
        for c in f["claims"].as_array().into_iter().flatten() {
            let host = c["host"].as_str().unwrap_or("?");
            let statement = c["statement"].as_str().unwrap_or("(data shape declaration)");
            let state = c["state"].as_str().unwrap_or("changed");
            lines.push(format!("    [{state}] ({host}) {statement}"));
        }
    }

    let reason = format!(
        "Scryer close gate — this session's edits reached the anchored span(s) of {} claim(s) \
         in {} file(s):\n{}\nBefore stopping, reconcile each: if the claim still describes the \
         code, no write is needed; if behaviour changed, update the model over MCP (update_nodes \
         to reword the claim, update_source_map to re-anchor, mark_implemented to fold finished \
         plan work, flag_drift for new undescribed behaviour). Then finish — this gate fires \
         only once per session.",
        needs
            .iter()
            .map(|f| f["claims"].as_array().map(Vec::len).unwrap_or(0))
            .sum::<usize>(),
        needs.len(),
        lines.join("\n"),
    );
    emit(&serde_json::json!({ "decision": "block", "reason": reason }));
}

/// Out-of-sync anchors in the session's touched files (may silently re-anchor
/// moved symbols, exactly like get_health). No baseline yet → no flags → the
/// gate stays silent rather than crying wolf on a fresh model.
fn anchor_flags(r: &ModelRef, files: &[String]) -> Vec<scryer_core::session::AnchorFlag> {
    let files = files.iter().cloned().collect();
    let Ok(check) = scryer_extract::anchors::check_anchors_in(r, &files) else { return Vec::new() };
    check
        .observations
        .into_iter()
        .map(|o| scryer_core::session::AnchorFlag {
            state: serde_json::to_value(o.state)
                .ok()
                .and_then(|v| v.as_str().map(str::to_string))
                .unwrap_or_else(|| "changed".into()),
            key: o.key,
            host_name: o.host_name,
            file: o.file,
            symbol: o.symbol,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The envelope parser lifts every named file exactly once — add, update,
    /// a rename's move-to target, delete — and reads the same envelope out of
    /// a Bash heredoc wrapper, since newer Codex routes edits through Bash.
    #[test]
    fn envelope_files_parses_native_and_heredoc_patches() {
        let envelope = "*** Begin Patch\n\
                        *** Add File: src/new.rs\n\
                        +fn hello() {}\n\
                        *** Update File: src/lib.rs\n\
                        *** Move to: src/renamed.rs\n\
                        @@ fn old\n\
                        -a\n\
                        +b\n\
                        *** Update File: src/lib.rs\n\
                        *** Delete File: src/gone.rs\n\
                        *** End Patch";
        assert_eq!(
            envelope_files(envelope),
            vec!["src/new.rs", "src/lib.rs", "src/renamed.rs", "src/gone.rs"],
            "each file once, rename target included"
        );

        let heredoc = format!("apply_patch <<'PATCH'\n{envelope}\nPATCH");
        assert_eq!(envelope_files(&heredoc).len(), 4, "heredoc wrapper parses the same");

        assert!(
            envelope_files("cargo test && git status").is_empty(),
            "an ordinary command is not an envelope"
        );
    }

    /// Envelope paths are cwd-relative; the endpoint strips the project prefix
    /// from absolute paths, so the client absolutizes against the event's cwd —
    /// and leaves already-absolute paths alone.
    #[test]
    fn patched_files_absolutizes_against_the_events_cwd() {
        let event = serde_json::json!({
            "tool_name": "apply_patch",
            "cwd": "/repo/sub",
            "tool_input": {
                "command": "*** Begin Patch\n*** Update File: src/lib.rs\n*** End Patch"
            }
        });
        assert_eq!(patched_files(&event), vec!["/repo/sub/src/lib.rs"]);

        let event = serde_json::json!({
            "tool_name": "Bash",
            "cwd": "/repo",
            "tool_input": {
                "command": "apply_patch <<'EOF'\n*** Begin Patch\n*** Update File: /repo/src/lib.rs\n*** End Patch\nEOF"
            }
        });
        assert_eq!(patched_files(&event), vec!["/repo/src/lib.rs"], "absolute path untouched");
    }

    /// An overlay with no claims, directives, pending work or placement — a
    /// dark file — renders to nothing: noise there teaches the agent to skim.
    #[test]
    fn an_empty_overlay_renders_to_nothing() {
        assert_eq!(render_overlay(&serde_json::json!({ "file": "src/lib.rs" })), None);
        assert_eq!(render_overlay(&serde_json::json!({})), None);
        assert!(render_overlay(&serde_json::json!({
            "file": "src/lib.rs",
            "claims": [{ "hostName": "API", "statement": "serves requests" }]
        }))
        .is_some());
    }

}
