//! What other sessions have at stake in a file or a claim, and how to reach
//! them before touching it. [composition]
//!
//! Only facts that hold whether or not the other session is still running:
//! edits it made that are still uncommitted, todo tests it wrote that are
//! still todo, the last verdict it recorded for a claim. "Who is editing
//! this now" was tried and dropped: an open terminal kept every file a
//! session ever touched live, so the warnings were mostly false.
//!
//! Scryer carries no messages. Each fact names how the session can be
//! reached through its own harness (Claude Code by its peer name while it
//! runs, Codex by `codex queue` on its id) and otherwise points at a
//! comment thread, which every session sees in orient.

use crate::c4_model::ModelRef;
use crate::session_log::{SessionEntry, SessionEvent, SessionLog};
use crate::stores::harness_registry::{self, RunningSession};
use crate::stores as session_store;

/// How another session can be reached.
#[derive(Debug, Clone, PartialEq)]
pub enum Reach {
    /// A running Claude Code session, messaged by its peer name.
    Message { name: String, status: Option<String> },
    /// A Codex session, sent a message with `codex queue` on its id.
    CodexQueue { thread: String },
    /// No harness route: a comment thread on the code.
    Comment,
}

impl Reach {
    /// How to reach the session, as an agent reads it.
    pub fn how(&self) -> String {
        match self {
            Reach::Message { name, status: Some(status) } => format!("message {name} ({status})"),
            Reach::Message { name, status: None } => format!("message {name}"),
            Reach::CodexQueue { thread } => {
                format!("codex queue --thread {thread} --message \"…\", or a comment thread if it has ended")
            }
            Reach::Comment => "open a comment thread on the code (it is not running)".to_string(),
        }
    }
}

/// Another session, as a fact names it.
#[derive(Debug, Clone, PartialEq)]
pub struct Peer {
    pub session: String,
    /// The task it last oriented on.
    pub task: Option<String>,
    pub reach: Reach,
}

impl Peer {
    /// The session as a sentence names it: by the task it oriented on.
    pub fn who(&self) -> String {
        match &self.task {
            Some(task) => format!("another session (\"{task}\")"),
            None => "another session".to_string(),
        }
    }
}

/// How a session in `harness` with id `session` is reached, given what its
/// harness reports `running`.
pub fn reach_for(harness: Option<&str>, session: &str, running: impl FnOnce(&str) -> Option<RunningSession>) -> Reach {
    match harness {
        Some("codex") => Reach::CodexQueue { thread: session.to_string() },
        Some("copilot") => Reach::Comment,
        _ => running(session).map_or(Reach::Comment, |s| Reach::Message { name: s.name, status: s.status }),
    }
}

/// What another session has at stake in one file.
#[derive(Debug, Clone, PartialEq)]
pub struct FileStake {
    pub peer: Peer,
    /// It edited the file and those edits are not committed yet.
    pub uncommitted: bool,
    /// The todo tests it wrote in the file that are still todo (JUnit names).
    pub todos: Vec<String>,
}

impl FileStake {
    /// The stake as one line for an agent about to edit the file.
    pub fn line(&self) -> String {
        let mut what = Vec::new();
        if self.uncommitted {
            what.push("has uncommitted edits here".to_string());
        }
        if !self.todos.is_empty() {
            what.push(format!("owns the todo tests {} here", self.todos.join(", ")));
        }
        format!(
            "{} {}: don't revert or delete them; to change them, {}",
            self.peer.who(),
            what.join(" and "),
            self.peer.reach.how()
        )
    }
}

/// What sessions other than `me` have at stake in `file`, given the file's
/// tests that are todo now (`todo_now`, JUnit names).
pub fn stakes_in_file(r: &ModelRef, file: &str, me: Option<&str>, todo_now: &[String]) -> Vec<FileStake> {
    stakes_in_file_with(r, file, me, todo_now, harness_registry::claude_code_session)
}

