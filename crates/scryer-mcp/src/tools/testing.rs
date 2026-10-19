//! The test loop over MCP: which tests a change reaches, the verdicts a run
//! leaves behind, and probes of a test's teeth. Scryer never executes tests —
//! it names exactly what to run and reads the receipts.
//!
//! Verdicts and probe results are keyed by the derived claim id (the test's
//! node in the derived tree); the agent names a test by its JUnit name and the
//! tree maps it.
//!
//! Resets the probe worktree and records which deliberate breaks the claim's test failed to catch
//!
//! Ingests JUnit test reports and links pass/fail verdicts to the claims their tests cover, flagging verdicts the code has since moved past as stale
//!
//! Reports the blast radius of claims whose attached tests hold a missing or stale verdict
//!
//! Opens a mutation probe against a verified claim: syncs a git worktree to current code and hands back the span to break
//!
//! Ingests a JUnit report, recording each test's verdict against the claims it's attached to
//!
//! Runs mutation probes in an isolated git worktree to check whether a claim's test would actually catch a deliberate break
//!
//! Names the test files whose claims hold a missing or stale verdict

use crate::derived;
use crate::helpers::*;
use crate::server::ScryerServer;
use crate::types::*;
use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, Content},
    tool, tool_router, ErrorData as McpError,
};
use scryer_core::derived_tree::{ClaimState, DerivedClaim, DerivedTree};
use scryer_core::test_results::TestOutcome;
use scryer_core::worktree;
use scryer_core::ModelRef;
use scryer_extract::test_status::{
    ingest_mutation_report, ingest_report, record_probe_result, render_test_command, reports_dir, set_test_command,
    test_command, RadiusTest,
};

/// How many radius tests a response lists by name; the command carries all.
const TESTS_SHOWN: usize = 12;

fn error(text: impl Into<String>) -> Result<CallToolResult, McpError> {
    Ok(CallToolResult::error(vec![Content::text(text.into())]))
}

fn load(model_ref: &ModelRef) -> Result<derived::Loaded, String> {
    derived::load(model_ref).map_err(|e| format!("Could not derive the tree for {model_ref}: {e}"))
}

/// The product files a probe of `claim`'s test breaks: what the test file
/// imports, test files left out — or the test file itself when its tests sit
/// beside the code (Rust's `#[cfg(test)]` modules).
fn probe_targets(tree: &DerivedTree, edges: &[(String, String)], claim: &DerivedClaim) -> Vec<String> {
    let is_test = |f: &str| tree.claims.iter().any(|c| c.file == f) && !has_product_code(tree, f);
    let mut files: Vec<String> = edges
        .iter()
        .filter(|(src, dst)| *src == claim.file && !is_test(dst))
        .map(|(_, dst)| dst.clone())
        .collect();
    if has_product_code(tree, &claim.file) {
        files.push(claim.file.clone());
    }
    files.sort();
    files.dedup();
    files
}

/// Whether `file` defines product symbols the tree knows (not only tests).
fn has_product_code(tree: &DerivedTree, file: &str) -> bool {
    tree.planned.source_map.values().flatten().any(|l| l.pattern == file)
}

/// Read a report file named absolute or project-relative.
fn read_report(model_ref: &ModelRef, path: &str) -> Result<String, String> {
    let path = std::path::Path::new(path);
    let abs = if path.is_absolute() { path.to_path_buf() } else { model_ref.project_path().join(path) };
    std::fs::read_to_string(&abs).map_err(|e| format!("Failed to read report '{}': {e}", abs.display()))
}

