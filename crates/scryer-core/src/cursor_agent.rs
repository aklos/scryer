//! Detect Cursor's agent CLI through the current `cursor agent` command or the
//! legacy standalone `agent` binary.

use std::path::{Path, PathBuf};
use std::process::Command;

const SCRYER_MCP_PERMISSION: &str = "Mcp(scryer:*)";

/// Find Cursor's agent entry point, preferring the current `cursor agent`
/// launcher and falling back to the legacy standalone binary.
pub fn find_cursor_agent() -> Option<PathBuf> {
    find_on_path("cursor")
        .filter(|p| is_cursor_agent(p))
        .or_else(|| find_on_path("agent").filter(|p| is_cursor_agent(p)))
        .or_else(|| {
            let home = dirs::home_dir()?;
            let local = home.join(".local/bin/agent");
            local
                .exists()
                .then_some(local)
                .filter(|p| is_cursor_agent(p))
        })
}

/// Does this executable expose Cursor's agent CLI?
pub fn is_cursor_agent(path: &Path) -> bool {
    if path_canonical_looks_like_cursor(path) {
        return true;
    }
    if version_looks_like_cursor_agent(path) {
        return true;
    }
    agent_status_succeeds(path)
}

/// The agent status command exits 0 when logged in.
pub fn cursor_agent_authenticated(path: &Path) -> bool {
    is_cursor_agent(path) && agent_status_succeeds(path)
}

/// Whether this entry point requires the `agent` subcommand.
pub fn uses_cursor_agent_subcommand(path: &Path) -> bool {
    path.file_stem()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("cursor"))
}

/// Check whether this project grants Cursor CLI access to Scryer's MCP tools.
pub fn cursor_mcp_permission_enabled(project: &Path) -> bool {
    let path = project.join(".cursor").join("cli.json");
    let Ok(raw) = std::fs::read_to_string(path) else {
        return false;
    };
    let Ok(root) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return false;
    };
    let allow_enabled = root
        .pointer("/permissions/allow")
        .and_then(|value| value.as_array())
        .is_some_and(|allow| {
            allow
                .iter()
                .any(|entry| entry.as_str() == Some(SCRYER_MCP_PERMISSION))
        });
    allow_enabled
        && root
            .pointer("/permissions/deny")
            .is_some_and(|value| value.is_array())
}

/// Merge Scryer's least-privilege MCP permission into `.cursor/cli.json`.
///
/// Existing permissions and unrelated settings are preserved. Malformed JSON
/// is refused rather than replaced because the file may contain user policy.
pub fn install_cursor_mcp_permission(project: &Path) -> Result<PathBuf, String> {
    let cursor_dir = project.join(".cursor");
    let path = cursor_dir.join("cli.json");
    let mut root: serde_json::Value = if path.exists() {
        let raw = std::fs::read_to_string(&path).map_err(|error| error.to_string())?;
        serde_json::from_str(&raw).map_err(|error| {
            format!(
                "{} is not valid JSON ({error}); refusing to overwrite it",
                path.display()
            )
        })?
    } else {
        serde_json::json!({})
    };

    if !root.is_object() {
        return Err(format!("{} must contain a JSON object", path.display()));
    }
    if root.get("permissions").is_none() {
        root["permissions"] = serde_json::json!({});
    } else if !root["permissions"].is_object() {
        return Err(format!("{}.permissions must be an object", path.display()));
    }
    for key in ["allow", "deny"] {
        if root["permissions"].get(key).is_none() {
            root["permissions"][key] = serde_json::json!([]);
        } else if !root["permissions"][key].is_array() {
            return Err(format!(
                "{}.permissions.{key} must be an array",
                path.display()
            ));
        }
    }

    let allow = root
        .pointer_mut("/permissions/allow")
        .and_then(|value| value.as_array_mut())
        .expect("permissions.allow was initialized as an array");
    if !allow
        .iter()
        .any(|entry| entry.as_str() == Some(SCRYER_MCP_PERMISSION))
    {
        allow.push(serde_json::json!(SCRYER_MCP_PERMISSION));
    }

    std::fs::create_dir_all(&cursor_dir).map_err(|error| error.to_string())?;
    let json = serde_json::to_string_pretty(&root).map_err(|error| error.to_string())?;
    std::fs::write(&path, json).map_err(|error| error.to_string())?;
    Ok(path)
}

fn path_canonical_looks_like_cursor(path: &Path) -> bool {
    if let Ok(target) = std::fs::read_link(path) {
        let t = target.to_string_lossy().to_ascii_lowercase();
        if t.contains("cursor-agent") || t.contains("/.cursor/") {
            return true;
        }
    }
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let s = canonical.to_string_lossy().to_ascii_lowercase();
    s.contains("cursor-agent") || s.contains("/.cursor/")
}

