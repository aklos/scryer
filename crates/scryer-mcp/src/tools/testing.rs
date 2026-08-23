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
use scryer_extract::test_status::{ingest_report, test_blast_radius, test_statuses, RadiusFile};

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
}
