//! Per-scope codebase CONTEXT: manifest facts + a per-file symbol index + a
//! dependency graph, all parser-only. This is the MAP a modeling agent reads
//! FROM to build a C4 model fast and correctly — it is never a model itself,
//! and it is never persisted to disk. [`build_context`] assembles the full
//! project context; [`slice_scope`] returns exactly the slice one subagent
//! needs to model a directory subtree (its files, their symbols, the symbol
//! edges internal to the scope, and the file edges crossing its boundary).
//!
//! Where a `ScryModel` mints `node-N` ids and freezes a component/symbol tree,
//! this layer deliberately does neither: symbols are addressed by a synthetic
//! `rel_path#name@line` key that anchors them to source without implying any C4
//! structure. Choosing components (clustering from cohesion + the dependency
//! graph) and writing responsibilities is the agent's job — we precompute the
//! map, not the model.

use crate::lang::{Def, FileParse};
use crate::tsconfig::TsAliases;
use crate::manifest::Container;
use serde::Serialize;
use std::collections::{BTreeSet, HashMap, HashSet};

/// A parsed source file, project-relative path with `/` separators.
pub struct ParsedFile {
    pub rel_path: String,
    pub parse: FileParse,
    /// Full source text — used to cut per-symbol evidence excerpts. May be
    /// empty (tests, synthetic inputs); symbols then carry no excerpt.
    pub source: String,
}

/// The full deterministic context for a project: manifest facts + a per-file
/// symbol index + a dependency graph. Built once per project, then sliced per
/// scope. A MAP the agent reads from — never a model, never persisted.
#[derive(Debug, Clone, Serialize)]
pub struct ProjectContext {
    pub project_name: String,
    /// Declared build/deploy units (manifest facts), sorted by `dir`.
    pub containers: Vec<ContainerFacts>,
    /// Per-file symbol index, in `rel_path` order. Only files that fall under a
    /// discovered container are included (see ownership note in `build_context`).
    pub files: Vec<FileContext>,
    /// Symbol→symbol dependency edges (keyed by `SymbolContext.key`), sorted.
    pub symbol_edges: Vec<Edge>,
    /// File→file dependency edges (keyed by `rel_path`), sorted.
    pub file_edges: Vec<Edge>,
    /// Non-serialized lookup indexes used to build container slices without
    /// rescanning the whole project graph for every agent job.
    #[serde(skip)]
    index: ContextIndex,
}

#[derive(Debug, Clone, Default)]
struct ContextIndex {
    files_by_container: HashMap<String, Vec<usize>>,
    file_container_by_path: HashMap<String, String>,
    symbol_edges_by_container: HashMap<String, Vec<usize>>,
    file_edges_by_container: HashMap<String, Vec<usize>>,
}

/// A declared build/deploy unit — a 1:1 projection of [`manifest::Container`]
/// onto the wire, carrying only the literal declared facts.
#[derive(Debug, Clone, Serialize)]
pub struct ContainerFacts {
    /// Directory relative to the project root, `/`-separated. Empty = root.
    pub dir: String,
    pub name: String,
    /// Literal declared technology (e.g. a Dockerfile base image). `None` when
    /// nothing is declared — naming the unit is the agent's job.
    pub technology: Option<String>,
    /// Directories of other containers this one declares a path dependency on.
    pub dep_dirs: Vec<String>,
}

/// One source file's symbols.
#[derive(Debug, Clone, Serialize)]
pub struct FileContext {
    pub rel_path: String,
    /// Owning container's directory (longest-prefix match; empty = root).
    pub container_dir: String,
    /// Symbols declared in the file, sorted by `(start_line, name)`.
    pub symbols: Vec<SymbolContext>,
}

/// One addressable symbol: a [`lang::Def`] plus a synthetic, source-anchored key.
#[derive(Debug, Clone, Serialize)]
pub struct SymbolContext {
    /// Stable in-payload identity: `rel_path#name@start_line`. NOT a `node-N`
    /// id — it anchors the symbol to source without implying C4 structure.
    pub key: String,
    pub name: String,
    /// 1-based inclusive line range of the whole definition.
    pub start_line: u32,
    pub end_line: u32,
    /// Declared field/variant names when this is a data shape; else empty.
    pub fields: Vec<String>,
    pub is_data_shape: bool,
    /// Source excerpt: contiguous doc/attribute lines above the definition plus
    /// the definition itself, capped at extraction time (no truncation marker —
    /// `excerpt_total_lines` says how long the real thing is). Carried for the
    /// compact Wave 2 payload only; skipped here so the full ScopeContext wire
    /// (drift checks) is unchanged.
    #[serde(skip)]
    pub excerpt: String,
    /// Full line count of doc block + definition, before any cap.
    #[serde(skip)]
    pub excerpt_total_lines: u32,
}

/// A directed dependency edge between two `key`s (symbol edges) or two
/// `rel_path`s (file edges).
#[derive(Debug, Clone, Serialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct Edge {
    pub src: String,
    pub dst: String,
}

fn symbol_key(rel_path: &str, name: &str, start_line: u32) -> String {
    format!("{}#{}@{}", rel_path, name, start_line)
}

/// Generous per-symbol excerpt cap applied at extraction time. Bounds the
/// memory a `ProjectContext` holds; the compact payload tightens further.
const EXCERPT_MAX_LINES: usize = 48;
const EXCERPT_MAX_BYTES: usize = 2_400;

/// Scope-level evidence budget for the compact Wave 2 payload. When the
/// excerpts of a scope sum past this, the per-symbol line cap steps down the
/// ladder until they fit (or the smallest cap is reached — splitting a scope
/// that is still too big at the smallest cap is A3's job, not truncation's).
const EVIDENCE_BUDGET_BYTES: usize = 300_000;
const EVIDENCE_LINE_LADDER: [usize; 5] = [48, 32, 20, 12, 6];

/// Cut the evidence excerpt for one definition: the contiguous run of
/// doc-comment / attribute / decorator lines immediately above it, then the
/// definition body, capped by lines and bytes. Returns the excerpt and the
/// uncapped line count (doc block + body).
fn extract_excerpt(lines: &[&str], start_line: u32, end_line: u32) -> (String, u32) {
    let start = (start_line as usize).saturating_sub(1);
    let end = (end_line as usize).min(lines.len());
    if start >= end {
        return (String::new(), 0);
    }

    // Doc comments, attributes, and decorators directly above the definition
    // are the highest-value evidence per byte — walk them in.
    let is_doc_line = |s: &str| {
        let t = s.trim_start();
        t.starts_with("//")
            || t.starts_with("/*")
            || t.starts_with('*')
            || t.starts_with('#')
            || t.starts_with('@')
            || t.starts_with("\"\"\"")
            || t.starts_with("'''")
    };
    let mut doc_start = start;
    while doc_start > 0
        && !lines[doc_start - 1].trim().is_empty()
        && is_doc_line(lines[doc_start - 1])
    {
        doc_start -= 1;
    }

    let total = end - doc_start;
    let mut out = String::new();
    let mut taken = 0usize;
    for line in &lines[doc_start..end] {
        if taken >= EXCERPT_MAX_LINES || out.len() + line.len() + 1 > EXCERPT_MAX_BYTES {
            break;
        }
        if taken > 0 {
            out.push('\n');
        }
        out.push_str(line);
        taken += 1;
    }
    (out, total as u32)
}

/// Render a symbol's `code` field at a given line cap, with a trailing
/// `… +N lines` marker when the definition continues past what is shown.
fn render_code(excerpt: &str, total_lines: u32, max_lines: usize) -> Option<String> {
    if excerpt.is_empty() {
        return None;
    }
    let lines: Vec<&str> = excerpt.lines().collect();
    let keep = lines.len().min(max_lines);
    let hidden = (total_lines as usize).saturating_sub(keep);
    let mut out = lines[..keep].join("\n");
    if hidden > 0 {
        out.push_str(&format!("\n… +{hidden} lines"));
    }
    Some(out)
}

/// Assemble the full project context from discovered containers + parsed files.
/// Inputs are exactly what [`crate::extract_context`] collects (`files` already
/// sorted by `rel_path`). Pure and deterministic: the same source always yields
/// the same context.
pub fn build_context(
    project_name: &str,
    containers: &[Container],
    files: &[ParsedFile],
    ts_aliases: &[TsAliases],
) -> ProjectContext {
    // Containers, sorted by dir for a stable order.
    let mut containers_sorted: Vec<&Container> = containers.iter().collect();
    containers_sorted.sort_by(|a, b| a.dir.cmp(&b.dir));
    let container_facts: Vec<ContainerFacts> = containers_sorted
        .iter()
        .map(|c| ContainerFacts {
            dir: c.dir.clone(),
            name: c.name.clone(),
            technology: c.technology.clone(),
            dep_dirs: c.dep_dirs.clone(),
        })
        .collect();

    // Per-file symbol index. A file is included only if it falls under a
    // discovered container (longest-prefix match) — the same ownership rule the
    // old model assembler used; an empty-string root container (the fallback
    // when nothing is declared) owns everything, so unowned files only occur
    // when containers are all subdirectories and a file sits above them all.
    // (Zero-def files were already dropped upstream by `extract_context`; that
    // and the supported-extension gate are payload-completeness questions
    // deferred to when the orchestrator exercises real scopes.)
    let mut file_ctxs: Vec<FileContext> = Vec::new();
    let mut recs: Vec<SymRec> = Vec::new();
    for f in files {
        let Some(cdir) = owning_container_dir(&f.rel_path, &containers_sorted) else {
            continue;
        };
        let source_lines: Vec<&str> = f.source.lines().collect();
        let mut defs: Vec<&Def> = f.parse.defs.iter().collect();
        defs.sort_by(|a, b| (a.start_line, &a.name).cmp(&(b.start_line, &b.name)));
        let mut symbols = Vec::with_capacity(defs.len());
        for def in defs {
            let key = symbol_key(&f.rel_path, &def.name, def.start_line);
            let (excerpt, excerpt_total_lines) =
                extract_excerpt(&source_lines, def.start_line, def.end_line);
            symbols.push(SymbolContext {
                key: key.clone(),
                name: def.name.clone(),
                start_line: def.start_line,
                end_line: def.end_line,
                fields: def.fields.clone(),
                is_data_shape: def.is_data_shape,
                excerpt,
                excerpt_total_lines,
            });
            recs.push(SymRec {
                key,
                name: def.name.clone(),
                file_rel: f.rel_path.clone(),
                container_dir: cdir.to_string(),
                start: def.start_line,
                end: def.end_line,
            });
        }
        file_ctxs.push(FileContext {
            rel_path: f.rel_path.clone(),
            container_dir: cdir.to_string(),
            symbols,
        });
    }

    let (symbol_edges, file_edges) = build_edges(files, &recs, containers, ts_aliases);

    let mut context = ProjectContext {
        project_name: project_name.to_string(),
        containers: container_facts,
        files: file_ctxs,
        symbol_edges,
        file_edges,
        index: ContextIndex::default(),
    };
    context.index = build_index(&context);
    context
}

