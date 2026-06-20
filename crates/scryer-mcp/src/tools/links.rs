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
        description = "Add one or more links between nodes. Direction is from initiator/requester (src) to provider/dependency (dst). Returns the assigned link IDs. Relationships connect nodes at the SAME C4 level: src and dst must be siblings (same parent), or the deeper node's parent must already link to the other node (so it shows as a reference on that surface) — otherwise the link is rejected with guidance. The whole batch is rejected if any link is illegal, so order parent-level links before the child-level links that depend on them. Also rejects missing endpoints, self-loops, and links between an ancestor and its descendant."
    )]
    fn add_links(
        &self,
        Parameters(req): Parameters<AddLinkRequest>,
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

        // Enforce the same-level / reference rule with every new link present,
        // so a batch may add a parent-level link and the child-level link that
        // depends on it together (order within the batch doesn't matter). Any
        // illegal link rejects the whole batch — nothing is written.
        let violations: Vec<String> = req
            .links
            .iter()
            .filter_map(|item| {
                scryer_core::validate::link_violation(&model, &item.src, &item.dst)
                    .map(|v| scryer_core::validate::describe_violation(&model, &item.src, &item.dst, &v))
            })
            .collect();
        if !violations.is_empty() {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "No links added — {} rejected:\n{}",
                violations.len(),
                violations.join("\n")
            ))]));
        }

        if let Err(e) = scryer_core::write_planned_at(&model_ref, &model) {
            return Ok(CallToolResult::error(vec![Content::text(e)]));
        }

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

        if let Err(e) = scryer_core::write_planned_at(&model_ref, &model) {
            return Ok(CallToolResult::error(vec![Content::text(e)]));
        }

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

        let target: HashSet<&str> = req.link_ids.iter().map(|s| s.as_str()).collect();
        let before = model.links.len();
        model.links.retain(|l| !target.contains(l.id.as_str()));

        if let Err(e) = scryer_core::write_planned_at(&model_ref, &model) {
            return Ok(CallToolResult::error(vec![Content::text(e)]));
        }

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Deleted {} link(s)",
            before - model.links.len()
        ))]))
    }
}
