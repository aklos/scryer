//! The session hooks' entry points: read the session's log, answer, and
//! append what the answer implies. The hook client calls these in-process —
//! no app, no server.

use crate::domain::session::{fnv1a64, relativize, test_key, SessionLog};
use std::collections::HashSet;
use crate::infrastructure::session_store;
use crate::ModelRef;

pub use crate::domain::session::{SessionEntry, SessionEvent};
pub use session_store::{ensure_dir as ensure_sessions_dir, list as list_sessions, read as read_session};

/// The fold of `session`'s log so far.
pub fn session_log(r: &ModelRef, session: &str) -> SessionLog {
    SessionLog::from_events(&session_store::read(r, session))
}

/// Record that `session` edited `file` (any path form the harness gave). A
/// file outside the project — a probe worktree, a scratch file — is not the
/// project's code, so it is not recorded.
pub fn record_touch(r: &ModelRef, session: &str, file: &str) -> Result<(), String> {
    let file = relativize(r.project_path(), file);
    if std::path::Path::new(&file).is_absolute() || file.starts_with("../") {
        return Ok(());
    }
    session_store::append(r, session, SessionEvent::Touch { file })
}

/// Mark that a shell command is about to run in `session`. `tool` is the
/// harness's id for the call, when it gives one.
pub fn record_shell_start(r: &ModelRef, session: &str, tool: Option<&str>) -> Result<(), String> {
    let ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or_default();
    session_store::append(r, session, SessionEvent::ShellStart { tool: tool.map(str::to_string), ns })
}

/// Record that `session` opened (`true`) or closed a probe.
pub fn record_probe(r: &ModelRef, session: &str, open: bool) -> Result<(), String> {
    session_store::append(r, session, if open { SessionEvent::ProbeOpen } else { SessionEvent::ProbeClose })
}

/// Whether `session` has a probe open.
pub fn probe_open(r: &ModelRef, session: &str) -> bool {
    session_log(r, session).probe_open
}

/// A shell command finished: record every project file modified since it
/// started as touched — edits made with `sed`, a script or a heredoc are the
/// session's as surely as an Edit tool's. Scryer's own `.scryer/` files are
/// not code. Several sessions can share one checkout, so a file another
/// session logged touching while the command ran is theirs, not this one's.
/// Returns the newly touched files.
pub fn record_shell_edits(r: &ModelRef, session: &str, tool: Option<&str>) -> Result<Vec<String>, String> {
    let log = session_log(r, session);
    let Some(since) = log.shell_started(tool) else { return Ok(Vec::new()) };
    let since_secs = since / 1_000_000_000;
    let others: HashSet<String> = session_store::list(r)
        .into_iter()
        .filter(|(id, mtime)| id != session && *mtime >= since_secs)
        .flat_map(|(id, _)| session_store::read(r, &id))
        .filter(|e| e.at >= since_secs)
        .filter_map(|e| match e.event {
            SessionEvent::Touch { file } => Some(file),
            _ => None,
        })
        .collect();
    let mut added = Vec::new();
    for file in session_store::files_modified_since_ns(r.project_path(), since) {
        if file.starts_with(".scryer/") || log.touched.contains(&file) || others.contains(&file) {
            continue;
        }
        session_store::append(r, session, SessionEvent::Touch { file: file.clone() })?;
        added.push(file);
    }
    Ok(added)
}

/// Serve the read overlay `payload` for `file`, or `None` when the session
/// was already shown this exact payload for it — re-reads must not re-inject
/// an identical block. Without a session there is nothing to dedupe
/// against: always answer. The payload is built by the caller from the
/// derived tree.
pub fn overlay(
    r: &ModelRef,
    session: Option<&str>,
    file: &str,
    payload: serde_json::Value,
) -> Result<Option<serde_json::Value>, String> {
    let file = relativize(r.project_path(), file);
    if let Some(session) = session.filter(|s| !s.is_empty()) {
        let hash = fnv1a64(&serde_json::to_vec(&payload).unwrap_or_default());
        if session_log(r, session).overlay_is_repeat(&file, hash) {
            return Ok(None);
        }
        session_store::append(r, session, SessionEvent::Overlay { file, hash })?;
    }
    Ok(Some(payload))
}

/// Record the task the agent oriented on — the first one titles the session's
/// change, so later ones are not kept.
pub fn record_task(r: &ModelRef, session: &str, task: &str) -> Result<(), String> {
    let task = task.trim();
    if task.is_empty() || session_log(r, session).task.is_some() {
        return Ok(());
    }
    session_store::append(r, session, SessionEvent::Task { text: task.to_string() })
}

/// The longest first prompt kept as a title.
const PROMPT_TITLE_CHARS: usize = 120;

