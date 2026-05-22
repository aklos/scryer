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
        description = "Set source-map locations (line-precise) for one or more nodes. Pass an empty `locations` array to clear an entry. Use for operations (file + line range) and tests; for wider directory globs attached to containers/components use `sources` on the node itself via `update_nodes`."
    )]
    fn update_source_map(
        &self,
        Parameters(req): Parameters<UpdateSourceMapRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = resolve_model_ref(req.project.as_deref())?;
        let mut model = match scryer_core::read_model_at(&model_ref) {
            Ok(m) => m,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to read model at {}: {}",
                    model_ref, e
                ))]));
            }
        };

        for entry in &req.entries {
            if !model.nodes.iter().any(|n| n.id == entry.node_id) {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Node '{}' not found",
                    entry.node_id
                ))]));
            }
        }
        let count = req.entries.len();
        for entry in req.entries {
            if entry.locations.is_empty() {
                model.source_map.remove(&entry.node_id);
            } else {
                model.source_map.insert(entry.node_id, entry.locations);
            }
        }

        if let Err(e) = scryer_core::write_model_at(&model_ref, &model) {
            return Ok(CallToolResult::error(vec![Content::text(e)]));
        }
        let _ = scryer_core::save_baseline_at(&model_ref, &model);
        Ok(CallToolResult::success(vec![Content::text(format!(
            "Updated source map for {} node(s)",
            count
        ))]))
    }

    #[tool(
        description = "Create or replace one or more groups. Pass a single group object or an array of groups in `data`. Groups are organizational: at container level they represent deployment units, at component level they represent modules. Members must all be at the same C4 level. Groups can carry their own responsibilities."
    )]
    fn set_groups(
        &self,
        Parameters(req): Parameters<SetGroupsRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = resolve_model_ref(req.project.as_deref())?;
        let mut model = match scryer_core::read_model_at(&model_ref) {
            Ok(m) => m,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to read model at {}: {}",
                    model_ref, e
                ))]));
            }
        };

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

        if let Err(e) = scryer_core::write_model_at(&model_ref, &model) {
            return Ok(CallToolResult::error(vec![Content::text(e)]));
        }
        let _ = scryer_core::save_baseline_at(&model_ref, &model);
        Ok(CallToolResult::success(vec![Content::text(format!(
            "Wrote {} group(s)",
            count
        ))]))
    }

    #[tool(description = "Delete a group by id.")]
    fn delete_group(
        &self,
        Parameters(req): Parameters<DeleteGroupRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = resolve_model_ref(req.project.as_deref())?;
        let mut model = match scryer_core::read_model_at(&model_ref) {
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

        if let Err(e) = scryer_core::write_model_at(&model_ref, &model) {
            return Ok(CallToolResult::error(vec![Content::text(e)]));
        }
        let _ = scryer_core::save_baseline_at(&model_ref, &model);
        Ok(CallToolResult::success(vec![Content::text(format!(
            "Deleted group '{}'",
            req.group_id
        ))]))
    }

    #[tool(
        description = "Pause or resume drift detection for this project's model. Call with active=true before implementing code; active=false after."
    )]
    fn set_implementing(
        &self,
        Parameters(req): Parameters<SetImplementingRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = resolve_model_ref(req.project.as_deref())?;
        if let Err(e) = scryer_core::set_implementing_at(&model_ref, req.active) {
            return Ok(CallToolResult::error(vec![Content::text(e)]));
        }
        Ok(CallToolResult::success(vec![Content::text(format!(
            "Implementing flag = {}",
            req.active
        ))]))
    }
}
