pub mod application;
pub mod composition;
pub mod domain;
pub mod infrastructure;

// The crate's public surface, spelled the way consumers use it. Types and pure
// logic come from the domain; anything that touches the disk, git or the
// settings file is reached through composition, never through the adapter.
pub use composition::*;
pub use domain::layers::working_view;
pub use domain::agent_settings::{AgentSettings, SubagentSettings};
pub use infrastructure::storage::ModelLock;
pub use domain::model::*;
pub use domain::model_ref::*;
pub use domain::{build_edges as build_edges_derive, ears, ids, ownership, rules, seed, style, style_health, test_results};
pub use domain::ids::*;
/// Looking a file up in the model.
pub mod locate {
    pub use crate::application::locate::*;
    pub use crate::composition::locate::*;
}

/// The probe worktree.
pub mod worktree {
    pub use crate::infrastructure::worktree::*;
}
pub use domain::{concerns, diff, health};

/// The model's durable timeline.
pub mod history {
    pub use crate::composition::history_log::*;
    pub use crate::domain::history_events::*;
}

/// Anchor drift: what the model last saw, and what changed since.
pub mod drift {
    pub use crate::composition::sync::*;
    pub use crate::domain::drift_scope::*;
    pub use crate::composition::drift_verdicts as verdicts;
}

/// Fold refusals.
pub mod refusals {
    pub use crate::composition::refusal_ledger::*;
    pub use crate::domain::refusal::*;
}

/// Reading the shape of a project directory.
pub mod scan {
    pub use crate::composition::codebase::*;
    pub use crate::domain::product_code::*;
}

/// The developer's agent preferences.
pub mod settings {
    pub use crate::composition::settings_store::*;
    pub use crate::domain::agent_settings::*;
}

/// The build's dependency graph: its shape, its derivation, and its cache.
pub mod build_edges {
    pub use crate::composition::edges::*;
    pub use crate::domain::build_edges::*;
}

/// The session log and what the session hooks answer from it.
pub mod session {
    pub use crate::application::hooks::*;
    pub use crate::composition::session::*;
    pub use crate::domain::session::{asks_for_parity, Ask, AskEntry, AskKind, SessionLog};
}

/// The change ledger.
pub mod changes {
    pub use crate::composition::change_ledger::*;
    pub use crate::domain::changes::*;
}

/// Structural validation of a model.
pub mod validate {
    pub use crate::composition::coverage::*;
    pub use crate::domain::validate::*;
}

/// Reading and writing the model files.
pub mod storage {
    pub use crate::composition::model_store::*;
    pub use crate::domain::layers::working_view;
}

/// On-disk schema version. Files with a different `version` field are refused at load time.
pub const SCRY_VERSION: &str = "0.3";

