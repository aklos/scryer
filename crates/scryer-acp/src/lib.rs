pub mod client;
pub mod events;
pub mod prompt;
pub mod runtime;

pub use events::AgentEvent;
pub use runtime::AcpRuntime;

/// macOS GUI apps launched via Spotlight, Dock, or Finder inherit a minimal
/// PATH (`/usr/bin:/bin:/usr/sbin:/sbin`) that excludes user-local install
/// directories like `~/.local/bin`, `/opt/homebrew/bin`, or `/usr/local/bin`.
/// Append common locations so that `which::which()` can find agent binaries.
#[cfg(target_os = "macos")]
fn ensure_common_paths() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let current = std::env::var("PATH").unwrap_or_default();
        let home = std::env::var("HOME").unwrap_or_else(|_| "/Users/nobody".into());
        let extra = [
            format!("{home}/.local/bin"),
            "/usr/local/bin".into(),
            "/opt/homebrew/bin".into(),
            format!("{home}/.cargo/bin"),
        ];
        let mut parts: Vec<&str> = current.split(':').collect();
        for dir in &extra {
            if !parts.contains(&dir.as_str()) {
                parts.push(dir);
            }
        }
        std::env::set_var("PATH", parts.join(":"));
    });
}

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
    #[cfg(target_os = "macos")]
    ensure_common_paths();

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

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ActiveClient {
    pub name: String,
    pub version: String,
}
