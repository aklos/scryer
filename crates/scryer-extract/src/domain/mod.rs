//! Resolves identifier, import, and qualified-path references into dependency edges, within a file's container and across containers via manifest-declared names and TS aliases.
//!
//! Slices the full context into per-container/per-scope views and compacts them into a token-budgeted wire payload for agent consumption.
//!
//! Assembles the deterministic ProjectContext — every file's symbols plus the resolved symbol/file dependency graph — from parsed source and discovered containers.

pub mod anchors;
pub mod claim_edit;
pub mod claims;
pub mod context;
pub mod derive;
pub mod facts;
pub mod lang;
pub mod mutations;
pub mod structure;
pub mod test_status;
