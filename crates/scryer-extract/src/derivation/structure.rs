//! `.scryer/structure.toml`: the little the code cannot say about its own
//! shape — the system's name, its persons and externals and how they relate,
//! which test globs are e2e or integration, and each container's display
//! name, style, layers and the externals it talks to. Everything in it is
//! optional; an absent file means sensible defaults.

use glob::{MatchOptions, Pattern};
use serde::Deserialize;
use crate::code_map::Container;
use std::collections::{BTreeMap, BTreeSet};

pub use scryer_core::derived_tree::ClaimLevel;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Structure {
    /// `"all"` puts e2e tests in the test radius too; by default it holds
    /// unit and integration tests only.
    pub radius: Option<String>,
    #[serde(default)]
    pub system: SystemConfig,
    #[serde(default)]
    pub levels: LevelsConfig,
    /// Keyed by container directory or name (crate/package name).
    #[serde(default)]
    pub containers: BTreeMap<String, ContainerConfig>,
    /// Relationships, keyed by the source's name: the system, a person, an
    /// external, or a container `[containers]` declares (by key or display
    /// name), which places the relationship on that container and lifts it to
    /// the system for the context. Declared, not read off the code.
    #[serde(default)]
    pub relationships: BTreeMap<String, Vec<Relation>>,
}

/// One relationship's target: a bare name, or `{ to, label }`.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum Relation {
    To(String),
    Labelled(LabelledRelation),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LabelledRelation {
    pub to: String,
    pub label: Option<String>,
}

impl Relation {
    pub fn to(&self) -> &str {
        match self {
            Relation::To(to) => to,
            Relation::Labelled(r) => &r.to,
        }
    }

