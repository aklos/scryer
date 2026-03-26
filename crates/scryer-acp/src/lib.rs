pub mod client;
pub mod events;
pub mod prompt;
pub mod runtime;

pub use events::AgentEvent;
pub use runtime::AcpRuntime;

/// Read the active MCP client identity written by scryer-mcp on connection.
/// Returns (name, version) if available.
pub fn active_client() -> Option<ActiveClient> {
    let path = scryer_core::models_dir().join("active-client.json");
    let contents = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&contents).ok()
}

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

/// Detect an available agent from PATH without requiring a prior MCP connection.
/// Prefers Claude Code, then Codex.
pub fn detect_available_agent() -> Option<AgentLaunch> {
    if let Ok(path) = which::which("claude") {
        return Some(AgentLaunch::Cli {
            binary: path.to_string_lossy().to_string(),
            kind: AgentKind::ClaudeCode,
        });
    }
    if let Ok(path) = which::which("codex") {
        return Some(AgentLaunch::Cli {
            binary: path.to_string_lossy().to_string(),
            kind: AgentKind::Codex,
        });
    }
    None
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ActiveClient {
    pub name: String,
    pub version: String,
}
