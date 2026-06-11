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
use crate::manifest::Container;
use serde::Serialize;
use std::collections::{BTreeSet, HashMap, HashSet};

/// A parsed source file, project-relative path with `/` separators.
pub struct ParsedFile {
    pub rel_path: String,
    pub parse: FileParse,
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

/// Assemble the full project context from discovered containers + parsed files.
/// Inputs are exactly what [`crate::extract_context`] collects (`files` already
/// sorted by `rel_path`). Pure and deterministic: the same source always yields
/// the same context.
pub fn build_context(
    project_name: &str,
    containers: &[Container],
    files: &[ParsedFile],
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
        let mut defs: Vec<&Def> = f.parse.defs.iter().collect();
        defs.sort_by(|a, b| (a.start_line, &a.name).cmp(&(b.start_line, &b.name)));
        let mut symbols = Vec::with_capacity(defs.len());
        for def in defs {
            let key = symbol_key(&f.rel_path, &def.name, def.start_line);
            symbols.push(SymbolContext {
                key: key.clone(),
                name: def.name.clone(),
                start_line: def.start_line,
                end_line: def.end_line,
                fields: def.fields.clone(),
                is_data_shape: def.is_data_shape,
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

    let (symbol_edges, file_edges) = build_edges(files, &recs);

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
/// file using another file's symbol yields a file→file edge. Edges are
/// intentionally UNDER-reported: absence of an edge is not absence of a
/// dependency, and resolution is weaker for the generic-fallback languages
/// (go/java/ruby/c/cpp/c#/php), where only coarse definitions and bare
/// identifiers are seen.
fn build_edges(files: &[ParsedFile], recs: &[SymRec]) -> (Vec<Edge>, Vec<Edge>) {
    let mut ranges_by_file: HashMap<&str, Vec<(u32, u32, &str)>> = HashMap::new();
    let mut container_of_file: HashMap<&str, &str> = HashMap::new();
    let mut file_names: HashMap<&str, HashMap<&str, Vec<&str>>> = HashMap::new();
    let mut cont_names: HashMap<&str, HashMap<&str, Vec<(&str, &str)>>> = HashMap::new();

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

    let mut sym_edges: HashSet<(String, String)> = HashSet::new();
    let mut file_edges: HashSet<(String, String)> = HashSet::new();

    for f in files {
        let file = f.rel_path.as_str();
        let container_dir = container_of_file.get(file).copied();
        for ident in &f.parse.idents {
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
                        let dst_file = cands[0].1;
                        if dst_file != file {
                            file_edges.insert((file.to_string(), dst_file.to_string()));
                        }
                    }
                }
            }
        }
    }

    (sorted_edges(sym_edges), sorted_edges(file_edges))
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

    #[test]
    fn builds_symbol_and_file_edges() {
        // helper.rs declares `helper`. main.rs declares `run` (lines 1..10) and
        // `compute` (lines 12..14); inside run's body (line 5) it calls the
        // uniquely-named `compute` (symbol→symbol) and `helper` (file→file).
        let files = vec![
            ParsedFile {
                rel_path: "src/helper.rs".into(),
                parse: FileParse {
                    defs: vec![def("helper", 1, 3)],
                    idents: vec![ident("helper", 1)],
                },
            },
            ParsedFile {
                rel_path: "src/main.rs".into(),
                parse: FileParse {
                    defs: vec![def("run", 1, 10), def("compute", 12, 14)],
                    idents: vec![ident("run", 1), ident("compute", 5), ident("helper", 6)],
                },
            },
        ];
        let containers = vec![container("", "proj")];
        let ctx = build_context("proj", &containers, &files);

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
        // symbol edge run -> compute: the `compute` ident at line 5 is enclosed
        // by run (1..10), not by compute (12..14), and resolves uniquely.
        assert!(ctx.symbol_edges.iter().any(|e| {
            e.src == symbol_key("src/main.rs", "run", 1)
                && e.dst == symbol_key("src/main.rs", "compute", 12)
        }));
    }

    #[test]
    fn deterministic() {
        let files = vec![ParsedFile {
            rel_path: "a.rs".into(),
            parse: FileParse {
                defs: vec![def("x", 1, 2)],
                idents: vec![],
            },
        }];
        let containers = vec![container("", "p")];
        let a = build_context("p", &containers, &files);
        let b = build_context("p", &containers, &files);
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
                parse: FileParse {
                    defs: vec![def("serve", 1, 5)],
                    idents: vec![ident("util", 3)],
                },
            },
            ParsedFile {
                rel_path: "shared/util.ts".into(),
                parse: FileParse {
                    defs: vec![def("util", 1, 2)],
                    idents: vec![],
                },
            },
        ];
        let containers = vec![container("", "proj")];
        let ctx = build_context("proj", &containers, &files);
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
    fn compact_scope_interns_repeated_paths_and_symbol_keys() {
        let files = vec![
            ParsedFile {
                rel_path: "api/server.ts".into(),
                parse: FileParse {
                    defs: vec![def("serve", 1, 20), def("route", 22, 30)],
                    idents: vec![ident("route", 5), ident("util", 8)],
                },
            },
            ParsedFile {
                rel_path: "api/util.ts".into(),
                parse: FileParse {
                    defs: vec![def("util", 1, 4)],
                    idents: vec![],
                },
            },
        ];
        let containers = vec![container("api", "api")];
        let ctx = build_context("proj", &containers, &files);
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
}
