//! The build's cached dependency graph.

use crate::domain::build_edges::BuildEdges;
use crate::infrastructure::edges_cache;
use std::path::Path;

/// Persist the dependency graph next to the model.
pub fn write_build_edges(project: &Path, edges: &BuildEdges) -> Result<(), String> {
    edges_cache::write_edges_file(project, edges)
}

/// The cached dependency graph, when a build wrote one.
pub fn read_build_edges(path: &Path) -> Option<BuildEdges> {
    edges_cache::read_edges_file(path)
}
