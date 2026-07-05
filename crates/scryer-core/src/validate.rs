use crate::{Kind, ScryModel};
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// Maximum description length, in Unicode scalar values (characters, not bytes).
/// The frontend hard-caps description editors at the same value, so a UI-authored
/// description never trips this warning.
pub const DESCRIPTION_MAX_CHARS: usize = 200;

/// Run structural validation. Returns a list of human-readable warnings.
pub fn validate(model: &ScryModel) -> Vec<String> {
    let mut warnings: Vec<String> = Vec::new();

    let node_ids: HashSet<&str> = model.nodes.iter().map(|n| n.id.as_str()).collect();
    let group_ids: HashSet<&str> = model.groups.iter().map(|g| g.id.as_str()).collect();

    // --- Nodes ---
    let mut seen_node_ids: HashSet<&str> = HashSet::new();
    for n in &model.nodes {
        if !seen_node_ids.insert(n.id.as_str()) {
            warnings.push(format!("Duplicate node id: {}", n.id));
        }

        match &n.parent_id {
            Some(pid) => {
                if !node_ids.contains(pid.as_str()) {
                    warnings.push(format!(
                        "Node {} (\"{}\") has parent_id '{}' that doesn't exist",
                        n.id, n.name, pid
                    ));
                } else if let Some(parent) = model.nodes.iter().find(|p| p.id == *pid) {
                    let valid = matches!(
                        (parent.kind, n.kind),
                        (Kind::System, Kind::Container)
                            | (Kind::Container, Kind::Component)
                            | (Kind::Component, Kind::Symbol)
                    );
                    if !valid {
                        warnings.push(format!(
                            "Node {} (\"{}\") kind {:?} cannot have parent of kind {:?}",
                            n.id, n.name, n.kind, parent.kind
                        ));
                    }
                    if parent.external == Some(true) {
                        warnings.push(format!(
                            "Node {} (\"{}\") is a child of external node {} — externals have no children",
                            n.id, n.name, parent.id
                        ));
                    }
                }
            }
            None => {
                if !matches!(n.kind, Kind::Person | Kind::System) {
                    warnings.push(format!(
                        "Node {} (\"{}\") of kind {:?} has no parent — only person/system are top-level",
                        n.id, n.name, n.kind
                    ));
                }
            }
        }

        if n.external == Some(true) && !matches!(n.kind, Kind::System | Kind::Container) {
            warnings.push(format!(
                "Node {} (\"{}\") has external=true but kind is {:?} — only system/container can be external",
                n.id, n.name, n.kind
            ));
        }

        let mut resp_ids: HashSet<&str> = HashSet::new();
        for r in &n.responsibilities {
            if !resp_ids.insert(r.id.as_str()) {
                warnings.push(format!(
                    "Duplicate responsibility id '{}' on node {}",
                    r.id, n.id
                ));
            }
            if r.statement.trim().is_empty() {
                warnings.push(format!(
                    "Empty responsibility statement on node {} (id={})",
                    n.id, r.id
                ));
            }
        }

        if let Some(desc) = &n.description {
            let chars = desc.chars().count();
            if chars > DESCRIPTION_MAX_CHARS {
                warnings.push(format!(
                    "Node {} (\"{}\") description exceeds {} characters ({})",
                    n.id, n.name, DESCRIPTION_MAX_CHARS, chars
                ));
            }
        }

        // Symbol names are source identifiers. Types, classes, interfaces, and
        // React components legitimately start uppercase, so an uppercase start
        // is allowed — we only reject names that aren't identifier-shaped at all
        // (spaces, punctuation, leading digits).
        if n.kind == Kind::Symbol && !is_valid_identifier(&n.name, true) {
            warnings.push(format!(
                "Symbol node {} (\"{}\") name should be a valid identifier",
                n.id, n.name
            ));
        }

        // Empty symbol — carries no semantic content of its own: no
        // responsibility, no declared data shape, no rendered appearance. Such a
        // node justifies nothing on the diagram. This is a flag, not a hard
        // error: the agent must resolve each one by giving it a business
        // responsibility or removing it (folding it into the parent symbol that
        // uses it). Scoped to symbols — structural nodes carry meaning through
        // their children.
        if crate::is_node_empty(n) {
            warnings.push(format!(
                "Symbol node {} (\"{}\") is empty — give it a responsibility or data shape, or fold it into its parent and remove it (a link alone does not justify it)",
                n.id, n.name
            ));
        }
    }

    // --- Links ---
    let mut seen_link_ids: HashSet<&str> = HashSet::new();
    for l in &model.links {
        if !seen_link_ids.insert(l.id.as_str()) {
            warnings.push(format!("Duplicate link id: {}", l.id));
        }
        if !node_ids.contains(l.src.as_str()) {
            warnings.push(format!("Link {} has unknown src '{}'", l.id, l.src));
        }
        if !node_ids.contains(l.dst.as_str()) {
            warnings.push(format!("Link {} has unknown dst '{}'", l.id, l.dst));
        }
        if l.src == l.dst {
            warnings.push(format!(
                "Link {} has src == dst ({}) — self-links are invalid",
                l.id, l.src
            ));
        } else if let Some(v) = link_violation(model, &l.src, &l.dst) {
            warnings.push(describe_violation(model, &l.src, &l.dst, &v));
        }
    }

    // --- Groups ---
    let mut seen_group_ids: HashSet<&str> = HashSet::new();
    for g in &model.groups {
        if !seen_group_ids.insert(g.id.as_str()) {
            warnings.push(format!("Duplicate group id: {}", g.id));
        }
        if let Some(pgid) = &g.parent_group_id {
            if !group_ids.contains(pgid.as_str()) {
                warnings.push(format!(
                    "Group {} has parent_group_id '{}' that doesn't exist",
                    g.id, pgid
                ));
            }
        }
        if g.member_ids.is_empty() {
            warnings.push(format!(
                "Group {} (\"{}\") has no members — set memberIds to the node ids it groups, or delete it",
                g.id, g.name
            ));
        }
        match &g.parent_node_id {
            None => {
                if g.parent_group_id.is_none() {
                    warnings.push(format!(
                        "Group {} (\"{}\") has no parentNodeId — it renders at the top level instead of inside its parent node's diagram; set parentNodeId to the node whose children it groups",
                        g.id, g.name
                    ));
                }
            }
            Some(pnid) => match model.nodes.iter().find(|n| n.id == *pnid) {
                None => warnings.push(format!(
                    "Group {} has parentNodeId '{}' that doesn't exist",
                    g.id, pnid
                )),
                Some(_) => {
                    for mid in &g.member_ids {
                        if let Some(member) = model.nodes.iter().find(|n| n.id == *mid) {
                            if member.parent_id.as_deref() != Some(pnid.as_str()) {
                                warnings.push(format!(
                                    "Group {} member '{}' is not a child of parentNodeId '{}' — group members must be children of the node the group is anchored to",
                                    g.id, mid, pnid
                                ));
                            }
                        }
                    }
                }
            },
        }
        if let Some(desc) = &g.description {
            let chars = desc.chars().count();
            if chars > DESCRIPTION_MAX_CHARS {
                warnings.push(format!(
                    "Group {} (\"{}\") description exceeds {} characters ({})",
                    g.id, g.name, DESCRIPTION_MAX_CHARS, chars
                ));
            }
        }
        let mut member_kinds: HashSet<&str> = HashSet::new();
        for mid in &g.member_ids {
            match model.nodes.iter().find(|n| n.id == *mid) {
                Some(member) => {
                    member_kinds.insert(kind_name(&member.kind));
                }
                None => warnings.push(format!(
                    "Group {} member '{}' is not a node",
                    g.id, mid
                )),
            }
        }
        if member_kinds.len() > 1 {
            warnings.push(format!(
                "Group {} mixes member kinds ({:?}) — all members must be at the same level",
                g.id, member_kinds
            ));
        }

        let mut resp_ids: HashSet<&str> = HashSet::new();
        for r in &g.responsibilities {
            if !resp_ids.insert(r.id.as_str()) {
                warnings.push(format!(
                    "Duplicate responsibility id '{}' on group {}",
                    r.id, g.id
                ));
            }
            if r.statement.trim().is_empty() {
                warnings.push(format!(
                    "Empty responsibility statement on group {} (id={})",
                    g.id, r.id
                ));
            }
        }
    }

    // --- Responsibility id global uniqueness ---
    // Responsibility ids must be unique across the WHOLE model (every node AND
    // group), not merely within a host: `find_responsibility`, the id minters
    // (`next_*_id_union`, `IdMinter::absorb`), and every source_map / fold lookup
    // key by id alone and assume a single home. The per-host checks above catch a
    // repeat on the same host; this catches the same id living on two hosts,
    // where a lookup silently binds to whichever the model lists first.
    let mut resp_hosts: HashMap<&str, Vec<&str>> = HashMap::new();
    for n in &model.nodes {
        for r in &n.responsibilities {
            resp_hosts.entry(r.id.as_str()).or_default().push(n.id.as_str());
        }
    }
    for g in &model.groups {
        for r in &g.responsibilities {
            resp_hosts.entry(r.id.as_str()).or_default().push(g.id.as_str());
        }
    }
    let mut collisions: Vec<String> = Vec::new();
    for (rid, hosts) in &resp_hosts {
        // Distinct hosts only — a repeat on one host is already reported above.
        let mut distinct: Vec<&str> = hosts.iter().copied().collect::<HashSet<&str>>().into_iter().collect();
        if distinct.len() > 1 {
            distinct.sort_unstable();
            collisions.push(format!(
                "Responsibility id '{}' is used on multiple hosts ({}) — responsibility ids must be globally unique",
                rid,
                distinct.join(", ")
            ));
        }
    }
    // HashMap order is nondeterministic; sort so the warning list is stable.
    collisions.sort();
    warnings.extend(collisions);

    // --- Code-side mapping keys ---
    // source_map is keyed by responsibility id, or by a schema node id (a
    // schema's declaration site); boundaries by node id.
    let resp_ids: HashSet<&str> = model
        .nodes
        .iter()
        .flat_map(|n| n.responsibilities.iter())
        .chain(model.groups.iter().flat_map(|g| g.responsibilities.iter()))
        .map(|r| r.id.as_str())
        .collect();
    // Symbols that declare a data shape map their declaration location by node id.
    let property_node_ids: HashSet<&str> = model
        .nodes
        .iter()
        .filter(|n| !n.properties.is_empty())
        .map(|n| n.id.as_str())
        .collect();
    for id in model.source_map.keys() {
        if !resp_ids.contains(id.as_str()) && !property_node_ids.contains(id.as_str()) {
            warnings.push(format!(
                "Source map references unknown responsibility or property-bearing node '{}'",
                id
            ));
        }
    }
    for id in model.boundaries.keys() {
        if !node_ids.contains(id.as_str()) {
            warnings.push(format!("Boundary references unknown node '{}'", id));
        }
    }

    warnings.extend(check_disconnected(model));

    warnings
}

