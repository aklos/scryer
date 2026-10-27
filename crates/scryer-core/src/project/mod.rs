//! The project as other containers reach it: its derived tree and status, the
//! build's dependency graph, its worktrees, comments, styles and settings, each
//! read through its store. [composition]

pub mod codebase;
pub mod derived_store;
pub mod edges;
pub mod git_worktrees;
pub mod project_comments;
pub mod settings_store;
pub mod styles;

pub use codebase::*;
pub use derived_store::*;
pub use edges::*;
pub use git_worktrees::*;
pub use project_comments::*;
pub use settings_store::*;
pub use styles::*;
