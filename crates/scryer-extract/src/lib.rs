//! Deterministic, parser-only codebase CONTEXT extraction.
//!
//! scryer's modeling speed lever: instead of making the AI agent discover a
//! codebase from scratch, [`extract_context`] precomputes a MAP of it — every
//! container (declared build/deploy unit), every symbol with its exact line
//! range, and a conservative dependency graph — using only manifests and
//! tree-sitter parse trees (no LLM, no curated "known-X" lookup tables). The
//! orchestrator slices this per scope ([`slice_scope`]) and hands each modeling
//! subagent exactly the facts it needs, so the agent skips discovery and goes
//! straight to the semantic work: choosing components and writing
//! responsibilities. The context is a map the agent reads FROM — it is never a
//! C4 model, and it is never persisted to disk.

pub mod anchors;
pub mod context;
pub mod lang;
pub mod manifest;

pub use context::{
    build_context, compact_scope, slice_container, slice_scope, ContainerFacts, Edge, FileContext,
    ProjectContext, PromptScopeContext, ScopeContext, SymbolContext,
};

use context::ParsedFile;
use rayon::prelude::*;
use scryer_core::scan;
use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Mutex, OnceLock,
};

thread_local! {
    /// Tree-sitter parsers are mutable and not shared across workers. Rayon
    /// keeps worker threads alive, so this also reuses parser allocation across
    /// files and successive extraction calls.
    static PARSER: RefCell<tree_sitter::Parser> =
        RefCell::new(tree_sitter::Parser::new());
}

/// In-process incremental parse cache. The binary version and bundled grammar
/// versions implicitly version the cache because it never crosses processes.
static PARSE_CACHE: OnceLock<Mutex<HashMap<PathBuf, (u64, lang::FileParse)>>> = OnceLock::new();
const MAX_CACHED_FILES: usize = 20_000;

#[derive(Debug, Clone, Copy, Default)]
pub struct ExtractionStats {
    pub source_files: usize,
    pub parsed_files: usize,
    pub cache_hits: usize,
}

/// Walk a project directory and build its deterministic [`ProjectContext`].
pub fn extract_context(project: &Path) -> Result<ProjectContext, String> {
    extract_context_with_stats(project).map(|(context, _)| context)
}

/// Every file under `project` (project-relative, forward-slashed), skipping the
/// same vendor/build directories the extractor ignores. A cheap walk with no
/// parse — used to resolve boundary globs against real files (e.g. for
/// completeness: does a node's claimed territory actually contain code?).
pub fn list_project_files(project: &Path) -> std::collections::BTreeSet<String> {
    let mut out = std::collections::BTreeSet::new();
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
    for entry in walker.flatten() {
        if !entry.file_type().is_some_and(|ft| ft.is_file()) {
            continue;
        }
        if let Ok(rel) = entry.path().strip_prefix(project) {
            out.insert(rel.to_string_lossy().replace('\\', "/"));
        }
    }
    out
}

/// Extract with instrumentation for build logs and performance regression
/// checks. Unchanged source files reuse their previous parser output.
pub fn extract_context_with_stats(
    project: &Path,
) -> Result<(ProjectContext, ExtractionStats), String> {
    if !project.is_dir() {
        return Err(format!("'{}' is not a directory", project.display()));
    }

    let mut all_files: Vec<PathBuf> = Vec::new();
    let mut source_paths: Vec<(PathBuf, String)> = Vec::new();

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

    for entry in walker.flatten() {
        if !entry.file_type().is_some_and(|ft| ft.is_file()) {
            continue;
        }
        let path = entry.path();
        all_files.push(path.to_path_buf());
        // Gate: only files with a bundled grammar. A per-scope context that must
        // enumerate *every* file (configs, plain modules) would relax this — a
        // payload-completeness question deferred until the orchestrator needs it.
        if lang::ext_of(path)
            .and_then(lang::language_for_ext)
            .is_none()
        {
            continue;
        }
        let Ok(rel) = path.strip_prefix(project) else {
            continue;
        };
        let rel_path = rel.to_string_lossy().replace('\\', "/");
        // Gate: non-product files mint symbol nodes for code that carries no
        // architecture. Excluded deterministically here (structural, by
        // path/extension) so they never reach a modeling agent.
        if !is_modelable_file(&rel_path) {
            continue;
        }
        source_paths.push((path.to_path_buf(), rel_path));
    }
    let containers = manifest::discover_containers_from_files(project, &all_files);

    // Parsing dominates extraction on code-heavy repositories. Parse files in
    // parallel while reusing one parser per Rayon worker. Sorting afterward
    // preserves the extractor's deterministic output contract.
    let source_file_count = source_paths.len();
    let cache_hits = AtomicUsize::new(0);
    let parsed_files = AtomicUsize::new(0);
    let cache = PARSE_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut files: Vec<ParsedFile> = source_paths
        .into_par_iter()
        .filter_map(|(path, rel_path)| {
            let source = std::fs::read_to_string(&path).ok()?;
            let mut hasher = DefaultHasher::new();
            source.hash(&mut hasher);
            let content_hash = hasher.finish();
            let cached = cache
                .lock()
                .ok()
                .and_then(|entries| entries.get(&path).cloned())
                .filter(|(hash, _)| *hash == content_hash)
                .map(|(_, parse)| parse);
            let parse = match cached {
                Some(parse) => {
                    cache_hits.fetch_add(1, Ordering::Relaxed);
                    parse
                }
                None => {
                    let parse = PARSER.with(|parser| {
                        lang::parse_file_with(&path, &source, &mut parser.borrow_mut())
                    })?;
                    parsed_files.fetch_add(1, Ordering::Relaxed);
                    if let Ok(mut entries) = cache.lock() {
                        if entries.len() >= MAX_CACHED_FILES {
                            entries.clear();
                        }
                        entries.insert(path.clone(), (content_hash, parse.clone()));
                    }
                    parse
                }
            };
            (!parse.defs.is_empty()).then_some(ParsedFile {
                rel_path,
                parse,
                source,
            })
        })
        .collect();
    files.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));

    let project_name = project
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "project".to_string());

    let context = build_context(&project_name, &containers, &files);
    Ok((
        context,
        ExtractionStats {
            source_files: source_file_count,
            parsed_files: parsed_files.load(Ordering::Relaxed),
            cache_hits: cache_hits.load(Ordering::Relaxed),
        },
    ))
}

