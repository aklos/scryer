//! The harnesses' own records of their running sessions: whether a session is
//! up and the name another session messages it by. [infrastructure]
//!
//! Only Claude Code keeps one scryer can read: `<config>/sessions/<pid>.json`
//! per running process, with its `sessionId`, the `name` peers message it by
//! and whether it is `busy` or `idle`. It is the harness's internal file, not
//! an API, so every field is read defensively and anything unreadable is no
//! session: the caller falls back to a comment thread.

use std::path::{Path, PathBuf};

/// A session its harness reports running.
#[derive(Debug, Clone, PartialEq)]
pub struct RunningSession {
    /// The name other sessions message it by.
    pub name: String,
    /// `busy` or `idle`, when the harness says.
    pub status: Option<String>,
}

/// Claude Code's config directory: `$CLAUDE_CONFIG_DIR`, else `~/.claude`.
fn claude_config_dir() -> Option<PathBuf> {
    std::env::var_os("CLAUDE_CONFIG_DIR")
        .filter(|d| !d.is_empty())
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(".claude")))
}

/// The running Claude Code session with harness session id `id`, if any.
pub fn claude_code_session(id: &str) -> Option<RunningSession> {
    claude_code_session_in(&claude_config_dir()?.join("sessions"), id, process_alive)
}

/// [`claude_code_session`] against the registry directory `dir`, with
/// `alive` telling whether a pid still runs: a record left by a killed
/// process is no session.
pub fn claude_code_session_in(dir: &Path, id: &str, alive: impl Fn(u32) -> bool) -> Option<RunningSession> {
    std::fs::read_dir(dir).ok()?.flatten().find_map(|entry| {
        if entry.path().extension().and_then(|e| e.to_str()) != Some("json") {
            return None;
        }
        let record: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(entry.path()).ok()?).ok()?;
        if record["sessionId"].as_str() != Some(id) {
            return None;
        }
        let pid = record["pid"].as_u64().and_then(|p| u32::try_from(p).ok())?;
        let name = record["name"].as_str().filter(|n| !n.is_empty())?;
        alive(pid).then(|| RunningSession {
            name: name.to_string(),
            status: record["status"].as_str().map(str::to_string),
        })
    })
}

/// Whether process `pid` runs. Known on Linux; elsewhere the harness's
/// record is trusted.
fn process_alive(pid: u32) -> bool {
    !cfg!(target_os = "linux") || Path::new(&format!("/proc/{pid}")).exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry(records: &[serde_json::Value]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for r in records {
            let pid = r["pid"].as_u64().unwrap();
            std::fs::write(dir.path().join(format!("{pid}.json")), r.to_string()).unwrap();
        }
        dir
    }

    /// When a running Claude Code session carries the id, gives its name and status.
    #[test]
    fn a_running_claude_code_session_gives_its_name() {
        let dir = registry(&[
            serde_json::json!({ "pid": 11, "sessionId": "other", "name": "scryer-b2", "status": "busy" }),
            serde_json::json!({ "pid": 12, "sessionId": "s-1", "name": "scryer-a1", "status": "idle" }),
        ]);
        assert_eq!(
            claude_code_session_in(dir.path(), "s-1", |_| true),
            Some(RunningSession { name: "scryer-a1".into(), status: Some("idle".into()) })
        );
    }

    /// If no running Claude Code session carries the id, then gives none.
    #[test]
    fn an_ended_or_unknown_session_gives_none() {
        let dir = registry(&[serde_json::json!({ "pid": 12, "sessionId": "s-1", "name": "scryer-a1" })]);
        std::fs::write(dir.path().join("13.json"), "{ torn").unwrap();
        assert_eq!(claude_code_session_in(dir.path(), "s-1", |pid| pid != 12), None, "its process is gone");
        assert_eq!(claude_code_session_in(dir.path(), "s-2", |_| true), None, "no record carries the id");
        assert_eq!(claude_code_session_in(&dir.path().join("missing"), "s-1", |_| true), None);
    }
}
