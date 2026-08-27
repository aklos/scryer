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
use scryer_core::probe::{close_probe, open_probe, open_probes, ProbeEntry};
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
        let run = if f.commands.is_empty() {
            String::new()
        } else {
            format!(" · run: {}", f.commands.join(" / "))
        };
        out.push_str(&format!(
            "\n  {} — {} claim(s){stale}{run}",
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
        description = "Report a finished test run: point this at the JUnit XML file the runner just wrote and every attached test's result is recorded against its claim — ONE call per report file, never per test. Verdicts are cached keyed by content fingerprints of the claim's implementation and attached tests, so a later edit to either automatically flips the verdict to stale (no watcher, nothing re-runs). The response says what the report settled (recorded, failing) and what it did not — unmatched cases (normal: attachment is curated, the suite is not), ambiguous names, attachments the report never mentioned (normal for a partial or single-runner run) — plus the remaining blast radius. Works with any runner that can emit JUnit XML: vitest/playwright `--reporter=junit`, pytest `--junitxml=`, jest-junit, cargo-nextest, gotestsum, surefire… Call it after every run, full suite or targeted."
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
        description = "Which tests actually NEED running, computed from the model — never the whole suite. Every test-attached claim whose verdict is missing or stale (its implementation or attached test changed since the last recorded run) contributes its test files; claims with current verdicts contribute nothing. Run exactly the listed files with the runner's JUnit reporter on, then report each result file with `ingest_test_report`. An empty radius means every test-attached claim holds a current verdict. Claims with NO attached test never appear here — that gap is health's `untested`. Also summarizes current verdicts (passing / failing / stale) so you see the claim-level test state without running anything."
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
        description = "Open a falsification probe on one claim: ask whether its attached test would actually FAIL if the code stopped honouring the claim. A green verdict says the test passes; it does not say the test would notice a defect, and a test that asserts nothing passes forever. This answers that. Returns the claim's statement, the exact file and line span to break, and the test command to re-run — then YOU make one deliberate breaking edit inside that span, run the test, and expect RED. Green means the break survived: the test does not hold the claim, and that is the finding. Repeat for up to three distinct breaks aimed at what the claim actually says (a When/If claim names the trigger and response — attack those), stopping early on the first survivor, then call `end_probe`. The probe is a recorded transaction: the file's content is captured to disk BEFORE you edit, drift is suppressed on it while open, and `end_probe` restores it whatever the outcome — so never hand-revert, and never leave a probe open. Refused when the claim has no attached test, or its verdict is missing, stale, or not passing: breaking code behind a test you have not seen pass settles nothing."
    )]
    fn probe_claim(
        &self,
        Parameters(req): Parameters<ProbeClaimRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = resolve_model_ref(req.project.as_deref())?;
        let target = match probe_target(&model_ref, &req.resp_id) {
            Ok(t) => t,
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(e)])),
        };
        let abs = model_ref.project_path().join(&target.file);
        let original = match std::fs::read_to_string(&abs) {
            Ok(c) => c,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Cannot read {} to capture it: {e}",
                    target.file
                ))]))
            }
        };
        let _lock = match lock_or_err(&model_ref) {
            Ok(l) => l,
            Err(e) => return Ok(e),
        };
        // Capture lands on disk before the caller is told it may edit: a way
        // back that has not been written down is not a way back.
        if let Err(e) = open_probe(
            &model_ref,
            ProbeEntry {
                resp_id: target.resp_id.clone(),
                file: target.file.clone(),
                start_line: target.start_line,
                end_line: target.end_line,
                original,
                opened_at: scryer_core::drift::now_secs(),
            },
        ) {
            return Ok(CallToolResult::error(vec![Content::text(e)]));
        }
        drop(_lock);

        let mut msg = format!(
            "Probe OPEN on {} — {} is captured and will be restored by end_probe.\n\
             Claim: {}\n\
             Break inside: {}:{}-{}{}",
            target.resp_id,
            target.file,
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
        if !target.commands.is_empty() {
            msg.push_str(&format!("\nRun: {}", target.commands.join(" / ")));
        }
        if !target.tests.is_empty() {
            msg.push_str(&format!("\nAttached test(s): {}", target.tests.join(", ")));
        }
        msg.push_str(
            "\nMake ONE breaking edit inside the span, run the test, expect it to FAIL. \
             A test that still passes is a survivor — record it. Up to 3 breaks, stop at the \
             first survivor, then end_probe. Do not revert by hand.",
        );
        Ok(CallToolResult::success(vec![Content::text(msg)]))
    }

    #[tool(
        description = "Close an open probe: restores the captured file whatever happened, and records what the round found against the claim. Pass `probes` (how many deliberate breaks you tried) and `survivors` (one line per break the test did NOT catch, describing what you changed). No survivors means the test caught every break you tried — the claim reads as probed, NOT as proven: you sampled, you did not exhaust. Survivors are the real finding: the test does not hold the claim there, so strengthen it, re-run for a fresh verdict, and probe again. The result is fingerprint-keyed like a verdict, so editing the implementation or the test ages it to stale. ALWAYS call this after `probe_claim`, including when a probe went wrong — leaving one open leaves mutated code in the tree."
    )]
    fn end_probe(
        &self,
        Parameters(req): Parameters<EndProbeRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = resolve_model_ref(req.project.as_deref())?;
        let _lock = match lock_or_err(&model_ref) {
            Ok(l) => l,
            Err(e) => return Ok(e),
        };
        let closed = match close_probe(&model_ref, &req.resp_id) {
            Ok(c) => c,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Probe on {} could NOT be restored: {e}. The tree still holds the mutation — \
                     restore it before doing anything else.",
                    req.resp_id
                ))]))
            }
        };
        let Some(entry) = closed else {
            let open = open_probes(&model_ref);
            let hint = if open.is_empty() {
                "No probes are open.".to_string()
            } else {
                format!(
                    "Open probes: {}.",
                    open.iter()
                        .map(|e| format!("{} ({})", e.resp_id, e.file))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            };
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "No probe was open on {}. {hint}",
                req.resp_id
            ))]));
        };
        let survivors = req.survivors.clone();
        let survived = survivors.len();
        let result = record_probe_result(&model_ref, &req.resp_id, req.probes, survivors);
        drop(_lock);
        if let Err(e) = result {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "{} restored, but the probe result could not be recorded: {e}",
                entry.file
            ))]));
        }

        let mut msg = format!(
            "Probe CLOSED on {} — {} restored. {} break(s) tried, {} survived.",
            req.resp_id, entry.file, req.probes, survived
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
    fn tested_project() -> (ScryerServer, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let r = ModelRef::ProjectLocal(dir.path().to_path_buf());
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/m.ts"), IMPL_TS).unwrap();
        std::fs::write(dir.path().join("src/m.spec.ts"), SPEC_TS).unwrap();
        let mut m = ScryModel::new();
        m.nodes.push(Node {
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
            }],
            properties: Vec::new(),
            icon: None,
            visual: None,
            appearance: None,
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
                command: None,
            }],
        );
        m.test_map.insert(
            "r1".into(),
            vec![SourceLocation {
                pattern: "src/m.spec.ts".into(),
                symbol: Some("answers one".into()),
                line: None,
                end_line: None,
                command: Some("pnpm test".into()),
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
        assert!(text.contains("run: pnpm test"), "{text}");

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

    /// resp-747: the probe answers with the span to break and the command to
    /// re-run, and the capture is open before the agent is told to edit.
    #[test]
    fn probe_claim_answers_with_the_span_and_opens_the_capture() {
        let (server, dir) = tested_project();
        ingest(&server, &dir);

        let result = server
            .probe_claim(Parameters(ProbeClaimRequest {
                project: project_arg(&dir),
                resp_id: "r1".into(),
            }))
            .unwrap();

        let text = text_of(&result);
        assert!(text.contains("src/m.ts:1-3"), "{text}");
        assert!(text.contains("Run: pnpm test"), "{text}");
        let r = ModelRef::ProjectLocal(dir.path().to_path_buf());
        assert_eq!(
            scryer_core::probe::open_probes(&r).len(),
            1,
            "the capture is open before the caller edits anything"
        );
    }

    /// resp-748: without a verdict there is nothing for a red test to mean.
    #[test]
    fn probe_claim_refuses_a_claim_with_no_verdict() {
        let (server, dir) = tested_project();

        let result = server
            .probe_claim(Parameters(ProbeClaimRequest {
                project: project_arg(&dir),
                resp_id: "r1".into(),
            }))
            .unwrap();

        assert_eq!(result.is_error, Some(true));
        assert!(text_of(&result).contains("no recorded verdict"), "{}", text_of(&result));
        let r = ModelRef::ProjectLocal(dir.path().to_path_buf());
        assert!(
            scryer_core::probe::open_probes(&r).is_empty(),
            "a refused probe opens nothing"
        );
    }

    /// resp-749: closing restores the mutated file and reports the survivors.
    #[test]
    fn end_probe_restores_the_file_and_names_the_survivors() {
        let (server, dir) = tested_project();
        ingest(&server, &dir);
        server
            .probe_claim(Parameters(ProbeClaimRequest {
                project: project_arg(&dir),
                resp_id: "r1".into(),
            }))
            .unwrap();
        // The agent breaks the code, as the probe instructed.
        std::fs::write(dir.path().join("src/m.ts"), "export function alpha() {\n    return 2;\n}\n")
            .unwrap();

        let result = server
            .end_probe(Parameters(EndProbeRequest {
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
            std::fs::read_to_string(dir.path().join("src/m.ts")).unwrap(),
            IMPL_TS,
            "the mutation is reverted by scryer, never by hand"
        );
        let r = ModelRef::ProjectLocal(dir.path().to_path_buf());
        assert!(scryer_core::probe::open_probes(&r).is_empty());
        let probes = scryer_extract::test_status::probe_statuses(&r).unwrap();
        assert_eq!(probes[0].survived, 1, "and the finding is recorded");
    }

    /// A clean round says PROBED and explicitly refuses to say proven.
    #[test]
    fn a_clean_round_reads_as_probed_not_proven() {
        let (server, dir) = tested_project();
        ingest(&server, &dir);
        server
            .probe_claim(Parameters(ProbeClaimRequest {
                project: project_arg(&dir),
                resp_id: "r1".into(),
            }))
            .unwrap();

        let result = server
            .end_probe(Parameters(EndProbeRequest {
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

    /// Closing a probe nobody opened says so, and names what IS open.
    #[test]
    fn end_probe_without_an_open_probe_says_so() {
        let (server, dir) = tested_project();

        let result = server
            .end_probe(Parameters(EndProbeRequest {
                project: project_arg(&dir),
                resp_id: "r1".into(),
                probes: 1,
                survivors: Vec::new(),
            }))
            .unwrap();

        assert_eq!(result.is_error, Some(true));
        assert!(text_of(&result).contains("No probe was open"), "{}", text_of(&result));
    }


    /// resp-753: while a probe is open the tree is deliberately broken, and
    /// the ambient header says so first — every other number is about a tree
    /// that is not the one on disk.
    #[test]
    fn the_status_header_leads_with_an_open_probe() {
        let (server, dir) = tested_project();
        ingest(&server, &dir);
        let r = ModelRef::ProjectLocal(dir.path().to_path_buf());
        assert!(
            !status_header(&r).unwrap().starts_with("PROBE OPEN"),
            "precondition: a clean tree leads with the plan"
        );

        server
            .probe_claim(Parameters(ProbeClaimRequest {
                project: project_arg(&dir),
                resp_id: "r1".into(),
            }))
            .unwrap();

        let header = status_header(&r).unwrap();
        assert!(header.starts_with("PROBE OPEN"), "{header}");
        assert!(header.contains("src/m.ts mutated for r1"), "{header}");

        server
            .end_probe(Parameters(EndProbeRequest {
                project: project_arg(&dir),
                resp_id: "r1".into(),
                probes: 1,
                survivors: Vec::new(),
            }))
            .unwrap();

        assert!(
            !status_header(&r).unwrap().starts_with("PROBE OPEN"),
            "and the warning goes when the tree is restored"
        );
    }

}
