//! The project's code dependency graph and what it says about the C4 tree. The
//! extractor produces [`BuildEdges`] in one pass — symbol→symbol edges keyed
//! `path#name@line`, imports of outside packages, and the extent of every
//! outermost type — with no C4 structure of its own; it is cached to
//! `.scryer/.build_edges.json`.
//!
//! [`derive_graph`] joins it onto a model: each edge resolves to its host node
//! and symbol and rolls up through every containing level, each declared link
//! is rated by the code edges behind it, and sibling pairs the code connects
//! with no link are surfaced as candidates. [`BoundaryOwnership`] settles which
//! node owns a file when boundary globs overlap — the one most-specific ranking
//! the link audit, health and the style checks share.

use serde::{Deserialize, Serialize};


/// A directed dependency edge between two source-anchored endpoints.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CachedEdge {
    pub src: String,
    pub dst: String,
    /// The extractor matched a bare name with no import to follow — link
    /// evidence, never grounds for a structural violation.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub guessed: bool,
}

/// One import of a package outside the repository: the importing file and the
/// package name as the language spells it. Matched against a style's banned
/// packages; not a dependency inventory (see the extractor's `ExternalImport`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExternalImport {
    pub file: String,
    pub package: String,
}

/// The whole-project dependency graph for one build.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildEdges {
    /// Symbol→symbol edges, keyed by `path#name@startLine`.
    #[serde(default)]
    pub symbol_edges: Vec<CachedEdge>,
    /// Imports of packages outside the repository, per file.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub external_imports: Vec<ExternalImport>,
    /// Every outermost definition (a top-level type or free function) with
    /// its extent, so an edge from a method can be charged to the type that
    /// declares it. What a style's type roles are checked on.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub types: Vec<TypeSpan>,
}

/// One outermost definition: its symbol key (`path#name@startLine`), the last
/// line it spans, and how many definitions it nests (methods, constructors,
/// nested types) — zero for a pure data shape.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TypeSpan {
    pub key: String,
    pub end_line: u32,
    #[serde(default)]
    pub nested: u32,
}

impl BuildEdges {
    /// Parse one extractor symbol key (`path#name@line`) into `(path, name)`.
    /// The trailing `@line` only disambiguates same-named defs in one file; the
    /// commit-time join is on `(path, name)`, which tolerates the agent
    /// reporting a slightly different start line for the definition.
    pub fn split_symbol_key(key: &str) -> Option<(&str, &str)> {
        let (path, rest) = key.split_once('#')?;
        let name = rest.rsplit_once('@').map(|(n, _)| n).unwrap_or(rest);
        Some((path, name))
    }
}

// --- Link evidence: the import graph audits the model's links ----------------
//
// Links between nodes are a C4 MODEL PRIMITIVE — they live in `model.links`,
// nowhere else. The extractor's import graph never becomes a second edge
// system; it is EVIDENCE about the links the model declares: how many real
// code edges back each declared link, and which sibling pairs the code
// connects that no link covers (candidates to mint as real links, or to
// question). Both are regenerable annotations computed beside the model.

use crate::{Kind, ScryModel};
use std::collections::{BTreeMap, HashMap, HashSet};

/// A candidate link: two sibling nodes the code connects but no declared link
/// covers. `count` = number of underlying symbol→symbol import edges.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DerivedEdge {
    pub src: String,
    pub dst: String,
    pub count: u32,
}

/// The evidence rating of one declared link: how many underlying code edges
/// cross from the src subtree into the dst subtree. 0 = asserted-only — the
/// model claims a relationship the import graph doesn't show (which may still
/// be true: runtime calls, IPC, HTTP).
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkAudit {
    pub link_id: String,
    pub edge_count: u32,
}

