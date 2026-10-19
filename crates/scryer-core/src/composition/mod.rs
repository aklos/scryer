//! The entry points other containers call, and the wiring behind them.
//!
//! A library's public surface is not its adapters: a consumer asks for "the
//! derived tree of this project" and gets it, without naming the file store,
//! the git probe or the settings file underneath. Each function here is that
//! wiring — it picks the adapter and hands back what the caller asked for.

pub mod agent_setup;
pub mod codebase;
pub mod derived_store;
pub mod edges;
pub mod session;
pub mod settings_store;
pub mod styles;
pub mod worktree;

pub use codebase::*;
pub use derived_store::*;
pub use edges::*;
pub use settings_store::*;
pub use styles::*;