/// [`stakes_in_file`], asking `running` for a Claude Code session.
pub fn stakes_in_file_with(
    r: &ModelRef,
    file: &str,
    me: Option<&str>,
    todo_now: &[String],
    running: impl Fn(&str) -> Option<RunningSession>,
) -> Vec<FileStake> {
    // Asked of git once, and only when another session touched the file.
    let mut committed: Option<Option<u64>> = None;
    let mut out = Vec::new();
    for (id, _) in session_store::list(r) {
        if Some(id.as_str()) == me {
            continue;
        }
        let entries = session_store::read(r, &id);
        let log = SessionLog::from_events(&entries);
        let todos: Vec<String> = log
            .stubs
            .iter()
            .filter(|(f, t)| f == file && todo_now.contains(t))
            .map(|(_, t)| t.clone())
            .collect();
        let uncommitted = last_touch(&entries, file).is_some_and(|at| {
            let since = *committed.get_or_insert_with(|| crate::probe_tree::uncommitted_since(r.project_path(), file));
            since.is_some_and(|committed_at| at > committed_at)
        });
        if uncommitted || !todos.is_empty() {
            out.push(FileStake { peer: peer(&id, &log, &running), uncommitted, todos });
        }
    }
    out
}

/// The session that recorded the latest verdict for `claim` (a derived
/// claim id), unless that is `me`.
pub fn last_verifier(r: &ModelRef, claim: &str, me: Option<&str>) -> Option<Peer> {
    last_verifier_with(r, claim, me, harness_registry::claude_code_session)
}

/// [`last_verifier`], asking `running` for a Claude Code session.
pub fn last_verifier_with(
    r: &ModelRef,
    claim: &str,
    me: Option<&str>,
    running: impl Fn(&str) -> Option<RunningSession>,
) -> Option<Peer> {
    let (_, id, entries) = session_store::list(r)
        .into_iter()
        .filter_map(|(id, _)| {
            let entries = session_store::read(r, &id);
            let at = entries
                .iter()
                .filter(|e| matches!(&e.event, SessionEvent::Verdicts { ids } if ids.iter().any(|i| i == claim)))
                .map(|e| e.at)
                .max()?;
            Some((at, id, entries))
        })
        .max_by_key(|(at, _, _)| *at)?;
    (Some(id.as_str()) != me).then(|| peer(&id, &SessionLog::from_events(&entries), &running))
}

fn peer(id: &str, log: &SessionLog, running: impl Fn(&str) -> Option<RunningSession>) -> Peer {
    Peer { session: id.to_string(), task: log.task.clone(), reach: reach_for(log.harness.as_deref(), id, running) }
}

