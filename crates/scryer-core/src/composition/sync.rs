//! What the model last saw of the working tree, and what changed since.

use crate::domain::drift_scope::{DriftScope, SyncState};
use crate::infrastructure::{drift, storage};
use crate::{ModelRef, ScryModel};
use std::collections::BTreeSet;
use std::path::Path;

/// The project's recorded sync state.
pub fn read_sync_state(r: &ModelRef) -> SyncState {
    storage::read_sync_file(r)
}

/// Record a new sync state.
pub fn write_sync_state(r: &ModelRef, state: &SyncState) -> Result<(), String> {
    storage::write_sync_file(r, state)
}

/// The commit the working tree is on, when it is a git repository.
pub fn head_commit(project: &Path) -> Option<String> {
    drift::git_head_commit(project)
}

/// Files that changed since the recorded sync point.
pub fn changed_files_since(project: &Path, sync: &SyncState) -> BTreeSet<String> {
    drift::files_changed_since(project, sync)
}

/// Every product file in the project.
pub fn product_file_inventory(project: &Path) -> BTreeSet<String> {
    drift::walk_product_files(project)
}

/// The boundary-owning nodes whose code changed since the last reconcile.
pub fn drifted_scopes(model: &ScryModel, project: &Path, sync: &SyncState) -> Vec<DriftScope> {
    drift::scan_drifted_scopes(model, project, sync)
}
