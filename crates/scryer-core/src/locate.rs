//! Reverse lookup: a source location → the intent that governs it.
//!
//! Every other read starts from the model and walks toward code. `locate`
//! starts where an agent actually starts — a file it just read or is about to
//! edit — and returns that location's slice of the model: the claims anchored
//! there, the finest node that maps the file and its ancestor chain, the
//! boundary owner of the region, and the directives binding the location.

use crate::ownership::{node_ancestry, node_depth, owning_node_for_location, BoundaryOwnership};
use crate::{inherited_directives, InheritedDirectives, Kind, ScryModel, SourceLocation};
use serde::Serialize;

/// One node in the located ancestor chain (or the boundary owner).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OwnerLink {
    pub id: String,
    pub name: String,
    pub kind: Kind,
}

/// One piece of intent anchored at the located file: a responsibility claim,
/// or (with `statement` absent) a data-shape declaration keyed by node id.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocatedClaim {
    /// Responsibility id, or the node id for a schema declaration anchor.
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub statement: Option<String>,
    /// The node (or group) hosting the claim.
    pub host_id: String,
    pub host_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stale: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vagrant: Option<bool>,
    /// The claim's own binding directives (node-level inheritance is reported
    /// once on the result, not repeated per claim).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub directives: Vec<String>,
    pub anchor: SourceLocation,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocateResult {
    /// The finest node mapping the file, followed by each ancestor to the root.
    /// Empty when nothing maps or owns the file.
    pub owner_chain: Vec<OwnerLink>,
    /// The node whose boundary glob owns the file (most-specific match).
    pub boundary_owner: Option<OwnerLink>,
    pub claims: Vec<LocatedClaim>,
    /// The finest node's own directives; ancestors' contributions follow in
    /// `inherited_directives`, nearest first.
    pub own_directives: Vec<String>,
    pub inherited_directives: Vec<InheritedDirectives>,
    /// True when a requested symbol narrowed `claims`; false when the symbol
    /// matched no anchor and whole-file claims are returned instead.
    pub symbol_matched: bool,
}

/// Does a source-map anchor `pattern` cover `file`? Anchors are normally exact
/// project-relative paths, but `SourceLocation.pattern` documents glob support,
/// so a glob anchor must still be findable from the files it covers.
fn pattern_covers(pattern: &str, file: &str) -> bool {
    pattern == file
        || glob::Pattern::new(pattern).is_ok_and(|p| p.matches(file))
}

/// Resolve `file` (project-relative, `/`-separated) against the model. Pass the
/// WORKING view when plan edits must be visible. `symbol` narrows to claims
/// anchored to that identifier when any are; otherwise all of the file's claims
/// are returned with `symbol_matched: false`.
pub fn locate(model: &ScryModel, file: &str, symbol: Option<&str>) -> LocateResult {
    // Every (key, anchor) pair whose anchor covers the file.
    let mut hits: Vec<(&str, &SourceLocation)> = model
        .source_map
        .iter()
        .flat_map(|(key, locs)| locs.iter().map(move |l| (key.as_str(), l)))
        .filter(|(_, l)| pattern_covers(&l.pattern, file))
        .collect();
    let mut symbol_matched = false;
    if let Some(sym) = symbol {
        let narrowed: Vec<_> = hits
            .iter()
            .filter(|(_, l)| l.symbol.as_deref() == Some(sym))
            .cloned()
            .collect();
        if !narrowed.is_empty() {
            hits = narrowed;
            symbol_matched = true;
        }
    }

    let claims: Vec<LocatedClaim> = hits
        .iter()
        .filter_map(|(key, loc)| resolve_claim(model, key, loc))
        .collect();

    let boundary_owner = boundary_owner(model, file);

    // The finest node mapping the file (exact-path source-map entries), with
    // the boundary owner as the fallback scope for unmapped files.
    let fallback = boundary_owner.as_ref().map(|o| o.id.clone()).unwrap_or_default();
    let finest = owning_node_for_location(model, &fallback, file, symbol);
    let owner_chain: Vec<OwnerLink> = if finest.is_empty() {
        Vec::new()
    } else {
        node_ancestry(model, &finest)
            .iter()
            .filter_map(|id| owner_link(model, id))
            .collect()
    };

    let (own_directives, inherited) = match owner_chain.first() {
        Some(o) => {
            let own = model
                .nodes
                .iter()
                .find(|n| n.id == o.id)
                .map(|n| n.directives.clone())
                .unwrap_or_default();
            (own, inherited_directives(model, &o.id))
        }
        None => (Vec::new(), Vec::new()),
    };

    LocateResult {
        owner_chain,
        boundary_owner,
        claims,
        own_directives,
        inherited_directives: inherited,
        symbol_matched,
    }
}

