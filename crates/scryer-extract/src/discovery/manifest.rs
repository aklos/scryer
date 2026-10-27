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
//!
//! Discovery is two-pass: each directory's manifests become a container, then
//! dependencies declared by name rather than path (npm workspace siblings,
//! Cargo `{ workspace = true }`) resolve against the discovered set.

use crate::code_map as lang;
use scryer_core::scan;
use std::collections::{BTreeMap, BTreeSet};
use crate::code_map::Container;
use std::path::Path;

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

/// One directory's scan: the container (when the dir is a unit) plus dep
/// references that can only resolve once every container is known.
#[derive(Default)]
struct DirScan {
    dir: String,
    container: Option<Container>,
    /// npm dep names that may denote sibling workspace packages (bare semver
    /// or `workspace:` protocol) — matched against discovered names.
    npm_name_deps: Vec<String>,
    /// Cargo `{ workspace = true }` dep names, awaiting a root's table.
    cargo_ws_deps: Vec<String>,
    /// This dir's `[workspace.dependencies]` PATH entries: name -> dep dir.
    cargo_ws_table: Vec<(String, String)>,
}

/// Discover containers from a file list already collected by the caller. The
/// main extractor uses this to share one repository walk between manifest
/// discovery and source parsing.
///
/// Two passes: each manifest is read once, then dep references that resolve by
/// NAME — npm workspace siblings (`workspace:*`, or a bare version that npm
/// links to the sibling by name) and Cargo `{ workspace = true }` entries
/// (through the nearest workspace root's `[workspace.dependencies]` table) —
/// are joined against the discovered containers.
pub fn discover_containers_from_files(
    project: &Path,
    files: &[std::path::PathBuf],
) -> Vec<Container> {
    // Group every build/deploy signal by its directory.
    let mut by_dir: BTreeMap<String, Vec<Signal>> = BTreeMap::new();
    // Every file, project-relative, and the directories holding an app entry.
    let mut rels: BTreeSet<String> = BTreeSet::new();
    let mut app_dirs: BTreeSet<String> = BTreeSet::new();

    for path in files {
        let Ok(rel) = path.strip_prefix(project) else {
            continue;
        };
        let rel_text = rel.to_string_lossy().replace('\\', "/");
        let filename = rel.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if is_app_entry(filename) {
            let dir = rel_text.rsplit_once('/').map_or("", |(d, _)| d);
            if !dir.is_empty() && !scan::is_dot_path(dir) && !in_test_dir(dir) {
                app_dirs.insert(dir.to_string());
            }
        }
        rels.insert(rel_text);
        let deploy = match manifest_role(filename) {
            Some(role) => role,
            None => continue,
        };
        let dir = rel
            .parent()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        // A manifest under a dot directory (`.claude/worktrees/…`,
        // `.devcontainer/`) packages tooling, not the product.
        if scan::is_dot_path(&dir) {
            continue;
        }
        by_dir.entry(dir).or_default().push(Signal {
            filename: filename.to_string(),
            deploy,
        });
    }

    let mut scans: Vec<DirScan> = Vec::new();
    for (dir, signals) in &by_dir {
        scans.push(scan_dir(project, dir, signals));
    }

    let mut containers: Vec<Container> =
        scans.iter().filter_map(|s| s.container.clone()).collect();

    // Pass 2: name-based dep resolution.
    let name_to_dir: BTreeMap<&str, &str> = containers
        .iter()
        .map(|c| (c.name.as_str(), c.dir.as_str()))
        .collect();
    let mut extra: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for scan in &scans {
        let Some(c) = &scan.container else { continue };
        let mut add: Vec<String> = Vec::new();
        for name in &scan.npm_name_deps {
            if let Some(&dir) = name_to_dir.get(name.as_str()) {
                if dir != c.dir {
                    add.push(dir.to_string());
                }
            }
        }
        if !scan.cargo_ws_deps.is_empty() {
            // The nearest enclosing dir (or self) holding a workspace table.
            let root = scans
                .iter()
                .filter(|s| !s.cargo_ws_table.is_empty())
                .filter(|s| {
                    s.dir.is_empty()
                        || s.dir == c.dir
                        || c.dir.starts_with(&format!("{}/", s.dir))
                })
                .max_by_key(|s| s.dir.len());
            if let Some(root) = root {
                for name in &scan.cargo_ws_deps {
                    if let Some((_, dir)) =
                        root.cargo_ws_table.iter().find(|(n, _)| n == name)
                    {
                        if *dir != c.dir {
                            add.push(dir.clone());
                        }
                    }
                }
            }
        }
        if !add.is_empty() {
            extra.insert(c.dir.clone(), add);
        }
    }
    for c in &mut containers {
        if let Some(add) = extra.remove(&c.dir) {
            c.dep_dirs.extend(add);
            c.dep_dirs.sort();
            c.dep_dirs.dedup();
        }
    }

    if containers.is_empty() {
        containers.push(Container {
            dir: String::new(),
            name: basename(project).unwrap_or_else(|| "project".to_string()),
            technology: None,
            dep_dirs: Vec::new(),
            go_module: None,
            clj_paths: Vec::new(),
            packages: Vec::new(),
            entries: Vec::new(),
        });
    }

    // A directory with an app entry of its own (an `index.html`, a bundler
    // config) inside a package is an app of its own, not a part of the
    // package — when it holds source code.
    for dir in app_dirs {
        let prefix = format!("{dir}/");
        if containers.iter().any(|c| c.dir == dir)
            || !rels.iter().any(|r| r.starts_with(&prefix) && scan::is_product_code(r))
        {
            continue;
        }
        containers.push(Container {
            name: basename(Path::new(&dir)).unwrap_or_else(|| dir.clone()),
            dir,
            technology: None,
            dep_dirs: Vec::new(),
            go_module: None,
            clj_paths: Vec::new(),
            packages: Vec::new(),
            entries: Vec::new(),
        });
    }
    containers.sort_by(|a, b| a.dir.cmp(&b.dir));

    // The module scripts a unit's `index.html` loads are its entries.
    for c in &mut containers {
        let html = if c.dir.is_empty() { "index.html".to_string() } else { format!("{}/index.html", c.dir) };
        if !rels.contains(&html) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(project.join(&html)) else { continue };
        for src in html_module_scripts(&text) {
            if let Some(entry) = resolve_script(&c.dir, &src, &rels) {
                c.entries.push(entry);
            }
        }
        c.entries.sort();
        c.entries.dedup();
    }
    containers
}

