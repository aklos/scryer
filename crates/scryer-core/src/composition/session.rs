//! The session hooks' entry points: read the session's log and the model,
//! answer, and append what the answer implies. The hook client calls these
//! in-process — no app, no server.

use crate::application::hooks::{
    ask_views, asks_gate, close_view, session_summary, untraced_edits, AnchorFlag, AskView, CloseView,
};
use crate::application::locate::LocateReport;
use crate::composition::locate::locate_at;
use crate::composition::model_store::{read_model_at, read_planned_at};
use crate::domain::session::{asks_for_parity, fnv1a64, relativize, Ask, AskKind, SessionLog};
use serde::Serialize;
use std::collections::HashMap;
use crate::infrastructure::session_store;
use crate::ModelRef;

pub use crate::domain::session::{SessionEntry, SessionEvent};
pub use session_store::{ensure_dir as ensure_sessions_dir, list as list_sessions, read as read_session};

/// The fold of `session`'s log so far.
pub fn session_log(r: &ModelRef, session: &str) -> SessionLog {
    SessionLog::from_events(&session_store::read(r, session))
}

/// Record that `session` edited `file` (any path form the harness gave).
pub fn record_touch(r: &ModelRef, session: &str, file: &str) -> Result<(), String> {
    let file = relativize(r.project_path(), file);
    session_store::append(r, session, SessionEvent::Touch { file })
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

/// Break prompt `prompt` (default: the oldest one not filed yet) into asks.
/// A prompt that asks for parity with a source ("port X", "match the
/// prototype") is refused unless its build asks name the `source` and list
/// its features one per ask — one vague "port X" ask is how a missing
/// throttle passes 298 green tests.
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
    let text = log
        .prompts
        .iter()
        .find(|(id, _)| *id == prompt)
        .map(|(_, t)| t.as_str())
        .ok_or_else(|| format!("no prompt '{prompt}' in this session"))?;
    if let Some(a) = asks.iter().find(|a| a.text.trim().is_empty()) {
        return Err(format!("an ask needs text (got {:?})", a.text));
    }
    if asks_for_parity(text) {
        let sourced = asks.iter().filter(|a| a.kind == AskKind::Build && a.source.is_some()).count();
        if sourced < 2 {
            return Err(format!(
                "prompt {prompt} asks for parity with a source (\"{}\"). Read the source and file \
                 ONE build ask per feature it has, each with `source` set to the path it comes \
                 from — not one ask for the whole port.",
                text.chars().take(120).collect::<String>()
            ));
        }
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
}

pub fn resolve_ask(r: &ModelRef, session: &str, id: &str, how: Resolution) -> Result<(), String> {
    let log = session_log(r, session);
    let ask = log.ask(id).ok_or_else(|| format!("no ask '{id}' in this session"))?;
    let event = match how {
        Resolution::Claims(claims) => {
            if ask.ask.kind == AskKind::Answer {
                return Err(format!("{id} is an answer ask — resolve it with answered: true"));
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
            if ask.ask.kind == AskKind::Build {
                return Err(format!(
                    "{id} is a build ask — it is delivered by claims (claims: [...]), not by an answer"
                ));
            }
            SessionEvent::AskAnswered { id: id.to_string() }
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
        asks: ask_views(&log, &working, &verified),
        touched,
        untraced: untraced_edits(&log, &working),
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
) -> StopOutcome {
    let log = session_log(r, session);
    let working = working(r).unwrap_or_default();
    let linked: Vec<String> = log.asks.iter().flat_map(|a| a.claims.iter().cloned()).collect();
    let views = ask_views(&log, &working, &verified(&linked));

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
    let close = close_gate(r, session, flags);
    if !close.needs_reconcile.is_empty() {
        reasons.push(reconcile_reason(&close));
    }
    if !reasons.is_empty() {
        return StopOutcome { block: Some(reasons.join("\n\n")), summary: None };
    }

    let summary = session_summary(&views, &untraced_edits(&log, &working))
        .filter(|s| log.last_summary.as_deref() != Some(s.as_str()));
    if let Some(s) = &summary {
        let _ = session_store::append(r, session, SessionEvent::Summary { text: s.clone() });
    }
    StopOutcome { block: None, summary }
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
                  "responsibilities": [{ "id": "r-1", "statement": "serves requests" }] }
            ],
            "sourceMap": { "r-1": [{ "pattern": "src/auth.rs", "symbol": "verify" }] }
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

        let out = stop(&r, "s", no_flags, pass);
        let reason = out.block.expect("an unfiled prompt blocks");
        assert!(reason.contains("prompt p1 was never broken into asks"), "{reason}");
        assert!(!reason.to_lowercase().contains("review"), "never asks the user for review: {reason}");
        assert!(stop(&r, "s", no_flags, pass).block.is_none(), "never twice for the same prompt");

        let (_, filed) = file_asks(
            &r,
            "s",
            None,
            vec![ask("verify tokens", AskKind::Build), ask("explain the cache", AskKind::Answer)],
        )
        .unwrap();
        assert_eq!(filed.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(), vec!["a1", "a2"]);

        let reason = stop(&r, "s", no_flags, pass).block.expect("open asks block");
        assert!(reason.contains("a1") && reason.contains("no claim delivers it"), "{reason}");
        assert!(reason.contains("a2") && reason.contains("not answered"), "{reason}");
        let quiet = stop(&r, "s", no_flags, pass);
        assert!(quiet.block.is_none(), "one block per ask");
        assert!(quiet.summary.as_deref().unwrap().contains("2 open (a1, a2)"), "{quiet:?}");

        // Linked and verified, but the session never touched its code: open.
        resolve_ask(&r, "s", "a1", Resolution::Claims(vec!["r-1".into()])).unwrap();
        resolve_ask(&r, "s", "a2", Resolution::Answered).unwrap();
        let log = session_log(&r, "s");
        let working = working(&r).unwrap();
        let views = ask_views(&log, &working, &pass(&["r-1".into()]));
        assert!(matches!(&views[0].status, crate::session::AskStatus::Open { missing } if missing[0].contains("edited none")));
        assert_eq!(views[1].status, crate::session::AskStatus::Answered);
        let failing = ask_views(&log, &working, &HashMap::new());
        assert!(matches!(&failing[0].status, crate::session::AskStatus::Open { missing } if missing.iter().any(|m| m.contains("no passing test verdict"))));

        record_touch(&r, "s", "src/auth.rs").unwrap();
        record_touch(&r, "s", "src/unrelated.rs").unwrap();
        let out = stop(&r, "s", no_flags, pass);
        assert!(out.block.is_none());
        let summary = out.summary.unwrap();
        assert!(summary.contains("asks 2/2 done"), "{summary}");
        assert!(summary.contains("edits no ask accounts for: src/unrelated.rs"), "{summary}");
        assert!(stop(&r, "s", no_flags, pass).summary.is_none(), "an unchanged summary stays silent");

        // A new prompt arms the gate again, for that prompt only.
        record_prompt(&r, "s", "thanks").unwrap();
        assert!(stop(&r, "s", no_flags, pass).block.unwrap().contains("prompt p2"));
        file_asks(&r, "s", Some("p2"), vec![]).unwrap();
        assert!(stop(&r, "s", no_flags, pass).block.is_none());
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
        let out = stop(&r, "s", no_flags, pass);
        assert!(out.block.is_none());
        assert!(out.summary.unwrap().contains("descoped a1 \"add dark mode\": the theme system lands next sprint"));
    }

    /// "Port X" asks must be split into the source's features.
    #[test]
    fn a_parity_prompt_needs_one_sourced_ask_per_feature() {
        let (_dir, r) = project();
        record_prompt(&r, "s", "port the prototype flight model").unwrap();
        let vague = file_asks(&r, "s", None, vec![ask("port the flight model", AskKind::Build)]);
        assert!(vague.unwrap_err().contains("ONE build ask per feature"));
        let sourced = |t: &str| NewAsk {
            text: t.into(),
            kind: AskKind::Build,
            source: Some("proto/flight.js".into()),
        };
        let (_, filed) =
            file_asks(&r, "s", None, vec![sourced("thrust from throttle"), sourced("drag")]).unwrap();
        assert_eq!(filed[0].source.as_deref(), Some("proto/flight.js"));
        assert!(crate::session::asks_for_parity("match the prototype's HUD"));
        assert!(!crate::session::asks_for_parity("fix the report export"), "no false hit on 'report'");
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
