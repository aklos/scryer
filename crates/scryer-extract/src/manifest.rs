//! Container discovery from declared build/deploy units.
//!
//! A container is a directory the project *declares* as a build or deploy unit:
//! it holds a code manifest (package.json, Cargo `[package]`, pyproject,
//! requirements.txt, go.mod, …) and/or a deploy manifest (Dockerfile, fly.toml,
//! Procfile, …). A directory with its own Dockerfile is a deployable image by
//! definition; that's a fact, not a guess. Names and intra-repo dependency
//! edges come from declared values; `technology` is taken only from literal
//! declared strings (a Dockerfile's `FROM <image>`), never a filename→ecosystem
//! lookup table.

use scryer_core::scan;
use std::collections::BTreeMap;
use std::path::Path;

/// A declared build/deploy unit.
#[derive(Debug, Clone)]
pub struct Container {
    /// Directory relative to the project root, normalized with `/` separators.
    /// Empty string for the project root.
    pub dir: String,
    /// Declared name (crate/package name) or the directory basename.
    pub name: String,
    /// Literal declared technology — currently a Dockerfile base image. `None`
    /// when nothing is declared (Pass 2 names what the unit is).
    pub technology: Option<String>,
    /// Directories of other containers this one declares a path dependency on.
    pub dep_dirs: Vec<String>,
}

/// One manifest file found in a candidate directory.
struct Signal {
    filename: String,
    deploy: bool, // deploy manifest (vs code manifest)
}

/// Discover the project's containers. Always returns at least one container
/// (a root fallback) so the C4 hierarchy below it is well-formed.
pub fn discover_containers(project: &Path) -> Vec<Container> {
    let walker = ignore::WalkBuilder::new(project)
        .hidden(false)
        .filter_entry(|entry| {
            if entry.file_type().is_some_and(|ft| ft.is_dir()) {
                let name = entry.file_name().to_string_lossy();
                if scan::SKIP_DIRS.iter().any(|&s| name == s)
                    || scan::SKIP_BUILD_DIRS.iter().any(|&s| name == s)
                {
                    return false;
                }
            }
            true
        })
        .build();
    let files: Vec<std::path::PathBuf> = walker
        .flatten()
        .filter(|entry| entry.file_type().is_some_and(|ft| ft.is_file()))
        .map(|entry| entry.into_path())
        .collect();
    discover_containers_from_files(project, &files)
}

/// Discover containers from a file list already collected by the caller. The
/// main extractor uses this to share one repository walk between manifest
/// discovery and source parsing.
pub fn discover_containers_from_files(
    project: &Path,
    files: &[std::path::PathBuf],
) -> Vec<Container> {
    // Group every build/deploy signal by its directory.
    let mut by_dir: BTreeMap<String, Vec<Signal>> = BTreeMap::new();

    for path in files {
        let Ok(rel) = path.strip_prefix(project) else {
            continue;
        };
        let filename = rel.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let deploy = match manifest_role(filename) {
            Some(role) => role,
            None => continue,
        };
        let dir = rel
            .parent()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        by_dir.entry(dir).or_default().push(Signal {
            filename: filename.to_string(),
            deploy,
        });
    }

    let mut containers: Vec<Container> = Vec::new();
    for (dir, signals) in &by_dir {
        if let Some(c) = container_for_dir(project, dir, signals) {
            containers.push(c);
        }
    }

    if containers.is_empty() {
        containers.push(Container {
            dir: String::new(),
            name: basename(project).unwrap_or_else(|| "project".to_string()),
            technology: None,
            dep_dirs: Vec::new(),
        });
    }
    containers
}

/// `Some(true)` = deploy manifest, `Some(false)` = code manifest, `None` = not a
/// build/deploy signal.
fn manifest_role(filename: &str) -> Option<bool> {
    // Code manifests (a unit's dependency/build descriptor).
    let code = matches!(
        filename,
        "package.json"
            | "Cargo.toml"
            | "go.mod"
            | "pyproject.toml"
            | "setup.py"
            | "setup.cfg"
            | "Pipfile"
            | "pom.xml"
            | "build.gradle"
            | "build.gradle.kts"
            | "Gemfile"
            | "composer.json"
            | "mix.exs"
            | "pubspec.yaml"
            | "Package.swift"
            | "deno.json"
            | "deno.jsonc"
    ) || filename == "requirements.txt"
        || (filename.starts_with("requirements") && filename.ends_with(".txt"))
        || filename.ends_with(".csproj")
        || filename.ends_with(".fsproj");
    if code {
        return Some(false);
    }
    // Deploy manifests (a unit's packaging/runtime descriptor). A directory
    // holding one is a deployable unit even with no code manifest (e.g. a DB).
    let deploy = matches!(
        filename,
        "fly.toml" | "Procfile" | "vercel.json" | "netlify.toml" | "render.yaml" | "railway.json"
    ) || filename.starts_with("Dockerfile")
        || filename == "serverless.yml"
        || filename == "serverless.yaml";
    if deploy {
        return Some(true);
    }
    None
}

