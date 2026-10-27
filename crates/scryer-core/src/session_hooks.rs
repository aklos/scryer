//! The session hooks' entry points: read the session's log, answer, and
//! append what the answer implies. The hook client calls these in-process —
//! no app, no server. [composition]

use crate::session_log::{fnv1a64, relativize, test_key, SessionLog};
use std::collections::HashSet;
use crate::stores as session_store;
use crate::c4_model::ModelRef;

pub use crate::session_log::{SessionEntry, SessionEvent};
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

/// How long a shell command with no recorded end counts as still running.
const OPEN_SHELL_NS: u64 = 1800 * 1_000_000_000;

/// A shell command finished: record every project file modified since it
/// started as touched — edits made with `sed`, a script or a heredoc are the
/// session's as surely as an Edit tool's — and the command's end. Scryer's
/// own `.scryer/` files are not code. Several sessions can share one
/// checkout, so a file is left out when another session logged touching it
/// while the command ran, or when it changed while another session's shell
/// command was running too: then either could have written it, and a touch
/// that may be someone else's is worse than one missed. Returns the newly
/// touched files.
pub fn record_shell_edits(r: &ModelRef, session: &str, tool: Option<&str>) -> Result<Vec<String>, String> {
    let log = session_log(r, session);
    let Some(since) = log.shell_started(tool) else { return Ok(Vec::new()) };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or_default();
    let since_secs = since / 1_000_000_000;
    // A command still running in another session may have started before
    // this one, so its log can be older than this window.
    let horizon = since.saturating_sub(OPEN_SHELL_NS) / 1_000_000_000;
    let others: Vec<Vec<SessionEntry>> = session_store::list(r)
        .into_iter()
        .filter(|(id, mtime)| id != session && *mtime >= horizon)
        .map(|(id, _)| session_store::read(r, &id))
        .collect();
    let theirs: HashSet<&str> = others
        .iter()
        .flatten()
        .filter(|e| e.at >= since_secs)
        .filter_map(|e| match &e.event {
            SessionEvent::Touch { file } => Some(file.as_str()),
            _ => None,
        })
        .collect();
    let windows: Vec<(u64, u64)> = others
        .iter()
        .flat_map(|entries| SessionLog::from_events(entries).shell_windows(now, OPEN_SHELL_NS))
        .filter(|(_, end)| *end >= since)
        .collect();
    let mut added = Vec::new();
    for (file, mtime) in session_store::files_modified_since_ns_at(r.project_path(), since) {
        let shared = windows.iter().any(|(start, end)| (*start..=*end).contains(&mtime));
        if file.starts_with(".scryer/") || log.touched.contains(&file) || theirs.contains(file.as_str()) || shared {
            continue;
        }
        session_store::append(r, session, SessionEvent::Touch { file: file.clone() })?;
        added.push(file);
    }
    session_store::append(r, session, SessionEvent::ShellEnd { tool: tool.map(str::to_string), ns: now })?;
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

/// The task the session first oriented on, if any.
pub fn session_task(r: &ModelRef, session: &str) -> Option<String> {
    session_log(r, session).task
}

/// The session is starting: the first time, record the todo tests already in
/// the project (`(file, junit_name)`) and the claims already proven (derived
/// claim ids), computed only then, so its stops can tell the stubs it wrote
/// from ones that were there and the landing what it proved from what was.
pub fn begin(
    r: &ModelRef,
    session: &str,
    snapshot: impl FnOnce() -> (Vec<(String, String)>, Vec<String>),
) -> Result<(), String> {
    let log = session_log(r, session);
    if log.planned_seen.is_some() && log.proven_at_start.is_some() {
        return Ok(());
    }
    let (planned, proven) = snapshot();
    if log.planned_seen.is_none() {
        let tests = planned.iter().map(|(f, t)| test_key(f, t)).collect();
        session_store::append(r, session, SessionEvent::Planned { tests })?;
    }
    if log.proven_at_start.is_none() {
        session_store::append(r, session, SessionEvent::Proven { ids: proven })?;
    }
    Ok(())
}

/// Record that this session ingested verdicts for `ids`: the claims whose
/// change of state is its own.
pub fn record_verdicts(r: &ModelRef, session: &str, ids: Vec<String>) -> Result<(), String> {
    if ids.is_empty() {
        return Ok(());
    }
    session_store::append(r, session, SessionEvent::Verdicts { ids })
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

/// The sessions in this project mid-turn now ([`crate::session_log::working`]).
pub fn working_sessions(r: &ModelRef) -> std::collections::BTreeSet<String> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    session_store::list(r)
        .into_iter()
        .filter(|(_, updated)| updated + crate::session_log::WORKING_SECS >= now)
        .filter(|(id, _)| crate::session_log::working(&session_store::read(r, id), now))
        .map(|(id, _)| id)
        .collect()
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

    /// When a session starts, records the claims already proven, once, and each report it ingests records the claims it got verdicts for.
    #[test]
    fn a_session_records_its_baseline_and_its_verdicts() {
        let (_dir, r) = project();
        begin(&r, "s", || (Vec::new(), vec!["c1".to_string()])).unwrap();
        begin(&r, "s", || panic!("the baseline is recorded once")).unwrap();
        record_verdicts(&r, "s", vec!["c2".into()]).unwrap();
        record_verdicts(&r, "s", vec!["c2".into(), "c3".into()]).unwrap();
        let log = session_log(&r, "s");
        assert_eq!(log.proven_at_start, Some(vec!["c1".to_string()]));
        assert_eq!(log.verdicts, vec!["c2".to_string(), "c3".to_string()]);
    }

    /// At a stop, the todo tests the session wrote since its start in files
    /// it touched become its stubs — not ones that were already there, not
    /// ones in files it never touched, not another session's — and the
    /// claims its edits reach are recorded.
    #[test]
    fn a_stop_records_the_sessions_new_stubs_and_what_it_reached() {
        let (_dir, r) = project();
        let t = |f: &str, n: &str| (f.to_string(), n.to_string());
        begin(&r, "s", || (vec![t("src/a.rs", "old")], vec!["c1".to_string()])).unwrap();
        begin(&r, "s", || panic!("the start is recorded once")).unwrap();
        record_touch(&r, "s", "src/a.rs").unwrap();
        record_touch(&r, "other", "src/a.rs").unwrap();
        crate::stores::append(
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

    /// If a file changed while another session's shell command was running
    /// too, then it is neither session's touch from this command; a file
    /// changed after the other command ended is.
    #[test]
    fn a_file_changed_during_another_sessions_command_is_not_claimed() {
        let (dir, r) = project();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        let pause = || std::thread::sleep(std::time::Duration::from_millis(20));
        record_shell_start(&r, "theirs", Some("t-1")).unwrap();
        record_shell_start(&r, "mine", Some("m-1")).unwrap();
        pause();
        std::fs::write(dir.path().join("src/either.rs"), "?").unwrap();
        pause();
        assert!(record_shell_edits(&r, "theirs", Some("t-1")).unwrap().is_empty(), "mine is still running");
        pause();
        std::fs::write(dir.path().join("src/after.rs"), "mine").unwrap();
        assert_eq!(record_shell_edits(&r, "mine", Some("m-1")).unwrap(), vec!["src/after.rs"]);
        assert!(session_log(&r, "mine").shell_ends.iter().any(|(t, _)| t.as_deref() == Some("m-1")));
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

    /// When another session logged editing a file while this one's command ran, leaves the file to that session.
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
