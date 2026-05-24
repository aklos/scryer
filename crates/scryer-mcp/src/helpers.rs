use rmcp::ErrorData as McpError;
use scryer_core::{Cell, Group, GroupSize, Kind, Link, ModelRef, Node, Responsibility, ScryModel};
use std::collections::HashMap;

/// Strip empty values from a JSON tree to keep MCP responses compact.
pub(crate) fn strip_fields_compact(val: &mut serde_json::Value) {
    match val {
        serde_json::Value::Object(map) => {
            map.retain(|_, v| !matches!(v, serde_json::Value::String(s) if s.is_empty()));
            map.retain(|_, v| !v.is_null());
            map.retain(|_, v| !matches!(v, serde_json::Value::Array(a) if a.is_empty()));
            map.retain(|_, v| !matches!(v, serde_json::Value::Object(m) if m.is_empty()));
            for (_, v) in map.iter_mut() {
                strip_fields_compact(v);
            }
        }
        serde_json::Value::Array(arr) => {
            for v in arr.iter_mut() {
                strip_fields_compact(v);
            }
        }
        _ => {}
    }
}

pub(crate) fn parse_kind(s: &str) -> Result<Kind, McpError> {
    match s {
        "person" => Ok(Kind::Person),
        "system" => Ok(Kind::System),
        "container" => Ok(Kind::Container),
        "component" => Ok(Kind::Component),
        "symbol" => Ok(Kind::Symbol),
        "schema" => Ok(Kind::Schema),
        _ => Err(McpError::invalid_params(
            format!(
                "Invalid kind '{}'. Must be: person, system, container, component, symbol, schema",
                s
            ),
            None,
        )),
    }
}

pub(crate) fn kind_str(k: &Kind) -> &'static str {
    match k {
        Kind::Person => "person",
        Kind::System => "system",
        Kind::Container => "container",
        Kind::Component => "component",
        Kind::Symbol => "symbol",
        Kind::Schema => "schema",
    }
}

pub(crate) fn opt_str(s: &Option<String>) -> &str {
    s.as_deref().unwrap_or("none")
}

/// Build a denormalized graph view of a node for MCP responses:
/// adds `childIds`, `incomingLinks`, `outgoingLinks` to the node JSON.
pub(crate) fn denormalize_node(node: &Node, model: &ScryModel) -> serde_json::Value {
    let mut val = serde_json::to_value(node).unwrap_or(serde_json::Value::Null);
    if let serde_json::Value::Object(map) = &mut val {
        let child_ids: Vec<&str> = model
            .nodes
            .iter()
            .filter(|n| n.parent_id.as_deref() == Some(&node.id))
            .map(|n| n.id.as_str())
            .collect();
        let incoming: Vec<&str> = model
            .links
            .iter()
            .filter(|l| l.dst == node.id)
            .map(|l| l.id.as_str())
            .collect();
        let outgoing: Vec<&str> = model
            .links
            .iter()
            .filter(|l| l.src == node.id)
            .map(|l| l.id.as_str())
            .collect();
        map.insert("childIds".to_string(), serde_json::json!(child_ids));
        map.insert("incomingLinks".to_string(), serde_json::json!(incoming));
        map.insert("outgoingLinks".to_string(), serde_json::json!(outgoing));
    }
    val
}

