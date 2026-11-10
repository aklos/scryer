//! The test-evidence entry points other containers call. Each wires the
//! on-disk verdict cache to the derived tree; the cache itself stays private.

use crate::verdict_cache as store;

// Declared in the domain, named here so the crate has one public path per area.
pub use crate::test_evidence::{CoverageSummary, MutationSummary};
pub use crate::test_evidence::{
    parse_test_commands, plan_radius_runs, render_test_command, RadiusRun, RunnerCommand, ClaimProbeStatus, ClaimRecord, ClaimTestStatus, IngestSummary, ProbeRecord, RadiusTest,
    TestStatusCache, SLOW_TEST_MILLIS,
};
use scryer_core::ModelRef;

/// The derived tree with every claim's status (proven, stale, failing,
/// planned, and the probed mark) and the health counts, verdicts re-verified
/// against the code.
pub fn derived_status(
    r: &ModelRef,
) -> Result<(scryer_core::derived_tree::DerivedTree, scryer_core::derived_tree::DerivedStatus), String> {
    store::read_derived_status(r)
}

/// Ingest a cargo-mutants `outcomes.json` or Stryker `mutation.json` as
/// probe evidence: per file for both, per test where the tool says which
/// test killed what.
pub fn ingest_mutation_report(r: &ModelRef, report: &str) -> Result<MutationSummary, String> {
    store::ingest_mutation_file(r, report)
}

/// Ingest a per-test LCOV report: the lines each claim's test runs.
pub fn ingest_coverage_report(r: &ModelRef, lcov: &str) -> Result<CoverageSummary, String> {
    store::ingest_coverage_file(r, lcov)
}

/// Every claim's current test verdict, with staleness resolved against the code.
pub fn test_statuses(r: &ModelRef) -> Result<Vec<ClaimTestStatus>, String> {
    store::read_test_statuses(r)
}

/// The project's radius test commands, one per runner, when any is set.
pub fn test_commands(r: &ModelRef) -> Vec<RunnerCommand> {
    store::read_test_command(r).map(|raw| parse_test_commands(&raw)).unwrap_or_default()
}

/// Set one runner's radius command (`[*.rs] cargo …`, or a bare template for
/// the files no other runner claims), keeping the other runners'.
pub fn set_test_command(r: &ModelRef, line: &str) -> Result<(), String> {
    let raw = store::read_test_command(r).unwrap_or_default();
    store::write_test_command(r, &crate::test_evidence::set_runner_command(&raw, line))
}

/// Create the git-ignored directory radius runs report into; returns its
/// project-relative path.
pub fn reports_dir(r: &ModelRef) -> Result<String, String> {
    store::ensure_reports_dir(r)
}

/// Parse a JUnit report and record every case it matches.
pub fn ingest_report(r: &ModelRef, xml: &str) -> Result<IngestSummary, String> {
    store::ingest_report_file(r, xml)
}

/// Record a finished probe run against its claim.
pub fn record_probe_result(
    r: &ModelRef,
    resp_id: &str,
    probes: u32,
    survivors: Vec<String>,
) -> Result<(), String> {
    store::store_probe_result(r, resp_id, probes, survivors)
}

/// Every claim's current probe status.
pub fn probe_statuses(r: &ModelRef) -> Result<Vec<ClaimProbeStatus>, String> {
    store::read_probe_statuses(r)
}
