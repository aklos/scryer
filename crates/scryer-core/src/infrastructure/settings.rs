use std::fs;
use std::path::PathBuf;

pub use crate::domain::agent_settings::*;

// --- Subagent settings (global, ~/.scryer/settings.json) ---

/// Global scryer config directory (`~/.scryer`). Distinct from each project's
/// own `.scryer/` directory, which holds that project's `model.scry`.
pub fn global_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".scryer")
}

fn settings_path() -> PathBuf {
    global_dir().join("settings.json")
}

pub fn read_settings_file() -> SubagentSettings {
    let path = settings_path();
    if !path.exists() {
        return SubagentSettings::default();
    }
    fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn write_settings_file(settings: &SubagentSettings) -> Result<(), String> {
    let dir = global_dir();
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    fs::write(settings_path(), json).map_err(|e| e.to_string())
}