    pub fn label(&self) -> Option<&str> {
        match self {
            Relation::To(_) => None,
            Relation::Labelled(r) => r.label.as_deref(),
        }
    }
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
    /// The externals this container is the one to talk to.
    #[serde(default)]
    pub uses: Vec<Relation>,
    /// Dev tooling (a demo, a harness, a viewer): on the map, but no product.
    /// It needs no style, and it may import any layer of what it builds on;
    /// its own cycles still count. A declaration, never detected.
    #[serde(default)]
    pub tooling: bool,
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
        let s: Structure = toml::from_str(text).map_err(|e| format!("structure.toml: {e}"))?;
        s.check_names()?;
        Ok(s)
    }

    /// Every name a relationship or a `uses` gives must be declared: under
    /// `[system]`, or — for a relationship — as a container under
    /// `[containers]`, the way a misspelt key is an error.
    fn check_names(&self) -> Result<(), String> {
        let known = |name: &str| {
            self.system.name.as_deref() == Some(name)
                || self.system.persons.iter().any(|p| p == name)
                || self.system.externals.iter().any(|e| e == name)
                || self.names_container(name)
        };
        let unknown = |name: &str| {
            format!(
                "structure.toml: [relationships] names \"{name}\", which is not the system's name, a person or an external under [system], nor a container under [containers]"
            )
        };
        for (src, targets) in &self.relationships {
            if !known(src) {
                return Err(unknown(src));
            }
            for t in targets {
                if !known(t.to()) {
                    return Err(unknown(t.to()));
                }
                if t.to() == src {
                    return Err(format!("structure.toml: [relationships] relates \"{src}\" to itself"));
                }
            }
        }
        for (key, c) in &self.containers {
            if let Some(t) = c.uses.iter().find(|t| !self.system.externals.iter().any(|e| e == t.to())) {
                return Err(format!(
                    "structure.toml: [containers.{key}] uses \"{}\", which is not an external under [system]",
                    t.to()
                ));
            }
        }
        Ok(())
    }

    /// Whether `name` is a container `[containers]` declares, by its key
    /// (directory or package name) or its display name.
    pub fn names_container(&self, name: &str) -> bool {
        self.containers.iter().any(|(key, c)| key == name || c.name.as_deref() == Some(name))
    }

    /// The declared relationships as (source, target, label). An end may be
    /// the system, a person, an external or a declared container.
    pub fn relationships(&self) -> impl Iterator<Item = (&str, &str, Option<&str>)> {
        self.relationships
            .iter()
            .flat_map(|(src, targets)| targets.iter().map(move |t| (src.as_str(), t.to(), t.label())))
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

    /// Whether the test radius includes e2e tests (`radius = "all"`).
    pub fn radius_includes_e2e(&self) -> bool {
        self.radius.as_deref() == Some("all")
    }

    /// Make a container of every directory `[containers."<dir>"]` declares
    /// that no manifest made one: a key naming no discovered container's
    /// directory or package, under which the project has files. Named after
    /// the directory; the section's `name` still applies.
    pub fn promote_declared_dirs(&self, containers: &mut Vec<Container>, files: &BTreeSet<String>) {
        for key in self.containers.keys() {
            let prefix = format!("{key}/");
            if key.is_empty()
                || containers.iter().any(|c| &c.dir == key || &c.name == key)
                || !files.iter().any(|f| f.starts_with(&prefix))
            {
                continue;
            }
            containers.push(Container {
                dir: key.clone(),
                name: key.rsplit('/').next().unwrap_or(key).to_string(),
                technology: None,
                dep_dirs: Vec::new(),
                go_module: None,
                clj_paths: Vec::new(),
                packages: Vec::new(),
                entries: Vec::new(),
            });
        }
        containers.sort_by(|a, b| a.dir.cmp(&b.dir));
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
    set_container_field(text, dir, "name", name)
}

/// `structure.toml` text with one string field of one container set, the
/// way [`set_container_name`] sets the name.
pub fn set_container_field(text: &str, dir: &str, key: &str, value: &str) -> String {
    let quoted_name = format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""));
    let headers = [format!("[containers.\"{dir}\"]"), format!("[containers.{dir}]")];
    let lines: Vec<&str> = text.lines().collect();
    let Some(start) = lines.iter().position(|l| headers.iter().any(|h| l.trim() == h)) else {
        let sep = if text.is_empty() || text.ends_with("\n\n") { "" } else if text.ends_with('\n') { "\n" } else { "\n\n" };
        return format!("{text}{sep}[containers.\"{dir}\"]\n{key} = {quoted_name}\n");
    };
    let end = lines[start + 1..]
        .iter()
        .position(|l| l.trim_start().starts_with('['))
        .map_or(lines.len(), |i| start + 1 + i);
    let mut out: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
    let is_key = |l: &str| l.split('=').next().is_some_and(|k| k.trim() == key) && l.contains('=');
    match (start + 1..end).find(|&i| is_key(lines[i])) {
        Some(i) => out[i] = format!("{key} = {quoted_name}"),
        None => out.insert(start + 1, format!("{key} = {quoted_name}")),
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
radius = "all"

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
        assert!(s.radius_includes_e2e());
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
        assert!(!s.radius_includes_e2e());
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

    const SYSTEM: &str = "[system]\nname = \"Pay\"\npersons = [\"merchant\", \"cardholder\"]\nexternals = [\"bank\", \"db\"]\n";

    /// When relationships name the system, persons and externals, bare and
    /// labelled targets both parse, in declaration order per source.
    #[test]
    fn parses_relationships() {
        let s = Structure::parse(&format!(
            "{SYSTEM}\n[relationships]\ncardholder = [\"Pay\"]\nmerchant = [{{ to = \"Pay\", label = \"integrates\" }}]\n\"Pay\" = [\"bank\", \"db\"]\n"
        ))
        .unwrap();
        let rels: Vec<_> = s.relationships().collect();
        assert_eq!(
            rels,
            vec![
                ("Pay", "bank", None),
                ("Pay", "db", None),
                ("cardholder", "Pay", None),
                ("merchant", "Pay", Some("integrates")),
            ]
        );
    }

    /// When a relationship or a container's uses names something not declared
    /// under [system], or a target has a misspelt key, parsing fails.
    #[test]
    fn rejects_unknown_relationship_names() {
        for bad in [
            "[relationships]\nmerchant = [\"Payy\"]\n",
            "[relationships]\nshopper = [\"Pay\"]\n",
            "[relationships]\nmerchant = [{ too = \"Pay\" }]\n",
            "[relationships]\nbank = [\"bank\"]\n",
            "[containers.ledger]\nuses = [\"merchant\"]\n",
        ] {
            let err = Structure::parse(&format!("{SYSTEM}\n{bad}")).err();
            assert!(err.is_some(), "accepted: {bad}");
        }
        let err = Structure::parse(&format!("{SYSTEM}\n[relationships]\nmerchant = [\"Payy\"]\n")).unwrap_err();
        assert!(err.contains("\"Payy\""), "{err}");
    }

    /// When a relationship names a container declared under [containers], by its key or its display name, it parses.
    #[test]
    fn a_relationship_may_name_a_declared_container() {
        let s = Structure::parse(&format!(
            "{SYSTEM}\n[relationships]\nmerchant = [{{ to = \"Checkout\", label = \"pays in\" }}]\nledger = [\"bank\"]\n\n[containers.web]\nname = \"Checkout\"\n\n[containers.ledger]\n"
        ))
        .unwrap();
        let rels: Vec<_> = s.relationships().collect();
        assert_eq!(rels, vec![("ledger", "bank", None), ("merchant", "Checkout", Some("pays in"))]);
        assert!(Structure::parse(&format!("{SYSTEM}\n[relationships]\nmerchant = [\"web\"]\n")).is_err());
    }

    /// When a container says which externals it uses, they parse with their
    /// labels.
    #[test]
    fn parses_a_containers_uses() {
        let s = Structure::parse(&format!(
            "{SYSTEM}\n[containers.ledger]\nuses = [\"db\", {{ to = \"bank\", label = \"settles with\" }}]\n"
        ))
        .unwrap();
        let uses = &s.container("ledger", "crates/ledger").unwrap().uses;
        assert_eq!(uses.iter().map(|u| (u.to(), u.label())).collect::<Vec<_>>(), vec![("db", None), ("bank", Some("settles with"))]);
    }
}
