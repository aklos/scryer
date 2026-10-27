//! The data shapes and file I/O every scryer surface shares: the C4 tree the
//! code derives, its status and health, test evidence, session logs, styles,
//! and the project's agent integration.
//!
//! Layered as a `library`, each module naming its layer at the end of its
//! doc's first paragraph. The facades below gather a concern's layers under
//! the one name consumers use.

pub mod agent_config;
pub mod agent_switch;
pub mod comment_threads;
pub mod dir_walk;
pub mod health;
pub mod probe_tree;
pub mod product_code;
pub mod session_hooks;
pub mod session_log;
pub mod c4_model;
pub use c4_model::{model, ids, concerns, seed, model_ref};
pub mod derived;
pub use derived::{c4_tree, derived_status, landing, worktree_standing};
pub mod architecture;
pub use architecture::{style, style_health, dependency_graph};
pub mod claim_grammar;
pub use claim_grammar::{ears, rules};
pub mod evidence;
pub use evidence::{test_results, verdicts};
pub mod agent_integration;
pub use agent_integration::{agent_entries, agent_settings};
pub mod stores;
pub use stores::{comment_store, session_store, verdict_store, settings_file, derived_tree_cache, edges_cache};
pub mod project;
pub use project::{codebase, derived_store, edges, git_worktrees, project_comments, settings_store, styles};

// The crate's public surface, spelled the way consumers use it. Types and pure
// logic come from the domain modules; anything that touches the disk, git or
// the settings file is reached through the composition modules, never
// through the adapter.
pub use codebase::*;
pub use derived_store::*;
pub use edges::*;
pub use settings_store::*;
pub use styles::*;
pub use agent_settings::{AgentSettings, SubagentSettings};
pub use model::*;
pub use model_ref::*;
pub use dependency_graph as build_edges_derive;
pub use ids::*;

/// Comment threads: their shape, and reading and writing them across the
/// project's worktrees.
pub mod comments {
    pub use crate::project_comments::*;
    pub use crate::comment_threads::*;
}

/// The project's git worktrees: the probe tree and the live checkouts.
pub mod worktree {
    pub use crate::git_worktrees::*;
    pub use crate::worktree_standing::*;
}

/// Reading the shape of a project directory.
pub mod scan {
    pub use crate::codebase::*;
    pub use crate::product_code::*;
}

/// Agent integration: one switch per project.
pub mod agent_setup {
    pub use crate::agent_switch::*;
    pub use crate::agent_entries::{EntryState, Part, Tool};
}

/// The developer's agent preferences.
pub mod settings {
    pub use crate::settings_store::*;
    pub use crate::agent_settings::*;
}

/// The C4 tree derived from packaging and tests: its shape and its cache.
pub mod derived_tree {
    pub use crate::derived_store::*;
    pub use crate::derived_status::*;
    pub use crate::c4_tree::*;
    pub use crate::landing::*;
}

/// The build's dependency graph: its shape, its derivation, and its cache.
pub mod build_edges {
    pub use crate::edges::*;
    pub use crate::dependency_graph::*;
}

/// The session log and what the session hooks answer from it.
pub mod session {
    pub use crate::session_hooks::*;
    pub use crate::session_log::{relativize, SessionLog};
}

/// On-disk schema version of the derived tree's model shape.
pub const SCRY_VERSION: &str = "0.3";
