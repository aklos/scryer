//! The session log — what one coding-agent session did in this project.
//!
//! Hooks and the MCP server append events as the session works; the app reads
//! the same file to show what the session is changing. The log is append-only:
//! state is the fold of its events ([`SessionLog::from_events`]), never a
//! mutable record, so a writer racing another writer can only add a line.
//!
//! One file per session id, so a resumed session (same id) keeps its history
//! and a new one starts clean — no TTL, no pruning. A line this build does not
//! know (an event an older or newer scryer wrote) is skipped on read.

use serde::{Deserialize, Serialize};

/// One line of the session file: when, and what.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionEntry {
    /// Unix seconds.
    pub at: u64,
    #[serde(flatten)]
    pub event: SessionEvent,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "event", rename_all = "camelCase")]
pub enum SessionEvent {
    /// The session edited this project-relative file.
    Touch { file: String },
    /// The read overlay for `file` was served with this payload hash.
    Overlay { file: String, hash: u64 },
    /// The task the agent oriented on, in its own words — the session's
    /// title. Only the first is kept.
    Task { text: String },
    /// The developer's first prompt, clipped: the session's title when the
    /// agent never oriented. Only the first is kept.
    Prompt { text: String },
    /// A shell command is about to run: files modified after `ns` (unix
    /// nanoseconds) and before it finishes are the session's edits. `tool` is
    /// the harness's tool-call id, so concurrent commands pair up.
    ShellStart {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tool: Option<String>,
        ns: u64,
    },
    /// The session opened a probe; until it closes, the developer's tree is
    /// off-limits to commands that discard uncommitted work.
    ProbeOpen,
    ProbeClose,
    /// The agent stopped — the end of a turn, not necessarily of the session.
    Stop,
    /// The todo tests in the project when this was recorded — at the
    /// session's start and at each stop — as [`test_key`]s, so the next stop
    /// can tell the stubs this session wrote from ones that were there.
    Planned {
        #[serde(default)]
        tests: Vec<String>,
    },
    /// This session wrote this todo test: the work is its until the test
    /// passes. The owner record; the test's own text carries no tag.
    Stub { file: String, test: String },
    /// The claims this session's edits reach (derived claim ids), as of its
    /// last stop.
    Reached {
        #[serde(default)]
        ids: Vec<String>,
    },
}

/// One test as the session log names it: `file#junit_name`. A JUnit name is
/// unique only within its file.
pub fn test_key(file: &str, junit_name: &str) -> String {
    format!("{file}#{junit_name}")
}

/// The fold of a session's events.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SessionLog {
    /// Files edited, first-touch order, each once.
    pub touched: Vec<String>,
    /// The last overlay hash served per file.
    pub overlays: Vec<(String, u64)>,
    /// Whether the session has a probe open.
    pub probe_open: bool,
    /// Shell commands started, `(tool-call id, unix ns)`, in start order.
    pub shell_starts: Vec<(Option<String>, u64)>,
    /// The first task the agent oriented on.
    pub task: Option<String>,
    /// The developer's first prompt.
    pub prompt: Option<String>,
    /// The todo tests last recorded (at start or the last stop), as
    /// [`test_key`]s; `None` until the first record.
    pub planned_seen: Option<Vec<String>>,
    /// The todo tests this session wrote, `(file, junit_name)`, each once.
    pub stubs: Vec<(String, String)>,
    /// The claims its edits reached at the last stop.
    pub reached: Vec<String>,
}

impl SessionLog {
    pub fn from_events<'a>(entries: impl IntoIterator<Item = &'a SessionEntry>) -> Self {
        let mut log = SessionLog::default();
        for e in entries {
            log.apply(&e.event);
        }
        log
    }

    pub fn apply(&mut self, event: &SessionEvent) {
        match event {
            SessionEvent::Touch { file } => {
                if !self.touched.contains(file) {
                    self.touched.push(file.clone());
                }
            }
            SessionEvent::Overlay { file, hash } => {
                match self.overlays.iter_mut().find(|(f, _)| f == file) {
                    Some(entry) => entry.1 = *hash,
                    None => self.overlays.push((file.clone(), *hash)),
                }
            }
            SessionEvent::ProbeOpen => self.probe_open = true,
            SessionEvent::ProbeClose => self.probe_open = false,
            SessionEvent::Stop => {}
            SessionEvent::Planned { tests } => self.planned_seen = Some(tests.clone()),
            SessionEvent::Stub { file, test } => {
                if !self.stubs.iter().any(|(f, t)| f == file && t == test) {
                    self.stubs.push((file.clone(), test.clone()));
                }
            }
            SessionEvent::Reached { ids } => self.reached = ids.clone(),
            SessionEvent::ShellStart { tool, ns } => self.shell_starts.push((tool.clone(), *ns)),
            SessionEvent::Task { text } => {
                if self.task.is_none() {
                    self.task = Some(text.clone());
                }
            }
            SessionEvent::Prompt { text } => {
                if self.prompt.is_none() {
                    self.prompt = Some(text.clone());
                }
            }
        }
    }

    /// The todo tests in `planned` (`(file, junit_name)`) this session wrote
    /// since its last record: in a file it touched, absent from what was
    /// there at its start or last stop, and not already its own.
    pub fn new_stubs<'p>(&self, planned: &'p [(String, String)]) -> Vec<&'p (String, String)> {
        let seen = self.planned_seen.as_deref().unwrap_or_default();
        planned
            .iter()
            .filter(|(file, test)| {
                self.touched.contains(file)
                    && !seen.contains(&test_key(file, test))
                    && !self.stubs.iter().any(|(f, t)| f == file && t == test)
            })
            .collect()
    }

    /// What the session is called: the task it oriented on, else the
    /// developer's first prompt.
    pub fn title(&self) -> Option<&str> {
        self.task.as_deref().or(self.prompt.as_deref())
    }

    /// When the shell command `tool` started: its own start when the harness
    /// names the call, else the latest start.
    pub fn shell_started(&self, tool: Option<&str>) -> Option<u64> {
        tool.and_then(|t| {
            self.shell_starts
                .iter()
                .rev()
                .find(|(id, _)| id.as_deref() == Some(t))
                .map(|(_, ns)| *ns)
        })
        .or_else(|| self.shell_starts.last().map(|(_, ns)| *ns))
    }

    /// Whether `hash` is exactly what this session was last shown for `file` —
    /// a repeat that should stay silent. A changed payload is not a repeat.
    pub fn overlay_is_repeat(&self, file: &str, hash: u64) -> bool {
        self.overlays.iter().any(|(f, h)| f == file && *h == hash)
    }
}

