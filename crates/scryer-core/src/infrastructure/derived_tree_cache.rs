//! The derived tree, cached next to the model. Deriving it is
//! `scryer-extract`'s job; this is only its file.

use crate::domain::derived_tree::DerivedTree;
use std::path::Path;

/// Persist the derived tree. Best-effort callers may ignore the error — a
/// missing cache only means the next read derives again.
pub fn write_derived_file(path: &Path, tree: &DerivedTree) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string(tree).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| e.to_string())
}

/// Read the cached derived tree, if one was written.
pub fn read_derived_file(path: &Path) -> Option<DerivedTree> {
    let json = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&json).ok()
}
