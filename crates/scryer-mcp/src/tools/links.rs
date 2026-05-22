use crate::helpers::*;
use crate::server::ScryerServer;
use crate::types::*;
use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, Content},
    tool, tool_router, ErrorData as McpError,
};
use scryer_core::Link;
use std::collections::HashSet;

#[tool_router(router = tool_router_links, vis = "pub(crate)")]
impl ScryerServer {
    #[tool(
        description = "Add one or more links between nodes. Direction is from initiator/requester (src) to provider/dependency (dst). Returns the assigned link IDs. Rejects links with missing endpoints or self-loops."
    )]
    fn add_links(
        &self,
        Parameters(req): Parameters<AddLinkRequest>,
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

        let node_ids: HashSet<String> =
            model.nodes.iter().map(|n| n.id.clone()).collect();
        let mut added: Vec<String> = Vec::new();
        for item in &req.links {
            if !node_ids.contains(&item.src) {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Unknown src node '{}'",
                    item.src
                ))]));
            }
            if !node_ids.contains(&item.dst) {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Unknown dst node '{}'",
                    item.dst
                ))]));
            }
            if item.src == item.dst {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Self-link rejected: {} -> {}",
                    item.src, item.dst
                ))]));
            }
            let id = scryer_core::make_link_id(&item.src, &item.dst);
            let link = Link {
                id: id.clone(),
                src: item.src.clone(),
                dst: item.dst.clone(),
                label: item.label.clone(),
                method: item.method.clone(),
            };
            model.links.push(link);
            added.push(id);
        }

        if let Err(e) = scryer_core::write_model_at(&model_ref, &model) {
            return Ok(CallToolResult::error(vec![Content::text(e)]));
        }
        let _ = scryer_core::save_baseline_at(&model_ref, &model);

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Added {} link(s): {}",
            added.len(),
            added.join(", ")
        ))]))
    }

    #[tool(
        description = "Patch one or more links by id. Only fields present are changed."
    )]
    fn update_links(
        &self,
        Parameters(req): Parameters<UpdateLinkRequest>,
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

        let mut updated = 0usize;
        for u in &req.links {
            let Some(l) = model.links.iter_mut().find(|l| l.id == u.link_id) else {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Link '{}' not found",
                    u.link_id
                ))]));
            };
            if let Some(v) = &u.label {
                l.label = v.clone();
            }
            if let Some(v) = &u.method {
                l.method = Some(v.clone());
            }
            updated += 1;
        }

        if let Err(e) = scryer_core::write_model_at(&model_ref, &model) {
            return Ok(CallToolResult::error(vec![Content::text(e)]));
        }
        let _ = scryer_core::save_baseline_at(&model_ref, &model);

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Updated {} link(s)",
            updated
        ))]))
    }

    #[tool(
        description = "Delete one or more links by id."
    )]
    fn delete_links(
        &self,
        Parameters(req): Parameters<DeleteLinkRequest>,
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

        let target: HashSet<&str> = req.link_ids.iter().map(|s| s.as_str()).collect();
        let before = model.links.len();
        model.links.retain(|l| !target.contains(l.id.as_str()));

        if let Err(e) = scryer_core::write_model_at(&model_ref, &model) {
            return Ok(CallToolResult::error(vec![Content::text(e)]));
        }
        let _ = scryer_core::save_baseline_at(&model_ref, &model);

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Deleted {} link(s)",
            before - model.links.len()
        ))]))
    }
}