/// Record the developer's first prompt, clipped, as the session's fallback
/// title. Later prompts are not kept.
pub fn record_prompt(r: &ModelRef, session: &str, prompt: &str) -> Result<(), String> {
    let line = prompt.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.is_empty() || session_log(r, session).prompt.is_some() {
        return Ok(());
    }
    let mut text: String = line.chars().take(PROMPT_TITLE_CHARS).collect();
    if line.chars().count() > PROMPT_TITLE_CHARS {
        text.push('…');
    }
    session_store::append(r, session, SessionEvent::Prompt { text })
}

/// The task the session first oriented on, if any.
pub fn session_task(r: &ModelRef, session: &str) -> Option<String> {
    session_log(r, session).task
}

/// The session is starting: the first time, record the todo tests already in
/// the project (`(file, junit_name)`, computed only then), so its first stop
/// can tell the stubs it wrote from ones that were there.
pub fn begin(
    r: &ModelRef,
    session: &str,
    planned: impl FnOnce() -> Vec<(String, String)>,
) -> Result<(), String> {
    if session_log(r, session).planned_seen.is_some() {
        return Ok(());
    }
    let tests = planned().iter().map(|(f, t)| test_key(f, t)).collect();
    session_store::append(r, session, SessionEvent::Planned { tests })
}

/// The session stopped: close its record. Given the project's todo tests
/// (`None` when they could not be read), the ones it wrote since its last
/// record — new, in a file it touched, not already another session's —
/// become its stubs, and the claims its edits reach are noted. Nothing is
/// said to the agent or the user: the record is read where they look.
pub fn stop(
    r: &ModelRef,
    session: &str,
    planned: Option<&[(String, String)]>,
    reached: Vec<String>,
) -> Result<(), String> {
    let Some(planned) = planned else {
        return session_store::append(r, session, SessionEvent::Stop);
    };
    let log = session_log(r, session);
    let theirs: HashSet<(String, String)> = session_store::list(r)
        .into_iter()
        .filter(|(id, _)| id != session)
        .flat_map(|(id, _)| session_log(r, &id).stubs)
        .collect();
    for (file, test) in log.new_stubs(planned).into_iter().filter(|s| !theirs.contains(s)) {
        session_store::append(r, session, SessionEvent::Stub { file: file.clone(), test: test.clone() })?;
    }
    let tests = planned.iter().map(|(f, t)| test_key(f, t)).collect();
    session_store::append(r, session, SessionEvent::Planned { tests })?;
    if !log.touched.is_empty() || !log.reached.is_empty() {
        session_store::append(r, session, SessionEvent::Reached { ids: reached })?;
    }
    session_store::append(r, session, SessionEvent::Stop)
}

/// Point harness process `pid` at `session`, so an MCP server spawned by that
/// process finds the session it serves now — the harness's session id can
/// change under a running server (`/clear`). A pointer, not history: one small
/// file per pid, overwritten.
pub fn bind_pid(r: &ModelRef, session: &str, pid: u32) -> Result<(), String> {
    session_store::ensure_dir(r)?;
    let dir = r.sessions_dir().join(".by-pid");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(pid.to_string());
    if std::fs::read_to_string(&path).ok().as_deref() == Some(session) {
        return Ok(());
    }
    std::fs::write(path, session).map_err(|e| e.to_string())
}

/// Another session working in this project now, as orient shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct LiveSession {
    pub id: String,
    /// Its title: the task it oriented on, else its first prompt.
    pub task: Option<String>,
    pub touched: Vec<String>,
    /// The todo tests it owns, `(file, junit_name)`.
    pub stubs: Vec<(String, String)>,
}

/// How recent a touch keeps a session live without a running process.
const LIVE_SECS: u64 = 3600;

/// The sessions live in this project besides `except`: a harness process
/// bound to it is still running, or it touched a file in the last hour.
pub fn live_sessions(r: &ModelRef, except: Option<&str>) -> Vec<LiveSession> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let running: HashSet<String> = std::fs::read_dir(r.sessions_dir().join(".by-pid"))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_name().to_str().and_then(|p| p.parse::<u32>().ok()).is_some_and(process_alive))
        .filter_map(|e| std::fs::read_to_string(e.path()).ok())
        .map(|s| s.trim().to_string())
        .collect();
    session_store::list(r)
        .into_iter()
        .filter(|(id, _)| Some(id.as_str()) != except)
        .filter_map(|(id, _)| {
            let entries = session_store::read(r, &id);
            let recent = entries
                .iter()
                .any(|e| matches!(e.event, SessionEvent::Touch { .. }) && e.at + LIVE_SECS >= now);
            if !recent && !running.contains(&id) {
                return None;
            }
            let log = SessionLog::from_events(&entries);
            let task = log.title().map(str::to_string);
            Some(LiveSession { id, task, touched: log.touched, stubs: log.stubs })
        })
        .collect()
}

