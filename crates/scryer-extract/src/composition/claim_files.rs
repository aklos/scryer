//! Writing claims back into the code: reword a test's description, add a
//! claim as a todo stub, rename a container in `structure.toml`.

use crate::infrastructure::{claim_files as files, migrate as migration};
use std::path::Path;

pub use crate::infrastructure::claim_files::stub_target;
pub use crate::infrastructure::migrate::MigrationReport;

/// Reword a derived claim's test description in its file. Returns the file.
pub fn edit_claim_text(project: &Path, resp_id: &str, text: &str) -> Result<String, String> {
    files::edit_claim_text(project, resp_id, text)
}

/// Write `text` as a todo stub for `node_id`. Returns the file written.
pub fn write_claim_stub(project: &Path, node_id: &str, text: &str) -> Result<String, String> {
    files::write_claim_stub(project, node_id, text)
}

/// Set a container's display name in `.scryer/structure.toml`.
pub fn rename_container(project: &Path, node_id: &str, name: &str) -> Result<(), String> {
    files::rename_container(project, node_id, name)
}

/// One-shot migration of the project's authored model into its code.
pub fn migrate(project: &Path) -> Result<MigrationReport, String> {
    migration::migrate(project)
}
