//! `scryer-mcp hook` — the Claude Code session-hook client.
//!
//! Claude Code invokes this once per hook event with the event JSON on stdin.
//! The client bridges the event to the desktop app's loopback endpoint
//! (advertised in `.scryer/hook.json` while the app has the project open):
//!
//! - SessionStart      → GET /status   → inject the model's status line
//! - PostToolUse Read  → GET /overlay  → inject the file's governing intent
//! - PostToolUse Edit… → POST /touch   → record the touch, say nothing
//! - Stop              → GET /close    → block once with unreconciled claims
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

pub fn run_hook_client() -> Result<(), Box<dyn std::error::Error>> {
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
        "SessionStart" => session_start(&endpoint),
        "PostToolUse" => post_tool_use(&endpoint, &event),
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
            let v: serde_json::Value = serde_json::from_str(&raw).ok()?;
            return Some(Endpoint {
                port: v["port"].as_u64()? as u16,
                token: v["token"].as_str()?.to_string(),
            });
        }
        dir = d.parent().map(Path::to_path_buf);
    }
    None
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

fn session_start(ep: &Endpoint) {
    let Some(status) = call(ep, "GET", "/status", "") else { return };
    let Some(line) = status["statusLine"].as_str() else { return };
    let context = format!(
        "{line}\nThe Scryer app is open on this project, so its architecture model is live and \
         binding. As you read files, the claims and directives governing them are injected \
         automatically; `locate {{file}}` (MCP) answers on demand, `get_pending` lists \
         outstanding plan work."
    );
    emit(&serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "SessionStart",
            "additionalContext": context,
        }
    }));
}

fn post_tool_use(ep: &Endpoint, event: &serde_json::Value) {
    let tool = event["tool_name"].as_str().unwrap_or_default();
    let Some(file) = event["tool_input"]["file_path"].as_str() else { return };

    match tool {
        "Read" => {
            let Some(overlay) = call(
                ep,
                "GET",
                &format!("/overlay?file={}", percent_encode(file)),
                "",
            ) else {
                return;
            };
            if let Some(text) = render_overlay(&overlay) {
                emit(&serde_json::json!({
                    "hookSpecificOutput": {
                        "hookEventName": "PostToolUse",
                        "additionalContext": text,
                    }
                }));
            }
        }
        "Edit" | "Write" | "NotebookEdit" => {
            let session = event["session_id"].as_str().unwrap_or_default();
            let body = serde_json::json!({ "session": session, "file": file }).to_string();
            let _ = call(ep, "POST", "/touch", &body);
            // No output: touch recording must cost the session zero tokens.
        }
        _ => {}
    }
}

/// The compact intent overlay for one file — or `None` when the model has
/// nothing to say about it (dark files stay silent; noise here would teach
/// the agent to ignore the channel).
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
    if claims.is_empty() && directives.is_empty() && pending.is_empty() {
        return None;
    }

    let file = overlay["file"].as_str().unwrap_or("this file");
    let mut out = String::new();
    match overlay["path"].as_str() {
        Some(p) => out.push_str(&format!("[scryer] {file} — {p}\n")),
        None => out.push_str(&format!("[scryer] {file}\n")),
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

    // Only files that carry anchored claims gate the stop — touching dark or
    // unmodeled files owes the model nothing.
    let mut lines: Vec<String> = Vec::new();
    for f in close["files"].as_array().into_iter().flatten() {
        let claims = f["claims"].as_array().cloned().unwrap_or_default();
        if claims.is_empty() {
            continue;
        }
        let file = f["file"].as_str().unwrap_or("?");
        lines.push(format!("- {file}:"));
        for c in &claims {
            let host = c["host"].as_str().unwrap_or("?");
            let statement = c["statement"].as_str().unwrap_or("(data shape)");
            lines.push(format!("    ({host}) {statement}"));
        }
    }
    if lines.is_empty() {
        return;
    }

    let reason = format!(
        "Scryer close gate — this session edited {} modeled file(s) whose claims may no longer \
         match the code:\n{}\nBefore stopping, reconcile each: if the claim still describes the \
         code, no write is needed; if behaviour changed, update the model over MCP (update_nodes \
         to reword the claim, update_source_map to re-anchor, mark_implemented to fold finished \
         plan work, flag_drift for new undescribed behaviour). Then finish — this gate fires \
         only once per session.",
        lines.iter().filter(|l| l.starts_with("- ")).count(),
        lines.join("\n"),
    );
    emit(&serde_json::json!({ "decision": "block", "reason": reason }));
}