/// One leaf code edge with both endpoints resolved to their host node and the
/// symbol that anchored them, deduped with a `count`. Intra-node and containment
/// edges (one endpoint an ancestor of the other) are dropped — only cross-subtree
/// edges carry connection signal. These are the per-symbol rows the aggregate
/// `link_audit` / `unmodeled` counts are built from; the UI reads them to attribute
/// a node's "implied connections" and to expand a declared link into its code paths.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedEdge {
    pub src_node: String,
    pub src_symbol: String,
    /// Project-relative file the source symbol lives in.
    pub src_file: String,
    pub dst_node: String,
    pub dst_symbol: String,
    /// Project-relative file the target symbol lives in.
    pub dst_file: String,
    pub count: u32,
    /// Every underlying code edge was a bare-name guess (see [`CachedEdge`]).
    pub guessed: bool,
}

#[derive(Debug, Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DerivedGraph {
    /// One entry per declared model link.
    pub link_audit: Vec<LinkAudit>,
    /// Same-kind SIBLING pairs (above symbol level) the code connects but no
    /// declared link covers in either direction — candidate links the model is
    /// missing. Sorted by (src, dst).
    pub unmodeled: Vec<DerivedEdge>,
    /// Every cross-subtree leaf edge, resolved to (node, symbol) on both ends and
    /// deduped. Sorted by (src_node, dst_node, src_symbol, dst_symbol).
    pub resolved_edges: Vec<ResolvedEdge>,
}

// Glob specificity comes from `ownership::pattern_rank` — the ONE ranking all
// ownership decisions share, so the link audit can never attribute a contested
// file differently from health/drift.

