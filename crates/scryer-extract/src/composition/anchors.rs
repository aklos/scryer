//! The anchor entry points other containers call. Each one wires the stored
//! baseline and the project's files to the pure span logic; the adapters
//! themselves stay private to this crate.
//!
//! Checks every anchor whose file changed since the last sync, silently re-anchoring moved-but-unchanged symbols and reporting content changes, breaks, or missing files as observations.
//!
//! Reports every anchor the baseline holds no fingerprint for — a silent gap where drift can never fire.
//!
//! Flags explicit line-range mappings that cover their whole enclosing symbol, where a symbol-only anchor was intended instead.
//!
//! Fingerprints every model anchor against the working tree and writes the baseline, called at every reconcile point.
//!
//! Refreshes the baseline for exactly a given set of claim keys, leaving every other entry's fingerprint untouched.
//!
//! Rolls up anchor regressions that no pending plan item explains into per-node drift scopes, suppressing churn the plan already accounts for.

// The vocabulary a caller reads these results in. Declared in the domain; named
// here so `scryer_extract::anchors::…` stays one path for the whole surface.
pub use crate::domain::anchors::{
    covers_extent, AnchorBaseline, AnchorCheck, AnchorEntry, AnchorObservation, AnchorState,
    UntrackedAnchor,
};
use crate::infrastructure::anchors as store;
use scryer_core::{drift, ModelRef, ScryModel};
use std::path::Path;

/// Re-resolve every anchor in a touched file and report what changed, broke or
/// went missing; moved-but-identical spans are re-anchored in place.
pub fn check_anchors(r: &ModelRef) -> Result<AnchorCheck, String> {
    store::run_anchor_check(r)
}

/// [`check_anchors`] for a known set of changed files — no project walk.
pub fn check_anchors_in(
    r: &ModelRef,
    files: &std::collections::BTreeSet<String>,
) -> Result<AnchorCheck, String> {
    store::run_anchor_check_in(r, files)
}

/// Fingerprint every anchor in the model and write the baseline.
pub fn write_baseline(r: &ModelRef) -> Result<usize, String> {
    store::store_baseline(r)
}

/// Write the baseline for just these anchor keys — the fold's own anchors,
/// rather than the whole model.
pub fn write_baseline_for(
    r: &ModelRef,
    keys: &std::collections::BTreeSet<String>,
) -> Result<usize, String> {
    store::store_baseline_for(r, keys)
}

/// Anchors the baseline has never seen — code the model points at but never
/// fingerprinted.
pub fn untracked_anchors(r: &ModelRef) -> Result<Vec<UntrackedAnchor>, String> {
    store::find_untracked(r)
}

/// Scopes whose code changed with nothing in the plan to account for it.
pub fn out_of_plan_scopes(r: &ModelRef) -> Result<Vec<drift::DriftScope>, String> {
    store::find_out_of_plan(r)
}

/// Warnings for anchors whose line range covers their whole symbol.
pub fn whole_symbol_warnings(model: &ScryModel, project: &Path) -> Vec<String> {
    store::whole_symbol_checks(model, project)
}

/// A cached reader for symbol extents, for a caller resolving many anchors in
/// one pass.
pub struct SpanReader<'p> {
    inner: store::ExtentResolver<'p>,
}

impl<'p> SpanReader<'p> {
    pub fn new(project: &'p Path) -> Self {
        Self { inner: store::ExtentResolver::new(project) }
    }

    /// The (start, end) lines of `symbol` in `rel`, nearest to `near`.
    pub fn extent(&mut self, rel: &str, symbol: &str, near: Option<u32>) -> Option<(u32, u32)> {
        self.inner.extent(rel, symbol, near)
    }
}
