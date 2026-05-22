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
                            | (Kind::Component, Kind::Operation)
                            | (Kind::Component, Kind::Model)
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

        if !n.properties.is_empty() && n.kind != Kind::Model {
            warnings.push(format!(
                "Node {} (\"{}\") has properties but kind is {:?} — properties are only valid on model",
                n.id, n.name, n.kind
            ));
        }

        if !n.responsibilities.is_empty() && n.kind == Kind::Model {
            warnings.push(format!(
                "Node {} (\"{}\") is a model kind but carries responsibilities — models carry properties, not responsibilities",
                n.id, n.name
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

        if n.kind == Kind::Operation && !is_valid_identifier(&n.name, false) {
            warnings.push(format!(
                "Operation node {} (\"{}\") name should be a valid identifier (lowercase start)",
                n.id, n.name
            ));
        }
        if n.kind == Kind::Model && !is_valid_identifier(&n.name, true) {
            warnings.push(format!(
                "Model node {} (\"{}\") name should be a valid type name (letter start, [a-zA-Z0-9_])",
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

    // --- Source map keys ---
    for id in model.source_map.keys() {
        if !node_ids.contains(id.as_str()) {
            warnings.push(format!("Source map references unknown node '{}'", id));
        }
    }

    warnings
}

fn kind_name(k: &Kind) -> &'static str {
    match k {
        Kind::Person => "person",
        Kind::System => "system",
        Kind::Container => "container",
        Kind::Component => "component",
        Kind::Operation => "operation",
        Kind::Model => "model",
    }
}

/// Cross-reference the model's source map against the project's filesystem.
/// Catches manifest directories with no source coverage and shared source
/// directories mapped across container boundaries.
pub fn validate_coverage(model: &ScryModel, project_path: &Path) -> Vec<String> {
    let mut warnings: Vec<String> = Vec::new();

    let manifest_dirs = scryer_core::scan::manifest_dirs(project_path);

    // Collect every source pattern from the source map and from node.sources
    let mut all_patterns: Vec<&str> = Vec::new();
    for locs in model.source_map.values() {
        for loc in locs {
            all_patterns.push(&loc.pattern);
        }
    }
    for n in &model.nodes {
        for s in &n.sources {
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

    for (node_id, locs) in &model.source_map {
        for loc in locs {
            record_pattern(&loc.pattern, node_id);
        }
    }
    for n in &model.nodes {
        for s in &n.sources {
            record_pattern(&s.pattern, &n.id);
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
