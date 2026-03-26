use crate::server::ScryerServer;
use crate::types::*;
use crate::validate::*;
use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, Content},
    tool, tool_router, ErrorData as McpError,
};
use scryer_core::{C4Edge, C4EdgeData};
use std::collections::HashSet;

#[tool_router(router = tool_router_edges, vis = "pub(crate)")]
impl ScryerServer {
    #[tool(description = "Add one or more relationship edges between nodes")]
    fn add_edges(
        &self,
        Parameters(req): Parameters<AddEdgeRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = match self.resolve_model(req.model) {
            Ok(r) => r,
            Err(e) => return Ok(e),
        };
        let mut model = match scryer_core::read_model_at(&model_ref) {
            Ok(m) => m,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to read model '{}': {}",
                    model_ref, e
                ))]));
            }
        };

        let mut added = Vec::new();
        for item in req.edges {
            if !model.nodes.iter().any(|n| n.id == item.source) {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Source node '{}' not found",
                    item.source
                ))]));
            }
            if !model.nodes.iter().any(|n| n.id == item.target) {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Target node '{}' not found",
                    item.target
                ))]));
            }

            if item.label.len() > 30 {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Edge label '{}' exceeds 30 character limit",
                    item.label
                ))]));
            }

            let id = scryer_core::make_edge_id(&item.source, &item.target);
            if model.edges.iter().any(|e| e.id == id) {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Edge from '{}' to '{}' already exists",
                    item.source, item.target
                ))]));
            }

            model.edges.push(C4Edge {
                id: id.clone(),
                source: item.source,
                target: item.target,
                data: Some(C4EdgeData {
                    label: item.label,
                    method: item.method,
                }),
            });
            added.push(id);
        }

        let cross_level_warnings = check_disconnected_nodes(&model);
        let bidir_warnings = check_bidirectional_edges(&model);
        let mention_warnings = check_mention_edges(&model);
        let cross_container_warnings = check_cross_container_edges(&model);
        match scryer_core::write_model_at(&model_ref, &model) {
            Ok(()) => {
                let _ = scryer_core::save_baseline_at(&model_ref, &model);
                let mut msg = format!("Added {} edge(s)", added.len());
                if !cross_level_warnings.is_empty() {
                    msg.push_str(&format!(
                        "\n\n⚠️ DISCONNECTED NODES: The UI shows one abstraction level at a time. \
                        These nodes will appear disconnected at their viewing level. \
                        Use add_edges to fix:\n- {}",
                        cross_level_warnings.join("\n- ")
                    ));
                }
                if !bidir_warnings.is_empty() {
                    msg.push_str(&format!(
                        "\n\n⚠️ BIDIRECTIONAL EDGES: \
                        Review these and merge into a single edge if they represent the same interaction. \
                        Use delete_edges to remove the redundant edge:\n- {}",
                        bidir_warnings.join("\n- ")
                    ));
                }
                if !mention_warnings.is_empty() {
                    msg.push_str(&format!(
                        "\n\n⚠️ MENTIONS WITHOUT EDGES: Descriptions reference nodes with @[Name] \
                        but no edge exists between them. Add the missing edges:\n- {}",
                        mention_warnings.join("\n- ")
                    ));
                }
                if !cross_container_warnings.is_empty() {
                    msg.push_str(&format!(
                        "\n\n⚠️ CROSS-CONTAINER COMPONENT EDGES: Components are internal to their container. \
                        These edges reach inside another container's boundary. \
                        Re-target them to the container node instead:\n- {}",
                        cross_container_warnings.join("\n- ")
                    ));
                }
                Ok(CallToolResult::success(vec![Content::text(msg)]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e)])),
        }
    }

    #[tool(description = "Update one or more existing edges")]
    fn update_edges(
        &self,
        Parameters(req): Parameters<UpdateEdgeRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = match self.resolve_model(req.model) {
            Ok(r) => r,
            Err(e) => return Ok(e),
        };
        let mut model = match scryer_core::read_model_at(&model_ref) {
            Ok(m) => m,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to read model '{}': {}",
                    model_ref, e
                ))]));
            }
        };

        let mut updated = 0usize;
        for item in req.edges {
            let edge = match model.edges.iter_mut().find(|e| e.id == item.edge_id) {
                Some(e) => e,
                None => {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "Edge '{}' not found",
                        item.edge_id
                    ))]));
                }
            };

            let data = edge.data.get_or_insert(C4EdgeData {
                label: String::new(),
                method: None,
            });
            if let Some(label) = item.label {
                if label.len() > 30 {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "Edge label '{}' exceeds 30 character limit",
                        label
                    ))]));
                }
                data.label = label;
            }
            if let Some(tech) = item.method {
                data.method = Some(tech);
            }
            updated += 1;
        }

        match scryer_core::write_model_at(&model_ref, &model) {
            Ok(()) => {
                let _ = scryer_core::save_baseline_at(&model_ref, &model);
                Ok(CallToolResult::success(vec![Content::text(format!(
                    "Updated {} edge(s)",
                    updated
                ))]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e)])),
        }
    }

    #[tool(description = "Delete one or more edges from the model")]
    fn delete_edges(
        &self,
        Parameters(req): Parameters<DeleteEdgeRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = match self.resolve_model(req.model) {
            Ok(r) => r,
            Err(e) => return Ok(e),
        };
        let mut model = match scryer_core::read_model_at(&model_ref) {
            Ok(m) => m,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to read model '{}': {}",
                    model_ref, e
                ))]));
            }
        };

        let ids_to_delete: HashSet<&str> = req.edge_ids.iter().map(|s| s.as_str()).collect();
        for eid in &req.edge_ids {
            if !model.edges.iter().any(|e| e.id == *eid) {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Edge '{}' not found",
                    eid
                ))]));
            }
        }
        model
            .edges
            .retain(|e| !ids_to_delete.contains(e.id.as_str()));

        match scryer_core::write_model_at(&model_ref, &model) {
            Ok(()) => {
                let _ = scryer_core::save_baseline_at(&model_ref, &model);
                Ok(CallToolResult::success(vec![Content::text(format!(
                    "Deleted {} edge(s)",
                    req.edge_ids.len()
                ))]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e)])),
        }
    }
}