/// A file that makes its directory an app: a page (`index.html`) or a
/// bundler's config (`vite.config.ts`, `webpack.config.js`,
/// `remotion.config.ts`, `next.config.mjs`).
fn is_app_entry(filename: &str) -> bool {
    filename == "index.html"
        || ["vite", "webpack", "remotion", "next"].iter().any(|tool| {
            filename
                .strip_prefix(tool)
                .and_then(|r| r.strip_prefix(".config."))
                .is_some_and(|ext| matches!(ext, "js" | "ts" | "mjs" | "cjs" | "mts" | "cts"))
        })
}

/// A directory of test code or fixtures, whose apps are test material.
fn in_test_dir(dir: &str) -> bool {
    dir.split('/').any(|d| matches!(d, "tests" | "test" | "__tests__" | "e2e" | "fixtures" | "__fixtures__" | "testdata"))
}

/// The `src` of every `<script type="module" src="…">` in a page.
fn html_module_scripts(html: &str) -> Vec<String> {
    let mut out = Vec::new();
    for tag in html.split("<script").skip(1) {
        let Some(head) = tag.split('>').next() else { continue };
        if !head.contains("module") {
            continue;
        }
        for quote in ['"', '\''] {
            if let Some(rest) = head.split(&format!("src={quote}")).nth(1) {
                if let Some(src) = rest.split(quote).next() {
                    out.push(src.to_string());
                }
            }
        }
    }
    out
}

