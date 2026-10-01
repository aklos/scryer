//! The build's dependency graph, cached next to the model. Deriving the graph
//! is `domain::build_edges`; this is only its file.

use crate::domain::build_edges::BuildEdges;
use std::path::Path;

/// Persist the build dependency graph next to the model. Best-effort callers
/// should ignore the error — a missing cache only means the commit tool wires
/// no automatic links, not that the build fails.
pub fn write_edges_file(project: &Path, edges: &BuildEdges) -> Result<(), String> {
    let dir = project.join(".scryer");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let json = serde_json::to_string(edges).map_err(|e| e.to_string())?;
    std::fs::write(dir.join(".build_edges.json"), json).map_err(|e| e.to_string())
}

/// Read the cached build dependency graph, if one was written for this build.
pub fn read_edges_file(path: &Path) -> Option<BuildEdges> {
    let json = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&json).ok()
}