fn version_looks_like_cursor_agent(path: &Path) -> bool {
    let Ok(out) = cursor_agent_command(path).arg("--version").output() else {
        return false;
    };
    if !out.status.success() {
        return false;
    }
    let Ok(text) = String::from_utf8(out.stdout) else {
        return false;
    };
    let line = text.lines().next().unwrap_or("").trim();
    looks_like_cursor_version(line)
}

fn agent_status_succeeds(path: &Path) -> bool {
    cursor_agent_command(path)
        .arg("status")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn cursor_agent_command(path: &Path) -> Command {
    let mut command = Command::new(path);
    if uses_cursor_agent_subcommand(path) {
        command.arg("agent");
    }
    command.current_dir(std::env::temp_dir());
    command
}

/// Cursor agent versions look like `YYYY.MM.DD-<git-ish>`.
fn looks_like_cursor_version(s: &str) -> bool {
    let s = s.trim();
    if s.to_ascii_lowercase().contains("cursor") {
        return true;
    }
    let bytes = s.as_bytes();
    bytes.len() >= 12
        && bytes.get(4) == Some(&b'.')
        && bytes.get(7) == Some(&b'.')
        && s.contains('-')
}

fn find_on_path(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths).find_map(|dir| {
            let candidate = dir.join(name);
            if candidate.is_file() {
                Some(candidate)
            } else {
                let exe = dir.join(format!("{name}.exe"));
                exe.is_file().then_some(exe)
            }
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_version_shape() {
        assert!(looks_like_cursor_version("2026.07.23-e383d2b"));
        assert!(!looks_like_cursor_version("claude-code 1.0"));
        assert!(!looks_like_cursor_version(""));
    }

    #[test]
    fn current_cursor_launcher_uses_agent_subcommand() {
        assert!(uses_cursor_agent_subcommand(Path::new("/usr/bin/cursor")));
        assert!(!uses_cursor_agent_subcommand(Path::new(
            "/home/dev/.local/bin/agent"
        )));
    }

    #[test]
    fn cursor_permission_install_is_idempotent_and_preserves_foreign_policy() {
        let dir = tempfile::tempdir().unwrap();
        let cursor_dir = dir.path().join(".cursor");
        std::fs::create_dir_all(&cursor_dir).unwrap();
        std::fs::write(
            cursor_dir.join("cli.json"),
            serde_json::json!({
                "permissions": {
                    "allow": ["Read(**/*.md)"],
                    "deny": ["Write(.env*)"]
                },
                "other": true
            })
            .to_string(),
        )
        .unwrap();

        install_cursor_mcp_permission(dir.path()).unwrap();
        install_cursor_mcp_permission(dir.path()).unwrap();

        assert!(cursor_mcp_permission_enabled(dir.path()));
        let root: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(cursor_dir.join("cli.json")).unwrap())
                .unwrap();
        assert_eq!(root["permissions"]["allow"].as_array().unwrap().len(), 2);
        assert_eq!(root["permissions"]["deny"][0], "Write(.env*)");
        assert_eq!(root["other"], true);
    }

    #[test]
    fn cursor_permission_install_creates_required_deny_array() {
        let dir = tempfile::tempdir().unwrap();

        install_cursor_mcp_permission(dir.path()).unwrap();

        let root: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(dir.path().join(".cursor/cli.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(root["permissions"]["allow"][0], SCRYER_MCP_PERMISSION);
        assert_eq!(root["permissions"]["deny"], serde_json::json!([]));
        assert!(cursor_mcp_permission_enabled(dir.path()));
    }

    #[test]
    fn cursor_permission_install_refuses_malformed_json() {
        let dir = tempfile::tempdir().unwrap();
        let cursor_dir = dir.path().join(".cursor");
        std::fs::create_dir_all(&cursor_dir).unwrap();
        let path = cursor_dir.join("cli.json");
        std::fs::write(&path, "{ invalid").unwrap();

        let error = install_cursor_mcp_permission(dir.path()).unwrap_err();

        assert!(error.contains("refusing to overwrite"));
        assert_eq!(std::fs::read_to_string(path).unwrap(), "{ invalid");
    }

    /// Live probe — only passes when Cursor CLI is installed on the dev machine.
    #[test]
    #[ignore]
    fn find_cursor_agent_live() {
        let found = find_cursor_agent();
        eprintln!("find_cursor_agent: {found:?}");
        assert!(found.is_some(), "expected Cursor agent CLI on this machine");
    }
}