/// Join the cached import graph onto the model. Deterministic; resolution is
/// best-effort (unresolvable endpoints are skipped, never guessed).
pub fn derive_graph(model: &ScryModel, edges: &BuildEdges) -> DerivedGraph {
    let node_by_id: HashMap<&str, &crate::Node> =
        model.nodes.iter().map(|n| (n.id.as_str(), n)).collect();

    // --- endpoint resolution ---------------------------------------------------
    // 1) (file, symbol name) → host node, from sourceMap. Definition anchors
    //    (node-id keys) are authored first and win over responsibility anchors.
    let mut by_symbol: HashMap<(&str, &str), &str> = HashMap::new();
    let mut resp_host: HashMap<&str, &str> = HashMap::new();
    for node in &model.nodes {
        for resp in &node.responsibilities {
            resp_host.insert(resp.id.as_str(), node.id.as_str());
        }
    }
    // Two passes for deterministic precedence: definitions, then responsibilities.
    for node in &model.nodes {
        if let Some(locs) = model.source_map.get(&node.id) {
            for loc in locs {
                if let Some(sym) = &loc.symbol {
                    by_symbol
                        .entry((loc.pattern.as_str(), sym.as_str()))
                        .or_insert(node.id.as_str());
                }
            }
        }
    }
    for (key, locs) in &model.source_map {
        let Some(host) = resp_host.get(key.as_str()) else { continue };
        for loc in locs {
            if let Some(sym) = &loc.symbol {
                by_symbol
                    .entry((loc.pattern.as_str(), sym.as_str()))
                    .or_insert(host);
            }
        }
    }

    // 2) file → deepest boundary-owning node (fallback for symbols the model
    //    didn't mint nodes/anchors for).
    let depth_of = |id: &str| -> usize {
        let mut d = 0;
        let mut cur = node_by_id.get(id).copied();
        let mut seen: HashSet<&str> = HashSet::new();
        while let Some(n) = cur {
            if !seen.insert(n.id.as_str()) {
                break;
            }
            match n.parent_id.as_deref().and_then(|p| node_by_id.get(p)) {
                Some(p) => {
                    d += 1;
                    cur = Some(*p);
                }
                None => break,
            }
        }
        d
    };
    // Boundary fallback for symbols the model didn't anchor: which owner's glob
    // claims the file. One entry per (owner, pattern), each scored by glob
    // specificity and the owner's tree depth, sorted best-first so the
    // MOST-SPECIFIC matching boundary wins — a narrow `crates/foo/**/*` beats a
    // catch-all `**/*`, instead of whichever owner merely sorts first by id. The
    // file then lands in its real owner, not a repo-wide net swallowing it.
    let boundary_globs: Vec<(&str, glob::Pattern, (usize, usize), usize)> = {
        let mut v: Vec<(&str, glob::Pattern, (usize, usize), usize)> = Vec::new();
        for (id, sources) in &model.boundaries {
            if !node_by_id.contains_key(id.as_str()) {
                continue;
            }
            let depth = depth_of(id.as_str());
            for s in sources {
                if let Ok(pat) = glob::Pattern::new(&s.pattern) {
                    v.push((id.as_str(), pat, pattern_rank(&s.pattern), depth));
                }
            }
        }
        // Most-specific glob first, then deepest owner, then id — deterministic.
        v.sort_by(|a, b| {
            b.2.cmp(&a.2)
                .then_with(|| b.3.cmp(&a.3))
                .then_with(|| a.0.cmp(b.0))
        });
        v
    };
    let file_owner = |file: &str| -> Option<&str> {
        boundary_globs
            .iter()
            .find(|(_, pat, _, _)| pat.matches(file))
            .map(|(id, _, _, _)| *id)
    };

    let resolve = |endpoint: &str| -> Option<&str> {
        let (path, name) = BuildEdges::split_symbol_key(endpoint)?;
        by_symbol
            .get(&(path, name))
            .copied()
            .or_else(|| file_owner(path))
    };

    // --- ancestor chains ---------------------------------------------------------
    let chain = |id: &str| -> Vec<&str> {
        let mut out = vec![];
        let mut cur = node_by_id.get(id).copied();
        let mut seen: HashSet<&str> = HashSet::new();
        while let Some(n) = cur {
            if !seen.insert(n.id.as_str()) {
                break;
            }
            out.push(n.id.as_str());
            cur = n.parent_id.as_deref().and_then(|p| node_by_id.get(p)).copied();
        }
        out
    };

    // --- roll every code edge up both chains ------------------------------------
    let mut pair_counts: BTreeMap<(&str, &str), u32> = BTreeMap::new();
    // Leaf edges, resolved to (node, symbol) on both ends, deduped — the
    // per-symbol detail behind the aggregate counts. See `ResolvedEdge`.
    // Value: (count, every underlying edge guessed).
    let mut resolved: BTreeMap<(&str, &str, &str, &str, &str, &str), (u32, bool)> = BTreeMap::new();
    for edge in &edges.symbol_edges {
        let (Some(src), Some(dst)) = (resolve(&edge.src), resolve(&edge.dst)) else {
            continue;
        };
        if src == dst {
            continue;
        }
        let src_chain = chain(src);
        let dst_chain = chain(dst);
        let src_set: HashSet<&str> = src_chain.iter().copied().collect();
        let dst_set: HashSet<&str> = dst_chain.iter().copied().collect();
        // Record the leaf edge unless one endpoint contains the other (then it
        // is internal to a subtree, not a connection between two of them).
        if !src_set.contains(dst) && !dst_set.contains(src) {
            let (s_file, s_sym) = BuildEdges::split_symbol_key(&edge.src).unwrap_or(("", ""));
            let (d_file, d_sym) = BuildEdges::split_symbol_key(&edge.dst).unwrap_or(("", ""));
            let slot = resolved.entry((src, dst, s_sym, d_sym, s_file, d_file)).or_insert((0, true));
            slot.0 += 1;
            slot.1 &= edge.guessed;
        }
        for &a in &src_chain {
            if dst_set.contains(a) {
                continue; // a contains dst — containment, not dependency
            }
            for &b in &dst_chain {
                if a == b || src_set.contains(b) {
                    continue;
                }
                *pair_counts.entry((a, b)).or_insert(0) += 1;
            }
        }
    }

    // --- outputs: evidence about the model's links, nothing more ----------------
    let link_audit: Vec<LinkAudit> = model
        .links
        .iter()
        .map(|l| LinkAudit {
            link_id: l.id.clone(),
            edge_count: pair_counts
                .get(&(l.src.as_str(), l.dst.as_str()))
                .copied()
                .unwrap_or(0),
        })
        .collect();

    let declared: HashSet<(&str, &str)> = model
        .links
        .iter()
        .map(|l| (l.src.as_str(), l.dst.as_str()))
        .collect();
    let unmodeled: Vec<DerivedEdge> = pair_counts
        .iter()
        .filter(|((a, b), _)| {
            let (Some(na), Some(nb)) = (node_by_id.get(*a), node_by_id.get(*b)) else {
                return false;
            };
            // Same-kind siblings only — same-parent pairs are where the rules
            // expect links; symbol wiring is volume, not architecture.
            na.kind == nb.kind
                && na.parent_id == nb.parent_id
                && na.kind != Kind::Symbol
                && !declared.contains(&(*a, *b))
                && !declared.contains(&(*b, *a))
        })
        .map(|((a, b), count)| DerivedEdge {
            src: (*a).to_string(),
            dst: (*b).to_string(),
            count: *count,
        })
        .collect();

    let resolved_edges: Vec<ResolvedEdge> = resolved
        .iter()
        .map(|((sn, dn, ss, ds, sf, df), (count, guessed))| ResolvedEdge {
            src_node: (*sn).to_string(),
            src_symbol: (*ss).to_string(),
            src_file: (*sf).to_string(),
            dst_node: (*dn).to_string(),
            dst_symbol: (*ds).to_string(),
            dst_file: (*df).to_string(),
            count: *count,
            guessed: *guessed,
        })
        .collect();

    DerivedGraph {
        link_audit,
        unmodeled,
        resolved_edges,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Link, Node, Responsibility, Source, SourceLocation};

    fn node(id: &str, kind: Kind, parent: Option<&str>) -> Node {
        Node {
            style: None,
            layer: None,
            id: id.into(),
            kind,
            name: id.into(),
            vagrant: None,
            stale: None,
            parent_id: parent.map(Into::into),
            external: None,
            technology: None,
            description: None,
            responsibilities: Vec::new(),
            properties: Vec::new(),
            icon: None,
            notes: None,
            position: None,
            directives: Vec::new(),
        }
    }

    fn anchor(model: &mut ScryModel, node_id: &str, file: &str, symbol: &str) {
        model.source_map.insert(
            node_id.into(),
            vec![SourceLocation {
                pattern: file.into(),
                symbol: Some(symbol.into()),
                line: Some(1),
                end_line: Some(5),
            }],
        );
    }

    /// Two symbols anchored in different containers; one import edge between
    /// them. The pair must roll up to component AND container level, audit the
    /// declared container link as code-backed, and flag the undeclared
    /// component pair as unmodeled.
    #[test]
    fn rollup_audit_and_unmodeled() {
        let mut m = ScryModel::new();
        m.nodes.push(node("sys", Kind::System, None));
        m.nodes.push(node("ca", Kind::Container, Some("sys")));
        m.nodes.push(node("cb", Kind::Container, Some("sys")));
        m.nodes.push(node("compa", Kind::Component, Some("ca")));
        m.nodes.push(node("compa2", Kind::Component, Some("ca")));
        m.nodes.push(node("compb", Kind::Component, Some("cb")));
        m.nodes.push(node("syma", Kind::Symbol, Some("compa")));
        m.nodes.push(node("syma2", Kind::Symbol, Some("compa2")));
        m.nodes.push(node("symb", Kind::Symbol, Some("compb")));
        anchor(&mut m, "syma", "a/src/x.ts", "useThing");
        anchor(&mut m, "syma2", "a/src/z.ts", "zed");
        anchor(&mut m, "symb", "b/src/y.ts", "thing");
        m.links.push(Link {
            kind: None,
            id: "link-1".into(),
            src: "ca".into(),
            dst: "cb".into(),
            label: "uses".into(),
            method: None,
        });
        m.links.push(Link {
            kind: None,
            id: "link-2".into(),
            src: "cb".into(),
            dst: "ca".into(),
            label: "asserted only".into(),
            method: None,
        });

        let edges = BuildEdges {
            external_imports: Vec::new(),
            types: Vec::new(),
            symbol_edges: vec![
                CachedEdge {
                    src: "a/src/x.ts#useThing@1".into(),
                    dst: "b/src/y.ts#thing@1".into(),
                    guessed: false,
                },
                // Intra-container, cross-component: sibling pair, undeclared.
                CachedEdge {
                    src: "a/src/x.ts#useThing@1".into(),
                    dst: "a/src/z.ts#zed@1".into(),
                    guessed: false,
                },
            ],
        };
        let g = derive_graph(&m, &edges);

        let audit = |id: &str| g.link_audit.iter().find(|a| a.link_id == id).unwrap().edge_count;
        assert_eq!(audit("link-1"), 1, "declared link is code-backed");
        assert_eq!(audit("link-2"), 0, "reverse claim is asserted-only");

        // compa→compa2: siblings the code connects, no declared link — candidate.
        assert!(
            g.unmodeled.iter().any(|e| e.src == "compa" && e.dst == "compa2"),
            "unmodeled sibling pair surfaces: {:?}",
            g.unmodeled
        );
        // compa→compb crosses containers (not siblings) — propagation covers it
        // via the declared ca→cb link, so it is NOT a candidate.
        assert!(g.unmodeled.iter().all(|e| !(e.src == "compa" && e.dst == "compb")));
        // Symbol pairs are volume, not architecture — never in unmodeled.
        assert!(g.unmodeled.iter().all(|e| e.src != "syma"));

        // resolved_edges keeps the per-symbol leaf detail the aggregates hide:
        // both code edges, resolved to (node, symbol) on each end, deduped.
        let find = |sn: &str, dn: &str| {
            g.resolved_edges
                .iter()
                .find(|e| e.src_node == sn && e.dst_node == dn)
        };
        let cross = find("syma", "symb").expect("cross-container leaf edge kept");
        assert_eq!((cross.src_symbol.as_str(), cross.dst_symbol.as_str()), ("useThing", "thing"));
        assert_eq!(cross.count, 1);
        let sib = find("syma", "syma2").expect("cross-component leaf edge kept");
        assert_eq!((sib.src_symbol.as_str(), sib.dst_symbol.as_str()), ("useThing", "zed"));
        // Only the two real edges — no containment/self rows.
        assert_eq!(g.resolved_edges.len(), 2, "{:?}", g.resolved_edges);
    }

    /// Endpoints with no symbol anchor fall back to the deepest boundary owner;
    /// edges inside one node (or up/down one chain) are containment and produce
    /// no evidence. The cross-container pair surfaces as an unmodeled candidate
    /// (sibling containers, no declared link).
    #[test]
    fn boundary_fallback_and_containment_skip() {
        let mut m = ScryModel::new();
        m.nodes.push(node("sys", Kind::System, None));
        m.nodes.push(node("ca", Kind::Container, Some("sys")));
        m.nodes.push(node("cb", Kind::Container, Some("sys")));
        m.boundaries.insert(
            "ca".into(),
            vec![Source { pattern: "a/**/*".into(), comment: None }],
        );
        m.boundaries.insert(
            "cb".into(),
            vec![Source { pattern: "b/**/*".into(), comment: None }],
        );

        let edges = BuildEdges {
            external_imports: Vec::new(),
            types: Vec::new(),
            symbol_edges: vec![
                // Cross-container: counts via file-owner fallback.
                CachedEdge { src: "a/m.ts#f@1".into(), dst: "b/n.ts#g@1".into(), guessed: false },
                // Same container: containment, never evidence.
                CachedEdge { src: "a/m.ts#f@1".into(), dst: "a/o.ts#h@1".into(), guessed: false },
            ],
        };
        let g = derive_graph(&m, &edges);
        assert_eq!(
            g.unmodeled,
            vec![DerivedEdge { src: "ca".into(), dst: "cb".into(), count: 1 }]
        );
    }

    /// A catch-all `**/*` boundary must NOT outrank a narrower boundary that
    /// also matches the file: most-specific glob wins, so a contested file lands
    /// in its real owner, not the repo-wide net. Regression for an App-Frontend
    /// `**/*` boundary (whose id sorts first) swallowing other crates' symbols.
    #[test]
    fn specific_boundary_beats_catch_all() {
        let mut m = ScryModel::new();
        m.nodes.push(node("sys", Kind::System, None));
        // c0's id sorts first AND its `**/*` matches everything — under the old
        // depth-then-id ordering it would win every contested file.
        m.nodes.push(node("c0", Kind::Container, Some("sys")));
        m.nodes.push(node("c1", Kind::Container, Some("sys")));
        m.nodes.push(node("c2", Kind::Container, Some("sys")));
        m.boundaries.insert("c0".into(), vec![Source { pattern: "**/*".into(), comment: None }]);
        m.boundaries.insert("c1".into(), vec![Source { pattern: "a/**/*".into(), comment: None }]);
        m.boundaries.insert("c2".into(), vec![Source { pattern: "b/**/*".into(), comment: None }]);

        let edges = BuildEdges {
            external_imports: Vec::new(),
            types: Vec::new(),
            // Both endpoints are also matched by c0's `**/*`; the specific owners
            // must win, yielding a real c1->c2 edge rather than c0->c0 (self).
            symbol_edges: vec![CachedEdge {
                src: "a/m.ts#f@1".into(),
                dst: "b/n.ts#g@1".into(),
                guessed: false,
            }],
        };
        let g = derive_graph(&m, &edges);
        assert_eq!(
            g.unmodeled,
            vec![DerivedEdge { src: "c1".into(), dst: "c2".into(), count: 1 }],
            "narrow boundaries own their files; the `**/*` net never wins a contested file"
        );
    }

    /// A responsibility anchor (resp-id key) resolves the symbol to its host
    /// node just like a definition anchor.
    #[test]
    fn responsibility_anchors_resolve_endpoints() {
        let mut m = ScryModel::new();
        m.nodes.push(node("sys", Kind::System, None));
        let mut sa = node("syma", Kind::Symbol, Some("sys"));
        sa.responsibilities.push(Responsibility {
            concern: None,
            id: "r1".into(),
            statement: "does".into(),
            vagrant: None,
            stale: None,
            stale_proposal: None,
            directives: Vec::new(),
            last_touched_at: None,
        });
        m.nodes.push(sa);
        m.nodes.push(node("symb", Kind::Symbol, Some("sys")));
        m.links.push(Link {
            kind: None,
            id: "link-1".into(),
            src: "syma".into(),
            dst: "symb".into(),
            label: "calls".into(),
            method: None,
        });
        m.source_map.insert(
            "r1".into(),
            vec![SourceLocation {
                pattern: "src/a.ts".into(),
                symbol: Some("doA".into()),
                line: Some(1),
                end_line: None,
            }],
        );
        anchor(&mut m, "symb", "src/b.ts", "doB");

        let edges = BuildEdges {
            external_imports: Vec::new(),
            types: Vec::new(),
            symbol_edges: vec![CachedEdge {
                src: "src/a.ts#doA@1".into(),
                dst: "src/b.ts#doB@9".into(),
                guessed: false,
            }],
        };
        let g = derive_graph(&m, &edges);
        assert_eq!(
            g.link_audit[0].edge_count, 1,
            "resp-anchored endpoint resolved and backs the declared link"
        );
    }
}

