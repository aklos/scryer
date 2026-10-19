//! The test evidence scryer keeps per claim: the last verdict a report gave
//! it and the anchor content that verdict was true of, probe rounds, and
//! mutation-testing results. Shapes only; the extractor fingerprints and
//! writes them, and any reader can load them.

use crate::domain::test_results::TestOutcome;
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
    /// Every case the ingested reports held, attached or not, latest run per
    /// case — so a test attached after its report was ingested can still take
    /// the outcome that run gave it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cases: Vec<KeptCase>,
}

/// One report case as it last ran, kept whether or not a claim had the test
/// attached at ingest.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct KeptCase {
    pub case: crate::domain::test_results::TestCase,
    /// Unix nanoseconds of the ingest that reported it: code or tests
    /// modified after this make the outcome stale for a later attachment.
    pub ingested_ns: u64,
}

/// A cached verdict as the caller should present it.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// Where the breaks came from when not an agent's probe round:
    /// `"mutation"` for an ingested mutation report, which carries no
    /// fingerprints and so reads as of its ingest time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

/// Breaks a mutation tool tried against some code, and how they fared.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MutantTally {
    pub caught: u32,
    pub survived: u32,
    /// What each survivor was, in the tool's words.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub survivors: Vec<String>,
}

/// The last ingested mutation report per tool, attributed per file and,
/// where the tool says which test killed what, per test.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MutationCache {
    /// Project-relative source file → its mutants.
    #[serde(default)]
    pub files: BTreeMap<String, MutantTally>,
    /// Test name as the tool reports it → mutants it killed (`caught`) and
    /// surviving mutants it covered (`survived`).
    #[serde(default)]
    pub tests: BTreeMap<String, MutantTally>,
    /// Unix seconds of the ingest each file's tally came from.
    #[serde(default)]
    pub ingested_at: u64,
}
