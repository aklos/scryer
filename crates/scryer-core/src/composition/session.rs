//! The session hooks' entry points: read the session's log and the model,
//! answer, and append what the answer implies. The hook client calls these
//! in-process — no app, no server.

use crate::application::hooks::{
    ask_views, asks_gate, close_view, session_summary, untraced_edits, AnchorFlag, AskView, CloseView, Unfolded,
};
use crate::application::locate::LocateReport;
use crate::composition::locate::locate_at;
use crate::composition::model_store::{read_model_at, read_planned_at};
use crate::domain::session::{fnv1a64, relativize, Ask, AskKind, SessionLog};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
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
    let flags = flags(&log.touched);
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

/// Record the user's prompt verbatim; returns its id.
pub fn record_prompt(r: &ModelRef, session: &str, text: &str) -> Result<String, String> {
    let id = session_log(r, session).next_prompt_id();
    session_store::append(r, session, SessionEvent::Prompt { id: id.clone(), text: text.to_string() })?;
    Ok(id)
}

/// Record that the agent wrote these plan elements.
pub fn record_model_edit(r: &ModelRef, session: &str, keys: Vec<String>) -> Result<(), String> {
    if keys.is_empty() {
        return Ok(());
    }
    session_store::append(r, session, SessionEvent::ModelEdit { keys })
}

/// The rationale a session's change opens with: its first prompt.
pub fn first_prompt(r: &ModelRef, session: &str) -> Option<String> {
    session_log(r, session).prompts.first().map(|(_, t)| t.clone())
}

/// One ask as the agent files it.
#[derive(Debug, Clone)]
pub struct NewAsk {
    pub text: String,
    pub kind: AskKind,
    pub source: Option<String>,
}

/// Break prompt `prompt` (default: the oldest one not filed yet) into asks,
/// recorded as the agent gives them. What a prompt asks for is the agent's
/// reading, never inferred here from its wording.
pub fn file_asks(
    r: &ModelRef,
    session: &str,
    prompt: Option<&str>,
    asks: Vec<NewAsk>,
) -> Result<(String, Vec<Ask>), String> {
    let log = session_log(r, session);
    let prompt = match prompt {
        Some(p) => p.to_string(),
        None => log
            .unfiled_prompts()
            .first()
            .map(|p| p.to_string())
            .or_else(|| log.prompts.last().map(|(id, _)| id.clone()))
            .ok_or("no prompt is recorded for this session yet")?,
    };
    if !log.prompts.iter().any(|(id, _)| *id == prompt) {
        return Err(format!("no prompt '{prompt}' in this session"));
    }
    if let Some(a) = asks.iter().find(|a| a.text.trim().is_empty()) {
        return Err(format!("an ask needs text (got {:?})", a.text));
    }
    let first = log.asks.len() + 1;
    let filed: Vec<Ask> = asks
        .into_iter()
        .enumerate()
        .map(|(i, a)| Ask {
            id: format!("a{}", first + i),
            text: a.text.trim().to_string(),
            kind: a.kind,
            source: a.source.map(|s| relativize(r.project_path(), &s)),
        })
        .collect();
    session_store::append(r, session, SessionEvent::Asks { prompt: prompt.clone(), asks: filed.clone() })?;
    Ok((prompt, filed))
}

/// How the agent resolves an ask.
#[derive(Debug, Clone)]
pub enum Resolution {
    /// These claims deliver it (added to any linked before).
    Claims(Vec<String>),
    Answered,
    Descoped(String),
    /// An action ask was carried out; the note says what was done.
    Done(String),
    /// Files the ask accounts for beyond its claims' anchors.
    Files(Vec<String>),
}

