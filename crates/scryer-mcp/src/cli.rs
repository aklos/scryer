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

use crate::helpers::{pending_changes, status_counts, StatusCounts};
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

/// `scryer-mcp statusline` — the Claude Code statusline command. Claude Code
/// invokes it on conversation updates with a session JSON blob on stdin and
/// shows the first stdout line under the prompt. Prints NOTHING when no model
/// is found (a blank segment, not an error), and always exits 0. Also callable
/// from a user's own statusline script — stdin is only read when it is piped,
/// and an empty/foreign payload falls back to the process cwd.
pub(crate) fn run_statusline() -> Result<(), Box<dyn std::error::Error>> {
    use std::io::{IsTerminal, Read};
    let mut input = String::new();
    if !std::io::stdin().is_terminal() {
        let _ = std::io::stdin().read_to_string(&mut input);
    }
    let start = match statusline_start_dir(&input).or_else(|| std::env::current_dir().ok()) {
        Some(d) => d,
        None => return Ok(()),
    };
    if let Some(project) = find_project(&start) {
        if let Some(c) = status_counts(&scryer_core::ModelRef::ProjectLocal(project)) {
            println!("{}", status_line(&c));
        }
    }
    Ok(())
}

/// Where to start the project walk, from Claude Code's statusline payload:
/// the workspace's project dir first (stable across `cd`), then its current
/// dir, then the event cwd. None when stdin wasn't that payload.
pub(crate) fn statusline_start_dir(input: &str) -> Option<PathBuf> {
    let v: serde_json::Value = serde_json::from_str(input).ok()?;
    ["/workspace/project_dir", "/workspace/current_dir", "/cwd"]
        .iter()
        .find_map(|p| v.pointer(p).and_then(|d| d.as_str()))
        .map(PathBuf::from)
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

/// `scryer-mcp check` — the opt-in CI gate. Exit 0 clean, 1 findings, 2 no
/// model / unusable repo (a misconfigured CI step should be loud, unlike
/// `status`). Default failure conditions: validator warnings on the working
/// view (the same trio the `validate_model` tool runs) and anchors whose code
/// is gone — broken/missing fingerprints plus a baseline-free existence sweep.
/// `--fail-on-drift` and `--fail-on-pending` gate stricter teams; without the
/// flags those dimensions are reported as notes, and dimensions that CANNOT be
/// verified (no committed baseline / reconcile anchor) say so instead of
/// passing silently.
pub(crate) fn run_check(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let mut fail_on_drift = false;
    let mut fail_on_pending = false;
    let mut start: Option<PathBuf> = None;
    for a in args {
        match a.as_str() {
            "--fail-on-drift" => fail_on_drift = true,
            "--fail-on-pending" => fail_on_pending = true,
            _ if !a.starts_with('-') && start.is_none() => start = Some(PathBuf::from(a)),
            other => {
                eprintln!(
                    "unknown argument '{other}'\nusage: scryer-mcp check [--fail-on-drift] [--fail-on-pending] [path]"
                );
                std::process::exit(2);
            }
        }
    }
    let start = match start {
        Some(p) => p,
        None => std::env::current_dir()?,
    };
    let Some(project) = find_project(&start) else {
        eprintln!("scryer check: no model found (searched up from {})", start.display());
        std::process::exit(2);
    };
    let r = scryer_core::ModelRef::ProjectLocal(project.clone());
    let report = match check_report(&r, fail_on_drift, fail_on_pending) {
        Ok(rep) => rep,
        Err(e) => {
            eprintln!("scryer check: {e}");
            std::process::exit(2);
        }
    };
    if report.failures.is_empty() {
        println!("scryer check: clean ({})", project.display());
    } else {
        println!("scryer check: {} finding(s)", report.failures.len());
        for f in &report.failures {
            println!("- {f}");
        }
    }
    for n in &report.notes {
        println!("note: {n}");
    }
    if !report.failures.is_empty() {
        std::process::exit(1);
    }
    Ok(())
}

pub(crate) struct CheckReport {
    /// Findings that fail the gate.
    pub failures: Vec<String>,
    /// Non-gating observations: dimensions opted out of, or unverifiable ones
    /// — named explicitly so a vacuous pass never reads as a real one.
    pub notes: Vec<String>,
}

pub(crate) fn check_report(
    r: &scryer_core::ModelRef,
    fail_on_drift: bool,
    fail_on_pending: bool,
) -> Result<CheckReport, String> {
    use scryer_extract::anchors::AnchorState;

    let committed = scryer_core::read_model_at(r)?;
    let planned = scryer_core::read_planned_at(r)?;
    let working = scryer_core::working_view(&committed, &planned);
    let project = r.project_path();
    let mut failures: Vec<String> = Vec::new();
    let mut notes: Vec<String> = Vec::new();

    // 1) The validator trio the `validate_model` tool runs, on the working view.
    let mut warnings = scryer_core::validate::validate(&working);
    warnings.extend(scryer_core::validate::validate_coverage(&working, project));
    warnings.extend(scryer_extract::anchors::whole_symbol_warnings(&working, project));
    failures.extend(warnings.into_iter().map(|w| format!("validator: {w}")));

    // 2) Anchor fingerprints, when a committed baseline exists. Changed spans
    //    are drift (unreconciled churn), not breakage — they gate only under
    //    --fail-on-drift. Broken/missing spans always gate: the model points
    //    at code that is gone.
    let mut swept: std::collections::HashSet<(String, String)> = Default::default();
    if r.anchors_path().exists() {
        let check = scryer_extract::anchors::check_anchors(r)?;
        let mut changed = 0usize;
        for o in &check.observations {
            match o.state {
                AnchorState::Changed => changed += 1,
                AnchorState::Broken => {
                    swept.insert((o.key.clone(), o.file.clone()));
                    failures.push(format!(
                        "anchor: '{}' ({}) → {}{} — anchored span not found",
                        o.key,
                        o.host_name,
                        o.file,
                        o.symbol.as_deref().map(|s| format!(" `{s}`")).unwrap_or_default()
                    ));
                }
                AnchorState::FileMissing => {
                    swept.insert((o.key.clone(), o.file.clone()));
                    failures.push(format!(
                        "anchor: '{}' ({}) → {} — file is gone",
                        o.key, o.host_name, o.file
                    ));
                }
            }
        }
        if changed > 0 {
            let line = format!("{changed} anchored claim(s) changed since the last reconcile");
            if fail_on_drift {
                failures.push(format!("drift: {line}"));
            } else {
                notes.push(format!("{line} (not gating — pass --fail-on-drift)"));
            }
        }
    } else {
        notes.push(
            "anchor fingerprints unverified — no baseline (.scryer/.anchors.json). A reconcile \
             (get_health / get_drift over MCP) writes it; commit it for CI to check against."
                .into(),
        );
    }

    // 3) Baseline-free existence sweep: an exact-path anchor whose file is
    //    gone fails even before any reconcile baseline exists. Globs are
    //    completeness's domain (get_health), not a gate. Test anchors sweep
    //    too, under their namespaced key — a claim's attached test that no
    //    longer exists is exactly what a CI gate is for.
    let mut keyed: Vec<(String, &Vec<scryer_core::SourceLocation>)> = working
        .source_map
        .iter()
        .map(|(k, v)| (k.clone(), v))
        .chain(working.test_map.iter().map(|(k, v)| (scryer_core::test_key(k), v)))
        .collect();
    keyed.sort_by(|a, b| a.0.cmp(&b.0));
    for (key, locs) in keyed {
        for loc in locs {
            if loc.pattern.contains(['*', '?', '[']) {
                continue;
            }
            if swept.contains(&(key.clone(), loc.pattern.clone())) {
                continue; // already reported off its fingerprint
            }
            if !project.join(&loc.pattern).exists() {
                failures.push(format!("anchor: '{}' → {} — file does not exist", key, loc.pattern));
            }
        }
    }

    // 4) Scope drift (mtime ∩ git-diff when the anchor carries a commit, so a
    //    fresh CI checkout doesn't false-alarm).
    if r.sync_path().exists() {
        let sync = scryer_core::read_sync_state(r);
        let scopes = scryer_core::drift::drifted_scopes(&committed, project, &sync);
        if !scopes.is_empty() {
            if fail_on_drift {
                for s in &scopes {
                    let mut preview = s.changed_files.iter().take(3).cloned().collect::<Vec<_>>().join(", ");
                    if s.changed_files.len() > 3 {
                        preview.push_str(&format!(", … {} more", s.changed_files.len() - 3));
                    }
                    failures.push(format!(
                        "drift: '{}' — {} changed file(s): {}",
                        s.node_name,
                        s.changed_files.len(),
                        preview
                    ));
                }
            } else {
                notes.push(format!(
                    "{} drifted scope(s) (not gating — pass --fail-on-drift)",
                    scopes.len()
                ));
            }
        }
    } else if fail_on_drift {
        notes.push(
            "drift unverifiable — no reconcile anchor (.scryer/.sync). A reconcile writes it; \
             commit it for CI to measure against."
                .into(),
        );
    }

    // 5) Outstanding plan work.
    let pending = pending_changes(&committed, &planned);
    if !pending.is_empty() {
        if fail_on_pending {
            for ch in pending.iter().take(10) {
                failures.push(format!("pending: {} '{}'", kind_label(&ch.kind), ch.label));
            }
            if pending.len() > 10 {
                failures.push(format!("pending: … {} more", pending.len() - 10));
            }
        } else {
            notes.push(format!(
                "{} pending plan item(s) (not gating — pass --fail-on-pending)",
                pending.len()
            ));
        }
    }

    Ok(CheckReport { failures, notes })
}

fn kind_label(k: &scryer_core::diff::ElementKind) -> &'static str {
    use scryer_core::diff::ElementKind as EK;
    match k {
        EK::Node => "node",
        EK::Link => "link",
        EK::Responsibility => "responsibility",
        EK::Property => "property",
        EK::Group => "group",
    }
}

