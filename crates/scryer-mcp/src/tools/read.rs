use crate::helpers::*;
use crate::server::ScryerServer;
use crate::types::*;
use crate::validate::*;
use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, Content},
    tool, tool_router, ErrorData as McpError,
};
use scryer_core::{C4Node, ModelRef, SourceLocation};
use std::collections::{HashMap, HashSet};

impl ScryerServer {
    /// Resolve an optional model name to a ModelRef.
    /// Priority: explicit name > session active model > cwd project-local > cwd global match.
    /// Sets the resolved model as the session's active model.
    pub(crate) fn resolve_model(&self, name: Option<String>) -> Result<ModelRef, CallToolResult> {
        let model_ref = match name {
            Some(n) => ModelRef::parse(&n),
            None => {
                // Check session state first
                if let Some(ref active) = *self.active_model.lock().unwrap() {
                    return Ok(active.clone());
                }
                // Fall back to cwd discovery
                let cwd = std::env::current_dir().map_err(|_| {
                    CallToolResult::error(vec![Content::text("Cannot determine current working directory")])
                })?;
                scryer_core::resolve_model_for_project_ref(&cwd).ok_or_else(|| {
                    CallToolResult::error(vec![Content::text(
                        "No model found for the current working directory. Pass a model name explicitly, or use list_models to see available models."
                    )])
                })?
            }
        };
        // Remember as active
        *self.active_model.lock().unwrap() = Some(model_ref.clone());
        Ok(model_ref)
    }
}

#[tool_router(router = tool_router_read, vis = "pub(crate)")]
impl ScryerServer {
    #[tool(description = "List available models. Shows the project model (from .scryer/model.scry in the current working directory, marked with *) and any templates (in ~/.scryer/). The project model is auto-selected as the active model. To work on a template instead, pass its name to any tool.")]
    fn list_models(&self) -> Result<CallToolResult, McpError> {
        match scryer_core::list_all_models() {
            Ok(entries) => {
                let text = if entries.is_empty() {
                    "No models found. Use set_model to create one.".to_string()
                } else {
                    let cwd = std::env::current_dir().ok();
                    let cwd_canonical = cwd.as_ref().and_then(|p| std::fs::canonicalize(p).ok());
                    let mut project_lines = Vec::new();
                    let mut template_lines = Vec::new();
                    for entry in &entries {
                        let linked = if let Some(ref cc) = cwd_canonical {
                            entry.project_path.as_ref()
                                .and_then(|pp| std::fs::canonicalize(pp).ok())
                                .map_or(false, |mc| &mc == cc)
                        } else {
                            false
                        };
                        if entry.is_local {
                            let prefix = if linked { "* " } else { "  " };
                            project_lines.push(format!("{}{} (project)", prefix, entry.display_name));
                        } else {
                            template_lines.push(format!("  {}", entry.display_name));
                        }
                    }
                    let mut sections = Vec::new();
                    if !project_lines.is_empty() {
                        sections.push(project_lines.join("\n"));
                    }
                    if !template_lines.is_empty() {
                        sections.push(format!("Templates:\n{}", template_lines.join("\n")));
                    }
                    sections.join("\n\n")
                };
                Ok(CallToolResult::success(vec![Content::text(text)]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e)])),
        }
    }

    #[tool(
        description = "Get the full JSON content of a model. If name is omitted, automatically resolves the model linked to the current working directory (project-local .scryer/model.scry first, then global). Returns {nodes: [{id, parentId?, data: {name, description, kind, technology?, external?, shape?, status?, sources?, contract?}}], edges: [{id, source, target, data: {label, method?}}], flows: [{id, name, description?, steps: [{id, description?, branches?: [{condition, steps}]}]}], sourceMap: {nodeId: [{pattern, line?, endLine?}]}, contract?, startingLevel?}. Positions and node type are omitted (UI-only). Step descriptions can use @[Name] mentions to reference architecture nodes. For scoped reads, prefer get_node. For implementation, use get_task instead — it handles dependency ordering and returns one work unit at a time."
    )]
    fn get_model(
        &self,
        Parameters(req): Parameters<GetModelRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = match self.resolve_model(req.name) {
            Ok(r) => r,
            Err(e) => return Ok(e),
        };
        match scryer_core::read_model_at(&model_ref) {
            Ok(model) => {
                let _ = scryer_core::save_baseline_at(&model_ref, &model);
                let mut val = serde_json::to_value(&model).unwrap();
                strip_fields_compact(&mut val);

                let ref_str = model_ref.to_ref_string();
                externalize_attachments(&mut val, &ref_str);
                let json = serde_json::to_string(&val)
                    .unwrap_or_else(|e| format!("Serialization error: {}", e));
                Ok(CallToolResult::success(vec![Content::text(json)]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to read model '{}': {}",
                model_ref, e
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
        let model_ref = match self.resolve_model(req.name) {
            Ok(r) => r,
            Err(e) => return Ok(e),
        };
        let model = match scryer_core::read_model_at(&model_ref) {
            Ok(m) => m,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to read model '{}': {}",
                    model_ref, e
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

        let ref_str = model_ref.to_ref_string();
        let mut result = serde_json::json!({
            "node": target,
            "descendants": descendants,
            "internal_edges": internal_edges,
            "external_edges": external_edges,
            "source_map": source_map,
        });
        strip_ui_fields(&mut result);
        externalize_attachments(&mut result, &ref_str);

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
        let model_ref = match self.resolve_model(req.name) {
            Ok(r) => r,
            Err(e) => return Ok(e),
        };
        match scryer_core::read_model_at(&model_ref) {
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
                        format!("Model '{}' passed all checks.", model_ref)
                    )]));
                }

                let mut msg = format!("Model '{}' — {total} warning(s):", model_ref);
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
        let model_ref = match self.resolve_model(req.name) {
            Ok(r) => r,
            Err(e) => return Ok(e),
        };
        let current = match scryer_core::read_model_at(&model_ref) {
            Ok(m) => m,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to read model '{}': {}",
                    model_ref, e
                ))]));
            }
        };

        let baseline = match scryer_core::read_baseline_at(&model_ref) {
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
