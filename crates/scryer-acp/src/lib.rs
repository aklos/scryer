pub mod client;
pub mod events;
pub mod prompt;
pub mod runtime;

pub use events::{AgentEvent, Usage};
pub use runtime::AcpRuntime;

/// Which agent harness we're dealing with.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AgentKind {
    ClaudeCode,
    Codex,
    Cursor,
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
        "cursor" => return cursor_launch(),
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

/// Resolve Cursor's current launcher or legacy standalone agent binary.
fn cursor_launch() -> Option<AgentLaunch> {
    scryer_core::cursor_agent::find_cursor_agent().map(|path| AgentLaunch::Cli {
        binary: path.to_string_lossy().to_string(),
        kind: AgentKind::Cursor,
    })
}

/// Detect an available agent honoring a user preference. The preferred agent is
/// tried first; if it isn't on PATH we fall back to the other so a fill still
/// runs. `pref` is "auto" | "claudeCode" | "codex" | "cursor".
pub fn detect_available_agent_pref(pref: &str) -> Option<AgentLaunch> {
    let auto = || claude_launch().or_else(codex_launch).or_else(cursor_launch);
    match pref {
        "codex" => codex_launch().or_else(claude_launch).or_else(cursor_launch),
        "claudeCode" => claude_launch().or_else(codex_launch).or_else(cursor_launch),
        "cursor" => cursor_launch().or_else(claude_launch).or_else(codex_launch),
        _ => auto(),
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ActiveClient {
    pub name: String,
    pub version: String,
}