fn build_index(ctx: &ProjectContext) -> ContextIndex {
    let mut index = ContextIndex::default();
    let mut file_container: HashMap<&str, &str> = HashMap::new();
    let mut symbol_container: HashMap<&str, &str> = HashMap::new();

    for (file_idx, file) in ctx.files.iter().enumerate() {
        index
            .files_by_container
            .entry(file.container_dir.clone())
            .or_default()
            .push(file_idx);
        file_container.insert(file.rel_path.as_str(), file.container_dir.as_str());
        index
            .file_container_by_path
            .insert(file.rel_path.clone(), file.container_dir.clone());
        for symbol in &file.symbols {
            symbol_container.insert(symbol.key.as_str(), file.container_dir.as_str());
        }
    }
    for (edge_idx, edge) in ctx.symbol_edges.iter().enumerate() {
        let Some(container) = symbol_container.get(edge.src.as_str()) else {
            continue;
        };
        if symbol_container.get(edge.dst.as_str()) == Some(container) {
            index
                .symbol_edges_by_container
                .entry((*container).to_string())
                .or_default()
                .push(edge_idx);
        }
    }
    for (edge_idx, edge) in ctx.file_edges.iter().enumerate() {
        let src = file_container.get(edge.src.as_str()).copied();
        let dst = file_container.get(edge.dst.as_str()).copied();
        if let Some(container) = src {
            index
                .file_edges_by_container
                .entry(container.to_string())
                .or_default()
                .push(edge_idx);
        }
        if let Some(container) = dst.filter(|dst| Some(*dst) != src) {
            index
                .file_edges_by_container
                .entry(container.to_string())
                .or_default()
                .push(edge_idx);
        }
    }
    index
}

/// Provenance of one emitted symbol, retained to resolve the dependency graph.
struct SymRec {
    key: String,
    name: String,
    file_rel: String,
    container_dir: String,
    start: u32,
    end: u32,
}

/// Resolve identifier occurrences into dependency edges, keyed by the synthetic
/// symbol `key`. A reference contributes an edge only when its name resolves
/// *unambiguously* in scope (declared exactly once) — names declared more than
/// once (`new`, `build`, …) are skipped, which removes most false edges.
/// Symbols resolve within their file; files resolve within their container, so a
/// file using another file's symbol yields a file→file edge. Cross-container
/// resolution rides the languages' declared forms: Rust qualified paths
/// (`PathRef`, via the crate map) and TS/JS imports (`ImportRef`, via the
/// package map and file-path resolution). Edges are intentionally
/// UNDER-reported: absence of an edge is not absence of a dependency, and
/// resolution is weaker for the generic-fallback languages
/// (go/java/ruby/c/cpp/c#/php), where only coarse definitions and bare
/// identifiers are seen.
/// Bare identifier names that overwhelmingly denote universal trait/inherent
/// methods or constructors. A name-only reference to one of these is noise, not
/// a dependency (see the skip in `build_edges`).
const UNIVERSAL_NAMES: &[&str] = &[
    "new",
    "default",
    "build",
    "from",
    "into",
    "clone",
    "to_string",
    "to_owned",
    "as_str",
    "as_ref",
    "as_bytes",
    "len",
    "is_empty",
    "unwrap",
    "expect",
    "parse",
    "get",
    "insert",
    "push",
    "iter",
    "into_iter",
    "contains",
    "with_capacity",
    "next",
    "collect",
];

/// Path-head segments that never name a workspace container: language keywords
/// and the standard library roots. Skipped before the crate-map lookup.
const PATH_BUILTINS: &[&str] = &["crate", "self", "super", "std", "core", "alloc"];

