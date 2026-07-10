//! Most-specific boundary ownership.
//!
//! A file can match several nodes' boundary globs at once — most commonly a root
//! container's broad `**/*` overlapping a nested container's `crates/foo/**/*`.
//! The extractor and the build already resolve this by *longest-prefix
//! ownership*: a file belongs to the single deepest container whose directory is
//! a prefix of it. Health and drift must use the same rule, or a broad boundary
//! double-claims every file it overlaps — inflating its coverage denominator and
//! flagging it as drifted whenever any file anywhere changes.
//!
//! [`BoundaryOwnership`] compiles every node's boundary globs once and answers
//! "does node N own file F?" by comparing N's best matching glob against the most
//! specific glob anywhere in the model that also matches F.

use crate::ScryModel;
use std::collections::{HashMap, HashSet};

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
    /// a node that no longer does (a `set_model` overwrite or a hand-edit that
    /// dropped the node but left its boundary — write-time GC catches the tool
    /// paths, not these) is skipped, so a dead broad glob can't double-claim files
    /// or inflate a coverage denominator.
    pub fn from_boundaries(
        boundaries: &HashMap<String, Vec<crate::Source>>,
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

/// Resolve the node that should host a drift finding at `(source_file, symbol)`.
///
/// Drift is reviewed per container, so attaching every finding to the reviewed
/// node parks code-level behaviour on the container. But the source map already
/// ties each file to the symbol/component that implements it — `src/TopBar.tsx` →
/// the `TopBar` symbol, `validate.rs` → the `validate` symbol. This routes a
/// finding to the FINEST node that maps the file (preferring a symbol node whose
/// name is the cited `symbol`), so undescribed behaviour lands at its true
/// altitude. Falls back to `scope_id` (the reviewed container) only when no node
/// maps the file yet — a brand-new file with no symbol node.
pub fn owning_node_for_location(
    model: &ScryModel,
    scope_id: &str,
    source_file: &str,
    symbol: Option<&str>,
) -> String {
    let candidates: Vec<&crate::Node> = model
        .nodes
        .iter()
        .filter(|n| node_maps_file(model, n, source_file))
        .collect();

    // 1. The exact symbol node, when the finding names one that maps the file.
    if let Some(sym) = symbol {
        if let Some(n) = candidates.iter().find(|n| n.name == sym) {
            return n.id.clone();
        }
    }

    // 2. The finest node mapping the file. When several share the deepest level
    //    (many symbols in one file) and the symbol didn't disambiguate, attach to
    //    their common ancestor — the component that owns the file — rather than an
    //    arbitrary sibling symbol.
    if let Some(max_depth) = candidates.iter().map(|n| node_depth(model, &n.id)).max() {
        let deepest: Vec<&str> = candidates
            .iter()
            .filter(|n| node_depth(model, &n.id) == max_depth)
            .map(|n| n.id.as_str())
            .collect();
        if deepest.len() == 1 {
            return deepest[0].to_string();
        }
        if let Some(anc) = lowest_common_ancestor(model, &deepest) {
            return anc;
        }
    }

    // 3. No node maps the file yet — keep the finding on the reviewed container.
    scope_id.to_string()
}

/// Does `node` claim `file` through the source map — a direct node-key mapping
/// (a schema/symbol declaration site) or any of its responsibilities' locations?
/// A location's pattern matches by exact path OR as a glob (the same rule
/// `locate::anchor_matches` uses), so a claim anchored via a glob — `src/parsers/
/// **` — still routes the finding to its host instead of falling back to the
/// reviewed container.
fn node_maps_file(model: &ScryModel, node: &crate::Node, file: &str) -> bool {
    let maps = |key: &str| {
        model.source_map.get(key).is_some_and(|locs| {
            locs.iter().any(|l| {
                l.pattern == file || glob::Pattern::new(&l.pattern).is_ok_and(|p| p.matches(file))
            })
        })
    };
    maps(&node.id) || node.responsibilities.iter().any(|r| maps(&r.id))
}

/// Depth in the parent tree (a top-level node is 0).
pub(crate) fn node_depth(model: &ScryModel, id: &str) -> usize {
    node_ancestry(model, id).len().saturating_sub(1)
}

/// `[id, parent, …, root]` — the node followed by each ancestor.
pub(crate) fn node_ancestry(model: &ScryModel, id: &str) -> Vec<String> {
    let mut chain = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut cur = Some(id.to_string());
    while let Some(c) = cur {
        if !seen.insert(c.clone()) {
            break; // cycle guard
        }
        cur = model
            .nodes
            .iter()
            .find(|n| n.id == c)
            .and_then(|n| n.parent_id.clone());
        chain.push(c);
    }
    chain
}

/// The deepest node that is an ancestor (or self) of every id in `ids`.
fn lowest_common_ancestor(model: &ScryModel, ids: &[&str]) -> Option<String> {
    let mut iter = ids.iter();
    let first = iter.next()?;
    // Root-first so the shared prefix is the common ancestry.
    let mut common: Vec<String> = node_ancestry(model, first);
    common.reverse();
    for id in iter {
        let mut anc = node_ancestry(model, id);
        anc.reverse();
        let keep = common
            .iter()
            .zip(anc.iter())
            .take_while(|(a, b)| a == b)
            .count();
        common.truncate(keep);
        if common.is_empty() {
            return None;
        }
    }
    common.last().cloned()
}

#[cfg(test)]
mod tests {
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
        // A broad glob left behind by a dropped node (a set_model overwrite or a
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

    /// container `c` → component `comp` → symbols `TopBar`, `Other`; a stray
    /// brand-new file is mapped to nothing.
    fn frontend_model() -> ScryModel {
        let mut m = ScryModel::new();
        m.nodes.push(node("c", "container", "App Frontend", None, &[]));
        m.nodes.push(node("comp", "component", "Workspace Chrome", Some("c"), &[]));
        m.nodes.push(node("tb", "symbol", "TopBar", Some("comp"), &["r-tb"]));
        m.nodes.push(node("ot", "symbol", "Other", Some("comp"), &["r-ot"]));
        let loc = |f: &str| vec![serde_json::from_value(serde_json::json!({ "pattern": f })).unwrap()];
        m.source_map.insert("r-tb".into(), loc("src/TopBar.tsx"));
        m.source_map.insert("r-ot".into(), loc("src/Other.tsx"));
        m.boundaries.insert(
            "c".into(),
            vec![serde_json::from_value(serde_json::json!({ "pattern": "**/*" })).unwrap()],
        );
        m
    }

    #[test]
    fn routes_to_the_symbol_that_maps_the_file() {
        let m = frontend_model();
        // Named symbol → that symbol, not the reviewed container.
        assert_eq!(owning_node_for_location(&m, "c", "src/TopBar.tsx", Some("TopBar")), "tb");
        // Even with no symbol, the only node mapping the file is the symbol.
        assert_eq!(owning_node_for_location(&m, "c", "src/Other.tsx", None), "ot");
    }

    #[test]
    fn glob_anchored_claim_routes_to_its_host() {
        let mut m = frontend_model();
        // Anchor the TopBar symbol's claim via a GLOB. A file the glob covers
        // must route to that symbol, not fall back to the reviewed container.
        m.source_map.insert(
            "r-tb".into(),
            vec![serde_json::from_value(serde_json::json!({ "pattern": "src/parsers/**/*.rs" })).unwrap()],
        );
        assert_eq!(owning_node_for_location(&m, "c", "src/parsers/json.rs", None), "tb");
    }

    #[test]
    fn ambiguous_file_falls_back_to_the_common_ancestor() {
        let mut m = frontend_model();
        // Both symbols now map the SAME file; with no symbol cue, attach to the
        // component that owns them, not an arbitrary sibling.
        let loc = |f: &str| vec![serde_json::from_value(serde_json::json!({ "pattern": f })).unwrap()];
        m.source_map.insert("r-tb".into(), loc("src/shared.tsx"));
        m.source_map.insert("r-ot".into(), loc("src/shared.tsx"));
        assert_eq!(owning_node_for_location(&m, "c", "src/shared.tsx", None), "comp");
        // A symbol cue still disambiguates to the exact symbol.
        assert_eq!(owning_node_for_location(&m, "c", "src/shared.tsx", Some("Other")), "ot");
    }

    #[test]
    fn unmapped_file_stays_on_the_reviewed_container() {
        let m = frontend_model();
        assert_eq!(owning_node_for_location(&m, "c", "src/brand-new.tsx", Some("Thing")), "c");
    }
}