// --- Boundary ownership: which node's boundary glob owns a file ---------------
//
// A file can match several nodes' boundary globs at once — a root container's
// broad `**/*` overlapping a nested container's `crates/foo/**/*`. A file
// belongs to the most specific glob that matches it, so a broad boundary never
// double-claims every file it overlaps.


/// A boundary glob's specificity: the length of its literal prefix before the
/// first glob metacharacter. `**/*` → 0, `crates/core/**/*` → 11. The deepest
/// (longest-prefix) matching boundary wins a file.
pub fn pattern_specificity(pattern: &str) -> usize {
    pattern
        .find(|c: char| matches!(c, '*' | '?' | '[' | '{'))
        .unwrap_or(pattern.len())
}

/// Full ordering rank for contested files: literal-prefix length first,
/// pattern length as the tiebreak (`src/**/*.ts` outranks `src/**/*`). The
/// ONE ranking every ownership decision uses — health/drift attribution and
/// the link audit diverged here once, attributing the same file differently.
pub fn pattern_rank(pattern: &str) -> (usize, usize) {
    (pattern_specificity(pattern), pattern.len())
}

struct NodeBoundaries {
    node_id: String,
    /// (compiled glob, rank) for each boundary pattern that compiled.
    patterns: Vec<(glob::Pattern, (usize, usize))>,
}

