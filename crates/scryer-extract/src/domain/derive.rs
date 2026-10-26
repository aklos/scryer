//! The C4 tree, derived. Containers are the packages the manifests declare,
//! components the modules or directories inside them, symbols the extracted
//! definitions, links the import graph between them. Claims are the lifted
//! test descriptions, placed by test level: e2e under the system, integration
//! under its container, unit under its component; a symbol records the claims
//! whose tests name it. Nothing here is authored except the little
//! `structure.toml` adds: the persons and externals, and the relationships
//! declared between them, the system and its containers.

use crate::domain::claims::FileLift;
use crate::domain::context::{ContainerFacts, Edge, ExternalImport, FileContext};
use crate::domain::lang::Member;
use crate::domain::structure::{ClaimLevel, Structure};
use scryer_core::derived_tree::{DerivedClaim, DerivedTree};
use scryer_core::style::Styles;
use scryer_core::{
    Kind, Link, LinkKind, Node, Responsibility, SchemaProperty, ScryModel, Source, SourceLocation,
};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

/// Everything the tree is derived from.
pub struct DeriveInput<'a> {
    pub project_name: &'a str,
    pub containers: &'a [ContainerFacts],
    /// Parsed product files with their symbols.
    pub files: &'a [FileContext],
    pub file_edges: &'a [Edge],
    /// Imports of packages outside the repository, for the IPC a frontend
    /// talks to its host through (`@tauri-apps/api`).
    pub external_imports: &'a [ExternalImport],
    /// Every product source file's lifted claims and module doc.
    pub lifts: &'a BTreeMap<String, FileLift>,
    /// Every file in the project, for manifest conventions.
    pub all_files: &'a BTreeSet<String>,
    pub structure: &'a Structure,
    /// The style table: the built-ins plus the project's own styles, which
    /// name the layers a styled container's components can take.
    pub styles: &'a Styles,
    pub fingerprint: u64,
}

/// The technology a React function component's symbol carries: its
/// properties are its props, and it is no data shape. Mirrors
/// `REACT_COMPONENT` in the frontend's `viewmodel.ts`.
pub const REACT_COMPONENT: &str = "React component";

const ALPHABET: &[u8; 32] = b"0123456789abcdefghjkmnpqrstvwxyz";

/// A deterministic id in the model's `<prefix>-<suffix>` shape, so the same
/// code always derives the same ids.
fn stable_id(prefix: &str, key: &str, taken: &mut HashSet<String>) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in key.bytes() {
        hash = (hash ^ b as u64).wrapping_mul(0x100000001b3);
    }
    loop {
        let suffix: String = (0..8)
            .map(|i| ALPHABET[((hash >> (i * 5)) & 31) as usize] as char)
            .collect();
        let id = format!("{prefix}-{suffix}");
        if taken.insert(id.clone()) {
            return id;
        }
        hash = hash.wrapping_mul(0x100000001b3).wrapping_add(1);
    }
}

/// Test code by path: test directories and the runners' file conventions.
pub fn is_test_path(rel: &str) -> bool {
    let (dirs, file) = rel.rsplit_once('/').unwrap_or(("", rel));
    if dirs
        .split('/')
        .any(|d| matches!(d, "tests" | "test" | "__tests__" | "spec" | "e2e"))
    {
        return true;
    }
    let stem = file.split('.').next().unwrap_or(file);
    file.contains(".test.")
        || file.contains(".spec.")
        || file == "conftest.py"
        || (file.ends_with(".py") && (stem.starts_with("test_") || stem.ends_with("_test")))
        || file.ends_with("_test.go")
        || ((file.ends_with(".cs") || file.ends_with(".java"))
            && (stem.ends_with("Tests") || stem.ends_with("Test")))
}

/// A test file's stem with its runner markers dropped: `tree.test.ts`,
/// `test_tree.py` and `TreeTests.cs` all name `tree`/`Tree`.
fn tested_stem(file: &str) -> String {
    let stem = file.split('.').next().unwrap_or(file);
    let stem = stem.strip_prefix("test_").unwrap_or(stem);
    let stem = stem.strip_suffix("_test").unwrap_or(stem);
    let stem = stem.strip_suffix("Tests").or_else(|| stem.strip_suffix("Test")).unwrap_or(stem);
    stem.to_string()
}

fn file_name(rel: &str) -> &str {
    rel.rsplit('/').next().unwrap_or(rel)
}

fn file_stem(rel: &str) -> &str {
    let name = file_name(rel);
    name.split('.').next().unwrap_or(name)
}

const ENTRY_FILES: &[&str] = &[
    "lib.rs", "main.rs", "mod.rs", "index.ts", "index.tsx", "index.js", "index.jsx", "index.mjs",
    "__init__.py", "main.py", "Program.cs",
];

fn is_test_container(c: &ContainerFacts) -> bool {
    let base = c.dir.rsplit('/').next().unwrap_or(&c.dir).to_lowercase();
    let name = c.name.to_lowercase();
    [base, name].iter().any(|n| {
        n.ends_with(".tests") || n.ends_with("-tests") || n.ends_with("_tests") || n.ends_with(".test")
            || n == "tests"
    })
}

/// Where a file sits inside its container: `crates/x/src/lib.rs` in
/// `crates/x` is `src/lib.rs`.
fn inner_of<'p>(container_dir: &str, rel: &'p str) -> &'p str {
    if container_dir.is_empty() {
        rel
    } else {
        rel.strip_prefix(container_dir).and_then(|r| r.strip_prefix('/')).unwrap_or(rel)
    }
}

