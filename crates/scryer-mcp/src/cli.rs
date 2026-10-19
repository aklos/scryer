//! The command-line side of scryer-mcp: the status line, the CI gate, the
//! claim lint, and agent-integration setup.
//!
//! Everything reads the C4 tree derived from the code and each claim's
//! derived status — never the authored model. The project is found by walking
//! up from the cwd (or an explicit path argument) to the first directory
//! holding `.scryer/`; the first hit ends the walk, so a nested project never
//! reports its parent's. `status` and `statusline` exit 0 on every legal
//! invocation, so they are safe to call from prompts; `check` and `lint` are
//! gates and exit non-zero on findings.
//!
//! Prints the single-line statusline segment Claude Code shows under the prompt, staying silent when no model is found
//!
//! Walks up from a start directory to the nearest .scryer/, stopping at the first hit so a nested project never reports its parent's model
//!
//! Runs the CI gate: exit 0 clean, 1 on findings, 2 when no model exists or the repo is unusable
//!
//! Registers scryer-mcp's statusline command in Claude Code's project settings without ever clobbering a foreign statusline entry
//!
//! Renders the human-facing status line from loop-state counts: changes in flight, pending work, drift, and test verdicts
//!
//! Phrases outstanding plan work as "N pending across M nodes", suppressing the breakdown when the plan is empty
//!
//! Computes gate failures and notes across validator warnings, broken/missing anchors, drift, and test verdicts, naming any dimension it cannot verify

use scryer_core::derived_tree::{DerivedCounts, DerivedStatus, DerivedTree};
use scryer_core::ModelRef;
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
    let status = find_project(&start).and_then(|p| derived_status(&p).ok());
    match status {
        Some((_, s)) if json => println!("{}", status_json(&s.totals)),
        Some((_, s)) => println!("{}", status_line(&s.totals)),
        None if json => println!("{}", serde_json::json!({ "project": false })),
        None => println!("scryer: no project (searched up from {})", start.display()),
    }
    Ok(())
}

/// `scryer-mcp statusline` — the Claude Code statusline command. Claude Code
/// invokes it on conversation updates with a session JSON blob on stdin and
/// shows the first stdout line under the prompt. Prints NOTHING when no
/// project is found (a blank segment, not an error), and always exits 0. Also
/// callable from a user's own statusline script — stdin is only read when it
/// is piped, and an empty/foreign payload falls back to the process cwd.
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
    if let Some((_, s)) = find_project(&start).and_then(|p| derived_status(&p).ok()) {
        println!("{}", status_line(&s.totals));
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
/// first hit ends the walk — an empty `.scryer` is not a reason to keep
/// climbing into a parent project (same rule as the hook client's discovery).
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

fn derived_status(project: &Path) -> Result<(DerivedTree, DerivedStatus), String> {
    scryer_extract::test_status::derived_status(&ModelRef::ProjectLocal(project.to_path_buf()))
}

/// The project a gate runs on — the nearest `.scryer/` above the path
/// argument (or the cwd), else that directory itself, since a repo can be
/// checked before scryer ever wrote there — and the flags given.
fn gate_project(args: &[String], usage: &str) -> Result<(PathBuf, Vec<String>), Box<dyn std::error::Error>> {
    let mut flags = Vec::new();
    let mut start: Option<PathBuf> = None;
    for a in args {
        if a.starts_with('-') {
            flags.push(a.clone());
        } else if start.is_none() {
            start = Some(PathBuf::from(a));
        } else {
            eprintln!("unexpected argument '{a}'\nusage: {usage}");
            std::process::exit(2);
        }
    }
    let start = match start {
        Some(p) => p,
        None => std::env::current_dir()?,
    };
    Ok((find_project(&start).unwrap_or(start), flags))
}

/// `scryer-mcp migrate [path]`: move an authored model into the code once —
/// claims into test descriptions, statements into module docs, untested
/// claims into todo stubs, container names and styles into structure.toml.
/// Prints each write; a second run prints nothing to do.
pub(crate) fn run_migrate(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let (project, _) = gate_project(args, "scryer-mcp migrate [path]")?;
    let report = match scryer_extract::claim_files::migrate(&project) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("migrate: {e}");
            std::process::exit(2);
        }
    };
    let section = |title: &str, rows: &[String]| {
        if !rows.is_empty() {
            println!("{title} ({}):", rows.len());
            for r in rows {
                println!("  {r}");
            }
        }
    };
    section("test descriptions", &report.described);
    section("module docs", &report.documented);
    section("todo stubs", &report.stubbed);
    section("structure.toml", &report.structure);
    section("left as they are", &report.skipped);
    if !report.wrote_anything() {
        println!("nothing to migrate: the code already says it all");
    }
    Ok(())
}

/// One test description that breaks the claim grammar.
pub(crate) struct LintFinding {
    pub at: String,
    pub code: &'static str,
    pub message: &'static str,
    pub description: String,
}

