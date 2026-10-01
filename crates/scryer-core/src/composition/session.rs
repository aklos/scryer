//! The session hooks' entry points: read the session's log and the model,
//! answer, and append what the answer implies. The hook client calls these
//! in-process — no app, no server.

use crate::application::hooks::{close_view, AnchorFlag, CloseView};
use crate::application::locate::LocateReport;
use crate::composition::locate::locate_at;
use crate::composition::model_store::{read_model_at, read_planned_at};
use crate::domain::session::{fnv1a64, relativize, SessionLog};
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
}
