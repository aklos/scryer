use crate::helpers::*;
use crate::server::ScryerServer;
use crate::types::*;
use crate::validate;
use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, Content},
    tool, tool_router, ErrorData as McpError,
};
use scryer_core::{Kind, Link, Node, ScryModel};
use std::collections::HashSet;

/// Remove code-side mapping for a set of nodes about to be deleted: their
/// boundary globs (keyed by node id) and the source-map locations of every
/// responsibility they own (keyed by responsibility id). Call before the nodes
/// are retained out of the model, since it reads their responsibilities.
fn prune_code_map(model: &mut ScryModel, removed_node_ids: &HashSet<String>) {
    let removed_resp_ids: HashSet<String> = model
        .nodes
        .iter()
        .filter(|n| removed_node_ids.contains(&n.id))
        .flat_map(|n| n.responsibilities.iter().map(|r| r.id.clone()))
        .collect();
    // source_map keys are responsibility ids or schema node ids — drop both.
    model
        .source_map
        .retain(|k, _| !removed_resp_ids.contains(k) && !removed_node_ids.contains(k));
    model.boundaries.retain(|k, _| !removed_node_ids.contains(k));
}

#[tool_router(router = tool_router_nodes, vis = "pub(crate)")]
impl ScryerServer {
    #[tool(
        description = "Replace the entire model. The JSON payload must include `version: \"0.3\"`, `nodes`, `links`, and optional `groups` and `sourceMap`. Validation warnings are returned but the write is committed regardless — fix the warnings in a follow-up call."
    )]
    fn set_model(
        &self,
        Parameters(req): Parameters<SetModelRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = resolve_model_ref(req.project.as_deref())?;
        *self.active_model.lock().unwrap() = Some(model_ref.clone());
        let _lock = match lock_or_err(&model_ref) {
            Ok(l) => l,
            Err(e) => return Ok(e),
        };

        let mut model: ScryModel = match serde_json::from_str(&req.data) {
            Ok(m) => m,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Invalid model JSON: {}",
                    e
                ))]));
            }
        };
        if let Ok(prior) = scryer_core::read_model_at(&model_ref) {
            enforce_readonly_directives(&mut model, &prior);
            enforce_readonly_layout(&mut model, &prior);
        } else {
            enforce_readonly_directives(&mut model, &ScryModel::default());
            enforce_readonly_layout(&mut model, &ScryModel::default());
        }
        if model.version != scryer_core::SCRY_VERSION {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "Model version '{}' does not match expected '{}'",
                model.version,
                scryer_core::SCRY_VERSION
            ))]));
        }

        if let Err(e) = scryer_core::write_model_at(&model_ref, &model) {
            return Ok(CallToolResult::error(vec![Content::text(e)]));
        }
        let _ = scryer_core::save_baseline_at(&model_ref, &model);

        let warnings = validate::validate(&model);
        let mut msg = format!(
            "Wrote model to {} — {} nodes, {} links, {} groups",
            model_ref,
            model.nodes.len(),
            model.links.len(),
            model.groups.len()
        );
        if !warnings.is_empty() {
            msg.push_str(&format!("\n\n{} warning(s):", warnings.len()));
            for w in warnings {
                msg.push_str(&format!("\n- {}", w));
            }
        }
        Ok(CallToolResult::success(vec![Content::text(msg)]))
    }

    #[tool(
        description = "Add one or more nodes to the model. Pass an array of node items. IDs are auto-assigned. Use set_node or set_model when adding many nodes at once with their links."
    )]
    fn add_nodes(
        &self,
        Parameters(req): Parameters<AddNodeRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = resolve_model_ref(req.project.as_deref())?;
        let _lock = match lock_or_err(&model_ref) {
            Ok(l) => l,
            Err(e) => return Ok(e),
        };
        let mut model = match scryer_core::read_model_at(&model_ref) {
            Ok(m) => m,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to read model at {}: {}",
                    model_ref, e
                ))]));
            }
        };

        let prior = model.clone();
        let mut added_ids: Vec<String> = Vec::new();
        for item in &req.nodes {
            let kind = parse_kind(&item.kind)?;
            let id = scryer_core::next_node_id(&model);
            let node = Node {
                id: id.clone(),
                kind,
                name: item.name.clone(),
                parent_id: item.parent_id.clone(),
                external: item.external,
                technology: item.technology.clone(),
                description: item.description.clone(),
                responsibilities: item.responsibilities.clone().unwrap_or_default(),
                properties: item.properties.clone().unwrap_or_default(),
                cell: None,
                icon: None,
                deprecated: None,
                relocated: None,
                locked: None,
                relocated_to: None,
                relocated_from: None,
            };
            model.nodes.push(node);
            added_ids.push(id);
        }
        enforce_readonly_directives(&mut model, &prior);
        enforce_readonly_layout(&mut model, &prior);

        if let Err(e) = scryer_core::write_model_at(&model_ref, &model) {
            return Ok(CallToolResult::error(vec![Content::text(e)]));
        }
        let _ = scryer_core::save_baseline_at(&model_ref, &model);

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Added {} node(s): {}",
            added_ids.len(),
            added_ids.join(", ")
        ))]))
    }

    #[tool(
        description = "Patch one or more existing nodes by id. Only fields present in each item are changed. Pass `responsibilities` or `properties` to replace the whole array (pass an empty array to clear). When changing `status`, pass `reason` with a short factual explanation. Code-side mapping (line-precise locations per responsibility, and boundary globs per node) is written separately via `update_source_map`."
    )]
    fn update_nodes(
        &self,
        Parameters(req): Parameters<UpdateNodeRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = resolve_model_ref(req.project.as_deref())?;
        let _lock = match lock_or_err(&model_ref) {
            Ok(l) => l,
            Err(e) => return Ok(e),
        };
        let mut model = match scryer_core::read_model_at(&model_ref) {
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
        for u in &req.nodes {
            let Some(n) = model.nodes.iter_mut().find(|n| n.id == u.node_id) else {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Node '{}' not found",
                    u.node_id
                ))]));
            };

            if let Some(v) = &u.kind {
                n.kind = parse_kind(v)?;
            }
            if let Some(v) = &u.name {
                n.name = v.clone();
            }
            if let Some(v) = &u.description {
                n.description = Some(v.clone());
            }
            if let Some(v) = &u.technology {
                n.technology = Some(v.clone());
            }
            if let Some(v) = u.external {
                n.external = Some(v);
            }
            if let Some(v) = &u.responsibilities {
                n.responsibilities = v.clone();
            }
            if let Some(v) = &u.properties {
                n.properties = v.clone();
            }
            if let Some(v) = u.deprecated {
                n.deprecated = if v { Some(true) } else { None };
            }
            if let Some(v) = u.relocated {
                n.relocated = if v { Some(true) } else { None };
            }
            if let Some(v) = &u.parent_id {
                n.parent_id = Some(v.clone());
            }
            updated += 1;
        }
        enforce_readonly_directives(&mut model, &prior);
        enforce_readonly_layout(&mut model, &prior);

        if let Err(e) = scryer_core::write_model_at(&model_ref, &model) {
            return Ok(CallToolResult::error(vec![Content::text(e)]));
        }
        let _ = scryer_core::save_baseline_at(&model_ref, &model);

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Updated {} node(s)",
            updated
        ))]))
    }

    #[tool(
        description = "Replace a node's subtree. The data payload is JSON `{ \"nodes\": [...], \"links\": [...] }` where every node in `nodes` has a parent chain rooted at `node_id`. All existing descendants of `node_id` are removed before the new subtree is inserted. Links replace any link whose endpoints are inside the subtree; links connecting to nodes outside the subtree are also accepted."
    )]
    fn set_node(
        &self,
        Parameters(req): Parameters<SetNodeRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = resolve_model_ref(req.project.as_deref())?;
        let _lock = match lock_or_err(&model_ref) {
            Ok(l) => l,
            Err(e) => return Ok(e),
        };
        let mut model = match scryer_core::read_model_at(&model_ref) {
            Ok(m) => m,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to read model at {}: {}",
                    model_ref, e
                ))]));
            }
        };

        if !model.nodes.iter().any(|n| n.id == req.node_id) {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "Node '{}' not found",
                req.node_id
            ))]));
        }
        let prior = model.clone();

        #[derive(serde::Deserialize)]
        struct SubtreePayload {
            nodes: Vec<Node>,
            #[serde(default)]
            links: Vec<Link>,
        }
        let payload: SubtreePayload = match serde_json::from_str(&req.data) {
            Ok(p) => p,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Invalid subtree JSON: {}",
                    e
                ))]));
            }
        };

        // Compute current descendants
        let mut to_remove: HashSet<String> = HashSet::new();
        let mut frontier = vec![req.node_id.clone()];
        while let Some(id) = frontier.pop() {
            for child in model.nodes.iter().filter(|n| n.parent_id.as_deref() == Some(&id)) {
                if to_remove.insert(child.id.clone()) {
                    frontier.push(child.id.clone());
                }
            }
        }

        // Drop descendants + their links + their code-side mapping
        prune_code_map(&mut model, &to_remove);
        model.nodes.retain(|n| !to_remove.contains(&n.id));
        model
            .links
            .retain(|l| !to_remove.contains(&l.src) && !to_remove.contains(&l.dst));

        // Append new subtree nodes (skip node_id itself if accidentally included)
        for n in payload.nodes {
            if n.id == req.node_id {
                continue;
            }
            model.nodes.push(n);
        }
        // Append links, skipping any whose endpoints don't exist
        let node_ids: HashSet<&str> = model.nodes.iter().map(|n| n.id.as_str()).collect();
        for l in payload.links {
            if node_ids.contains(l.src.as_str()) && node_ids.contains(l.dst.as_str()) {
                model.links.push(l);
            }
        }
        enforce_readonly_directives(&mut model, &prior);
        enforce_readonly_layout(&mut model, &prior);

        if let Err(e) = scryer_core::write_model_at(&model_ref, &model) {
            return Ok(CallToolResult::error(vec![Content::text(e)]));
        }
        let _ = scryer_core::save_baseline_at(&model_ref, &model);

        let warnings = validate::validate(&model);
        let mut msg = format!("Replaced subtree under {}", req.node_id);
        if !warnings.is_empty() {
            msg.push_str(&format!("\n\n{} warning(s):", warnings.len()));
            for w in warnings {
                msg.push_str(&format!("\n- {}", w));
            }
        }
        Ok(CallToolResult::success(vec![Content::text(msg)]))
    }

    #[tool(
        description = "Delete one or more nodes. Each node's descendants, connected links, and source-map entries are also removed."
    )]
    fn delete_nodes(
        &self,
        Parameters(req): Parameters<DeleteNodeRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = resolve_model_ref(req.project.as_deref())?;
        let _lock = match lock_or_err(&model_ref) {
            Ok(l) => l,
            Err(e) => return Ok(e),
        };
        let mut model = match scryer_core::read_model_at(&model_ref) {
            Ok(m) => m,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to read model at {}: {}",
                    model_ref, e
                ))]));
            }
        };

        // Expand to include all descendants
        let mut to_remove: HashSet<String> = req.node_ids.iter().cloned().collect();
        let mut frontier: Vec<String> = req.node_ids.clone();
        while let Some(id) = frontier.pop() {
            for child in model.nodes.iter().filter(|n| n.parent_id.as_deref() == Some(&id)) {
                if to_remove.insert(child.id.clone()) {
                    frontier.push(child.id.clone());
                }
            }
        }

        let before = model.nodes.len();
        prune_code_map(&mut model, &to_remove);
        model.nodes.retain(|n| !to_remove.contains(&n.id));
        model
            .links
            .retain(|l| !to_remove.contains(&l.src) && !to_remove.contains(&l.dst));
        // Prune dead group memberships
        for g in model.groups.iter_mut() {
            g.member_ids.retain(|m| !to_remove.contains(m));
        }

        if let Err(e) = scryer_core::write_model_at(&model_ref, &model) {
            return Ok(CallToolResult::error(vec![Content::text(e)]));
        }
        let _ = scryer_core::save_baseline_at(&model_ref, &model);

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Deleted {} node(s) (including descendants)",
            before - model.nodes.len()
        ))]))
    }

    #[tool(
        description = "Move responsibilities between nodes. Enforces transition rules: proposed responsibilities just move (no trace at source); implemented/verified responsibilities leave a locked relocated copy at the source and arrive as relocated at the destination. Deleting the destination copy later unlocks the source. Vagrant and locked responsibilities cannot be moved."
    )]
    fn move_responsibilities(
        &self,
        Parameters(req): Parameters<MoveResponsibilitiesRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = resolve_model_ref(req.project.as_deref())?;
        let _lock = match lock_or_err(&model_ref) {
            Ok(l) => l,
            Err(e) => return Ok(e),
        };
        let mut model = match scryer_core::read_model_at(&model_ref) {
            Ok(m) => m,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to read model at {}: {}",
                    model_ref, e
                ))]));
            }
        };

        let mut moved = 0usize;
        for mv in &req.moves {
            let resp = {
                let from_node = model.nodes.iter().find(|n| n.id == mv.from_node_id);
                let Some(from_node) = from_node else {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "Source node '{}' not found", mv.from_node_id
                    ))]));
                };
                let Some(r) = from_node.responsibilities.iter().find(|r| r.id == mv.responsibility_id) else {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "Responsibility '{}' not found on node '{}'", mv.responsibility_id, mv.from_node_id
                    ))]));
                };
                if r.locked == Some(true) {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "Responsibility '{}' is locked and cannot be moved", mv.responsibility_id
                    ))]));
                }
                if r.vagrant == Some(true) {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "Vagrant responsibility '{}' cannot be moved", mv.responsibility_id
                    ))]));
                }
                r.clone()
            };

            if !model.nodes.iter().any(|n| n.id == mv.to_node_id) {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Destination node '{}' not found", mv.to_node_id
                ))]));
            }

            let has_code = matches!(
                resp.status,
                Some(scryer_core::Status::Implemented)
                    | Some(scryer_core::Status::Verified)
                    | Some(scryer_core::Status::Relocated)
            );

            let new_id = {
                let all_resps: Vec<&scryer_core::Responsibility> = model.nodes.iter()
                    .flat_map(|n| n.responsibilities.iter())
                    .collect();
                let max = all_resps.iter()
                    .filter_map(|r| r.id.strip_prefix("resp-").and_then(|s| s.parse::<u64>().ok()))
                    .max()
                    .unwrap_or(0);
                format!("resp-{}", max + 1)
            };

            if has_code {
                // Source: mark as relocated + locked
                let from = model.nodes.iter_mut().find(|n| n.id == mv.from_node_id).unwrap();
                if let Some(r) = from.responsibilities.iter_mut().find(|r| r.id == mv.responsibility_id) {
                    r.status = Some(scryer_core::Status::Relocated);
                    r.locked = Some(true);
                    r.relocated_to = Some(mv.to_node_id.clone());
                }
                // Destination: add relocated copy
                let dest_resp = scryer_core::Responsibility {
                    id: new_id,
                    statement: resp.statement.clone(),
                    status: Some(scryer_core::Status::Relocated),
                    vagrant: None,
                    locked: None,
                    relocated_to: None,
                    relocated_from: Some(mv.from_node_id.clone()),
                    directives: resp.directives.clone(),
                    last_touched_at: None,
                };
                let to = model.nodes.iter_mut().find(|n| n.id == mv.to_node_id).unwrap();
                to.responsibilities.push(dest_resp);
            } else {
                // Proposed: just move
                let from = model.nodes.iter_mut().find(|n| n.id == mv.from_node_id).unwrap();
                from.responsibilities.retain(|r| r.id != mv.responsibility_id);
                let dest_resp = scryer_core::Responsibility {
                    id: new_id,
                    statement: resp.statement.clone(),
                    status: resp.status,
                    vagrant: None,
                    locked: None,
                    relocated_to: None,
                    relocated_from: None,
                    directives: resp.directives.clone(),
                    last_touched_at: None,
                };
                let to = model.nodes.iter_mut().find(|n| n.id == mv.to_node_id).unwrap();
                to.responsibilities.push(dest_resp);
            }
            moved += 1;
        }

        if let Err(e) = scryer_core::write_model_at(&model_ref, &model) {
            return Ok(CallToolResult::error(vec![Content::text(e)]));
        }
        let _ = scryer_core::save_baseline_at(&model_ref, &model);

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Moved {} responsibility(ies)", moved
        ))]))
    }
}

// Helper kept here because `Kind` is used in subtree handling below.
#[allow(dead_code)]
fn _kind_check(_k: Kind) {}