/// The full file→intent report against the model on disk: working-view locate
/// plus the derived context every consumer wants — the breadcrumb path and the
/// pending plan entries scoped to the located elements. The single payload
/// generator behind the MCP `locate` tool and the session-hook overlay, so the
/// two surfaces can never drift apart.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocateReport {
    #[serde(flatten)]
    pub result: LocateResult,
    /// Root-first breadcrumb of the owner chain, e.g. "Acme / API / Auth".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Plan entries (committed→planned diff) touching the located elements.
    pub pending: Vec<crate::diff::ElementChange>,
}

/// Resolve `file` against the project's WORKING view (plan + committed anchors)
/// and scope the plan diff to what was located. `file` must already be
/// project-relative and `/`-separated.
pub fn locate_at(
    r: &crate::ModelRef,
    file: &str,
    symbol: Option<&str>,
) -> Result<LocateReport, String> {
    let committed = crate::read_model_at(r)?;
    let planned = crate::read_planned_at(r)?;
    let working = crate::working_view(&committed, &planned);

    let result = locate(&working, file, symbol);

    let mut located_ids: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for o in &result.owner_chain {
        located_ids.insert(&o.id);
    }
    if let Some(b) = &result.boundary_owner {
        located_ids.insert(&b.id);
    }
    for c in &result.claims {
        located_ids.insert(&c.id);
        located_ids.insert(&c.host_id);
    }
    let pending: Vec<crate::diff::ElementChange> = crate::plan_diff_at(r)?
        .changes
        .into_iter()
        .filter(|c| {
            located_ids.contains(c.id.as_str())
                || c.owner_id.as_deref().is_some_and(|o| located_ids.contains(o))
        })
        .collect();

    let path = (!result.owner_chain.is_empty()).then(|| {
        result
            .owner_chain
            .iter()
            .rev()
            .map(|o| o.name.as_str())
            .collect::<Vec<_>>()
            .join(" / ")
    });

    Ok(LocateReport { result, path, pending })
}

fn owner_link(model: &ScryModel, id: &str) -> Option<OwnerLink> {
    model.nodes.iter().find(|n| n.id == id).map(|n| OwnerLink {
        id: n.id.clone(),
        name: n.name.clone(),
        kind: n.kind,
    })
}

/// The node whose boundary glob owns `file` — the deepest node when several
/// tie at the winning specificity.
fn boundary_owner(model: &ScryModel, file: &str) -> Option<OwnerLink> {
    let ownership = BoundaryOwnership::new(model);
    let mut owners: Vec<&str> = model
        .boundaries
        .keys()
        .filter(|id| ownership.owns(id, file))
        .map(|id| id.as_str())
        .collect();
    owners.sort_by_key(|id| std::cmp::Reverse(node_depth(model, id)));
    owners.first().and_then(|id| owner_link(model, id))
}

