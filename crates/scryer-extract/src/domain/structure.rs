//! `.scryer/structure.toml`: the little the code cannot say about its own
//! shape — the system's name, its persons and externals, which test globs are
//! e2e or integration, and each container's display name, style and layers.
//! Everything in it is optional; an absent file means sensible defaults.

use glob::{MatchOptions, Pattern};
use serde::Deserialize;
use std::collections::BTreeMap;

pub use scryer_core::derived_tree::ClaimLevel;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Structure {
    #[serde(default)]
    pub system: SystemConfig,
    #[serde(default)]
    pub levels: LevelsConfig,
    /// Keyed by container directory or name (crate/package name).
    #[serde(default)]
    pub containers: BTreeMap<String, ContainerConfig>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SystemConfig {
    pub name: Option<String>,
    #[serde(default)]
    pub persons: Vec<String>,
    #[serde(default)]
    pub externals: Vec<String>,
}

/// Test-path globs per level; a key left out keeps its default, and a test
/// matching neither is a unit test.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LevelsConfig {
    pub e2e: Option<Vec<String>>,
    pub integration: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContainerConfig {
    /// Display name in place of the package name.
    pub name: Option<String>,
    pub style: Option<String>,
    #[serde(default)]
    pub layers: Vec<String>,
}

const DEFAULT_E2E: &[&str] = &["**/e2e/**", "**/*.e2e.*", "**/*.e2e-spec.*"];
const DEFAULT_INTEGRATION: &[&str] = &["**/integration/**", "**/*.integration.*"];

const GLOB_OPTS: MatchOptions = MatchOptions {
    case_sensitive: true,
    require_literal_separator: true,
    require_literal_leading_dot: false,
};

impl Structure {
    pub fn parse(text: &str) -> Result<Self, String> {
        toml::from_str(text).map_err(|e| format!("structure.toml: {e}"))
    }

    /// The configured level a test file sits at, if a glob places it.
    /// `None` means no glob matched — unit, unless a language convention
    /// (Cargo's `tests/` directory) says otherwise.
    pub fn configured_level(&self, rel_path: &str) -> Option<ClaimLevel> {
        let matches = |configured: &Option<Vec<String>>, defaults: &[&str]| match configured {
            Some(globs) => globs.iter().any(|g| glob_match(g, rel_path)),
            None => defaults.iter().any(|g| glob_match(g, rel_path)),
        };
        if matches(&self.levels.e2e, DEFAULT_E2E) {
            Some(ClaimLevel::System)
        } else if matches(&self.levels.integration, DEFAULT_INTEGRATION) {
            Some(ClaimLevel::Container)
        } else {
            None
        }
    }

    /// Whether `[levels]` names integration globs itself, replacing the
    /// defaults and the Cargo convention.
    pub fn integration_configured(&self) -> bool {
        self.levels.integration.is_some()
    }

    /// The config for a container, looked up by directory, then by name
    /// (two packages may share a name; a directory is unambiguous).
    pub fn container(&self, name: &str, dir: &str) -> Option<&ContainerConfig> {
        self.containers.get(dir).or_else(|| self.containers.get(name))
    }
}

/// `structure.toml` text with one container's display name set, comments and
/// everything else left as written. The section is keyed by directory and
/// added when missing.
pub fn set_container_name(text: &str, dir: &str, name: &str) -> String {
    let quoted_name = format!("\"{}\"", name.replace('\\', "\\\\").replace('"', "\\\""));
    let headers = [format!("[containers.\"{dir}\"]"), format!("[containers.{dir}]")];
    let lines: Vec<&str> = text.lines().collect();
    let Some(start) = lines.iter().position(|l| headers.iter().any(|h| l.trim() == h)) else {
        let sep = if text.is_empty() || text.ends_with("\n\n") { "" } else if text.ends_with('\n') { "\n" } else { "\n\n" };
        return format!("{text}{sep}[containers.\"{dir}\"]\nname = {quoted_name}\n");
    };
    let end = lines[start + 1..]
        .iter()
        .position(|l| l.trim_start().starts_with('['))
        .map_or(lines.len(), |i| start + 1 + i);
    let mut out: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
    match (start + 1..end).find(|&i| lines[i].trim_start().starts_with("name") && lines[i].contains('=')) {
        Some(i) => out[i] = format!("name = {quoted_name}"),
        None => out.insert(start + 1, format!("name = {quoted_name}")),
    }
    let mut joined = out.join("\n");
    if text.ends_with('\n') {
        joined.push('\n');
    }
    joined
}

fn glob_match(glob: &str, path: &str) -> bool {
    Pattern::new(glob).is_ok_and(|p| p.matches_with(path, GLOB_OPTS))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// When structure.toml is written as the plan shows, every section parses.
    #[test]
    fn parses_the_documented_shape() {
        let s = Structure::parse(
            r#"
[system]
name = "scryer"
persons = ["developer", "agent"]
externals = ["claude code", "codex", "copilot cli"]

[levels]
e2e = ["tests/e2e/**"]
integration = ["crates/*/tests/**", "tests/**"]

[containers.scryer-core]
style = "layered"
layers = ["domain", "application", "composition", "infrastructure"]
"#,
        )
        .unwrap();
        assert_eq!(s.system.name.as_deref(), Some("scryer"));
        assert_eq!(s.system.persons, vec!["developer", "agent"]);
        assert_eq!(s.system.externals.len(), 3);
        let core = s.container("scryer-core", "crates/scryer-core").unwrap();
        assert_eq!(core.style.as_deref(), Some("layered"));
        assert_eq!(core.layers.len(), 4);
        assert_eq!(s.configured_level("tests/e2e/app.spec.ts"), Some(ClaimLevel::System));
        assert_eq!(s.configured_level("crates/scryer-core/tests/fold.rs"), Some(ClaimLevel::Container));
        assert_eq!(s.configured_level("tests/smoke.rs"), Some(ClaimLevel::Container));
        assert_eq!(s.configured_level("crates/scryer-core/src/model.rs"), None);
        assert!(s.integration_configured());
    }

    /// When the file is absent, e2e and integration directories still sort
    /// by name and everything else is unit.
    #[test]
    fn defaults_without_a_file() {
        let s = Structure::default();
        assert_eq!(s.configured_level("web/e2e/login.spec.ts"), Some(ClaimLevel::System));
        assert_eq!(s.configured_level("src/api.integration.test.ts"), Some(ClaimLevel::Container));
        assert_eq!(s.configured_level("tests/Server.Tests/ChatTests.cs"), None);
        assert!(!s.integration_configured());
    }

    /// When a container is renamed, its section's name changes in place, a
    /// missing name or section is added, and comments survive.
    #[test]
    fn renames_a_container_in_place() {
        let text = "# shape\n[containers.\"crates/core\"]\nname = \"Core\"\nstyle = \"library\"\n\n[containers.web]\nstyle = \"feature-sliced\"\n";
        let out = set_container_name(text, "crates/core", "Model \"Core\"");
        assert!(out.starts_with("# shape\n[containers.\"crates/core\"]\nname = \"Model \\\"Core\\\"\"\nstyle"), "{out}");
        let out = set_container_name(&out, "web", "Web UI");
        assert!(out.contains("[containers.web]\nname = \"Web UI\"\nstyle = \"feature-sliced\""), "{out}");
        let out = set_container_name(&out, "docs", "Docs");
        let s = Structure::parse(&out).unwrap();
        assert_eq!(s.container("x", "crates/core").unwrap().name.as_deref(), Some("Model \"Core\""));
        assert_eq!(s.container("x", "web").unwrap().name.as_deref(), Some("Web UI"));
        assert_eq!(s.container("x", "docs").unwrap().name.as_deref(), Some("Docs"));
        assert_eq!(s.container("x", "web").unwrap().style.as_deref(), Some("feature-sliced"));
    }

    /// A misspelt key is an error, not a silent default.
    #[test]
    fn rejects_unknown_keys() {
        assert!(Structure::parse("[system]\nnmae = \"x\"\n").is_err());
    }
}
