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

pub mod context;
pub mod lang;
pub mod manifest;

pub use context::{
    build_context, slice_container, slice_scope, ContainerFacts, Edge, FileContext, ProjectContext,
    ScopeContext, SymbolContext,
};

use context::ParsedFile;
use scryer_core::scan;
use std::path::Path;

/// Walk a project directory and build its deterministic [`ProjectContext`].
pub fn extract_context(project: &Path) -> Result<ProjectContext, String> {
    if !project.is_dir() {
        return Err(format!("'{}' is not a directory", project.display()));
    }

    let containers = manifest::discover_containers(project);
    let mut files: Vec<ParsedFile> = Vec::new();

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
        // Gate: only files with a bundled grammar. A per-scope context that must
        // enumerate *every* file (configs, plain modules) would relax this — a
        // payload-completeness question deferred until the orchestrator needs it.
        if lang::ext_of(path).and_then(lang::language_for_ext).is_none() {
            continue;
        }
        let Ok(rel) = path.strip_prefix(project) else {
            continue;
        };
        let rel_path = rel.to_string_lossy().replace('\\', "/");
        let Ok(source) = std::fs::read_to_string(path) else {
            continue;
        };
        let Some(parse) = lang::parse_file(path, &source) else {
            continue;
        };
        // Gate: files with zero top-level definitions carry no symbols (see note
        // above — relaxing this is the same deferred completeness question).
        if parse.defs.is_empty() {
            continue;
        }
        files.push(ParsedFile { rel_path, parse });
    }

    files.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));

    let project_name = project
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "project".to_string());

    Ok(build_context(&project_name, &containers, &files))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

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
        assert!(symbols > 100, "many symbols from a repo this size (got {symbols})");

        eprintln!(
            "context: {} containers, {} files, {} symbols, {} symbol-edges, {} file-edges",
            ctx.containers.len(),
            ctx.files.len(),
            symbols,
            ctx.symbol_edges.len(),
            ctx.file_edges.len()
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
            assert!(keys.contains(e.src.as_str()), "symbol edge src exists: {}", e.src);
            assert!(keys.contains(e.dst.as_str()), "symbol edge dst exists: {}", e.dst);
        }
        let rels: HashSet<&str> = ctx.files.iter().map(|f| f.rel_path.as_str()).collect();
        for e in &ctx.file_edges {
            assert!(rels.contains(e.src.as_str()), "file edge src exists: {}", e.src);
            assert!(rels.contains(e.dst.as_str()), "file edge dst exists: {}", e.dst);
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
}