/// Resolve a source-map key to the claim it anchors: a responsibility on a
/// node or group, or a node-keyed data-shape declaration. Keys resolving to
/// nothing (validator territory) are skipped.
fn resolve_claim(model: &ScryModel, key: &str, loc: &SourceLocation) -> Option<LocatedClaim> {
    for n in &model.nodes {
        if let Some(r) = n.responsibilities.iter().find(|r| r.id == key) {
            return Some(LocatedClaim {
                id: r.id.clone(),
                statement: Some(r.statement.clone()),
                host_id: n.id.clone(),
                host_name: n.name.clone(),
                stale: r.stale,
                vagrant: r.vagrant,
                directives: r.directives.clone(),
                anchor: loc.clone(),
            });
        }
        // A node-keyed anchor: the declaration site of a data-shape symbol.
        if n.id == key {
            return Some(LocatedClaim {
                id: n.id.clone(),
                statement: None,
                host_id: n.id.clone(),
                host_name: n.name.clone(),
                stale: n.stale,
                vagrant: n.vagrant,
                directives: n.directives.clone(),
                anchor: loc.clone(),
            });
        }
    }
    for g in &model.groups {
        if let Some(r) = g.responsibilities.iter().find(|r| r.id == key) {
            return Some(LocatedClaim {
                id: r.id.clone(),
                statement: Some(r.statement.clone()),
                host_id: g.id.clone(),
                host_name: g.name.clone(),
                stale: r.stale,
                vagrant: r.vagrant,
                directives: r.directives.clone(),
                anchor: loc.clone(),
            });
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ScryModel;

    fn node(id: &str, kind: &str, name: &str, parent: Option<&str>, resp: &[(&str, &str)]) -> crate::Node {
        let resps: Vec<_> = resp
            .iter()
            .map(|(rid, s)| serde_json::json!({ "id": rid, "statement": s }))
            .collect();
        serde_json::from_value(serde_json::json!({
            "id": id, "kind": kind, "name": name, "parentId": parent,
            "responsibilities": resps,
        }))
        .unwrap()
    }

    fn loc(file: &str, symbol: Option<&str>) -> crate::SourceLocation {
        serde_json::from_value(serde_json::json!({ "pattern": file, "symbol": symbol })).unwrap()
    }

    /// System > container `api` (boundary src/**) > component `auth` with two
    /// symbols anchored in the same file.
    fn model() -> ScryModel {
        let mut m = ScryModel::new();
        m.nodes.push(node("sys", "system", "Acme", None, &[]));
        m.nodes.push(node("api", "container", "API", Some("sys"), &[]));
        m.nodes.push(node("auth", "component", "Auth", Some("api"), &[]));
        m.nodes.push(node(
            "vt", "symbol", "verify_token", Some("auth"),
            &[("r-vt", "rejects forged credentials")],
        ));
        m.nodes.push(node(
            "hp", "symbol", "hash_password", Some("auth"),
            &[("r-hp", "stores only salted hashes")],
        ));
        m.source_map.insert("r-vt".into(), vec![loc("src/auth.rs", Some("verify_token"))]);
        m.source_map.insert("r-hp".into(), vec![loc("src/auth.rs", Some("hash_password"))]);
        m.boundaries.insert(
            "api".into(),
            vec![serde_json::from_value(serde_json::json!({ "pattern": "src/**/*" })).unwrap()],
        );
        m
    }

    #[test]
    fn file_lookup_returns_all_claims_and_the_common_ancestor_chain() {
        let res = locate(&model(), "src/auth.rs", None);
        assert_eq!(res.claims.len(), 2);
        // Two symbols tie for deepest → chain starts at their component.
        let ids: Vec<&str> = res.owner_chain.iter().map(|o| o.id.as_str()).collect();
        assert_eq!(ids, ["auth", "api", "sys"]);
        assert_eq!(res.boundary_owner.as_ref().map(|o| o.id.as_str()), Some("api"));
        assert!(!res.symbol_matched);
    }

    #[test]
    fn symbol_narrows_claims_and_chain() {
        let res = locate(&model(), "src/auth.rs", Some("verify_token"));
        assert!(res.symbol_matched);
        assert_eq!(res.claims.len(), 1);
        assert_eq!(res.claims[0].id, "r-vt");
        assert_eq!(res.owner_chain[0].id, "vt");
    }

    #[test]
    fn unknown_symbol_falls_back_to_whole_file_claims() {
        let res = locate(&model(), "src/auth.rs", Some("no_such_fn"));
        assert!(!res.symbol_matched);
        assert_eq!(res.claims.len(), 2);
    }

    #[test]
    fn unanchored_file_reports_only_the_boundary_owner() {
        let res = locate(&model(), "src/dark.rs", None);
        assert!(res.claims.is_empty());
        assert_eq!(res.boundary_owner.as_ref().map(|o| o.id.as_str()), Some("api"));
        // Fallback scope: the chain is the boundary owner's ancestry.
        assert_eq!(res.owner_chain[0].id, "api");
    }

    #[test]
    fn unowned_file_returns_empty() {
        let res = locate(&model(), "docs/readme.md", None);
        assert!(res.claims.is_empty());
        assert!(res.boundary_owner.is_none());
        assert!(res.owner_chain.is_empty());
    }

    #[test]
    fn glob_anchor_is_found_from_a_covered_file() {
        let mut m = model();
        m.source_map.insert("r-vt".into(), vec![loc("src/auth/**/*.rs", Some("verify_token"))]);
        let res = locate(&m, "src/auth/token.rs", None);
        assert_eq!(res.claims.len(), 1);
        assert_eq!(res.claims[0].id, "r-vt");
    }

    #[test]
    fn directives_come_from_the_finest_node_and_its_ancestry() {
        let mut m = model();
        m.nodes.iter_mut().find(|n| n.id == "auth").unwrap().directives =
            vec!["must never log tokens".into()];
        m.nodes.iter_mut().find(|n| n.id == "api").unwrap().directives =
            vec!["must stay stateless".into()];
        let res = locate(&m, "src/auth.rs", None);
        assert_eq!(res.own_directives, vec!["must never log tokens".to_string()]);
        assert_eq!(res.inherited_directives.len(), 1);
        assert_eq!(res.inherited_directives[0].node_id, "api");
    }
}