/// Where a file sits inside its container, the source root stripped:
/// `crates/x/src/domain/model.rs` reads (`crates/x/src`, `domain/model.rs`).
fn inner_path<'p>(container_dir: &str, rel: &'p str) -> (&'p str, &'p str) {
    let inner = if container_dir.is_empty() {
        rel
    } else {
        rel.strip_prefix(container_dir).and_then(|r| r.strip_prefix('/')).unwrap_or(rel)
    };
    let inner = inner.strip_prefix("src/").unwrap_or(inner);
    (rel[..rel.len() - inner.len()].trim_end_matches('/'), inner)
}

/// A tool's configuration file outside its container's source root
/// (`vite.config.ts`, `docs/astro.config.mjs`, `harness/vite.config.ts`): it
/// configures the build and is no part of the product. One inside `src/`
/// (Astro's `content.config.ts`) is product code like any other.
fn is_tool_config(container_dir: &str, rel: &str) -> bool {
    !inner_path(container_dir, rel).0.ends_with("src") && file_name(rel).contains(".config.")
}

/// A member as the property a symbol node shows: an optional one keeps its
/// `?` on the label.
fn property(m: &Member) -> SchemaProperty {
    SchemaProperty {
        label: if m.optional { format!("{}?", m.name) } else { m.name.clone() },
        description: m.doc.clone().unwrap_or_default(),
        ty: m.ty.clone(),
        vagrant: None,
        stale: None,
        last_touched_at: None,
    }
}

/// The component a product file belongs to: the first directory under the
/// source root, or the file's own module when it sits at the root.
/// Returns (component path, display name).
fn component_of(container_dir: &str, rel: &str) -> (String, String) {
    let (root, inner) = inner_path(container_dir, rel);
    let join = |seg: &str| if root.is_empty() { seg.to_string() } else { format!("{root}/{seg}") };
    match inner.split_once('/') {
        Some((dir, _)) => (join(dir), dir.to_string()),
        None => {
            let stem = file_stem(inner);
            (join(stem), stem.to_string())
        }
    }
}

struct Component {
    id: String,
    name: String,
    container_dir: String,
    files: Vec<String>,
    patterns: BTreeSet<String>,
}

