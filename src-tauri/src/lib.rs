//! The native process hosting the desktop app's window, filesystem access and
//! agent orchestration. Laid out as an imperative `shell` over a small pure
//! `core`.

pub mod core;
pub mod shell;

pub use shell::app::run;