/// Outcome of a statusline install attempt.
pub(crate) enum StatuslineInstall {
    /// Written (fresh, or an idempotent refresh of our own entry).
    Installed(PathBuf),
    /// A foreign statusline is configured — left untouched; the caller prints
    /// how to compose instead.
    ForeignExists(PathBuf),
}

/// Is this `statusLine` entry ours? Identified by the command invoking the
/// scryer-mcp binary's `statusline` subcommand — the marker
/// [`install_statusline`] writes (mirrors `is_scryer_hook_entry` in the app).
fn is_scryer_statusline(entry: &serde_json::Value) -> bool {
    entry["command"]
        .as_str()
        .is_some_and(|c| c.contains("scryer-mcp") && c.trim_end().ends_with(" statusline"))
}

/// Register `scryer-mcp statusline` as the Claude Code statusline for this
/// project, in the personal settings file (same conventions as the app's hook
/// install: absolute binary path, refuse to overwrite invalid JSON). Unlike
/// hooks, `statusLine` is a SINGLE slot — a whole-line replacement — so a
/// foreign entry is never clobbered: the caller tells the user how to append
/// our segment to their own script instead.
pub(crate) fn install_statusline(
    project: &Path,
    binary_path: &str,
) -> Result<StatuslineInstall, String> {
    let claude_dir = project.join(".claude");
    let settings_path = claude_dir.join("settings.local.json");
    let mut root: serde_json::Value = if settings_path.exists() {
        let contents = std::fs::read_to_string(&settings_path).map_err(|e| e.to_string())?;
        serde_json::from_str(&contents).map_err(|e| {
            format!(
                "{} is not valid JSON ({e}); refusing to overwrite it — fix the file and retry.",
                settings_path.display()
            )
        })?
    } else {
        serde_json::json!({})
    };

    let existing = root.get("statusLine");
    if existing.is_some_and(|e| !e.is_null() && !is_scryer_statusline(e)) {
        return Ok(StatuslineInstall::ForeignExists(settings_path));
    }

    root["statusLine"] = serde_json::json!({
        "type": "command",
        "command": format!("\"{binary_path}\" statusline"),
    });
    std::fs::create_dir_all(&claude_dir).map_err(|e| e.to_string())?;
    std::fs::write(
        &settings_path,
        serde_json::to_string_pretty(&root).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok(StatuslineInstall::Installed(settings_path))
}

pub(crate) fn status_line(c: &StatusCounts) -> String {
    // "2 changes in flight" — the ledger's in-progress workstreams; silent in
    // the serial (unfiled) workflow.
    let changes = match c.open_changes {
        0 => String::new(),
        1 => " · 1 change in flight".to_string(),
        n => format!(" · {n} changes in flight"),
    };
    // Both altitudes, always together: the ELEMENT queue (what the agent's own
    // header and get_pending report — a node with three reworded claims counts
    // three) and the carriers it lands on (the cards the canvas draws).
    // Reporting only carriers is what let this line read "5 pending" while the
    // agent in the same terminal read 23.
    let pending = pending_phrase(c);
    match &c.baseline {
        None => format!("scryer: {pending} · no reconcile anchor yet{changes}"),
        Some(b) => format!(
            "scryer: {pending} · {} drift scope(s) · anchors: {} broken, {} changed{changes}",
            b.drift_scopes, b.anchors_broken, b.anchors_changed
        ),
    }
}

/// "23 pending across 8 nodes" — the shared phrasing for outstanding plan work.
/// An empty plan drops the breakdown: "0 pending across 0 nodes" is noise.
/// Mirrors `planCountLabel` (src/changeMarks.ts).
pub(crate) fn pending_phrase(c: &StatusCounts) -> String {
    if c.pending == 0 {
        return "0 pending".to_string();
    }
    format!(
        "{} pending across {} node{}",
        c.pending,
        c.carriers,
        if c.carriers == 1 { "" } else { "s" }
    )
}

fn status_json(c: &StatusCounts) -> String {
    // `pending` is the element queue — the same number get_pending returns —
    // with `carriers` beside it for the node/group altitude the canvas draws.
    let v = serde_json::json!({
        "pending": c.pending,
        "carriers": c.carriers,
        "openChanges": c.open_changes,
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
        assert_eq!(status_line(&c), "scryer: 1 pending across 1 node · no reconcile anchor yet");
    }

    /// The line reports BOTH altitudes, because they differ: three reworded
    /// claims on one node are three items in the agent's queue and one card on
    /// the canvas. Quoting only the carrier count is what made the terminal
    /// read "1 pending" beside an agent reading 3.
    #[test]
    fn status_line_reports_elements_and_the_carriers_they_sit_on() {
        let dir = tempfile::tempdir().unwrap();
        let r = ModelRef::ProjectLocal(dir.path().to_path_buf());
        let mut committed = ScryModel::new();
        let mut sys = node("sys", Kind::System, "Acme", None);
        for i in 1..=3 {
            sys.responsibilities.push(
                serde_json::from_value(serde_json::json!({
                    "id": format!("r{i}"), "statement": format!("does thing {i}"),
                }))
                .unwrap(),
            );
        }
        committed.nodes.push(sys.clone());
        scryer_core::write_model_at(&r, &committed).unwrap();

        let mut plan = ScryModel::new();
        let mut planned_sys = sys;
        for (i, resp) in planned_sys.responsibilities.iter_mut().enumerate() {
            resp.statement = format!("does thing {}, revised", i + 1);
        }
        plan.nodes.push(planned_sys);
        scryer_core::write_planned_at(&r, &plan).unwrap();

        let c = status_counts(&r).unwrap();
        assert_eq!(c.pending, 3);
        assert_eq!(c.carriers, 1);
        assert_eq!(status_line(&c), "scryer: 3 pending across 1 node · no reconcile anchor yet");
    }

    /// A clean, fully-anchored model passes — and the unverifiable anchor
    /// dimension is NAMED in the notes, never silently vacuous. Pending work
    /// is a note by default and a listed failure under --fail-on-pending.
    #[test]
    fn check_gates_pending_only_when_asked() {
        let dir = tempfile::tempdir().unwrap();
        let r = ModelRef::ProjectLocal(dir.path().to_path_buf());
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/auth.rs"), "fn verify() {}\n").unwrap();
        let mut m = ScryModel::new();
        m.nodes.push(node("sys", Kind::System, "Acme", None));
        let mut api = node("api", Kind::Container, "API", Some("sys"));
        api.responsibilities = vec![serde_json::from_value(
            serde_json::json!({ "id": "r-1", "statement": "verifies requests" }),
        )
        .unwrap()];
        m.nodes.push(api);
        m.source_map.insert(
            "r-1".into(),
            vec![serde_json::from_value(serde_json::json!({ "pattern": "src/auth.rs" })).unwrap()],
        );
        scryer_core::write_model_at(&r, &m).unwrap();

        let rep = check_report(&r, false, false).unwrap();
        assert!(rep.failures.is_empty(), "clean model: {:?}", rep.failures);
        assert!(
            rep.notes.iter().any(|n| n.contains("no baseline")),
            "unverified anchors must be named: {:?}",
            rep.notes
        );

        // Add a pending plan item: a note by default, a failure when gating.
        let mut plan = m.clone();
        plan.nodes.push(node("worker", Kind::Container, "Worker", Some("sys")));
        plan.links.push(scryer_core::Link {
            id: "l-1".into(),
            src: "api".into(),
            dst: "worker".into(),
            label: "enqueues".into(),
            method: None,
        });
        scryer_core::write_planned_at(&r, &plan).unwrap();
        let rep = check_report(&r, false, false).unwrap();
        assert!(rep.failures.is_empty(), "{:?}", rep.failures);
        assert!(rep.notes.iter().any(|n| n.contains("pending plan item")));
        let rep = check_report(&r, false, true).unwrap();
        assert!(
            rep.failures.iter().any(|f| f.starts_with("pending: node 'Worker'")),
            "{:?}",
            rep.failures
        );
    }

    /// A claim's attached test that no longer exists gates like any exact-path
    /// anchor, under its `test:` key — a "test-backed" claim whose test is
    /// gone is exactly what a CI gate exists to catch.
    #[test]
    fn check_fails_on_missing_attached_test_file() {
        let dir = tempfile::tempdir().unwrap();
        let r = ModelRef::ProjectLocal(dir.path().to_path_buf());
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/auth.rs"), "fn verify() {}\n").unwrap();
        let mut m = ScryModel::new();
        m.nodes.push(node("sys", Kind::System, "Acme", None));
        let mut api = node("api", Kind::Container, "API", Some("sys"));
        api.responsibilities = vec![serde_json::from_value(
            serde_json::json!({ "id": "r-1", "statement": "verifies requests" }),
        )
        .unwrap()];
        m.nodes.push(api);
        m.source_map.insert(
            "r-1".into(),
            vec![serde_json::from_value(serde_json::json!({ "pattern": "src/auth.rs" })).unwrap()],
        );
        m.test_map.insert(
            "r-1".into(),
            vec![serde_json::from_value(serde_json::json!({ "pattern": "tests/gone.rs" }))
                .unwrap()],
        );
        scryer_core::write_model_at(&r, &m).unwrap();

        let rep = check_report(&r, false, false).unwrap();
        assert!(
            rep.failures
                .iter()
                .any(|f| f.contains("test:r-1") && f.contains("tests/gone.rs")),
            "missing attached test gates: {:?}",
            rep.failures
        );
    }

    /// An exact-path anchor whose file is gone fails the gate even with no
    /// fingerprint baseline — and with a baseline, the same anchor is reported
    /// once off its fingerprint, not twice.
    #[test]
    fn check_fails_on_missing_anchor_files_without_duplicates() {
        let dir = tempfile::tempdir().unwrap();
        let r = ModelRef::ProjectLocal(dir.path().to_path_buf());
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/auth.rs"), "fn verify() {}\n").unwrap();
        let mut m = ScryModel::new();
        m.nodes.push(node("sys", Kind::System, "Acme", None));
        let mut api = node("api", Kind::Container, "API", Some("sys"));
        api.responsibilities = vec![serde_json::from_value(
            serde_json::json!({ "id": "r-1", "statement": "verifies requests" }),
        )
        .unwrap()];
        m.nodes.push(api);
        m.source_map.insert(
            "r-1".into(),
            vec![serde_json::from_value(serde_json::json!({ "pattern": "src/auth.rs" })).unwrap()],
        );
        scryer_core::write_model_at(&r, &m).unwrap();

        // No baseline: the existence sweep catches the deleted file.
        std::fs::remove_file(dir.path().join("src/auth.rs")).unwrap();
        let rep = check_report(&r, false, false).unwrap();
        let hits = rep.failures.iter().filter(|f| f.contains("src/auth.rs")).count();
        assert_eq!(hits, 1, "sweep reports the gone file once: {:?}", rep.failures);

        // With a baseline: reported off the fingerprint, still exactly once.
        std::fs::write(dir.path().join("src/auth.rs"), "fn verify() {}\n").unwrap();
        scryer_extract::anchors::write_baseline(&r).unwrap();
        std::fs::remove_file(dir.path().join("src/auth.rs")).unwrap();
        let rep = check_report(&r, false, false).unwrap();
        let hits: Vec<&String> =
            rep.failures.iter().filter(|f| f.contains("src/auth.rs")).collect();
        assert_eq!(hits.len(), 1, "fingerprint + sweep must not double-report: {:?}", rep.failures);
        assert!(hits[0].contains("file is gone"), "{:?}", hits);
    }

    /// Validator warnings gate by default — here a source-map entry pointing
    /// at an id the model doesn't hold.
    #[test]
    fn check_fails_on_validator_warnings() {
        let dir = tempfile::tempdir().unwrap();
        let r = ModelRef::ProjectLocal(dir.path().to_path_buf());
        let mut m = ScryModel::new();
        m.nodes.push(node("sys", Kind::System, "Acme", None));
        m.source_map.insert(
            "ghost-id".into(),
            vec![serde_json::from_value(serde_json::json!({ "pattern": "src/x.rs" })).unwrap()],
        );
        scryer_core::write_model_at(&r, &m).unwrap();

        let rep = check_report(&r, false, false).unwrap();
        assert!(
            rep.failures.iter().any(|f| f.starts_with("validator:") && f.contains("ghost-id")),
            "{:?}",
            rep.failures
        );
    }

    /// The statusline start dir prefers the workspace's project dir, falls
    /// through its variants, and rejects a non-payload stdin.
    #[test]
    fn statusline_start_dir_prefers_project_dir() {
        let full = serde_json::json!({
            "cwd": "/c",
            "workspace": { "current_dir": "/b", "project_dir": "/a" },
        })
        .to_string();
        assert_eq!(statusline_start_dir(&full), Some(PathBuf::from("/a")));
        let cwd_only = serde_json::json!({ "cwd": "/c" }).to_string();
        assert_eq!(statusline_start_dir(&cwd_only), Some(PathBuf::from("/c")));
        assert_eq!(statusline_start_dir(""), None);
        assert_eq!(statusline_start_dir("not json"), None);
    }

    /// Install is idempotent on our own entry, preserves the rest of the
    /// settings file, and never clobbers a foreign statusline — the single
    /// slot belongs to the user.
    #[test]
    fn statusline_install_is_idempotent_and_never_clobbers() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path();
        let settings = project.join(".claude/settings.local.json");
        std::fs::create_dir_all(project.join(".claude")).unwrap();
        std::fs::write(
            &settings,
            serde_json::json!({ "permissions": { "allow": ["mcp__scryer"] } }).to_string(),
        )
        .unwrap();

        // Fresh install, then a refresh from a moved binary: still one entry,
        // pointing at the new path, other settings intact.
        for path in ["/opt/scryer/scryer-mcp", "/usr/local/bin/scryer-mcp"] {
            match install_statusline(project, path).unwrap() {
                StatuslineInstall::Installed(p) => assert_eq!(p, settings),
                StatuslineInstall::ForeignExists(_) => panic!("our own entry must refresh"),
            }
        }
        let root: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&settings).unwrap()).unwrap();
        assert_eq!(
            root["statusLine"]["command"],
            "\"/usr/local/bin/scryer-mcp\" statusline"
        );
        assert_eq!(root["permissions"]["allow"][0], "mcp__scryer");

        // A foreign statusline is reported, not replaced.
        std::fs::write(
            &settings,
            serde_json::json!({
                "statusLine": { "type": "command", "command": "my-fancy-prompt.sh" }
            })
            .to_string(),
        )
        .unwrap();
        match install_statusline(project, "/opt/scryer/scryer-mcp").unwrap() {
            StatuslineInstall::ForeignExists(p) => assert_eq!(p, settings),
            StatuslineInstall::Installed(_) => panic!("foreign statusline was clobbered"),
        }
        let root: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&settings).unwrap()).unwrap();
        assert_eq!(root["statusLine"]["command"], "my-fancy-prompt.sh");

        // Invalid JSON is refused, not overwritten.
        std::fs::write(&settings, "{ not json").unwrap();
        assert!(install_statusline(project, "/opt/scryer/scryer-mcp").is_err());
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
