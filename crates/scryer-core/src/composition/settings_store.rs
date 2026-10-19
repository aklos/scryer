//! The developer's global agent preferences.
//!
//! Reads the global per-agent settings file, falling back to defaults when absent.
//!
//! Writes the global per-agent settings file.

use crate::domain::agent_settings::SubagentSettings;
use crate::infrastructure::settings;

/// The stored preferences, or the defaults.
pub fn read_subagent_settings() -> SubagentSettings {
    settings::read_settings_file()
}

/// Persist the preferences.
pub fn write_subagent_settings(s: &SubagentSettings) -> Result<(), String> {
    settings::write_settings_file(s)
}