/// Build a container for one directory from the signals it holds. Returns `None`
/// only for a directory whose sole signal is a Cargo *workspace* root with no
/// package and nothing deployable.
fn container_for_dir(project: &Path, dir: &str, signals: &[Signal]) -> Option<Container> {
    let abs_dir = if dir.is_empty() {
        project.to_path_buf()
    } else {
        project.join(dir)
    };

    let mut name: Option<String> = None;
    let mut dep_dirs: Vec<String> = Vec::new();
    let mut technology: Option<String> = None;
    // A real unit unless the only thing we saw is a Cargo workspace root.
    let mut is_unit = false;
    let mut saw_cargo_workspace_only = false;

    for sig in signals {
        let path = abs_dir.join(&sig.filename);
        let Ok(text) = std::fs::read_to_string(&path) else {
            // Unreadable but present: still a deploy/code signal.
            is_unit = true;
            continue;
        };
        match sig.filename.as_str() {
            "Cargo.toml" => match parse_cargo(&text, dir) {
                CargoManifest::Package { pkg_name, deps } => {
                    is_unit = true;
                    name = name.or(pkg_name);
                    dep_dirs.extend(deps);
                }
                CargoManifest::WorkspaceOnly => saw_cargo_workspace_only = true,
                CargoManifest::Unparseable => is_unit = true,
            },
            "package.json" => {
                is_unit = true;
                let (pkg_name, deps) = parse_package_json(&text, dir);
                name = name.or(pkg_name);
                dep_dirs.extend(deps);
            }
            "pyproject.toml" => {
                is_unit = true;
                name = name.or_else(|| pyproject_name(&text));
            }
            _ => {
                is_unit = true;
                if sig.deploy && technology.is_none() && sig.filename.starts_with("Dockerfile") {
                    technology = dockerfile_base_image(&text);
                }
            }
        }
    }

    if !is_unit {
        // Only a Cargo workspace root lived here, with nothing deployable.
        let _ = saw_cargo_workspace_only;
        return None;
    }

    let name = name
        .or_else(|| {
            if dir.is_empty() {
                basename(project)
            } else {
                basename(Path::new(dir))
            }
        })
        .unwrap_or_else(|| dir.to_string());

    dep_dirs.sort();
    dep_dirs.dedup();
    Some(Container {
        dir: dir.to_string(),
        name,
        technology,
        dep_dirs,
    })
}

/// The base image of the final build stage: the last `FROM <image>` line.
/// Skipped if it interpolates a variable (`FROM ${BASE}`) — not a literal fact.
fn dockerfile_base_image(text: &str) -> Option<String> {
    let mut last: Option<String> = None;
    for line in text.lines() {
        let l = line.trim();
        if l.len() < 5 || !l.get(..4).is_some_and(|p| p.eq_ignore_ascii_case("from")) {
            continue;
        }
        if !l.as_bytes()[4].is_ascii_whitespace() {
            continue;
        }
        let image = l[4..].trim().split_whitespace().next().unwrap_or("");
        if image.is_empty() || image.contains('$') {
            continue;
        }
        last = Some(image.to_string());
    }
    last
}

enum CargoManifest {
    Package {
        pkg_name: Option<String>,
        deps: Vec<String>,
    },
    WorkspaceOnly,
    Unparseable,
}

fn parse_cargo(text: &str, manifest_dir: &str) -> CargoManifest {
    let Ok(value) = text.parse::<toml::Value>() else {
        return CargoManifest::Unparseable;
    };
    let has_package = value.get("package").is_some();
    let has_workspace = value.get("workspace").is_some();
    if !has_package && has_workspace {
        return CargoManifest::WorkspaceOnly;
    }
    if !has_package {
        return CargoManifest::Unparseable;
    }

    let pkg_name = value
        .get("package")
        .and_then(|p| p.get("name"))
        .and_then(|n| n.as_str())
        .map(|s| s.to_string());

    let mut deps: Vec<String> = Vec::new();
    for table in ["dependencies", "dev-dependencies", "build-dependencies"] {
        let Some(tbl) = value.get(table).and_then(|t| t.as_table()) else {
            continue;
        };
        for spec in tbl.values() {
            if let Some(path) = spec
                .as_table()
                .and_then(|t| t.get("path"))
                .and_then(|p| p.as_str())
            {
                if let Some(resolved) = resolve_rel(manifest_dir, path) {
                    deps.push(resolved);
                }
            }
        }
    }
    CargoManifest::Package { pkg_name, deps }
}

