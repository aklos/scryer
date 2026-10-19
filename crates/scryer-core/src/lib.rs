//! The data shapes and file I/O every scryer surface shares: the C4 tree the
//! code derives, its status and health, test evidence, session logs, styles,
//! and the project's agent integration.

pub mod composition;
pub mod domain;
pub mod infrastructure;

// The crate's public surface, spelled the way consumers use it. Types and pure
// logic come from the domain; anything that touches the disk, git or the
// settings file is reached through composition, never through the adapter.
pub use composition::*;
pub use domain::agent_settings::{AgentSettings, SubagentSettings};
pub use domain::model::*;
pub use domain::model_ref::*;
pub use domain::{build_edges as build_edges_derive, ears, ids, rules, seed, style, style_health, test_results, verdicts};
pub use domain::ids::*;

/// The probe worktree.
pub mod worktree {
    pub use crate::infrastructure::worktree::*;
}
pub use domain::{concerns, health};

/// Reading the shape of a project directory.
pub mod scan {
    pub use crate::composition::codebase::*;
    pub use crate::domain::product_code::*;
}

/// Agent integration: one switch per project.
pub mod agent_setup {
    pub use crate::composition::agent_setup::*;
    pub use crate::domain::agent_setup::{EntryState, Part, Tool};
}

/// The developer's agent preferences.
pub mod settings {
    pub use crate::composition::settings_store::*;
    pub use crate::domain::agent_settings::*;
}

/// The C4 tree derived from packaging and tests: its shape and its cache.
pub mod derived_tree {
    pub use crate::composition::derived_store::*;
    pub use crate::domain::derived_status::*;
    pub use crate::domain::derived_tree::*;
    pub use crate::domain::landing::*;
}

/// The build's dependency graph: its shape, its derivation, and its cache.
pub mod build_edges {
    pub use crate::composition::edges::*;
    pub use crate::domain::build_edges::*;
}

/// The session log and what the session hooks answer from it.
pub mod session {
    pub use crate::composition::session::*;
    pub use crate::domain::session::{relativize, SessionLog};
}

/// On-disk schema version of the derived tree's model shape.
pub const SCRY_VERSION: &str = "0.3";
