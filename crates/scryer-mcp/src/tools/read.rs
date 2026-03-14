use crate::helpers::*;
use crate::server::ScryerServer;
use crate::types::*;
use crate::validate::*;
use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, Content},
    tool, tool_router, ErrorData as McpError,
};
use scryer_core::{C4Node, SourceLocation};
use std::collections::{HashMap, HashSet};

#[tool_router(router = tool_router_read, vis = "pub(crate)")]
impl ScryerServer {
    #[tool(description = "List all available architecture models")]
    fn list_models(&self) -> Result<CallToolResult, McpError> {
        match scryer_core::list_models() {
            Ok(names) => {
                let text = if names.is_empty() {
                    "No models found. Use set_model to create one.".to_string()
                } else {
                    names.join("\n")
                };
                Ok(CallToolResult::success(vec![Content::text(text)]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e)])),
        }
    }

    #[tool(
        description = "Get the full JSON content of a model. Returns {nodes: [{id, parentId?, data: {name, description, kind, technology?, external?, shape?, status?, sources?, contract?}}], edges: [{id, source, target, data: {label, method?}}], flows: [{id, name, description?, steps: [{id, description?, branches?: [{condition, steps}]}]}], sourceMap: {nodeId: [{pattern, line?, endLine?}]}, contract?, startingLevel?}. Positions and node type are omitted (UI-only). Step descriptions can use @[Name] mentions to reference architecture nodes. For scoped reads, prefer get_node. For implementation, use get_task instead — it handles dependency ordering and returns one work unit at a time."
    )]
    fn get_model(
        &self,
        Parameters(req): Parameters<GetModelRequest>,
    ) -> Result<CallToolResult, McpError> {
        match scryer_core::read_model(&req.name) {
            Ok(model) => {
                let _ = scryer_core::save_baseline(&req.name, &model);
                let mut val = serde_json::to_value(&model).unwrap();
                strip_fields_compact(&mut val);

                externalize_attachments(&mut val, &req.name);
                let json = serde_json::to_string(&val)
                    .unwrap_or_else(|e| format!("Serialization error: {}", e));
                Ok(CallToolResult::success(vec![Content::text(json)]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to read model '{}': {}",
                req.name, e
            ))])),
        }
    }

    #[tool(
        description = "Get a scoped subtree of a model. Returns the target node, all its descendants, edges between them, and edges connecting the subtree to external nodes (with external node names/kinds for context). Use this instead of get_model when you only need to inspect or work on a specific system, container, or component. Response is a JSON object with: `node` (the target), `descendants` (array), `internal_edges` (edges within subtree), `external_edges` (edges connecting subtree to outside, with `external_node_name` and `external_node_kind` fields added)."
    )]
    fn get_node(
        &self,
        Parameters(req): Parameters<GetNodeRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model = match scryer_core::read_model(&req.name) {
            Ok(m) => m,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to read model '{}': {}",
                    req.name, e
                ))]));
            }
        };

        let target = match model.nodes.iter().find(|n| n.id == req.node_id) {
            Some(n) => n,
            None => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Node '{}' not found",
                    req.node_id
                ))]));
            }
        };

        // Collect all descendant IDs
        let mut subtree_ids: HashSet<String> = HashSet::new();
        subtree_ids.insert(req.node_id.clone());
        let mut changed = true;
        while changed {
            changed = false;
            for n in &model.nodes {
                if let Some(pid) = &n.parent_id {
                    if subtree_ids.contains(pid) && !subtree_ids.contains(&n.id) {
                        subtree_ids.insert(n.id.clone());
                        changed = true;
                    }
                }
            }
        }

        let descendants: Vec<&C4Node> = model
            .nodes
            .iter()
            .filter(|n| subtree_ids.contains(&n.id) && n.id != req.node_id)
            .collect();

        // Partition edges
        let mut internal_edges: Vec<serde_json::Value> = Vec::new();
        let mut external_edges: Vec<serde_json::Value> = Vec::new();
        for edge in &model.edges {
            let src_in = subtree_ids.contains(&edge.source);
            let tgt_in = subtree_ids.contains(&edge.target);
            if src_in && tgt_in {
                internal_edges.push(serde_json::to_value(edge).unwrap());
            } else if src_in || tgt_in {
                let mut val = serde_json::to_value(edge).unwrap();
                // Add context about the external node
                let ext_id = if src_in { &edge.target } else { &edge.source };
                if let Some(ext_node) = model.nodes.iter().find(|n| n.id == *ext_id) {
                    val.as_object_mut().unwrap().insert(
                        "external_node_name".to_string(),
                        serde_json::Value::String(ext_node.data.name.clone()),
                    );
                    val.as_object_mut().unwrap().insert(
                        "external_node_kind".to_string(),
                        serde_json::Value::String(kind_str(&ext_node.data.kind).to_string()),
                    );
                }
                external_edges.push(val);
            }
        }

        // Source map entries for subtree nodes
        let source_map: HashMap<&str, &Vec<SourceLocation>> = model
            .source_map
            .iter()
            .filter(|(k, _)| subtree_ids.contains(k.as_str()))
            .map(|(k, v)| (k.as_str(), v))
            .collect();

        let mut result = serde_json::json!({
            "node": target,
            "descendants": descendants,
            "internal_edges": internal_edges,
            "external_edges": external_edges,
            "source_map": source_map,
        });
        strip_ui_fields(&mut result);
        externalize_attachments(&mut result, &req.name);

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&result).unwrap(),
        )]))
    }

    #[tool(description = "Get the C4 modeling rules that govern how diagrams should be structured")]
    fn get_rules(&self) -> Result<CallToolResult, McpError> {
        Ok(CallToolResult::success(vec![Content::text(
            scryer_core::rules::RULES,
        )]))
    }

    #[tool(
        description = "Validate a model against C4 rules. Returns all warnings: disconnected nodes, bidirectional edges, mentions without edges, cross-container component edges. Run this after making changes to catch modeling errors."
    )]
    fn validate_model(
        &self,
        Parameters(req): Parameters<GetModelRequest>,
    ) -> Result<CallToolResult, McpError> {
        match scryer_core::read_model(&req.name) {
            Ok(model) => {
                let disconnected = check_disconnected_nodes(&model);
                let bidir = check_bidirectional_edges(&model);
                let mentions = check_mention_edges(&model);
                let cross_container = check_cross_container_edges(&model);

                let all_warnings: Vec<(&str, Vec<String>)> = vec![
                    ("DISCONNECTED NODES", disconnected),
                    ("BIDIRECTIONAL EDGES", bidir),
                    ("MENTIONS WITHOUT EDGES", mentions),
                    ("CROSS-CONTAINER COMPONENT EDGES", cross_container),
                ];

                let total: usize = all_warnings.iter().map(|(_, w)| w.len()).sum();
                if total == 0 {
                    return Ok(CallToolResult::success(vec![Content::text(
                        format!("Model '{}' passed all checks.", req.name)
                    )]));
                }

                let mut msg = format!("Model '{}' — {} warning(s):", req.name, total);
                for (label, warnings) in all_warnings {
                    if !warnings.is_empty() {
                        msg.push_str(&format!("\n\n⚠️ {}:\n- {}", label, warnings.join("\n- ")));
                    }
                }
                Ok(CallToolResult::success(vec![Content::text(msg)]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e)])),
        }
    }

    #[tool(
        description = "Get the structure of a project directory. Returns an annotated directory tree showing manifests (package.json, Cargo.toml, etc.), infrastructure configs (Dockerfile, fly.toml, SAM templates, CI/CD), and environment templates. Use this before modeling to understand a codebase's structure without manual exploration. The tree uses [manifest], [infrastructure], [environment] annotations and '... (N more)' for collapsed subtrees. Respects .gitignore and skips build output/dependency directories."
    )]
    fn get_structure(
        &self,
        Parameters(req): Parameters<GetStructureRequest>,
    ) -> Result<CallToolResult, McpError> {
        let path = std::path::Path::new(&req.path);
        match scryer_core::scan::project_structure(path) {
            Ok(tree) => Ok(CallToolResult::success(vec![Content::text(tree)])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e)])),
        }
    }

    #[tool(
        description = "Show what changed in a model since the AI last read or wrote it. Returns a human-readable diff listing: nodes added/removed/modified, edges added/removed/modified, contract changes, flows added/removed/modified. Baseline is set automatically on get_model, get_node, set_model, and any write operation. Call this to see what the user changed without re-reading the full model."
    )]
    fn get_changes(
        &self,
        Parameters(req): Parameters<GetChangesRequest>,
    ) -> Result<CallToolResult, McpError> {
        let current = match scryer_core::read_model(&req.name) {
            Ok(m) => m,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to read model '{}': {}",
                    req.name, e
                ))]));
            }
        };

        let baseline = match scryer_core::read_baseline(&req.name) {
            Some(b) => b,
            None => {
                return Ok(CallToolResult::error(vec![Content::text(
                    "No baseline found. Call get_model first to establish a reference point.",
                )]));
            }
        };

        let diff = compute_diff(&baseline, &current);
        Ok(CallToolResult::success(vec![Content::text(diff)]))
    }
}
