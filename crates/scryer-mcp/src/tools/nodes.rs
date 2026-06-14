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
        } else {
            enforce_readonly_directives(&mut model, &ScryModel::default());
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
                icon: None,
                visual: None,
                appearance: None,
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
            if let Some(v) = u.visual {
                n.visual = if v { Some(true) } else { None };
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
        scryer_core::rewrite_renamed_wikilinks(&mut model, &prior);

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
        description = "Mark a node's outstanding work as implemented after you've written the code — the counterpart to `get_unimplemented`, which closes the loop. Advances `proposed`/`changed` items to `implemented` and clears the `stale` drift flag on anything it advances (re-implementation is the verdict that resolves it). With no `responsibilityIds`, advances EVERYTHING outstanding on the node: every proposed/changed/stale responsibility and every proposed/changed property, plus a proposed/changed appearance (the visual). Pass `responsibilityIds` to advance only those responsibilities. Leaves clean `implemented`/`verified` items untouched (advancing to `verified` is a separate, checked step). Call this when you finish implementing, so the model stops reporting the work as outstanding."
    )]
    fn mark_implemented(
        &self,
        Parameters(req): Parameters<MarkImplementedRequest>,
    ) -> Result<CallToolResult, McpError> {
        use scryer_core::Status;
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

        let Some(n) = model.nodes.iter_mut().find(|n| n.id == req.node_id) else {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "Node '{}' not found",
                req.node_id
            ))]));
        };

        let outstanding = |s: &Option<Status>| {
            matches!(s, Some(Status::Proposed) | Some(Status::Changed))
        };
        let mut resp_done = 0usize;
        let mut prop_done = 0usize;
        let mut appearance_done = false;

        match &req.responsibility_ids {
            // Scoped: advance exactly the named responsibilities (trust the agent).
            Some(ids) => {
                for r in n.responsibilities.iter_mut() {
                    if ids.contains(&r.id) {
                        r.status = Some(Status::Implemented);
                        r.stale = None; // re-implementation resolves the drift flag
                        resp_done += 1;
                    }
                }
            }
            // Whole node: advance every outstanding facet to implemented. A
            // stale flag IS outstanding work (the claim needs re-discharging),
            // whatever status it sits on.
            None => {
                for r in n.responsibilities.iter_mut() {
                    if outstanding(&r.status) || r.stale == Some(true) {
                        r.status = Some(Status::Implemented);
                        r.stale = None;
                        resp_done += 1;
                    }
                }
                for p in n.properties.iter_mut() {
                    if outstanding(&p.status) {
                        p.status = Some(Status::Implemented);
                        prop_done += 1;
                    }
                }
                if let Some(a) = n.appearance.as_mut() {
                    if outstanding(&a.status) {
                        a.status = Some(Status::Implemented);
                        appearance_done = true;
                    }
                }
            }
        }

        enforce_readonly_directives(&mut model, &prior);
        if let Err(e) = scryer_core::write_model_at(&model_ref, &model) {
            return Ok(CallToolResult::error(vec![Content::text(e)]));
        }
        let _ = scryer_core::save_baseline_at(&model_ref, &model);

        let mut parts = Vec::new();
        if resp_done > 0 {
            parts.push(format!("{} responsibilit{}", resp_done, if resp_done == 1 { "y" } else { "ies" }));
        }
        if prop_done > 0 {
            parts.push(format!("{} propert{}", prop_done, if prop_done == 1 { "y" } else { "ies" }));
        }
        if appearance_done {
            parts.push("the appearance".to_string());
        }
        let summary = if parts.is_empty() {
            format!("Nothing outstanding on '{}' — model unchanged.", req.node_id)
        } else {
            format!("Marked implemented on '{}': {}.", req.node_id, parts.join(", "))
        };
        Ok(CallToolResult::success(vec![Content::text(summary)]))
    }

    #[tool(
        description = "Re-parent nodes — move a node (and its whole subtree) to a different parent, e.g. a component to another container. Validated: the new parent must satisfy the kind hierarchy (system→container→component→symbol; omit newParentId only for top-level systems/persons), must not be external, and must not sit inside the moved node's own subtree. The node leaves any group at its old level (groups organize siblings). Links to former siblings may become invalid — run `validate_model` after structural moves."
    )]
    fn move_nodes(
        &self,
        Parameters(req): Parameters<MoveNodesRequest>,
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

        let mut moved = 0usize;
        for mv in &req.moves {
            let Some(node) = model.nodes.iter().find(|n| n.id == mv.node_id) else {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Node '{}' not found",
                    mv.node_id
                ))]));
            };
            if node.locked == Some(true) {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Node '{}' is locked and cannot be moved",
                    mv.node_id
                ))]));
            }
            let kind = node.kind;

            match mv.new_parent_id.as_deref() {
                None => {
                    if !matches!(kind, Kind::System | Kind::Person) {
                        return Ok(CallToolResult::error(vec![Content::text(format!(
                            "Node '{}' ({:?}) cannot be top-level — only person/system are",
                            mv.node_id, kind
                        ))]));
                    }
                }
                Some(pid) => {
                    let Some(parent) = model.nodes.iter().find(|n| n.id == pid) else {
                        return Ok(CallToolResult::error(vec![Content::text(format!(
                            "New parent '{}' not found",
                            pid
                        ))]));
                    };
                    let valid = matches!(
                        (parent.kind, kind),
                        (Kind::System, Kind::Container)
                            | (Kind::Container, Kind::Component)
                            | (Kind::Component, Kind::Symbol)
                    );
                    if !valid {
                        return Ok(CallToolResult::error(vec![Content::text(format!(
                            "A {:?} cannot be parented by a {:?}",
                            kind, parent.kind
                        ))]));
                    }
                    if parent.external == Some(true) {
                        return Ok(CallToolResult::error(vec![Content::text(format!(
                            "External node '{}' cannot have children",
                            pid
                        ))]));
                    }
                    // The new parent must not be the node itself or inside its
                    // own subtree (that would orphan the chain into a cycle).
                    let mut cur = Some(pid.to_string());
                    while let Some(id) = cur {
                        if id == mv.node_id {
                            return Ok(CallToolResult::error(vec![Content::text(format!(
                                "Cannot move '{}' under its own subtree",
                                mv.node_id
                            ))]));
                        }
                        cur = model
                            .nodes
                            .iter()
                            .find(|n| n.id == id)
                            .and_then(|n| n.parent_id.clone());
                    }
                }
            }

            let node = model.nodes.iter_mut().find(|n| n.id == mv.node_id).unwrap();
            node.parent_id = mv.new_parent_id.clone();
            // Groups organize siblings at one level — leaving the level leaves
            // the group.
            for g in model.groups.iter_mut() {
                g.member_ids.retain(|m| m != &mv.node_id);
            }
            moved += 1;
        }

        enforce_readonly_directives(&mut model, &prior);
        if let Err(e) = scryer_core::write_model_at(&model_ref, &model) {
            return Ok(CallToolResult::error(vec![Content::text(e)]));
        }
        let _ = scryer_core::save_baseline_at(&model_ref, &model);
        let warnings = validate::validate(&model);
        let mut msg = format!("Moved {moved} node(s).");
        if !warnings.is_empty() {
            msg.push_str(&format!(
                " {} validation warning(s) — run validate_model:",
                warnings.len()
            ));
            for w in warnings.iter().take(5) {
                msg.push_str(&format!("\n- {}", w));
            }
        }
        Ok(CallToolResult::success(vec![Content::text(msg)]))
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
        scryer_core::rewrite_renamed_wikilinks(&mut model, &prior);

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

            // Relocated copies keep their lifecycle status (relocation is the
            // relocated_to/relocated_from flag pair), so status alone tells us
            // whether code backs the claim.
            let has_code = matches!(
                resp.status,
                Some(scryer_core::Status::Implemented) | Some(scryer_core::Status::Verified)
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
                // Relocation is a FLAG pair, never a status: the source becomes
                // a locked ghost pointing forward; the claim's lifecycle is
                // untouched by the move.
                let from = model.nodes.iter_mut().find(|n| n.id == mv.from_node_id).unwrap();
                if let Some(r) = from.responsibilities.iter_mut().find(|r| r.id == mv.responsibility_id) {
                    r.locked = Some(true);
                    r.relocated_to = Some(mv.to_node_id.clone());
                }
                // Destination: live copy pointing back, status carried through.
                let dest_resp = scryer_core::Responsibility {
                    id: new_id,
                    statement: resp.statement.clone(),
                    status: resp.status,
                    vagrant: None,
                    stale: None,
                    locked: None,
                    relocated_to: None,
                    relocated_from: Some(mv.from_node_id.clone()),
                    directives: resp.directives.clone(),
                    last_touched_at: None,
                    changed_from: None,
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
                    stale: None,
                    locked: None,
                    relocated_to: None,
                    relocated_from: None,
                    directives: resp.directives.clone(),
                    last_touched_at: None,
                    changed_from: None,
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

#[cfg(test)]
mod tests {
    use super::*;
    use scryer_core::{Appearance, ModelRef, Responsibility, Status};

    fn node(id: &str, kind: Kind, name: &str, parent: Option<&str>) -> Node {
        Node {
            id: id.into(),
            kind,
            name: name.into(),
            parent_id: parent.map(|p| p.into()),
            external: None,
            technology: None,
            description: None,
            responsibilities: Vec::new(),
            properties: Vec::new(),
            icon: None,
            visual: None,
            appearance: None,
            deprecated: None,
            relocated: None,
            locked: None,
            relocated_to: None,
            relocated_from: None,
        }
    }

    fn resp(id: &str, status: Status) -> Responsibility {
        Responsibility {
            id: id.into(),
            statement: format!("does {id}"),
            status: Some(status),
            vagrant: None,
            stale: None,
            locked: None,
            relocated_to: None,
            relocated_from: None,
            directives: Vec::new(),
            last_touched_at: None,
            changed_from: None,
        }
    }

    #[test]
    fn mark_implemented_advances_outstanding_incl_appearance() {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());
        let mut m = ScryModel::new();
        let mut c = node("node-1", Kind::Component, "ModelTree", None);
        c.responsibilities = vec![
            resp("r-prop", Status::Proposed),
            resp("r-chg", Status::Changed),
            resp("r-impl", Status::Implemented),
            resp("r-ver", Status::Verified),
        ];
        c.appearance = Some(Appearance {
            status: Some(Status::Changed),
            dist_path: None,
            built_at: None,
            source_hash: None,
        });
        m.nodes.push(c);
        scryer_core::write_model_at(&model_ref, &m).unwrap();
        let server = ScryerServer::new();
        let project = dir.path().to_string_lossy().to_string();

        // Whole-node: every outstanding facet advances; implemented/verified untouched.
        server
            .mark_implemented(Parameters(MarkImplementedRequest {
                project: Some(project),
                node_id: "node-1".into(),
                responsibility_ids: None,
            }))
            .unwrap();

        let m = scryer_core::read_model_at(&model_ref).unwrap();
        let n = &m.nodes[0];
        let st = |id: &str| n.responsibilities.iter().find(|r| r.id == id).unwrap().status;
        assert_eq!(st("r-prop"), Some(Status::Implemented)); // proposed -> implemented
        assert_eq!(st("r-chg"), Some(Status::Implemented)); // changed -> implemented
        assert_eq!(st("r-impl"), Some(Status::Implemented)); // unchanged
        assert_eq!(st("r-ver"), Some(Status::Verified)); // NOT downgraded
        assert_eq!(n.appearance.as_ref().unwrap().status, Some(Status::Implemented));
    }

    #[test]
    fn mark_implemented_scoped_to_named_responsibilities() {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());
        let mut m = ScryModel::new();
        let mut c = node("node-1", Kind::Component, "Billing", None);
        c.responsibilities = vec![resp("r-a", Status::Proposed), resp("r-b", Status::Proposed)];
        c.appearance = Some(Appearance {
            status: Some(Status::Changed),
            dist_path: None,
            built_at: None,
            source_hash: None,
        });
        m.nodes.push(c);
        scryer_core::write_model_at(&model_ref, &m).unwrap();
        let server = ScryerServer::new();

        server
            .mark_implemented(Parameters(MarkImplementedRequest {
                project: Some(dir.path().to_string_lossy().to_string()),
                node_id: "node-1".into(),
                responsibility_ids: Some(vec!["r-a".into()]),
            }))
            .unwrap();

        let m = scryer_core::read_model_at(&model_ref).unwrap();
        let n = &m.nodes[0];
        let st = |id: &str| n.responsibilities.iter().find(|r| r.id == id).unwrap().status;
        assert_eq!(st("r-a"), Some(Status::Implemented)); // named -> advanced
        assert_eq!(st("r-b"), Some(Status::Proposed)); // not named -> left
        // scoped call leaves appearance alone
        assert_eq!(n.appearance.as_ref().unwrap().status, Some(Status::Changed));
    }

    /// move_nodes re-parents with kind/cycle validation and pulls the node out
    /// of its old-level group.
    #[test]
    fn move_nodes_validates_and_leaves_group() {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());
        let mut m = ScryModel::new();
        m.nodes.push(node("sys", Kind::System, "Sys", None));
        m.nodes.push(node("ca", Kind::Container, "A", Some("sys")));
        m.nodes.push(node("cb", Kind::Container, "B", Some("sys")));
        m.nodes.push(node("comp", Kind::Component, "Comp", Some("ca")));
        m.nodes.push(node("sym", Kind::Symbol, "sym", Some("comp")));
        m.groups.push(scryer_core::Group {
            id: "g1".into(),
            name: "Edge".into(),
            description: None,
            member_ids: vec!["comp".into()],
            parent_group_id: None,
            parent_node_id: Some("ca".into()),
            responsibilities: Vec::new(),
            icon: None,
        });
        scryer_core::write_model_at(&model_ref, &m).unwrap();
        let server = ScryerServer::new();
        let project = dir.path().to_string_lossy().to_string();

        // Valid: component A→B. The subtree (sym) follows; the group lets go.
        let r = server
            .move_nodes(Parameters(MoveNodesRequest {
                project: Some(project.clone()),
                moves: vec![NodeMove { node_id: "comp".into(), new_parent_id: Some("cb".into()) }],
            }))
            .unwrap();
        assert!(!r.is_error.unwrap_or(false), "{r:?}");
        let m = scryer_core::read_model_at(&model_ref).unwrap();
        let comp = m.nodes.iter().find(|n| n.id == "comp").unwrap();
        assert_eq!(comp.parent_id.as_deref(), Some("cb"));
        let sym = m.nodes.iter().find(|n| n.id == "sym").unwrap();
        assert_eq!(sym.parent_id.as_deref(), Some("comp"), "subtree intact");
        assert!(m.groups[0].member_ids.is_empty(), "left the old-level group");

        // Invalid kind pair: component under system.
        let r = server
            .move_nodes(Parameters(MoveNodesRequest {
                project: Some(project.clone()),
                moves: vec![NodeMove { node_id: "comp".into(), new_parent_id: Some("sys".into()) }],
            }))
            .unwrap();
        assert!(r.is_error.unwrap_or(false), "kind pair rejected");

        // Cycle: container under a symbol inside its own subtree.
        let r = server
            .move_nodes(Parameters(MoveNodesRequest {
                project: Some(project),
                moves: vec![NodeMove { node_id: "cb".into(), new_parent_id: Some("sym".into()) }],
            }))
            .unwrap();
        assert!(r.is_error.unwrap_or(false), "cycle rejected");
    }
}
