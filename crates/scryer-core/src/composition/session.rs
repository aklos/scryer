//! The session hooks' entry points: read the session's log and the model,
//! answer, and append what the answer implies. The hook client calls these
//! in-process — no app, no server.

use crate::application::hooks::{close_view, session_summary, AnchorFlag, CloseView, Unfolded};
use crate::application::locate::LocateReport;
use crate::composition::locate::locate_at;
use crate::composition::model_store::{read_model_at, read_planned_at};
use crate::domain::session::{fnv1a64, relativize, SessionLog};
use serde::Serialize;
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
    for file in crate::infrastructure::drift::files_modified_since_ns(r.project_path(), since) {
        if file.starts_with(".scryer/") || log.touched.contains(&file) || others.contains(&file) {
            continue;
        }
        session_store::append(r, session, SessionEvent::Touch { file: file.clone() })?;
        added.push(file);
    }
    Ok(added)
}

/// The intent overlay for `file`, or `None` when the session was already shown
/// this exact payload for it — re-reads must not re-inject an identical block.
/// Without a session there is nothing to dedupe against: always answer.
/// The returned report is serialized with the relative `file` added.
pub fn overlay(
    r: &ModelRef,
    session: Option<&str>,
    file: &str,
) -> Result<Option<serde_json::Value>, String> {
    let file = relativize(r.project_path(), file);
    let report: LocateReport = locate_at(r, &file, None)?;
    let mut v = serde_json::to_value(&report).map_err(|e| e.to_string())?;
    if let serde_json::Value::Object(map) = &mut v {
        map.insert("file".into(), serde_json::json!(file));
    }
    if let Some(session) = session.filter(|s| !s.is_empty()) {
        let hash = fnv1a64(&serde_json::to_vec(&v).unwrap_or_default());
        if session_log(r, session).overlay_is_repeat(&file, hash) {
            return Ok(None);
        }
        session_store::append(r, session, SessionEvent::Overlay { file, hash })?;
    }
    Ok(Some(v))
}

/// The close view for `session`'s touches. The gate fires ONCE per session:
/// the verdict on a flagged claim may legitimately be "it still describes the
/// code", which writes nothing, so a re-derived gate would fire on every stop
/// forever. A view that fires is recorded; a clean one leaves the gate armed.
/// `flags` runs the anchor check over the touched files, only when needed.
pub fn close_gate(
    r: &ModelRef,
    session: &str,
    flags: impl FnOnce(&[String]) -> Vec<AnchorFlag>,
) -> CloseView {
    let log = session_log(r, session);
    if log.reconcile_gated || log.touched.is_empty() {
        return CloseView::default();
    }
    // Drift the session found rather than made: an anchor already in this
    // state when the session first read its file is not this session's to
    // reconcile.
    let flags: Vec<AnchorFlag> = flags(&log.touched)
        .into_iter()
        .filter(|f| !log.found_stale(&f.file, &f.key, &f.state))
        .collect();
    let committed = read_model_at(r).ok();
    let working = match (&committed, read_planned_at(r)) {
        (Some(c), Ok(p)) => Some(crate::working_view(c, &p)),
        _ => committed.clone(),
    };
    let view = close_view(committed.as_ref(), working.as_ref(), &flags, &log.touched);
    if !view.needs_reconcile.is_empty() {
        let _ = session_store::append(r, session, SessionEvent::ReconcileGate);
    }
    view
}