fn build_edges(
    files: &[ParsedFile],
    recs: &[SymRec],
    containers: &[Container],
    ts_aliases: &[TsAliases],
) -> (Vec<Edge>, Vec<Edge>) {
    let mut ranges_by_file: HashMap<&str, Vec<(u32, u32, &str)>> = HashMap::new();
    let mut container_of_file: HashMap<&str, &str> = HashMap::new();
    let mut file_names: HashMap<&str, HashMap<&str, Vec<&str>>> = HashMap::new();
    let mut cont_names: HashMap<&str, HashMap<&str, Vec<(&str, &str)>>> = HashMap::new();

    // Manifest map: a `use`/path head segment -> the container it names. Rust
    // source spells `scryer-extract` as `scryer_extract`, so normalize hyphens.
    // This is what lets a reference resolve ACROSS containers, not just within
    // one — the gap the same-container scope below cannot close.
    let mut crate_to_dir: HashMap<String, &str> = HashMap::new();
    for c in containers {
        crate_to_dir.insert(c.name.replace('-', "_"), c.dir.as_str());
    }

    // The TS/JS counterpart: a bare import spec's package name -> container.
    // VERBATIM declared names (npm names keep `@scope/` and hyphens), plus the
    // file inventory for resolving relative and subpath specs to actual files.
    let mut pkg_to_dir: HashMap<&str, &str> = HashMap::new();
    for c in containers {
        pkg_to_dir.insert(c.name.as_str(), c.dir.as_str());
    }
    let inventory: HashSet<&str> = files.iter().map(|f| f.rel_path.as_str()).collect();

    // Every directory holding files — lets a Python module path resolve to a
    // PACKAGE (for submodule fallback) even when its `__init__.py` carries no
    // defs and was dropped upstream.
    let mut package_dirs: HashSet<String> = HashSet::new();
    for f in files {
        let mut p = f.rel_path.as_str();
        while let Some((dir, _)) = p.rsplit_once('/') {
            if !package_dirs.insert(dir.to_string()) {
                break; // ancestors already inserted
            }
            p = dir;
        }
    }

    for r in recs {
        ranges_by_file
            .entry(r.file_rel.as_str())
            .or_default()
            .push((r.start, r.end, r.key.as_str()));
        container_of_file.insert(r.file_rel.as_str(), r.container_dir.as_str());
        file_names
            .entry(r.file_rel.as_str())
            .or_default()
            .entry(r.name.as_str())
            .or_default()
            .push(r.key.as_str());
        cont_names
            .entry(r.container_dir.as_str())
            .or_default()
            .entry(r.name.as_str())
            .or_default()
            .push((r.key.as_str(), r.file_rel.as_str()));
    }

    let enclosing = |file: &str, line: u32| -> Option<&str> {
        ranges_by_file
            .get(file)?
            .iter()
            .filter(|(s, e, _)| *s <= line && line <= *e)
            .min_by_key(|(s, e, _)| e - s)
            .map(|(_, _, k)| *k)
    };

    // Module name -> file within each container, for imports whose leaf names a
    // module rather than a def (`use scryer_core::drift;`). Lets the container
    // coupling surface as a file edge even when no symbol is named.
    let mut mod_files: HashMap<&str, HashMap<&str, Vec<&str>>> = HashMap::new();
    for (&file, &dir) in &container_of_file {
        mod_files
            .entry(dir)
            .or_default()
            .entry(module_name(file))
            .or_default()
            .push(file);
    }

    let mut sym_edges: HashSet<(String, String)> = HashSet::new();
    let mut file_edges: HashSet<(String, String)> = HashSet::new();

    for f in files {
        let file = f.rel_path.as_str();
        let container_dir = container_of_file.get(file).copied();

        // The nearest (longest-dir) tsconfig governs this file's bare specs.
        let alias_config = ts_aliases
            .iter()
            .filter(|a| a.dir.is_empty() || file.starts_with(&format!("{}/", a.dir)))
            .max_by_key(|a| a.dir.len());

        // --- TS/JS/Python imports: the languages' declared cross-file/container form ---
        // Resolve each import to a target file (relative specs against the
        // importing file's directory; bare specs through the package map +
        // subpath; Python module paths against the container/src/root roots),
        // emit the file edge, and index the LOCAL bindings so the ident pass
        // below can attach symbol edges at the usage sites — the import line
        // itself is module-scoped, exactly like a Rust `use`. Every recognized
        // import consumes its locals even when unresolved (external package,
        // unparsed target): the name is lexically bound to another module, so
        // letting it fall through to the bare-name scopes would misattribute
        // it to a same-name local def.
        let is_python = file.ends_with(".py") || file.ends_with(".pyi");
        let mut imported_locals: HashMap<&str, Option<&str>> = HashMap::new();
        for imp in &f.parse.imports {
            let mut target_file: Option<&str> = None;
            let mut target_dir: Option<&str> = None;
            // Python only: the module path as a directory base, kept even when
            // no module FILE resolved, for the submodule fallback below.
            let mut module_base: Option<String> = None;
            if is_python {
                (target_file, module_base) = resolve_py_import(
                    file,
                    &imp.spec,
                    container_dir,
                    &crate_to_dir,
                    &inventory,
                    &package_dirs,
                );
            } else if imp.spec.starts_with('.') {
                target_file = resolve_relative(file, &imp.spec)
                    .and_then(|base| find_module_file(&base, &inventory));
            } else if !imp.spec.starts_with('/') {
                // tsconfig `paths`/`baseUrl` first — TS applies them before
                // package resolution — then the declared package map.
                target_file =
                    alias_config.and_then(|cfg| resolve_ts_alias(&imp.spec, cfg, &inventory));
                if target_file.is_none() {
                    let (pkg, subpath) = split_package_spec(&imp.spec);
                    match pkg_to_dir.get(pkg) {
                        // Self-referencing package import: the same-container
                        // scopes above already cover it — don't consume.
                        Some(&dir) if Some(dir) == container_dir => continue,
                        Some(&dir) => {
                            target_dir = Some(dir);
                            target_file = find_package_file(dir, subpath, &inventory);
                        }
                        None => {} // external package: consume locals, no edges
                    }
                }
            }
            if let Some(t) = target_file.filter(|t| *t != file) {
                file_edges.insert((file.to_string(), t.to_string()));
            }
            for n in &imp.names {
                // Resolve in the target module file when one was found, else
                // container-wide — the unique-or-skip discipline throughout.
                let hit = target_file
                    .and_then(|t| file_names.get(t))
                    .and_then(|m| m.get(n.name.as_str()))
                    .filter(|d| d.len() == 1)
                    .map(|d| (d[0], None))
                    .or_else(|| {
                        target_dir
                            .and_then(|dir| cont_names.get(dir))
                            .and_then(|m| m.get(n.name.as_str()))
                            .filter(|c| c.len() == 1)
                            .map(|c| (c[0].0, Some(c[0].1)))
                    });
                match hit {
                    Some((dst_key, dst_file)) => {
                        if let Some(df) = dst_file.filter(|df| *df != file) {
                            file_edges.insert((file.to_string(), df.to_string()));
                        }
                        imported_locals.insert(n.local.as_str(), Some(dst_key));
                    }
                    None => {
                        // Python: the imported name may be a SUBMODULE, not a
                        // symbol (`from . import sibling`, `from pkg import
                        // mod`) — file-level evidence to that module's file.
                        if let Some(base) = &module_base {
                            let sub = if base.is_empty() {
                                n.name.clone()
                            } else {
                                format!("{base}/{}", n.name)
                            };
                            if let Some(subfile) = py_module_file(&sub, &inventory) {
                                if subfile != file {
                                    file_edges.insert((file.to_string(), subfile.to_string()));
                                }
                            }
                        }
                        imported_locals.insert(n.local.as_str(), None);
                    }
                }
            }
        }

        for ident in &f.parse.idents {
            // Imported locals take precedence over every bare-name scope: the
            // binding is a per-file lexical fact, so it even overrides the
            // universal-name skip (`import { get } from "./api"` is exact
            // evidence in a way a bare `get` is not).
            if let Some(&dst) = imported_locals.get(ident.name.as_str()) {
                if let (Some(dst_key), Some(src)) = (dst, enclosing(file, ident.line)) {
                    if src != dst_key {
                        sym_edges.insert((src.to_string(), dst_key.to_string()));
                    }
                }
                continue;
            }
            // Universal trait/inherent method & constructor names (`Vec::new`,
            // `x.clone()`, `T::from`, …) carry no dependency signal: the name-only
            // resolver can't tell `ScryModel::new` from `Vec::new`, so a single
            // captured def with one of these names would falsely absorb every call
            // site in the container. Link evidence deliberately under-reports
            // (absence of an edge ≠ absence of a dependency), so skip them.
            if UNIVERSAL_NAMES.contains(&ident.name.as_str()) {
                continue;
            }
            if let Some(defs) = file_names
                .get(file)
                .and_then(|m| m.get(ident.name.as_str()))
            {
                if defs.len() == 1 {
                    let dst = defs[0];
                    if let Some(src) = enclosing(file, ident.line) {
                        if src != dst {
                            sym_edges.insert((src.to_string(), dst.to_string()));
                        }
                    }
                }
            }
            if let Some(cd) = container_dir {
                if let Some(cands) = cont_names.get(cd).and_then(|m| m.get(ident.name.as_str())) {
                    if cands.len() == 1 {
                        let (dst_key, dst_file) = cands[0];
                        if dst_file != file {
                            file_edges.insert((file.to_string(), dst_file.to_string()));
                            // The reference resolved uniquely to a symbol in
                            // ANOTHER file of the same container. Record the
                            // symbol→symbol edge too, not just the file pairing:
                            // cross-file deps are the dominant form of component
                            // coupling (one file ≈ one module), and both the link
                            // audit and commit-time link derivation join on symbol
                            // anchors, so without this they never see it.
                            if let Some(src) = enclosing(file, ident.line) {
                                if src != dst_key {
                                    sym_edges.insert((src.to_string(), dst_key.to_string()));
                                }
                            }
                        }
                    }
                }
            }
        }

        // --- third scope: CROSS-container references via the manifest map ---
        // The two scopes above resolve names only within the file's own
        // container. A qualified path (`scryer_extract::anchors::write_baseline`
        // or a `use` of it) names its target container in the head segment, so
        // we can follow it across the boundary — the one resolution the
        // name-only design deliberately ducked. Same unique-or-skip discipline:
        // an ambiguous leaf yields no edge.
        for pref in &f.parse.paths {
            let head = pref.segments.first().map(String::as_str).unwrap_or("");
            if PATH_BUILTINS.contains(&head) {
                continue;
            }
            let Some(&dst_dir) = crate_to_dir.get(head) else {
                continue; // head names an external crate or nothing we model
            };
            if Some(dst_dir) == container_dir {
                continue; // same container — already covered above
            }
            // The resolvable symbol is the last segment that isn't a universal
            // method/constructor name: `ExtentResolver::new` -> `ExtentResolver`.
            let Some(sym) = pref.segments[1..]
                .iter()
                .map(String::as_str)
                .rev()
                .find(|s| !UNIVERSAL_NAMES.contains(s))
            else {
                continue;
            };
            match cont_names.get(dst_dir).and_then(|m| m.get(sym)) {
                Some(cands) if cands.len() == 1 => {
                    let (dst_key, dst_file) = cands[0];
                    file_edges.insert((file.to_string(), dst_file.to_string()));
                    // Symbol edge from the enclosing function when the reference
                    // sits at a real call site (a `use` line is module-scoped, so
                    // `enclosing` returns None and only the file edge stands).
                    // The commit-time link audit joins on symbol anchors, so this
                    // is what makes a cross-crate dependency count as evidence.
                    if let Some(src) = enclosing(file, pref.line) {
                        if src != dst_key {
                            sym_edges.insert((src.to_string(), dst_key.to_string()));
                        }
                    }
                }
                Some(_) => {} // ambiguous leaf — skip (under-report, never guess)
                None => {
                    // The leaf names a module, not a def (`use scryer_core::drift;`).
                    // Resolve to that module's file so the coupling still shows up.
                    if let Some(cands) = mod_files.get(dst_dir).and_then(|m| m.get(sym)) {
                        if cands.len() == 1 && cands[0] != file {
                            file_edges.insert((file.to_string(), cands[0].to_string()));
                        }
                    }
                }
            }
        }
    }

    (sorted_edges(sym_edges), sorted_edges(file_edges))
}

/// The module name a Rust file contributes: its stem, except `mod.rs`/`lib.rs`/
/// `main.rs` which take the parent directory's name (that's the module path
/// segment a `use` would spell).
fn module_name(file_rel: &str) -> &str {
    let name = file_rel.rsplit('/').next().unwrap_or(file_rel);
    let stem = name.strip_suffix(".rs").unwrap_or(name);
    if matches!(stem, "mod" | "lib" | "main") {
        let parent = file_rel
            .strip_suffix(name)
            .and_then(|p| p.trim_end_matches('/').rsplit('/').next())
            .unwrap_or("");
        if !parent.is_empty() {
            return parent;
        }
    }
    stem
}

/// Known TS/JS source extensions, in resolution priority order.
const TS_EXTS: &[&str] = &["ts", "tsx", "js", "jsx", "mts", "cts", "mjs", "cjs"];

/// Resolve a relative import spec against the importing file's location into a
/// normalized project-relative base path (extension handling is
/// [`find_module_file`]'s job). `None` when `..` escapes the project root.
fn resolve_relative(file_rel: &str, spec: &str) -> Option<String> {
    let dir = file_rel.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
    let mut parts: Vec<&str> = if dir.is_empty() {
        Vec::new()
    } else {
        dir.split('/').collect()
    };
    for comp in spec.split('/') {
        match comp {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            other => parts.push(other),
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

/// Find the file a TS/JS module base path denotes, in the candidate order
/// Node/bundlers use: the exact path, the `.ts` twins of an emitted-extension
/// spec (NodeNext code writes `./x.js` for the source `./x.ts`), bare source
/// extensions, then directory index files. First hit wins — the same
/// deterministic preference a bundler applies.
fn find_module_file<'a>(base: &str, inventory: &HashSet<&'a str>) -> Option<&'a str> {
    let hit = |c: String| inventory.get(c.as_str()).copied();
    if let Some(f) = hit(base.to_string()) {
        return Some(f);
    }
    for (emitted, twins) in [
        ("js", &["ts", "tsx"][..]),
        ("mjs", &["mts"][..]),
        ("cjs", &["cts"][..]),
    ] {
        if let Some(stem) = base.strip_suffix(&format!(".{emitted}")) {
            if let Some(f) = twins.iter().find_map(|t| hit(format!("{stem}.{t}"))) {
                return Some(f);
            }
        }
    }
    for ext in TS_EXTS {
        if let Some(f) = hit(format!("{base}.{ext}")) {
            return Some(f);
        }
    }
    for ext in TS_EXTS {
        if let Some(f) = hit(format!("{base}/index.{ext}")) {
            return Some(f);
        }
    }
    None
}

/// Find the module file a bare package spec denotes inside the target
/// container: `dir/<subpath>`, then under `dir/src/` (the dominant monorepo
/// source layout); a spec with no subpath falls to the index-file candidates.
/// `None` is fine — symbol names still resolve container-wide.
fn find_package_file<'a>(dir: &str, subpath: &str, inventory: &HashSet<&'a str>) -> Option<&'a str> {
    let join = |a: &str, b: &str| {
        if a.is_empty() {
            b.to_string()
        } else {
            format!("{a}/{b}")
        }
    };
    let bases = if subpath.is_empty() {
        vec![dir.to_string(), join(dir, "src")]
    } else {
        vec![join(dir, subpath), join(&join(dir, "src"), subpath)]
    };
    bases.iter().find_map(|b| find_module_file(b, inventory))
}