/// Every claim in the tree whose description breaks the EARS grammar.
pub(crate) fn lint_tree(tree: &DerivedTree) -> Vec<LintFinding> {
    let mut out = Vec::new();
    for c in &tree.claims {
        for issue in scryer_core::ears::lint(&c.description, &c.concerns) {
            out.push(LintFinding {
                at: format!("{}:{}", c.file, c.line),
                code: issue.code,
                message: issue.message,
                description: c.description.clone(),
            });
        }
    }
    out
}

fn lint_line(f: &LintFinding) -> String {
    format!("{}: {}: {} — \"{}\"", f.at, f.code, f.message, f.description)
}

/// `scryer-mcp lint [path]` — lint every behaviour test's description as an
/// EARS claim. Exit 0 clean, 1 findings, 2 when the tree cannot be derived.
pub(crate) fn run_lint(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    const USAGE: &str = "scryer-mcp lint [path]";
    let (project, flags) = gate_project(args, USAGE)?;
    if let Some(f) = flags.first() {
        eprintln!("unknown argument '{f}'\nusage: {USAGE}");
        std::process::exit(2);
    }
    let tree = match scryer_extract::derived_tree::derived_tree(&project) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("scryer lint: {e}");
            std::process::exit(2);
        }
    };
    let findings = lint_tree(&tree);
    if findings.is_empty() {
        println!("scryer lint: {} claim(s), clean", tree.claims.len());
        return Ok(());
    }
    for f in &findings {
        println!("{}", lint_line(f));
    }
    println!("scryer lint: {} finding(s) in {} claim(s)", findings.len(), tree.claims.len());
    std::process::exit(1);
}

/// What `check` gates on beyond what always fails it.
#[derive(Default)]
pub(crate) struct Gates {
    pub stale: bool,
    pub mutants: bool,
}

/// `scryer-mcp check [--fail-on-stale] [--fail-on-mutants] [path]` — the CI
/// gate. Exit 0 clean, 1 findings, 2 when the tree cannot be derived. Failing
/// claims, style violations and lint errors always fail it; stale claims and
/// surviving mutants fail it when asked, and are reported as notes otherwise.
pub(crate) fn run_check(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    const USAGE: &str = "scryer-mcp check [--fail-on-stale] [--fail-on-mutants] [path]";
    let (project, flags) = gate_project(args, USAGE)?;
    let mut gates = Gates::default();
    for f in &flags {
        match f.as_str() {
            "--fail-on-stale" => gates.stale = true,
            "--fail-on-mutants" => gates.mutants = true,
            other => {
                eprintln!("unknown argument '{other}'\nusage: {USAGE}");
                std::process::exit(2);
            }
        }
    }
    let (tree, status) = match derived_status(&project) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("scryer check: {e}");
            std::process::exit(2);
        }
    };
    let report = check_report(&tree, &status, &gates);
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
    /// Non-gating observations, named so a pass never hides what it skipped.
    pub notes: Vec<String>,
}

pub(crate) fn check_report(tree: &DerivedTree, status: &DerivedStatus, gates: &Gates) -> CheckReport {
    use scryer_core::derived_tree::{ClaimState, ProbeMark};
    let mut failures = Vec::new();
    let mut notes = Vec::new();
    let at = |id: &str| {
        tree.claims
            .iter()
            .find(|c| c.id == id)
            .map(|c| format!("{}:{} \"{}\"", c.file, c.line, c.description))
            .unwrap_or_else(|| id.to_string())
    };
    let in_state = |state: ClaimState| status.claims.iter().filter(move |c| c.state == state);

    for c in in_state(ClaimState::Failing) {
        failures.push(format!("failing: {}", at(&c.resp_id)));
    }
    let stale: Vec<String> = in_state(ClaimState::Stale).map(|c| at(&c.resp_id)).collect();
    if gates.stale {
        failures.extend(stale.iter().map(|s| format!("stale: {s} — re-run it and ingest the report")));
    } else if !stale.is_empty() {
        notes.push(format!("{} stale claim(s) (not gating — pass --fail-on-stale)", stale.len()));
    }
    // A surviving mutant marks the code a mutation run broke (per node), or
    // the test a probe or Stryker named (per claim).
    let survivors: Vec<String> = status
        .nodes
        .iter()
        .filter(|(_, n)| n.probe == Some(ProbeMark::Crosshair))
        .map(|(id, _)| crate::helpers::breadcrumb(&tree.planned, id))
        .chain(status.claims.iter().filter(|c| c.probe == Some(ProbeMark::Crosshair)).map(|c| at(&c.resp_id)))
        .collect();
    if gates.mutants {
        failures.extend(survivors.iter().map(|s| format!("surviving mutant: {s}")));
    } else if !survivors.is_empty() {
        notes.push(format!(
            "{} place(s) with a surviving mutant or break (not gating — pass --fail-on-mutants)",
            survivors.len()
        ));
    }
    for v in &tree.style_violations {
        failures.push(format!("structural: {} ({})", v.detail, v.file));
    }
    failures.extend(lint_tree(tree).iter().map(|f| format!("lint: {}", lint_line(f))));
    let planned = in_state(ClaimState::Planned).count();
    if planned > 0 {
        notes.push(format!("{planned} todo test(s) — planned work, not a failure"));
    }
    if status.claims.is_empty() {
        notes.push("no claims: no behaviour test carries a description yet".into());
    }
    CheckReport { failures, notes }
}