/// The session is reading `file`: the first time, record which of its anchors
/// are already out of sync (`flags` runs the anchor check on that one file),
/// so the close gate later tells drift the session made from drift it found.
/// Later reads cost nothing.
pub fn record_first_sight(
    r: &ModelRef,
    session: &str,
    file: &str,
    flags: impl FnOnce(&str) -> Vec<AnchorFlag>,
) -> Result<(), String> {
    let file = relativize(r.project_path(), file);
    if std::path::Path::new(&file).is_absolute() || session_log(r, session).has_seen(&file) {
        return Ok(());
    }
    let stale = flags(&file)
        .into_iter()
        .filter(|f| f.file == file)
        .map(|f| (f.key, f.state))
        .collect();
    session_store::append(r, session, SessionEvent::FirstSight { file, stale })
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

/// Record that the agent wrote these plan elements.
pub fn record_model_edit(r: &ModelRef, session: &str, keys: Vec<String>) -> Result<(), String> {
    if keys.is_empty() {
        return Ok(());
    }
    session_store::append(r, session, SessionEvent::ModelEdit { keys })
}

fn working(r: &ModelRef) -> Option<crate::ScryModel> {
    let committed = read_model_at(r).ok()?;
    Some(match read_planned_at(r) {
        Ok(p) => crate::working_view(&committed, &p),
        Err(_) => committed,
    })
}

/// A session as the user reviews it: what was edited and which claims that
/// reached, and what it planned and left unbuilt.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionView {
    pub session: String,
    pub touched: Vec<TouchedFile>,
    /// Plan entries the session planned and has not folded.
    pub unfolded: Vec<Unfolded>,
    pub model_edits: Vec<String>,
    /// Unix seconds of the first and last event.
    pub started_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct TouchedFile {
    pub file: String,
    /// Claims anchored in the file: `(id, statement)`.
    pub claims: Vec<(String, String)>,
}

pub fn session_view(r: &ModelRef, session: &str) -> SessionView {
    let entries = session_store::read(r, session);
    let log = SessionLog::from_events(&entries);
    let working = working(r).unwrap_or_default();
    let touched = log
        .touched
        .iter()
        .map(|f| TouchedFile {
            file: f.clone(),
            claims: crate::application::locate::locate(&working, f, None)
                .claims
                .into_iter()
                .filter(|c| !c.via_test)
                .map(|c| (c.id, c.statement.unwrap_or_default()))
                .collect(),
        })
        .collect();
    SessionView {
        session: session.to_string(),
        touched,
        unfolded: unfolded(r, session),
        model_edits: log.model_edits.clone(),
        started_at: entries.first().map_or(0, |e| e.at),
        updated_at: entries.last().map_or(0, |e| e.at),
    }
}

/// What the Stop hook does: block with a reason for the agent, or let the
/// session stop with an optional summary line for the user.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StopOutcome {
    pub block: Option<String>,
    pub summary: Option<String>,
}

/// The Stop check. Blocks on planned work left unbuilt with no note once code
/// was edited after it was planned, on an unprobed test, and on unreconciled
/// anchors — each at most once. A clean stop hands the user a summary, unless
/// it says what the last one said.
pub fn stop(
    r: &ModelRef,
    session: &str,
    flags: impl FnOnce(&[String]) -> Vec<AnchorFlag>,
    probes: impl FnOnce(&SessionLog) -> (Option<String>, Option<String>),
) -> StopOutcome {
    let log = session_log(r, session);
    let (probe_block, probe_summary) = probes(&log);
    let left = unfolded(r, session);

    let mut reasons: Vec<String> = Vec::new();
    // An entry nothing was built on since it was planned is a plan waiting
    // on the user's sign-off — blocking there buries the question.
    let silent: Vec<&Unfolded> = left
        .iter()
        .filter(|u| {
            u.note.is_none() && !log.gated_pending.contains(&u.key) && log.edited_since_planning(&u.key)
        })
        .collect();
    if !silent.is_empty() {
        reasons.push(pending_reason(&silent));
        let keys = silent.iter().map(|u| u.key.clone()).collect();
        let _ = session_store::append(r, session, SessionEvent::PendingGate { keys });
    }
    if let Some(reason) = probe_block.filter(|_| !log.probe_gated) {
        reasons.push(reason);
        let _ = session_store::append(r, session, SessionEvent::ProbeGate);
    }
    let close = close_gate(r, session, flags);
    if !close.needs_reconcile.is_empty() {
        reasons.push(reconcile_reason(&close));
    }
    if !reasons.is_empty() {
        return StopOutcome { block: Some(reasons.join("\n\n")), summary: None };
    }

    let summary = match (session_summary(&left), probe_summary) {
        (Some(s), Some(p)) => Some(format!("{s} · {p}")),
        (None, Some(p)) => Some(format!("scryer · {p}")),
        (s, None) => s,
    }
    .filter(|s| log.last_summary.as_deref() != Some(s.as_str()));
    if let Some(s) = &summary {
        let _ = session_store::append(r, session, SessionEvent::Summary { text: s.clone() });
    }
    StopOutcome { block: None, summary }
}

