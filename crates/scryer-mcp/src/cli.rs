//! `scryer-mcp status` — the model's loop state as a one-liner for humans.
//!
//! Reads the model straight from disk: no app, no MCP session. The project is
//! found by walking up from the cwd (or an explicit path argument) to the
//! first directory holding `.scryer/` — the first hit ends the walk, so a
//! nested project never reports its parent's model. Every legal invocation
//! exits 0, including "no model found", so the command is safe to call
//! unconditionally from prompts and statuslines.
//!
//! The line matches the hook endpoint's `statusLine` wording (both are the
//! same ambient signal, one pulled, one pushed), and `--json` returns the
//! counts under the same keys as `GET /status`.

use crate::helpers::{status_counts, StatusCounts};
use std::path::{Path, PathBuf};

pub(crate) fn run_status(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let mut json = false;
    let mut start: Option<PathBuf> = None;
    for a in args {
        match a.as_str() {
            "--json" => json = true,
            _ if !a.starts_with('-') && start.is_none() => start = Some(PathBuf::from(a)),
            other => {
                eprintln!("unknown argument '{other}'\nusage: scryer-mcp status [--json] [path]");
                std::process::exit(2);
            }
        }
    }
    let start = match start {
        Some(p) => p,
        None => std::env::current_dir()?,
    };

    let counts = find_project(&start).and_then(|project| {
        status_counts(&scryer_core::ModelRef::ProjectLocal(project))
    });
    match counts {
        Some(c) if json => println!("{}", status_json(&c)),
        Some(c) => println!("{}", status_line(&c)),
        None if json => println!("{}", serde_json::json!({ "model": false })),
        None => println!("scryer: no model (searched up from {})", start.display()),
    }
    Ok(())
}

/// Walk up from `start` to the first directory containing `.scryer/`. The
/// first hit ends the walk whether or not a readable model is inside — an
/// empty or broken `.scryer` is not a reason to keep climbing into a parent
/// project's model (same rule as the hook client's discovery).
pub(crate) fn find_project(start: &Path) -> Option<PathBuf> {
    let mut dir = Some(start.to_path_buf());
    while let Some(d) = dir {
        if d.join(".scryer").is_dir() {
            return Some(d);
        }
        dir = d.parent().map(Path::to_path_buf);
    }
    None
}

pub(crate) fn status_line(c: &StatusCounts) -> String {
    match &c.baseline {
        None => format!("scryer: {} pending · no reconcile anchor yet", c.pending),
        Some(b) => format!(
            "scryer: {} pending · {} drift scope(s) · anchors: {} broken, {} changed",
            c.pending, b.drift_scopes, b.anchors_broken, b.anchors_changed
        ),
    }
}

fn status_json(c: &StatusCounts) -> String {
    let v = serde_json::json!({
        "pending": c.pending,
        "driftScopes": c.baseline.as_ref().map(|b| b.drift_scopes),
        "anchorsBroken": c.baseline.as_ref().map(|b| b.anchors_broken),
        "anchorsChanged": c.baseline.as_ref().map(|b| b.anchors_changed),
        "statusLine": status_line(c),
    });
    serde_json::to_string_pretty(&v).unwrap_or_else(|_| "{}".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use scryer_core::{Kind, ModelRef, Node, ScryModel};

    fn node(id: &str, kind: Kind, name: &str, parent: Option<&str>) -> Node {
        let kind_str = match kind {
            Kind::Person => "person",
            Kind::System => "system",
            Kind::Container => "container",
            Kind::Component => "component",
            Kind::Symbol => "symbol",
        };
        serde_json::from_value(serde_json::json!({
            "id": id, "kind": kind_str, "name": name, "parentId": parent,
        }))
        .unwrap()
    }

    /// The walk stops at the FIRST `.scryer` — a nested project must never
    /// report its parent's model — and finds it from a deep subdirectory.
    #[test]
    fn find_project_stops_at_the_nearest_scryer_dir() {
        let dir = tempfile::tempdir().unwrap();
        let outer = dir.path();
        let inner = outer.join("apps/web");
        std::fs::create_dir_all(outer.join(".scryer")).unwrap();
        std::fs::create_dir_all(inner.join(".scryer")).unwrap();
        std::fs::create_dir_all(inner.join("src/deep")).unwrap();

        assert_eq!(find_project(&inner.join("src/deep")).unwrap(), inner);
        assert_eq!(find_project(&outer.join("docs")).as_deref(), Some(outer));
    }

    /// A never-reconciled model reports its pending count and says the drift
    /// baseline does not exist yet — no fake zeros.
    #[test]
    fn status_line_before_any_reconcile_admits_no_baseline() {
        let dir = tempfile::tempdir().unwrap();
        let r = ModelRef::ProjectLocal(dir.path().to_path_buf());
        let committed = ScryModel::new();
        scryer_core::write_model_at(&r, &committed).unwrap();
        let mut plan = ScryModel::new();
        plan.nodes.push(node("sys", Kind::System, "Acme", None));
        scryer_core::write_planned_at(&r, &plan).unwrap();

        let c = status_counts(&r).unwrap();
        assert_eq!(status_line(&c), "scryer: 1 pending · no reconcile anchor yet");
    }

    /// With a reconcile baseline in place, the line carries real drift and
    /// anchor counts (all quiet here — nothing changed since the baseline).
    #[test]
    fn status_line_after_reconcile_reports_counts() {
        let dir = tempfile::tempdir().unwrap();
        let r = ModelRef::ProjectLocal(dir.path().to_path_buf());
        let mut m = ScryModel::new();
        m.nodes.push(node("sys", Kind::System, "Acme", None));
        scryer_core::write_model_at(&r, &m).unwrap();
        scryer_core::write_sync_state(&r, &scryer_core::drift::SyncState::anchored_now(None))
            .unwrap();
        scryer_extract::anchors::write_baseline(&r).unwrap();

        let c = status_counts(&r).unwrap();
        assert_eq!(
            status_line(&c),
            "scryer: 0 pending · 0 drift scope(s) · anchors: 0 broken, 0 changed"
        );
    }
}
