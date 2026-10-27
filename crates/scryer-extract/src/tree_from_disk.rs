//! Deriving the C4 tree off disk: walk the project, lift every source file's
//! claims, derive the tree, check it against the declared styles, and cache
//! it keyed on file mtimes so an unchanged project reads it back without
//! parsing. [infrastructure]

use crate::claim_text::{self as claims, FileLift};
use crate::code_map as context;
use crate::derivation::{derive_tree, DeriveInput};
use crate::derivation::Structure;
use crate::discovery;
use rayon::prelude::*;
use scryer_core::derived_tree::{
    read_derived_tree_at, write_derived_tree_at, DerivedTree, DerivedViolation,
};
use scryer_core::{scan, ModelRef};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};

/// Bumped whenever derivation changes, so a cache from an older binary is
/// never mistaken for current.
const DERIVE_VERSION: u32 = 22;

thread_local! {
    static PARSER: RefCell<tree_sitter::Parser> = RefCell::new(tree_sitter::Parser::new());
}

fn structure_path(project: &Path) -> PathBuf {
    project.join(".scryer").join("structure.toml")
}

/// The project's `structure.toml`, or defaults when there is none.
pub fn read_structure(project: &Path) -> Result<Structure, String> {
    match std::fs::read_to_string(structure_path(project)) {
        Ok(text) => Structure::parse(&text),
        Err(_) => Ok(Structure::default()),
    }
}

/// Hash of every walked file's path, mtime and size, plus `structure.toml`
/// and the project's style files (`.scryer/` is not walked).
fn fingerprint(project: &Path, files: &[PathBuf]) -> u64 {
    let style_files: Vec<PathBuf> = std::fs::read_dir(project.join(".scryer").join("styles"))
        .map(|entries| entries.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();
    let mut stamps: Vec<(String, u128, u64)> = files
        .iter()
        .chain(std::iter::once(&structure_path(project)))
        .chain(std::iter::once(&project.join(crate::derivation::ARCHITECTURE_FILE)))
        .chain(&style_files)
        .filter_map(|path| {
            let meta = std::fs::metadata(path).ok()?;
            let mtime = meta
                .modified()
                .ok()?
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            Some((path.to_string_lossy().to_string(), mtime, meta.len()))
        })
        .collect();
    stamps.sort();
    let mut hasher = DefaultHasher::new();
    DERIVE_VERSION.hash(&mut hasher);
    stamps.hash(&mut hasher);
    hasher.finish()
}

/// The project's derived tree: the cached one when no file changed since,
/// otherwise derived afresh and cached.
pub fn derived_tree(project: &Path) -> Result<DerivedTree, String> {
    if !project.is_dir() {
        return Err(format!("'{}' is not a directory", project.display()));
    }
    let r = ModelRef::ProjectLocal(project.to_path_buf());
    let files = discovery::walk_files(project);
    let print = fingerprint(project, &files);
    if let Some(cached) = read_derived_tree_at(&r).filter(|t| t.fingerprint == print) {
        return Ok(cached);
    }
    let tree = derive_uncached(project, &files, print)?;
    // A failed cache write only costs the next read a derivation.
    let _ = write_derived_tree_at(&r, &tree);
    Ok(tree)
}

/// Derive the tree without touching the cache — for reading a project that
/// must not be written to.
pub fn derived_tree_uncached(project: &Path) -> Result<DerivedTree, String> {
    let files = discovery::walk_files(project);
    let print = fingerprint(project, &files);
    derive_uncached(project, &files, print)
}

fn derive_uncached(project: &Path, files: &[PathBuf], print: u64) -> Result<DerivedTree, String> {
    let structure = read_structure(project)?;
    let all_files: BTreeSet<String> = files
        .iter()
        .filter_map(|p| p.strip_prefix(project).ok())
        .map(|rel| rel.to_string_lossy().replace('\\', "/"))
        .collect();
    let mut containers = discovery::discover_containers_from_files(project, files);
    structure.promote_declared_dirs(&mut containers, &all_files);
    let ts_aliases = discovery::discover_ts_aliases(project, files);
    let (parsed, _) = discovery::parse_sources(project, files);
    let project_name = project
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "project".to_string());
    let ctx = context::build_context(&project_name, &containers, &parsed, &ts_aliases);
    let lifts: BTreeMap<String, FileLift> = all_files
        .par_iter()
        .filter(|rel| scan::is_product_code(rel))
        .filter_map(|rel| {
            let source = std::fs::read_to_string(project.join(rel)).ok()?;
            let lift = PARSER.with(|p| claims::lift_file(rel, &source, &mut p.borrow_mut()));
            Some((rel.clone(), lift))
        })
        .collect();
    let styles = scryer_core::load_styles(project);
    let manifest_descriptions: BTreeMap<String, (String, String)> = ctx
        .containers
        .iter()
        .filter_map(|c| discovery::manifest_description(project, &c.dir).map(|d| (c.dir.clone(), d)))
        .collect();
    let architecture = std::fs::read_to_string(project.join(crate::derivation::ARCHITECTURE_FILE)).ok();
    let mut tree = derive_tree(&DeriveInput {
        project_name: &ctx.project_name,
        containers: &ctx.containers,
        files: &ctx.files,
        file_edges: &ctx.file_edges,
        external_imports: &ctx.external_imports,
        lifts: &lifts,
        all_files: &all_files,
        structure: &structure,
        styles: &styles,
        manifest_descriptions: &manifest_descriptions,
        architecture: architecture.as_deref(),
        fingerprint: print,
    });
    // The styles structure.toml declares, checked against the real imports.
    let edges = ctx.build_edges();
    let graph = scryer_core::build_edges::derive_graph(&tree.planned, &edges);
    let report = scryer_core::style_health::check_code(
        &tree.planned,
        &styles,
        &graph,
        &edges,
        Some(&all_files),
    );
    // An exempt component is asked for no layer; cycles still count.
    let exempt: std::collections::HashSet<&str> = tree.layer_exempt.iter().map(String::as_str).collect();
    let violations = report.exempting(&exempt).violations;
    // A forbidden link names the file whose import makes it. The checker
    // sees symbol edges only, and an import that sits outside any definition
    // (a module-scope `use` behind an `impl Trait for T`) makes a file edge
    // alone; the derivation has those.
    let lift = |node: &str| {
        scryer_core::style::layer_component(&tree.planned, node).map(|c| c.id.clone()).unwrap_or_else(|| node.to_string())
    };
    let importer = |src: &str, dst: &str| {
        ctx.file_edges.iter().filter(|e| !e.guessed).find_map(|e| {
            let (a, b) = (tree.file_nodes.get(&e.src)?, tree.file_nodes.get(&e.dst)?);
            (lift(a) == src && lift(b) == dst).then(|| e.src.clone())
        })
    };
    tree.style_violations = violations
        .into_iter()
        .map(|v| {
            let file = match (&v.kind, v.other.as_deref()) {
                (scryer_core::style_health::ViolationKind::ForbiddenLink, Some(dst)) => {
                    importer(&v.node, dst).unwrap_or(v.file)
                }
                _ => v.file,
            };
            DerivedViolation { node_id: v.node, file, kind: v.kind.as_str().to_string(), detail: v.detail }
        })
        .collect();
    let unmatched = unmatched_entries(&structure, &containers, &tree.planned);
    tree.style_violations.extend(unmatched);
    let doubled = doubled_descriptions(&structure, &tree);
    tree.style_violations.extend(doubled);
    Ok(tree)
}

