//! The build's cached dependency graph: written best-effort to
//! `.scryer/.build_edges.json` beside the model, and read back when a pass
//! wrote one.

use crate::architecture::BuildEdges;
use crate::stores as edges_cache;
use std::path::Path;

/// Persist the dependency graph next to the model.
pub fn write_build_edges(project: &Path, edges: &BuildEdges) -> Result<(), String> {
    edges_cache::write_edges_file(project, edges)
}

/// The cached dependency graph, when a build wrote one.
pub fn read_build_edges(path: &Path) -> Option<BuildEdges> {
    edges_cache::read_edges_file(path)
}
