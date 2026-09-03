//! `scryer-mcp hook` — the session-hook client for Claude Code, Codex and
//! Copilot CLI.
//!
//! The harness invokes this once per hook event with the event JSON on stdin.
//! All three name the event fields the same way — `hook_event_name`,
//! `session_id`, `cwd`, `tool_name`, `tool_input` — so one client serves them
//! all, dispatching on the event and tool names. It bridges the event to the
//! desktop app's loopback endpoint (advertised in `.scryer/hook.json` while the
//! app has the project open):
//!
//! - SessionStart      → GET /status   → inject the model's status line
//! - PostToolUse read  → GET /overlay  → inject the file's governing intent
//!                                        (once per session until it changes)
//! - PostToolUse edit… → POST /touch   → record the touch, say nothing
//! - Stop              → GET /close    → block once with unreconciled claims
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
//! Every failure path — no discovery file, endpoint gone, malformed input —
//! exits 0 with no output: installed hooks are inert unless the Scryer app is
//! open. Opening the app is the opt-in; closing it the opt-out.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// Read timeout for endpoint calls. Status/overlay do real model reads and an
/// anchor scan on large repos; the registered hook timeout (10–15 s) is the
/// hard ceiling, this keeps a wedged endpoint from ever reaching it.
const READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(8);
const CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(300);

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
///   `additionalContext` on SessionStart and PostToolUse (only its PreToolUse
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

    let Some(endpoint) = discover(&event) else {
        return Ok(()); // app not open — stay silent
    };

    match event["hook_event_name"].as_str().unwrap_or_default() {
        "SessionStart" => session_start(&endpoint, harness),
        "PreToolUse" => pre_tool_use(&endpoint, &event, harness),
        "PostToolUse" => post_tool_use(&endpoint, &event, harness),
        "Stop" => stop(&endpoint, &event),
        _ => {}
    }
    Ok(())
}

struct Endpoint {
    port: u16,
    token: String,
}

/// Find the live endpoint: `$CLAUDE_PROJECT_DIR` first, then the event's
/// `cwd`, walking up so hooks fired from a subdirectory still find the
/// project's `.scryer/hook.json`.
fn discover(event: &serde_json::Value) -> Option<Endpoint> {
    let start = std::env::var("CLAUDE_PROJECT_DIR")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| event["cwd"].as_str().map(str::to_string))?;
    let mut dir = Some(PathBuf::from(start));
    while let Some(d) = dir {
        let candidate = d.join(".scryer").join("hook.json");
        if let Ok(raw) = std::fs::read_to_string(&candidate) {
            // A present file ends the walk whether or not we trust it: a stale or
            // malformed one is not a reason to keep climbing into a parent
            // project's model.
            return parse_live_endpoint(&raw);
        }
        dir = d.parent().map(Path::to_path_buf);
    }
    None
}

/// Parse a discovery file, returning the endpoint ONLY if the app that wrote it
/// is still alive. A crashed app leaves `.scryer/hook.json` behind with its old
/// port + token; a later local process binding that freed port could otherwise
/// harvest the token this client sends (the client hands it over in the request,
/// so the token is no defense against a squatter) and inject arbitrary text into
/// the agent's context. The pid gate closes that window: no live author, no
/// trust, stay silent.
fn parse_live_endpoint(raw: &str) -> Option<Endpoint> {
    let v: serde_json::Value = serde_json::from_str(raw).ok()?;
    let pid = v["pid"].as_u64()? as u32;
    if !process_alive(pid) {
        return None;
    }
    Some(Endpoint {
        port: v["port"].as_u64()? as u16,
        token: v["token"].as_str()?.to_string(),
    })
}

/// Is `pid` a currently-running process? Probes without signalling.
#[cfg(unix)]
fn process_alive(pid: u32) -> bool {
    // kill(pid, 0): 0 → alive; EPERM → alive but not ours to signal; ESRCH → gone.
    let r = unsafe { libc::kill(pid as libc::pid_t, 0) };
    r == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

#[cfg(windows)]
fn process_alive(pid: u32) -> bool {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
    // OpenProcess by pid fails once the pid is released, so a successful open is
    // a sufficient liveness signal for "the app is still running".
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return false;
        }
        CloseHandle(handle);
        true
    }
}