pub fn resolve_ask(r: &ModelRef, session: &str, id: &str, how: Resolution) -> Result<(), String> {
    let log = session_log(r, session);
    let ask = log.ask(id).ok_or_else(|| format!("no ask '{id}' in this session"))?;
    let event = match how {
        Resolution::Claims(claims) => {
            match ask.ask.kind {
                AskKind::Answer => return Err(format!("{id} is an answer ask — resolve it with answered: true")),
                AskKind::Action => return Err(format!("{id} is an action ask — resolve it with done: \"<what you did>\"")),
                AskKind::Build => {}
            }
            let working = working(r).ok_or("the model could not be read")?;
            let unknown: Vec<&String> = claims
                .iter()
                .filter(|c| {
                    !working
                        .nodes
                        .iter()
                        .flat_map(|n| n.responsibilities.iter())
                        .chain(working.groups.iter().flat_map(|g| g.responsibilities.iter()))
                        .any(|resp| &resp.id == *c)
                })
                .collect();
            if !unknown.is_empty() {
                return Err(format!("not claims in the model: {unknown:?}"));
            }
            SessionEvent::AskLinked { id: id.to_string(), claims }
        }
        Resolution::Answered => {
            if ask.ask.kind != AskKind::Answer {
                return Err(format!("{id} is not an answer ask — answered: true closes questions only"));
            }
            SessionEvent::AskAnswered { id: id.to_string() }
        }
        Resolution::Done(note) => {
            if ask.ask.kind != AskKind::Action {
                return Err(format!(
                    "{id} is not an action ask — done closes actions (commit, push, run) only"
                ));
            }
            let note = note.trim().to_string();
            if note.is_empty() {
                return Err("done needs a one-line note saying what was done".into());
            }
            SessionEvent::AskDone { id: id.to_string(), note }
        }
        Resolution::Files(files) => {
            let files: Vec<String> = files
                .iter()
                .map(|f| relativize(r.project_path(), f))
                .filter(|f| !f.is_empty())
                .collect();
            if files.is_empty() {
                return Err("files needs at least one project file".into());
            }
            SessionEvent::AskFiles { id: id.to_string(), files }
        }
        Resolution::Descoped(reason) => {
            let reason = reason.trim().to_string();
            if reason.is_empty() {
                return Err("a descope needs a one-line reason the user will read".into());
            }
            SessionEvent::AskDescoped { id: id.to_string(), reason }
        }
    };
    session_store::append(r, session, event)
}

/// Claim ids the plan still holds unbuilt: added or changed, not yet folded.
fn pending_claims(r: &ModelRef) -> HashSet<String> {
    let (Ok(committed), Ok(planned)) = (read_model_at(r), read_planned_at(r)) else {
        return HashSet::new();
    };
    crate::domain::diff::diff(&committed, &planned)
        .changes
        .into_iter()
        .filter(|ch| ch.kind == crate::domain::diff::ElementKind::Responsibility)
        .map(|ch| ch.id)
        .collect()
}

fn working(r: &ModelRef) -> Option<crate::ScryModel> {
    let committed = read_model_at(r).ok()?;
    Some(match read_planned_at(r) {
        Ok(p) => crate::working_view(&committed, &p),
        Err(_) => committed,
    })
}