/// Resolves file ownership across the whole model by most-specific matching
/// boundary. Build once, query per (node, file).
pub struct BoundaryOwnership {
    nodes: Vec<NodeBoundaries>,
}

impl BoundaryOwnership {
    pub fn new(model: &ScryModel) -> Self {
        let live: HashSet<&str> = model.nodes.iter().map(|n| n.id.as_str()).collect();
        Self::from_boundaries(&model.boundaries, &live)
    }

    /// Build ownership from a boundaries map directly, so callers spanning the
    /// committed/planned seam can pass a UNION of both layers' boundaries (each
    /// boundary has a single home — committed owns committed nodes', the draft
    /// owns plan-added ones — so completeness must see both).
    ///
    /// `live_nodes` are the ids of nodes that actually exist; a boundary keyed to
    /// a node that no longer does (a `replace_model` overwrite or a hand-edit that
    /// dropped the node but left its boundary — write-time GC catches the tool
    /// paths, not these) is skipped, so a dead broad glob can't double-claim files
    /// or inflate a coverage denominator.
    pub fn from_boundaries(
        boundaries: &BTreeMap<String, Vec<crate::Source>>,
        live_nodes: &HashSet<&str>,
    ) -> Self {
        let nodes = boundaries
            .iter()
            .filter(|(node_id, _)| live_nodes.contains(node_id.as_str()))
            .filter_map(|(node_id, sources)| {
                let patterns: Vec<(glob::Pattern, (usize, usize))> = sources
                    .iter()
                    .filter_map(|s| {
                        glob::Pattern::new(&s.pattern)
                            .ok()
                            .map(|p| (p, pattern_rank(&s.pattern)))
                    })
                    .collect();
                if patterns.is_empty() {
                    return None;
                }
                Some(NodeBoundaries {
                    node_id: node_id.clone(),
                    patterns,
                })
            })
            .collect();
        Self { nodes }
    }