/// Per-level connectivity (the C4 "same level of abstraction" rule). Each C4
/// diagram is one level: the system context shows persons + systems; a system's
/// container view shows its containers plus reference nodes linked into it; a
/// container's component view shows its components plus references; likewise for
/// the code level. A relationship is only meaningful where both endpoints are
/// visible at that level. This flags an owned node that connects to nothing
/// visible at its own level — e.g. an actor that links only to containers,
/// never to the system, so the system context diagram has no relationship for
/// it. A relationship modeled only at a coarser level (parent→X with no child
/// linking to X) is NOT flagged: X simply isn't surfaced on the inner view,
/// which is legal — relationships may be modeled only as deep as they're useful.
fn check_disconnected(model: &ScryModel) -> Vec<String> {
    let mut warnings: Vec<String> = Vec::new();
    let by_id: HashMap<&str, &crate::Node> =
        model.nodes.iter().map(|n| (n.id.as_str(), n)).collect();

    // Does any link connect two nodes both present in `visible`? Mark endpoints.
    let connected_within = |visible: &HashSet<&str>| -> HashSet<&str> {
        let mut connected: HashSet<&str> = HashSet::new();
        for l in &model.links {
            if visible.contains(l.src.as_str()) && visible.contains(l.dst.as_str()) {
                connected.insert(l.src.as_str());
                connected.insert(l.dst.as_str());
            }
        }
        connected
    };
    let has_any_link = |id: &str| model.links.iter().any(|l| l.src == id || l.dst == id);

    let check_level = |owned: &HashSet<&str>,
                       refs: &HashSet<&str>,
                       view: &str,
                       warnings: &mut Vec<String>| {
        let visible: HashSet<&str> = owned.union(refs).copied().collect();
        let connected = connected_within(&visible);
        for oid in owned {
            if connected.contains(oid) {
                continue;
            }
            let n = by_id[oid];
            if has_any_link(oid) {
                warnings.push(format!(
                    "'{}' ({}) has links but none at this level — it will appear disconnected in the {}",
                    n.name, kind_name(&n.kind), view
                ));
            } else if owned.len() > 1 {
                warnings.push(format!(
                    "'{}' ({}) has no links — it will appear disconnected",
                    n.name, kind_name(&n.kind)
                ));
            }
        }
    };

    // System context: persons + systems.
    let system_level: HashSet<&str> = model
        .nodes
        .iter()
        .filter(|n| matches!(n.kind, Kind::Person | Kind::System))
        .map(|n| n.id.as_str())
        .collect();
    let empty: HashSet<&str> = HashSet::new();
    check_level(&system_level, &empty, "system context", &mut warnings);

    // For each non-external parent, the view of its direct children (one C4
    // level down): owned = children; refs = the references actually surfaced on
    // that view — outside nodes linked to a direct child (not merely to the
    // parent), matching the canvas projection. A relationship modeled only at
    // the parent level (parent→X with no child link) is not surfaced here and
    // is not flagged: it is a legal coarser-grained relationship.
    for parent in &model.nodes {
        if parent.external == Some(true) {
            continue;
        }
        let child_kind = match parent.kind {
            Kind::System => Kind::Container,
            Kind::Container => Kind::Component,
            Kind::Component => Kind::Symbol, // code level
            _ => continue,
        };
        let owned: HashSet<&str> = model
            .nodes
            .iter()
            .filter(|n| {
                n.parent_id.as_deref() == Some(parent.id.as_str())
                    && n.kind == child_kind
            })
            .map(|n| n.id.as_str())
            .collect();
        if owned.is_empty() {
            continue;
        }
        // Reference nodes: outside the owned set, linked to a direct child
        // (this is what the canvas surfaces on the view), excluding the parent
        // itself and the parent's parent.
        let mut refs: HashSet<&str> = HashSet::new();
        for l in &model.links {
            let touches_child =
                owned.contains(l.src.as_str()) || owned.contains(l.dst.as_str());
            if !touches_child {
                continue;
            }
            for end in [l.src.as_str(), l.dst.as_str()] {
                if end == parent.id || owned.contains(end) {
                    continue;
                }
                if Some(end) == parent.parent_id.as_deref() {
                    continue;
                }
                if by_id.contains_key(end) {
                    refs.insert(end);
                }
            }
        }
        let view = format!("{} view of '{}'", kind_name(&child_kind), parent.name);
        check_level(&owned, &refs, &view, &mut warnings);
    }

    warnings
}