/// Plan entries tagged to `session`'s change that are still pending — the
/// work it planned and neither folded nor reverted — with any progress note.
pub fn unfolded(r: &ModelRef, session: &str) -> Vec<Unfolded> {
    let (Ok(committed), Ok(planned)) = (read_model_at(r), read_planned_at(r)) else {
        return Vec::new();
    };
    let Some(cid) = crate::domain::changes::session_change(&planned, session).map(|c| c.id.clone()) else {
        return Vec::new();
    };
    crate::domain::diff::diff(&committed, &planned)
        .changes
        .iter()
        .map(|ch| Unfolded {
            key: crate::domain::changes::key_for(ch),
            label: ch.label.clone(),
            note: (ch.kind == crate::domain::diff::ElementKind::Responsibility)
                .then(|| planned.notes.get(&ch.id).cloned())
                .flatten(),
        })
        .filter(|u| planned.change_map.get(&u.key) == Some(&cid))
        .collect()
}

const PENDING_SHOWN: usize = 15;

fn pending_reason(left: &[&Unfolded]) -> String {
    let mut lines: Vec<String> = left
        .iter()
        .take(PENDING_SHOWN)
        .map(|u| format!("- {}: {}", u.key, u.label))
        .collect();
    if left.len() > PENDING_SHOWN {
        lines.push(format!("- … {} more (get_pending)", left.len() - PENDING_SHOWN));
    }
    format!(
        "Scryer — this session planned {} model entr{} it has not folded, with no note saying \
         why:\n{}\nFinish each: built and tested → mark_implemented (anchors + tests); dropped → \
         revert the plan entry; a drift finding → resolve_drift; genuinely unfinished → \
         note_claims {{notes: {{id: \"built …; left …; waits on …\"}}}} — the user reads it on \
         the claim and the next session starts from it. Nobody closes these after you.",
        left.len(),
        if left.len() == 1 { "y" } else { "ies" },
        lines.join("\n"),
    )
}

const RECONCILE_FILES_SHOWN: usize = 8;

