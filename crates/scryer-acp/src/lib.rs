//! Spawns and drives AI coding agent subprocesses — today, to author fixture
//! data for a component's live preview.
//!
//! Layered as a `library`, each module naming its layer at the end of its
//! doc: `launch` and `events` are the domain, `prompt` the application,
//! `runtime` and `client` the infrastructure, `agents` the composition that
//! another container uses. The re-exports below are that public surface.

mod agents;
mod client;
pub mod events;
pub mod launch;
pub mod prompt;
mod runtime;

pub use agents::{detect_available_agent_pref, resolve_agent_binary, AgentSync};
pub use events::{AgentEvent, Usage};
pub use launch::{AcpKind, ActiveClient, AgentKind, AgentLaunch, LaunchMode};