#[cfg(not(any(unix, windows)))]
fn process_alive(_pid: u32) -> bool {
    true // no portable probe — fail open on exotic targets
}

/// One tiny HTTP exchange against the loopback endpoint. `None` on any
/// failure — the caller treats that as "app not reachable, stay silent".
fn call(ep: &Endpoint, method: &str, target: &str, body: &str) -> Option<serde_json::Value> {
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], ep.port));
    let mut stream = std::net::TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT).ok()?;
    stream.set_read_timeout(Some(READ_TIMEOUT)).ok()?;
    write!(
        stream,
        "{method} {target} HTTP/1.1\r\nHost: localhost\r\nx-scryer-token: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        ep.token,
        body.len(),
    )
    .ok()?;
    let mut response = String::new();
    stream.read_to_string(&mut response).ok()?;
    // "HTTP/1.x 200 …" — parse the status code, don't pin the minor version.
    let status: u16 = response.split_whitespace().nth(1)?.parse().ok()?;
    if status != 200 {
        return None;
    }
    let json_start = response.find("\r\n\r\n")? + 4;
    serde_json::from_str(&response[json_start..]).ok()
}

fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn emit(v: &serde_json::Value) {
    println!("{}", serde_json::to_string(v).unwrap_or_default());
}

fn session_start(ep: &Endpoint, harness: Harness) {
    let Some(status) = call(ep, "GET", "/status", "") else { return };
    let Some(line) = status["statusLine"].as_str() else { return };
    // Harness-neutral wording: on Claude Code and Copilot the overlay arrives
    // as files are read, on Codex as they are edited — "work in" covers all
    // three truthfully.
    let context = format!(
        "{line}\nThe Scryer app is open on this project, so its architecture model is live and \
         binding. The claims and directives governing a file are injected automatically as you \
         work in it; `locate {{file}}` (MCP) answers on demand, `get_pending` lists \
         outstanding plan work."
    );
    harness.emit_context("SessionStart", &context);
}

/// The file a tool call names. `file_path` is Claude Code's and Codex's key,
/// `path` Copilot's; accepting both keeps one lookup for every harness.
fn tool_file(event: &serde_json::Value) -> Option<&str> {
    event["tool_input"]["file_path"]
        .as_str()
        .or_else(|| event["tool_input"]["path"].as_str())
}

/// The session id an event carries, if any. Copilot sends none on some events;
/// an absent or empty id means "no session" and the endpoint skips its
/// per-session bookkeeping for the request.
fn session_id(event: &serde_json::Value) -> Option<&str> {
    event["session_id"].as_str().filter(|s| !s.is_empty())
}

/// The `/overlay` request target for one file. The session rides along so the
/// endpoint can stay silent when this session already saw an identical overlay
/// for the file — the same block re-injected on every re-read costs context and
/// teaches the agent to skim the channel. No session, no parameter: the
/// endpoint then injects every time.
fn overlay_target(file: &str, session: Option<&str>) -> String {
    let mut target = format!("/overlay?file={}", percent_encode(file));
    if let Some(session) = session {
        target.push_str(&format!("&session={}", percent_encode(session)));
    }
    target
}

fn post_tool_use(ep: &Endpoint, event: &serde_json::Value, harness: Harness) {
    match harness.tool_kind(event["tool_name"].as_str().unwrap_or_default()) {
        ToolKind::Read => {
            let Some(file) = tool_file(event) else { return };
            let Some(overlay) =
                call(ep, "GET", &overlay_target(file, session_id(event)), "")
            else {
                return;
            };
            if let Some(text) = render_overlay(&overlay) {
                harness.emit_context("PostToolUse", &text);
            }
        }
        ToolKind::Write => {
            let Some(file) = tool_file(event) else { return };
            touch(ep, event, file);
        }
        ToolKind::Patch => {
            for file in patched_files(event) {
                touch(ep, event, &file);
            }
        }
        ToolKind::Other => {}
    }
}

