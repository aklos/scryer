//! The test-evidence entry points other containers call. Each wires the
//! on-disk verdict cache to the model; the cache itself stays private.


use crate::infrastructure::test_status as store;

// Declared in the domain, named here so the crate has one public path per area.
pub use crate::domain::test_status::{
    render_test_command, ClaimProbeStatus, ClaimRecord, ClaimTestStatus, Evidence, IngestSummary,
    ProbeCheck, ProbeRecord, ProbeTarget, RadiusFile, RadiusTest, SessionRadius, TestStatusCache,
    probe_budget, MAX_PROBES_PER_SESSION, SLOW_TEST_MILLIS,
};
use scryer_core::session::SessionLog;
use scryer_core::test_results::ReportMatch;
use scryer_core::ModelRef;
use std::collections::BTreeMap;

/// Record a finished run's results against the claims their tests are attached to.
pub fn record_test_results(r: &ModelRef, report: &ReportMatch) -> Result<usize, String> {
    store::store_test_results(r, report)
}

/// Every claim's current test verdict, with staleness resolved against the code.
pub fn test_statuses(r: &ModelRef) -> Result<Vec<ClaimTestStatus>, String> {
    store::read_test_statuses(r)
}

/// The evidence standing behind each of these claims.
pub fn claim_evidence(
    r: &ModelRef,
    resp_ids: &[String],
) -> Result<BTreeMap<String, Evidence>, String> {
    store::evidence_for_claim(r, resp_ids)
}

/// The test files whose claims hold missing or stale verdicts — what to re-run.
pub fn test_blast_radius(r: &ModelRef) -> Result<Vec<RadiusFile>, String> {
    store::compute_blast_radius(r)
}

/// What needs running for one session's touches (`None`: project-wide).
pub fn session_radius(r: &ModelRef, touched: Option<&[String]>) -> Result<SessionRadius, String> {
    store::compute_session_radius(r, touched)
}

/// The project's radius test-command template, when one is set.
pub fn test_command(r: &ModelRef) -> Option<String> {
    store::read_test_command(r)
}

/// Store the project's radius test-command template.
pub fn set_test_command(r: &ModelRef, template: &str) -> Result<(), String> {
    store::write_test_command(r, template)
}

/// Create the git-ignored directory radius runs report into; returns its
/// project-relative path.
pub fn reports_dir(r: &ModelRef) -> Result<String, String> {
    store::ensure_reports_dir(r)
}

/// The Stop hook's probe check over `log`'s touches.
pub fn probe_check(r: &ModelRef, log: &SessionLog) -> ProbeCheck {
    store::session_probe_check(r, &log.touched)
}

/// Parse a JUnit report and record every case it matches.
pub fn ingest_report(r: &ModelRef, xml: &str) -> Result<IngestSummary, String> {
    store::ingest_report_file(r, xml)
}

/// What a mutation probe of this claim would target.
pub fn probe_target(r: &ModelRef, resp_id: &str) -> Result<ProbeTarget, String> {
    store::resolve_probe_target(r, resp_id)
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