    /// The highest rank among every node's boundaries that match `file`,
    /// or `None` when no boundary matches it at all.
    fn winning_specificity(&self, file: &str) -> Option<(usize, usize)> {
        self.nodes
            .iter()
            .flat_map(|n| n.patterns.iter())
            .filter(|(p, _)| p.matches(file))
            .map(|(_, rank)| *rank)
            .max()
    }

    /// The rank of `node_id`'s best boundary matching `file`, or `None`.
    fn node_match(&self, node_id: &str, file: &str) -> Option<(usize, usize)> {
        self.nodes
            .iter()
            .filter(|n| n.node_id == node_id)
            .flat_map(|n| n.patterns.iter())
            .filter(|(p, _)| p.matches(file))
            .map(|(_, rank)| *rank)
            .max()
    }

    /// Does `node_id` own `file`? True when the node has a matching boundary and
    /// it is (tied for) the most specific match in the model. A file matched only
    /// by equally-specific boundaries is owned by all of them (a rare tie, e.g.
    /// two hand-authored identical globs).
    pub fn owns(&self, node_id: &str, file: &str) -> bool {
        match (self.node_match(node_id, file), self.winning_specificity(file)) {
            (Some(mine), Some(win)) => mine == win,
            _ => false,
        }
    }

    /// Owned-file sets keyed by node id, for callers that want them precomputed.
    pub fn owned_by<'a>(
        &self,
        files: impl IntoIterator<Item = &'a str>,
    ) -> HashMap<String, Vec<String>> {
        let mut out: HashMap<String, Vec<String>> = HashMap::new();
        for file in files {
            let Some(win) = self.winning_specificity(file) else {
                continue;
            };
            for n in &self.nodes {
                if n
                    .patterns
                    .iter()
                    .filter(|(p, _)| p.matches(file))
                    .any(|(_, rank)| *rank == win)
                {
                    out.entry(n.node_id.clone()).or_default().push(file.to_string());
                }
            }
        }
        out
    }
}

