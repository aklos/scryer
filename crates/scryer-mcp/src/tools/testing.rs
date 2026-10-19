//! The test loop over MCP: which tests a change reaches, the verdicts a run
//! leaves behind, and probes of a test's teeth. Scryer never executes tests —
//! it names exactly what to run and reads the receipts.
//!
//! Verdicts and probe results are keyed by the derived claim id (the test's
//! node in the derived tree); the agent names a test by its JUnit name and the
//! tree maps it.

use crate::derived;
use crate::helpers::*;
use crate::server::ScryerServer;
use crate::types::*;
use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, Content},
    tool, tool_router, ErrorData as McpError,
};
use scryer_core::derived_tree::{DerivedClaim, DerivedTree};
use scryer_core::test_results::TestOutcome;
use scryer_core::worktree;
use scryer_core::ModelRef;
use scryer_extract::test_status::{
    ingest_report, record_probe_result, render_test_command, reports_dir, set_test_command, test_command,
    test_statuses, RadiusTest,
};

/// How many radius tests a response lists by name; the command carries all.
const TESTS_SHOWN: usize = 12;

fn error(text: impl Into<String>) -> Result<CallToolResult, McpError> {
    Ok(CallToolResult::error(vec![Content::text(text.into())]))
}

fn load(model_ref: &ModelRef) -> Result<DerivedTree, String> {
    derived::load(model_ref).map_err(|e| format!("Could not derive the tree for {model_ref}: {e}"))
}

