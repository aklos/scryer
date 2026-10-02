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
    /// The Stop gate blocked on unreconciled anchors — at most once per session.
    ReconcileGate,
    /// The agent wrote these plan elements (change-map keys).
    ModelEdit { keys: Vec<String> },
    /// The summary last shown to the user, so an unchanged one stays silent.
    Summary { text: String },
    /// The Stop gate blocked on these unfolded plan entries, which carried
    /// no progress note. Each is blocked on at most once.
    PendingGate {
        #[serde(default)]
        keys: Vec<String>,
    },
    /// The Stop gate blocked until this session's new tests were
    /// mutation-probed — at most once per session.
    ProbeGate,
    /// A shell command is about to run: files modified after `ns` (unix
    /// nanoseconds) and before it finishes are the session's edits. `tool` is
    /// the harness's tool-call id, so concurrent commands pair up.
    ShellStart {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tool: Option<String>,
        ns: u64,
    },
}

/// The fold of a session's events.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SessionLog {
    /// Files edited, first-touch order, each once.
    pub touched: Vec<String>,
    /// The last overlay hash served per file.
    pub overlays: Vec<(String, u64)>,
    /// Whether the reconcile gate already fired.
    pub reconcile_gated: bool,
    /// Unfolded plan entries the Stop gate already blocked on.
    pub gated_pending: Vec<String>,
    /// Whether the probe gate already fired.
    pub probe_gated: bool,
    /// Plan elements the agent wrote, first-write order, each once.
    pub model_edits: Vec<String>,
    pub last_summary: Option<String>,
    /// Shell commands started, `(tool-call id, unix ns)`, in start order.
    pub shell_starts: Vec<(Option<String>, u64)>,
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
            SessionEvent::ReconcileGate => self.reconcile_gated = true,
            SessionEvent::ModelEdit { keys } => {
                for k in keys {
                    if !self.model_edits.contains(k) {
                        self.model_edits.push(k.clone());
                    }
                }
            }
            SessionEvent::Summary { text } => self.last_summary = Some(text.clone()),
            SessionEvent::PendingGate { keys } => self.gated_pending.extend(keys.iter().cloned()),
            SessionEvent::ProbeGate => self.probe_gated = true,
            SessionEvent::ShellStart { tool, ns } => self.shell_starts.push((tool.clone(), *ns)),
        }
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
        let gate = serde_json::to_string(&entry(SessionEvent::ReconcileGate)).unwrap();
        assert_eq!(gate, r#"{"at":1,"event":"reconcileGate"}"#);
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
        assert!(!log.reconcile_gated);
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