/// Resolve a bare spec through a governing tsconfig: `paths` patterns first
/// (an exact pattern beats a `*` pattern; among `*` patterns the longest
/// matched prefix wins, as tsc picks), each target tried in declared order
/// with `*` substituted; then a `baseUrl`-relative file. `None` falls through
/// to package resolution.
fn resolve_ts_alias<'a>(
    spec: &str,
    config: &TsAliases,
    inventory: &HashSet<&'a str>,
) -> Option<&'a str> {
    let mut best: Option<(usize, String, &[String])> = None; // (specificity, captured, targets)
    for (pattern, targets) in &config.paths {
        match pattern.split_once('*') {
            None => {
                if pattern == spec {
                    best = Some((usize::MAX, String::new(), targets));
                }
            }
            Some((prefix, suffix)) => {
                if spec.len() >= prefix.len() + suffix.len()
                    && spec.starts_with(prefix)
                    && spec.ends_with(suffix)
                    && best.as_ref().is_none_or(|(l, ..)| prefix.len() > *l)
                {
                    let captured = spec[prefix.len()..spec.len() - suffix.len()].to_string();
                    best = Some((prefix.len(), captured, targets));
                }
            }
        }
    }
    if let Some((_, captured, targets)) = best {
        for target in targets {
            let base = target.replacen('*', &captured, 1);
            if let Some(f) = find_module_file(&base, inventory) {
                return Some(f);
            }
        }
    }
    let base_url = config.base_url.as_deref()?;
    let base = if base_url.is_empty() {
        spec.to_string()
    } else {
        format!("{base_url}/{spec}")
    };
    find_module_file(&base, inventory)
}

/// Resolve a Python module spec from `file` into `(module file, module base
/// path)`. Relative specs climb one directory per extra leading dot; absolute
/// specs try the importing container's roots (its dir, then `dir/src` for the
/// src layout), the project root, and — via the declared package-name map —
/// the named container's roots. The base comes back even when no module FILE
/// exists (a defs-less `__init__.py` never enters the inventory) so
/// `from pkg import name` can still resolve `name` as a submodule.
fn resolve_py_import<'a>(
    file: &str,
    spec: &str,
    container_dir: Option<&str>,
    crate_to_dir: &HashMap<String, &'a str>,
    inventory: &HashSet<&'a str>,
    package_dirs: &HashSet<String>,
) -> (Option<&'a str>, Option<String>) {
    let join = |root: &str, path: &str| {
        if root.is_empty() {
            path.to_string()
        } else {
            format!("{root}/{path}")
        }
    };
    if let Some(stripped) = spec.strip_prefix('.') {
        let level = 1 + stripped.bytes().take_while(|b| *b == b'.').count();
        let rest = &spec[level..];
        let dir = file.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
        let mut parts: Vec<&str> = if dir.is_empty() {
            Vec::new()
        } else {
            dir.split('/').collect()
        };
        for _ in 1..level {
            if parts.pop().is_none() {
                return (None, None); // climbed past the project root
            }
        }
        let base = if rest.is_empty() {
            parts.join("/")
        } else {
            join(&parts.join("/"), &rest.replace('.', "/"))
        };
        let target = py_module_file(&base, inventory);
        let known = target.is_some() || package_dirs.contains(&base);
        return (target, known.then_some(base));
    }
    let path = spec.replace('.', "/");
    let head = spec.split('.').next().unwrap_or("");
    let mut roots: Vec<String> = Vec::new();
    if let Some(cd) = container_dir {
        roots.push(cd.to_string());
        roots.push(join(cd, "src"));
    }
    roots.push(String::new());
    if let Some(&dst) = crate_to_dir.get(head) {
        roots.push(dst.to_string());
        roots.push(join(dst, "src"));
    }
    let mut seen: HashSet<&str> = HashSet::new();
    for root in &roots {
        if !seen.insert(root.as_str()) {
            continue;
        }
        let base = join(root, &path);
        if let Some(f) = py_module_file(&base, inventory) {
            return (Some(f), Some(base));
        }
        if package_dirs.contains(&base) {
            return (None, Some(base));
        }
    }
    (None, None) // external package: consume locals, no edges
}

/// The file a Python module base denotes: the module itself (`a/b` ->
/// `a/b.py`), its stub, or the package's `__init__`.
fn py_module_file<'a>(base: &str, inventory: &HashSet<&'a str>) -> Option<&'a str> {
    let hit = |c: String| inventory.get(c.as_str()).copied();
    hit(format!("{base}.py"))
        .or_else(|| hit(format!("{base}.pyi")))
        .or_else(|| hit(format!("{base}/__init__.py")))
        .or_else(|| hit(format!("{base}/__init__.pyi")))
}

/// Split a bare import spec into (package name, subpath): `@acme/ui/button` ->
/// `("@acme/ui", "button")`, `lodash` -> `("lodash", "")`.
fn split_package_spec(spec: &str) -> (&str, &str) {
    let mut slashes = spec.match_indices('/').map(|(i, _)| i);
    let cut = if spec.starts_with('@') {
        slashes.nth(1)
    } else {
        slashes.next()
    };
    match cut {
        Some(i) => (&spec[..i], &spec[i + 1..]),
        None => (spec, ""),
    }
}

fn sorted_edges(set: HashSet<(String, String)>) -> Vec<Edge> {
    let mut v: Vec<Edge> = set
        .into_iter()
        .map(|(src, dst)| Edge { src, dst })
        .collect();
    v.sort();
    v
}

/// One subagent's slice of the project context: everything needed to model the
/// directory subtree `scope` (a container dir, or any subdirectory) without the
/// rest of the repo. Files strictly under the scope, the containers relevant to
/// it (the enclosing container plus any nested under it), the symbol edges
/// internal to the scope, and the file edges partitioned by how they cross the
/// scope boundary — so the agent sees external dependencies it must *link to*
/// but not *model here*.
#[derive(Debug, Clone, Serialize)]
pub struct ScopeContext {
    pub scope: String,
    pub containers: Vec<ContainerFacts>,
    pub files: Vec<FileContext>,
    pub internal_symbol_edges: Vec<Edge>,
    pub internal_file_edges: Vec<Edge>,
    /// Edges from a file in scope to a file outside it (this scope depends on …).
    pub outbound_file_edges: Vec<Edge>,
    /// Edges from a file outside the scope to a file in it (… depends on this scope).
    pub inbound_file_edges: Vec<Edge>,
}

/// Token-efficient wire format for one modeling agent. Paths and symbols are
/// interned once; graph edges use integer ids rather than repeating long
/// `path#symbol@line` strings. The full [`ScopeContext`] remains available for
/// inspection and drift workflows.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptScopeContext {
    pub scope: String,
    /// String table for every file referenced by this scope.
    pub paths: Vec<String>,
    pub files: Vec<PromptFile>,
    pub symbol_edges: Vec<[u32; 2]>,
    pub file_edges: PromptFileEdges,
}

