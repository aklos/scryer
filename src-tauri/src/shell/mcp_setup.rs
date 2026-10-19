//! The app's side of agent integration: thin commands over
//! `scryer_core::agent_setup`, the same code path `scryer-mcp setup` runs —
//! detecting which agent CLIs are installed, reporting the project's
//! integration status and switching it on or off with the scryer-mcp binary
//! found beside the app or on PATH.

use scryer_core::agent_setup::{self, SetupStatus};
use std::path::Path;

/// Which agent CLIs are on PATH, for the launch readout.
#[tauri::command]
pub(crate) fn detect_ai_tools() -> serde_json::Value {
    let detected = agent_setup::detect_tools();
    let has = |t| detected.contains(&t);
    serde_json::json!({
        "claude": has(agent_setup::Tool::ClaudeCode),
        "codex": has(agent_setup::Tool::Codex),
        "copilot": has(agent_setup::Tool::Copilot),
    })
}

/// Where agent integration stands for the project: `scryer-mcp setup`.
#[tauri::command]
pub(crate) fn agent_integration(project_path: String) -> SetupStatus {
    agent_setup::setup_status(Path::new(&project_path), &agent_setup::detect_tools())
}

/// Switch agent integration on or off: `scryer-mcp setup on|off`, registering
/// the scryer-mcp that ships with the app.
#[tauri::command]
pub(crate) fn set_agent_integration(project_path: String, on: bool) -> Result<SetupStatus, String> {
    let project = Path::new(&project_path);
    let detected = agent_setup::detect_tools();
    if on {
        let binary = find_scryer_mcp().ok_or("scryer-mcp binary not found")?;
        agent_setup::setup_on(project, &detected, &binary)
    } else {
        agent_setup::setup_off(project, &detected)
    }
}

/// Find the scryer-mcp binary path by checking common locations.
pub(crate) fn find_scryer_mcp() -> Option<String> {
    // Check next to scryer (same install dir)
    if let Ok(exe) = std::env::current_exe() {
        let sibling = exe.parent().map(|p| p.join("scryer-mcp"));
        if let Some(s) = sibling {
            if s.exists() {
                return Some(s.to_string_lossy().to_string());
            }
        }
    }
    // Check PATH
    which::which("scryer-mcp")
        .ok()
        .map(|p| p.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// With no binary beside the app, the PATH lookup finds scryer-mcp.
    #[test]
    fn find_scryer_mcp_falls_back_to_the_path_lookup() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = tempfile::tempdir().unwrap();
        let fake = dir.path().join("scryer-mcp");
        std::fs::write(&fake, "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();

        let old_path = std::env::var_os("PATH").unwrap_or_default();
        let mut paths = vec![dir.path().to_path_buf()];
        paths.extend(std::env::split_paths(&old_path));
        std::env::set_var("PATH", std::env::join_paths(paths).unwrap());
        let found = find_scryer_mcp();
        std::env::set_var("PATH", &old_path);

        assert_eq!(found.as_deref(), Some(fake.to_string_lossy().as_ref()));
    }
}
