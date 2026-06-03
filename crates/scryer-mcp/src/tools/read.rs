use crate::helpers::*;
use crate::server::ScryerServer;
use crate::types::*;
use crate::validate;
use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, Content},
    tool, tool_router, ErrorData as McpError,
};
use scryer_core::Node;
use std::collections::HashSet;

/// Placeholder for tools that take no parameters.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub(crate) struct EmptyRequest {}

#[tool_router(router = tool_router_read, vis = "pub(crate)")]
impl ScryerServer {
    #[tool(
        description = "Read the full scryer model for a project. Returns a denormalized graph view: each node carries its childIds, incomingLinks, and outgoingLinks. The MCP baseline is updated on every call so `get_changes` can diff against this snapshot."
    )]
    fn get_model(
        &self,
        Parameters(req): Parameters<GetModelRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = resolve_model_ref(req.project.as_deref())?;
        *self.active_model.lock().unwrap() = Some(model_ref.clone());

        let model = match scryer_core::read_model_at(&model_ref) {
            Ok(m) => m,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to read model at {}: {}",
                    model_ref, e
                ))]));
            }
        };
        let _ = scryer_core::save_baseline_at(&model_ref, &model);

        let nodes_json: Vec<serde_json::Value> =
            model.nodes.iter().map(|n| denormalize_node(n, &model)).collect();
        let mut payload = serde_json::json!({
            "version": model.version,
            "nodes": nodes_json,
            "links": model.links,
            "groups": model.groups,
            "sourceMap": model.source_map,
            "boundaries": model.boundaries,
        });
        strip_fields_compact(&mut payload);
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string()),
        )]))
    }

    #[tool(
        description = "Read one node and its subtree. Returns the target node, all its descendants, internal links among them, and external links to nodes outside the subtree (with the external nodes' names + kinds for context). `referencesForChildren` lists the partners of THIS node's own links — the only nodes its children may link to, since relationships connect nodes at the same level (a child links to an external/sibling only if this node already does). Includes the code-side mapping for the subtree: `sourceMap` (line-precise locations keyed by responsibility id, plus schema-node declaration locations keyed by node id) and `boundaries` (globs keyed by node id)."
    )]
    fn get_node(
        &self,
        Parameters(req): Parameters<GetNodeRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = resolve_model_ref(req.project.as_deref())?;
        *self.active_model.lock().unwrap() = Some(model_ref.clone());

        let model = match scryer_core::read_model_at(&model_ref) {
            Ok(m) => m,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to read model at {}: {}",
                    model_ref, e
                ))]));
            }
        };
        let _ = scryer_core::save_baseline_at(&model_ref, &model);

        if !model.nodes.iter().any(|n| n.id == req.node_id) {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "Node '{}' not found",
                req.node_id
            ))]));
        }

        let mut subtree_ids: HashSet<String> = HashSet::new();
        subtree_ids.insert(req.node_id.clone());
        let mut frontier = vec![req.node_id.clone()];
        while let Some(id) = frontier.pop() {
            for child in model.nodes.iter().filter(|n| n.parent_id.as_deref() == Some(&id)) {
                if subtree_ids.insert(child.id.clone()) {
                    frontier.push(child.id.clone());
                }
            }
        }

        let subtree_nodes: Vec<&Node> = model
            .nodes
            .iter()
            .filter(|n| subtree_ids.contains(&n.id))
            .collect();

        let internal_links: Vec<_> = model
            .links
            .iter()
            .filter(|l| subtree_ids.contains(&l.src) && subtree_ids.contains(&l.dst))
            .collect();

        let external_links: Vec<_> = model
            .links
            .iter()
            .filter(|l| {
                let s = subtree_ids.contains(&l.src);
                let d = subtree_ids.contains(&l.dst);
                (s && !d) || (!s && d)
            })
            .collect();

        let mut context_ids: HashSet<&str> = HashSet::new();
        for l in &external_links {
            if !subtree_ids.contains(&l.src) {
                context_ids.insert(l.src.as_str());
            }
            if !subtree_ids.contains(&l.dst) {
                context_ids.insert(l.dst.as_str());
            }
        }
        let context_nodes: Vec<serde_json::Value> = model
            .nodes
            .iter()
            .filter(|n| context_ids.contains(n.id.as_str()))
            .map(|n| {
                serde_json::json!({
                    "id": n.id,
                    "name": n.name,
                    "kind": kind_str(&n.kind),
                })
            })
            .collect();

        // Source map is keyed by responsibility id (include entries for any
        // responsibility owned by a node in the subtree) or by a schema node id
        // (include entries for any schema node in the subtree).
        let subtree_resp_ids: HashSet<&str> = subtree_nodes
            .iter()
            .flat_map(|n| n.responsibilities.iter())
            .map(|r| r.id.as_str())
            .collect();
        let source_map: serde_json::Map<String, serde_json::Value> = model
            .source_map
            .iter()
            .filter(|(k, _)| {
                subtree_resp_ids.contains(k.as_str()) || subtree_ids.contains(k.as_str())
            })
            .map(|(k, v)| (k.clone(), serde_json::to_value(v).unwrap_or(serde_json::Value::Null)))
            .collect();

        // Boundaries are keyed by node id.
        let boundaries: serde_json::Map<String, serde_json::Value> = model
            .boundaries
            .iter()
            .filter(|(k, _)| subtree_ids.contains(k.as_str()))
            .map(|(k, v)| (k.clone(), serde_json::to_value(v).unwrap_or(serde_json::Value::Null)))
            .collect();

        // References available to this node's children: the partners of the
        // node's OWN links. Because links are same-level, a child may only link
        // to a node this node already links to — these are exactly that set, so
        // the agent knows what its components/containers are allowed to wire to.
        let references_for_children: Vec<serde_json::Value> = model
            .links
            .iter()
            .filter_map(|l| {
                let (other, direction) = if l.src == req.node_id {
                    (&l.dst, "outgoing")
                } else if l.dst == req.node_id {
                    (&l.src, "incoming")
                } else {
                    return None;
                };
                let n = model.nodes.iter().find(|n| &n.id == other)?;
                Some(serde_json::json!({
                    "id": n.id,
                    "name": n.name,
                    "kind": kind_str(&n.kind),
                    "direction": direction,
                    "label": l.label,
                }))
            })
            .collect();

        let mut payload = serde_json::json!({
            "node": subtree_nodes.iter().find(|n| n.id == req.node_id),
            "descendants": subtree_nodes.iter().filter(|n| n.id != req.node_id).collect::<Vec<_>>(),
            "internalLinks": internal_links,
            "externalLinks": external_links,
            "contextNodes": context_nodes,
            "referencesForChildren": references_for_children,
            "sourceMap": source_map,
            "boundaries": boundaries,
        });
        strip_fields_compact(&mut payload);
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string()),
        )]))
    }

    #[tool(
        description = "Return the scryer modeling rules — the constraints that govern responsibilities, decomposition, groups, and link semantics. Call before building or editing a model."
    )]
    fn get_rules(
        &self,
        Parameters(_): Parameters<EmptyRequest>,
    ) -> Result<CallToolResult, McpError> {
        Ok(CallToolResult::success(vec![Content::text(
            scryer_core::rules::RULES.to_string(),
        )]))
    }

    #[tool(
        description = "Diff the current model against the last-seen baseline. Baseline is updated on every read/write tool call. Use this to see what the user changed since the agent last read, without re-reading the full model."
    )]
    fn get_changes(
        &self,
        Parameters(req): Parameters<GetChangesRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = resolve_model_ref(req.project.as_deref())?;
        let current = match scryer_core::read_model_at(&model_ref) {
            Ok(m) => m,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to read model at {}: {}",
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

    #[tool(
        description = "Annotated project directory tree. Surfaces manifests ([manifest]), infrastructure configs ([infrastructure]), and environment templates ([environment]). Use before modeling to identify deployable units, data stores, external integrations, and frameworks. Respects .gitignore and skips build output / dependency directories."
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
        description = "Run the structural validator. Returns a list of warnings: parent-kind mismatches, unknown link endpoints, group members at mixed levels, and source-map entries that reference unknown ids. An empty list means the model is structurally clean (does NOT check responsibility quality)."
    )]
    fn validate_model(
        &self,
        Parameters(req): Parameters<ValidateModelRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = resolve_model_ref(req.project.as_deref())?;
        let model = match scryer_core::read_model_at(&model_ref) {
            Ok(m) => m,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to read model at {}: {}",
                    model_ref, e
                ))]));
            }
        };
        let mut warnings = validate::validate(&model);
        warnings.extend(validate::validate_coverage(&model, model_ref.project_path()));
        if warnings.is_empty() {
            Ok(CallToolResult::success(vec![Content::text(
                "Model is structurally clean.",
            )]))
        } else {
            let mut msg = format!("Model '{}' — {} warning(s):", model_ref, warnings.len());
            for w in &warnings {
                msg.push_str(&format!("\n- {}", w));
            }
            Ok(CallToolResult::success(vec![Content::text(msg)]))
        }
    }
}
