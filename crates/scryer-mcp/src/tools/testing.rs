//! The claim-level test-status loop over MCP: report a finished run's JUnit
//! file, get back what it settled and what still needs running. Scryer never
//! executes tests — these tools read the receipts a run leaves behind and
//! keep the blast radius (missing/stale verdicts → attached test files)
//! current, so the agent runs exactly what a change invalidated instead of
//! the whole suite.

use crate::helpers::*;
use crate::server::ScryerServer;
use crate::types::*;
use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, Content},
    tool, tool_router, ErrorData as McpError,
};
use scryer_core::test_results::TestOutcome;
use scryer_core::worktree;
use scryer_extract::test_status::{
    ingest_report, probe_target, record_probe_result, test_blast_radius, test_statuses,
    RadiusFile,
};

/// Render the blast radius as response lines. Shared so the ingest response
/// answers "what still needs running" the same way `get_test_radius` does.
fn radius_lines(radius: &[RadiusFile]) -> String {
    if radius.is_empty() {
        return "Radius clear — every test-attached claim holds a current verdict.".into();
    }
    let mut out = format!(
        "Blast radius — {} test file(s) whose claims hold missing or stale verdicts:",
        radius.len()
    );
    for f in radius {
        let stale = if f.stale > 0 {
            format!(", {} stale", f.stale)
        } else {
            String::new()
        };
        out.push_str(&format!(
            "\n  {} — {} claim(s){stale}",
            f.pattern,
            f.claims.len()
        ));
    }
    out.push_str(
        "\nRun exactly these files with the runner's JUnit reporter on, then ingest_test_report each report file.",
    );
    out
}

