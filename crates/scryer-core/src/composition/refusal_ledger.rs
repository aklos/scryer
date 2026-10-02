//! Why the fold declined specific claims.

use crate::domain::refusal::Refusal;
use crate::infrastructure::refusals;
use crate::ModelRef;

/// Every standing refusal for this project.
pub fn read_refusals(r: &ModelRef) -> Vec<Refusal> {
    refusals::read_refusal_file(r)
}

/// Record this fold's refusals and clear the ones it resolved.
pub fn update_refusals(r: &ModelRef, refused: &[Refusal], folded: &[String]) -> Result<(), String> {
    refusals::update_refusal_file(r, refused, folded)
}

/// Drop refusals whose claims no longer exist.
pub fn prune_refusals(r: &ModelRef, live_resp_ids: &std::collections::HashSet<String>) {
    refusals::prune_refusal_file(r, live_resp_ids)
}
