//! Reading the shape of a project directory: whether it looks like a codebase
//! (a `.git` or a root manifest), which subdirectories carry a manifest, and
//! the annotated tree of its product files.

use crate::infrastructure::project_dir;
use std::path::Path;

/// Does this directory hold code we can model?
pub fn is_codebase(path: &Path) -> bool {
    project_dir::dir_is_codebase(path)
}

/// Every manifest directory and the manifest that declares it.
pub fn manifest_dirs(path: &Path) -> Vec<(String, String)> {
    project_dir::find_manifest_dirs(path)
}

/// The annotated directory tree a design-first model starts from.
pub fn project_structure(path: &Path) -> Result<String, String> {
    project_dir::render_project_tree(path)
}
