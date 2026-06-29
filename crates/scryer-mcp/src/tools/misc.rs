use crate::helpers::*;
use crate::server::ScryerServer;
use crate::types::*;
use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, Content},
    tool, tool_router, ErrorData as McpError,
};
use scryer_core::Group;
use std::collections::HashSet;

#[tool_router(router = tool_router_misc, vis = "pub(crate)")]
impl ScryerServer {
    #[tool(
        description = "Write the code-side mapping (agent-produced, regenerable). `entries` set source locations keyed by responsibility id — the conformance numerator (where reality discharges a responsibility). Each location is the SPECIFIC line range that does the work: `pattern` = file, `line`/`endLine` = the range, `symbol` = the enclosing definition (anchor + context). A line range must be a PROPER subset of its symbol — when one responsibility is the whole definition's work, omit `line`/`endLine` (a symbol-only anchor means the whole definition). Ranges that cover the whole symbol are normalized to symbol-only anchors and reported back. `schemas` set the declaration location of a schema-kind node (which has properties, not responsibilities) — keyed by node id, normally one location: `pattern` = file, `symbol` = the type name, `line`/`endLine` = the declaration range. `boundaries` set directory globs keyed by node id — the coverage denominator (the code region a node owns); use for containers/components, keeping a child's boundary within its parent's. Pass an empty `locations`/`sources` array to clear an entry."
    )]
    fn update_source_map(
        &self,
        Parameters(mut req): Parameters<UpdateSourceMapRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = resolve_model_ref(req.project.as_deref())?;
        let _lock = match lock_or_err(&model_ref) {
            Ok(l) => l,
            Err(e) => return Ok(e),
        };
        let mut model = match scryer_core::read_planned_at(&model_ref) {
            Ok(m) => m,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to read model at {}: {}",
                    model_ref, e
                ))]));
            }
        };

        let resp_ids: HashSet<&str> = model
            .nodes
            .iter()
            .flat_map(|n| n.responsibilities.iter())
            .chain(model.groups.iter().flat_map(|g| g.responsibilities.iter()))
            .map(|r| r.id.as_str())
            .collect();
        for entry in &req.entries {
            if !resp_ids.contains(entry.responsibility_id.as_str()) {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Responsibility '{}' not found",
                    entry.responsibility_id
                ))]));
            }
        }
        for s in &req.schemas {
            match model.nodes.iter().find(|n| n.id == s.node_id) {
                None => {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "Node '{}' not found",
                        s.node_id
                    ))]));
                }
                Some(n) if n.properties.is_empty() => {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "Node '{}' declares no properties — a `schemas` entry maps a data shape's declaration location; use `entries` (responsibilities) for behavior",
                        s.node_id
                    ))]));
                }
                _ => {}
            }
        }
        for b in &req.boundaries {
            if !model.nodes.iter().any(|n| n.id == b.node_id) {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Node '{}' not found",
                    b.node_id
                ))]));
            }
        }

        // Normalize whole-symbol mappings: an explicit line range must be a
        // PROPER subset of its enclosing symbol. A range covering the whole
        // extent is stripped to the symbol-only anchor (the honest encoding
        // for "this whole definition") and reported back so the agent learns.
        let mut normalized: Vec<String> = Vec::new();
        {
            let mut resolver =
                scryer_extract::anchors::ExtentResolver::new(model_ref.project_path());
            for entry in &mut req.entries {
                for loc in &mut entry.locations {
                    let (Some(sym), Some(line)) = (loc.symbol.clone(), loc.line) else {
                        continue;
                    };
                    let end = loc.end_line.unwrap_or(line);
                    let Some(extent) = resolver.extent(&loc.pattern, &sym, Some(line)) else {
                        continue;
                    };
                    if scryer_extract::anchors::covers_extent(line, end, extent) {
                        loc.line = None;
                        loc.end_line = None;
                        normalized.push(format!(
                            "{}: {} L{}-{} covered the whole symbol `{}` (L{}-{})",
                            entry.responsibility_id, loc.pattern, line, end, sym, extent.0, extent.1
                        ));
                    }
                }
            }
        }

        // Code-side mapping has a SINGLE home, keyed by element: the committed
        // model owns every committed element's anchor; the planned draft holds
        // anchors only for elements it ADDS (not yet committed). So route by
        // element residence — a committed element's anchor is written to
        // committed and kept OUT of the draft (no shadow copy to drift); a
        // plan-added element's anchor stays in the draft and folds into committed
        // later (auto_commit carries it across). The working view merges the two
        // layers for display (see `effectiveSourceMap`), so a committed-side
        // write surfaces immediately without the draft mirroring it.
        let mut committed = scryer_core::read_model_at(&model_ref).ok();
        let (committed_resp_ids, committed_node_ids): (HashSet<String>, HashSet<String>) =
            match committed.as_ref() {
                Some(c) => (
                    c.nodes
                        .iter()
                        .flat_map(|n| n.responsibilities.iter())
                        .chain(c.groups.iter().flat_map(|g| g.responsibilities.iter()))
                        .map(|r| r.id.clone())
                        .collect(),
                    c.nodes.iter().map(|n| n.id.clone()).collect(),
                ),
                None => (HashSet::new(), HashSet::new()),
            };
        let mut committed_dirty = false;

        let count = req.entries.len() + req.schemas.len() + req.boundaries.len();
        for entry in req.entries {
            let key = entry.responsibility_id;
            if entry.locations.is_empty() {
                model.source_map.remove(&key);
                if committed_resp_ids.contains(&key) {
                    if let Some(c) = committed.as_mut() {
                        committed_dirty |= c.source_map.remove(&key).is_some();
                    }
                }
            } else if committed_resp_ids.contains(&key) {
                model.source_map.remove(&key);
                if let Some(c) = committed.as_mut() {
                    c.source_map.insert(key, entry.locations);
                    committed_dirty = true;
                }
            } else {
                model.source_map.insert(key, entry.locations);
            }
        }
        for s in req.schemas {
            let key = s.node_id;
            if s.locations.is_empty() {
                model.source_map.remove(&key);
                if committed_node_ids.contains(&key) {
                    if let Some(c) = committed.as_mut() {
                        committed_dirty |= c.source_map.remove(&key).is_some();
                    }
                }
            } else if committed_node_ids.contains(&key) {
                model.source_map.remove(&key);
                if let Some(c) = committed.as_mut() {
                    c.source_map.insert(key, s.locations);
                    committed_dirty = true;
                }
            } else {
                model.source_map.insert(key, s.locations);
            }
        }
        for b in req.boundaries {
            let key = b.node_id;
            if b.sources.is_empty() {
                model.boundaries.remove(&key);
                if committed_node_ids.contains(&key) {
                    if let Some(c) = committed.as_mut() {
                        committed_dirty |= c.boundaries.remove(&key).is_some();
                    }
                }
            } else if committed_node_ids.contains(&key) {
                model.boundaries.remove(&key);
                if let Some(c) = committed.as_mut() {
                    c.boundaries.insert(key, b.sources);
                    committed_dirty = true;
                }
            } else {
                model.boundaries.insert(key, b.sources);
            }
        }

        if let Err(e) = scryer_core::write_planned_at(&model_ref, &model) {
            return Ok(CallToolResult::error(vec![Content::text(e)]));
        }
        // Persist the committed-side writes in the same lock so the single home
        // is updated atomically with the draft.
        if committed_dirty {
            if let Some(c) = committed {
                if let Err(e) = scryer_core::write_model_at(&model_ref, &c) {
                    return Ok(CallToolResult::error(vec![Content::text(e)]));
                }
            }
        }
        let mut msg = format!("Updated code-side mapping ({} entr(ies))", count);
        if !normalized.is_empty() {
            msg.push_str(&format!(
                "\n\nNormalized {} location(s) — the line range covered the whole enclosing symbol, so it was dropped and the symbol-only anchor kept. A range must be a PROPER subset of its symbol: map the specific lines that do each responsibility's work, or omit line/endLine to mean the whole definition:",
                normalized.len()
            ));
            for n in &normalized {
                msg.push_str(&format!("\n- {}", n));
            }
        }
        Ok(CallToolResult::success(vec![Content::text(msg)]))
    }

    #[tool(
        description = "GENERATION-PIPELINE primitive — create or replace groups in bulk from raw `Group` JSON (used during codebase→model generation). Pass a single group object or an array of groups in `data`. Groups are organizational: at container level they represent deployment units, at component level they represent modules. Each group MUST list its `memberIds` and set `parentNodeId` to the node whose children those members are — parentNodeId anchors the group to that node's level so it renders inside that node's diagram (a memberless or parentNodeId-less group renders empty at the top level). Members must all be at the same C4 level. Groups can carry their own responsibilities. For interactive editing, use the typed `add_group` / `update_group` / `delete_group` instead."
    )]
    fn set_groups(
        &self,
        Parameters(req): Parameters<SetGroupsRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = resolve_model_ref(req.project.as_deref())?;
        let _lock = match lock_or_err(&model_ref) {
            Ok(l) => l,
            Err(e) => return Ok(e),
        };
        let mut model = match scryer_core::read_planned_at(&model_ref) {
            Ok(m) => m,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to read model at {}: {}",
                    model_ref, e
                ))]));
            }
        };

        let prior = model.clone();
        let groups: Vec<Group> = match serde_json::from_str::<Vec<Group>>(&req.data) {
            Ok(arr) => arr,
            Err(_) => match serde_json::from_str::<Group>(&req.data) {
                Ok(g) => vec![g],
                Err(e) => {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "Invalid group JSON: {}",
                        e
                    ))]));
                }
            },
        };
        if groups.is_empty() {
            return Ok(CallToolResult::error(vec![Content::text(
                "Empty group array",
            )]));
        }

        // Validate members exist + share a level
        let node_kinds: std::collections::HashMap<&str, scryer_core::Kind> =
            model.nodes.iter().map(|n| (n.id.as_str(), n.kind)).collect();
        for g in &groups {
            let mut kinds: HashSet<scryer_core::Kind> = HashSet::new();
            for mid in &g.member_ids {
                match node_kinds.get(mid.as_str()) {
                    Some(k) => {
                        kinds.insert(*k);
                    }
                    None => {
                        return Ok(CallToolResult::error(vec![Content::text(format!(
                            "Group '{}' member '{}' is not a node",
                            g.id, mid
                        ))]))
                    }
                }
            }
            if kinds.len() > 1 {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Group '{}' mixes member kinds — all members must be at the same level",
                    g.id
                ))]));
            }
        }

        let count = groups.len();
        for g in groups {
            if let Some(existing) = model.groups.iter_mut().find(|x| x.id == g.id) {
                *existing = g;
            } else {
                model.groups.push(g);
            }
        }
        enforce_readonly_directives(&mut model, &prior);

        if let Err(e) = scryer_core::write_planned_at(&model_ref, &model) {
            return Ok(CallToolResult::error(vec![Content::text(e)]));
        }
        Ok(CallToolResult::success(vec![Content::text(format!(
            "Wrote {} group(s)",
            count
        ))]))
    }

    #[tool(
        description = "Patch an existing group by id — change its name, description, members, or responsibilities. Only fields present in each item are changed; omit a field to leave it. `memberIds`, if given, replaces the membership (2+ nodes, all children of the group's parent node — same C4 level). This is the typed counterpart to `add_group`/`delete_group` for EDITING a group without reassembling raw JSON."
    )]
    fn update_group(
        &self,
        Parameters(req): Parameters<UpdateGroupRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = resolve_model_ref(req.project.as_deref())?;
        let _lock = match lock_or_err(&model_ref) {
            Ok(l) => l,
            Err(e) => return Ok(e),
        };
        let mut model = match scryer_core::read_planned_at(&model_ref) {
            Ok(m) => m,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to read model at {}: {}",
                    model_ref, e
                ))]));
            }
        };

        let prior = model.clone();
        let mut updated = 0usize;
        for item in &req.items {
            // Validate a replacement membership against the group's own level
            // before mutating, so a bad member leaves the model untouched.
            if let Some(members) = &item.member_ids {
                if members.len() < 2 {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "Group '{}' needs at least 2 members",
                        item.group_id
                    ))]));
                }
                let parent_node = match model.groups.iter().find(|g| g.id == item.group_id) {
                    Some(g) => g.parent_node_id.clone(),
                    None => {
                        return Ok(CallToolResult::error(vec![Content::text(format!(
                            "Group '{}' not found",
                            item.group_id
                        ))]))
                    }
                };
                for mid in members {
                    match model.nodes.iter().find(|n| &n.id == mid) {
                        None => {
                            return Ok(CallToolResult::error(vec![Content::text(format!(
                                "Group member '{}' is not a node",
                                mid
                            ))]))
                        }
                        Some(n) if parent_node.is_some() && n.parent_id != parent_node => {
                            return Ok(CallToolResult::error(vec![Content::text(format!(
                                "Group member '{}' is not a child of the group's parent node",
                                mid
                            ))]))
                        }
                        _ => {}
                    }
                }
            }

            let Some(g) = model.groups.iter_mut().find(|g| g.id == item.group_id) else {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Group '{}' not found",
                    item.group_id
                ))]));
            };
            if let Some(v) = &item.name {
                g.name = v.clone();
            }
            if let Some(v) = &item.description {
                g.description = Some(v.clone());
            }
            if let Some(v) = &item.member_ids {
                g.member_ids = v.clone();
            }
            if let Some(v) = &item.responsibilities {
                g.responsibilities = v.clone();
            }
            updated += 1;
        }
        enforce_readonly_directives(&mut model, &prior);

        if let Err(e) = scryer_core::write_planned_at(&model_ref, &model) {
            return Ok(CallToolResult::error(vec![Content::text(e)]));
        }
        Ok(CallToolResult::success(vec![Content::text(format!(
            "Updated {} group(s)",
            updated
        ))]))
    }

    #[tool(description = "Delete a group by id.")]
    fn delete_group(
        &self,
        Parameters(req): Parameters<DeleteGroupRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = resolve_model_ref(req.project.as_deref())?;
        let _lock = match lock_or_err(&model_ref) {
            Ok(l) => l,
            Err(e) => return Ok(e),
        };
        let mut model = match scryer_core::read_planned_at(&model_ref) {
            Ok(m) => m,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to read model at {}: {}",
                    model_ref, e
                ))]));
            }
        };

        let before = model.groups.len();
        model.groups.retain(|g| g.id != req.group_id);
        // Detach any child groups from the deleted parent
        for g in model.groups.iter_mut() {
            if g.parent_group_id.as_deref() == Some(&req.group_id) {
                g.parent_group_id = None;
            }
        }
        if model.groups.len() == before {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "Group '{}' not found",
                req.group_id
            ))]));
        }

        if let Err(e) = scryer_core::write_planned_at(&model_ref, &model) {
            return Ok(CallToolResult::error(vec![Content::text(e)]));
        }
        Ok(CallToolResult::success(vec![Content::text(format!(
            "Deleted group '{}'",
            req.group_id
        ))]))
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::handler::server::wrapper::Parameters;
    use scryer_core::{Group, Kind, ModelRef, Node, ScryModel};

    fn node(id: &str, kind: Kind, name: &str, parent: Option<&str>) -> Node {
        Node {
            id: id.into(),
            kind,
            name: name.into(),
            vagrant: None,
            stale: None,
            parent_id: parent.map(|p| p.into()),
            external: None,
            technology: None,
            description: None,
            responsibilities: Vec::new(),
            properties: Vec::new(),
            icon: None,
            visual: None,
            appearance: None,
            notes: None,
            directives: Vec::new(),
        }
    }

    /// update_group patches an existing group by id: rename + clear
    /// responsibilities, leaving membership intact when memberIds is omitted;
    /// an unknown id is rejected.
    #[test]
    fn update_group_patches_fields_by_id() {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());
        let mut m = ScryModel::new();
        m.nodes.push(node("node-1", Kind::System, "Acme", None));
        m.nodes.push(node("node-2", Kind::Container, "Web", Some("node-1")));
        m.nodes.push(node("node-3", Kind::Container, "Worker", Some("node-1")));
        m.groups.push(Group {
            id: "group-1".into(),
            name: "Backend".into(),
            description: None,
            member_ids: vec!["node-2".into(), "node-3".into()],
            parent_group_id: None,
            parent_node_id: Some("node-1".into()),
            responsibilities: vec![],
            icon: None,
        });
        scryer_core::write_planned_at(&model_ref, &m).unwrap();

        let server = ScryerServer::new();
        let project = dir.path().to_string_lossy().to_string();

        server
            .update_group(Parameters(UpdateGroupRequest {
                project: Some(project.clone()),
                items: vec![UpdateGroupItem {
                    group_id: "group-1".into(),
                    name: Some("Platform".into()),
                    description: Some("deployable backend".into()),
                    member_ids: None,
                    responsibilities: Some(vec![]),
                }],
            }))
            .unwrap();

        let g = scryer_core::read_planned_at(&model_ref).unwrap().groups[0].clone();
        assert_eq!(g.name, "Platform");
        assert_eq!(g.description.as_deref(), Some("deployable backend"));
        assert_eq!(g.member_ids.len(), 2, "membership unchanged when memberIds omitted");
        assert!(g.responsibilities.is_empty(), "responsibilities cleared");

        let res = server
            .update_group(Parameters(UpdateGroupRequest {
                project: Some(project),
                items: vec![UpdateGroupItem {
                    group_id: "group-999".into(),
                    name: Some("X".into()),
                    description: None,
                    member_ids: None,
                    responsibilities: None,
                }],
            }))
            .unwrap();
        assert!(res.is_error.unwrap_or(false), "unknown group id rejected");
    }
}
