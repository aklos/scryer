//! Reading the shape of a project directory.

use crate::infrastructure::scan;
use std::path::Path;

/// Does this directory hold code we can model?
pub fn is_codebase(path: &Path) -> bool {
    scan::dir_is_codebase(path)
}

/// Every manifest directory and the manifest that declares it.
pub fn manifest_dirs(path: &Path) -> Vec<(String, String)> {
    scan::find_manifest_dirs(path)
}

/// The annotated directory tree a design-first model starts from.
pub fn project_structure(path: &Path) -> Result<String, String> {
    scan::render_project_tree(path)
}
