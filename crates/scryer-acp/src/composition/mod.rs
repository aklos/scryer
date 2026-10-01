//! The crate's entry points: everything that reaches the outside world to
//! decide WHICH agent to launch, and the handle another container drives a
//! session through. The runtime itself stays private — a consumer names this
//! handle, never the subprocess adapter behind it.

use crate::domain::events::AgentEvent;
use crate::domain::launch::{AcpKind, AgentKind, AgentLaunch, LaunchMode};
use crate::infrastructure::runtime::AcpRuntime;
use tokio::sync::mpsc;

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

/// A handle on the agent runtime: start a session, cancel every session.
/// Cloneable, so the app can hold one and hand copies to its commands.
#[derive(Clone)]
pub struct AgentSync {
    runtime: AcpRuntime,
}

impl AgentSync {
    /// Build the runtime (spawns its background thread).
    pub fn open() -> Self {
        Self { runtime: AcpRuntime::new() }
    }

    /// Start a session and return its id once the launch succeeds.
    #[allow(clippy::too_many_arguments)]
    pub async fn start_session(
        &self,
        agent_binary: String,
        mode: LaunchMode,
        cwd: String,
        model_name: String,
        effort: String,
        mcp_binary: String,
        prompt: String,
        allowed_tools: Vec<String>,
        event_tx: mpsc::UnboundedSender<AgentEvent>,
    ) -> Result<String, String> {
        self.runtime
            .start_session(
                agent_binary, mode, cwd, model_name, effort, mcp_binary, prompt, allowed_tools,
                event_tx,
            )
            .await
    }

    /// Cancel every active session.
    pub async fn cancel(&self) -> Result<(), String> {
        self.runtime.cancel().await
    }
}
