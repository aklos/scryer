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
