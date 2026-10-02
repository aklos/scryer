//! What a claim's test evidence IS: the cached verdict records, the status a
//! reader sees, and how a claim's evidence is classified. Reading and writing
//! the cache is `infrastructure::test_status`; the entry points other
//! containers call are in `composition::test_status`.

use scryer_core::test_results::{ReportMatch, TestOutcome};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// One claim's cached verdict and the anchor content it was true of.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimRecord {
    pub resp_id: String,
    pub outcome: TestOutcome,
    /// How many report cases fed the verdict.
    pub cases: usize,
    /// Seconds since the epoch at record time — display only, never used for
    /// invalidation.
    pub recorded_at: u64,
    /// Anchor identity (`{key}|{file}|{symbol}`) → span content hash at
    /// record time, across BOTH dimensions: the claim's implementation
    /// anchors under its bare key and its attached tests under `test:{id}`.
    /// Empty means nothing was resolvable when the result landed — such a
    /// record can only ever read as stale.
    #[serde(default)]
    pub fingerprints: BTreeMap<String, String>,
    /// Attached test name → its wall time in millis on the run that recorded
    /// this verdict — what marks a test slow enough to keep out of the radius.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub millis: BTreeMap<String, u64>,
}

/// One claim's probe history: how many deliberate breaks were tried against
/// its span, and how many its attached test failed to catch.
///
/// Kept apart from [`ClaimRecord`] on purpose. A verdict answers "does the
/// test pass"; a probe answers "would it fail if the code were wrong". They
/// go stale together — same anchors, same fingerprints — but they are never
/// the same claim about the code, and collapsing them would let a green
/// verdict read as proof it isn't.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeRecord {
    pub resp_id: String,
    /// Deliberate breaks tried against the claim's span.
    pub probes: u32,
    /// How many of them the attached test did NOT catch.
    pub survived: u32,
    /// What each survivor was, in the prober's words — the audit trail for a
    /// claim that reads as probed, and the to-do list for one that doesn't.
    #[serde(default)]
    pub survivors: Vec<String>,
    pub recorded_at: u64,
    /// Same anchor identity → content hash map a verdict records, taken the
    /// same way, so an edit to the implementation or the test ages a probe
    /// result exactly as it ages a verdict.
    #[serde(default)]
    pub fingerprints: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestStatusCache {
    #[serde(default)]
    pub results: Vec<ClaimRecord>,
    #[serde(default)]
    pub probes: Vec<ProbeRecord>,
}

/// A cached verdict as the caller should present it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimTestStatus {
    pub resp_id: String,
    pub outcome: TestOutcome,
    pub cases: usize,
    /// The code behind the claim (implementation or attached test) no longer
    /// hashes as it did when this outcome was reported.
    pub stale: bool,
    pub recorded_at: u64,
}

/// A cached probe result as the caller should present it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimProbeStatus {
    pub resp_id: String,
    pub probes: u32,
    pub survived: u32,
    pub survivors: Vec<String>,
    /// The code behind the claim no longer hashes as it did when these
    /// probes ran — the result describes code that has since moved on.
    pub stale: bool,
    pub recorded_at: u64,
}

/// What one ingested report amounted to.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IngestSummary {
    /// Cases the report held (all of them, matched or not).
    pub cases: usize,
    /// Claims whose outcome was recorded.
    pub recorded: usize,
    pub report: ReportMatch,
}

pub(crate) fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// What the model can say about one claim's test evidence — the fold's gate.
/// Deterministic: a test is attached or it isn't, and its recorded verdict is
/// current-and-passing or it isn't. `tests` names the attached test files so a
/// refusal can say exactly what to run.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind", rename_all_fields = "camelCase")]
pub enum Evidence {
    /// No test attached at all.
    NoTest,
    /// A test is attached but no report has ever been ingested for it.
    NoVerdict { tests: Vec<String> },
    /// The recorded verdict's fingerprints no longer match the tree.
    Stale { tests: Vec<String> },
    /// The current verdict is not `Passed` (failed, errored, or skipped).
    Failing { outcome: TestOutcome, tests: Vec<String> },
    /// A test is attached and its verdict is current and passing.
    Verified,
}

impl Evidence {
    pub fn verified(&self) -> bool {
        matches!(self, Evidence::Verified)
    }

    /// The missing fact, in the words a refusal uses.
    pub fn reason(&self) -> String {
        match self {
            Evidence::NoTest => "no test attached".to_string(),
            Evidence::NoVerdict { tests } => {
                format!("no verdict recorded: run {} and ingest_test_report", tests.join(", "))
            }
            Evidence::Stale { tests } => {
                format!("verdict stale: run {} and ingest_test_report", tests.join(", "))
            }
            Evidence::Failing { outcome, tests } => {
                format!("verdict {outcome:?}: fix and re-run {}", tests.join(", "))
            }
            Evidence::Verified => "verified".to_string(),
        }
    }

    /// The attached test files, when any.
    pub fn tests(&self) -> &[String] {
        match self {
            Evidence::NoVerdict { tests }
            | Evidence::Stale { tests }
            | Evidence::Failing { tests, .. } => tests,
            _ => &[],
        }
    }
}