/// The file a page's script `src` names: relative to the page, or — for a
/// root-absolute `/…` — to the page's directory or the project root,
/// whichever holds it.
fn resolve_script(dir: &str, src: &str, rels: &BTreeSet<String>) -> Option<String> {
    if src.contains("://") {
        return None;
    }
    let candidates = match src.strip_prefix('/') {
        Some(abs) => vec![resolve_rel(dir, abs), resolve_rel("", abs)],
        None => vec![resolve_rel(dir, src)],
    };
    candidates.into_iter().flatten().find(|c| rels.contains(c))
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
            | "deps.edn"
            | "project.clj"
            | "shadow-cljs.edn"
            | "bb.edn"
            | "build.boot"
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

/// Scan one directory's signals: build its container (`None` only for a
/// directory whose sole signal is a Cargo *workspace* root with no package and
/// nothing deployable) and collect the name-based dep references for pass 2.
fn scan_dir(project: &Path, dir: &str, signals: &[Signal]) -> DirScan {
    let abs_dir = if dir.is_empty() {
        project.to_path_buf()
    } else {
        project.join(dir)
    };

    let mut scan = DirScan {
        dir: dir.to_string(),
        ..Default::default()
    };
    let mut name: Option<String> = None;
    let mut dep_dirs: Vec<String> = Vec::new();
    let mut technology: Option<String> = None;
    let mut go_module: Option<String> = None;
    let mut clj_paths: Vec<String> = Vec::new();
    let mut packages: Vec<String> = Vec::new();
    let mut entries: Vec<String> = Vec::new();
    // A real unit unless the only thing we saw is a Cargo workspace root.
    let mut is_unit = false;

    for sig in signals {
        let path = abs_dir.join(&sig.filename);
        let Ok(text) = std::fs::read_to_string(&path) else {
            // Unreadable but present: still a deploy/code signal.
            is_unit = true;
            continue;
        };
        match sig.filename.as_str() {
            "Cargo.toml" => match parse_cargo(&text, dir) {
                CargoManifest::Package {
                    pkg_name,
                    names,
                    targets,
                    deps,
                    ws_deps,
                    ws_table,
                } => {
                    is_unit = true;
                    name = name.or(pkg_name);
                    dep_dirs.extend(deps);
                    packages.extend(names);
                    entries.extend(targets);
                    scan.cargo_ws_deps.extend(ws_deps);
                    scan.cargo_ws_table.extend(ws_table);
                }
                CargoManifest::WorkspaceOnly { ws_table } => {
                    scan.cargo_ws_table.extend(ws_table);
                }
                CargoManifest::Unparseable => is_unit = true,
            },
            "package.json" => {
                is_unit = true;
                let npm = parse_package_json(&text, dir);
                name = name.or(npm.name);
                dep_dirs.extend(npm.path_deps);
                packages.extend(npm.names);
                entries.extend(npm.entries);
                scan.npm_name_deps.extend(npm.name_deps);
            }
            "pyproject.toml" => {
                is_unit = true;
                name = name.or_else(|| pyproject_name(&text));
            }
            "go.mod" => {
                is_unit = true;
                go_module = go_module.or_else(|| go_mod_module(&text));
                // The declared module path IS the unit's name; display the
                // last segment ("proj" for github.com/acme/proj).
                name = name.or_else(|| {
                    go_module
                        .as_deref()
                        .and_then(|m| m.rsplit('/').next())
                        .map(|s| s.to_string())
                });
            }
            // Clojure build descriptors declare their source roots, which is
            // the only way `app.db` maps to a file path.
            "deps.edn" | "bb.edn" => {
                is_unit = true;
                clj_paths.extend(lang::edn_string_vec(&text, ":paths"));
            }
            "shadow-cljs.edn" => {
                is_unit = true;
                clj_paths.extend(lang::edn_string_vec(&text, ":source-paths"));
            }
            "project.clj" => {
                is_unit = true;
                if let Some((project, paths)) = lang::project_clj_header(&text) {
                    name = name.or(Some(project));
                    clj_paths.extend(paths);
                }
            }
            f if f.ends_with(".csproj") || f.ends_with(".fsproj") => {
                is_unit = true;
                dep_dirs.extend(msbuild_project_refs(&text, dir));
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
        // Only a Cargo workspace root lived here (its table still feeds pass 2).
        return scan;
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
    clj_paths.sort();
    clj_paths.dedup();
    packages.sort();
    packages.dedup();
    entries.sort();
    entries.dedup();
    scan.container = Some(Container {
        dir: dir.to_string(),
        name,
        technology,
        dep_dirs,
        go_module,
        clj_paths,
        packages,
        entries,
    });
    scan
}

/// The `module <path>` directive of a go.mod, quotes tolerated.
/// The directories of the projects an MSBuild project file references:
/// `<ProjectReference Include="../Shared/Shared.csproj" />` → `src/Shared`
/// for a manifest in `src/Server`.
fn msbuild_project_refs(text: &str, manifest_dir: &str) -> Vec<String> {
    let mut out = Vec::new();
    for chunk in text.split("<ProjectReference").skip(1) {
        let Some(at) = chunk.find("Include=\"") else { continue };
        let rest = &chunk[at + "Include=\"".len()..];
        let Some(end) = rest.find('"') else { continue };
        let target = rest[..end].replace('\\', "/");
        let mut parts: Vec<&str> = manifest_dir.split('/').filter(|p| !p.is_empty()).collect();
        for seg in target.split('/') {
            match seg {
                "" | "." => {}
                ".." => {
                    parts.pop();
                }
                s => parts.push(s),
            }
        }
        parts.pop(); // the referenced project file itself
        out.push(parts.join("/"));
    }
    out
}

fn go_mod_module(text: &str) -> Option<String> {
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("module") {
            if rest.starts_with([' ', '\t']) {
                let module = rest.trim().trim_matches('"');
                if !module.is_empty() {
                    return Some(module.to_string());
                }
            }
        }
    }
    None
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
        // Skip option tokens: `FROM --platform=linux/amd64 node:18`.
        let image = l[4..]
            .split_whitespace()
            .find(|t| !t.starts_with("--"))
            .unwrap_or("");
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
        /// Every dependency's name, from every dependency table.
        names: Vec<String>,
        /// The `path` of each declared `[lib]`, `[[bin]]` and `[[example]]`
        /// target, resolved.
        targets: Vec<String>,
        /// Resolved in-repo path deps.
        deps: Vec<String>,
        /// `{ workspace = true }` dep names, resolved in pass 2 through the
        /// nearest workspace root's table.
        ws_deps: Vec<String>,
        /// This manifest's own `[workspace.dependencies]` path entries
        /// (a root can be a package too).
        ws_table: Vec<(String, String)>,
    },
    WorkspaceOnly {
        ws_table: Vec<(String, String)>,
    },
    Unparseable,
}

/// Every dependency table in a Cargo manifest: the three top-level ones plus
/// each `[target.'cfg(...)'.dependencies]` family.
fn cargo_dep_tables(value: &toml::Value) -> Vec<&toml::value::Table> {
    const FAMILIES: [&str; 3] = ["dependencies", "dev-dependencies", "build-dependencies"];
    let mut out = Vec::new();
    for family in FAMILIES {
        if let Some(t) = value.get(family).and_then(|t| t.as_table()) {
            out.push(t);
        }
    }
    if let Some(targets) = value.get("target").and_then(|t| t.as_table()) {
        for target in targets.values() {
            for family in FAMILIES {
                if let Some(t) = target.get(family).and_then(|t| t.as_table()) {
                    out.push(t);
                }
            }
        }
    }
    out
}

fn parse_cargo(text: &str, manifest_dir: &str) -> CargoManifest {
    let Ok(value) = text.parse::<toml::Value>() else {
        return CargoManifest::Unparseable;
    };
    // `[workspace.dependencies]` path entries — the table `{ workspace = true }`
    // members resolve through.
    let ws_table: Vec<(String, String)> = value
        .get("workspace")
        .and_then(|w| w.get("dependencies"))
        .and_then(|d| d.as_table())
        .map(|tbl| {
            tbl.iter()
                .filter_map(|(dep_name, spec)| {
                    let path = spec.as_table()?.get("path")?.as_str()?;
                    Some((dep_name.clone(), resolve_rel(manifest_dir, path)?))
                })
                .collect()
        })
        .unwrap_or_default();

    let has_package = value.get("package").is_some();
    if !has_package {
        return if value.get("workspace").is_some() {
            CargoManifest::WorkspaceOnly { ws_table }
        } else {
            CargoManifest::Unparseable
        };
    }

    let pkg_name = value
        .get("package")
        .and_then(|p| p.get("name"))
        .and_then(|n| n.as_str())
        .map(|s| s.to_string());

    let mut names: Vec<String> = Vec::new();
    let mut deps: Vec<String> = Vec::new();
    let mut ws_deps: Vec<String> = Vec::new();
    for tbl in cargo_dep_tables(&value) {
        for (dep_name, spec) in tbl {
            names.push(dep_name.clone());
            let Some(spec) = spec.as_table() else { continue };
            if let Some(path) = spec.get("path").and_then(|p| p.as_str()) {
                if let Some(resolved) = resolve_rel(manifest_dir, path) {
                    deps.push(resolved);
                }
            } else if spec.get("workspace").and_then(|w| w.as_bool()) == Some(true) {
                ws_deps.push(dep_name.clone());
            }
        }
    }
    let mut targets: Vec<String> = Vec::new();
    let mut target_path = |t: &toml::Value| {
        if let Some(path) = t.get("path").and_then(|p| p.as_str()).and_then(|p| resolve_rel(manifest_dir, p)) {
            targets.push(path);
        }
    };
    if let Some(lib) = value.get("lib") {
        target_path(lib);
    }
    for kind in ["bin", "example"] {
        for t in value.get(kind).and_then(|b| b.as_array()).into_iter().flatten() {
            target_path(t);
        }
    }
    CargoManifest::Package {
        pkg_name,
        names,
        targets,
        deps,
        ws_deps,
        ws_table,
    }
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

#[derive(Default)]
struct NpmManifest {
    name: Option<String>,
    /// Resolved in-repo path deps (`file:`/`link:`/`workspace:./…`).
    path_deps: Vec<String>,
    /// Dep names that may denote sibling workspace packages: `workspace:*`
    /// (and friends), or a bare version — npm/yarn workspaces link those by
    /// NAME, so pass 2 matches them against discovered container names.
    name_deps: Vec<String>,
    /// Every dependency's name.
    names: Vec<String>,
    /// `main`, `module`, `exports` and `bin` targets, resolved.
    entries: Vec<String>,
}

fn parse_package_json(text: &str, manifest_dir: &str) -> NpmManifest {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
        return NpmManifest::default();
    };
    let mut out = NpmManifest {
        name: value
            .get("name")
            .and_then(|n| n.as_str())
            .map(|s| s.to_string()),
        ..Default::default()
    };
    // Every path an entry field names; `exports` and `bin` may nest maps.
    fn paths(v: &serde_json::Value, out: &mut Vec<String>) {
        match v {
            serde_json::Value::String(s) => out.push(s.clone()),
            serde_json::Value::Object(m) => m.values().for_each(|v| paths(v, out)),
            serde_json::Value::Array(a) => a.iter().for_each(|v| paths(v, out)),
            _ => {}
        }
    }
    let mut named = Vec::new();
    for field in ["main", "module", "exports", "bin"] {
        if let Some(v) = value.get(field) {
            paths(v, &mut named);
        }
    }
    out.entries = named.iter().filter(|p| !p.contains('*')).filter_map(|p| resolve_rel(manifest_dir, p)).collect();

    for table in ["dependencies", "devDependencies"] {
        let Some(obj) = value.get(table).and_then(|t| t.as_object()) else {
            continue;
        };
        for (dep_name, spec) in obj {
            out.names.push(dep_name.clone());
            let Some(s) = spec.as_str() else { continue };
            if let Some(rel) = s.strip_prefix("file:").or_else(|| s.strip_prefix("link:")) {
                // An absolute path points outside repo semantics — treating it
                // as project-relative minted a false in-repo dep dir.
                if rel.starts_with('/') || rel.contains(':') {
                    continue;
                }
                if let Some(resolved) = resolve_rel(manifest_dir, rel) {
                    out.path_deps.push(resolved);
                }
            } else if let Some(rest) = s.strip_prefix("workspace:") {
                if rest.starts_with('.') {
                    // `workspace:../ui` — an explicit relative path.
                    if let Some(resolved) = resolve_rel(manifest_dir, rest) {
                        out.path_deps.push(resolved);
                    }
                } else {
                    // `workspace:*` / `workspace:^` / `workspace:~` name the
                    // dep itself; `workspace:@acme/ui@*` aliases a sibling.
                    let name = rest
                        .rsplit_once('@')
                        .filter(|(pkg, _)| !pkg.is_empty())
                        .map(|(pkg, _)| pkg.to_string())
                        .unwrap_or_else(|| dep_name.clone());
                    out.name_deps.push(name);
                }
            } else {
                out.name_deps.push(dep_name.clone());
            }
        }
    }
    out
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

/// The description a container's manifest declares, with that manifest's
/// project-relative path: Cargo `[package] description`, package.json
/// `description`, pyproject `[project] description`. The first that has one.
pub fn manifest_description(project: &Path, dir: &str) -> Option<(String, String)> {
    let rel = |file: &str| if dir.is_empty() { file.to_string() } else { format!("{dir}/{file}") };
    let read = |file: &str| std::fs::read_to_string(project.join(rel(file))).ok();
    let toml_at = |text: &str, table: &str| {
        let v: toml::Value = toml::from_str(text).ok()?;
        v.get(table)?.get("description")?.as_str().map(str::to_string)
    };
    let found = [
        ("Cargo.toml", read("Cargo.toml").and_then(|t| toml_at(&t, "package"))),
        (
            "package.json",
            read("package.json").and_then(|t| {
                let v: serde_json::Value = serde_json::from_str(&t).ok()?;
                v.get("description")?.as_str().map(str::to_string)
            }),
        ),
        ("pyproject.toml", read("pyproject.toml").and_then(|t| toml_at(&t, "project"))),
    ];
    found
        .into_iter()
        .find_map(|(file, d)| d.map(|d| d.trim().to_string()).filter(|d| !d.is_empty()).map(|d| (rel(file), d)))
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
            CargoManifest::Package {
                pkg_name, deps, ..
            } => {
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
            CargoManifest::WorkspaceOnly { .. }
        ));
    }

    /// An MSBuild ProjectReference resolves to the referenced project's
    /// directory, relative to the project root.
    #[test]
    fn msbuild_project_references_become_dep_dirs() {
        let text = r#"<Project Sdk="Microsoft.NET.Sdk">
  <ItemGroup>
    <ProjectReference Include="../../src/Server/Server.csproj" />
    <ProjectReference Include="..\..\src\Shared\Shared.csproj" />
  </ItemGroup>
</Project>"#;
        assert_eq!(
            msbuild_project_refs(text, "tests/Server.Tests"),
            vec!["src/Server".to_string(), "src/Shared".to_string()]
        );
    }

    /// `{ workspace = true }` deps and target-specific tables are collected;
    /// the root's `[workspace.dependencies]` path entries feed the table.
    #[test]
    fn cargo_workspace_deps_and_target_tables_parsed() {
        let member = r#"
[package]
name = "member"
version = "0.1.0"

[dependencies]
shared = { workspace = true }

[target.'cfg(unix)'.dependencies]
platform = { path = "../platform" }
"#;
        match parse_cargo(member, "crates/member") {
            CargoManifest::Package { deps, ws_deps, .. } => {
                assert_eq!(deps, vec!["crates/platform"], "target tables count");
                assert_eq!(ws_deps, vec!["shared"]);
            }
            _ => panic!("expected package"),
        }

        let root = r#"
[workspace]
members = ["crates/*"]

[workspace.dependencies]
shared = { path = "crates/shared" }
serde = "1"
"#;
        match parse_cargo(root, "") {
            CargoManifest::WorkspaceOnly { ws_table } => {
                assert_eq!(
                    ws_table,
                    vec![("shared".to_string(), "crates/shared".to_string())],
                    "version-only workspace deps are external — path entries only"
                );
            }
            _ => panic!("expected workspace root"),
        }
    }

    /// npm workspace deps resolve by NAME; absolute `file:` specs are not
    /// in-repo dirs.
    #[test]
    fn package_json_workspace_and_name_deps_parsed() {
        let json = r#"{
  "name": "@acme/app",
  "dependencies": {
    "@acme/ui": "workspace:*",
    "renamed": "workspace:@acme/shared@^1.0.0",
    "local": "file:../local",
    "absolute": "file:/opt/elsewhere",
    "react": "^18.0.0"
  }
}"#;
        let npm = parse_package_json(json, "packages/app");
        assert_eq!(npm.name.as_deref(), Some("@acme/app"));
        assert_eq!(npm.path_deps, vec!["packages/local"]);
        assert!(npm.name_deps.contains(&"@acme/ui".to_string()));
        assert!(
            npm.name_deps.contains(&"@acme/shared".to_string()),
            "aliased workspace dep matches the real package name"
        );
        assert!(npm.name_deps.contains(&"react".to_string()));
        assert!(
            !npm.path_deps.iter().any(|d| d.contains("elsewhere")),
            "absolute file: specs never mint in-repo dirs"
        );
    }

    /// End-to-end pass 2: a pnpm-style workspace links `workspace:*` deps by
    /// name; a Cargo member resolves `{ workspace = true }` through the root's
    /// table; a bare-name dep on a non-sibling stays external.
    #[test]
    fn discovery_resolves_name_based_deps() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let write = |rel: &str, text: &str| {
            let path = root.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, text).unwrap();
            path
        };
        let files = vec![
            write("packages/ui/package.json", r#"{"name":"@acme/ui"}"#),
            write(
                "packages/app/package.json",
                r#"{"name":"@acme/app","dependencies":{"@acme/ui":"workspace:*","react":"^18.0.0"}}"#,
            ),
            write(
                "Cargo.toml",
                "[workspace]\nmembers = [\"crates/*\"]\n\n[workspace.dependencies]\nshared = { path = \"crates/shared\" }\n",
            ),
            write(
                "crates/shared/Cargo.toml",
                "[package]\nname = \"shared\"\nversion = \"0.1.0\"\n",
            ),
            write(
                "crates/member/Cargo.toml",
                "[package]\nname = \"member\"\nversion = \"0.1.0\"\n\n[dependencies]\nshared = { workspace = true }\n",
            ),
        ];
        let containers = discover_containers_from_files(root, &files);
        let dep_dirs = |name: &str| {
            containers
                .iter()
                .find(|c| c.name == name)
                .map(|c| c.dep_dirs.clone())
                .unwrap_or_default()
        };
        assert_eq!(
            dep_dirs("@acme/app"),
            vec!["packages/ui"],
            "workspace:* resolves by name; external react does not"
        );
        assert_eq!(
            dep_dirs("member"),
            vec!["crates/shared"],
            "workspace=true resolves through the root table"
        );
        assert!(dep_dirs("@acme/ui").is_empty());
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
        assert_eq!(manifest_role("deps.edn"), Some(false));
        assert_eq!(manifest_role("project.clj"), Some(false));
        assert_eq!(manifest_role("shadow-cljs.edn"), Some(false));
        assert_eq!(manifest_role("bb.edn"), Some(false));
        assert_eq!(manifest_role("README.md"), None);
    }

    /// A Clojure namespace maps to a file path under a DECLARED source root,
    /// so `:paths` is what makes `app.db` resolvable at all.
    #[test]
    fn clojure_source_roots_read_from_declared_manifests() {
        let deps = r#"{:paths ["src" "resources"]
 :deps {org.clojure/clojure {:mvn/version "1.11.1"}}
 :aliases {:test {:extra-paths ["test"]}}}
