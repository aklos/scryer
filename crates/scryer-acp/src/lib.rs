pub mod client;
pub mod events;
pub mod prompt;
pub mod runtime;

pub use events::AgentEvent;
pub use runtime::AcpRuntime;

/// Which agent harness we're dealing with.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AgentKind {
    ClaudeCode,
    Codex,
    Other,
}

/// How to launch a resolved agent.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "mode", rename_all = "camelCase")]
pub enum AgentLaunch {
    /// Spawn via CLI print mode. Uses the user's subscription.
    Cli { binary: String, kind: AgentKind },
    /// Spawn as an ACP subprocess. Requires API key or its own auth.
    Acp { binary: String },
}

/// Resolve an MCP client name to a launch config.
/// Known CLI agents get Cli mode; others fall back to ACP conventions.
pub fn resolve_agent_binary(client_name: &str) -> Option<AgentLaunch> {
    // Known CLI agents that support print mode
    match client_name {
        "claude-code" => {
            if let Ok(path) = which::which("claude") {
                return Some(AgentLaunch::Cli {
                    binary: path.to_string_lossy().to_string(),
                    kind: AgentKind::ClaudeCode,
                });
            }
        }
        "codex" | "codex-cli" => {
            if let Ok(path) = which::which("codex") {
                return Some(AgentLaunch::Cli {
                    binary: path.to_string_lossy().to_string(),
                    kind: AgentKind::Codex,
                });
            }
        }
        _ => {}
    }

    // Try ACP adapter binary: "{name}-acp" or the name itself
    let acp_name = format!("{}-acp", client_name.replace(' ', "-"));
    if let Ok(path) = which::which(&acp_name) {
        return Some(AgentLaunch::Acp { binary: path.to_string_lossy().to_string() });
    }
    if let Ok(path) = which::which(client_name) {
        return Some(AgentLaunch::Acp { binary: path.to_string_lossy().to_string() });
    }

    None
}

fn claude_launch() -> Option<AgentLaunch> {
    which::which("claude").ok().map(|path| AgentLaunch::Cli {
        binary: path.to_string_lossy().to_string(),
        kind: AgentKind::ClaudeCode,
    })
}

fn codex_launch() -> Option<AgentLaunch> {
    which::which("codex").ok().map(|path| AgentLaunch::Cli {
        binary: path.to_string_lossy().to_string(),
        kind: AgentKind::Codex,
    })
}

/// Detect an available agent from PATH without requiring a prior MCP connection.
/// Prefers Claude Code, then Codex.
pub fn detect_available_agent() -> Option<AgentLaunch> {
    detect_available_agent_pref("auto")
}

/// Detect an available agent honoring a user preference. The preferred agent is
/// tried first; if it isn't on PATH we fall back to the other so a fill still
/// runs. `pref` is "auto" | "claudeCode" | "codex".
pub fn detect_available_agent_pref(pref: &str) -> Option<AgentLaunch> {
    match pref {
        "codex" => codex_launch().or_else(claude_launch),
        "claudeCode" => claude_launch().or_else(codex_launch),
        _ => claude_launch().or_else(codex_launch),
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ActiveClient {
    pub name: String,
    pub version: String,
}