/// The descriptions structure.toml gives a container or component the code
/// already describes: two sources would drift apart, and the code's wins.
fn doubled_descriptions(structure: &Structure, tree: &DerivedTree) -> Vec<DerivedViolation> {
    let model = &tree.planned;
    let in_code = |id: &str| tree.doc_files.get(id).is_some_and(|f| f != crate::derivation::STRUCTURE_FILE);
    let key_of = |id: &str| -> Vec<&str> {
        model.boundaries.get(id).into_iter().flatten().map(|b| b.pattern.strip_suffix("/**").unwrap_or(&b.pattern)).collect()
    };
    let mut out = Vec::new();
    for n in &model.nodes {
        let entry = match n.kind {
            scryer_core::Kind::Component => key_of(&n.id)
                .into_iter()
                .find(|k| structure.component_description(k).is_some())
                .map(|k| format!("[components.\"{k}\"]")),
            scryer_core::Kind::Container => structure
                .containers
                .iter()
                .find(|(k, c)| c.description.is_some() && (key_of(&n.id).contains(&k.as_str()) || c.name.as_deref() == Some(&n.name)))
                .map(|(k, _)| format!("[containers.\"{k}\"]")),
            _ => None,
        };
        let (Some(entry), Some(file)) = (entry, tree.doc_files.get(&n.id)) else { continue };
        if in_code(&n.id) && n.description.is_some() {
            out.push(DerivedViolation {
                node_id: n.id.clone(),
                file: crate::derivation::STRUCTURE_FILE.to_string(),
                kind: "unmatched".to_string(),
                detail: format!("{entry} describes {}, which {file} already describes: remove the description from structure.toml", n.name),
            });
        }
    }
    out
}

/// The `[containers]` and `[components]` entries of structure.toml that key
/// nothing the code has: a package or module renamed or moved since. Each
/// is a finding charged to the system, fixed by re-keying or removing it.
fn unmatched_entries(
    structure: &Structure,
    containers: &[crate::code_map::Container],
    model: &scryer_core::ScryModel,
) -> Vec<DerivedViolation> {
    let Some(system) = model.nodes.iter().find(|n| n.kind == scryer_core::Kind::System && n.external.is_none()) else {
        return Vec::new();
    };
    let component_paths: BTreeSet<&str> = model
        .nodes
        .iter()
        .filter(|n| n.kind == scryer_core::Kind::Component)
        .filter_map(|n| model.boundaries.get(&n.id))
        .flatten()
        .map(|src| src.pattern.strip_suffix("/**").unwrap_or(&src.pattern))
        .collect();
    let finding = |table: &str, key: &str, what: &str| DerivedViolation {
        node_id: system.id.clone(),
        file: ".scryer/structure.toml".to_string(),
        kind: "unmatched".to_string(),
        detail: format!("[{table}.\"{key}\"] keys no {what} in the code: re-key it to where the code is now, or remove it"),
    };
    let containers = structure
        .containers
        .keys()
        .filter(|key| !containers.iter().any(|c| &c.dir == *key || &c.name == *key))
        .map(|key| finding("containers", key, "container"));
    let components = structure
        .components
        .keys()
        .filter(|key| !component_paths.contains(key.as_str()))
        .map(|key| finding("components", key, "component"));
    containers.chain(components).collect()
}