#[derive(Debug, Clone, Serialize)]
pub struct PromptFile {
    /// Index into `paths`.
    pub path: u32,
    pub symbols: Vec<PromptSymbol>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PromptSymbol {
    /// Scope-global symbol id used by `symbolEdges`.
    pub id: u32,
    pub name: String,
    /// Inclusive start/end lines.
    pub lines: [u32; 2],
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<String>,
    #[serde(skip_serializing_if = "is_false")]
    pub data: bool,
    /// Source excerpt: doc comment + signature + leading body. A trailing
    /// `… +N lines` marker means the definition continues in the file.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Debug, Clone, Serialize)]
pub struct PromptFileEdges {
    pub internal: Vec<[u32; 2]>,
    pub outbound: Vec<[u32; 2]>,
    pub inbound: Vec<[u32; 2]>,
}

impl PromptScopeContext {
    /// Rough scheduling weight proportional to the amount of evidence the agent
    /// must reason over. It is deterministic and cheap to compute.
    pub fn work_units(&self) -> usize {
        let symbols: usize = self.files.iter().map(|f| f.symbols.len()).sum();
        let edges = self.symbol_edges.len()
            + self.file_edges.internal.len()
            + self.file_edges.outbound.len()
            + self.file_edges.inbound.len();
        self.files.len() * 4 + symbols * 3 + edges
    }
}

/// Convert a scope into the compact agent wire format.
pub fn compact_scope(scope: &ScopeContext) -> PromptScopeContext {
    let mut all_paths: BTreeSet<&str> = scope.files.iter().map(|f| f.rel_path.as_str()).collect();
    for edge in scope
        .internal_file_edges
        .iter()
        .chain(&scope.outbound_file_edges)
        .chain(&scope.inbound_file_edges)
    {
        all_paths.insert(edge.src.as_str());
        all_paths.insert(edge.dst.as_str());
    }
    let paths: Vec<String> = all_paths.into_iter().map(str::to_string).collect();
    let path_ids: HashMap<String, u32> = paths
        .iter()
        .enumerate()
        .map(|(idx, path)| (path.clone(), idx as u32))
        .collect();

    // Payload-size guard: pick the largest per-symbol line cap whose total
    // evidence fits the scope budget. Most scopes fit at the full cap; a huge
    // one degrades toward signature+doc-only rather than blowing the prompt.
    let line_cap = EVIDENCE_LINE_LADDER
        .iter()
        .copied()
        .find(|cap| {
            let total: usize = scope
                .files
                .iter()
                .flat_map(|f| &f.symbols)
                .filter_map(|s| render_code(&s.excerpt, s.excerpt_total_lines, *cap))
                .map(|code| code.len())
                .sum();
            total <= EVIDENCE_BUDGET_BYTES
        })
        .unwrap_or(*EVIDENCE_LINE_LADDER.last().unwrap());

    let mut symbol_ids: HashMap<&str, u32> = HashMap::new();
    let mut next_symbol = 0u32;
    let files = scope
        .files
        .iter()
        .map(|file| {
            let symbols = file
                .symbols
                .iter()
                .map(|symbol| {
                    let id = next_symbol;
                    next_symbol += 1;
                    symbol_ids.insert(symbol.key.as_str(), id);
                    PromptSymbol {
                        id,
                        name: symbol.name.clone(),
                        lines: [symbol.start_line, symbol.end_line],
                        fields: symbol.fields.clone(),
                        data: symbol.is_data_shape,
                        code: render_code(&symbol.excerpt, symbol.excerpt_total_lines, line_cap),
                    }
                })
                .collect();
            PromptFile {
                path: path_ids[file.rel_path.as_str()],
                symbols,
            }
        })
        .collect();

    let symbol_edges = scope
        .internal_symbol_edges
        .iter()
        .filter_map(|edge| {
            Some([
                *symbol_ids.get(edge.src.as_str())?,
                *symbol_ids.get(edge.dst.as_str())?,
            ])
        })
        .collect();
    let encode_file_edges = |edges: &[Edge]| {
        edges
            .iter()
            .map(|edge| [path_ids[edge.src.as_str()], path_ids[edge.dst.as_str()]])
            .collect()
    };

    PromptScopeContext {
        scope: scope.scope.clone(),
        paths,
        files,
        symbol_edges,
        file_edges: PromptFileEdges {
            internal: encode_file_edges(&scope.internal_file_edges),
            outbound: encode_file_edges(&scope.outbound_file_edges),
            inbound: encode_file_edges(&scope.inbound_file_edges),
        },
    }
}

/// True when `path` is at or under directory `scope`. An empty scope is the
/// whole project.
pub fn is_under(path: &str, scope: &str) -> bool {
    scope.is_empty() || path == scope || path.starts_with(&format!("{}/", scope))
}

/// Slice the full project context down to a directory scope (a raw path prefix).
/// An empty scope is the whole project. For modeling one container, prefer
/// [`slice_container`], which slices by ownership so a root container does not
/// swallow nested containers' files.
pub fn slice_scope(ctx: &ProjectContext, scope: &str) -> ScopeContext {
    let files: Vec<FileContext> = ctx
        .files
        .iter()
        .filter(|f| is_under(&f.rel_path, scope))
        .cloned()
        .collect();
    // Containers relevant to the scope: the enclosing container(s) and any
    // nested under the scope. `is_under(scope, c.dir)` catches the enclosing
    // container (incl. the empty-string root), `is_under(c.dir, scope)` the
    // nested ones.
    let containers: Vec<ContainerFacts> = ctx
        .containers
        .iter()
        .filter(|c| is_under(&c.dir, scope) || is_under(scope, &c.dir))
        .cloned()
        .collect();
    build_scope(ctx, scope.to_string(), files, containers)
}

/// Slice the context to the files OWNED by one container (the longest-prefix
/// ownership rule used in [`build_context`], NOT a raw path prefix — so a root
/// container with `dir == ""` gets only its own top-level files, not the whole
/// repo). This is the per-container scope Wave 2 modeling consumes.
pub fn slice_container(ctx: &ProjectContext, container_dir: &str) -> ScopeContext {
    let files: Vec<FileContext> = ctx
        .index
        .files_by_container
        .get(container_dir)
        .into_iter()
        .flatten()
        .map(|&idx| ctx.files[idx].clone())
        .collect();
    let containers: Vec<ContainerFacts> = ctx
        .containers
        .iter()
        .filter(|c| c.dir == container_dir)
        .cloned()
        .collect();
    build_container_scope(ctx, container_dir.to_string(), files, containers)
}

fn build_container_scope(
    ctx: &ProjectContext,
    scope: String,
    files: Vec<FileContext>,
    containers: Vec<ContainerFacts>,
) -> ScopeContext {
    let internal_symbol_edges = ctx
        .index
        .symbol_edges_by_container
        .get(scope.as_str())
        .into_iter()
        .flatten()
        .map(|&idx| ctx.symbol_edges[idx].clone())
        .collect();
    let mut internal_file_edges = Vec::new();
    let mut outbound_file_edges = Vec::new();
    let mut inbound_file_edges = Vec::new();
    for &idx in ctx
        .index
        .file_edges_by_container
        .get(scope.as_str())
        .into_iter()
        .flatten()
    {
        let edge = &ctx.file_edges[idx];
        let src_inside = ctx
            .index
            .file_container_by_path
            .get(edge.src.as_str())
            .is_some_and(|container| container == &scope);
        let dst_inside = ctx
            .index
            .file_container_by_path
            .get(edge.dst.as_str())
            .is_some_and(|container| container == &scope);
        match (src_inside, dst_inside) {
            (true, true) => internal_file_edges.push(edge.clone()),
            (true, false) => outbound_file_edges.push(edge.clone()),
            (false, true) => inbound_file_edges.push(edge.clone()),
            (false, false) => {}
        }
    }
    ScopeContext {
        scope,
        containers,
        files,
        internal_symbol_edges,
        internal_file_edges,
        outbound_file_edges,
        inbound_file_edges,
    }
}

/// Assemble a [`ScopeContext`] from a chosen set of in-scope files: carries the
/// files + relevant containers, the symbol edges internal to the set, and the
/// file edges partitioned by how they cross the scope boundary.
fn build_scope(
    ctx: &ProjectContext,
    scope: String,
    files: Vec<FileContext>,
    containers: Vec<ContainerFacts>,
) -> ScopeContext {
    let in_scope: HashSet<&str> = files.iter().map(|f| f.rel_path.as_str()).collect();
    let keys_in_scope: HashSet<&str> = files
        .iter()
        .flat_map(|f| f.symbols.iter().map(|s| s.key.as_str()))
        .collect();

    let internal_symbol_edges: Vec<Edge> = ctx
        .symbol_edges
        .iter()
        .filter(|e| {
            keys_in_scope.contains(e.src.as_str()) && keys_in_scope.contains(e.dst.as_str())
        })
        .cloned()
        .collect();

    let mut internal_file_edges = Vec::new();
    let mut outbound_file_edges = Vec::new();
    let mut inbound_file_edges = Vec::new();
    for e in &ctx.file_edges {
        match (
            in_scope.contains(e.src.as_str()),
            in_scope.contains(e.dst.as_str()),
        ) {
            (true, true) => internal_file_edges.push(e.clone()),
            (true, false) => outbound_file_edges.push(e.clone()),
            (false, true) => inbound_file_edges.push(e.clone()),
            (false, false) => {}
        }
    }

    ScopeContext {
        scope,
        containers,
        files,
        internal_symbol_edges,
        internal_file_edges,
        outbound_file_edges,
        inbound_file_edges,
    }
}

/// The container whose directory is the longest prefix of `file_rel`.
fn owning_container_dir<'a>(file_rel: &str, containers: &[&'a Container]) -> Option<&'a str> {
    containers
        .iter()
        .filter(|c| c.dir.is_empty() || file_rel.starts_with(&format!("{}/", c.dir)))
        .max_by_key(|c| c.dir.len())
        .map(|c| c.dir.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::{Def, Ident};

    fn def(name: &str, start: u32, end: u32) -> Def {
        Def {
            name: name.to_string(),
            start_line: start,
            end_line: end,
            fields: Vec::new(),
            is_data_shape: false,
        }
    }

    fn ident(name: &str, line: u32) -> Ident {
        Ident {
            name: name.to_string(),
            line,
        }
    }

    fn container(dir: &str, name: &str) -> Container {
        Container {
            dir: dir.to_string(),
            name: name.to_string(),
            technology: None,
            dep_dirs: Vec::new(),
        }
    }

    fn imp(spec: &str, names: &[(&str, &str)], line: u32) -> crate::lang::ImportRef {
        crate::lang::ImportRef {
            spec: spec.to_string(),
            names: names
                .iter()
                .map(|(name, local)| crate::lang::ImportedSym {
                    name: name.to_string(),
                    local: local.to_string(),
                })
                .collect(),
            line,
        }
    }

    /// A TS-ish ParsedFile: only what build_edges reads.
    fn ts_file(
        rel_path: &str,
        defs: Vec<Def>,
        idents: Vec<Ident>,
        imports: Vec<crate::lang::ImportRef>,
    ) -> ParsedFile {
        ParsedFile {
            rel_path: rel_path.to_string(),
            source: String::new(),
            parse: FileParse {
                defs,
                idents,
                paths: vec![],
                imports,
            },
        }
    }

    /// The synthetic key of the unique symbol named `name` across the context.
    fn key_of(ctx: &ProjectContext, name: &str) -> String {
        let keys: Vec<&str> = ctx
            .files
            .iter()
            .flat_map(|f| &f.symbols)
            .filter(|s| s.name == name)
            .map(|s| s.key.as_str())
            .collect();
        assert_eq!(keys.len(), 1, "expected exactly one symbol named {name}");
        keys[0].to_string()
    }

    fn has_file_edge(ctx: &ProjectContext, src: &str, dst: &str) -> bool {
        ctx.file_edges.iter().any(|e| e.src == src && e.dst == dst)
    }

    fn has_sym_edge(ctx: &ProjectContext, src: &str, dst: &str) -> bool {
        let (src, dst) = (key_of(ctx, src), key_of(ctx, dst));
        ctx.symbol_edges
            .iter()
            .any(|e| e.src == src && e.dst == dst)
    }

    #[test]
    fn builds_symbol_and_file_edges() {
        // helper.rs declares `helper`. main.rs declares `run` (lines 1..10) and
        // `compute` (lines 12..14); inside run's body (line 5) it calls the
        // uniquely-named `compute` (symbol→symbol) and `helper` (file→file).
        let files = vec![
            ParsedFile {
                rel_path: "src/helper.rs".into(),
                source: String::new(),
                parse: FileParse {
                    defs: vec![def("helper", 1, 3)],
                    idents: vec![ident("helper", 1)],
                    paths: vec![],
                    imports: vec![],
                },
            },
            ParsedFile {
                rel_path: "src/main.rs".into(),
                source: String::new(),
                parse: FileParse {
                    defs: vec![def("run", 1, 10), def("compute", 12, 14)],
                    idents: vec![ident("run", 1), ident("compute", 5), ident("helper", 6)],
                    paths: vec![],
                    imports: vec![],
                },
            },
        ];
        let containers = vec![container("", "proj")];
        let ctx = build_context("proj", &containers, &files, &[]);

        assert_eq!(ctx.files.len(), 2);
        // Synthetic keys, never node-N.
        assert!(ctx
            .files
            .iter()
            .flat_map(|f| &f.symbols)
            .all(|s| s.key.contains('#') && s.key.contains('@')));

        // file edge main.rs -> helper.rs (run references the unique `helper`).
        assert!(ctx
            .file_edges
            .iter()
            .any(|e| e.src == "src/main.rs" && e.dst == "src/helper.rs"));
        // ...and the SYMBOL edge run -> helper for the same cross-file reference:
        // `helper` (line 6) is enclosed by run (1..10) and resolves uniquely in
        // the container, so the coupling is recorded at symbol granularity, not
        // just as a file pairing.
        assert!(
            ctx.symbol_edges.iter().any(|e| {
                e.src == symbol_key("src/main.rs", "run", 1)
                    && e.dst == symbol_key("src/helper.rs", "helper", 1)
            }),
            "cross-file reference should also yield a symbol edge"
        );
        // symbol edge run -> compute: the `compute` ident at line 5 is enclosed
        // by run (1..10), not by compute (12..14), and resolves uniquely.
        assert!(ctx.symbol_edges.iter().any(|e| {
            e.src == symbol_key("src/main.rs", "run", 1)
                && e.dst == symbol_key("src/main.rs", "compute", 12)
        }));
    }

    /// A reference to a universal method/constructor name (`new`, `clone`, …)
    /// is noise — the name-only resolver can't tell `Thing::new` from `Vec::new`
    /// — so it yields NO edge even when a single same-named def exists.
    #[test]
    fn universal_method_names_yield_no_edge() {
        // model.rs defines `new` (the lone `new` in the container). caller.rs's
        // `run` calls `.new()` — which here means `Vec::new()`/`String::new()`,
        // not `Model::new`. The resolver must not wire it.
        let files = vec![
            ParsedFile {
                rel_path: "src/model.rs".into(),
                source: String::new(),
                parse: FileParse {
                    defs: vec![def("new", 1, 3)],
                    idents: vec![],
                    paths: vec![],
                    imports: vec![],
                },
            },
            ParsedFile {
                rel_path: "src/caller.rs".into(),
                source: String::new(),
                parse: FileParse {
                    defs: vec![def("run", 1, 10)],
                    idents: vec![ident("new", 5)],
                    paths: vec![],
                    imports: vec![],
                },
            },
        ];
        let containers = vec![container("", "proj")];
        let ctx = build_context("proj", &containers, &files, &[]);

        assert!(
            !ctx.symbol_edges.iter().any(|e| e.dst.contains("#new@")),
            "a universal name must not produce a symbol edge"
        );
        assert!(
            !ctx.file_edges.iter().any(|e| e.dst == "src/model.rs"),
            "a universal name must not produce a file edge either"
        );
    }

    #[test]
    fn deterministic() {
        let files = vec![ParsedFile {
            rel_path: "a.rs".into(),
            source: String::new(),
            parse: FileParse {
                defs: vec![def("x", 1, 2)],
                idents: vec![],
                paths: vec![],
                imports: vec![],
            },
        }];
        let containers = vec![container("", "p")];
        let a = build_context("p", &containers, &files, &[]);
        let b = build_context("p", &containers, &files, &[]);
        assert_eq!(
            serde_json::to_string(&a).unwrap(),
            serde_json::to_string(&b).unwrap()
        );
    }

    #[test]
    fn slice_partitions_files_and_edges() {
        let files = vec![
            ParsedFile {
                rel_path: "api/server.ts".into(),
                source: String::new(),
                parse: FileParse {
                    defs: vec![def("serve", 1, 5)],
                    idents: vec![ident("util", 3)],
                    paths: vec![],
                    imports: vec![],
                },
            },
            ParsedFile {
                rel_path: "shared/util.ts".into(),
                source: String::new(),
                parse: FileParse {
                    defs: vec![def("util", 1, 2)],
                    idents: vec![],
                    paths: vec![],
                    imports: vec![],
                },
            },
        ];
        let containers = vec![container("", "proj")];
        let ctx = build_context("proj", &containers, &files, &[]);
        // server.ts -> util.ts crosses the api/ boundary.
        let scoped = slice_scope(&ctx, "api");
        assert_eq!(scoped.files.len(), 1);
        assert_eq!(scoped.files[0].rel_path, "api/server.ts");
        assert!(scoped.internal_file_edges.is_empty());
        assert!(scoped
            .outbound_file_edges
            .iter()
            .any(|e| e.src == "api/server.ts" && e.dst == "shared/util.ts"));
        // The empty-root container encloses the api/ scope.
        assert!(scoped.containers.iter().any(|c| c.dir.is_empty()));
    }

    #[test]
    fn embeds_doc_and_body_excerpts() {
        let source = "\
/// Adds one to the input.
#[inline]
fn add_one(x: u32) -> u32 {
    x + 1
}
";
        let files = vec![ParsedFile {
            rel_path: "src/math.rs".into(),
            source: source.into(),
            parse: FileParse {
                defs: vec![def("add_one", 3, 5)],
                idents: vec![],
                paths: vec![],
                imports: vec![],
            },
        }];
        let containers = vec![container("", "p")];
        let ctx = build_context("p", &containers, &files, &[]);
        let compact = compact_scope(&slice_container(&ctx, ""));

        let code = compact.files[0].symbols[0].code.as_deref().unwrap();
        // Doc comment and attribute above the definition are walked in.
        assert!(code.starts_with("/// Adds one to the input."), "{code}");
        assert!(code.contains("#[inline]"));
        assert!(code.contains("x + 1"));
        // Complete definition — no truncation marker.
        assert!(!code.contains("… +"), "{code}");
    }

    #[test]
    fn long_definitions_truncate_with_marker_and_budget_tightens_the_cap() {
        // One oversized definition: marker carries the hidden line count.
        let big_source: String = (0..200)
            .map(|i| format!("    let value_{i:04} = compute_something_with_a_long_name({i});\n"))
            .collect::<String>();
        let files = vec![ParsedFile {
            rel_path: "src/big.rs".into(),
            source: format!("fn big() {{\n{big_source}}}\n"),
            parse: FileParse {
                defs: vec![def("big", 1, 202)],
                idents: vec![],
                paths: vec![],
                imports: vec![],
            },
        }];
        let containers = vec![container("", "p")];
        let ctx = build_context("p", &containers, &files, &[]);
        let compact = compact_scope(&slice_container(&ctx, ""));
        let code = compact.files[0].symbols[0].code.as_deref().unwrap();
        let shown = code.lines().count() - 1;
        assert!(shown <= 48, "line cap respected (got {shown})");
        // The marker accounts for every hidden line of the 202-line definition.
        let marker = code.lines().last().unwrap();
        let hidden: usize = marker
            .strip_prefix("… +")
            .and_then(|m| m.strip_suffix(" lines"))
            .unwrap_or_else(|| panic!("marker line: {marker}"))
            .parse()
            .unwrap();
        assert_eq!(shown + hidden, 202);

        // Many such files blow the scope budget — the per-symbol cap steps down.
        let many: Vec<ParsedFile> = (0..200)
            .map(|n| ParsedFile {
                rel_path: format!("src/f{n:03}.rs"),
                source: format!("fn f{n}() {{\n{big_source}}}\n"),
                parse: FileParse {
                    defs: vec![def(&format!("f{n}"), 1, 202)],
                    idents: vec![],
                    paths: vec![],
                    imports: vec![],
                },
            })
            .collect();
        let ctx = build_context("p", &containers, &many, &[]);
        let compact = compact_scope(&slice_container(&ctx, ""));
        let max_code_lines = compact
            .files
            .iter()
            .flat_map(|f| &f.symbols)
            .filter_map(|s| s.code.as_deref())
            .map(|c| c.lines().count())
            .max()
            .unwrap();
        assert!(
            max_code_lines < 48,
            "budget should tighten the line cap (got {max_code_lines})"
        );
        let total: usize = serde_json::to_string(&compact).unwrap().len();
        assert!(
            total < 450_000,
            "payload stays near the evidence budget (got {total})"
        );
    }

    #[test]
    fn compact_scope_interns_repeated_paths_and_symbol_keys() {
        let files = vec![
            ParsedFile {
                rel_path: "api/server.ts".into(),
                source: String::new(),
                parse: FileParse {
                    defs: vec![def("serve", 1, 20), def("route", 22, 30)],
                    idents: vec![ident("route", 5), ident("util", 8)],
                    paths: vec![],
                    imports: vec![],
                },
            },
            ParsedFile {
                rel_path: "api/util.ts".into(),
                source: String::new(),
                parse: FileParse {
                    defs: vec![def("util", 1, 4)],
                    idents: vec![],
                    paths: vec![],
                    imports: vec![],
                },
            },
        ];
        let containers = vec![container("api", "api")];
        let ctx = build_context("proj", &containers, &files, &[]);
        let scope = slice_container(&ctx, "api");
        let compact = compact_scope(&scope);

        assert_eq!(compact.paths, vec!["api/server.ts", "api/util.ts"]);
        assert_eq!(compact.files.len(), 2);
        assert_eq!(compact.files[0].symbols[0].id, 0);
        assert!(compact
            .symbol_edges
            .iter()
            .all(|edge| edge[0] < 3 && edge[1] < 3));

        let full_json = serde_json::to_string(&scope).unwrap();
        let compact_json = serde_json::to_string(&compact).unwrap();
        assert!(
            compact_json.len() < full_json.len(),
            "compact payload should be smaller: {} vs {}",
            compact_json.len(),
            full_json.len()
        );
    }

    fn pathref(segments: &[&str], line: u32) -> crate::lang::PathRef {
        crate::lang::PathRef {
            segments: segments.iter().map(|s| s.to_string()).collect(),
            line,
        }
    }

    /// A fully-qualified call-site reference into ANOTHER crate resolves through
    /// the manifest map and yields BOTH a file edge and a symbol edge attributed
    /// to the calling function — the gap the same-container scopes can't close.
    #[test]
    fn cross_container_qualified_call_resolves() {
        // extract/src/anchors.rs defines `write_baseline`. mcp/src/tools/intent.rs
        // calls `scryer_extract::anchors::write_baseline(..)` from inside `run`.
        let files = vec![
            ParsedFile {
                rel_path: "crates/scryer-extract/src/anchors.rs".into(),
                source: String::new(),
                parse: FileParse {
                    defs: vec![def("write_baseline", 1, 5)],
                    idents: vec![],
                    paths: vec![],
                    imports: vec![],
                },
            },
            ParsedFile {
                rel_path: "crates/scryer-mcp/src/tools/intent.rs".into(),
                source: String::new(),
                parse: FileParse {
                    defs: vec![def("run", 1, 10)],
                    idents: vec![],
                    paths: vec![pathref(&["scryer_extract", "anchors", "write_baseline"], 6)],
                    imports: vec![],
                },
            },
        ];
        let containers = vec![
            container("crates/scryer-extract", "scryer-extract"),
            container("crates/scryer-mcp", "scryer-mcp"),
        ];
        let ctx = build_context("proj", &containers, &files, &[]);

        assert!(
            ctx.file_edges.iter().any(|e| {
                e.src == "crates/scryer-mcp/src/tools/intent.rs"
                    && e.dst == "crates/scryer-extract/src/anchors.rs"
            }),
            "cross-crate reference should yield a file edge"
        );
        assert!(
            ctx.symbol_edges.iter().any(|e| {
                e.src == symbol_key("crates/scryer-mcp/src/tools/intent.rs", "run", 1)
                    && e.dst
                        == symbol_key("crates/scryer-extract/src/anchors.rs", "write_baseline", 1)
            }),
            "the call site is enclosed by `run`, so a symbol edge must exist too"
        );
    }

    /// Hyphenated crate names (`scryer-extract`) are spelled with underscores in
    /// Rust paths (`scryer_extract`); the map normalizes so they still match.
    #[test]
    fn cross_container_normalizes_crate_name_hyphens() {
        let files = vec![
            ParsedFile {
                rel_path: "crates/scryer-extract/src/lib.rs".into(),
                source: String::new(),
                parse: FileParse {
                    defs: vec![def("Anchor", 1, 3)],
                    idents: vec![],
                    paths: vec![],
                    imports: vec![],
                },
            },
            ParsedFile {
                rel_path: "crates/scryer-mcp/src/m.rs".into(),
                source: String::new(),
                parse: FileParse {
                    defs: vec![def("run", 1, 4)],
                    idents: vec![],
                    paths: vec![pathref(&["scryer_extract", "Anchor"], 2)],
                    imports: vec![],
                },
            },
        ];
        let containers = vec![
            container("crates/scryer-extract", "scryer-extract"),
            container("crates/scryer-mcp", "scryer-mcp"),
        ];
        let ctx = build_context("proj", &containers, &files, &[]);
        assert!(ctx
            .file_edges
            .iter()
            .any(|e| e.dst == "crates/scryer-extract/src/lib.rs"));
    }

    /// An ambiguous leaf (the same name defined in two files of the target crate)
    /// resolves to nothing — the under-report discipline holds across containers.
    #[test]
    fn cross_container_ambiguous_leaf_yields_no_edge() {
        let files = vec![
            ParsedFile {
                rel_path: "crates/dep/src/a.rs".into(),
                source: String::new(),
                parse: FileParse {
                    defs: vec![def("Thing", 1, 3)],
                    idents: vec![],
                    paths: vec![],
                    imports: vec![],
                },
            },
            ParsedFile {
                rel_path: "crates/dep/src/b.rs".into(),
                source: String::new(),
                parse: FileParse {
                    defs: vec![def("Thing", 1, 3)],
                    idents: vec![],
                    paths: vec![],
                    imports: vec![],
                },
            },
            ParsedFile {
                rel_path: "crates/app/src/m.rs".into(),
                source: String::new(),
                parse: FileParse {
                    defs: vec![def("run", 1, 4)],
                    idents: vec![],
                    paths: vec![pathref(&["dep", "Thing"], 2)],
                    imports: vec![],
                },
            },
        ];
        let containers = vec![
            container("crates/dep", "dep"),
            container("crates/app", "app"),
        ];
        let ctx = build_context("proj", &containers, &files, &[]);
        assert!(
            !ctx.file_edges
                .iter()
                .any(|e| e.src == "crates/app/src/m.rs"),
            "ambiguous target leaf must not mint a cross-crate edge"
        );
    }

    /// A bare module import (`use dep::helpers;`) names a module, not a def, and
    /// still resolves to that module's file (file edge only — no symbol).
    #[test]
    fn cross_container_module_import_resolves_to_file() {
        let files = vec![
            ParsedFile {
                rel_path: "crates/dep/src/helpers.rs".into(),
                source: String::new(),
                parse: FileParse {
                    defs: vec![def("assist", 1, 3)],
                    idents: vec![],
                    paths: vec![],
                    imports: vec![],
                },
            },
            ParsedFile {
                rel_path: "crates/app/src/m.rs".into(),
                source: String::new(),
                parse: FileParse {
                    defs: vec![def("run", 1, 4)],
                    idents: vec![],
                    // `use dep::helpers;` — leaf is the module, not a symbol.
                    paths: vec![pathref(&["dep", "helpers"], 1)],
                    imports: vec![],
                },
            },
        ];
        let containers = vec![
            container("crates/dep", "dep"),
            container("crates/app", "app"),
        ];
        let ctx = build_context("proj", &containers, &files, &[]);
        assert!(
            ctx.file_edges.iter().any(|e| {
                e.src == "crates/app/src/m.rs" && e.dst == "crates/dep/src/helpers.rs"
            }),
            "module import should resolve to the module's file"
        );
    }

    /// Same-crate qualified paths and stdlib paths must NOT produce edges: the
    /// head is either the file's own container or an external crate we don't model.
    #[test]
    fn cross_container_ignores_self_and_external() {
        let files = vec![ParsedFile {
            rel_path: "crates/app/src/m.rs".into(),
            source: String::new(),
            parse: FileParse {
                defs: vec![def("run", 1, 4), def("Helper", 6, 8)],
                idents: vec![],
                paths: vec![
                    pathref(&["std", "collections", "HashMap"], 1),
                    pathref(&["app", "Helper"], 2), // own crate -> handled elsewhere
                    pathref(&["crate", "Helper"], 3),
                ],
                imports: vec![],
            },
        }];
        let containers = vec![container("crates/app", "app")];
        let ctx = build_context("proj", &containers, &files, &[]);
        // No cross-container edges: std is external, app is self.
        assert!(ctx.file_edges.is_empty(), "no cross-crate edges expected");
    }

    /// The audit's litmus case: `import { Button } from "@acme/ui"` across a
    /// pnpm workspace produced ZERO edges. The package name maps to the ui
    /// container verbatim; the usage site yields the symbol edge.
    #[test]
    fn ts_package_import_resolves_cross_container() {
        let files = vec![
            ts_file(
                "packages/ui/src/Button.tsx",
                vec![def("Button", 1, 8)],
                vec![],
                vec![],
            ),
            ts_file(
                "packages/app/src/App.tsx",
                vec![def("App", 3, 9)],
                vec![ident("Button", 5)],
                vec![imp("@acme/ui", &[("Button", "Button")], 1)],
            ),
        ];
        let containers = vec![
            container("packages/app", "@acme/app"),
            container("packages/ui", "@acme/ui"),
        ];
        let ctx = build_context("proj", &containers, &files, &[]);
        assert!(has_file_edge(
            &ctx,
            "packages/app/src/App.tsx",
            "packages/ui/src/Button.tsx"
        ));
        assert!(has_sym_edge(&ctx, "App", "Button"));
    }

    /// Relative imports resolve exactly — extension candidates, directory
    /// index files, `as`-renamed usage sites — even when the bare name is
    /// ambiguous container-wide (two `helper` defs), which the name-only
    /// scopes must skip.
    #[test]
    fn ts_relative_import_resolves_file_and_symbols() {
        let files = vec![
            ts_file("src/util.ts", vec![def("helper", 1, 3)], vec![], vec![]),
            ts_file("src/other.ts", vec![def("helper", 1, 3)], vec![], vec![]),
            ts_file(
                "src/widgets/index.ts",
                vec![def("Widget", 1, 4)],
                vec![],
                vec![],
            ),
            ts_file(
                "src/main.ts",
                vec![def("run", 3, 9)],
                vec![ident("h", 5), ident("Widget", 6)],
                vec![
                    imp("./util", &[("helper", "h")], 1),
                    imp("./widgets", &[("Widget", "Widget")], 2),
                ],
            ),
        ];
        let containers = vec![container("", "app")];
        let ctx = build_context("proj", &containers, &files, &[]);
        assert!(has_file_edge(&ctx, "src/main.ts", "src/util.ts"));
        assert!(has_file_edge(&ctx, "src/main.ts", "src/widgets/index.ts"));
        // The alias `h` resolves to util.ts's `helper`, not other.ts's.
        let run = key_of(&ctx, "run");
        let util_helper = ctx
            .files
            .iter()
            .find(|f| f.rel_path == "src/util.ts")
            .unwrap()
            .symbols[0]
            .key
            .clone();
        assert!(ctx
            .symbol_edges
            .iter()
            .any(|e| e.src == run && e.dst == util_helper));
        assert!(has_sym_edge(&ctx, "run", "Widget"));
    }

    /// NodeNext-style specs write the EMITTED `.js` extension for a `.ts`
    /// source; namespace/side-effect imports carry file evidence only.
    #[test]
    fn ts_emitted_extension_and_bare_imports_resolve_to_files() {
        let files = vec![
            ts_file("src/util.ts", vec![def("helper", 1, 3)], vec![], vec![]),
            ts_file(
                "src/main.ts",
                vec![def("run", 2, 6)],
                vec![],
                vec![
                    imp("./util.js", &[("helper", "helper")], 1),
                    imp("./helpers", &[], 2), // unresolvable: no such file
                ],
            ),
            ts_file("src/setup.ts", vec![def("init", 1, 2)], vec![], vec![]),
            ts_file(
                "src/boot.ts",
                vec![def("boot", 2, 4)],
                vec![],
                vec![imp("./setup", &[], 1)], // side-effect import
            ),
        ];
        let containers = vec![container("", "app")];
        let ctx = build_context("proj", &containers, &files, &[]);
        assert!(has_file_edge(&ctx, "src/main.ts", "src/util.ts"));
        assert!(has_file_edge(&ctx, "src/boot.ts", "src/setup.ts"));
        assert!(!ctx.file_edges.iter().any(|e| e.dst.contains("helpers")));
    }

    /// An import binding is exact lexical evidence: it overrides the
    /// universal-name skip that blocks bare `get` idents.
    #[test]
    fn ts_imported_universal_name_resolves() {
        let files = vec![
            ts_file("src/api.ts", vec![def("get", 1, 3)], vec![], vec![]),
            ts_file(
                "src/main.ts",
                vec![def("run", 3, 7)],
                vec![ident("get", 5)],
                vec![imp("./api", &[("get", "get")], 1)],
            ),
        ];
        let containers = vec![container("", "app")];
        let ctx = build_context("proj", &containers, &files, &[]);
        assert!(has_file_edge(&ctx, "src/main.ts", "src/api.ts"));
        assert!(has_sym_edge(&ctx, "run", "get"));
    }

    /// An import from an UNMAPPED package still consumes its local bindings:
    /// the usage must not be misattributed to a same-name def elsewhere in the
    /// container. Same for a `..` spec escaping the project root.
    #[test]
    fn ts_external_import_consumes_locals_without_edges() {
        let files = vec![
            ts_file("src/store.ts", vec![def("merge", 1, 3)], vec![], vec![]),
            ts_file(
                "src/main.ts",
                vec![def("run", 3, 8)],
                vec![ident("merge", 5), ident("outside", 6)],
                vec![
                    imp("lodash", &[("merge", "merge")], 1),
                    imp("../../elsewhere", &[("outside", "outside")], 2),
                ],
            ),
        ];
        let containers = vec![container("", "app")];
        let ctx = build_context("proj", &containers, &files, &[]);
        assert!(ctx.file_edges.is_empty(), "no edges to external modules");
        assert!(
            ctx.symbol_edges.is_empty(),
            "imported `merge` must not resolve to the local store.ts def"
        );
    }

    /// End-to-end through real parsing (no hand-built idents): a pnpm-workspace
    /// shape where the app imports `{ Button }` from `@acme/ui` and uses it as
    /// a JSX element. Guards the whole chain: import capture, package-map
    /// resolution, and JSX usage-site identifiers.
    #[test]
    fn ts_pnpm_workspace_end_to_end() {
        let parse = |rel: &str, src: &str| ParsedFile {
            rel_path: rel.to_string(),
            source: src.to_string(),
            parse: crate::lang::parse_file(std::path::Path::new(rel), src).unwrap(),
        };
        let files = vec![
            parse(
                "packages/ui/src/Button.tsx",
                "export function Button() { return <button/>; }\n",
            ),
            parse(
                "packages/app/src/App.tsx",
                "import { Button } from \"@acme/ui\";\nexport function App() {\n  return <Button />;\n}\n",
            ),
        ];
        let containers = vec![
            container("packages/app", "@acme/app"),
            container("packages/ui", "@acme/ui"),
        ];
        let ctx = build_context("proj", &containers, &files, &[]);
        assert!(has_file_edge(
            &ctx,
            "packages/app/src/App.tsx",
            "packages/ui/src/Button.tsx"
        ));
        assert!(has_sym_edge(&ctx, "App", "Button"));
    }

    /// Python absolute imports resolve through the src layout of the
    /// importing container, with symbol edges at usage sites.
    #[test]
    fn py_absolute_import_resolves_same_container() {
        let files = vec![
            ts_file("src/app/util.py", vec![def("helper", 1, 3)], vec![], vec![]),
            ts_file(
                "src/app/main.py",
                vec![def("run", 3, 9)],
                vec![ident("helper", 5)],
                vec![imp("app.util", &[("helper", "helper")], 1)],
            ),
        ];
        let containers = vec![container("", "myapp")];
        let ctx = build_context("proj", &containers, &files, &[]);
        assert!(has_file_edge(&ctx, "src/app/main.py", "src/app/util.py"));
        assert!(has_sym_edge(&ctx, "run", "helper"));
    }

    /// The Python analog of the audit's litmus case: a cross-package import in
    /// a uv/poetry-style workspace resolves through the DECLARED distribution
    /// name (`acme-lib` imported as `acme_lib`), hyphens normalized like the
    /// Rust crate map.
    #[test]
    fn py_cross_container_import_resolves_via_declared_name() {
        let files = vec![
            ts_file(
                "packages/lib/src/acme_lib/dates.py",
                vec![def("fmt_date", 1, 3)],
                vec![],
                vec![],
            ),
            ts_file(
                "packages/app/src/acme_app/main.py",
                vec![def("run", 3, 9)],
                vec![ident("fmt_date", 5)],
                vec![imp("acme_lib.dates", &[("fmt_date", "fmt_date")], 1)],
            ),
        ];
        let containers = vec![
            container("packages/app", "acme-app"),
            container("packages/lib", "acme-lib"),
        ];
        let ctx = build_context("proj", &containers, &files, &[]);
        assert!(has_file_edge(
            &ctx,
            "packages/app/src/acme_app/main.py",
            "packages/lib/src/acme_lib/dates.py"
        ));
        assert!(has_sym_edge(&ctx, "run", "fmt_date"));
    }

    /// Relative imports: `.sibling` resolves within the package, `..config`
    /// climbs a level, and `from . import other` falls back to the SUBMODULE
    /// file when the name isn't a symbol of the package module.
    #[test]
    fn py_relative_imports_resolve() {
        let files = vec![
            ts_file("pkg/sibling.py", vec![def("calc", 1, 3)], vec![], vec![]),
            ts_file("pkg/other.py", vec![def("stuff", 1, 3)], vec![], vec![]),
            ts_file("config.py", vec![def("load", 1, 3)], vec![], vec![]),
            ts_file(
                "pkg/mod.py",
                vec![def("run", 4, 12)],
                vec![ident("calc", 6), ident("load", 7)],
                vec![
                    imp(".sibling", &[("calc", "calc")], 1),
                    imp(".", &[("other", "other")], 2),
                    imp("..config", &[("load", "load")], 3),
                ],
            ),
        ];
        let containers = vec![container("", "app")];
        let ctx = build_context("proj", &containers, &files, &[]);
        assert!(has_file_edge(&ctx, "pkg/mod.py", "pkg/sibling.py"));
        assert!(has_file_edge(&ctx, "pkg/mod.py", "pkg/other.py"));
        assert!(has_file_edge(&ctx, "pkg/mod.py", "config.py"));
        assert!(has_sym_edge(&ctx, "run", "calc"));
        assert!(has_sym_edge(&ctx, "run", "load"));
    }

    /// An import from an external distribution consumes its locals: the usage
    /// must not be misattributed to a same-name def elsewhere in the container.
    #[test]
    fn py_external_import_consumes_locals_without_edges() {
        let files = vec![
            ts_file("store.py", vec![def("fetch", 1, 3)], vec![], vec![]),
            ts_file(
                "main.py",
                vec![def("run", 3, 8)],
                vec![ident("fetch", 5)],
                vec![imp("requests.api", &[("fetch", "fetch")], 1)],
            ),
        ];
        let containers = vec![container("", "app")];
        let ctx = build_context("proj", &containers, &files, &[]);
        assert!(ctx.file_edges.is_empty(), "no edges to external modules");
        assert!(
            ctx.symbol_edges.is_empty(),
            "imported `fetch` must not resolve to the local store.py def"
        );
    }

    /// A `paths` alias (`@/*` -> `src/*`) resolves a bare spec to a file, with
    /// symbol edges at the usage site; alias resolution takes precedence over
    /// the package map.
    #[test]
    fn ts_alias_import_resolves() {
        let files = vec![
            ts_file(
                "src/components/Button.tsx",
                vec![def("Button", 1, 6)],
                vec![],
                vec![],
            ),
            ts_file(
                "src/App.tsx",
                vec![def("App", 3, 9)],
                vec![ident("Button", 5)],
                vec![imp("@/components/Button", &[("Button", "Button")], 1)],
            ),
        ];
        let containers = vec![container("", "app")];
        let aliases = vec![TsAliases {
            dir: String::new(),
            base_url: None,
            paths: vec![("@/*".to_string(), vec!["src/*".to_string()])],
        }];
        let ctx = build_context("proj", &containers, &files, &aliases);
        assert!(has_file_edge(
            &ctx,
            "src/App.tsx",
            "src/components/Button.tsx"
        ));
        assert!(has_sym_edge(&ctx, "App", "Button"));
    }

    /// A bare spec with no matching alias pattern resolves as a
    /// `baseUrl`-relative file; exact (starless) patterns match too. The
    /// governing config is the NEAREST one up the tree.
    #[test]
    fn ts_baseurl_and_exact_alias_resolve_via_nearest_config() {
        let files = vec![
            ts_file(
                "packages/app/src/util/format.ts",
                vec![def("fmt", 1, 3)],
                vec![],
                vec![],
            ),
            ts_file(
                "packages/app/src/config.ts",
                vec![def("Config", 1, 5)],
                vec![],
                vec![],
            ),
            ts_file(
                "packages/app/src/main.ts",
                vec![def("run", 3, 9)],
                vec![ident("fmt", 5), ident("Config", 6)],
                vec![
                    imp("util/format", &[("fmt", "fmt")], 1),
                    imp("config", &[("Config", "Config")], 2),
                ],
            ),
        ];
        let containers = vec![container("packages/app", "app")];
        let aliases = vec![
            // A root config that would resolve nothing — must NOT govern.
            TsAliases {
                dir: String::new(),
                base_url: Some("elsewhere".to_string()),
                paths: vec![],
            },
            TsAliases {
                dir: "packages/app".to_string(),
                base_url: Some("packages/app/src".to_string()),
                paths: vec![(
                    "config".to_string(),
                    vec!["packages/app/src/config.ts".to_string()],
                )],
            },
        ];
        let ctx = build_context("proj", &containers, &files, &aliases);
        assert!(has_file_edge(
            &ctx,
            "packages/app/src/main.ts",
            "packages/app/src/util/format.ts"
        ));
        assert!(has_file_edge(
            &ctx,
            "packages/app/src/main.ts",
            "packages/app/src/config.ts"
        ));
        assert!(has_sym_edge(&ctx, "run", "fmt"));
        assert!(has_sym_edge(&ctx, "run", "Config"));
    }

    /// A subpath spec (`@acme/ui/button`) resolves through the package dir,
    /// including the `src/` monorepo layout.
    #[test]
    fn ts_package_subpath_resolves_through_src() {
        let files = vec![
            ts_file(
                "packages/ui/src/button.ts",
                vec![def("Button", 1, 6)],
                vec![],
                vec![],
            ),
            ts_file(
                "packages/app/src/App.tsx",
                vec![def("App", 3, 9)],
                vec![ident("Button", 5)],
                vec![imp("@acme/ui/button", &[("Button", "Button")], 1)],
            ),
        ];
        let containers = vec![
            container("packages/app", "@acme/app"),
            container("packages/ui", "@acme/ui"),
        ];
        let ctx = build_context("proj", &containers, &files, &[]);
        assert!(has_file_edge(
            &ctx,
            "packages/app/src/App.tsx",
            "packages/ui/src/button.ts"
        ));
        assert!(has_sym_edge(&ctx, "App", "Button"));
    }
}