/// The declared distribution name of a pyproject: PEP 621 `[project] name`,
/// falling back to classic Poetry's `[tool.poetry] name`. This is what a
/// cross-package `import` spells (modulo `-` -> `_`), so it feeds the
/// package-name map exactly like a crate/npm name.
fn pyproject_name(text: &str) -> Option<String> {
    let value = text.parse::<toml::Value>().ok()?;
    value
        .get("project")
        .and_then(|p| p.get("name"))
        .or_else(|| {
            value
                .get("tool")
                .and_then(|t| t.get("poetry"))
                .and_then(|p| p.get("name"))
        })
        .and_then(|n| n.as_str())
        .map(|s| s.to_string())
}

fn parse_package_json(text: &str, manifest_dir: &str) -> (Option<String>, Vec<String>) {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
        return (None, Vec::new());
    };
    let name = value
        .get("name")
        .and_then(|n| n.as_str())
        .map(|s| s.to_string());

    let mut deps: Vec<String> = Vec::new();
    for table in ["dependencies", "devDependencies"] {
        let Some(obj) = value.get(table).and_then(|t| t.as_object()) else {
            continue;
        };
        for spec in obj.values() {
            if let Some(s) = spec.as_str() {
                let rel = s.strip_prefix("file:").or_else(|| s.strip_prefix("link:"));
                if let Some(rel) = rel {
                    if let Some(resolved) = resolve_rel(manifest_dir, rel) {
                        deps.push(resolved);
                    }
                }
            }
        }
    }
    (name, deps)
}

/// Resolve `rel` against `base_dir` (both project-relative, `/`-separated),
/// collapsing `.`/`..`. Returns a normalized project-relative string, `None`
/// when `..` escapes the project root. Shared with tsconfig alias resolution.
pub(crate) fn resolve_rel(base_dir: &str, rel: &str) -> Option<String> {
    let mut parts: Vec<&str> = if base_dir.is_empty() {
        Vec::new()
    } else {
        base_dir.split('/').collect()
    };
    for comp in rel.split('/') {
        match comp {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            other => parts.push(other),
        }
    }
    Some(parts.join("/"))
}

fn basename(path: &Path) -> Option<String> {
    path.file_name().map(|n| n.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_relative_paths() {
        assert_eq!(
            resolve_rel("src-tauri", "../crates/scryer-core").unwrap(),
            "crates/scryer-core"
        );
        assert_eq!(resolve_rel("a/b", "./c").unwrap(), "a/b/c");
        assert_eq!(resolve_rel("", "crates/x").unwrap(), "crates/x");
    }

    #[test]
    fn cargo_package_with_path_dep() {
        let toml = r#"
[package]
name = "scryer"
version = "0.1.0"

[dependencies]
scryer-core = { path = "../crates/scryer-core" }
serde = "1"
"#;
        match parse_cargo(toml, "src-tauri") {
            CargoManifest::Package { pkg_name, deps } => {
                assert_eq!(pkg_name.as_deref(), Some("scryer"));
                assert_eq!(deps, vec!["crates/scryer-core"]);
            }
            _ => panic!("expected package"),
        }
    }

    #[test]
    fn cargo_workspace_root_is_not_a_container() {
        let toml = "[workspace]\nmembers = [\"a\"]\nresolver = \"2\"\n";
        assert!(matches!(
            parse_cargo(toml, ""),
            CargoManifest::WorkspaceOnly
        ));
    }

    #[test]
    fn deploy_and_code_manifests_recognized() {
        assert_eq!(manifest_role("Dockerfile"), Some(true));
        assert_eq!(manifest_role("Dockerfile.builder"), Some(true));
        assert_eq!(manifest_role("fly.toml"), Some(true));
        assert_eq!(manifest_role("requirements.txt"), Some(false));
        assert_eq!(manifest_role("requirements-dev.txt"), Some(false));
        assert_eq!(manifest_role("pyproject.toml"), Some(false));
        assert_eq!(manifest_role("package.json"), Some(false));
        assert_eq!(manifest_role("README.md"), None);
    }

    #[test]
    fn pyproject_names_parsed() {
        assert_eq!(
            pyproject_name("[project]\nname = \"acme-lib\"\nversion = \"1.0\"\n").as_deref(),
            Some("acme-lib")
        );
        assert_eq!(
            pyproject_name("[tool.poetry]\nname = \"acme-poetry\"\n").as_deref(),
            Some("acme-poetry")
        );
        assert_eq!(pyproject_name("[build-system]\nrequires = []\n"), None);
    }

    #[test]
    fn dockerfile_base_image_takes_last_from() {
        let df = "FROM node:18 AS build\nRUN pnpm build\nFROM node:18-alpine\nCOPY --from=build /app .\n";
        assert_eq!(dockerfile_base_image(df).as_deref(), Some("node:18-alpine"));
        let mongo = "FROM mongo:7\nCOPY mongod.conf /etc/\n";
        assert_eq!(dockerfile_base_image(mongo).as_deref(), Some("mongo:7"));
        let interp = "ARG BASE\nFROM ${BASE}\n";
        assert_eq!(dockerfile_base_image(interp), None);
    }
}
