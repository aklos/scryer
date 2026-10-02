//! The entry points other containers call, and the wiring behind them.
//!
//! A library's public surface is not its adapters: a consumer asks for "the
//! model of this project" and gets it, without naming the file store, the
//! git probe or the settings file underneath. Each function here is that
//! wiring — it picks the adapter and hands back what the caller asked for.

pub mod change_ledger;
pub mod codebase;
pub mod coverage;
pub mod drift_verdicts;
pub mod fold;
pub mod edges;
pub mod locate;
pub mod history_log;
pub mod model_store;
pub mod refusal_ledger;
pub mod session;
pub mod settings_store;
pub mod styles;
pub mod sync;

pub use change_ledger::*;
pub use codebase::*;
pub use coverage::*;
pub use fold::*;
pub use locate::*;
pub use edges::*;
pub use history_log::*;
pub use model_store::*;
pub use refusal_ledger::*;
pub use settings_store::*;
pub use styles::*;
pub use sync::*;