"#;
        assert_eq!(lang::edn_string_vec(deps, ":paths"), vec!["src", "resources"]);
        // A key that isn't there, and a nested `:extra-paths` that is NOT the
        // top-level `:paths`, both read as nothing.
        assert!(lang::edn_string_vec(deps, ":source-paths").is_empty());
        assert!(lang::edn_string_vec(deps, ":extra-paths").is_empty());

        let shadow = r#"{:source-paths ["src/main" "src/test"]
 :builds {:app {:target :browser}}}
"#;
        assert_eq!(
            lang::edn_string_vec(shadow, ":source-paths"),
            vec!["src/main", "src/test"]
        );

        // Leiningen: the group qualifier is dropped from the display name, as
        // it is for every other ecosystem.
        let lein = r#"(defproject acme/my-app "0.1.0-SNAPSHOT"
  :description "A thing"
  :dependencies [[org.clojure/clojure "1.11.1"]]
  :source-paths ["src/clj"]
  :test-paths ["test/clj"])
"#;
        assert_eq!(
            lang::project_clj_header(lein),
            Some((
                "my-app".to_string(),
                vec!["src/clj".to_string(), "test/clj".to_string()]
            ))
        );
        assert_eq!(lang::project_clj_header("(ns not-a-project)"), None);
    }

    /// A `deps.edn` makes its directory a unit and hands the resolver its
    /// source roots.
    #[test]
    fn clojure_container_discovered_with_paths() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let write = |rel: &str, text: &str| {
            let path = root.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, text).unwrap();
            path
        };
        let files = vec![
            write("svc/deps.edn", r#"{:paths ["src" "test"]}"#),
            write("svc/src/app/core.clj", "(ns app.core)\n"),
        ];
        let containers = discover_containers_from_files(root, &files);
        let svc = containers.iter().find(|c| c.dir == "svc").expect("svc unit");
        assert_eq!(svc.name, "svc");
        assert_eq!(svc.clj_paths, vec!["src", "test"]);
    }

    #[test]
    fn go_mod_module_parsed() {
        assert_eq!(
            go_mod_module("module github.com/acme/proj\n\ngo 1.22\n").as_deref(),
            Some("github.com/acme/proj")
        );
        assert_eq!(
            go_mod_module("// a comment\nmodule \"example.com/x\"\n").as_deref(),
            Some("example.com/x")
        );
        assert_eq!(go_mod_module("go 1.22\n"), None);
        // `module` must be a directive, not a prefix of another word.
        assert_eq!(go_mod_module("modules-thing foo\n"), None);
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

    /// A manifest-bearing directory yields its declared name, technology
    /// (the Dockerfile base image), and path dependencies on one container.
    #[test]
    fn a_scanned_dir_carries_name_technology_and_path_deps() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let write = |rel: &str, text: &str| {
            let path = root.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, text).unwrap();
            path
        };
        let files = vec![
            write(
                "api/package.json",
                r#"{"name":"@acme/api","dependencies":{"lib":"file:../lib"}}"#,
            ),
            write("api/Dockerfile", "FROM node:20-alpine\nCOPY . .\n"),
            write("lib/package.json", r#"{"name":"@acme/lib"}"#),
        ];
        let containers = discover_containers_from_files(root, &files);
        let api = containers.iter().find(|c| c.name == "@acme/api").unwrap();
        assert_eq!(api.dir, "api");
        assert_eq!(api.technology.as_deref(), Some("node:20-alpine"));
        assert_eq!(api.dep_dirs, vec!["lib"]);
    }
}