// --- code-location ownership (source-map based) ------------------------------






#[cfg(test)]
mod ownership_tests {
    use super::*;
    use crate::ScryModel;

    fn model_with_boundaries(pairs: &[(&str, &str)]) -> ScryModel {
        let mut m = ScryModel::new();
        for (nid, glob) in pairs {
            // Each boundary's node must be live, or ownership now filters it out.
            m.nodes.push(node(nid, "container", nid, None, &[]));
            m.boundaries.insert(
                nid.to_string(),
                vec![serde_json::from_value(serde_json::json!({ "pattern": glob })).unwrap()],
            );
        }
        m
    }

    #[test]
    fn specificity_ranks_literal_prefix_length() {
        assert_eq!(pattern_specificity("**/*"), 0);
        assert_eq!(pattern_specificity("src/**/*"), 4);
        assert_eq!(pattern_specificity("crates/core/**/*"), 12);
        assert_eq!(pattern_specificity("exact/file.rs"), 13);
    }

    #[test]
    fn broad_boundary_yields_files_to_nested_one() {
        // Root `**/*` overlaps a nested crate. The nested crate owns its files;
        // the root owns only what nothing more specific claims.
        let m = model_with_boundaries(&[("root", "**/*"), ("core", "crates/core/**/*")]);
        let own = BoundaryOwnership::new(&m);

        assert!(own.owns("core", "crates/core/lib.rs"));
        assert!(!own.owns("root", "crates/core/lib.rs"));

        assert!(own.owns("root", "src/main.ts"));
        assert!(!own.owns("core", "src/main.ts"));
    }