#[tool_router(router = tool_router_testing, vis = "pub(crate)")]
impl ScryerServer {
    #[tool(
        description = "Report a finished test run: point at the JUnit XML file the runner wrote and every \
         attached test's result is recorded against its claim — ONE call per report file. \
         Verdicts are fingerprint-keyed, so a later edit to implementation or test flips them to \
         stale. Reports what settled, what did not (unmatched, ambiguous, unmentioned), and the \
         remaining radius. Call after every run.\n\
         Rules: test-verdicts, test-attachment"
    )]
    fn ingest_test_report(
        &self,
        Parameters(req): Parameters<IngestTestReportRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = resolve_model_ref(req.project.as_deref())?;
        let path = std::path::Path::new(&req.path);
        let abs = if path.is_absolute() {
            path.to_path_buf()
        } else {
            model_ref.project_path().join(path)
        };
        let xml = match std::fs::read_to_string(&abs) {
            Ok(x) => x,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to read report '{}': {e}",
                    abs.display()
                ))]));
            }
        };
        // The cache write serializes behind the model lock like every other
        // state write — two agents ingesting concurrently must not lose one
        // report's verdicts to a read-modify-write race.
        let _lock = match lock_or_err(&model_ref) {
            Ok(l) => l,
            Err(e) => return Ok(e),
        };
        let summary = match ingest_report(&model_ref, &xml) {
            Ok(s) => s,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to ingest '{}': {e}",
                    abs.display()
                ))]));
            }
        };
        drop(_lock);

        let mut msg = format!(
            "Ingested {} case(s) from {} — verdicts recorded for {} claim(s).",
            summary.cases, req.path, summary.recorded
        );
        let mut red: Vec<(&String, &scryer_core::test_results::ClaimOutcome)> = summary
            .report
            .claims
            .iter()
            .filter(|(_, c)| matches!(c.outcome, TestOutcome::Failed | TestOutcome::Errored))
            .collect();
        red.sort_by_key(|(id, _)| id.as_str());
        if !red.is_empty() {
            msg.push_str(&format!("\n{} claim(s) RED:", red.len()));
            for (id, c) in &red {
                msg.push_str(&format!("\n  {id}: {:?} ({} case(s))", c.outcome, c.cases));
            }
        }
        if summary.report.unmatched_cases > 0 {
            msg.push_str(&format!(
                "\nunmatched: {} case(s) named no attached test (normal — attachment is curated).",
                summary.report.unmatched_cases
            ));
        }
        if !summary.report.ambiguous.is_empty() {
            msg.push_str(&format!(
                "\nambiguous: {} case(s) matched attachments in several files and were NOT recorded:",
                summary.report.ambiguous.len()
            ));
            for a in summary.report.ambiguous.iter().take(5) {
                msg.push_str(&format!(
                    "\n  \"{}\" claimed by {}",
                    a.case.name,
                    a.candidates.join(", ")
                ));
            }
        }
        if !summary.report.unseen.is_empty() {
            msg.push_str(&format!(
                "\nunseen: {} attachment(s) never appeared in this report — expected for a partial or single-runner run; a name that no runner ever reports is a rotted attachment.",
                summary.report.unseen.len()
            ));
        }
        match test_blast_radius(&model_ref) {
            Ok(radius) => msg.push_str(&format!("\n{}", radius_lines(&radius))),
            Err(e) => msg.push_str(&format!("\n(blast radius unavailable: {e})")),
        }
        if let Some(h) = status_header(&model_ref) {
            msg.push_str(&format!("\n{h}"));
        }
        Ok(CallToolResult::success(vec![Content::text(msg)]))
    }

    #[tool(
        description = "Which test files actually NEED running: those attached to claims whose verdict is \
         missing or stale. Run exactly those with the JUnit reporter on, then ingest_test_report \
         each result file. Empty means every attached claim holds a current verdict; claims with \
         no test never appear (that is health's `untested`). Also summarizes current verdicts.\n\
         Rules: test-verdicts"
    )]
    fn get_test_radius(
        &self,
        Parameters(req): Parameters<GetTestRadiusRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = resolve_model_ref(req.project.as_deref())?;
        let radius = match test_blast_radius(&model_ref) {
            Ok(r) => r,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(read_fail(
                    "model", &model_ref, &e,
                ))]));
            }
        };
        let verdicts = test_statuses(&model_ref).unwrap_or_default();
        let stale = verdicts.iter().filter(|s| s.stale).count();
        let count_fresh = |o: TestOutcome| {
            verdicts.iter().filter(|s| !s.stale && s.outcome == o).count()
        };
        let mut msg = radius_lines(&radius);
        msg.push_str(&format!(
            "\nVerdicts: {} passing · {} failing · {} errored · {} stale · {} claim(s) recorded in all.",
            count_fresh(TestOutcome::Passed),
            count_fresh(TestOutcome::Failed),
            count_fresh(TestOutcome::Errored),
            stale,
            verdicts.len()
        ));
        Ok(CallToolResult::success(vec![Content::text(msg)]))
    }

    #[tool(
        description = "Open a falsification probe on one claim: would its attached test FAIL if the code \
         stopped honouring it? Syncs an isolated git worktree and returns its path, the claim, \
         the exact span to break, and the test files. DELEGATE THIS TO A SUBAGENT on a cheap \
         model; close with close_probe. Refused without an attached test and a current passing \
         verdict, or outside a git repo.\n\
         Rules: probe-loop"
    )]
    fn open_probe(
        &self,
        Parameters(req): Parameters<ProbeClaimRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = resolve_model_ref(req.project.as_deref())?;
        let target = match probe_target(&model_ref, &req.resp_id) {
            Ok(t) => t,
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(e)])),
        };
        // Sync BEFORE answering: the path handed back has to already hold the
        // developer's current code, or the subagent would break a stale copy
        // and report a survivor that says nothing about what they are writing.
        let wt = match worktree::ensure_synced(model_ref.project_path()) {
            Ok(w) => w,
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(e)])),
        };

        let mut msg = format!(
            "Probe OPEN on {} — work ONLY in the probe worktree:\n  {}\n\
             It holds your current code, uncommitted work included. The developer's \
             own tree is untouched and must stay that way.\n\
             Claim: {}\n\
             Break inside: {}:{}-{}{}",
            target.resp_id,
            wt.display(),
            target.statement,
            target.file,
            target.start_line,
            target.end_line,
            target
                .symbol
                .as_deref()
                .map(|s| format!(" ({s})"))
                .unwrap_or_default(),
        );
        if !target.tests.is_empty() {
            msg.push_str(&format!("\nRun only these test(s): {}", target.tests.join(", ")));
        }
        msg.push_str(
            "\nMake ONE breaking edit inside the span, run those tests in the worktree, expect \
             them to FAIL. A test that still passes is a survivor — record what you changed. \
             Up to 3 breaks, stop at the first survivor, then close_probe. No need to revert: \
             close_probe resets the worktree.",
        );
        Ok(CallToolResult::success(vec![Content::text(msg)]))
    }

    #[tool(
        description = "Close a probe: resets the probe worktree whatever happened and records the round against \
         the claim. Pass `probes` (breaks tried) and `survivors` (one line per break the test did \
         NOT catch). Call after every open_probe, including when a probe went wrong.\n\
         Rules: probe-loop"
    )]
    fn close_probe(
        &self,
        Parameters(req): Parameters<EndProbeRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = resolve_model_ref(req.project.as_deref())?;
        // Reset first and unconditionally. Recording can fail; a worktree left
        // holding a mutation would silently poison the next probe's baseline.
        let reset = worktree::reset(model_ref.project_path());
        let _lock = match lock_or_err(&model_ref) {
            Ok(l) => l,
            Err(e) => return Ok(e),
        };
        let survivors = req.survivors.clone();
        let survived = survivors.len();
        let result = record_probe_result(&model_ref, &req.resp_id, req.probes, survivors);
        drop(_lock);
        if let Err(e) = result {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "The probe result for {} could not be recorded: {e}",
                req.resp_id
            ))]));
        }
        if let Err(e) = reset {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "Probe result recorded, but the probe worktree could not be reset: {e}. \
                 The next probe would start from mutated code — clear it before probing again."
            ))]));
        }

        let mut msg = format!(
            "Probe CLOSED on {} — worktree reset. {} break(s) tried, {} survived.",
            req.resp_id, req.probes, survived
        );
        if survived == 0 {
            msg.push_str(
                "\nEvery break was caught — the claim reads as PROBED. That is a sample, not a \
                 proof: an exhaustive run could still find one.",
            );
        } else {
            msg.push_str("\nSURVIVED — the attached test does not catch:");
            for s in &req.survivors {
                msg.push_str(&format!("\n  {s}"));
            }
            msg.push_str(
                "\nStrengthen the test so each survivor fails it, re-run and ingest_test_report \
                 for a fresh verdict, then probe again.",
            );
        }
        if let Some(h) = status_header(&model_ref) {
            msg.push_str(&format!("\n{h}"));
        }
        Ok(CallToolResult::success(vec![Content::text(msg)]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scryer_core::{Kind, ModelRef, Node, Responsibility, ScryModel, SourceLocation};

    const IMPL_TS: &str = "export function alpha() {\n    return 1;\n}\n";
    const SPEC_TS: &str = "describe(\"alpha\", () => {\n  it(\"answers one\", () => {\n    expect(alpha()).toBe(1);\n  });\n});\n";
    const REPORT: &str = r#"<testsuites><testsuite name="s">
        <testcase classname="src/m.spec.ts" name="alpha &gt; answers one"/>
    </testsuite></testsuites>"#;

    /// A project whose one claim is implemented in src/m.ts and attached to a
    /// vitest-style test in src/m.spec.ts.
    /// Run a git command in the fixture, failing loudly — the probe worktree
    /// is real git behaviour, so these tests use a real repo.
    fn git(dir: &std::path::Path, args: &[&str]) {
        let out = std::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    }

    /// Keep probe worktrees out of the home directory of whoever runs the
    /// suite. Set once, before any test reads it — `set_var` is
    /// process-global and these run in parallel.
    fn isolate_probes_root() {
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| {
            // Never clear the root here: nextest gives each test its own
            // process, so a wipe would race sibling tests already using it.
            // Slugs are unique per fixture, and /tmp is the OS's to reap.
            std::env::set_var("SCRYER_PROBES_DIR", std::env::temp_dir().join("scryer-mcp-probe-tests"));
        });
    }

    fn tested_project() -> (ScryerServer, tempfile::TempDir) {
        isolate_probes_root();
        let dir = tempfile::tempdir().unwrap();
        let r = ModelRef::ProjectLocal(dir.path().to_path_buf());
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/m.ts"), IMPL_TS).unwrap();
        std::fs::write(dir.path().join("src/m.spec.ts"), SPEC_TS).unwrap();
        git(dir.path(), &["init", "-q"]);
        git(dir.path(), &["config", "user.email", "t@t.t"]);
        git(dir.path(), &["config", "user.name", "t"]);
        git(dir.path(), &["add", "-A"]);
        git(dir.path(), &["commit", "-qm", "init"]);
        let mut m = ScryModel::new();
        m.nodes.push(Node {
            style: None,
            layer: None,
            id: "sym".into(),
            kind: Kind::Symbol,
            name: "alpha".into(),
            vagrant: None,
            stale: None,
            parent_id: None,
            external: None,
            technology: None,
            description: None,
            responsibilities: vec![Responsibility {
                concern: None,
                id: "r1".into(),
                statement: "answers one".into(),
                vagrant: None,
                stale: None,
                stale_proposal: None,
                directives: Vec::new(),
                last_touched_at: None,
                vagrant_origin: None,
                approved_statement: None,
            }],
            properties: Vec::new(),
            icon: None,
            notes: None,
            position: None,
            directives: Vec::new(),
        });
        m.source_map.insert(
            "r1".into(),
            vec![SourceLocation {
                pattern: "src/m.ts".into(),
                symbol: Some("alpha".into()),
                line: None,
                end_line: None,
            }],
        );
        m.test_map.insert(
            "r1".into(),
            vec![SourceLocation {
                pattern: "src/m.spec.ts".into(),
                symbol: Some("answers one".into()),
                line: None,
                end_line: None,
            }],
        );
        scryer_core::write_model_at(&r, &m).unwrap();
        (ScryerServer::new(), dir)
    }

    fn text_of(result: &CallToolResult) -> String {
        result
            .content
            .iter()
            .find_map(|c| c.as_text().map(|t| t.text.clone()))
            .unwrap()
    }

    fn project_arg(dir: &tempfile::TempDir) -> Option<String> {
        Some(dir.path().to_string_lossy().to_string())
    }

    #[test]
    fn ingest_records_verdicts_and_clears_the_radius() {
        let (server, dir) = tested_project();
        // Before any report: the radius names the attached test file.
        let before = server
            .get_test_radius(Parameters(GetTestRadiusRequest { project: project_arg(&dir) }))
            .unwrap();
        let text = text_of(&before);
        assert!(text.contains("src/m.spec.ts"), "{text}");
        assert!(text.contains("1 claim(s)"), "{text}");

        std::fs::write(dir.path().join("report.xml"), REPORT).unwrap();
        let result = server
            .ingest_test_report(Parameters(IngestTestReportRequest {
                project: project_arg(&dir),
                path: "report.xml".into(),
            }))
            .unwrap();
        let text = text_of(&result);
        assert!(text.contains("verdicts recorded for 1 claim(s)"), "{text}");
        assert!(text.contains("Radius clear"), "{text}");

        let after = server
            .get_test_radius(Parameters(GetTestRadiusRequest { project: project_arg(&dir) }))
            .unwrap();
        let text = text_of(&after);
        assert!(text.contains("Radius clear"), "{text}");
        assert!(text.contains("1 passing"), "{text}");
    }

    #[test]
    fn an_edit_after_ingest_re_enters_the_radius_as_stale() {
        let (server, dir) = tested_project();
        std::fs::write(dir.path().join("report.xml"), REPORT).unwrap();
        server
            .ingest_test_report(Parameters(IngestTestReportRequest {
                project: project_arg(&dir),
                path: "report.xml".into(),
            }))
            .unwrap();
        std::fs::write(
            dir.path().join("src/m.ts"),
            IMPL_TS.replace("return 1", "return 2"),
        )
        .unwrap();
        let result = server
            .get_test_radius(Parameters(GetTestRadiusRequest { project: project_arg(&dir) }))
            .unwrap();
        let text = text_of(&result);
        assert!(text.contains("src/m.spec.ts"), "{text}");
        assert!(text.contains("1 stale"), "{text}");
    }

    #[test]
    fn a_failing_report_names_the_red_claims() {
        let (server, dir) = tested_project();
        let failing = REPORT.replace("/>", "><failure message=\"expected 1\"/></testcase>");
        std::fs::write(dir.path().join("report.xml"), failing).unwrap();
        let result = server
            .ingest_test_report(Parameters(IngestTestReportRequest {
                project: project_arg(&dir),
                path: "report.xml".into(),
            }))
            .unwrap();
        let text = text_of(&result);
        assert!(text.contains("1 claim(s) RED"), "{text}");
        assert!(text.contains("r1: Failed"), "{text}");
    }

    /// The ambient header speaks about tests ONLY when a verdict is failing
    /// or stale — verified-green and no-reports-yet are both silence.
    #[test]
    fn the_status_header_mentions_tests_only_when_red_or_stale() {
        let (server, dir) = tested_project();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());
        // No verdicts recorded yet: silence.
        let header = crate::helpers::status_header(&model_ref).unwrap();
        assert!(!header.contains("tests:"), "{header}");

        // All green: still silence.
        std::fs::write(dir.path().join("report.xml"), REPORT).unwrap();
        server
            .ingest_test_report(Parameters(IngestTestReportRequest {
                project: project_arg(&dir),
                path: "report.xml".into(),
            }))
            .unwrap();
        let header = crate::helpers::status_header(&model_ref).unwrap();
        assert!(!header.contains("tests:"), "{header}");

        // The implementation moves past the verdict: the header speaks.
        std::fs::write(
            dir.path().join("src/m.ts"),
            IMPL_TS.replace("return 1", "return 2"),
        )
        .unwrap();
        let header = crate::helpers::status_header(&model_ref).unwrap();
        assert!(header.contains("tests: 1 stale"), "{header}");

        // A red verdict on current code is the alarm case.
        let failing = REPORT.replace("/>", "><failure message=\"expected 1\"/></testcase>");
        std::fs::write(dir.path().join("report.xml"), failing).unwrap();
        server
            .ingest_test_report(Parameters(IngestTestReportRequest {
                project: project_arg(&dir),
                path: "report.xml".into(),
            }))
            .unwrap();
        let header = crate::helpers::status_header(&model_ref).unwrap();
        assert!(header.contains("tests: 1 failing"), "{header}");
    }

    #[test]
    fn unreadable_or_malformed_reports_answer_with_the_diagnostic() {
        let (server, dir) = tested_project();
        let missing = server
            .ingest_test_report(Parameters(IngestTestReportRequest {
                project: project_arg(&dir),
                path: "nope.xml".into(),
            }))
            .unwrap();
        assert_eq!(missing.is_error, Some(true));
        assert!(text_of(&missing).contains("Failed to read report"));

        std::fs::write(dir.path().join("bad.xml"), "<html>hi</html>").unwrap();
        let malformed = server
            .ingest_test_report(Parameters(IngestTestReportRequest {
                project: project_arg(&dir),
                path: "bad.xml".into(),
            }))
            .unwrap();
        assert_eq!(malformed.is_error, Some(true));
        assert!(text_of(&malformed).contains("not a JUnit report"), "{}", text_of(&malformed));
    }

    // --- probes ---

    fn ingest(server: &ScryerServer, dir: &tempfile::TempDir) {
        std::fs::write(dir.path().join("report.xml"), REPORT).unwrap();
        server
            .ingest_test_report(Parameters(IngestTestReportRequest {
                project: project_arg(dir),
                path: "report.xml".into(),
            }))
            .unwrap();
    }

    /// resp-764: the probe answers with the span to break, its attached test
    /// files, and the worktree to do it in — and the developer's own tree is
    /// not part of the transaction at all.
    #[test]
    fn probe_claim_answers_with_the_span_and_the_worktree() {
        let (server, dir) = tested_project();
        ingest(&server, &dir);

        let result = server
            .open_probe(Parameters(ProbeClaimRequest {
                project: project_arg(&dir),
                resp_id: "r1".into(),
            }))
            .unwrap();

        let text = text_of(&result);
        assert!(text.contains("src/m.ts:1-3"), "{text}");
        assert!(text.contains("Run only these test(s): src/m.spec.ts"), "{text}");

        let wt = scryer_core::worktree::worktree_path(dir.path());
        assert!(text.contains(&wt.display().to_string()), "the worktree is named: {text}");
        assert_eq!(
            std::fs::read_to_string(wt.join("src/m.ts")).unwrap(),
            IMPL_TS,
            "and it already holds the code to break"
        );
        std::fs::remove_dir_all(&wt).ok();
    }

    /// resp-763: without git there is nowhere safe to break code, and the
    /// answer is a refusal rather than a fallback onto the developer's tree.
    #[test]
    fn probe_claim_refuses_a_project_that_is_not_a_git_repo() {
        let (server, dir) = tested_project();
        ingest(&server, &dir);
        std::fs::remove_dir_all(dir.path().join(".git")).unwrap();

        let result = server
            .open_probe(Parameters(ProbeClaimRequest {
                project: project_arg(&dir),
                resp_id: "r1".into(),
            }))
            .unwrap();

        assert_eq!(result.is_error, Some(true));
        assert!(text_of(&result).contains("not a git repository"), "{}", text_of(&result));
    }

    /// resp-765: without a verdict there is nothing for a red test to mean.
    #[test]
    fn probe_claim_refuses_a_claim_with_no_verdict() {
        let (server, dir) = tested_project();

        let result = server
            .open_probe(Parameters(ProbeClaimRequest {
                project: project_arg(&dir),
                resp_id: "r1".into(),
            }))
            .unwrap();

        assert_eq!(result.is_error, Some(true));
        assert!(text_of(&result).contains("no recorded verdict"), "{}", text_of(&result));
    }

    /// resp-766 and resp-762: the mutation lands in the worktree, closing
    /// resets it, the finding is recorded, and the developer's own file was
    /// never a participant.
    #[test]
    fn end_probe_resets_the_worktree_and_names_the_survivors() {
        let (server, dir) = tested_project();
        ingest(&server, &dir);
        server
            .open_probe(Parameters(ProbeClaimRequest {
                project: project_arg(&dir),
                resp_id: "r1".into(),
            }))
            .unwrap();
        // The subagent breaks the code, as the probe instructed — in the
        // worktree, which is the only place it was given.
        let wt = scryer_core::worktree::worktree_path(dir.path());
        std::fs::write(wt.join("src/m.ts"), "export function alpha() {\n    return 2;\n}\n")
            .unwrap();

        let result = server
            .close_probe(Parameters(EndProbeRequest {
                project: project_arg(&dir),
                resp_id: "r1".into(),
                probes: 3,
                survivors: vec!["returning 2 instead of 1 went unnoticed".into()],
            }))
            .unwrap();

        let text = text_of(&result);
        assert!(text.contains("3 break(s) tried, 1 survived"), "{text}");
        assert!(text.contains("returning 2 instead of 1"), "{text}");
        assert_eq!(
            std::fs::read_to_string(wt.join("src/m.ts")).unwrap(),
            IMPL_TS,
            "the worktree is reset by scryer, never by hand"
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join("src/m.ts")).unwrap(),
            IMPL_TS,
            "and the developer's own file was never touched"
        );
        let r = ModelRef::ProjectLocal(dir.path().to_path_buf());
        let probes = scryer_extract::test_status::probe_statuses(&r).unwrap();
        assert_eq!(probes[0].survived, 1, "and the finding is recorded");
        std::fs::remove_dir_all(&wt).ok();
    }

    /// A clean round says PROBED and explicitly refuses to say proven.
    #[test]
    fn a_clean_round_reads_as_probed_not_proven() {
        let (server, dir) = tested_project();
        ingest(&server, &dir);
        server
            .open_probe(Parameters(ProbeClaimRequest {
                project: project_arg(&dir),
                resp_id: "r1".into(),
            }))
            .unwrap();

        let result = server
            .close_probe(Parameters(EndProbeRequest {
                project: project_arg(&dir),
                resp_id: "r1".into(),
                probes: 3,
                survivors: Vec::new(),
            }))
            .unwrap();

        let text = text_of(&result);
        assert!(text.contains("PROBED"), "{text}");
        assert!(text.contains("sample, not a"), "{text}");
    }

    /// resp-772: a surviving break says a test the model calls green does not
    /// hold its claim, so it rides the ambient header — while a claim nobody
    /// has probed stays silent, since that is nearly every claim and a
    /// standing count of it would be noise.
    #[test]
    fn the_status_header_speaks_only_for_surviving_breaks() {
        let (server, dir) = tested_project();
        ingest(&server, &dir);
        let r = ModelRef::ProjectLocal(dir.path().to_path_buf());
        assert!(
            !status_header(&r).unwrap().contains("probes:"),
            "an unprobed claim is not a finding"
        );

        let probe = |survivors: Vec<String>| {
            server
                .open_probe(Parameters(ProbeClaimRequest {
                    project: project_arg(&dir),
                    resp_id: "r1".into(),
                }))
                .unwrap();
            server
                .close_probe(Parameters(EndProbeRequest {
                    project: project_arg(&dir),
                    resp_id: "r1".into(),
                    probes: 2,
                    survivors,
                }))
                .unwrap();
        };

        probe(Vec::new());
        assert!(
            !status_header(&r).unwrap().contains("probes:"),
            "a clean round is not a finding either"
        );

        probe(vec!["returning 2 went unnoticed".into()]);
        let header = status_header(&r).unwrap();
        assert!(header.contains("probes: 1 claim with a surviving break"), "{header}");
        std::fs::remove_dir_all(scryer_core::worktree::worktree_path(dir.path())).ok();
    }

    /// resp-771: the churn of a probe — many edits, many test runs — must not
    /// land in the context of the session that asked for it, so the tool tells
    /// the caller to hand the loop off rather than running it inline.
    #[test]
    fn probe_claim_tells_the_caller_to_delegate_the_loop() {
        let desc = ScryerServer::tool_router_testing()
            .list_all()
            .into_iter()
            .find(|t| t.name == "open_probe")
            .expect("open_probe is registered")
            .description
            .clone()
            .unwrap_or_default()
            .to_string();
        assert!(desc.contains("DELEGATE THIS TO A SUBAGENT"), "{desc}");
        assert!(desc.contains("cheap model"), "{desc}");
    }
}