/// One test file the blast radius says to run, and why.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RadiusFile {
    /// The attached test file (as the attachment spells it).
    pub pattern: String,
    /// The claims whose verdicts running this file would refresh.
    pub claims: Vec<String>,
    /// How many of those claims have a stale verdict (the rest have none).
    pub stale: usize,
}

/// A test the radius would run past this wall time is slow: it stays out of
/// the radius command unless a claim it backs was touched this session.
pub const SLOW_TEST_MILLIS: u64 = 5_000;

/// One attached test in the radius.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RadiusTest {
    /// The test file, as the attachment spells it.
    pub pattern: String,
    /// The test's name; `None` for an attachment that stores none (only its
    /// file can select it).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Radius claims a run of this test would refresh.
    pub claims: Vec<String>,
    /// Its wall time on the last recorded run, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub millis: Option<u64>,
}

/// What needs running, scoped to one session's work when there is one.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRadius {
    /// Whether this is one session's radius; `false` is the project-wide one
    /// (no session to scope by).
    pub scoped: bool,
    /// Per file: the claims with missing or stale verdicts that are this
    /// session's (all of them when unscoped).
    pub files: Vec<RadiusFile>,
    /// The tests to run for them.
    pub run: Vec<RadiusTest>,
    /// Slow tests left out — none of their claims was touched this session.
    pub slow: Vec<RadiusTest>,
    /// Stale verdicts on claims this session never touched: someone else's.
    pub not_yours: Vec<String>,
}

/// Fill a test-command template for `run`. `{names}` and `{files}` expand to
/// the tests' names and their distinct files, space-joined; `{names:SEP}` /
/// `{files:SEP}` join with SEP instead, so a per-item prefix rides in the
/// separator (`Name~{names:|Name~}` → `Name~a|Name~b`). `{report}` is the
/// JUnit file to write, `{reports}` its directory. Items are inserted raw:
/// the template owns its quoting.
pub fn render_test_command(template: &str, run: &[RadiusTest], report: &str, reports: &str) -> String {
    let mut names: Vec<&str> = Vec::new();
    let mut files: Vec<&str> = Vec::new();
    for t in run {
        if let Some(n) = t.name.as_deref() {
            if !names.contains(&n) {
                names.push(n);
            }
        }
        if !files.contains(&t.pattern.as_str()) {
            files.push(&t.pattern);
        }
    }
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else {
            out.push_str(&rest[open..]);
            return out;
        };
        let inner = &after[..close];
        let (key, sep) = inner.split_once(':').unwrap_or((inner, " "));
        match key {
            "names" => out.push_str(&names.join(sep)),
            "files" => out.push_str(&files.join(sep)),
            "report" if inner == key => out.push_str(report),
            "reports" if inner == key => out.push_str(reports),
            _ => out.push_str(&rest[open..open + close + 2]),
        }
        rest = &after[close + 1..];
    }
    out.push_str(rest);
    out
}

/// What the Stop hook's probe check found: a reason to block (ONCE per
/// session — the caller keeps that count) and a line for the user when a
/// probe this session's tests faced let a break survive.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ProbeCheck {
    pub block: Option<String>,
    pub summary: Option<String>,
}

/// How many of a session's freshly tested claims must be probed before Stop.
pub const PROBES_PER_SESSION: usize = 2;

/// Where a probe should aim, and what to run afterwards.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeTarget {
    pub resp_id: String,
    pub statement: String,
    /// Project-relative file holding the claim's implementation.
    pub file: String,
    /// 1-based inclusive span to break. Resolved the same way a fingerprint
    /// resolves it, so the probe lands inside exactly the region the claim is
    /// anchored to and nowhere else.
    pub start_line: u32,
    pub end_line: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// The attached tests to re-run.
    pub tests: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test(pattern: &str, name: Option<&str>) -> RadiusTest {
        RadiusTest { pattern: pattern.into(), name: name.map(Into::into), claims: Vec::new(), millis: None }
    }

    #[test]
    fn a_command_template_takes_names_files_and_the_report() {
        let run = [
            test("tests/Shared/a.cs", Some("Docks")),
            test("tests/Shared/a.cs", Some("Undocks")),
            test("tests/Server/b.cs", None),
        ];
        let cmd = render_test_command(
            "tools/test.sh \"FullyQualifiedName~{names:|FullyQualifiedName~}\" {report} -- {files}",
            &run,
            ".scryer/reports/radius.xml",
            ".scryer/reports",
        );
        assert_eq!(
            cmd,
            "tools/test.sh \"FullyQualifiedName~Docks|FullyQualifiedName~Undocks\" \
             .scryer/reports/radius.xml -- tests/Shared/a.cs tests/Server/b.cs"
        );
    }

    #[test]
    fn unknown_placeholders_and_stray_braces_pass_through() {
        let run = [test("t.rs", Some("x"))];
        assert_eq!(
            render_test_command("run {other} {names} {reports} {", &run, "r.xml", "d"),
            "run {other} x d {"
        );
    }
}