#[tool_router(router = tool_router_testing, vis = "pub(crate)")]
impl ScryerServer {
    #[tool(
        description = "Report a finished test run: point at the JUnit XML file the runner wrote and each \
         test's result is recorded against its claim — ONE call per report file. Verdicts are \
         fingerprint-keyed, so a later edit to the code or the test makes them stale. Call after \
         every run."
    )]
    fn ingest_test_report(
        &self,
        Parameters(req): Parameters<IngestReportRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = resolve_model_ref(req.project.as_deref())?;
        let xml = match read_report(&model_ref, &req.path) {
            Ok(x) => x,
            Err(e) => return error(e),
        };
        // Only to name the claims in the answer by their tests.
        let tree = load(&model_ref).ok().map(|l| l.tree);
        // Two agents ingesting at once must not lose one report's verdicts
        // to a read-modify-write race.
        let _lock = match lock_or_err(&model_ref) {
            Ok(l) => l,
            Err(e) => return Ok(e),
        };
        let summary = match ingest_report(&model_ref, &xml) {
            Ok(s) => s,
            Err(e) => return error(format!("Failed to ingest '{}': {e}", req.path)),
        };
        drop(_lock);

        let name_of = |id: &str| {
            tree.as_ref()
                .and_then(|t| t.claims.iter().find(|c| c.id == id))
                .map(|c| c.junit_name.clone())
                .unwrap_or_else(|| id.to_string())
        };
        let mut msg = format!(
            "Ingested {} case(s) from {} — verdicts recorded for {} claim(s).",
            summary.cases, req.path, summary.recorded
        );
        let mut red: Vec<(String, TestOutcome)> = summary
            .report
            .claims
            .iter()
            .filter(|(_, c)| matches!(c.outcome, TestOutcome::Failed | TestOutcome::Errored))
            .map(|(id, c)| (name_of(id), c.outcome))
            .collect();
        red.sort_by(|a, b| a.0.cmp(&b.0));
        if !red.is_empty() {
            msg.push_str(&format!("\n{} RED:", red.len()));
            for (name, outcome) in &red {
                msg.push_str(&format!("\n  {name}: {outcome:?}"));
            }
        }
        if summary.report.unmatched_cases > 0 {
            msg.push_str(&format!(
                "\nunmatched: {} case(s) named no test in the tree.",
                summary.report.unmatched_cases
            ));
        }
        if !summary.report.ambiguous.is_empty() {
            msg.push_str(&format!(
                "\nambiguous: {} case(s) matched tests in several files and were NOT recorded.",
                summary.report.ambiguous.len()
            ));
        }
        Ok(CallToolResult::success(vec![Content::text(msg)]))
    }

    #[tool(
        description = "Report a mutation run (cargo-mutants outcomes.json, Stryker mutation.json): each \
         surviving mutant is recorded against the claims whose tests should have caught it. A \
         survivor is a finding — strengthen the test."
    )]
    fn ingest_mutation_report(
        &self,
        Parameters(req): Parameters<IngestReportRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = resolve_model_ref(req.project.as_deref())?;
        let report = match read_report(&model_ref, &req.path) {
            Ok(x) => x,
            Err(e) => return error(e),
        };
        let _lock = match lock_or_err(&model_ref) {
            Ok(l) => l,
            Err(e) => return Ok(e),
        };
        let s = match ingest_mutation_report(&model_ref, &report) {
            Ok(s) => s,
            Err(e) => return error(format!("Failed to ingest '{}': {e}", req.path)),
        };
        drop(_lock);
        let mut msg = format!(
            "Ingested {} mutant(s) from {} ({}) across {} file(s): {} caught, {} survived.",
            s.mutants, req.path, s.tool, s.files, s.caught, s.survived
        );
        if s.survived > 0 {
            msg.push_str("
A survivor is a finding: locate the files it names and strengthen their tests.");
        }
        Ok(CallToolResult::success(vec![Content::text(msg)]))
    }

    #[tool(
        description = "What to run: the tests that reach `files` by import (the files' own tests included), at \
         unit and integration level — e2e too when .scryer/structure.toml says `radius = \"all\"`. \
         Omit `files` for this session's edits. Answers with the exact command once it is set \
         via `command`. Never run the full suite. Then ingest_test_report."
    )]
    fn get_test_radius(
        &self,
        Parameters(req): Parameters<GetTestRadiusRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = resolve_model_ref(req.project.as_deref())?;
        if let Some(template) = req.command.as_deref().filter(|t| !t.trim().is_empty()) {
            if let Err(e) = set_test_command(&model_ref, template) {
                return error(format!("The test command could not be stored: {e}"));
            }
        }
        let files: Vec<String> = match req.files.filter(|f| !f.is_empty()) {
            Some(f) => f,
            None => self
                .session_id(&model_ref)
                .map(|s| scryer_core::session::session_log(&model_ref, &s).touched)
                .unwrap_or_default(),
        };
        if files.is_empty() {
            return error("Name the `files` you changed — this session has recorded no edits yet.");
        }
        let tree = match load(&model_ref) {
            Ok(l) => l.tree,
            Err(e) => return error(e),
        };
        let edges = match derived::import_edges(&model_ref, &tree) {
            Ok(e) => e,
            Err(e) => return error(format!("Could not read the import graph: {e}")),
        };
        let with_e2e = scryer_extract::derived_tree::read_structure(model_ref.project_path())
            .map(|s| s.radius_includes_e2e())
            .unwrap_or(false);
        let hits = derived::radius(&tree, &edges, &files, with_e2e);

        let scope = if with_e2e { "unit, integration and e2e" } else { "unit and integration" };
        if hits.is_empty() {
            return Ok(CallToolResult::success(vec![Content::text(format!(
                "Radius clear — no {scope} test reaches {} by import.",
                files.join(", ")
            ))]));
        }
        let run: Vec<RadiusTest> = hits
            .iter()
            .map(|h| RadiusTest {
                pattern: h.claim.file.clone(),
                name: Some(h.claim.identifier.clone()),
                claims: vec![h.claim.id.clone()],
                millis: None,
            })
            .collect();
        let nfiles = run.iter().map(|t| t.pattern.as_str()).collect::<std::collections::BTreeSet<_>>().len();
        let mut msg = format!("Radius — {} {scope} test(s) in {nfiles} file(s) reach the change.", run.len());
        match test_command(&model_ref) {
            Some(template) => {
                let dir = reports_dir(&model_ref).unwrap_or_else(|_| ".scryer/reports".into());
                let report = format!("{dir}/radius.xml");
                msg.push_str(&format!(
                    "\nRun exactly:\n  {}\nthen ingest_test_report each report it writes ({report}).",
                    render_test_command(&template, &run, &report, &dir)
                ));
            }
            None => msg.push_str(
                "\nNo test command set — set it once: get_test_radius {command}, e.g. \
                 \"npx vitest run {files} -t '{names:|}' --reporter=junit --outputFile={report}\". \
                 Until then run these with a JUnit reporter and ingest_test_report:",
            ),
        }
        for h in hits.iter().take(TESTS_SHOWN) {
            msg.push_str(&format!(
                "\n  [{}] {} — {}",
                derived::test_level(h.claim.level),
                h.claim.junit_name,
                h.claim.description
            ));
        }
        if hits.len() > TESTS_SHOWN {
            msg.push_str(&format!("\n  … {} more", hits.len() - TESTS_SHOWN));
        }
        Ok(CallToolResult::success(vec![Content::text(msg)]))
    }

    #[tool(
        description = "Open a falsification probe on one `test` (its JUnit name): would it FAIL if the code \
         stopped doing what its description claims? Syncs an isolated git worktree and returns \
         its path, the claim and the files to break. Run it yourself, never through a subagent; \
         close with close_probe. Refused for a todo test, a failing or stale one, or outside a \
         git repo."
    )]
    fn open_probe(
        &self,
        Parameters(req): Parameters<ProbeRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = resolve_model_ref(req.project.as_deref())?;
        let loaded = match load(&model_ref) {
            Ok(l) => l,
            Err(e) => return error(e),
        };
        let tree = &loaded.tree;
        let claim = match derived::claim_for_test(tree, &req.test) {
            Ok(c) => c.clone(),
            Err(e) => return error(e),
        };
        // A probe means something only against a test that passes on the
        // code as it stands.
        match loaded.status_of(&claim).map(|s| s.state) {
            Some(ClaimState::Proven) => {}
            Some(ClaimState::Planned) => {
                return error(format!("{} is a todo test — implement it before probing it.", claim.junit_name))
            }
            Some(ClaimState::Failing) => {
                return error(format!("{} is failing — a probe needs a passing test.", claim.junit_name))
            }
            Some(ClaimState::Stale) | None => {
                return error(format!(
                    "{} has no current verdict — run it, ingest_test_report, then probe.",
                    claim.junit_name
                ))
            }
        }
        let edges = derived::import_edges(&model_ref, tree).unwrap_or_default();
        let targets = probe_targets(tree, &edges, &claim);
        // Sync BEFORE answering: the path handed back has to already hold the
        // developer's current code, or a break would land on a stale copy.
        let wt = match worktree::ensure_synced(model_ref.project_path()) {
            Ok(w) => w,
            Err(e) => return error(e),
        };
        if let Some(sid) = self.session_id(&model_ref) {
            let _ = scryer_core::session::record_probe(&model_ref, &sid, true);
        }

        let mut msg = format!(
            "Probe OPEN on {} — work ONLY in the probe worktree:\n  {}\n\
             It holds your current code, uncommitted work included. The developer's own tree is \
             untouched and must stay that way.\n\
             Claim: {}\n\
             Test: {}:{}",
            claim.junit_name,
            wt.display(),
            claim.description,
            claim.file,
            claim.line,
        );
        if targets.is_empty() {
            msg.push_str("\nBreak in: the code this test exercises (the tree found no import to name).");
        } else {
            msg.push_str(&format!("\nBreak in: {}", targets.join(", ")));
        }
        msg.push_str(
            "\nAim each break at what the claim says — its trigger, its response — then run only \
             this test in the worktree and expect it to FAIL. A test that still passes is a \
             survivor: record what you changed. Up to 3 breaks, stop at the first survivor, then \
             close_probe. Never revert a break or run git checkout/restore/stash/reset: leave the \
             last break in place, close_probe resets the worktree and refuses a round with no \
             break in it.",
        );
        Ok(CallToolResult::success(vec![Content::text(msg)]))
    }

    #[tool(
        description = "Close a probe: resets the probe worktree whatever happened and records the round \
         against the `test`. Pass `probes` (breaks tried) and `survivors` (one line per break the \
         test did NOT catch). Call after every open_probe, including when a probe went wrong. \
         No survivors reads as probed, not proven."
    )]
    fn close_probe(
        &self,
        Parameters(req): Parameters<CloseProbeRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = resolve_model_ref(req.project.as_deref())?;
        let tree = load(&model_ref).map(|l| l.tree);
        let claim = tree.as_ref().ok().and_then(|t| derived::claim_for_test(t, &req.test).ok().cloned());
        let targets = match (&tree, &claim) {
            (Ok(t), Some(c)) => probe_targets(t, &derived::import_edges(&model_ref, t).unwrap_or_default(), c),
            _ => Vec::new(),
        };
        // Read before the reset wipes it: the last break must still be in the
        // worktree, or the breaks reported were made somewhere else.
        let broke_here = worktree::holds_change(model_ref.project_path(), &targets);
        // Reset first and unconditionally. Recording can fail; a worktree left
        // holding a mutation would silently poison the next probe's baseline.
        let reset = worktree::reset(model_ref.project_path());
        if let Some(sid) = self.session_id(&model_ref) {
            let _ = scryer_core::session::record_probe(&model_ref, &sid, false);
        }
        let reset_note = match &reset {
            Ok(()) => "worktree reset.".to_string(),
            Err(e) => format!("the probe worktree could not be reset: {e}. Clear it before probing again."),
        };
        let Some(claim) = claim else {
            return error(format!("Probe CLOSED, {reset_note} No test named '{}', so nothing was recorded.", req.test));
        };
        let survived = req.survivors.len();
        // A survivor is a break tried, whatever the count says.
        let tried = req.probes.max(survived as u32);
        if tried == 0 || !broke_here {
            // Nothing was broken where it counts, so nothing was learned:
            // recording it would read as probed and overwrite the last real
            // round's finding.
            let why = if tried == 0 {
                "No break was tried, so nothing was recorded: the test stays UNPROBED."
            } else {
                "The code in the probe worktree matched the developer's at close, so no break was in \
                 place there: nothing was recorded and the test stays UNPROBED. Breaks go in the \
                 probe worktree only; if any touched the developer's own tree, tell the user which files."
            };
            return Ok(CallToolResult::success(vec![Content::text(format!(
                "Probe CLOSED on {} — {reset_note}\n{why}",
                claim.junit_name
            ))]));
        }
        let _lock = match lock_or_err(&model_ref) {
            Ok(l) => l,
            Err(e) => return Ok(e),
        };
        let recorded = record_probe_result(&model_ref, &claim.id, tried, req.survivors.clone());
        drop(_lock);

        let mut msg = format!(
            "Probe CLOSED on {} — {reset_note} {tried} break(s) tried, {survived} survived.",
            claim.junit_name
        );
        if survived == 0 {
            msg.push_str("\nEvery break was caught — the test reads as PROBED. That is a sample, not a proof.");
        } else {
            msg.push_str("\nSURVIVED — the test does not catch:");
            for s in &req.survivors {
                msg.push_str(&format!("\n  {s}"));
            }
            msg.push_str("\nThat is a finding: strengthen the test so each survivor fails it, re-run, probe again.");
        }
        if let Err(e) = recorded {
            msg.push_str(&format!("\nThe round could not be recorded: {e}"));
        }
        Ok(CallToolResult::success(vec![Content::text(msg)]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::fixture::*;

    fn radius(server: &ScryerServer, dir: &tempfile::TempDir, files: Option<&[&str]>) -> String {
        text_of(
            &server
                .get_test_radius(Parameters(GetTestRadiusRequest {
                    project: project_arg(dir),
                    files: files.map(|f| f.iter().map(|s| s.to_string()).collect()),
                    command: None,
                }))
                .unwrap(),
        )
    }

    /// When code changes, reaches the unit tests beside it and the integration tests that import it through the crate's name, e2e only under `radius = "all"`, and never a todo test.
    #[test]
    fn the_radius_holds_tests_by_import_and_level() {
        let dir = project();
        let server = ScryerServer::new();
        let text = radius(&server, &dir, Some(&["crates/notes/src/store/mod.rs"]));
        assert!(text.contains("[unit] keeps_draft"), "{text}");
        assert!(text.contains("[integration] roundtrip"), "{text}");
        assert!(!text.contains("flow"), "e2e left out by default: {text}");
        assert!(!text.contains("merges"), "a todo test has nothing to run: {text}");

        write(dir.path(), ".scryer/structure.toml", "radius = \"all\"\n\n[system]\nname = \"notes\"\n\n[levels]\ne2e = [\"e2e/**\"]\n");
        let all = radius(&server, &dir, Some(&["crates/notes/src/store/mod.rs"]));
        assert!(all.contains("[e2e] flow"), "{all}");

        let sync = radius(&server, &dir, Some(&["crates/notes/src/sync.rs"]));
        assert!(sync.contains("Radius clear"), "nothing imports sync.rs: {sync}");
    }

    /// With no files named, the radius covers this session's edits; with
    /// none recorded it asks for files.
    #[test]
    fn the_radius_defaults_to_the_sessions_edits() {
        let dir = project();
        let server = ScryerServer::for_session("s");
        assert!(radius(&server, &dir, None).contains("Name the `files`"));
        let r = ModelRef::ProjectLocal(dir.path().to_path_buf());
        scryer_core::session::record_touch(&r, "s", "crates/notes/src/draft.rs").unwrap();
        assert!(radius(&server, &dir, None).contains("keeps_draft"));
    }

    /// Once the command is set, the radius answers with it filled in.
    #[test]
    fn the_radius_answers_with_the_configured_command() {
        let dir = project();
        let text = text_of(
            &ScryerServer::new()
                .get_test_radius(Parameters(GetTestRadiusRequest {
                    project: project_arg(&dir),
                    files: Some(vec!["crates/notes/src/draft.rs".into()]),
                    command: Some("cargo test {names} -- --format junit > {report}".into()),
                }))
                .unwrap(),
        );
        assert!(text.contains("cargo test keeps_draft -- --format junit > .scryer/reports/radius.xml"), "{text}");
    }

    fn open(server: &ScryerServer, dir: &tempfile::TempDir, test: &str) -> CallToolResult {
        server.open_probe(Parameters(ProbeRequest { project: project_arg(dir), test: test.into() })).unwrap()
    }

    fn close(server: &ScryerServer, dir: &tempfile::TempDir, test: &str, probes: u32, survivors: Vec<String>) -> String {
        text_of(
            &server
                .close_probe(Parameters(CloseProbeRequest {
                    project: project_arg(dir),
                    test: test.into(),
                    probes,
                    survivors,
                }))
                .unwrap(),
        )
    }

    /// A probe needs a test that passes on the code as it stands: a todo
    /// test, an unknown name and a test with no current verdict are refused.
    /// (The opened probe is exercised end to end in tests/probe_flow.rs.)
    #[test]
    fn a_probe_is_refused_without_a_passing_test() {
        let dir = project();
        let server = ScryerServer::new();
        let refused = |test: &str| {
            let r = open(&server, &dir, test);
            assert_eq!(r.is_error, Some(true), "{test}");
            text_of(&r)
        };
        assert!(refused("merges").contains("todo test"));
        assert!(refused("nope").contains("No test named"));
        assert!(refused("keeps_draft").contains("no current verdict"));
    }

    /// Closing a probe with no break left in the worktree records nothing
    /// and says the test stays unprobed.
    #[test]
    fn closing_with_no_break_in_the_worktree_records_nothing() {
        isolate_probes_root();
        let dir = project();
        git_init(dir.path());
        let text = close(&ScryerServer::new(), &dir, "keeps_draft", 3, Vec::new());
        assert!(text.contains("UNPROBED"), "{text}");
    }

    /// The tool names it, so the agent never delegates the probe loop.
    #[test]
    fn open_probe_tells_the_caller_not_to_delegate() {
        let desc = ScryerServer::tool_router_testing()
            .list_all()
            .into_iter()
            .find(|t| t.name == "open_probe")
            .unwrap()
            .description
            .unwrap_or_default()
            .to_string();
        assert!(desc.contains("never through a subagent"), "{desc}");
    }

    /// An unreadable report answers with what went wrong.
    #[test]
    fn an_unreadable_report_answers_with_the_diagnostic() {
        let dir = project();
        let r = ScryerServer::new()
            .ingest_test_report(Parameters(IngestReportRequest { project: project_arg(&dir), path: "nope.xml".into() }))
            .unwrap();
        assert_eq!(r.is_error, Some(true));
        assert!(text_of(&r).contains("Failed to read report"));
    }

    /// A cargo-mutants report is read per file, and a survivor shows on the
    /// claims of the component it landed in.
    #[test]
    fn a_mutation_report_marks_the_claims_it_reaches() {
        let dir = project();
        write(
            dir.path(),
            "mutants.out/outcomes.json",
            r#"{"outcomes":[
              {"scenario":"Baseline","summary":"Success"},
              {"scenario":{"Mutant":{"package":"notes","file":"crates/notes/src/draft.rs","function":{"function_name":"save"},"span":{"start":{"line":4,"column":1}},"replacement":"true","genre":"FnValue"}},"summary":"MissedMutant"}
            ],"total_mutants":1}"#,
        );
        let server = ScryerServer::new();
        let text = text_of(
            &server
                .ingest_mutation_report(Parameters(IngestReportRequest {
                    project: project_arg(&dir),
                    path: "mutants.out/outcomes.json".into(),
                }))
                .unwrap(),
        );
        assert!(text.contains("1 mutant(s)") && text.contains("1 survived"), "{text}");
        let loaded = derived::load(&ModelRef::ProjectLocal(dir.path().to_path_buf())).unwrap();
        let keeps = loaded.tree.claims.iter().find(|c| c.identifier == "keeps_draft").unwrap();
        assert_eq!(loaded.node_probe(&keeps.node_id), Some("a break survived in this code"));

        let bad = server
            .ingest_mutation_report(Parameters(IngestReportRequest { project: project_arg(&dir), path: "Cargo.toml".into() }))
            .unwrap();
        assert_eq!(bad.is_error, Some(true));
    }
}