// --- Link legality: the same-level / reference-propagation rule -------------
//
// Relationships connect nodes that share a diagram. Two nodes share a diagram
// when they have the same parent (true siblings). A deeper node may also link
// to a node from outside its surface ONLY when that node is a *reference* on
// the surface — and a reference exists only because the parent (one level up)
// links to it. So a cross-level link is legal iff the parent of the deeper
// endpoint also links to the shallower endpoint, recursively up to a level
// where both are siblings. This is the single source of truth for the rule;
// `add_links` calls it to reject illegal links, and `validate` calls it to
// flag pre-existing ones.

/// Why a link is illegal. Carries node ids; resolve names via `describe_violation`.
pub enum LinkViolation {
    /// One endpoint is an ancestor of the other — containment, not a relationship.
    Containment { ancestor: String, descendant: String },
    /// Same depth, different parents — the two never share a diagram.
    SameLevelDifferentParent,
    /// A deeper→shallower link with no authorizing link from the deeper node's
    /// parent to the shallower endpoint, so the shallower node isn't a reference
    /// on the deeper node's surface.
    UnauthorizedCrossLevel {
        deeper: String,
        other: String,
        parent: String,
    },
}

fn parent_of<'a>(model: &'a ScryModel, id: &str) -> Option<&'a str> {
    model
        .nodes
        .iter()
        .find(|n| n.id == id)
        .and_then(|n| n.parent_id.as_deref())
}

