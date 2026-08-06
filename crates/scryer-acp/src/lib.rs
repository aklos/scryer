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
        // Copilot CLI identifies itself over MCP as `github-copilot-developer`,
        // a name it shares with every other Copilot host (VS Code included), so
        // that handshake can only ever mean "some Copilot" — resolving it to the
        // CLI is right precisely because the CLI is the only one we can spawn.
        "copilot" | "copilot-cli" | "github-copilot" | "github-copilot-developer" => {
            if let Some(launch) = copilot_launch() {
                return Some(launch);
            }
        }
        _ => {}
    }

    // Try ACP adapter binary: "{name}-acp" or the name itself
    let acp_name = format!("{}-acp", client_name.replace(' ', "-"));
    if let Ok(path) = which::which(&acp_name) {
        return Some(AgentLaunch::Acp {
            binary: path.to_string_lossy().to_string(),
            kind: AcpKind::Adapter,
        });
    }
    if let Ok(path) = which::which(client_name) {
        return Some(AgentLaunch::Acp {
            binary: path.to_string_lossy().to_string(),
            kind: AcpKind::Adapter,
        });
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

/// Copilot CLI runs in ACP mode rather than print mode. Its `-p` mode would
/// work, but ACP is the better fit: the MCP server rides the session request,
/// which sidesteps Copilot skipping a project's `.mcp.json` in untrusted
/// folders, and the protocol reports tool calls as structured updates instead
/// of leaving the activity readout to parse a text stream.
fn copilot_launch() -> Option<AgentLaunch> {
    which::which("copilot").ok().map(|path| AgentLaunch::Acp {
        binary: path.to_string_lossy().to_string(),
        kind: AcpKind::Copilot,
    })
}

/// Detect an available agent honoring a user preference. The preferred agent is
/// tried first; if it isn't on PATH we fall back to the others so a fill still
/// runs. `pref` is "auto" | "claudeCode" | "codex" | "copilot".
pub fn detect_available_agent_pref(pref: &str) -> Option<AgentLaunch> {
    // Fallback order is the same everywhere — preference first, then the rest in
    // a fixed order — so "my agent isn't installed" degrades predictably.
    let others = || claude_launch().or_else(codex_launch).or_else(copilot_launch);
    match pref {
        "codex" => codex_launch().or_else(others),
        "claudeCode" => claude_launch().or_else(others),
        "copilot" => copilot_launch().or_else(others),
        _ => others(),
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ActiveClient {
    pub name: String,
    pub version: String,
}