/// Deterministic exclusion of non-product source files — code that exists but
/// carries no architecture, so modeling it only inflates the node graph:
/// TypeScript declaration/mirror files (`*.d.ts`), test doubles in a `stubs/`
/// directory, and generated sources. Structural only (path + extension); the
/// significance of a *real* definition is the modeling agent's semantic call,
/// never decided here. NOTE: config files are deliberately NOT excluded — a
/// CMS/ORM collection config (e.g. Payload, Drizzle) declares the real data
/// model, so whether a config earns a symbol stays the agent's judgment.
fn is_modelable_file(rel_path: &str) -> bool {
    if rel_path.ends_with(".d.ts") {
        return false;
    }
    let mut segs = rel_path.split('/');
    if segs.any(|s| s == "stubs" || s == "generated" || s == "__generated__") {
        return false;
    }
    let file = rel_path.rsplit('/').next().unwrap_or(rel_path);
    if file.contains(".generated.") || file.contains(".gen.") {
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn excludes_non_product_files() {
        assert!(!is_modelable_file("docs/src/stubs/tauri.ts"));
        assert!(!is_modelable_file("src/types/api.d.ts"));
        assert!(!is_modelable_file("src/schema.generated.ts"));
        assert!(!is_modelable_file("app/__generated__/gql.ts"));
        // real product code stays
        assert!(is_modelable_file("crates/scryer-extract/src/manifest.rs"));
        assert!(is_modelable_file("src/App.tsx"));
        // config is NOT excluded — may declare a real data model
        assert!(is_modelable_file("docs/src/content.config.ts"));
        assert!(is_modelable_file("src/collections/Users.ts"));
    }

    /// Ad-hoc: dump the container/file/symbol/edge summary for an arbitrary repo.
    /// `REPO=/path cargo test -p scryer-extract dump_external -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn dump_external() {
        let repo = std::path::PathBuf::from(std::env::var("REPO").expect("set REPO"));
        let ctx = extract_context(&repo).expect("extraction");
        eprintln!("=== {} containers ===", ctx.containers.len());
        for c in &ctx.containers {
            let files = ctx
                .files
                .iter()
                .filter(|f| f.container_dir == c.dir)
                .count();
            let syms: usize = ctx
                .files
                .iter()
                .filter(|f| f.container_dir == c.dir)
                .map(|f| f.symbols.len())
                .sum();
            eprintln!(
                "  '{}' (dir='{}')  tech={:?}  files={}  symbols={}",
                c.name, c.dir, c.technology, files, syms
            );
        }
        let syms: usize = ctx.files.iter().map(|f| f.symbols.len()).sum();
        eprintln!(
            "totals: {} files, {} symbols, {} symbol-edges, {} file-edges",
            ctx.files.len(),
            syms,
            ctx.symbol_edges.len(),
            ctx.file_edges.len()
        );
    }

    /// Run extraction on this very repository and assert the context holds
    /// together: a project name, the workspace crates as containers, plenty of
    /// symbols, unique source-anchored keys, and edges that reference only real
    /// keys/files. No `ScryModel` is built — this layer emits a map, not a model.
    #[test]
    fn extracts_this_repo_cleanly() {
        // crates/scryer-extract -> repo root
        let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();

        let ctx = extract_context(&repo).expect("extraction");

        assert!(!ctx.project_name.is_empty(), "has a project name");
        assert!(
            ctx.containers.len() >= 5,
            "the workspace crates as containers (got {})",
            ctx.containers.len()
        );
        let symbols: usize = ctx.files.iter().map(|f| f.symbols.len()).sum();
        assert!(
            symbols > 100,
            "many symbols from a repo this size (got {symbols})"
        );

        eprintln!(
            "context: {} containers, {} files, {} symbols, {} symbol-edges, {} file-edges",
            ctx.containers.len(),
            ctx.files.len(),
            symbols,
            ctx.symbol_edges.len(),
            ctx.file_edges.len()
        );
        let (full_bytes, compact_bytes) = ctx.containers.iter().fold(
            (0usize, 0usize),
            |(full_total, compact_total), container| {
                let scope = slice_container(&ctx, &container.dir);
                let compact = compact_scope(&scope);
                eprintln!(
                    "  scope '{}': {} files, {} work units, {} bytes",
                    container.dir,
                    scope.files.len(),
                    compact.work_units(),
                    serde_json::to_vec(&compact).unwrap().len(),
                );
                (
                    full_total + serde_json::to_vec(&scope).unwrap().len(),
                    compact_total + serde_json::to_vec(&compact).unwrap().len(),
                )
            },
        );
        eprintln!(
            "prompt payload: {} -> {} bytes ({:.1}% smaller)",
            full_bytes,
            compact_bytes,
            100.0 - (compact_bytes as f64 / full_bytes.max(1) as f64 * 100.0),
        );

        // Files are emitted in rel_path order (determinism guarantee).
        let mut sorted = ctx.files.clone();
        sorted.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
        assert!(
            ctx.files
                .iter()
                .map(|f| &f.rel_path)
                .eq(sorted.iter().map(|f| &f.rel_path)),
            "files sorted by rel_path"
        );

        // Every symbol key is unique and source-anchored (never a node-N id).
        let keys: HashSet<&str> = ctx
            .files
            .iter()
            .flat_map(|f| f.symbols.iter().map(|s| s.key.as_str()))
            .collect();
        let key_count: usize = ctx.files.iter().map(|f| f.symbols.len()).sum();
        assert_eq!(keys.len(), key_count, "symbol keys are unique");
        assert!(
            keys.iter().all(|k| k.contains('#') && k.contains('@')),
            "keys are rel_path#name@line, not node ids"
        );

        // Every container_dir on a file resolves to a real container.
        let cdirs: HashSet<&str> = ctx.containers.iter().map(|c| c.dir.as_str()).collect();
        for f in &ctx.files {
            assert!(
                cdirs.contains(f.container_dir.as_str()),
                "file '{}' has unknown container '{}'",
                f.rel_path,
                f.container_dir
            );
        }

        // Edges reference only real keys / files (no dangling endpoints).
        for e in &ctx.symbol_edges {
            assert!(
                keys.contains(e.src.as_str()),
                "symbol edge src exists: {}",
                e.src
            );
            assert!(
                keys.contains(e.dst.as_str()),
                "symbol edge dst exists: {}",
                e.dst
            );
        }
        let rels: HashSet<&str> = ctx.files.iter().map(|f| f.rel_path.as_str()).collect();
        for e in &ctx.file_edges {
            assert!(
                rels.contains(e.src.as_str()),
                "file edge src exists: {}",
                e.src
            );
            assert!(
                rels.contains(e.dst.as_str()),
                "file edge dst exists: {}",
                e.dst
            );
        }

        // Slicing one crate yields a strict subset that still references real keys.
        let scope = "crates/scryer-extract";
        let scoped = slice_scope(&ctx, scope);
        assert!(!scoped.files.is_empty(), "the extract crate has files");
        assert!(
            scoped.files.iter().all(|f| f.rel_path.starts_with(scope)),
            "sliced files are all under the scope"
        );
        assert!(
            scoped.containers.iter().any(|c| c.dir == scope),
            "the scope's own container is present"
        );
    }

    #[test]
    fn repeated_extraction_reuses_unchanged_parses() {
        let dir = tempfile::tempdir().expect("temp dir");
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name":"cached-project"}"#,
        )
        .unwrap();
        std::fs::write(
            dir.path().join("main.ts"),
            "export function run() { return 1; }",
        )
        .unwrap();

        let (_, first) = extract_context_with_stats(dir.path()).expect("first extraction");
        let (_, second) = extract_context_with_stats(dir.path()).expect("second extraction");
        assert_eq!(first.parsed_files, 1);
        assert_eq!(second.parsed_files, 0);
        assert_eq!(second.cache_hits, 1);

        std::fs::write(
            dir.path().join("main.ts"),
            "export function run() { return 2; }",
        )
        .unwrap();
        let (_, changed) = extract_context_with_stats(dir.path()).expect("changed extraction");
        assert_eq!(changed.parsed_files, 1);
        assert_eq!(changed.cache_hits, 0);
    }
}
