//! The native process behind the desktop app: its window, the project watch
//! and test-report ingestion, agent integration setup, the preview server and
//! the code inspector's source reads. Laid out as an imperative `shell` over a
//! small pure `core` (syntax highlighting and symbol resolution).

pub mod core;
pub mod shell;

pub use shell::app::run;
