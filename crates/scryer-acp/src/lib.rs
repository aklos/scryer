//! Spawns and drives AI coding agent subprocesses to update architecture
//! models.
//!
//! Layered as a `library`: `domain` is the launch vocabulary and the event
//! shapes, `application` the prompts, `infrastructure` the subprocess runtime
//! and the ACP client, `composition` the entry points another container uses.
//! The re-exports below are that public surface.
//!
//! Relay prompts to the agent and stream its events and usage back to the host.
//!
//! Resolve which coding agent to launch and how to communicate with it.
//!
//! Negotiate and drive a session with an external coding agent.

pub mod application;
pub mod composition;
pub mod domain;
pub mod infrastructure;

pub use application::prompt;
pub use composition::{detect_available_agent_pref, resolve_agent_binary, AgentSync};
pub use domain::events::{self, AgentEvent, Usage};
pub use domain::launch::{AcpKind, ActiveClient, AgentKind, AgentLaunch, LaunchMode};
