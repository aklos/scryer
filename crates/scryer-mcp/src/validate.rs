use crate::helpers::kind_str;
use scryer_core::{C4Kind, C4ModelData, C4Node, ModelProperty};
use std::collections::{HashMap, HashSet};

/// Check that a name is a valid identifier: starts with lowercase letter, then [a-zA-Z0-9_]
fn is_valid_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Check that a name is a valid type name: starts with any letter, then [a-zA-Z0-9_]
fn is_valid_type_name(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

pub(crate) fn validate_identifier(name: &str, node_label: &str) -> Result<(), String> {
    if !is_valid_identifier(name) {
        Err(format!(
            "Name '{}' for {} must be a valid identifier (camelCase or snake_case: start with lowercase letter, then [a-zA-Z0-9_])",
            name, node_label
        ))
    } else {
        Ok(())
    }
}

pub(crate) fn validate_type_name(name: &str, node_label: &str) -> Result<(), String> {
    if !is_valid_type_name(name) {
        Err(format!(
            "Name '{}' for {} must be a valid type name (start with a letter, then [a-zA-Z0-9_])",
            name, node_label
        ))
    } else {
        Ok(())
    }
}

pub(crate) fn validate_property_labels(properties: &[ModelProperty], node_label: &str) -> Result<(), String> {
    for prop in properties {
        if !is_valid_identifier(&prop.label) {
            return Err(format!(
                "Property label '{}' on {} must be a valid identifier (camelCase or snake_case: start with lowercase letter, then [a-zA-Z0-9_])",
                prop.label, node_label
            ));
        }
    }
    Ok(())
}

/// Check that no node is parented under an external system.
pub(crate) fn validate_no_children_of_external(nodes: &[C4Node]) -> Result<(), String> {
    let external_ids: HashSet<&str> = nodes
        .iter()
        .filter(|n| n.data.kind == C4Kind::System && n.data.external.unwrap_or(false))
        .map(|n| n.id.as_str())
        .collect();
    for node in nodes {
        if let Some(pid) = &node.parent_id {
            if external_ids.contains(pid.as_str()) {
                return Err(format!(
                    "Cannot add '{}' inside external system '{}'. External systems are opaque and must not have child nodes.",
                    node.data.name,
                    nodes.iter().find(|n| n.id == *pid).map(|n| n.data.name.as_str()).unwrap_or(pid)
                ));
            }
        }
    }
    Ok(())
}

/// Find nodes that will appear disconnected (no edges) at their viewing level.
pub(crate) fn check_disconnected_nodes(model: &C4ModelData) -> Vec<String> {
    let mut warnings: Vec<String> = Vec::new();

    let check_level = |
        owned_ids: &HashSet<&str>,
        ref_ids: &HashSet<&str>,
        view_name: &str,
        parent_name: Option<&str>,
        warnings: &mut Vec<String>,
    | {
        let visible: HashSet<&str> = owned_ids.union(ref_ids).copied().collect();

        let mut connected: HashSet<&str> = HashSet::new();
        for edge in &model.edges {
            let src = edge.source.as_str();
            let tgt = edge.target.as_str();
            if visible.contains(src) && visible.contains(tgt) {
                connected.insert(src);
                connected.insert(tgt);
            }
        }

        for oid in owned_ids {
            if connected.contains(oid) {
                continue;
            }
            let node = model.nodes.iter().find(|n| n.id == *oid).unwrap();
            let has_any_edge = model.edges.iter().any(|e| e.source == *oid || e.target == *oid);
            if has_any_edge {
                warnings.push(format!(
                    "'{}' ({}) has edges but none at this level — \
                    it will appear disconnected in the {}",
                    node.data.name, kind_str(&node.data.kind), view_name
                ));
            } else if owned_ids.len() > 1 {
                warnings.push(format!(
                    "'{}' ({}) has no edges — it will appear disconnected",
                    node.data.name, kind_str(&node.data.kind)
                ));
            }
        }

        for rid in ref_ids {
            if connected.contains(rid) {
                continue;
            }
            let node = model.nodes.iter().find(|n| n.id == *rid).unwrap();
            if let Some(pname) = parent_name {
                warnings.push(format!(
                    "'{}' ({}) has edges to '{}' but not to any of its children — \
                    it will appear disconnected in the {}. \
                    Add edges from the relevant children to '{}'",
                    node.data.name, kind_str(&node.data.kind),
                    pname, view_name, node.data.name
                ));
            }
        }
    };

    // === System level ===
    let system_level_ids: HashSet<&str> = model
        .nodes
        .iter()
        .filter(|n| matches!(n.data.kind, C4Kind::Person | C4Kind::System))
        .map(|n| n.id.as_str())
        .collect();

    let empty: HashSet<&str> = HashSet::new();
    check_level(&system_level_ids, &empty, "system view", None, &mut warnings);

    // === Container level ===
    let systems: Vec<&C4Node> = model
        .nodes
        .iter()
        .filter(|n| n.data.kind == C4Kind::System && !n.data.external.unwrap_or(false))
        .collect();

    for system in &systems {
        let container_ids: HashSet<&str> = model
            .nodes
            .iter()
            .filter(|n| n.data.kind == C4Kind::Container && n.parent_id.as_deref() == Some(&system.id))
            .map(|n| n.id.as_str())
            .collect();

        if container_ids.is_empty() {
            continue;
        }

        let ref_ids: HashSet<&str> = system_level_ids
            .iter()
            .filter(|id| {
                let node = model.nodes.iter().find(|n| n.id == **id).unwrap();
                (node.data.kind == C4Kind::Person
                    || (node.data.kind == C4Kind::System && node.id != system.id))
                    && model.edges.iter().any(|e| {
                        let touches_system = e.source == system.id || e.target == system.id;
                        let touches_container = container_ids.contains(e.source.as_str())
                            || container_ids.contains(e.target.as_str());
                        let touches_ref = e.source == **id || e.target == **id;
                        touches_ref && (touches_system || touches_container)
                    })
            })
            .copied()
            .collect();

        let view_name = format!("container view of '{}'", system.data.name);
        check_level(&container_ids, &ref_ids, &view_name, Some(&system.data.name), &mut warnings);
    }

    // === Component level ===
    let containers: Vec<&C4Node> = model
        .nodes
        .iter()
        .filter(|n| n.data.kind == C4Kind::Container)
        .collect();

    for container in &containers {
        let component_ids: HashSet<&str> = model
            .nodes
            .iter()
            .filter(|n| n.data.kind == C4Kind::Component && n.parent_id.as_deref() == Some(&container.id))
            .map(|n| n.id.as_str())
            .collect();

        if component_ids.is_empty() {
            continue;
        }

        let ref_ids: HashSet<&str> = model
            .edges
            .iter()
            .filter_map(|e| {
                if e.source == container.id && !component_ids.contains(e.target.as_str()) {
                    Some(e.target.as_str())
                } else if e.target == container.id && !component_ids.contains(e.source.as_str()) {
                    Some(e.source.as_str())
                } else {
                    None
                }
            })
            .filter(|id| {
                let node = model.nodes.iter().find(|n| n.id == *id);
                match node {
                    Some(n) => {
                        if Some(*id) == container.parent_id.as_deref() { return false; }
                        if n.data.kind == C4Kind::Component { return false; }
                        true
                    }
                    None => false,
                }
            })
            .collect();

        let view_name = format!("component view of '{}'", container.data.name);
        check_level(&component_ids, &ref_ids, &view_name, Some(&container.data.name), &mut warnings);
    }

    warnings
}

/// Find bidirectional edge pairs (A→B and B→A) that likely violate C4 rule 1.
pub(crate) fn check_bidirectional_edges(model: &C4ModelData) -> Vec<String> {
    let mut warnings = Vec::new();
    let mut seen = HashSet::new();
    for edge in &model.edges {
        let pair = if edge.source < edge.target {
            (edge.source.clone(), edge.target.clone())
        } else {
            (edge.target.clone(), edge.source.clone())
        };
        if !seen.insert(pair.clone()) {
            let labels: Vec<&str> = model.edges.iter()
                .filter(|e| {
                    (e.source == pair.0 && e.target == pair.1)
                    || (e.source == pair.1 && e.target == pair.0)
                })
                .filter_map(|e| e.data.as_ref().map(|d| d.label.as_str()))
                .collect();
            let src_name = model.nodes.iter().find(|n| n.id == pair.0).map(|n| n.data.name.as_str()).unwrap_or(&pair.0);
            let tgt_name = model.nodes.iter().find(|n| n.id == pair.1).map(|n| n.data.name.as_str()).unwrap_or(&pair.1);
            warnings.push(format!(
                "'{}' ↔ '{}' has edges in both directions ({}). \
                C4 rule 1: one edge per relationship. Are these genuinely independent relationships, \
                or should they be a single edge?",
                src_name, tgt_name, labels.join(", ")
            ));
        }
    }
    warnings
}

/// Check that @[Name] mentions in descriptions have corresponding edges.
pub(crate) fn check_mention_edges(model: &C4ModelData) -> Vec<String> {
    let mut warnings = Vec::new();

    let name_to_id: HashMap<&str, &str> = model
        .nodes
        .iter()
        .map(|n| (n.data.name.as_str(), n.id.as_str()))
        .collect();

    let parent_of: HashMap<&str, &str> = model
        .nodes
        .iter()
        .filter_map(|n| n.parent_id.as_deref().map(|p| (n.id.as_str(), p)))
        .collect();

    let mut connected: HashSet<(String, String)> = HashSet::new();
    for edge in &model.edges {
        let mut src_chain = vec![edge.source.clone()];
        {
            let mut cur = edge.source.as_str();
            while let Some(&p) = parent_of.get(cur) {
                src_chain.push(p.to_string());
                cur = p;
            }
        }
        let mut tgt_chain = vec![edge.target.clone()];
        {
            let mut cur = edge.target.as_str();
            while let Some(&p) = parent_of.get(cur) {
                tgt_chain.push(p.to_string());
                cur = p;
            }
        }
        for s in &src_chain {
            for t in &tgt_chain {
                connected.insert((s.clone(), t.clone()));
                connected.insert((t.clone(), s.clone()));
            }
        }
    }

    for node in &model.nodes {
        if node.data.description.is_empty() {
            continue;
        }
        let desc = &node.data.description;
        let mut search_from = 0;
        while let Some(start) = desc[search_from..].find("@[") {
            let abs_start = search_from + start + 2;
            let Some(end) = desc[abs_start..].find(']') else { break };
            let mentioned_name = &desc[abs_start..abs_start + end];
            search_from = abs_start + end + 1;
            if let Some(&mentioned_id) = name_to_id.get(mentioned_name) {
                if mentioned_id == node.id {
                    continue;
                }
                if parent_of.get(mentioned_id) == Some(&node.id.as_str()) {
                    continue;
                }
                if node.parent_id.as_deref() == Some(mentioned_id) {
                    continue;
                }
                let mentioned_parent = parent_of.get(mentioned_id).copied();
                if node.parent_id.as_deref().is_some() && node.parent_id.as_deref() == mentioned_parent {
                    continue;
                }
                if !connected.contains(&(node.id.clone(), mentioned_id.to_string())) {
                    warnings.push(format!(
                        "'{}' mentions @[{}] in its description but has no edge connecting them. \
                        Add an edge or remove the mention.",
                        node.data.name, mentioned_name
                    ));
                }
            }
        }
    }

    warnings
}

/// Check for edges between components in different containers.
pub(crate) fn check_cross_container_edges(model: &C4ModelData) -> Vec<String> {
    let mut warnings = Vec::new();

    let node_map: HashMap<&str, &C4Node> = model.nodes.iter().map(|n| (n.id.as_str(), n)).collect();

    for edge in &model.edges {
        let src = node_map.get(edge.source.as_str());
        let tgt = node_map.get(edge.target.as_str());
        let (Some(src_node), Some(tgt_node)) = (src, tgt) else { continue };

        if !matches!(src_node.data.kind, C4Kind::Component) || !matches!(tgt_node.data.kind, C4Kind::Component) {
            continue;
        }

        let src_parent = src_node.parent_id.as_deref();
        let tgt_parent = tgt_node.parent_id.as_deref();
        if src_parent != tgt_parent {
            let src_container = src_parent.and_then(|p| node_map.get(p)).map(|n| n.data.name.as_str()).unwrap_or("?");
            let tgt_container = tgt_parent.and_then(|p| node_map.get(p)).map(|n| n.data.name.as_str()).unwrap_or("?");
            warnings.push(format!(
                "'{}' (in {}) → '{}' (in {}): components in different containers cannot have direct edges. \
                Edge should target the container '{}' instead of its internal component.",
                src_node.data.name, src_container, tgt_node.data.name, tgt_container, tgt_container
            ));
        }
    }

    warnings
}

/// Check if a node can be set to "verified" by verifying all inherited expect contract items are passed.
pub(crate) fn check_verified_gate(
    nodes: &[C4Node],
    groups: &[scryer_core::Group],
    node_id: &str,
    parent_id: &Option<String>,
    own_contract: &scryer_core::Contract,
) -> Vec<String> {
    let mut unmet = Vec::new();
    for ci in &own_contract.expect {
        if ci.passed() != Some(true) {
            unmet.push(format!("  - {}", ci.text()));
        }
    }
    let mut cur_id = parent_id.clone();
    while let Some(pid) = &cur_id {
        if let Some(anc) = nodes.iter().find(|n| n.id == *pid) {
            for ci in &anc.data.contract.expect {
                if ci.passed() != Some(true) {
                    unmet.push(format!("  - {} (from {})", ci.text(), anc.data.name));
                }
            }
            cur_id = anc.parent_id.clone();
        } else {
            break;
        }
    }
    let mut check_id = Some(node_id.to_string());
    while let Some(cid) = &check_id {
        for g in groups {
            if g.member_ids.contains(cid) {
                for ci in &g.contract.expect {
                    if ci.passed() != Some(true) {
                        unmet.push(format!("  - {} (from group {})", ci.text(), g.name));
                    }
                }
            }
        }
        check_id = nodes.iter().find(|n| n.id == *cid).and_then(|n| n.parent_id.clone());
    }
    unmet
}

pub(crate) fn validate_parent(
    model: &C4ModelData,
    kind: &C4Kind,
    parent_id: Option<&str>,
) -> Result<(), String> {
    match kind {
        C4Kind::Person | C4Kind::System => {
            if parent_id.is_some() {
                return Err("Person and system nodes must be top-level (no parent_id)".into());
            }
        }
        C4Kind::Container => {
            let pid =
                parent_id.ok_or("Container nodes require a parent_id (must be inside a system)")?;
            let parent = model
                .nodes
                .iter()
                .find(|n| n.id == pid)
                .ok_or(format!("Parent node '{}' not found", pid))?;
            if parent.data.kind != C4Kind::System {
                return Err(format!(
                    "Container parent must be a system, got {:?}",
                    parent.data.kind
                ));
            }
            if parent.data.external.unwrap_or(false) {
                return Err(format!(
                    "Cannot add containers inside external system '{}'. External systems are opaque and must not have child nodes. Instead, model each external service as its own top-level external system (e.g. separate 'S3' and 'Rekognition' systems instead of containers inside 'AWS').",
                    parent.data.name
                ));
            }
        }
        C4Kind::Component => {
            let pid = parent_id
                .ok_or("Component nodes require a parent_id (must be inside a container)")?;
            let parent = model
                .nodes
                .iter()
                .find(|n| n.id == pid)
                .ok_or(format!("Parent node '{}' not found", pid))?;
            if parent.data.kind != C4Kind::Container {
                return Err(format!(
                    "Component parent must be a container, got {:?}",
                    parent.data.kind
                ));
            }
        }
        C4Kind::Operation | C4Kind::Process | C4Kind::Model => {
            let label = kind_str(kind);
            let pid = parent_id.ok_or(format!(
                "{} nodes require a parent_id (must be inside a component)",
                label
            ))?;
            let parent = model
                .nodes
                .iter()
                .find(|n| n.id == pid)
                .ok_or(format!("Parent node '{}' not found", pid))?;
            if parent.data.kind != C4Kind::Component {
                return Err(format!(
                    "{} parent must be a component, got {:?}",
                    label, parent.data.kind
                ));
            }
        }
    }
    Ok(())
}