/// Record one touched file. No output: touch recording must cost the session
/// zero tokens.
fn touch(ep: &Endpoint, event: &serde_json::Value, file: &str) {
    let session = event["session_id"].as_str().unwrap_or_default();
    let body = serde_json::json!({ "session": session, "file": file }).to_string();
    let _ = call(ep, "POST", "/touch", &body);
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
fn pre_tool_use(ep: &Endpoint, event: &serde_json::Value, harness: Harness) {
    let mut sections: Vec<String> = Vec::new();
    let session = session_id(event);
    for file in patched_files(event).iter().take(OVERLAY_FILE_CAP) {
        let Some(overlay) = call(ep, "GET", &overlay_target(file, session), "") else {
            continue;
        };
        if let Some(text) = render_overlay(&overlay) {
            sections.push(text);
        }
    }
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
/// the agent to ignore the channel). The endpoint answers a repeat request —
/// same session, same file, unchanged payload — with an overlay that has no
/// claims, directives or pending work, so the same `None` keeps it silent.
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

fn stop(ep: &Endpoint, event: &serde_json::Value) {
    // Never block twice: a prior block already told the agent what to do, and
    // the flag is Claude Code's own infinite-loop guard.
    if event["stop_hook_active"].as_bool() == Some(true) {
        return;
    }
    let session = event["session_id"].as_str().unwrap_or_default();
    let Some(close) = call(
        ep,
        "GET",
        &format!("/close?session={}", percent_encode(session)),
        "",
    ) else {
        return;
    };

    // The endpoint already did the discrimination work: `needsReconcile`
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn own_process_reads_as_alive() {
        assert!(process_alive(std::process::id()));
    }

    /// A live author yields the endpoint; a discovery file with no pid — or one
    /// whose author has exited — is not trusted, so the token is never offered.
    #[test]
    fn only_a_live_author_is_trusted() {
        let live =
            serde_json::json!({ "port": 42, "token": "tok", "pid": std::process::id() }).to_string();
        let ep = parse_live_endpoint(&live).expect("live pid → endpoint");
        assert_eq!(ep.port, 42);
        assert_eq!(ep.token, "tok");

        let no_pid = serde_json::json!({ "port": 42, "token": "tok" }).to_string();
        assert!(parse_live_endpoint(&no_pid).is_none(), "a file with no pid is stale");
    }

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

    /// The overlay request names the session when the event carries one
    /// (percent-encoded, so the endpoint can dedupe per session) and omits the
    /// parameter otherwise — a session-less harness must still get the overlay.
    #[test]
    fn overlay_target_carries_the_session_when_present() {
        assert_eq!(
            overlay_target("/repo/src/lib.rs", Some("sess 1")),
            "/overlay?file=/repo/src/lib.rs&session=sess%201"
        );
        assert_eq!(overlay_target("/repo/src/lib.rs", None), "/overlay?file=/repo/src/lib.rs");

        let event = serde_json::json!({ "session_id": "abc", "tool_input": {} });
        assert_eq!(session_id(&event), Some("abc"));
        assert_eq!(session_id(&serde_json::json!({ "session_id": "" })), None, "empty id is no id");
        assert_eq!(session_id(&serde_json::json!({})), None);
        assert!(overlay_target("f.rs", session_id(&event)).ends_with("&session=abc"));
    }

    /// An overlay payload with no claims, directives or pending work — what the
    /// endpoint returns for a repeat request — renders to nothing, so the dedupe
    /// on the server side keeps the client silent without a client-side rule.
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

    /// A reaped child's pid is dead, so its (fabricated) discovery file is
    /// rejected — the crash-then-squat scenario the pid gate exists to block.
    #[cfg(unix)]
    #[test]
    fn a_dead_authors_file_is_rejected() {
        let Ok(mut child) = std::process::Command::new("true").spawn() else {
            return; // no `true` on PATH in this sandbox — skip
        };
        let pid = child.id();
        let _ = child.wait(); // reap → pid now gone (barring immediate reuse)
        let raw = serde_json::json!({ "port": 1, "token": "t", "pid": pid }).to_string();
        assert!(parse_live_endpoint(&raw).is_none());
    }
}
