//! The native process hosting the desktop app's window, filesystem access and
//! agent orchestration. Laid out as an imperative `shell` over a small pure
//! `core`.
//!
//! Host the desktop application window and native OS integration.
//!
//! Set up AI coding tool integration for a developer's project.
//!
//! Watch a project's model files and read and write the model on disk.
//!
//! Orchestrate AI agent sessions for building the model and checking drift.
//!
//! Resolve source spans and symbols for the code inspector.

pub mod core;
pub mod shell;

pub use shell::app::run;