fn depth(model: &ScryModel, id: &str) -> usize {
    let mut d = 0usize;
    let mut cur = id.to_string();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    while let Some(p) = parent_of(model, &cur) {
        if !seen.insert(cur.clone()) {
            break; // cycle guard — a malformed parent chain never loops forever
        }
        d += 1;
        cur = p.to_string();
    }
    d
}

fn is_ancestor(model: &ScryModel, anc: &str, desc: &str) -> bool {
    let mut cur = parent_of(model, desc).map(str::to_string);
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    while let Some(p) = cur {
        if p == anc {
            return true;
        }
        if !seen.insert(p.clone()) {
            break; // cycle guard — a malformed parent chain never loops forever
        }
        cur = parent_of(model, &p).map(str::to_string);
    }
    false
}

fn linked_either(model: &ScryModel, x: &str, y: &str) -> bool {
    model
        .links
        .iter()
        .any(|l| (l.src == x && l.dst == y) || (l.src == y && l.dst == x))
}

fn name_of<'a>(model: &'a ScryModel, id: &'a str) -> &'a str {
    model
        .nodes
        .iter()
        .find(|n| n.id == id)
        .map(|n| n.name.as_str())
        .unwrap_or(id)
}

/// Check a (prospective or existing) link against the same-level rule. Returns
/// `None` when the link is legal. Self-links are caller-handled (returns `None`).
pub fn link_violation(model: &ScryModel, src: &str, dst: &str) -> Option<LinkViolation> {
    if src == dst {
        return None;
    }
    if is_ancestor(model, src, dst) {
        return Some(LinkViolation::Containment {
            ancestor: src.to_string(),
            descendant: dst.to_string(),
        });
    }
    if is_ancestor(model, dst, src) {
        return Some(LinkViolation::Containment {
            ancestor: dst.to_string(),
            descendant: src.to_string(),
        });
    }
    // Same parent (including both top-level) → true siblings, always legal.
    if parent_of(model, src) == parent_of(model, dst) {
        return None;
    }
    let (dsrc, ddst) = (depth(model, src), depth(model, dst));
    if dsrc == ddst {
        return Some(LinkViolation::SameLevelDifferentParent);
    }
    let (deeper, other) = if dsrc > ddst { (src, dst) } else { (dst, src) };
    // `deeper` is strictly deeper than `other`, so it has a parent.
    let parent = match parent_of(model, deeper) {
        Some(p) => p.to_string(),
        None => return None,
    };
    if !linked_either(model, &parent, other) {
        return Some(LinkViolation::UnauthorizedCrossLevel {
            deeper: deeper.to_string(),
            other: other.to_string(),
            parent,
        });
    }
    // The authorizing higher-level link must itself be legal.
    link_violation(model, &parent, other)
}

