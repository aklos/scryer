//! Walks a project's file tree and turns it into a deterministic ProjectContext, reusing unchanged files' cached parses across calls.
//!
//! Enumerates every project file, including non-source ones, for boundary/territory checks used elsewhere in the system.

pub mod claim_files;
pub mod derived_tree;
pub mod extract;
pub mod test_status;
