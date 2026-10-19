//! Resolves which model/effort settings apply to whichever agent CLI is actually being launched.
//!
//! Starts, reuses, and tears down the shared deterministic preview dev server for a project.
//!
//! Launches a repair agent session that authors realistic fixture data when deterministic preview rendering comes out empty or crashes.
//!
//! Owns the cross-command shared state: the live ACP runtime handle and build-scoped cancel flag, the project file watcher, and the shared preview server.
//!
//! Cancels in-flight and about-to-launch agent sessions on user abort, ahead of any live runtime state.

pub mod app;
pub mod derived;
pub mod mcp_setup;
pub mod observability;
pub mod preview;
pub mod project;
pub mod source_view;
pub mod state;
pub mod test_reports;
