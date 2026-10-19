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
    /// Keyed by container name (crate/package name) or its directory.
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

    /// The config for a container, looked up by name, then by directory.
    pub fn container(&self, name: &str, dir: &str) -> Option<&ContainerConfig> {
        self.containers.get(name).or_else(|| self.containers.get(dir))
    }
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

    /// A misspelt key is an error, not a silent default.
    #[test]
    fn rejects_unknown_keys() {
        assert!(Structure::parse("[system]\nnmae = \"x\"\n").is_err());
    }
}