/// Whether process `pid` is running. Known on Linux; elsewhere a session is
/// live only by its recent touches.
fn process_alive(pid: u32) -> bool {
    cfg!(target_os = "linux") && std::path::Path::new(&format!("/proc/{pid}")).exists()
}

/// The session harness process `pid` last bound, if any.
pub fn session_for_pid(r: &ModelRef, pid: u32) -> Option<String> {
    let s = std::fs::read_to_string(r.sessions_dir().join(".by-pid").join(pid.to_string())).ok()?;
    let s = s.trim().to_string();
    session_store::session_path(r, &s).map(|_| s)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> (tempfile::TempDir, ModelRef) {
        let dir = tempfile::tempdir().unwrap();
        let r = ModelRef::ProjectLocal(dir.path().to_path_buf());
        std::fs::create_dir_all(dir.path().join(".scryer")).unwrap();
        (dir, r)
    }

    /// The overlay is served once per session until its payload changes.
    #[test]
    fn overlay_is_served_once_per_session_until_it_changes() {
        let (dir, r) = project();
        let abs = format!("{}/src/auth.rs", dir.path().display());
        let payload = |claim: &str| serde_json::json!({ "file": "src/auth.rs", "claims": [claim] });

        let first = overlay(&r, Some("s1"), &abs, payload("serves")).unwrap().expect("first read injects");
        assert_eq!(first["claims"][0], "serves");
        assert!(overlay(&r, Some("s1"), &abs, payload("serves")).unwrap().is_none(), "identical repeat is silent");
        assert!(overlay(&r, Some("s2"), &abs, payload("serves")).unwrap().is_some(), "per session");
        assert!(overlay(&r, None, &abs, payload("serves")).unwrap().is_some(), "no session: always");
        let changed = overlay(&r, Some("s1"), &abs, payload("serves authenticated")).unwrap();
        assert!(changed.is_some(), "a changed payload re-fires");
        assert!(overlay(&r, Some("s1"), &abs, payload("serves authenticated")).unwrap().is_none());
    }

    /// A stop is only recorded: it never blocks, whatever the session left.
    #[test]
    fn a_stop_is_recorded_and_never_blocks() {
        let (_dir, r) = project();
        record_touch(&r, "s", "src/auth.rs").unwrap();
        stop(&r, "s", Some(&[]), Vec::new()).unwrap();
        stop(&r, "s", None, Vec::new()).unwrap();
        let stops = session_store::read(&r, "s").into_iter().filter(|e| e.event == SessionEvent::Stop).count();
        assert_eq!(stops, 2);
    }

    /// When a session's first prompt arrives it becomes the session's title,
    /// clipped to 120 characters, unless the agent orients on a task; later
    /// prompts are not kept.
    #[test]
    fn the_first_prompt_titles_a_session_without_a_task() {
        let (_dir, r) = project();
        let long = "fix the landing page ".repeat(10);
        record_prompt(&r, "s", &long).unwrap();
        record_prompt(&r, "s", "something else").unwrap();
        let log = session_log(&r, "s");
        let title = log.title().unwrap();
        assert_eq!(title.chars().count(), 121, "{title}");
        assert!(title.starts_with("fix the landing page fix") && title.ends_with('…'));
        record_task(&r, "s", "Queue offline saves").unwrap();
        assert_eq!(session_log(&r, "s").title(), Some("Queue offline saves"));
    }

    /// At a stop, the todo tests the session wrote since its start in files
    /// it touched become its stubs — not ones that were already there, not
    /// ones in files it never touched, not another session's — and the
    /// claims its edits reach are recorded.
    #[test]
    fn a_stop_records_the_sessions_new_stubs_and_what_it_reached() {
        let (_dir, r) = project();
        let t = |f: &str, n: &str| (f.to_string(), n.to_string());
        begin(&r, "s", || vec![t("src/a.rs", "old")]).unwrap();
        begin(&r, "s", || panic!("the start is recorded once")).unwrap();
        record_touch(&r, "s", "src/a.rs").unwrap();
        record_touch(&r, "other", "src/a.rs").unwrap();
        crate::infrastructure::session_store::append(
            &r,
            "other",
            SessionEvent::Stub { file: "src/a.rs".into(), test: "theirs".into() },
        )
        .unwrap();

        let planned = [t("src/a.rs", "old"), t("src/a.rs", "mine"), t("src/a.rs", "theirs"), t("src/b.rs", "untouched")];
        stop(&r, "s", Some(&planned), vec!["resp-1".into()]).unwrap();
        let log = session_log(&r, "s");
        assert_eq!(log.stubs, vec![t("src/a.rs", "mine")]);
        assert_eq!(log.reached, vec!["resp-1"]);

        // A later stop with a new stub adds only that one.
        let planned = [planned.as_slice(), &[t("src/a.rs", "later")]].concat();
        stop(&r, "s", Some(&planned), vec!["resp-1".into(), "resp-2".into()]).unwrap();
        let log = session_log(&r, "s");
        assert_eq!(log.stubs, vec![t("src/a.rs", "mine"), t("src/a.rs", "later")]);
        assert_eq!(log.reached.len(), 2);
    }

    /// Edits outside the project — a probe worktree, a scratch file — are
    /// not the project's code and are never recorded.
    #[test]
    fn edits_outside_the_project_are_not_touches() {
        let (dir, r) = project();
        record_touch(&r, "s", "src/auth.rs").unwrap();
        record_touch(&r, "s", "/elsewhere/probes/wt/src/auth.rs").unwrap();
        record_touch(&r, "s", &format!("{}/../sibling/x.rs", dir.path().display())).unwrap();
        assert_eq!(session_log(&r, "s").touched, vec!["src/auth.rs"]);
    }

    /// Files a shell command modified between its start and finish become the
    /// session's touches — however the command wrote them — and scryer's own
    /// files and ones already touched are left out.
    #[test]
    fn a_shell_commands_edits_are_recorded_as_touches() {
        let (dir, r) = project();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/before.rs"), "old").unwrap();
        std::fs::write(dir.path().join("src/kept.rs"), "old").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));

        record_shell_start(&r, "s", Some("call-1")).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(dir.path().join("src/before.rs"), "sed'd").unwrap();
        std::fs::write(dir.path().join("src/new.rs"), "heredoc").unwrap();
        std::fs::write(dir.path().join(".scryer/scratch.json"), "{}").unwrap();

        let mut added = record_shell_edits(&r, "s", Some("call-1")).unwrap();
        added.sort();
        assert_eq!(added, vec!["src/before.rs", "src/new.rs"]);
        let log = session_log(&r, "s");
        assert!(log.touched.contains(&"src/new.rs".to_string()));
        assert!(!log.touched.contains(&"src/kept.rs".to_string()), "unmodified file untouched");
        assert!(
            record_shell_edits(&r, "s", Some("call-1")).unwrap().is_empty(),
            "a file already touched is not recorded twice"
        );
        assert!(record_shell_edits(&r, "other", None).unwrap().is_empty(), "no start, no touches");
    }

    /// Sessions sharing a checkout: a file another session logged editing
    /// while this one's command ran is left to that session.
    #[test]
    fn a_file_another_session_edited_meanwhile_is_not_this_sessions() {
        let (dir, r) = project();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        record_shell_start(&r, "a", Some("call-1")).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(dir.path().join("src/mine.rs"), "a's sed").unwrap();
        std::fs::write(dir.path().join("src/theirs.rs"), "b's edit").unwrap();
        record_touch(&r, "b", "src/theirs.rs").unwrap();

        assert_eq!(record_shell_edits(&r, "a", Some("call-1")).unwrap(), vec!["src/mine.rs"]);
        assert!(!session_log(&r, "a").touched.contains(&"src/theirs.rs".to_string()));
    }

    /// A session is live while a process bound to it runs or it touched a
    /// file in the last hour; the caller is never its own peer.
    #[test]
    fn live_sessions_are_running_or_recently_touching() {
        let (_dir, r) = project();
        record_touch(&r, "recent", "src/a.rs").unwrap();
        record_task(&r, "recent", "rename drafts").unwrap();
        crate::infrastructure::session_store::append(&r, "idle", SessionEvent::Stop).unwrap();
        crate::infrastructure::session_store::append(&r, "bound", SessionEvent::Stop).unwrap();
        bind_pid(&r, "bound", std::process::id()).unwrap();
        record_touch(&r, "me", "src/a.rs").unwrap();

        let mut live: Vec<LiveSession> = live_sessions(&r, Some("me"));
        live.sort_by(|a, b| a.id.cmp(&b.id));
        let ids: Vec<&str> = live.iter().map(|s| s.id.as_str()).collect();
        let expected: &[&str] = if cfg!(target_os = "linux") { &["bound", "recent"] } else { &["recent"] };
        assert_eq!(ids, expected);
        let recent = live.iter().find(|s| s.id == "recent").unwrap();
        assert_eq!((recent.task.as_deref(), recent.touched.as_slice()), (Some("rename drafts"), &["src/a.rs".to_string()][..]));
    }

    #[test]
    fn a_pid_points_at_the_session_it_last_bound() {
        let (_dir, r) = project();
        assert_eq!(session_for_pid(&r, 42), None);
        bind_pid(&r, "s1", 42).unwrap();
        bind_pid(&r, "s2", 42).unwrap();
        assert_eq!(session_for_pid(&r, 42).as_deref(), Some("s2"));
        assert_eq!(list_sessions(&r).len(), 0, "pointers are not sessions");
    }
}
