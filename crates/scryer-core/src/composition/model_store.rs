//! Reading and writing a project's two model layers.

use crate::domain::diff;
use crate::infrastructure::storage;
use crate::{ModelLock, ModelRef, ScryModel};

/// The committed model — what the code is believed to satisfy.
pub fn read_model_at(r: &ModelRef) -> Result<ScryModel, String> {
    storage::read_committed_file(r)
}

/// Write the committed model, stamping and validating it on the way.
pub fn write_model_at(r: &ModelRef, model: &ScryModel) -> Result<(), String> {
    storage::write_committed_file(r, model)
}

/// The planned draft — what the canvas edits.
pub fn read_planned_at(r: &ModelRef) -> Result<ScryModel, String> {
    storage::read_planned_file(r)
}

/// Write the planned draft.
pub fn write_planned_at(r: &ModelRef, model: &ScryModel) -> Result<(), String> {
    storage::write_planned_file(r, model)
}

/// The planned draft, seeded from committed when the project has none yet.
pub fn read_planned_seeded_at(r: &ModelRef) -> Result<ScryModel, String> {
    storage::read_planned_seeded_file(r)
}

/// Create the planned draft if it is missing.
pub fn ensure_planned_at(r: &ModelRef) -> Result<(), String> {
    storage::ensure_planned_file(r)
}

/// Snapshot the committed model as the baseline a revision diff reads against.
pub fn save_baseline_at(r: &ModelRef, model: &ScryModel) -> Result<(), String> {
    storage::save_baseline_file(r, model)
}

/// The saved baseline, when there is one.
pub fn read_baseline_at(r: &ModelRef) -> Option<ScryModel> {
    storage::read_baseline_file(r)
}

/// Take the project's model lock for a read-modify-write.
pub fn lock_model(r: &ModelRef) -> Result<ModelLock, String> {
    storage::lock_model_file(r)
}

/// The raw committed JSON, for callers that need the bytes.
pub fn read_model_raw_at(r: &ModelRef) -> Result<String, String> {
    storage::read_committed_raw_file(r)
}

/// Write raw committed JSON.
pub fn write_model_raw_at(r: &ModelRef, data: &str) -> Result<(), String> {
    storage::write_committed_raw_file(r, data)
}

/// The raw planned JSON.
pub fn read_planned_raw_at(r: &ModelRef) -> Result<String, String> {
    storage::read_planned_raw_file(r)
}

/// Write raw planned JSON.
pub fn write_planned_raw_at(r: &ModelRef, data: &str) -> Result<(), String> {
    storage::write_planned_raw_file(r, data)
}

/// Repair a planned draft that shadows the committed model.
pub fn heal_shadow_draft(r: &ModelRef) -> Result<bool, String> {
    storage::heal_shadow_file(r)
}

/// Remove a project's model files.
pub fn delete_model_at(r: &ModelRef) -> Result<(), String> {
    storage::delete_model_files(r)
}

/// The plan diff for a project: what the draft claims that the committed model
/// does not.
pub fn plan_diff_at(r: &ModelRef) -> Result<diff::ModelDiff, String> {
    storage::plan_diff_files(r)
}
