//! The developer's global agent preferences: read from the settings file, with
//! the defaults when there is none, and written back to it.

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

#[cfg(test)]
mod tests {
    /// If the agent settings file is missing or does not parse, then reads the default agent preferences.
    #[test]
    #[ignore]
    fn missing_or_corrupt_settings_read_as_defaults() {
        todo!()
    }
}