/// Compute a human-readable diff between baseline and current.
pub(crate) fn compute_diff(baseline: &ScryModel, current: &ScryModel) -> String {
    let base_nodes: HashMap<&str, &Node> =
        baseline.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    let curr_nodes: HashMap<&str, &Node> =
        current.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    let base_links: HashMap<&str, &Link> =
        baseline.links.iter().map(|l| (l.id.as_str(), l)).collect();
    let curr_links: HashMap<&str, &Link> =
        current.links.iter().map(|l| (l.id.as_str(), l)).collect();
    let base_groups: HashMap<&str, &Group> =
        baseline.groups.iter().map(|g| (g.id.as_str(), g)).collect();
    let curr_groups: HashMap<&str, &Group> =
        current.groups.iter().map(|g| (g.id.as_str(), g)).collect();

    let mut sections: Vec<String> = Vec::new();

    // Nodes added
    let added: Vec<_> = current
        .nodes
        .iter()
        .filter(|n| !base_nodes.contains_key(n.id.as_str()))
        .collect();
    if !added.is_empty() {
        let mut lines = vec![format!("Nodes added ({}):", added.len())];
        for n in &added {
            let mut detail = format!("  - {} \"{}\" ({})", n.id, n.name, kind_str(&n.kind));
            if let Some(pid) = &n.parent_id {
                detail.push_str(&format!(", parent={}", pid));
            }
            if let Some(tech) = &n.technology {
                detail.push_str(&format!(", technology={}", tech));
            }
            if !n.responsibilities.is_empty() {
                detail.push_str(&format!(
                    ", responsibilities={}",
                    n.responsibilities.len()
                ));
            }
            lines.push(detail);
        }
        sections.push(lines.join("\n"));
    }

    // Nodes removed
    let removed: Vec<_> = baseline
        .nodes
        .iter()
        .filter(|n| !curr_nodes.contains_key(n.id.as_str()))
        .collect();
    if !removed.is_empty() {
        let mut lines = vec![format!("Nodes removed ({}):", removed.len())];
        for n in &removed {
            lines.push(format!(
                "  - {} \"{}\" ({})",
                n.id,
                n.name,
                kind_str(&n.kind)
            ));
        }
        sections.push(lines.join("\n"));
    }

    // Nodes modified
    let mut mod_lines: Vec<String> = Vec::new();
    for (id, curr) in &curr_nodes {
        if let Some(base) = base_nodes.get(id) {
            let mut changes: Vec<String> = Vec::new();
            if base.name != curr.name {
                changes.push(format!("name \"{}\" -> \"{}\"", base.name, curr.name));
            }
            if base.description != curr.description {
                changes.push("description changed".to_string());
            }
            if base.kind != curr.kind {
                changes.push(format!(
                    "kind {} -> {}",
                    kind_str(&base.kind),
                    kind_str(&curr.kind)
                ));
            }
            if base.technology != curr.technology {
                changes.push(format!(
                    "technology {} -> {}",
                    opt_str(&base.technology),
                    opt_str(&curr.technology)
                ));
            }
            if base.external != curr.external {
                changes.push(format!(
                    "external {:?} -> {:?}",
                    base.external, curr.external
                ));
            }
            if base.parent_id != curr.parent_id {
                changes.push(format!(
                    "parentId {} -> {}",
                    base.parent_id.as_deref().unwrap_or("none"),
                    curr.parent_id.as_deref().unwrap_or("none")
                ));
            }
            if responsibilities_changed(&base.responsibilities, &curr.responsibilities) {
                changes.push(format!(
                    "responsibilities {} -> {}",
                    base.responsibilities.len(),
                    curr.responsibilities.len()
                ));
            }
            if base.properties != curr.properties {
                changes.push(format!(
                    "properties {} -> {}",
                    base.properties.len(),
                    curr.properties.len()
                ));
            }
            if !changes.is_empty() {
                mod_lines.push(format!(
                    "  - {} (\"{}\"): {}",
                    id,
                    curr.name,
                    changes.join(", ")
                ));
            }
        }
    }
    if !mod_lines.is_empty() {
        sections.push(format!(
            "Nodes modified ({}):\n{}",
            mod_lines.len(),
            mod_lines.join("\n")
        ));
    }

    // Links added
    let links_added: Vec<_> = current
        .links
        .iter()
        .filter(|l| !base_links.contains_key(l.id.as_str()))
        .collect();
    if !links_added.is_empty() {
        let mut lines = vec![format!("Links added ({}):", links_added.len())];
        for l in &links_added {
            lines.push(format!(
                "  - {}: {} -> {} \"{}\"",
                l.id, l.src, l.dst, l.label
            ));
        }
        sections.push(lines.join("\n"));
    }

    // Links removed
    let links_removed: Vec<_> = baseline
        .links
        .iter()
        .filter(|l| !curr_links.contains_key(l.id.as_str()))
        .collect();
    if !links_removed.is_empty() {
        let mut lines = vec![format!("Links removed ({}):", links_removed.len())];
        for l in &links_removed {
            lines.push(format!(
                "  - {}: {} -> {} \"{}\"",
                l.id, l.src, l.dst, l.label
            ));
        }
        sections.push(lines.join("\n"));
    }

    // Links modified
    let mut link_mod_lines: Vec<String> = Vec::new();
    for (id, curr) in &curr_links {
        if let Some(base) = base_links.get(id) {
            let mut changes: Vec<String> = Vec::new();
            if base.label != curr.label {
                changes.push(format!("label \"{}\" -> \"{}\"", base.label, curr.label));
            }
            if base.method != curr.method {
                changes.push(format!(
                    "method {} -> {}",
                    base.method.as_deref().unwrap_or("none"),
                    curr.method.as_deref().unwrap_or("none")
                ));
            }
            if !changes.is_empty() {
                link_mod_lines.push(format!("  - {}: {}", id, changes.join(", ")));
            }
        }
    }
    if !link_mod_lines.is_empty() {
        sections.push(format!(
            "Links modified ({}):\n{}",
            link_mod_lines.len(),
            link_mod_lines.join("\n")
        ));
    }

    // Groups added
    let groups_added: Vec<_> = current
        .groups
        .iter()
        .filter(|g| !base_groups.contains_key(g.id.as_str()))
        .collect();
    if !groups_added.is_empty() {
        let mut lines = vec![format!("Groups added ({}):", groups_added.len())];
        for g in &groups_added {
            lines.push(format!(
                "  - {} \"{}\" ({} members)",
                g.id,
                g.name,
                g.member_ids.len()
            ));
        }
        sections.push(lines.join("\n"));
    }

    // Groups removed
    let groups_removed: Vec<_> = baseline
        .groups
        .iter()
        .filter(|g| !curr_groups.contains_key(g.id.as_str()))
        .collect();
    if !groups_removed.is_empty() {
        let mut lines = vec![format!("Groups removed ({}):", groups_removed.len())];
        for g in &groups_removed {
            lines.push(format!("  - {} \"{}\"", g.id, g.name));
        }
        sections.push(lines.join("\n"));
    }

    // Groups modified
    let mut group_mod_lines: Vec<String> = Vec::new();
    for (id, curr) in &curr_groups {
        if let Some(base) = base_groups.get(id) {
            let mut changes: Vec<String> = Vec::new();
            if base.name != curr.name {
                changes.push(format!("name \"{}\" -> \"{}\"", base.name, curr.name));
            }
            if base.member_ids.len() != curr.member_ids.len() {
                changes.push(format!(
                    "members {} -> {}",
                    base.member_ids.len(),
                    curr.member_ids.len()
                ));
            }
            if responsibilities_changed(&base.responsibilities, &curr.responsibilities) {
                changes.push(format!(
                    "responsibilities {} -> {}",
                    base.responsibilities.len(),
                    curr.responsibilities.len()
                ));
            }
            if !changes.is_empty() {
                group_mod_lines.push(format!(
                    "  - {} (\"{}\"): {}",
                    id,
                    curr.name,
                    changes.join(", ")
                ));
            }
        }
    }
    if !group_mod_lines.is_empty() {
        sections.push(format!(
            "Groups modified ({}):\n{}",
            group_mod_lines.len(),
            group_mod_lines.join("\n")
        ));
    }

    if sections.is_empty() {
        "No changes since last seen.".to_string()
    } else {
        sections.join("\n\n")
    }
}