/// When the session last touched `file`, in unix seconds.
fn last_touch(entries: &[SessionEntry], file: &str) -> Option<u64> {
    entries
        .iter()
        .filter(|e| matches!(&e.event, SessionEvent::Touch { file: f } if f == file))
        .map(|e| e.at)
        .max()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn git(dir: &std::path::Path, args: &[&str]) {
        let ok = Command::new("git")
            .args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"])
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap()
            .status
            .success();
        assert!(ok, "git {args:?}");
    }

    /// A git project with `src/a.rs` committed.
    fn project() -> (tempfile::TempDir, ModelRef) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/a.rs"), "fn a() {}\n").unwrap();
        std::fs::write(dir.path().join(".gitignore"), ".scryer/\n").unwrap();
        git(dir.path(), &["init", "-q"]);
        git(dir.path(), &["add", "."]);
        git(dir.path(), &["commit", "-qm", "init"]);
        let r = ModelRef::ProjectLocal(dir.path().to_path_buf());
        (dir, r)
    }

    fn at(r: &ModelRef, session: &str, at: u64, event: SessionEvent) {
        let path = session_store::session_path(r, session).unwrap();
        session_store::ensure_dir(r).unwrap();
        let line = serde_json::to_string(&SessionEntry { at, event }).unwrap();
        let mut f = std::fs::OpenOptions::new().create(true).append(true).open(path).unwrap();
        std::io::Write::write_all(&mut f, format!("{line}\n").as_bytes()).unwrap();
    }

    fn now() -> u64 {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs()
    }

    fn running_a1(id: &str) -> Option<RunningSession> {
        (id == "theirs").then(|| RunningSession { name: "scryer-a1".into(), status: Some("idle".into()) })
    }

    /// When another session touched a file that has uncommitted changes since, names it.
    #[test]
    fn another_sessions_uncommitted_edit_is_named() {
        let (dir, r) = project();
        at(&r, "theirs", now() + 5, SessionEvent::Task { text: "rename drafts".into() });
        at(&r, "theirs", now() + 5, SessionEvent::Touch { file: "src/a.rs".into() });
        at(&r, "mine", now() + 5, SessionEvent::Touch { file: "src/a.rs".into() });
        assert!(stakes_in_file_with(&r, "src/a.rs", Some("mine"), &[], running_a1).is_empty(), "the file is clean");

        std::fs::write(dir.path().join("src/a.rs"), "fn a() { 1 }\n").unwrap();
        let stakes = stakes_in_file_with(&r, "src/a.rs", Some("mine"), &[], running_a1);
        assert_eq!(stakes.len(), 1, "never the caller itself: {stakes:?}");
        assert!(stakes[0].uncommitted);
        assert_eq!(stakes[0].peer.session, "theirs");
        assert_eq!(
            stakes[0].line(),
            "another session (\"rename drafts\") has uncommitted edits here: don't revert or delete them; \
             to change them, message scryer-a1 (idle)"
        );
    }

    /// If the file was committed after the other session's touch, then names no one.
    #[test]
    fn a_committed_touch_is_no_ones() {
        let (dir, r) = project();
        at(&r, "theirs", 1, SessionEvent::Touch { file: "src/a.rs".into() });
        std::fs::write(dir.path().join("src/a.rs"), "fn a() { 2 }\n").unwrap();
        assert!(stakes_in_file_with(&r, "src/a.rs", None, &[], running_a1).is_empty(), "touched before the commit");
    }

    /// When another session owns todo tests in a file, names it with those tests.
    #[test]
    fn another_sessions_todo_tests_are_named() {
        let (_dir, r) = project();
        at(&r, "theirs", 1, SessionEvent::Stub { file: "src/a.rs".into(), test: "merges".into() });
        at(&r, "theirs", 1, SessionEvent::Stub { file: "src/a.rs".into(), test: "renames".into() });
        let todo_now = ["merges".to_string()];
        let stakes = stakes_in_file_with(&r, "src/a.rs", None, &todo_now, |_| None);
        assert_eq!(stakes.len(), 1);
        assert_eq!(stakes[0].todos, vec!["merges"], "a stub that passes is no longer theirs to finish");
        assert!(!stakes[0].uncommitted);
        assert_eq!(stakes[0].peer.reach, Reach::Comment);
        assert_eq!(
            stakes[0].line(),
            "another session owns the todo tests merges here: don't revert or delete them; \
             to change them, open a comment thread on the code (it is not running)"
        );
    }

    /// When a claim's last verdict before a run came from another session, names that session.
    #[test]
    fn the_last_verifier_of_a_claim_is_named() {
        let (_dir, r) = project();
        at(&r, "older", 1, SessionEvent::Verdicts { ids: vec!["c1".into()] });
        at(&r, "theirs", 2, SessionEvent::Verdicts { ids: vec!["c1".into(), "c2".into()] });
        at(&r, "mine", 3, SessionEvent::Verdicts { ids: vec!["c2".into()] });
        assert_eq!(last_verifier_with(&r, "c1", Some("mine"), running_a1).map(|p| p.session).as_deref(), Some("theirs"));
        assert_eq!(last_verifier_with(&r, "c2", Some("mine"), running_a1), None, "the caller verified it last");
        assert_eq!(last_verifier_with(&r, "c3", Some("mine"), running_a1), None);
    }

    /// When a session runs in Codex, reaches it with codex queue by its id.
    #[test]
    fn a_codex_session_is_reached_by_codex_queue() {
        let reach = reach_for(Some("codex"), "019a-uuid", |_| panic!("Codex keeps no registry scryer reads"));
        assert_eq!(reach, Reach::CodexQueue { thread: "019a-uuid".into() });
        assert!(reach.how().starts_with("codex queue --thread 019a-uuid --message"));
    }

    /// If a session can't be messaged, then points to a comment thread.
    #[test]
    fn an_unreachable_session_is_reached_by_comment() {
        assert_eq!(reach_for(Some("copilot"), "s", running_a1), Reach::Comment);
        assert_eq!(reach_for(Some("claude-code"), "ended", running_a1), Reach::Comment);
        assert_eq!(reach_for(None, "ended", running_a1), Reach::Comment, "a log from before the harness was recorded");
        assert!(matches!(reach_for(Some("claude-code"), "theirs", running_a1), Reach::Message { .. }));
    }
}