pub fn derive_tree(input: &DeriveInput) -> DerivedTree {
    let s = input.structure;
    let styles = input.styles;
    let mut taken = HashSet::new();
    let mut model = ScryModel::new();

    // Containers, deepest first so a nested package owns its files.
    let mut by_depth: Vec<&ContainerFacts> = input.containers.iter().collect();
    by_depth.sort_by_key(|c| std::cmp::Reverse(c.dir.len()));
    let owner = |rel: &str| -> Option<&ContainerFacts> {
        by_depth.iter().copied().find(|c| {
            c.dir.is_empty() || rel.strip_prefix(c.dir.as_str()).is_some_and(|r| r.starts_with('/'))
        })
    };
    let mut test_containers: HashSet<&str> = input
        .containers
        .iter()
        .filter(|c| is_test_container(c))
        .map(|c| c.dir.as_str())
        .collect();
    // A package holding nothing but test code is a test project too.
    let mut has_product: HashSet<&str> = HashSet::new();
    let mut has_any: HashSet<&str> = HashSet::new();
    for rel in input.lifts.keys() {
        if let Some(c) = owner(rel) {
            has_any.insert(c.dir.as_str());
            if !is_test_path(rel) {
                has_product.insert(c.dir.as_str());
            }
        }
    }
    for dir in has_any.difference(&has_product) {
        test_containers.insert(dir);
    }
    let is_test_file = |rel: &str| {
        is_test_path(rel) || owner(rel).is_some_and(|c| test_containers.contains(c.dir.as_str()))
    };

    let system_name = s.system.name.clone().unwrap_or_else(|| input.project_name.to_string());
    let system_id = stable_id("node", "system", &mut taken);
    model.nodes.push(node(&system_id, Kind::System, &system_name, None));
    // The context level's nodes by their structure.toml name, for the
    // declared relationships between them.
    let mut context_ids: HashMap<&str, String> = HashMap::new();
    for person in &s.system.persons {
        let id = stable_id("node", &format!("person:{person}"), &mut taken);
        model.nodes.push(node(&id, Kind::Person, person, None));
        context_ids.insert(person, id);
    }
    for external in &s.system.externals {
        let id = stable_id("node", &format!("external:{external}"), &mut taken);
        let mut n = node(&id, Kind::System, external, None);
        n.external = Some(true);
        model.nodes.push(n);
        context_ids.insert(external, id);
    }
    if let Some(name) = s.system.name.as_deref() {
        context_ids.insert(name, system_id.clone());
    }

    let mut container_ids: HashMap<&str, String> = HashMap::new();
    let mut containers_sorted: Vec<&ContainerFacts> = input
        .containers
        .iter()
        .filter(|c| !test_containers.contains(c.dir.as_str()))
        .collect();
    containers_sorted.sort_by(|a, b| a.dir.cmp(&b.dir));
    for c in &containers_sorted {
        let id = stable_id("node", &format!("container:{}", c.dir), &mut taken);
        let config = s.container(&c.name, &c.dir);
        let name = config.and_then(|k| k.name.clone()).unwrap_or_else(|| c.name.clone());
        let mut n = node(&id, Kind::Container, &name, Some(&system_id));
        n.technology = c.technology.clone();
        n.style = config.and_then(|k| k.style.clone());
        n.tooling = config.is_some_and(|k| k.tooling).then_some(true);
        n.description = input
            .lifts
            .iter()
            .filter(|(rel, _)| owner(rel).is_some_and(|o| o.dir == c.dir))
            .filter(|(rel, _)| {
                let (_, inner) = inner_path(&c.dir, rel);
                ENTRY_FILES.contains(&inner)
            })
            .find_map(|(_, lift)| lift.module_description.clone());
        let glob = if c.dir.is_empty() { "**".to_string() } else { format!("{}/**", c.dir) };
        model.boundaries.insert(id.clone(), vec![Source { pattern: glob, comment: None }]);
        model.nodes.push(n);
        container_ids.insert(c.dir.as_str(), id);
    }

    let cargo_dirs: HashSet<&str> = input
        .containers
        .iter()
        .filter(|c| {
            let manifest = if c.dir.is_empty() { "Cargo.toml".to_string() } else { format!("{}/Cargo.toml", c.dir) };
            input.all_files.contains(&manifest)
        })
        .map(|c| c.dir.as_str())
        .collect();
    // A crate's `src/lib.rs` and `src/main.rs` are its composition root: the
    // module list and the wiring, not a component. Their doc describes the
    // container, their definitions and tests sit on it, and no style asks
    // them for a layer.
    let crate_root = |rel: &str| {
        rel.ends_with(".rs")
            && owner(rel).is_some_and(|c| {
                cargo_dirs.contains(c.dir.as_str()) && matches!(inner_path(&c.dir, rel).1, "lib.rs" | "main.rs")
            })
    };

    let mut edges_from: HashMap<&str, Vec<&str>> = HashMap::new();
    for e in input.file_edges.iter().filter(|e| !e.guessed) {
        edges_from.entry(e.src.as_str()).or_default().push(e.dst.as_str());
    }

    // A package's components are the directories its entry points reach
    // through the import graph: the entries its manifest or page names, and
    // Cargo's conventional targets. An entry's top directory in the package
    // is a source root, all of it reached (a Rust module tree is declared,
    // not imported). A package with no entry found keeps every directory.
    let mut product_paths: HashMap<&str, HashSet<String>> = HashMap::new();
    for c in &containers_sorted {
        let owned = |f: &str| owner(f).is_some_and(|o| o.dir == c.dir);
        let within = |f: &'_ str| -> String { inner_of(&c.dir, f).to_string() };
        let cargo_target = |f: &str| {
            let inner = within(f);
            matches!(inner.as_str(), "src/lib.rs" | "src/main.rs")
                || inner.starts_with("src/bin/")
                || inner.starts_with("examples/")
        };
        let seeds: BTreeSet<&str> = input
            .lifts
            .keys()
            .map(String::as_str)
            .filter(|f| owned(f) && !is_test_file(f))
            .filter(|f| {
                c.entries.iter().any(|e| e == f) || (cargo_dirs.contains(c.dir.as_str()) && f.ends_with(".rs") && cargo_target(f))
            })
            .collect();
        if seeds.is_empty() {
            continue;
        }
        let roots: BTreeSet<String> =
            seeds.iter().filter_map(|f| within(f).split_once('/').map(|(top, _)| format!("{top}/"))).collect();
        let mut reached: HashSet<&str> = input
            .lifts
            .keys()
            .map(String::as_str)
            .filter(|f| owned(f) && roots.iter().any(|r| within(f).starts_with(r.as_str())))
            .chain(seeds.iter().copied())
            .collect();
        let mut queue: Vec<&str> = reached.iter().copied().collect();
        while let Some(f) = queue.pop() {
            for dst in edges_from.get(f).into_iter().flatten() {
                if reached.insert(dst) {
                    queue.push(dst);
                }
            }
        }
        let paths = reached.iter().filter(|f| owned(f)).map(|f| component_of(&c.dir, f).0).collect();
        product_paths.insert(c.dir.as_str(), paths);
    }

    // Components: every product file's module or top directory. What no
    // entry reaches, and a tool's config, is the container's tooling.
    let mut components: BTreeMap<String, Component> = BTreeMap::new();
    let mut file_component: HashMap<&str, String> = HashMap::new();
    let mut tooling: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for rel in input.lifts.keys() {
        if is_test_file(rel) || crate_root(rel) {
            continue;
        }
        let Some(c) = owner(rel) else { continue };
        let Some(container_id) = container_ids.get(c.dir.as_str()) else { continue };
        let (path, name) = component_of(&c.dir, rel);
        let unreached = product_paths.get(c.dir.as_str()).is_some_and(|paths| !paths.contains(&path));
        if unreached || is_tool_config(&c.dir, rel) {
            tooling.entry(container_id.clone()).or_default().push(rel.clone());
            continue;
        }
        let comp = components.entry(path.clone()).or_insert_with(|| Component {
            id: String::new(),
            name,
            container_dir: c.dir.clone(),
            files: Vec::new(),
            patterns: BTreeSet::new(),
        });
        comp.files.push(rel.clone());
        let (_, inner) = inner_path(&c.dir, rel);
        comp.patterns.insert(if inner.contains('/') { format!("{path}/**") } else { rel.clone() });
        file_component.insert(rel.as_str(), path);
    }

    // Where each test file's claims go, by level.
    let mut homes: Vec<(&String, &FileLift, ClaimLevel, Home)> = Vec::new();
    for (rel, lift) in input.lifts {
        if lift.tests.is_empty() {
            continue;
        }
        let own = owner(rel);
        let level = s.configured_level(rel).unwrap_or_else(|| {
            // Cargo's convention: a crate's `tests/` directory is integration.
            let cargo_tests = !s.integration_configured()
                && rel.ends_with(".rs")
                && own.is_some_and(|c| {
                    cargo_dirs.contains(c.dir.as_str()) && {
                        let prefix = if c.dir.is_empty() { "tests/".to_string() } else { format!("{}/tests/", c.dir) };
                        rel.starts_with(&prefix)
                    }
                });
            if cargo_tests { ClaimLevel::Container } else { ClaimLevel::Component }
        });
        let target_container = own
            .filter(|c| container_ids.contains_key(c.dir.as_str()))
            .map(|c| c.dir.clone())
            .or_else(|| own.and_then(|c| tested_container(c, input, &container_ids, &edges_from, rel, &owner)));
        let home = match (level, target_container) {
            (ClaimLevel::System, _) | (_, None) => Home::System,
            (ClaimLevel::Container, Some(dir)) => Home::Container(dir),
            (ClaimLevel::Component, Some(dir)) if crate_root(rel) => Home::Container(dir),
            (ClaimLevel::Component, Some(dir)) => {
                unit_home(rel, &dir, &components, &file_component, &edges_from)
                    .unwrap_or_else(|| Home::TestStem(dir, tested_stem(file_name(rel))))
            }
        };
        homes.push((rel, lift, level, home));
    }

    // A product file with a module doc is a sub-component of its directory
    // component: the doc declares it one, tested or not. Unless it is that
    // directory's root, whose doc describes the directory itself.
    let sub_files: BTreeSet<&str> = file_component
        .keys()
        .copied()
        .filter(|f| {
            let comp = &components[&file_component[f]];
            input.lifts[*f].module_description.is_some()
                && inner_path(&comp.container_dir, f).1.contains('/')
                && !ENTRY_FILES.contains(&file_name(f))
        })
        .collect();

    for (path, comp) in components.iter_mut() {
        comp.id = stable_id("node", &format!("component:{path}"), &mut taken);
        let container = &container_ids[comp.container_dir.as_str()];
        let mut n = node(&comp.id, Kind::Component, &comp.name, Some(container));
        let config = input
            .containers
            .iter()
            .find(|c| c.dir == comp.container_dir)
            .and_then(|c| s.container(&c.name, &c.dir));
        let layers: Vec<String> = match config {
            Some(k) if !k.layers.is_empty() => k.layers.clone(),
            Some(k) => k
                .style
                .as_deref()
                .and_then(|st| styles.get(st))
                .map(|d| d.layer_names().iter().map(|l| l.to_string()).collect())
                .unwrap_or_default(),
            None => Vec::new(),
        };
        if layers.contains(&comp.name) {
            n.layer = Some(comp.name.clone());
        }
        // The module doc of the component's root: `domain/mod.rs`,
        // `domain.rs`, `index.ts`, `__init__.py`, or a one-file module.
        n.description = comp
            .files
            .iter()
            .filter(|rel| {
                let (_, inner) = inner_path(&comp.container_dir, rel);
                let name = file_name(rel);
                let depth = inner.matches('/').count();
                (depth == 1 && ENTRY_FILES.contains(&name))
                    || (depth == 0 && !ENTRY_FILES.contains(&name))
            })
            .find_map(|rel| input.lifts[rel].module_description.clone());
        model.boundaries.insert(
            comp.id.clone(),
            comp.patterns.iter().map(|p| Source { pattern: p.clone(), comment: None }).collect(),
        );
        model.nodes.push(n);
    }
    let mut sub_ids: HashMap<&str, String> = HashMap::new();
    for f in &sub_files {
        let id = stable_id("node", &format!("subcomponent:{f}"), &mut taken);
        let parent = &components[&file_component[f]].id;
        let mut n = node(&id, Kind::Component, file_stem(f), Some(parent));
        n.description = input.lifts[*f].module_description.clone();
        // A file split out of its directory's component still sits on that
        // directory's layer, so the style checks its imports like the rest.
        n.layer = model.nodes.iter().find(|p| &p.id == parent).and_then(|p| p.layer.clone());
        model.boundaries.insert(id.clone(), vec![Source { pattern: f.to_string(), comment: None }]);
        model.nodes.push(n);
        sub_ids.insert(f, id);
    }
    // A test whose code under test cannot be found gets a component of its
    // own, named after its file, on the container it belongs to.
    let mut stem_ids: BTreeMap<(String, String), String> = BTreeMap::new();
    for (rel, _, _, home) in &homes {
        let Home::TestStem(dir, stem) = home else { continue };
        let key = (dir.clone(), stem.clone());
        let id = stem_ids.entry(key).or_insert_with(|| {
            let id = stable_id("node", &format!("testcomponent:{dir}/{stem}"), &mut taken);
            model.nodes.push(node(&id, Kind::Component, stem, Some(&container_ids[dir.as_str()])));
            id
        });
        model
            .boundaries
            .entry(id.clone())
            .or_default()
            .push(Source { pattern: rel.to_string(), comment: None });
    }

    // Symbols: outermost definitions of product files, test code left out;
    // a crate root's sit on its container. Each carries its doc comment, and
    // a data shape its fields, a React component its props, typed and
    // documented as written.
    let mut file_symbols: HashMap<&str, Vec<(&str, usize)>> = HashMap::new();
    for f in input.files {
        let parent = match file_component.get(f.rel_path.as_str()) {
            Some(path) => sub_ids.get(f.rel_path.as_str()).unwrap_or(&components[path].id).clone(),
            None if crate_root(&f.rel_path) => match owner(&f.rel_path).and_then(|c| container_ids.get(c.dir.as_str())) {
                Some(id) => id.clone(),
                None => continue,
            },
            None => continue,
        };
        let lift = input.lifts.get(&f.rel_path);
        let in_test = |line: u32| {
            lift.is_some_and(|l| {
                l.test_spans.iter().any(|&(a, b)| a <= line && line <= b)
                    || l.tests.iter().any(|t| t.line <= line && line <= t.end_line)
            })
        };
        let mut open_end = 0u32;
        for sym in &f.symbols {
            if sym.start_line <= open_end {
                continue;
            }
            open_end = sym.end_line;
            if in_test(sym.start_line) {
                continue;
            }
            let id = stable_id("node", &format!("symbol:{}#{}", f.rel_path, sym.name), &mut taken);
            let mut n = node(&id, Kind::Symbol, &sym.name, Some(&parent));
            n.description = sym.doc.clone();
            if sym.is_data_shape || sym.is_component {
                n.properties = if sym.members.is_empty() {
                    sym.fields.iter().map(|name| property(&Member { name: name.clone(), ..Default::default() })).collect()
                } else {
                    sym.members.iter().map(property).collect()
                };
            }
            if sym.is_component {
                n.technology = Some(REACT_COMPONENT.to_string());
            }
            file_symbols.entry(f.rel_path.as_str()).or_default().push((sym.name.as_str(), model.nodes.len()));
            model.source_map.insert(
                id.clone(),
                vec![SourceLocation {
                    pattern: f.rel_path.clone(),
                    symbol: Some(sym.name.clone()),
                    line: Some(sym.start_line),
                    end_line: Some(sym.end_line),
                }],
            );
            model.nodes.push(n);
        }
    }

    // Claims onto their homes.
    let mut claims = Vec::new();
    let node_index: HashMap<String, usize> =
        model.nodes.iter().enumerate().map(|(i, n)| (n.id.clone(), i)).collect();
    for (rel, lift, level, home) in &homes {
        let node_id = match home {
            Home::File(f) => sub_ids
                .get(f.as_str())
                .unwrap_or(&components[&file_component[f.as_str()]].id)
                .clone(),
            Home::Component(path) => components[path].id.clone(),
            Home::Container(dir) => container_ids[dir.as_str()].clone(),
            Home::TestStem(dir, stem) => stem_ids[&(dir.clone(), stem.clone())].clone(),
            Home::System => system_id.clone(),
        };
        // The files whose symbols this file's tests can exercise: its own
        // (an inline test module) and the files it imports.
        let reachable: BTreeSet<&str> = std::iter::once(rel.as_str())
            .chain(edges_from.get(rel.as_str()).into_iter().flatten().copied())
            .collect();
        for t in &lift.tests {
            let id = stable_id("resp", &format!("claim:{rel}#{}", t.junit_name), &mut taken);
            for f in &reachable {
                for &(name, idx) in file_symbols.get(f).into_iter().flatten() {
                    if name.len() >= 3 && t.names.contains(name) {
                        model.nodes[idx].exercised_by.push(id.clone());
                    }
                }
            }
            let concern = t.concerns.iter().find(|c| !c.contains(':')).cloned();
            let idx = node_index[&node_id];
            model.nodes[idx].responsibilities.push(Responsibility {
                id: id.clone(),
                statement: t.description.clone(),
                concern,
                vagrant: None,
                stale: None,
                stale_proposal: None,
                directives: Vec::new(),
                last_touched_at: None,
            });
            model.test_map.insert(
                id.clone(),
                vec![SourceLocation {
                    pattern: rel.to_string(),
                    symbol: Some(t.identifier.clone()),
                    line: Some(t.line),
                    end_line: Some(t.end_line),
                }],
            );
            // The file the test exercises: an edit there ages its verdict.
            if let Home::File(f) = home {
                model.source_map.insert(
                    id.clone(),
                    vec![SourceLocation { pattern: f.clone(), symbol: None, line: None, end_line: None }],
                );
            }
            claims.push(DerivedClaim {
                id,
                node_id: node_id.clone(),
                level: *level,
                file: rel.to_string(),
                line: t.line,
                identifier: t.identifier.clone(),
                description: t.description.clone(),
                concerns: t.concerns.clone(),
                junit_name: t.junit_name.clone(),
                planned: t.planned,
            });
        }
    }

    // Links: the import graph between components and between containers. A
    // crate root (lib.rs, main.rs) is its container, not a component, but
    // what it imports is the container's dependency all the same: its edges
    // link the container itself, to the other side's component.
    let endpoint = |f: &str| -> Option<(&str, Option<&String>)> {
        match file_component.get(f) {
            Some(path) => Some((components[path].container_dir.as_str(), Some(&components[path].id))),
            None if crate_root(f) => owner(f).map(|c| (c.dir.as_str(), None)),
            None => None,
        }
    };
    let mut seen: HashSet<(String, String)> = HashSet::new();
    for e in input.file_edges.iter().filter(|e| !e.guessed) {
        let (Some((da, ca)), Some((db, cb))) = (endpoint(e.src.as_str()), endpoint(e.dst.as_str())) else {
            continue;
        };
        if da == db {
            // Inside one container only components link; its root wiring
            // its own modules is no dependency.
            if let (Some(ca), Some(cb)) = (ca, cb) {
                if ca != cb {
                    push_link(&mut model, &mut seen, &mut taken, ca, cb, Some(LinkKind::Depends), "");
                }
            }
            continue;
        }
        let (sa, sb) = (&container_ids[da], &container_ids[db]);
        push_link(&mut model, &mut seen, &mut taken, sa, sb, None, "");
        push_link(&mut model, &mut seen, &mut taken, ca.unwrap_or(sa), cb.unwrap_or(sb), Some(LinkKind::Depends), "");
    }

    // Tauri's IPC: a frontend calls its Rust host through `invoke`, never an
    // import, so the import graph has no edge between them. A JS/TS container
    // whose files import `@tauri-apps/api` invokes the commands of the repo's
    // crate on `tauri` — the app, the one building with `tauri-build`, when
    // several crates are on it.
    let depends_on = |c: &ContainerFacts, package: &str| {
        cargo_dirs.contains(c.dir.as_str()) && c.packages.iter().any(|p| p == package)
    };
    let mut hosts: Vec<&ContainerFacts> =
        containers_sorted.iter().copied().filter(|c| depends_on(c, "tauri")).collect();
    if hosts.iter().any(|c| depends_on(c, "tauri-build")) {
        hosts.retain(|c| depends_on(c, "tauri-build"));
    }
    let fronts: BTreeSet<&str> = input
        .external_imports
        .iter()
        .filter(|i| i.package == "@tauri-apps/api" && !is_test_file(&i.file))
        .filter_map(|i| owner(&i.file))
        .map(|c| c.dir.as_str())
        .filter(|d| container_ids.contains_key(d))
        .collect();
    for front in &fronts {
        for host in hosts.iter().filter(|h| h.dir != *front) {
            let (a, b) = (&container_ids[front], &container_ids[host.dir.as_str()]);
            push_link(&mut model, &mut seen, &mut taken, a, b, None, "invokes commands");
        }
    }

    // Declared links: the relationships structure.toml gives, and the
    // externals a container says it is the one to talk to. Facts the config
    // states, not read off imports. A relationship with a container at one
    // end sits on that container, and on the system too for the context, with
    // its label, unless the config relates the two itself; so does a
    // container's use of an external.
    let declared_container = |name: &str| -> Option<&String> {
        containers_sorted
            .iter()
            .find(|c| {
                c.dir == name
                    || c.name == name
                    || s.container(&c.name, &c.dir).and_then(|k| k.name.as_deref()) == Some(name)
            })
            .map(|c| &container_ids[c.dir.as_str()])
    };
    let mut related: HashSet<&str> = HashSet::new();
    let mut lifted: Vec<(String, String, &str)> = Vec::new();
    for (src, dst, label) in s.relationships() {
        related.insert(src);
        related.insert(dst);
        let label = label.unwrap_or_default();
        let (ca, cb) = (context_ids.get(src), context_ids.get(dst));
        if let (Some(a), Some(b)) = (ca, cb) {
            push_link(&mut model, &mut seen, &mut taken, a, b, None, label);
            continue;
        }
        let (Some(a), Some(b)) = (ca.or_else(|| declared_container(src)), cb.or_else(|| declared_container(dst))) else {
            continue;
        };
        push_link(&mut model, &mut seen, &mut taken, a, b, None, label);
        let lift = |end: Option<&String>, id: &String| end.map_or(system_id.clone(), |_| id.clone());
        let (la, lb) = (lift(ca, a), lift(cb, b));
        if la != lb {
            lifted.push((la, lb, label));
        }
    }
    for c in &containers_sorted {
        let Some(config) = s.container(&c.name, &c.dir) else { continue };
        for u in &config.uses {
            let Some(external) = context_ids.get(u.to()) else { continue };
            related.insert(u.to());
            let label = u.label().unwrap_or_default();
            push_link(&mut model, &mut seen, &mut taken, &container_ids[c.dir.as_str()], external, None, label);
            lifted.push((system_id.clone(), external.clone(), label));
        }
    }
    for (a, b, label) in &lifted {
        push_link(&mut model, &mut seen, &mut taken, a, b, None, label);
    }
    // An actor nothing relates is still the system's: a person uses it, and
    // it uses an external. Unlabelled, since nothing says how.
    for person in s.system.persons.iter().filter(|p| !related.contains(p.as_str())) {
        push_link(&mut model, &mut seen, &mut taken, &context_ids[person.as_str()], &system_id, None, "");
    }
    for external in s.system.externals.iter().filter(|e| !related.contains(e.as_str())) {
        push_link(&mut model, &mut seen, &mut taken, &system_id, &context_ids[external.as_str()], None, "");
    }

    scryer_core::concerns::register_concerns(&mut model);
    let planned_ids: HashSet<&str> = claims.iter().filter(|c| c.planned).map(|c| c.id.as_str()).collect();
    let mut committed = model.clone();
    for n in committed.nodes.iter_mut() {
        n.responsibilities.retain(|r| !planned_ids.contains(r.id.as_str()));
        n.exercised_by.retain(|id| !planned_ids.contains(id.as_str()));
    }
    committed.test_map.retain(|id, _| !planned_ids.contains(id.as_str()));
    // A crate root maps to its crate, so code no test reaches there is dark
    // like any other.
    let file_nodes: BTreeMap<String, String> = file_component
        .iter()
        .map(|(f, path)| (f.to_string(), sub_ids.get(f).unwrap_or(&components[path].id).clone()))
        .chain(input.lifts.keys().filter(|f| crate_root(f)).filter_map(|f| {
            let id = owner(f).and_then(|c| container_ids.get(c.dir.as_str()))?;
            Some((f.clone(), id.clone()))
        }))
        .collect();
    // Layer position asks nothing of what sits outside a container's source
    // root — build scripts, tool configs, examples, benches — nor of a
    // component named after an orphan test file.
    let has_src: HashSet<&str> = input
        .lifts
        .keys()
        .filter_map(|f| owner(f).filter(|c| inner_path(&c.dir, f).0.ends_with("src")).map(|c| c.dir.as_str()))
        .collect();
    let outside_source = |f: &str| {
        let Some(c) = owner(f) else { return false };
        let within = if c.dir.is_empty() { f } else { f.strip_prefix(c.dir.as_str()).and_then(|r| r.strip_prefix('/')).unwrap_or(f) };
        let aux = ["examples/", "benches/"].iter().any(|d| within.starts_with(d));
        if has_src.contains(c.dir.as_str()) {
            !within.starts_with("src/")
        } else {
            let name = file_name(within);
            aux || (!within.contains('/') && (name == "build.rs" || name.contains(".config.") || name.starts_with("tsconfig")))
        }
    };
    let mut layer_exempt: Vec<String> = components
        .values()
        .filter(|c| c.files.iter().all(|f| outside_source(f)))
        .map(|c| c.id.clone())
        .chain(stem_ids.values().cloned())
        .collect();
    layer_exempt.sort();
    // Dark code: product files no test exercises. Only a test file with a
    // test that runs (not a todo, skip or `todo!()` stub) reaches code, and
    // only through imports that run it: a type-only import names a file's
    // types without executing a line of it. Claim placement still reads
    // every import, type imports included.
    let mut runs_from: HashMap<&str, Vec<&str>> = HashMap::new();
    for e in input.file_edges.iter().filter(|e| !e.guessed && !e.type_only) {
        runs_from.entry(e.src.as_str()).or_default().push(e.dst.as_str());
    }
    let mut reached: HashSet<&str> = homes
        .iter()
        .filter(|(_, lift, ..)| lift.tests.iter().any(|t| !t.planned))
        .map(|(rel, ..)| rel.as_str())
        .collect();
    let mut queue: Vec<&str> = reached.iter().copied().collect();
    while let Some(f) = queue.pop() {
        for dst in runs_from.get(f).into_iter().flatten() {
            if reached.insert(dst) {
                queue.push(dst);
            }
        }
    }
    // A barrel (mod.rs, lib.rs, index.ts, __init__.py) is reached when
    // anything it re-exports is: tests import the module's items, which
    // resolve past the barrel to the files defining them. What it re-exports
    // is what it imports, and for a Rust module root also every file of the
    // module it declares. Nested barrels settle over a few passes.
    let barrels: Vec<&str> = file_nodes.keys().map(String::as_str).filter(|f| is_barrel(f)).collect();
    loop {
        let mut grew = false;
        for &b in &barrels {
            if reached.contains(b) {
                continue;
            }
            let module_dir = b.rsplit_once('/').map(|(dir, _)| format!("{dir}/")).unwrap_or_default();
            let rust_root = b.ends_with(".rs");
            let reexported = runs_from.get(b).into_iter().flatten().any(|d| reached.contains(d))
                || (rust_root && reached.iter().any(|r| *r != b && r.starts_with(&module_dir) && r.ends_with(".rs")));
            if reexported {
                reached.insert(b);
                grew = true;
            }
        }
        if !grew {
            break;
        }
    }
    let dark_files: Vec<String> =
        file_nodes.keys().filter(|f| !reached.contains(f.as_str())).cloned().collect();
    DerivedTree {
        fingerprint: input.fingerprint,
        committed,
        planned: model,
        claims,
        file_nodes,
        dark_files,
        style_violations: Vec::new(),
        layer_exempt,
        tooling,
    }
}