/// `scryer-mcp setup [on|off]`: switch agent integration for the project on
/// or off, or with no argument print what is installed. The project is the
/// nearest directory holding `.scryer/` above the cwd, else the cwd itself.
/// `on` registers this very binary.
pub(crate) fn run_setup(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    use scryer_core::agent_setup;
    let cwd = std::env::current_dir()?;
    let project = find_project(&cwd).unwrap_or(cwd);
    let detected = agent_setup::detect_tools();
    let status = match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        [] => agent_setup::setup_status(&project, &detected),
        ["on"] => {
            let binary = std::env::current_exe()?.canonicalize()?.to_string_lossy().to_string();
            agent_setup::setup_on(&project, &detected, &binary)?
        }
        ["off"] => agent_setup::setup_off(&project, &detected)?,
        _ => {
            eprintln!("usage: scryer-mcp setup [on|off]");
            std::process::exit(2);
        }
    };
    print!("{}", agent_setup::render_setup_status(&status));
    Ok(())
}

/// The ambient one-liner: claims by status, then surviving mutants when there
/// are any. "scryer: 120 proven · 3 stale · 0 failing · 2 planned".
pub(crate) fn status_line(t: &DerivedCounts) -> String {
    let mut line = format!(
        "scryer: {} proven · {} stale · {} failing · {} planned",
        t.proven, t.stale, t.failing, t.planned
    );
    if t.surviving_mutants > 0 {
        line.push_str(&format!(
            " · {} surviving mutant{}",
            t.surviving_mutants,
            if t.surviving_mutants == 1 { "" } else { "s" }
        ));
    }
    line
}

fn status_json(t: &DerivedCounts) -> String {
    let mut v = serde_json::to_value(t).unwrap_or_default();
    v["statusLine"] = serde_json::json!(status_line(t));
    serde_json::to_string_pretty(&v).unwrap_or_else(|_| "{}".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::fixture::{project, write};

    /// The walk stops at the FIRST `.scryer` — a nested project must never
    /// report its parent's — and finds it from a deep subdirectory.
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

    /// The line counts claims by status, and names surviving mutants only
    /// when there are some.
    #[test]
    fn the_status_line_counts_claims_by_status() {
        let mut t = DerivedCounts { proven: 3, stale: 2, failing: 1, planned: 4, ..Default::default() };
        assert_eq!(status_line(&t), "scryer: 3 proven · 2 stale · 1 failing · 4 planned");
        t.surviving_mutants = 2;
        assert!(status_line(&t).ends_with(" · 2 surviving mutants"));
        let dir = project();
        let (_, s) = derived_status(dir.path()).unwrap();
        assert_eq!(status_line(&s.totals), "scryer: 0 proven · 3 stale · 0 failing · 1 planned");
    }

    /// With no report ingested, claims are stale: a note by default, a
    /// failure under --fail-on-stale. A todo test is never a failure; a
    /// container with no declared style always is.
    #[test]
    fn check_gates_stale_claims_only_when_asked() {
        let dir = project();
        let (tree, status) = derived_status(dir.path()).unwrap();
        let lenient = check_report(&tree, &status, &Gates::default());
        let structural = |r: &CheckReport| r.failures.iter().filter(|f| f.starts_with("structural: ")).count();
        assert_eq!(structural(&lenient), 1, "the unstyled crate: {:?}", lenient.failures);
        assert_eq!(lenient.failures.len(), 1, "{:?}", lenient.failures);
        assert!(lenient.notes.iter().any(|n| n.contains("3 stale")), "{:?}", lenient.notes);
        assert!(lenient.notes.iter().any(|n| n.contains("1 todo test")), "{:?}", lenient.notes);
        let strict = check_report(&tree, &status, &Gates { stale: true, mutants: false });
        assert_eq!(strict.failures.iter().filter(|f| f.starts_with("stale: ")).count(), 3, "{:?}", strict.failures);
    }

    /// A description that breaks the claim grammar fails check and lint.
    #[test]
    fn check_and_lint_fail_on_a_malformed_claim() {
        let dir = project();
        write(
            dir.path(),
            "crates/notes/src/sync.rs",
            "pub fn sync() {}\n#[cfg(test)]\nmod tests {\n    /// Retries the sync when the network drops\n    #[test]\n    fn retries() {}\n}\n",
        );
        let (tree, status) = derived_status(dir.path()).unwrap();
        let lint = lint_tree(&tree);
        assert_eq!(lint.len(), 1);
        assert_eq!(lint[0].code, "trailing-condition");
        assert!(lint[0].at.starts_with("crates/notes/src/sync.rs:"));
        let report = check_report(&tree, &status, &Gates::default());
        assert!(
            report.failures.iter().any(|f| f.starts_with("lint: ") && f.contains("trailing-condition")),
            "{:?}",
            report.failures
        );
    }
}
