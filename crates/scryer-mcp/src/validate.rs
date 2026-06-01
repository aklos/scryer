use scryer_core::{Kind, ScryModel};
use std::collections::{HashMap, HashSet};
use std::path::Path;

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
            if desc.len() > 200 {
                warnings.push(format!(
                    "Node {} (\"{}\") description exceeds 200 characters ({})",
                    n.id, n.name, desc.len()
                ));
            }
        }

        if n.kind == Kind::Symbol && !is_valid_identifier(&n.name, false) {
            warnings.push(format!(
                "Symbol node {} (\"{}\") name should be a valid identifier",
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
            if desc.len() > 200 {
                warnings.push(format!(
                    "Group {} (\"{}\") description exceeds 200 characters ({})",
                    g.id, g.name, desc.len()
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
/// visible at that level. This flags:
///   - an owned node that connects to nothing visible at its own level
///     (e.g. an actor that links only to containers, never to the system, so
///     the system context diagram has no relationship for it);
///   - a reference node linked to a parent but to none of its children, so it
///     appears disconnected when you drill in.
fn check_disconnected(model: &ScryModel) -> Vec<String> {
    let mut warnings: Vec<String> = Vec::new();
    let by_id: HashMap<&str, &scryer_core::Node> =
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
                       parent_name: Option<&str>,
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
        for rid in refs {
            if connected.contains(rid) {
                continue;
            }
            let n = by_id[rid];
            if let Some(pname) = parent_name {
                warnings.push(format!(
                    "'{}' ({}) links to '{}' but not to any of its children — it will appear disconnected in the {}. Add a link from the relevant child to '{}'",
                    n.name, kind_name(&n.kind), pname, view, n.name
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
    check_level(&system_level, &empty, "system context", None, &mut warnings);

    // For each non-external parent, the view of its direct children (one C4
    // level down): owned = children; refs = any outside node linked to the
    // parent or to a child, minus the parent's own ancestors and same-level
    // siblings of the children.
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
        // Reference nodes: outside the owned set, linked to the parent or a
        // child, excluding the parent itself and the parent's parent.
        let mut refs: HashSet<&str> = HashSet::new();
        for l in &model.links {
            let touches_parent = l.src == parent.id || l.dst == parent.id;
            let touches_child =
                owned.contains(l.src.as_str()) || owned.contains(l.dst.as_str());
            if !(touches_parent || touches_child) {
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
        check_level(&owned, &refs, &view, Some(&parent.name), &mut warnings);
    }

    warnings
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

    let manifest_dirs = scryer_core::scan::manifest_dirs(project_path);

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
    let node_map: HashMap<&str, &scryer_core::Node> =
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
