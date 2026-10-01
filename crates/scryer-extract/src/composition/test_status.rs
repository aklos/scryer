//! The test-evidence entry points other containers call. Each wires the
//! on-disk verdict cache to the model; the cache itself stays private.


use crate::infrastructure::test_status as store;

// Declared in the domain, named here so the crate has one public path per area.
pub use crate::domain::test_status::{
    ClaimProbeStatus, ClaimRecord, ClaimTestStatus, Evidence, IngestSummary, ProbeRecord,
    ProbeTarget, RadiusFile, TestStatusCache,
};
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