    #[test]
    fn a_boundary_without_a_live_node_is_ignored() {
        // A broad glob left behind by a dropped node (a replace_model overwrite or a
        // hand-edit that write-time GC never saw) must not claim files.
        let mut m = model_with_boundaries(&[("core", "crates/core/**/*")]);
        m.boundaries.insert(
            "ghost".to_string(),
            vec![serde_json::from_value(serde_json::json!({ "pattern": "**/*" })).unwrap()],
        );
        let own = BoundaryOwnership::new(&m);
        assert!(own.owns("core", "crates/core/lib.rs"));
        assert!(!own.owns("ghost", "src/main.ts"), "dead boundary claims nothing");
        assert!(!own.owns("ghost", "crates/core/lib.rs"));
    }

    #[test]
    fn unmatched_file_is_owned_by_no_one() {
        let m = model_with_boundaries(&[("core", "crates/core/**/*")]);
        let own = BoundaryOwnership::new(&m);
        assert!(!own.owns("core", "docs/index.md"));
    }

    #[test]
    fn owned_by_partitions_by_most_specific() {
        let m = model_with_boundaries(&[("root", "**/*"), ("core", "crates/core/**/*")]);
        let own = BoundaryOwnership::new(&m);
        let map = own.owned_by(["crates/core/lib.rs", "src/main.ts"]);
        assert_eq!(map.get("core").map(|v| v.as_slice()), Some(&["crates/core/lib.rs".to_string()][..]));
        assert_eq!(map.get("root").map(|v| v.as_slice()), Some(&["src/main.ts".to_string()][..]));
    }

    // --- owning_node_for_location ------------------------------------------------

    fn node(id: &str, kind: &str, name: &str, parent: Option<&str>, resp: &[&str]) -> crate::Node {
        let resps: Vec<_> = resp
            .iter()
            .map(|rid| serde_json::json!({ "id": rid, "statement": "x" }))
            .collect();
        serde_json::from_value(serde_json::json!({
            "id": id, "kind": kind, "name": name, "parentId": parent,
            "responsibilities": resps,
        }))
        .unwrap()
    }





}