/// A file whose job is to gather and re-export a module: a Rust module or
/// crate root, a TS/JS index, a Python package init.
fn is_barrel(rel: &str) -> bool {
    matches!(
        file_name(rel),
        "mod.rs" | "lib.rs" | "index.ts" | "index.tsx" | "index.js" | "index.jsx" | "index.mjs" | "__init__.py"
    )
}

/// The product container a test project exercises: the one its name drops
/// the test suffix to, else its only declared project reference, else the
/// referenced one its imports reach most, else its first reference.
fn tested_container<'a>(
    test_container: &ContainerFacts,
    input: &DeriveInput,
    container_ids: &HashMap<&str, String>,
    edges_from: &HashMap<&str, Vec<&str>>,
    rel: &str,
    owner: &dyn Fn(&str) -> Option<&'a ContainerFacts>,
) -> Option<String> {
    let lower = test_container.name.to_lowercase();
    let bare = [".tests", "-tests", "_tests", ".test", "tests"]
        .iter()
        .find_map(|suf| lower.strip_suffix(suf))
        .unwrap_or(&lower)
        .to_string();
    if let Some(c) = input
        .containers
        .iter()
        .find(|c| container_ids.contains_key(c.dir.as_str()) && c.name.to_lowercase() == bare)
    {
        return Some(c.dir.clone());
    }
    // Only what the test project declares it references, when it declares.
    let deps: Vec<&String> =
        test_container.dep_dirs.iter().filter(|d| container_ids.contains_key(d.as_str())).collect();
    if let [only] = deps.as_slice() {
        return Some(only.to_string());
    }
    let allowed = |dir: &str| deps.is_empty() || deps.iter().any(|d| d.as_str() == dir);
    // The test file's own imports, else those of its whole test project.
    let reached = |sources: &mut dyn Iterator<Item = &str>| -> Option<String> {
        let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
        for src in sources {
            for dst in edges_from.get(src).into_iter().flatten() {
                if let Some(c) = owner(dst)
                    .filter(|c| container_ids.contains_key(c.dir.as_str()) && allowed(&c.dir))
                {
                    *counts.entry(c.dir.as_str()).or_default() += 1;
                }
            }
        }
        counts
            .into_iter()
            .max_by_key(|&(dir, n)| (n, std::cmp::Reverse(dir)))
            .map(|(dir, _)| dir.to_string())
    };
    let siblings = input
        .lifts
        .keys()
        .map(String::as_str)
        .filter(|f| owner(f).is_some_and(|c| c.dir == test_container.dir));
    if let Some(dir) = reached(&mut std::iter::once(rel)).or_else(|| reached(&mut { siblings })) {
        return Some(dir);
    }
    test_container
        .dep_dirs
        .iter()
        .find(|d| container_ids.contains_key(d.as_str()))
        .cloned()
}