/// Node ids that `id` may legally link to on its own surface: its siblings plus
/// the references inherited from its parent's links. Used to suggest valid
/// targets in a rejection message.
pub fn link_targets_for(model: &ScryModel, id: &str) -> Vec<String> {
    let parent = parent_of(model, id);
    let mut out: Vec<String> = Vec::new();
    for n in &model.nodes {
        if n.id != id && n.parent_id.as_deref() == parent {
            out.push(n.id.clone());
        }
    }
    if let Some(pp) = parent {
        for l in &model.links {
            let other = if l.src == pp {
                Some(&l.dst)
            } else if l.dst == pp {
                Some(&l.src)
            } else {
                None
            };
            if let Some(o) = other {
                if o.as_str() != id
                    && !is_ancestor(model, o, id)
                    && !is_ancestor(model, id, o)
                    && !out.contains(o)
                {
                    out.push(o.clone());
                }
            }
        }
    }
    out
}

/// Human-readable, corrective explanation of a `LinkViolation`. Shared by
/// `add_links` (rejection) and `validate` (warning).
pub fn describe_violation(
    model: &ScryModel,
    src: &str,
    dst: &str,
    v: &LinkViolation,
) -> String {
    match v {
        LinkViolation::Containment {
            ancestor,
            descendant,
        } => format!(
            "Link {src}→{dst} rejected: '{}' contains '{}' (it is an ancestor in the tree). \
             Containment is expressed by nesting, not by a link — drop this relationship.",
            name_of(model, ancestor),
            name_of(model, descendant)
        ),
        LinkViolation::SameLevelDifferentParent => format!(
            "Link {src}→{dst} rejected: '{}' and '{}' sit at the same level under different \
             parents, so they never share a diagram. Model the relationship between their \
             parents instead — it surfaces as a reference when you drill in.",
            name_of(model, src),
            name_of(model, dst)
        ),
        LinkViolation::UnauthorizedCrossLevel {
            deeper,
            other,
            parent,
        } => {
            let targets = link_targets_for(model, deeper);
            let avail = if targets.is_empty() {
                "(none yet)".to_string()
            } else {
                targets
                    .iter()
                    .map(|t| format!("'{}'", name_of(model, t)))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            format!(
                "Link {src}→{dst} rejected: relationships connect nodes at the same level, and \
                 '{other_name}' is not visible on the surface where '{deeper_name}' lives. First \
                 add a link between '{parent_name}' and '{other_name}' (one level up) — that makes \
                 '{other_name}' a reference on '{deeper_name}'s surface — then '{deeper_name}' may \
                 link to it. Otherwise link '{deeper_name}' to one of the nodes already on its \
                 surface: {avail}.",
                other_name = name_of(model, other),
                deeper_name = name_of(model, deeper),
                parent_name = name_of(model, parent),
            )
        }
    }
}

fn kind_name(k: &Kind) -> &'static str {
    match k {
        Kind::Person => "person",
        Kind::System => "system",
        Kind::Container => "container",
        Kind::Component => "component",
        Kind::Symbol => "symbol",
    }
}