/// A session as the user reviews it: what was asked, where each ask stands,
/// what was edited and which claims that reached, and what nothing asked for.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionView {
    pub session: String,
    pub prompts: Vec<PromptView>,
    /// Prompts the agent has not broken into asks yet.
    pub unfiled: Vec<String>,
    pub asks: Vec<AskView>,
    pub touched: Vec<TouchedFile>,
    pub untraced: Vec<String>,
    /// Plan entries the session planned and has not folded.
    pub unfolded: Vec<Unfolded>,
    pub model_edits: Vec<String>,
    /// Unix seconds of the first and last event.
    pub started_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct PromptView {
    pub id: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TouchedFile {
    pub file: String,
    /// Claims anchored in the file: `(id, statement)`.
    pub claims: Vec<(String, String)>,
}

pub fn session_view(
    r: &ModelRef,
    session: &str,
    verified: impl FnOnce(&[String]) -> HashMap<String, bool>,
) -> SessionView {
    let entries = session_store::read(r, session);
    let log = SessionLog::from_events(&entries);
    let working = working(r).unwrap_or_default();
    let linked: Vec<String> = log.asks.iter().flat_map(|a| a.claims.iter().cloned()).collect();
    let verified = verified(&linked);
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
        prompts: log.prompts.iter().map(|(id, text)| PromptView { id: id.clone(), text: text.clone() }).collect(),
        unfiled: log.unfiled_prompts().into_iter().map(str::to_string).collect(),
        asks: ask_views(&log, &working, &verified, &pending_claims(r)),
        touched,
        untraced: untraced_edits(&log, &working),
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

/// The Stop check. Blocks on asks first — prompts never broken into asks and
/// asks still open, each blocked on once — then on unreconciled anchors (once
/// per session). A clean stop hands the user a summary, unless it says what
/// the last one said.
pub fn stop(
    r: &ModelRef,
    session: &str,
    flags: impl FnOnce(&[String]) -> Vec<AnchorFlag>,
    verified: impl FnOnce(&[String]) -> HashMap<String, bool>,
    probes: impl FnOnce(&SessionLog) -> (Option<String>, Option<String>),
) -> StopOutcome {
    let log = session_log(r, session);
    let (probe_block, probe_summary) = probes(&log);
    let working = working(r).unwrap_or_default();
    let linked: Vec<String> = log.asks.iter().flat_map(|a| a.claims.iter().cloned()).collect();
    let views = ask_views(&log, &working, &verified(&linked), &pending_claims(r));
    let left = unfolded(r, session);

    let gate = asks_gate(&log, &views);
    let mut reasons: Vec<String> = Vec::new();
    if !gate.is_empty() {
        reasons.push(gate.reason());
        let _ = session_store::append(
            r,
            session,
            SessionEvent::AsksGate {
                prompts: gate.prompts.iter().map(|(id, _)| id.clone()).collect(),
                asks: gate.asks.iter().map(|a| a.id.clone()).collect(),
            },
        );
    }
    let silent: Vec<&Unfolded> = left
        .iter()
        .filter(|u| u.note.is_none() && !log.gated_pending.contains(&u.key))
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

    let summary = match (session_summary(&views, &untraced_edits(&log, &working), &left), probe_summary) {
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

fn reconcile_reason(close: &CloseView) -> String {
    let mut lines: Vec<String> = Vec::new();
    for f in &close.needs_reconcile {
        lines.push(format!("- {}:", f.file));
        for c in &f.claims {
            lines.push(format!(
                "    [{}] ({}) {}",
                c.state,
                c.host.as_deref().unwrap_or("?"),
                c.statement.as_deref().unwrap_or("(data shape declaration)")
            ));
        }
    }
    let claims: usize = close.needs_reconcile.iter().map(|f| f.claims.len()).sum();
    format!(
        "Scryer close gate — this session's edits reached the anchored span(s) of {claims} claim(s) \
         in {} file(s):\n{}\nReconcile each: if the claim still describes the code, no write is \
         needed; if behaviour changed, update the model over MCP (update_nodes to reword the claim, \
         update_source_map to re-anchor, mark_implemented to fold finished plan work, flag_drift \
         for new undescribed behaviour). This gate fires only once per session.",
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

    /// A claim on `api` anchored in src/auth.rs, with a test in tests/auth.rs.
    fn linkable(r: &ModelRef) {
        let mut m = crate::read_model_at(r).unwrap();
        m.test_map.insert(
            "r-1".into(),
            vec![serde_json::from_value(serde_json::json!({ "pattern": "tests/auth.rs" })).unwrap()],
        );
        crate::write_model_at(r, &m).unwrap();
    }

    fn ask(text: &str, kind: AskKind) -> NewAsk {
        NewAsk { text: text.into(), kind, source: None }
    }

    fn pass(ids: &[String]) -> HashMap<String, bool> {
        ids.iter().map(|i| (i.clone(), true)).collect()
    }

    fn no_flags(_: &[String]) -> Vec<AnchorFlag> {
        vec![]
    }

    /// A prompt never broken into asks blocks once; filed asks block once
    /// each while open; a build ask needs a linked, verified claim whose code
    /// the session touched; a clean stop summarises once.
    #[test]
    fn the_stop_gate_walks_the_ask_ledger() {
        let (_dir, r) = project();
        linkable(&r);
        let p1 = record_prompt(&r, "s", "make the API verify tokens and explain the cache").unwrap();
        assert_eq!(p1, "p1");

        let out = stop(&r, "s", no_flags, pass, no_probes);
        let reason = out.block.expect("an unfiled prompt blocks");
        assert!(reason.contains("prompt p1 was never broken into asks"), "{reason}");
        assert!(!reason.to_lowercase().contains("review"), "never asks the user for review: {reason}");
        assert!(stop(&r, "s", no_flags, pass, no_probes).block.is_none(), "never twice for the same prompt");

        let (_, filed) = file_asks(
            &r,
            "s",
            None,
            vec![ask("verify tokens", AskKind::Build), ask("explain the cache", AskKind::Answer)],
        )
        .unwrap();
        assert_eq!(filed.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(), vec!["a1", "a2"]);

        let reason = stop(&r, "s", no_flags, pass, no_probes).block.expect("open asks block");
        assert!(reason.contains("a1") && reason.contains("no claim delivers it"), "{reason}");
        assert!(reason.contains("a2") && reason.contains("not answered"), "{reason}");
        let quiet = stop(&r, "s", no_flags, pass, no_probes);
        assert!(quiet.block.is_none(), "one block per ask");
        assert!(quiet.summary.as_deref().unwrap().contains("2 open (a1, a2)"), "{quiet:?}");

        // Linked and verified, but the session never touched its code: open.
        resolve_ask(&r, "s", "a1", Resolution::Claims(vec!["r-1".into()])).unwrap();
        resolve_ask(&r, "s", "a2", Resolution::Answered).unwrap();
        let log = session_log(&r, "s");
        let working = working(&r).unwrap();
        let views = ask_views(&log, &working, &pass(&["r-1".into()]), &HashSet::new());
        assert!(matches!(&views[0].status, crate::session::AskStatus::Open { missing } if missing[0].contains("edited none")));
        assert_eq!(views[1].status, crate::session::AskStatus::Answered);
        let failing = ask_views(&log, &working, &HashMap::new(), &HashSet::new());
        assert!(matches!(&failing[0].status, crate::session::AskStatus::Open { missing } if missing.iter().any(|m| m.contains("no passing test verdict"))));

        record_touch(&r, "s", "src/auth.rs").unwrap();
        record_touch(&r, "s", "src/unrelated.rs").unwrap();
        let out = stop(&r, "s", no_flags, pass, no_probes);
        assert!(out.block.is_none());
        let summary = out.summary.unwrap();
        assert!(summary.contains("asks 2/2 done"), "{summary}");
        assert!(summary.contains("edits no ask accounts for: src/unrelated.rs"), "{summary}");
        assert!(stop(&r, "s", no_flags, pass, no_probes).summary.is_none(), "an unchanged summary stays silent");

        // A new prompt arms the gate again, for that prompt only.
        record_prompt(&r, "s", "thanks").unwrap();
        assert!(stop(&r, "s", no_flags, pass, no_probes).block.unwrap().contains("prompt p2"));
        file_asks(&r, "s", Some("p2"), vec![]).unwrap();
        assert!(stop(&r, "s", no_flags, pass, no_probes).block.is_none());
    }

    fn no_probes(_: &SessionLog) -> (Option<String>, Option<String>) {
        (None, None)
    }

    #[test]
    fn the_probe_gate_blocks_once_and_survivors_reach_the_summary() {
        let (_dir, r) = project();
        let probes = |_: &SessionLog| (Some("probe 2 claims".to_string()), None);
        assert_eq!(stop(&r, "s", no_flags, pass, probes).block.as_deref(), Some("probe 2 claims"));
        assert!(stop(&r, "s", no_flags, pass, probes).block.is_none(), "once per session");
        let survivor = |_: &SessionLog| (Some("probe".to_string()), Some("r-1's test missed a break".to_string()));
        let out = stop(&r, "s", no_flags, pass, survivor);
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

        let out = stop(&r, "s", no_flags, pass, no_probes);
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
        let reason = stop(&r, "s", no_flags, pass, no_probes).block.expect("unfolded work blocks");
        assert!(reason.contains("resp:r-2") && reason.contains("mark_implemented"), "{reason}");
        let quiet = stop(&r, "s", no_flags, pass, no_probes);
        assert!(quiet.block.is_none(), "once per session");
        let summary = quiet.summary.expect("the user still hears about it");
        assert!(summary.contains("planned, not built: 1") && summary.contains("rate-limits callers"), "{summary}");

        // An ask linked to a claim that is only planned is not delivered,
        // however green its test.
        record_prompt(&r, "s", "rate-limit the API").unwrap();
        file_asks(&r, "s", None, vec![ask("rate-limit callers", AskKind::Build)]).unwrap();
        resolve_ask(&r, "s", "a1", Resolution::Claims(vec!["r-2".into()])).unwrap();
        record_touch(&r, "s", "src/auth.rs").unwrap();
        let view = session_view(&r, "s", |_| pass(&["r-2".into()]));
        assert!(
            matches!(&view.asks[0].status, crate::session::AskStatus::Open { missing } if missing.iter().any(|m| m.contains("planned, not built"))),
            "{:?}",
            view.asks[0].status
        );
        assert_eq!(view.unfolded.len(), 1);
    }

    #[test]
    fn a_descope_needs_a_reason_and_shows_in_the_summary() {
        let (_dir, r) = project();
        record_prompt(&r, "s", "add dark mode").unwrap();
        file_asks(&r, "s", None, vec![ask("add dark mode", AskKind::Build)]).unwrap();
        assert!(resolve_ask(&r, "s", "a1", Resolution::Descoped("  ".into())).is_err());
        assert!(resolve_ask(&r, "s", "a1", Resolution::Answered).is_err(), "a build ask is not answered");
        assert!(resolve_ask(&r, "s", "a1", Resolution::Claims(vec!["r-404".into()])).is_err());
        resolve_ask(&r, "s", "a1", Resolution::Descoped("the theme system lands next sprint".into())).unwrap();
        let out = stop(&r, "s", no_flags, pass, no_probes);
        assert!(out.block.is_none());
        let summary = out.summary.unwrap();
        assert!(summary.contains("1 descoped (a1)"), "{summary}");
        assert!(!summary.contains("theme system"), "the reason stays off the stop line: {summary}");
        let view = session_view(&r, "s", |_| HashMap::new());
        assert_eq!(
            view.asks[0].status,
            crate::session::AskStatus::Descoped { reason: "the theme system lands next sprint".into() },
            "the reason is kept for the Session page"
        );
    }

    /// An action ask — commit, push — closes with `done` and a note, never by
    /// claims or by being descoped; it counts toward the session's done asks.
    #[test]
    fn an_action_ask_is_closed_as_done_with_a_note() {
        let (_dir, r) = project();
        record_prompt(&r, "s", "commit and push").unwrap();
        file_asks(&r, "s", None, vec![ask("commit and push", AskKind::Action)]).unwrap();
        assert!(resolve_ask(&r, "s", "a1", Resolution::Claims(vec!["r-1".into()])).is_err());
        assert!(resolve_ask(&r, "s", "a1", Resolution::Answered).is_err());
        assert!(resolve_ask(&r, "s", "a1", Resolution::Done("  ".into())).is_err(), "a note is required");
        let reason = stop(&r, "s", no_flags, pass, no_probes).block.expect("an undone action blocks");
        assert!(reason.contains("not done yet"), "{reason}");

        resolve_ask(&r, "s", "a1", Resolution::Done("pushed 079ea7b..8f88950".into())).unwrap();
        let view = session_view(&r, "s", |_| HashMap::new());
        assert_eq!(view.asks[0].status, crate::session::AskStatus::Done { note: "pushed 079ea7b..8f88950".into() });
        let summary = stop(&r, "s", no_flags, pass, no_probes).summary.unwrap();
        assert!(summary.contains("asks 1/1 done"), "{summary}");
    }

    /// Supporting files the agent attaches to an ask stop reading as unasked
    /// edits; edits outside the project are never recorded at all.
    #[test]
    fn attached_files_are_accounted_for_and_outside_edits_ignored() {
        let (dir, r) = project();
        record_prompt(&r, "s", "serve requests").unwrap();
        file_asks(&r, "s", None, vec![ask("serve", AskKind::Build)]).unwrap();
        resolve_ask(&r, "s", "a1", Resolution::Claims(vec!["r-1".into()])).unwrap();
        record_touch(&r, "s", "src/auth.rs").unwrap();
        record_touch(&r, "s", "src/helper.rs").unwrap();
        record_touch(&r, "s", "/elsewhere/probes/wt/src/auth.rs").unwrap();
        record_touch(&r, "s", &format!("{}/../sibling/x.rs", dir.path().display())).unwrap();

        let log = session_log(&r, "s");
        assert_eq!(log.touched, vec!["src/auth.rs", "src/helper.rs"], "outside files are not touches");
        assert_eq!(untraced_edits(&log, &working(&r).unwrap()), vec!["src/helper.rs"]);

        resolve_ask(&r, "s", "a1", Resolution::Files(vec![format!("{}/src/helper.rs", dir.path().display())]))
            .unwrap();
        assert!(untraced_edits(&session_log(&r, "s"), &working(&r).unwrap()).is_empty());
    }

    /// A claim with no When/While/If condition has nothing for a test to
    /// arrange, so it delivers on its touched code alone; a testable one still
    /// needs its passing verdict.
    #[test]
    fn an_untestable_claim_delivers_without_a_verdict() {
        let (_dir, r) = project();
        record_prompt(&r, "s", "log requests and serve them").unwrap();
        file_asks(&r, "s", None, vec![ask("log requests", AskKind::Build), ask("serve", AskKind::Build)]).unwrap();
        resolve_ask(&r, "s", "a1", Resolution::Claims(vec!["r-2".into()])).unwrap();
        resolve_ask(&r, "s", "a2", Resolution::Claims(vec!["r-1".into()])).unwrap();
        record_touch(&r, "s", "src/log.rs").unwrap();
        record_touch(&r, "s", "src/auth.rs").unwrap();

        let views = ask_views(&session_log(&r, "s"), &working(&r).unwrap(), &HashMap::new(), &HashSet::new());
        assert_eq!(views[0].status, crate::session::AskStatus::Delivered, "ubiquitous: {:?}", views[0].status);
        assert!(
            matches!(&views[1].status, crate::session::AskStatus::Open { missing }
                if missing.iter().any(|m| m.contains("no passing test verdict"))),
            "testable: {:?}",
            views[1].status
        );
    }

    /// Asks are filed as the agent reads the prompt: no wording in it — "a
    /// port request", "port the prototype" — makes the filing refuse.
    #[test]
    fn asks_are_filed_as_given_whatever_the_prompt_says() {
        let (_dir, r) = project();
        record_prompt(&r, "s", "file_asks flags this as a port request").unwrap();
        let (_, filed) = file_asks(&r, "s", None, Vec::new()).unwrap();
        assert!(filed.is_empty());

        record_prompt(&r, "s", "port the prototype flight model").unwrap();
        let (_, filed) =
            file_asks(&r, "s", Some("p2"), vec![ask("port the flight model", AskKind::Build)]).unwrap();
        assert_eq!(filed[0].text, "port the flight model");
        let sourced = NewAsk {
            text: "drag".into(),
            kind: AskKind::Build,
            source: Some("proto/flight.js".into()),
        };
        let (_, filed) = file_asks(&r, "s", Some("p2"), vec![sourced]).unwrap();
        assert_eq!(filed[0].source.as_deref(), Some("proto/flight.js"));
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
