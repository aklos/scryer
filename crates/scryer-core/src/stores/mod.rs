//! The project's and the developer's state on disk: comment threads, session
//! logs, verdicts, settings, the caches of the derived tree and the
//! dependency graph, the harnesses' records of their running sessions, and the
//! desktop app's view as agents see it.
//! [infrastructure]

pub mod comment_store;
pub mod session_store;
pub mod verdict_store;
pub mod settings_file;
pub mod derived_tree_cache;
pub mod edges_cache;
pub mod harness_registry;
pub mod app_view;

pub use comment_store::*;
pub use session_store::*;
pub use verdict_store::*;
pub use settings_file::*;
pub use derived_tree_cache::*;
pub use edges_cache::*;
