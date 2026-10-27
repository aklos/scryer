//! The session log — what one coding-agent session did in this project. [domain]
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
    /// A shell command is about to run: files modified after `ns` (unix
    /// nanoseconds) and before it finishes are the session's edits. `tool` is
    /// the harness's tool-call id, so concurrent commands pair up.
    ShellStart {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tool: Option<String>,
        ns: u64,
    },
    /// That shell command finished at `ns`: with its start, the window in
    /// which files it modified may be this session's — and so are no other
    /// session's.
    ShellEnd {
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
    /// The claims proven when the session began (derived claim ids): what
    /// it proves after is its own, and what of these it breaks.
    Proven {
        #[serde(default)]
        ids: Vec<String>,
    },
    /// This session ingested verdicts for these claims (derived claim ids).
    Verdicts {
        #[serde(default)]
        ids: Vec<String>,
    },
}

/// How long a session's last work keeps it working without a turn's end:
/// longer than a slow build, shorter than a lunch break.
pub const WORKING_SECS: u64 = 600;

/// Whether the session in `entries` is mid-turn at `now`: it read, edited,
/// ran a command, wrote a todo test or worked a probe since its last stop,
/// and did so in the last [`WORKING_SECS`] — a session killed mid-turn never
/// writes its stop. The records a stop writes are no work.
pub fn working(entries: &[SessionEntry], now: u64) -> bool {
    let after_stop = entries.iter().rposition(|e| e.event == SessionEvent::Stop).map_or(0, |i| i + 1);
    entries[after_stop..]
        .iter()
        .filter(|e| {
            !matches!(e.event, SessionEvent::Planned { .. } | SessionEvent::Reached { .. } | SessionEvent::Proven { .. })
        })
        .map(|e| e.at)
        .max()
        .is_some_and(|at| at + WORKING_SECS >= now)
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
    /// Shell commands finished, `(tool-call id, unix ns)`, in end order.
    pub shell_ends: Vec<(Option<String>, u64)>,
    /// The first task the agent oriented on.
    pub task: Option<String>,
    /// The todo tests last recorded (at start or the last stop), as
    /// [`test_key`]s; `None` until the first record.
    pub planned_seen: Option<Vec<String>>,
    /// The todo tests this session wrote, `(file, junit_name)`, each once.
    pub stubs: Vec<(String, String)>,
    /// The claims its edits reached at the last stop.
    pub reached: Vec<String>,
    /// The claims proven when it began; `None` for a log from before the
    /// baseline was recorded.
    pub proven_at_start: Option<Vec<String>>,
    /// The claims it ingested verdicts for, each once.
    pub verdicts: Vec<String>,
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
            SessionEvent::Proven { ids } => {
                if self.proven_at_start.is_none() {
                    self.proven_at_start = Some(ids.clone());
                }
            }
            SessionEvent::Verdicts { ids } => {
                for id in ids {
                    if !self.verdicts.contains(id) {
                        self.verdicts.push(id.clone());
                    }
                }
            }
            SessionEvent::ShellStart { tool, ns } => self.shell_starts.push((tool.clone(), *ns)),
            SessionEvent::ShellEnd { tool, ns } => self.shell_ends.push((tool.clone(), *ns)),
            SessionEvent::Task { text } => {
                if self.task.is_none() {
                    self.task = Some(text.clone());
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

    /// Each shell command's window, `(start, end)` in unix ns: a start pairs
    /// with the first end of the same tool call after it. One with no end yet
    /// is still running, up to `now` but no longer than `open_for` — a
    /// session killed mid-command never writes its end.
    pub fn shell_windows(&self, now: u64, open_for: u64) -> Vec<(u64, u64)> {
        self.shell_starts
            .iter()
            .map(|(tool, start)| {
                let end = self.shell_ends.iter().find(|(t, ns)| t == tool && ns >= start).map(|(_, ns)| *ns);
                (*start, end.unwrap_or_else(|| now.min(start + open_for)))
            })
            .collect()
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

    /// While a session has worked since its last stop, and lately, it is
    /// working; a stop, the records a stop writes, or ten quiet minutes end
    /// it.
    #[test]
    fn a_session_works_from_its_first_step_after_a_stop_until_the_next() {
        let at = |at, event| SessionEntry { at, event };
        let touch = |t| at(t, SessionEvent::Touch { file: "a.rs".into() });
        assert!(!working(&[], 100));
        assert!(working(&[touch(90)], 100));
        assert!(!working(&[touch(90), at(95, SessionEvent::Stop)], 100), "its turn ended");
        assert!(
            !working(&[touch(90), at(95, SessionEvent::Reached { ids: vec![] }), at(95, SessionEvent::Stop), at(96, SessionEvent::Planned { tests: vec![] })], 100),
            "bookkeeping is no work"
        );
        assert!(working(&[touch(90), at(95, SessionEvent::Stop), at(98, SessionEvent::ShellStart { tool: None, ns: 0 })], 100));
        assert!(!working(&[touch(90)], 90 + WORKING_SECS + 1), "killed mid-turn");
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