/// The close gate's block: one line per file, each claim counted once (its
/// code and test anchors are one claim) by its worst state. The statements
/// stay out — this lands in the user's transcript; `locate {file}` has them.
fn reconcile_reason(close: &CloseView) -> String {
    let mut total = 0;
    let mut lines: Vec<String> = Vec::new();
    for f in &close.needs_reconcile {
        let mut by_claim: Vec<(&str, &str)> = Vec::new();
        for c in &f.claims {
            let id = crate::test_resp_id(&c.id).unwrap_or(&c.id);
            match by_claim.iter_mut().find(|(k, _)| *k == id) {
                Some(entry) if c.state == "broken" => entry.1 = "broken",
                Some(_) => {}
                None => by_claim.push((id, c.state.as_str())),
            }
        }
        total += by_claim.len();
        let mut counts: Vec<(&str, usize)> = Vec::new();
        for (_, state) in &by_claim {
            match counts.iter_mut().find(|(s, _)| s == state) {
                Some(entry) => entry.1 += 1,
                None => counts.push((state, 1)),
            }
        }
        let counts: Vec<String> = counts.iter().map(|(s, n)| format!("{n} {s}")).collect();
        lines.push(format!("- {} — {}", f.file, counts.join(", ")));
    }
    let more = lines.len().saturating_sub(RECONCILE_FILES_SHOWN);
    lines.truncate(RECONCILE_FILES_SHOWN);
    if more > 0 {
        lines.push(format!("- +{more} more file(s)"));
    }
    format!(
        "Scryer close gate — {total} claim(s) in {} file(s) went out of sync with the code this \
         session:\n{}\n`locate {{file}}` lists them. If a claim still describes the code, nothing to \
         do; if its behaviour changed, reword it (update_nodes), re-anchor it (update_source_map) or \
         flag_drift. Fires once per session.",
        close.needs_reconcile.len(),
        lines.join("\n"),
    )
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
        let m: crate::ScryModel = serde_json::from_value(serde_json::json!({
            "version": "0.3", "links": [],
            "nodes": [
                { "id": "sys", "kind": "system", "name": "Acme" },
                { "id": "api", "kind": "container", "name": "API", "parentId": "sys",
                  "responsibilities": [
                      { "id": "r-1", "statement": "**When** a request arrives, **serves** it" },
                      { "id": "r-2", "statement": "**Log** every request" }
                  ] }
            ],
            "sourceMap": {
                "r-1": [{ "pattern": "src/auth.rs", "symbol": "verify" }],
                "r-2": [{ "pattern": "src/log.rs", "symbol": "log" }]
            }
        }))
        .unwrap();
        crate::write_model_at(&r, &m).unwrap();
        (dir, r)
    }

    #[test]
    fn overlay_is_served_once_per_session_until_it_changes() {
        let (dir, r) = project();
        let abs = format!("{}/src/auth.rs", dir.path().display());

        let first = overlay(&r, Some("s1"), &abs).unwrap().expect("first read injects");
        assert_eq!(first["file"], "src/auth.rs");
        assert_eq!(first["claims"][0]["id"], "r-1");
        assert!(overlay(&r, Some("s1"), &abs).unwrap().is_none(), "identical repeat is silent");
        assert!(overlay(&r, Some("s2"), &abs).unwrap().is_some(), "per session");
        assert!(overlay(&r, None, &abs).unwrap().is_some(), "no session: always");
        assert!(overlay(&r, None, &abs).unwrap().is_some());

        let mut m = crate::read_model_at(&r).unwrap();
        m.nodes[1].responsibilities[0].statement = "serves authenticated requests".into();
        crate::write_model_at(&r, &m).unwrap();
        let changed = overlay(&r, Some("s1"), &abs).unwrap().expect("changed payload re-fires");
        assert_eq!(changed["claims"][0]["statement"], "serves authenticated requests");
        assert!(overlay(&r, Some("s1"), &abs).unwrap().is_none());
    }

    #[test]
    fn the_close_gate_fires_once_per_session() {
        let (dir, r) = project();
        record_touch(&r, "s1", &format!("{}/src/auth.rs", dir.path().display())).unwrap();
        record_touch(&r, "s1", "README.md").unwrap();
        assert_eq!(session_log(&r, "s1").touched, vec!["src/auth.rs", "README.md"]);

        let clean = close_gate(&r, "s1", |_| vec![]);
        assert!(clean.needs_reconcile.is_empty());
        assert_eq!(clean.unmodeled, vec!["README.md"]);

        let flag = || AnchorFlag {
            key: "r-1".into(),
            host_name: "API".into(),
            file: "src/auth.rs".into(),
            symbol: Some("verify".into()),
            state: "changed".into(),
        };
        assert_eq!(close_gate(&r, "s1", |_| vec![flag()]).needs_reconcile.len(), 1);
        assert!(close_gate(&r, "s1", |_| vec![flag()]).needs_reconcile.is_empty(), "spent");
        assert!(close_gate(&r, "s2", |_| vec![flag()]).needs_reconcile.is_empty(), "s2 touched nothing");
        record_touch(&r, "s2", "src/auth.rs").unwrap();
        assert_eq!(close_gate(&r, "s2", |_| vec![flag()]).needs_reconcile.len(), 1, "s2 has its own gate");
    }

    use crate::application::hooks::{FileClaims, FlaggedClaim};

    fn flag_on(file: &str, key: &str, state: &str) -> AnchorFlag {
        AnchorFlag {
            key: key.into(),
            host_name: "API".into(),
            file: file.into(),
            symbol: None,
            state: state.into(),
        }
    }

    /// Drift the session found is not drift it made: a claim already out of
    /// sync when the session first read its file, and still in that state,
    /// stays out of the gate; one that changed state, or was in sync then, is
    /// the session's.
    #[test]
    fn the_close_gate_leaves_out_drift_the_session_found() {
        let (_dir, r) = project();
        let before = vec![flag_on("src/auth.rs", "r-1", "changed"), flag_on("src/other.rs", "r-9", "changed")];
        record_first_sight(&r, "s", "src/auth.rs", |_| before.clone()).unwrap();
        record_first_sight(&r, "s", "src/auth.rs", |_| panic!("only the first read checks")).unwrap();
        record_touch(&r, "s", "src/auth.rs").unwrap();

        let found = close_gate(&r, "s", |_| vec![flag_on("src/auth.rs", "r-1", "changed")]);
        assert!(found.needs_reconcile.is_empty(), "already changed at first sight: {found:?}");
        let made = close_gate(&r, "s", |_| vec![flag_on("src/auth.rs", "r-1", "broken")]);
        assert_eq!(made.needs_reconcile.len(), 1, "changed → broken is the session's");
    }

    /// The block lands in the user's transcript: one line per file, a claim's
    /// code and test anchors counted once, at most eight files.
    #[test]
    fn the_close_gate_block_is_one_line_per_file() {
        let claim = |id: &str, state: &str| FlaggedClaim {
            id: id.into(),
            host: Some("API".into()),
            symbol: None,
            state: state.into(),
            statement: Some("a very long statement that must not reach the user".into()),
        };
        let file = |i: usize| FileClaims {
            file: format!("src/f{i}.rs"),
            claims: vec![claim("r-1", "changed"), claim("test:r-1", "broken"), claim("r-2", "changed")],
        };
        let close = CloseView { needs_reconcile: (0..10).map(file).collect(), ..Default::default() };
        let reason = reconcile_reason(&close);
        assert!(reason.contains("- src/f0.rs — 1 broken, 1 changed"), "{reason}");
        assert!(reason.contains("- +2 more file(s)"), "{reason}");
        assert!(!reason.contains("src/f8.rs"), "{reason}");
        assert!(!reason.contains("very long statement"), "{reason}");
        assert!(reason.contains("20 claim(s) in 10 file(s)"), "{reason}");
    }

    fn no_flags(_: &[String]) -> Vec<AnchorFlag> {
        vec![]
    }

    fn no_probes(_: &SessionLog) -> (Option<String>, Option<String>) {
        (None, None)
    }

    #[test]
    fn the_probe_gate_blocks_once_and_survivors_reach_the_summary() {
        let (_dir, r) = project();
        let probes = |_: &SessionLog| (Some("probe 2 claims".to_string()), None);
        assert_eq!(stop(&r, "s", no_flags, probes).block.as_deref(), Some("probe 2 claims"));
        assert!(stop(&r, "s", no_flags, probes).block.is_none(), "once per session");
        let survivor = |_: &SessionLog| (Some("probe".to_string()), Some("r-1's test missed a break".to_string()));
        let out = stop(&r, "s", no_flags, survivor);
        assert_eq!(out.summary.as_deref(), Some("scryer · r-1's test missed a break"));
    }

    #[test]
    fn a_progress_note_answers_the_unfolded_gate_and_reaches_the_user() {
        let (_dir, r) = project();
        let mut plan = crate::read_model_at(&r).unwrap();
        plan.nodes[1].responsibilities.push(serde_json::from_value(
            serde_json::json!({ "id": "r-2", "statement": "rate-limits callers" }),
        )
        .unwrap());
        let cid = crate::domain::changes::open_change_for(&mut plan, "limit", Some("s"), 1);
        crate::domain::changes::tag(&mut plan, &["resp:r-2".to_string()], &cid);
        crate::write_planned_at(&r, &plan).unwrap();

        let notes: std::collections::BTreeMap<String, String> = [
            ("r-2".to_string(), "token bucket built; per-route limits left".to_string()),
            ("r-1".to_string(), "already folded".to_string()),
        ]
        .into();
        assert_eq!(crate::note_claims(&r, &notes).unwrap(), vec!["r-1".to_string()], "only pending claims take a note");

        let out = stop(&r, "s", no_flags, no_probes);
        assert!(out.block.is_none(), "a noted entry is an honest exit: {out:?}");
        let summary = out.summary.unwrap();
        assert!(summary.contains("per-route limits left") && !summary.contains("no note"), "{summary}");
        assert_eq!(unfolded(&r, "s")[0].note.as_deref(), Some("token bucket built; per-route limits left"));
    }

    #[test]
    fn the_stop_gate_names_unfolded_session_work_once() {
        let (_dir, r) = project();
        let mut plan = crate::read_model_at(&r).unwrap();
        plan.nodes[1].responsibilities.push(serde_json::from_value(
            serde_json::json!({ "id": "r-2", "statement": "rate-limits callers" }),
        )
        .unwrap());
        let cid = crate::domain::changes::open_change_for(&mut plan, "limit", Some("s"), 1);
        crate::domain::changes::tag(&mut plan, &["resp:r-2".to_string()], &cid);
        crate::write_planned_at(&r, &plan).unwrap();

        assert!(unfolded(&r, "other").is_empty(), "another session's work is not this one's");
        record_model_edit(&r, "s", vec!["resp:r-2".to_string()]).unwrap();
        record_touch(&r, "s", "src/auth.rs").unwrap();
        let reason = stop(&r, "s", no_flags, no_probes).block.expect("unfolded work blocks");
        assert!(reason.contains("resp:r-2") && reason.contains("mark_implemented"), "{reason}");
        let quiet = stop(&r, "s", no_flags, no_probes);
        assert!(quiet.block.is_none(), "once per session");
        let summary = quiet.summary.expect("the user still hears about it");
        assert!(summary.contains("planned, not built: 1") && summary.contains("rate-limits callers"), "{summary}");
        assert_eq!(session_view(&r, "s").unfolded.len(), 1);
    }

    /// A plan nothing was built on yet is waiting on the user's sign-off: the
    /// stop lets the question stand, and the gate arms once code is edited.
    #[test]
    fn a_plan_awaiting_sign_off_does_not_block_the_stop() {
        let (_dir, r) = project();
        let mut plan = crate::read_model_at(&r).unwrap();
        plan.nodes[1].responsibilities.push(serde_json::from_value(
            serde_json::json!({ "id": "r-2", "statement": "rate-limits callers" }),
        )
        .unwrap());
        let cid = crate::domain::changes::open_change_for(&mut plan, "limit", Some("s"), 1);
        crate::domain::changes::tag(&mut plan, &["resp:r-2".to_string()], &cid);
        crate::write_planned_at(&r, &plan).unwrap();

        record_touch(&r, "s", "src/log.rs").unwrap();
        record_model_edit(&r, "s", vec!["resp:r-2".to_string()]).unwrap();
        let out = stop(&r, "s", no_flags, no_probes);
        assert!(out.block.is_none(), "an edit before planning is not building it: {out:?}");
        assert!(out.summary.unwrap().contains("rate-limits callers"), "the user still sees it");

        record_touch(&r, "s", "src/auth.rs").unwrap();
        let reason = stop(&r, "s", no_flags, no_probes).block.expect("built on, then left unfolded");
        assert!(reason.contains("resp:r-2"), "{reason}");
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