/// Where a test file's claims nest.
enum Home {
    /// The product file under test (a sub-component when it has a doc).
    File(String),
    Component(String),
    Container(String),
    /// A component named after a test file whose code under test is unknown.
    TestStem(String, String),
    System,
}

/// The code a unit test exercises: its own file when the test is inline,
/// else the product file its name mirrors (nearest directory first), else
/// the component of its own directory, else the most-imported file of the
/// component its imports reach most.
fn unit_home(
    rel: &str,
    container_dir: &str,
    components: &BTreeMap<String, Component>,
    file_component: &HashMap<&str, String>,
    edges_from: &HashMap<&str, Vec<&str>>,
) -> Option<Home> {
    if file_component.contains_key(rel) {
        return Some(Home::File(rel.to_string()));
    }
    let in_container = |f: &str| components[&file_component[f]].container_dir == container_dir;
    let stem = tested_stem(file_name(rel));
    let shared_dirs = |f: &str| {
        f.split('/').zip(rel.split('/')).take_while(|(a, b)| a == b).count()
    };
    let mirrored = file_component
        .keys()
        .filter(|f| file_stem(f) == stem && in_container(f))
        .max_by_key(|f| (shared_dirs(f), std::cmp::Reverse(**f)));
    if let Some(f) = mirrored {
        return Some(Home::File(f.to_string()));
    }
    // A colocated test file sits in its component's directory.
    let (own_path, _) = component_of(container_dir, rel);
    if inner_path(container_dir, rel).1.contains('/') && components.contains_key(&own_path) {
        return Some(Home::Component(own_path));
    }
    // The component the test's imports reach most: in its own container
    // first, else in whichever container of the repo they reach.
    let most_imported = |within: &dyn Fn(&str) -> bool| -> Option<Home> {
        let mut by_component: BTreeMap<&String, usize> = BTreeMap::new();
        let mut by_file: BTreeMap<&str, usize> = BTreeMap::new();
        for dst in edges_from.get(rel).into_iter().flatten() {
            if let Some(path) = file_component.get(dst).filter(|_| within(dst)) {
                *by_component.entry(path).or_default() += 1;
                *by_file.entry(dst).or_default() += 1;
            }
        }
        let (top, _) = by_component.into_iter().max_by_key(|&(path, n)| (n, std::cmp::Reverse(path)))?;
        by_file
            .into_iter()
            .filter(|(f, _)| &file_component[f] == top)
            .max_by_key(|&(f, n)| (n, std::cmp::Reverse(f)))
            .map(|(f, _)| Home::File(f.to_string()))
    };
    most_imported(&in_container).or_else(|| most_imported(&|_| true))
}

fn push_link(
    model: &mut ScryModel,
    seen: &mut HashSet<(String, String)>,
    taken: &mut HashSet<String>,
    src: &str,
    dst: &str,
    kind: Option<LinkKind>,
    label: &str,
) {
    if !seen.insert((src.to_string(), dst.to_string())) {
        return;
    }
    let id = stable_id("link", &format!("{src}->{dst}"), taken);
    model.links.push(Link {
        id,
        src: src.to_string(),
        dst: dst.to_string(),
        label: label.to_string(),
        method: None,
        kind,
    });
}

fn node(id: &str, kind: Kind, name: &str, parent: Option<&str>) -> Node {
    Node {
        id: id.to_string(),
        kind,
        name: name.to_string(),
        parent_id: parent.map(str::to_string),
        external: None,
        technology: None,
        description: None,
        vagrant: None,
        stale: None,
        responsibilities: Vec::new(),
        properties: Vec::new(),
        icon: None,
        notes: None,
        position: None,
        directives: Vec::new(),
        style: None,
        layer: None,
        exercised_by: Vec::new(),
        tooling: None,
    }
}
