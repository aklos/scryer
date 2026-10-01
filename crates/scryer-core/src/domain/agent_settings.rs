//! The developer's agent preferences as a shape. Where they are stored is
//! `infrastructure::settings`.

use serde::{Deserialize, Serialize};

/// Per-agent model + reasoning effort. An empty model means "use the agent
/// CLI's own default". Effort values are agent-specific (Claude accepts
/// low/medium/high/xhigh/max; Codex accepts minimal/low/medium/high; Copilot
/// accepts none/low/medium/high/xhigh/max).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSettings {
    #[serde(default)]
    pub model: String,
    #[serde(default = "default_effort")]
    pub effort: String,
}

impl Default for AgentSettings {
    fn default() -> Self {
        Self {
            model: String::new(),
            effort: default_effort(),
        }
    }
}

/// Agent preference + each agent's own settings, applied to spawned fill
/// sessions. Field-level serde defaults keep older/partial settings.json files
/// loadable.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubagentSettings {
    /// Which agent to launch: "auto" | "claudeCode" | "codex" | "copilot".
    #[serde(default = "default_agent")]
    pub agent: String,
    #[serde(default)]
    pub claude: AgentSettings,
    #[serde(default)]
    pub codex: AgentSettings,
    #[serde(default)]
    pub copilot: AgentSettings,
    /// Confirm before any UI action launches an agent (a billable run). Lets the
    /// user see which agent + model + effort will run; "don't ask again" clears
    /// it. Defaults to true so the gate is opt-out, not opt-in.
    #[serde(default = "default_confirm_launch")]
    pub confirm_launch: bool,
}

impl Default for SubagentSettings {
    fn default() -> Self {
        Self {
            agent: default_agent(),
            claude: AgentSettings::default(),
            codex: AgentSettings::default(),
            copilot: AgentSettings::default(),
            confirm_launch: default_confirm_launch(),
        }
    }
}

pub(crate) fn default_agent() -> String {
    "auto".to_string()
}

pub(crate) fn default_confirm_launch() -> bool {
    true
}

pub(crate) fn default_effort() -> String {
    "medium".to_string()
}
