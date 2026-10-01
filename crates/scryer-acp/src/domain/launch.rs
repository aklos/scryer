//! The launch vocabulary: which agent harness, which ACP dialect, and how a
//! resolved agent is spawned. Pure data — probing PATH for a binary is the
//! composition layer's job.

/// Which agent harness we're dealing with.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AgentKind {
    ClaudeCode,
    Codex,
    Other,
}

/// Which ACP dialect a subprocess speaks — what to put on its command line
/// before the protocol takes over. The handshake itself is identical either
/// way; this only decides argv.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AcpKind {
    /// Copilot CLI, which serves ACP from its own binary rather than an
    /// adapter: `copilot --acp --stdio`. Model and reasoning effort are
    /// server-level flags there, so they are set at spawn rather than
    /// negotiated per session.
    Copilot,
    /// A `{name}-acp` adapter. Nothing can be assumed about its flags, so it is
    /// spawned bare and takes whatever defaults its own config gives it.
    Adapter,
}

/// How to launch a resolved agent.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "mode", rename_all = "camelCase")]
pub enum AgentLaunch {
    /// Spawn via CLI print mode. Uses the user's subscription.
    Cli { binary: String, kind: AgentKind },
    /// Spawn as an ACP subprocess. Requires API key or its own auth.
    Acp { binary: String, kind: AcpKind },
}

/// How the agent should be launched.
#[derive(Clone)]
pub enum LaunchMode {
    /// CLI print mode. Uses the user's subscription.
    Cli { kind: AgentKind },
    /// ACP subprocess.
    Acp { kind: AcpKind },
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ActiveClient {
    pub name: String,
    pub version: String,
}
