//! The deterministic codebase dependency graph, cached to disk for one model
//! build. The build orchestrator (in the Tauri app) extracts it once; the MCP
//! `commit_container_model` tool — which runs in a separate process and has no
//! parser of its own — reads it back to wire code-level links from the same
//! edges the modeling agent was shown, instead of asking the agent to author
//! (and mis-author) them by hand.
//!
//! Endpoints are the extractor's source-anchored symbol keys (`path#name@line`)
//! and project-relative file paths. They carry NO C4 structure — the commit
//! tool joins them back to freshly minted nodes by source location.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// A directed dependency edge between two source-anchored endpoints.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CachedEdge {
    pub src: String,
    pub dst: String,
}

/// The whole-project dependency graph for one build.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildEdges {
    /// Symbol→symbol edges, keyed by `path#name@startLine`.
    #[serde(default)]
    pub symbol_edges: Vec<CachedEdge>,
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

/// Persist the build dependency graph next to the model. Best-effort callers
/// should ignore the error — a missing cache only means the commit tool wires
/// no automatic links, not that the build fails.
pub fn write_build_edges(project: &Path, edges: &BuildEdges) -> Result<(), String> {
    let dir = project.join(".scryer");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let json = serde_json::to_string(edges).map_err(|e| e.to_string())?;
    std::fs::write(dir.join(".build_edges.json"), json).map_err(|e| e.to_string())
}

/// Read the cached build dependency graph, if one was written for this build.
pub fn read_build_edges(path: &Path) -> Option<BuildEdges> {
    let json = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&json).ok()
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
    pub dst_node: String,
    pub dst_symbol: String,
    pub count: u32,
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

/// Glob specificity for boundary-fallback ownership — higher wins. `.0` is the
/// literal-prefix length (chars before the first wildcard), `.1` the pattern
/// length as a tiebreak. So `crates/acp/**/*` (long literal prefix) outranks a
/// catch-all `**/*` (zero), and a contested file lands in its real owner.
fn glob_specificity(pattern: &str) -> (usize, usize) {
    let prefix = pattern
        .find(|c| matches!(c, '*' | '?' | '['))
        .unwrap_or(pattern.len());
    (prefix, pattern.len())
}

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
                    v.push((id.as_str(), pat, glob_specificity(&s.pattern), depth));
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
    let mut resolved: BTreeMap<(&str, &str, &str, &str), u32> = BTreeMap::new();
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
            let s_sym = BuildEdges::split_symbol_key(&edge.src).map_or("", |(_, n)| n);
            let d_sym = BuildEdges::split_symbol_key(&edge.dst).map_or("", |(_, n)| n);
            *resolved.entry((src, dst, s_sym, d_sym)).or_insert(0) += 1;
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
        .map(|((sn, dn, ss, ds), count)| ResolvedEdge {
            src_node: (*sn).to_string(),
            src_symbol: (*ss).to_string(),
            dst_node: (*dn).to_string(),
            dst_symbol: (*ds).to_string(),
            count: *count,
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
            visual: None,
            appearance: None,
            notes: None,
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
                command: None,
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
            id: "link-1".into(),
            src: "ca".into(),
            dst: "cb".into(),
            label: "uses".into(),
            method: None,
        });
        m.links.push(Link {
            id: "link-2".into(),
            src: "cb".into(),
            dst: "ca".into(),
            label: "asserted only".into(),
            method: None,
        });

        let edges = BuildEdges {
            symbol_edges: vec![
                CachedEdge {
                    src: "a/src/x.ts#useThing@1".into(),
                    dst: "b/src/y.ts#thing@1".into(),
                },
                // Intra-container, cross-component: sibling pair, undeclared.
                CachedEdge {
                    src: "a/src/x.ts#useThing@1".into(),
                    dst: "a/src/z.ts#zed@1".into(),
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
            symbol_edges: vec![
                // Cross-container: counts via file-owner fallback.
                CachedEdge { src: "a/m.ts#f@1".into(), dst: "b/n.ts#g@1".into() },
                // Same container: containment, never evidence.
                CachedEdge { src: "a/m.ts#f@1".into(), dst: "a/o.ts#h@1".into() },
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
            // Both endpoints are also matched by c0's `**/*`; the specific owners
            // must win, yielding a real c1->c2 edge rather than c0->c0 (self).
            symbol_edges: vec![CachedEdge {
                src: "a/m.ts#f@1".into(),
                dst: "b/n.ts#g@1".into(),
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
            id: "r1".into(),
            statement: "does".into(),
            vagrant: None,
            stale: None,
            directives: Vec::new(),
            last_touched_at: None,
        });
        m.nodes.push(sa);
        m.nodes.push(node("symb", Kind::Symbol, Some("sys")));
        m.links.push(Link {
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
                command: None,
            }],
        );
        anchor(&mut m, "symb", "src/b.ts", "doB");

        let edges = BuildEdges {
            symbol_edges: vec![CachedEdge {
                src: "src/a.ts#doA@1".into(),
                dst: "src/b.ts#doB@9".into(),
            }],
        };
        let g = derive_graph(&m, &edges);
        assert_eq!(
            g.link_audit[0].edge_count, 1,
            "resp-anchored endpoint resolved and backs the declared link"
        );
    }
}