/// The project's file-level import graph, `(importer, imported)`, without
/// guessed edges.
fn file_edges(model_ref: &ModelRef) -> Result<Vec<(String, String)>, String> {
    let ctx = scryer_extract::extract_context(model_ref.project_path())?;
    Ok(ctx.file_edges.into_iter().filter(|e| !e.guessed).map(|e| (e.src, e.dst)).collect())
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

/// The claim's fresh verdict, when one is recorded. SEAM: verdicts are keyed
/// by derived claim id once status derivation lands (step 3); until then a
/// missing verdict is not a refusal.
fn verdict(model_ref: &ModelRef, claim: &DerivedClaim) -> Option<(TestOutcome, bool)> {
    test_statuses(model_ref)
        .ok()?
        .into_iter()
        .find(|s| s.resp_id == claim.id)
        .map(|s| (s.outcome, s.stale))
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
        let path = std::path::Path::new(&req.path);
        let abs = if path.is_absolute() { path.to_path_buf() } else { model_ref.project_path().join(path) };
        let xml = match std::fs::read_to_string(&abs) {
            Ok(x) => x,
            Err(e) => return error(format!("Failed to read report '{}': {e}", abs.display())),
        };
        // Only to name the claims in the answer by their tests.
        let tree = load(&model_ref).ok();
        // Two agents ingesting at once must not lose one report's verdicts
        // to a read-modify-write race.
        let _lock = match lock_or_err(&model_ref) {
            Ok(l) => l,
            Err(e) => return Ok(e),
        };
        let summary = match ingest_report(&model_ref, &xml) {
            Ok(s) => s,
            Err(e) => return error(format!("Failed to ingest '{}': {e}", abs.display())),
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
        let _ = resolve_model_ref(req.project.as_deref())?;
        error("ingest_mutation_report is not yet available: mutation reports land with status derivation.")
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
            Ok(t) => t,
            Err(e) => return error(e),
        };
        let edges = match file_edges(&model_ref) {
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
        let tree = match load(&model_ref) {
            Ok(t) => t,
            Err(e) => return error(e),
        };
        let claim = match derived::claim_for_test(&tree, &req.test) {
            Ok(c) => c.clone(),
            Err(e) => return error(e),
        };
        if claim.planned {
            return error(format!("{} is a todo test — implement it before probing it.", claim.junit_name));
        }
        let verdict = verdict(&model_ref, &claim);
        match verdict {
            Some((TestOutcome::Passed, false)) | None => {}
            Some((_, true)) => return error(format!("{}'s verdict is stale — re-run it and ingest_test_report first.", claim.junit_name)),
            Some((o, _)) => return error(format!("{} is {o:?} — a probe needs a passing test.", claim.junit_name)),
        }
        let edges = file_edges(&model_ref).unwrap_or_default();
        let targets = probe_targets(&tree, &edges, &claim);
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
        if verdict.is_none() {
            msg.push_str("\nNo verdict is recorded for this test: run it green in the worktree before the first break.");
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
        let tree = load(&model_ref);
        let claim = tree.as_ref().ok().and_then(|t| derived::claim_for_test(t, &req.test).ok().cloned());
        let targets = match (&tree, &claim) {
            (Ok(t), Some(c)) => probe_targets(t, &file_edges(&model_ref).unwrap_or_default(), c),
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

    /// A change reaches the unit tests beside the code that imports it and
    /// the integration tests that import the crate; e2e only when
    /// structure.toml says `radius = "all"`; a todo test never.
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
        assert!(sync.contains("[integration] roundtrip") && !sync.contains("keeps_draft"), "{sync}");
        assert!(radius(&server, &dir, Some(&["README.md"])).contains("Radius clear"));
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
        assert!(text.contains("cargo test keeps_draft roundtrip -- --format junit > .scryer/reports/radius.xml"), "{text}");
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

    /// A probe opens on a test by its JUnit name: it names the worktree, the
    /// claim and the product files the test imports, and the worktree already
    /// holds the code to break. Closing resets the worktree; the developer's
    /// own file is never touched.
    #[test]
    fn a_probe_opens_on_a_test_and_closing_resets_the_worktree() {
        isolate_probes_root();
        let dir = project();
        git_init(dir.path());
        let server = ScryerServer::new();
        let text = text_of(&open(&server, &dir, "keeps_draft"));
        let wt = scryer_core::worktree::worktree_path(dir.path());
        assert!(text.contains(&wt.display().to_string()), "{text}");
        assert!(text.contains("Claim: When a save fails, keeps the draft"), "{text}");
        assert!(text.contains("Break in: crates/notes/src/draft.rs, crates/notes/src/store/mod.rs"), "{text}");

        let store = "crates/notes/src/store/mod.rs";
        let original = std::fs::read_to_string(dir.path().join(store)).unwrap();
        std::fs::write(wt.join(store), original.replace("true", "false")).unwrap();
        let closed = close(&server, &dir, "keeps_draft", 2, vec!["put returning false went unnoticed".into()]);
        assert!(closed.contains("2 break(s) tried, 1 survived") && closed.contains("finding"), "{closed}");
        assert_eq!(std::fs::read_to_string(wt.join(store)).unwrap(), original, "the worktree is reset");
        assert_eq!(std::fs::read_to_string(dir.path().join(store)).unwrap(), original);
        std::fs::remove_dir_all(&wt).ok();
    }

    /// A round with no break left in the worktree learned nothing and says so.
    #[test]
    fn a_round_with_no_break_in_the_worktree_records_nothing() {
        isolate_probes_root();
        let dir = project();
        git_init(dir.path());
        let server = ScryerServer::new();
        open(&server, &dir, "keeps_draft");
        assert!(close(&server, &dir, "keeps_draft", 3, Vec::new()).contains("UNPROBED"));
        std::fs::remove_dir_all(scryer_core::worktree::worktree_path(dir.path())).ok();
    }

    /// A todo test has nothing to probe, an unknown name is refused, and
    /// without git there is nowhere safe to break code.
    #[test]
    fn a_probe_is_refused_for_a_todo_an_unknown_test_or_no_git() {
        isolate_probes_root();
        let dir = project();
        let server = ScryerServer::new();
        let refused = |test: &str| {
            let r = open(&server, &dir, test);
            assert_eq!(r.is_error, Some(true), "{test}");
            text_of(&r)
        };
        assert!(refused("merges").contains("todo test"));
        assert!(refused("nope").contains("No test named"));
        assert!(refused("keeps_draft").contains("not a git repository"));
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

    /// Mutation ingest waits on status derivation, and says so.
    #[test]
    fn mutation_ingest_says_it_is_not_yet_available() {
        let dir = project();
        let r = ScryerServer::new()
            .ingest_mutation_report(Parameters(IngestReportRequest {
                project: project_arg(&dir),
                path: "mutants.out/outcomes.json".into(),
            }))
            .unwrap();
        assert_eq!(r.is_error, Some(true));
        assert!(text_of(&r).contains("not yet available"));
    }
}