/// Directives are user-authored and read-only to the AI. Before committing any
/// AI write, force every responsibility's `directives` back to whatever the
/// prior on-disk model held for that responsibility id; ids with no prior entry
/// get none. This lets the AI create, edit, and move responsibilities while
/// leaving directives entirely under the user's control. Not applied to
/// `move_responsibilities`, which preserves directives across a deliberate
/// responsibility-id rename.
pub(crate) fn enforce_readonly_directives(model: &mut ScryModel, prior: &ScryModel) {
    let prior_resps = prior
        .nodes
        .iter()
        .flat_map(|n| n.responsibilities.iter())
        .chain(prior.groups.iter().flat_map(|g| g.responsibilities.iter()));
    let prior_dir: HashMap<&str, &Vec<String>> = prior_resps
        .map(|r| (r.id.as_str(), &r.directives))
        .collect();
    let restore = |r: &mut Responsibility| {
        r.directives = prior_dir
            .get(r.id.as_str())
            .map(|d| (*d).clone())
            .unwrap_or_default();
    };
    for n in &mut model.nodes {
        n.responsibilities.iter_mut().for_each(&restore);
    }
    for g in &mut model.groups {
        g.responsibilities.iter_mut().for_each(&restore);
    }
}

/// Layout is frontend-owned. Node `cell` and group `cell`/`size` exist only so
/// the visual canvas can persist hand-arranged positions; correct placement
/// needs DOM measurement the AI doesn't have, so the AI must not place anything.
/// Before committing any AI write, force every node's `cell` and every group's
/// `cell`/`size` back to whatever the prior on-disk model held for that id; ids
/// with no prior entry get none, so newly added nodes/groups are left unplaced
/// for the canvas to lay out after it measures them.
pub(crate) fn enforce_readonly_layout(model: &mut ScryModel, prior: &ScryModel) {
    let prior_cell: HashMap<&str, Option<Cell>> =
        prior.nodes.iter().map(|n| (n.id.as_str(), n.cell)).collect();
    for n in &mut model.nodes {
        n.cell = prior_cell.get(n.id.as_str()).copied().flatten();
    }
    let prior_geom: HashMap<&str, (Option<Cell>, Option<GroupSize>)> = prior
        .groups
        .iter()
        .map(|g| (g.id.as_str(), (g.cell, g.size)))
        .collect();
    for g in &mut model.groups {
        let (cell, size) = prior_geom
            .get(g.id.as_str())
            .copied()
            .unwrap_or((None, None));
        g.cell = cell;
        g.size = size;
    }
}

fn responsibilities_changed(a: &[Responsibility], b: &[Responsibility]) -> bool {
    if a.len() != b.len() {
        return true;
    }
    for (ra, rb) in a.iter().zip(b.iter()) {
        if ra.id != rb.id
            || ra.statement != rb.statement
            || ra.status != rb.status
            || ra.vagrant != rb.vagrant
            || ra.directives != rb.directives
        {
            return true;
        }
    }
    false
}

/// Project root from request param, active model, or cwd.
pub(crate) fn resolve_model_ref(req_project: Option<&str>) -> Result<ModelRef, McpError> {
    let path = match req_project {
        Some(p) => std::path::PathBuf::from(p),
        None => std::env::current_dir().map_err(|e| {
            McpError::internal_error(format!("cannot read cwd: {}", e), None)
        })?,
    };
    Ok(ModelRef::ProjectLocal(path))
}
