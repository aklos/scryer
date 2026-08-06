use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

// --- Subagent settings (global, ~/.scryer/settings.json) ---

/// Global scryer config directory (`~/.scryer`). Distinct from each project's
/// own `.scryer/` directory, which holds that project's `model.scry`.
pub fn global_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".scryer")
}

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

fn default_agent() -> String {
    "auto".to_string()
}

fn default_confirm_launch() -> bool {
    true
}

fn default_effort() -> String {
    "medium".to_string()
}

fn settings_path() -> PathBuf {
    global_dir().join("settings.json")
}

pub fn read_subagent_settings() -> SubagentSettings {
    let path = settings_path();
    if !path.exists() {
        return SubagentSettings::default();
    }
    fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn write_subagent_settings(settings: &SubagentSettings) -> Result<(), String> {
    let dir = global_dir();
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    fs::write(settings_path(), json).map_err(|e| e.to_string())
}
