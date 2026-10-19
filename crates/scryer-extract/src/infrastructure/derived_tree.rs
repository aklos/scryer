//! Deriving the C4 tree off disk: walk the project, lift every source file's
//! claims, derive the tree, check it against the declared styles, and cache
//! it keyed on file mtimes so an unchanged project reads it back without
//! parsing.

use crate::domain::claims::{self, FileLift};
use crate::domain::context;
use crate::domain::derive::{derive_tree, DeriveInput};
use crate::domain::structure::Structure;
use crate::infrastructure::{manifest, tsconfig, walk};
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
const DERIVE_VERSION: u32 = 5;

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
    let files = walk::walk_files(project);
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
    let files = walk::walk_files(project);
    let print = fingerprint(project, &files);
    derive_uncached(project, &files, print)
}

fn derive_uncached(project: &Path, files: &[PathBuf], print: u64) -> Result<DerivedTree, String> {
    let structure = read_structure(project)?;
    let containers = manifest::discover_containers_from_files(project, files);
    let ts_aliases = tsconfig::discover_ts_aliases(project, files);
    let (parsed, _) = walk::parse_sources(project, files);
    let project_name = project
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "project".to_string());
    let ctx = context::build_context(&project_name, &containers, &parsed, &ts_aliases);
    let all_files: BTreeSet<String> = files
        .iter()
        .filter_map(|p| p.strip_prefix(project).ok())
        .map(|rel| rel.to_string_lossy().replace('\\', "/"))
        .collect();
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
    let mut tree = derive_tree(&DeriveInput {
        project_name: &ctx.project_name,
        containers: &ctx.containers,
        files: &ctx.files,
        file_edges: &ctx.file_edges,
        lifts: &lifts,
        all_files: &all_files,
        structure: &structure,
        styles: &styles,
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
    let positional = |kind: &str| matches!(kind, "layerless" | "layer_violation" | "misplaced");
    let violations: Vec<_> = report
        .violations
        .into_iter()
        .filter(|v| {
            !(positional(v.kind.as_str())
                && (exempt.contains(v.node.as_str()) || v.other.as_deref().is_some_and(|o| exempt.contains(o))))
        })
        .collect();
    tree.style_violations = violations
        .into_iter()
        .map(|v| DerivedViolation {
            node_id: v.node,
            file: v.file,
            kind: v.kind.as_str().to_string(),
            detail: v.detail,
        })
        .collect();
    Ok(tree)
}

