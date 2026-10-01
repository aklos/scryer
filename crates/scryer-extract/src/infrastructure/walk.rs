//! Reading the project off disk: which files are there, and their parse
//! trees. The parser cache lives here because it is an artifact of reading —
//! the map built from these files is the domain's.

use crate::domain::context::ParsedFile;
use crate::domain::lang;
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
pub struct ParseStats {
    pub source_files: usize,
    pub parsed_files: usize,
    pub cache_hits: usize,
}

/// Every file under `project`, skipping the vendor/build directories the
/// extractor ignores.
pub fn walk_files(project: &Path) -> Vec<PathBuf> {
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
    walker
        .flatten()
        .filter(|entry| entry.file_type().is_some_and(|ft| ft.is_file()))
        .map(|entry| entry.path().to_path_buf())
        .collect()
}

/// The project-relative, forward-slashed paths of `walk_files`.
pub fn project_files(project: &Path) -> std::collections::BTreeSet<String> {
    walk_files(project)
        .iter()
        .filter_map(|path| path.strip_prefix(project).ok())
        .map(|rel| rel.to_string_lossy().replace('\\', "/"))
        .collect()
}

/// Parse every product source file among `all_files`, in parallel, reusing the
/// cache for unchanged content. Output is sorted, so extraction stays
/// deterministic.
pub fn parse_sources(project: &Path, all_files: &[PathBuf]) -> (Vec<ParsedFile>, ParseStats) {
    let mut source_paths: Vec<(PathBuf, String)> = Vec::new();
    for path in all_files {
        // Gate: only files with a bundled grammar. A per-scope context that must
        // enumerate *every* file (configs, plain modules) would relax this — a
        // payload-completeness question deferred until the orchestrator needs it.
        if !lang::ext_of(path).is_some_and(lang::supports_ext) {
            continue;
        }
        let Ok(rel) = path.strip_prefix(project) else {
            continue;
        };
        let rel_path = rel.to_string_lossy().replace('\\', "/");
        // Gate: non-product files mint symbol nodes for code that carries no
        // architecture. Excluded deterministically here (structural, by
        // path/extension) so they never reach a modeling agent. Shared with
        // drift scoping, so both surfaces agree on what counts as product code.
        if !scan::is_product_code(&rel_path) {
            continue;
        }
        source_paths.push((path.clone(), rel_path));
    }

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
    (
        files,
        ParseStats {
            source_files: source_file_count,
            parsed_files: parsed_files.load(Ordering::Relaxed),
            cache_hits: cache_hits.load(Ordering::Relaxed),
        },
    )
}