/// Cross-reference the model's source map against the project's filesystem.
/// Catches manifest directories with no source coverage and shared source
/// directories mapped across container boundaries.
pub fn validate_coverage(model: &ScryModel, project_path: &Path) -> Vec<String> {
    let mut warnings: Vec<String> = Vec::new();

    let manifest_dirs = crate::scan::manifest_dirs(project_path);

    // Collect every source pattern from the source map (line-precise) and the
    // boundary globs.
    let mut all_patterns: Vec<&str> = Vec::new();
    for locs in model.source_map.values() {
        for loc in locs {
            all_patterns.push(&loc.pattern);
        }
    }
    for sources in model.boundaries.values() {
        for s in sources {
            all_patterns.push(&s.pattern);
        }
    }

    // Check A: uncovered manifest directories
    for (dir, filename) in &manifest_dirs {
        let prefix = format!("{}/", dir);
        let covered = all_patterns.iter().any(|p| p.starts_with(&prefix) || p == dir);
        if !covered {
            warnings.push(format!(
                "Manifest directory '{}/' (contains {}) is not covered by any source map entry \
                 — this may be a missing compilation unit",
                dir, filename
            ));
        }
    }

    // Check B: cross-container source overlap
    // Build node → container ancestor lookup
    let node_map: HashMap<&str, &crate::Node> =
        model.nodes.iter().map(|n| (n.id.as_str(), n)).collect();

    let mut container_of: HashMap<&str, &str> = HashMap::new();
    for n in &model.nodes {
        let mut cur = n;
        loop {
            if cur.kind == Kind::Container {
                container_of.insert(n.id.as_str(), cur.id.as_str());
                break;
            }
            match &cur.parent_id {
                Some(pid) => match node_map.get(pid.as_str()) {
                    Some(parent) => cur = parent,
                    None => break,
                },
                None => break,
            }
        }
    }

    // Map source directory prefixes to the set of containers they appear under
    let mut dir_to_containers: HashMap<String, HashSet<&str>> = HashMap::new();

    let mut record_pattern = |pattern: &str, node_id: &str| {
        let Some(&container_id) = container_of.get(node_id) else { return };
        // Extract meaningful directory prefix (up to first glob or the parent dir)
        let effective = pattern
            .find(|c: char| c == '*' || c == '?' || c == '{')
            .map(|i| &pattern[..i])
            .unwrap_or(pattern);
        let dir_prefix = if effective.ends_with('/') {
            effective.trim_end_matches('/').to_string()
        } else {
            Path::new(effective)
                .parent()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default()
        };
        if !dir_prefix.is_empty() {
            dir_to_containers
                .entry(dir_prefix)
                .or_default()
                .insert(container_id);
        }
    };

    // source_map is keyed by responsibility id (resolve to its owning node) or
    // by a schema node id directly — either way attribute the pattern to a node.
    let resp_to_node: HashMap<&str, &str> = model
        .nodes
        .iter()
        .flat_map(|n| n.responsibilities.iter().map(move |r| (r.id.as_str(), n.id.as_str())))
        .collect();
    let node_ids_set: HashSet<&str> = model.nodes.iter().map(|n| n.id.as_str()).collect();
    for (key, locs) in &model.source_map {
        let owner = resp_to_node
            .get(key.as_str())
            .copied()
            .or_else(|| node_ids_set.get(key.as_str()).copied());
        if let Some(node_id) = owner {
            for loc in locs {
                record_pattern(&loc.pattern, node_id);
            }
        }
    }
    for (node_id, sources) in &model.boundaries {
        for s in sources {
            record_pattern(&s.pattern, node_id);
        }
    }

    for (dir, containers) in &dir_to_containers {
        if containers.len() > 1 {
            let names: Vec<&str> = {
                let mut v: Vec<&str> = containers
                    .iter()
                    .filter_map(|&id| node_map.get(id).map(|n| n.name.as_str()))
                    .collect();
                v.sort();
                v
            };
            warnings.push(format!(
                "Source directory '{}' is mapped to nodes in multiple containers ({}) \
                 — verify shared library placement",
                dir,
                names.join(", ")
            ));
        }
    }

    warnings
}