/// FNV-1a, 64-bit. Not cryptographic — it only tells "same payload as last
/// time" from "different", and a collision costs one skipped overlay.
pub fn fnv1a64(bytes: &[u8]) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    bytes.iter().fold(OFFSET, |h, b| (h ^ u64::from(*b)).wrapping_mul(PRIME))
}

/// Normalize a harness-given path to the model's project-relative,
/// `/`-separated convention.
pub fn relativize(project: &std::path::Path, file: &str) -> String {
    let file = file.replace('\\', "/");
    let root = project.to_string_lossy().replace('\\', "/");
    let rel = file
        .strip_prefix(root.as_str())
        .map(|r| r.trim_start_matches('/'))
        .unwrap_or(&file);
    rel.trim_start_matches("./").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(event: SessionEvent) -> SessionEntry {
        SessionEntry { at: 1, event }
    }

    #[test]
    fn events_round_trip_as_one_json_line_each() {
        let e = entry(SessionEvent::Touch { file: "src/a.rs".into() });
        let line = serde_json::to_string(&e).unwrap();
        assert_eq!(line, r#"{"at":1,"event":"touch","file":"src/a.rs"}"#);
        assert_eq!(serde_json::from_str::<SessionEntry>(&line).unwrap(), e);
        let stop = serde_json::to_string(&entry(SessionEvent::Stop)).unwrap();
        assert_eq!(stop, r#"{"at":1,"event":"stop"}"#);
    }

    #[test]
    fn the_fold_dedupes_touches_and_keeps_the_latest_overlay() {
        let events = [
            entry(SessionEvent::Touch { file: "a.rs".into() }),
            entry(SessionEvent::Touch { file: "b.rs".into() }),
            entry(SessionEvent::Touch { file: "a.rs".into() }),
            entry(SessionEvent::Overlay { file: "a.rs".into(), hash: 1 }),
            entry(SessionEvent::Overlay { file: "a.rs".into(), hash: 2 }),
        ];
        let log = SessionLog::from_events(&events);
        assert_eq!(log.touched, vec!["a.rs", "b.rs"]);
        assert!(log.overlay_is_repeat("a.rs", 2));
        assert!(!log.overlay_is_repeat("a.rs", 1), "a changed payload re-fires");
        assert!(!log.overlay_is_repeat("b.rs", 2), "per file");
    }

    /// When a stub appeared since the last record in a file the session touched, counts it as the session's, and with no record yet, every stub in a touched file.
    #[test]
    fn new_stubs_are_the_ones_since_the_last_record_in_touched_files() {
        let planned = |tests: &[(&str, &str)]| -> Vec<(String, String)> {
            tests.iter().map(|(f, t)| (f.to_string(), t.to_string())).collect()
        };
        let now = planned(&[("a.rs", "old"), ("a.rs", "new"), ("b.rs", "elsewhere")]);
        let mut log = SessionLog::from_events(&[entry(SessionEvent::Touch { file: "a.rs".into() })]);
        assert_eq!(log.new_stubs(&now).len(), 2, "no record yet: both stubs in a.rs");

        log.apply(&SessionEvent::Planned { tests: vec![test_key("a.rs", "old")] });
        let fresh: Vec<&str> = log.new_stubs(&now).iter().map(|(_, t)| t.as_str()).collect();
        assert_eq!(fresh, ["new"]);

        log.apply(&SessionEvent::Stub { file: "a.rs".into(), test: "new".into() });
        assert!(log.new_stubs(&now).is_empty(), "already owned");
        assert_eq!(log.stubs, vec![("a.rs".to_string(), "new".to_string())]);
    }

    #[test]
    fn fnv1a64_matches_reference_vectors() {
        assert_eq!(fnv1a64(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a64(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a64(b"foobar"), 0x8594_4171_f739_67e8);
    }

    #[test]
    fn relativize_strips_the_project_root() {
        let root = std::path::Path::new("/repo");
        assert_eq!(relativize(root, "/repo/src/a.rs"), "src/a.rs");
        assert_eq!(relativize(root, "./src/a.rs"), "src/a.rs");
        assert_eq!(relativize(root, "src\\a.rs"), "src/a.rs");
    }
}