fn is_valid_identifier(s: &str, allow_upper_start: bool) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        None => return false,
        Some(c) => {
            if !c.is_ascii_alphabetic() && c != '_' {
                return false;
            }
            if !allow_upper_start && c.is_ascii_uppercase() {
                return false;
            }
        }
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[cfg(test)]
mod resp_id_tests {
    use super::validate;
    use crate::{Group, Node, ScryModel};

    fn node(v: serde_json::Value) -> Node {
        serde_json::from_value(v).unwrap()
    }
    fn group(v: serde_json::Value) -> Group {
        serde_json::from_value(v).unwrap()
    }

    /// A responsibility id living on two different hosts (here a node AND a group)
    /// breaks the global-uniqueness invariant the id minters and every id-keyed
    /// lookup rely on — the validator must flag it, naming every host.
    #[test]
    fn flags_a_responsibility_id_reused_across_hosts() {
        let mut m = ScryModel::new();
        m.nodes.push(node(serde_json::json!({
            "id": "node-1", "kind": "system", "name": "Acme",
            "responsibilities": [{ "id": "resp-1", "statement": "runs the show" }]
        })));
        m.nodes.push(node(serde_json::json!({
            "id": "node-2", "kind": "container", "name": "API", "parentId": "node-1",
            "responsibilities": [{ "id": "resp-1", "statement": "serves requests" }]
        })));
        m.groups.push(group(serde_json::json!({
            "id": "grp-1", "name": "Deploys as one", "parentNodeId": "node-1",
            "memberIds": ["node-2"],
            "responsibilities": [{ "id": "resp-1", "statement": "ships together" }]
        })));

        let warnings = validate(&m);
        let hits: Vec<&String> =
            warnings.iter().filter(|w| w.contains("globally unique")).collect();
        assert_eq!(hits.len(), 1, "exactly one collision warning, got: {warnings:?}");
        let w = hits[0];
        assert!(w.contains("resp-1"), "names the colliding id: {w}");
        assert!(
            w.contains("node-1") && w.contains("node-2") && w.contains("grp-1"),
            "names every host: {w}"
        );
    }

    /// Responsibility ids that are unique across hosts raise no collision warning.
    #[test]
    fn unique_responsibility_ids_pass() {
        let mut m = ScryModel::new();
        m.nodes.push(node(serde_json::json!({
            "id": "node-1", "kind": "system", "name": "Acme",
            "responsibilities": [{ "id": "resp-1", "statement": "runs the show" }]
        })));
        m.nodes.push(node(serde_json::json!({
            "id": "node-2", "kind": "container", "name": "API", "parentId": "node-1",
            "responsibilities": [{ "id": "resp-2", "statement": "serves requests" }]
        })));
        assert!(
            !validate(&m).iter().any(|w| w.contains("globally unique")),
            "distinct ids must not warn"
        );
    }
}
