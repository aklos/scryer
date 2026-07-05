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
use scryer_core::history::{EventKind, EventRow, HistoryEvent};
use std::collections::{HashMap, HashSet};

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

/// Replace `node_id`'s whole subtree in ONE model layer with `nodes` + `links`:
/// drop every current descendant (and its links and code-side mapping), then
/// insert the payload nodes (skipping `node_id` itself) and any payload link
/// whose endpoints both exist afterward. Returns `false` — leaving the layer
/// untouched — when `node_id` isn't present, so a caller can apply the same
/// replacement to both layers and skip whichever lacks the target.
fn replace_subtree(model: &mut ScryModel, node_id: &str, nodes: &[Node], links: &[Link]) -> bool {
    if !model.nodes.iter().any(|n| n.id == node_id) {
        return false;
    }
    let mut to_remove: HashSet<String> = HashSet::new();
    let mut frontier = vec![node_id.to_string()];
    while let Some(id) = frontier.pop() {
        for child in model.nodes.iter().filter(|n| n.parent_id.as_deref() == Some(&id)) {
            if to_remove.insert(child.id.clone()) {
                frontier.push(child.id.clone());
            }
        }
    }
    prune_code_map(model, &to_remove);
    model.nodes.retain(|n| !to_remove.contains(&n.id));
    model
        .links
        .retain(|l| !to_remove.contains(&l.src) && !to_remove.contains(&l.dst));
    for n in nodes {
        if n.id == node_id {
            continue;
        }
        model.nodes.push(n.clone());
    }
    let node_ids: HashSet<&str> = model.nodes.iter().map(|n| n.id.as_str()).collect();
    for l in links {
        if node_ids.contains(l.src.as_str()) && node_ids.contains(l.dst.as_str()) {
            model.links.push(l.clone());
        }
    }
    true
}

/// Fold a set of target nodes out of ONE model layer: relocate each target's own
/// non-vagrant responsibilities (keeping their ids and source anchors) up to its
/// parent, then remove the target and its descendants along with their links,
/// boundaries, and remaining anchors. Relocating BEFORE pruning is what keeps the
/// parent's coverage of that code intact — `prune_code_map` only drops anchors of
/// responsibilities still owned by a removed node, so a relocated claim survives
/// and the file never goes dark. Code on disk is never touched.
///
/// Returns `(relocated, removed, dropped_descendant_resps)` — the last is the count
/// of responsibilities on removed DESCENDANTS that are lost (a target's own claims
/// are relocated, never dropped), surfaced so the loss is never silent.
fn fold_out_layer(model: &mut ScryModel, target_ids: &[String]) -> (usize, usize, usize) {
    let mut relocated = 0usize;

    // 1) Relocate each present target's own non-vagrant responsibilities to its parent.
    for id in target_ids {
        let Some(idx) = model.nodes.iter().position(|n| &n.id == id) else {
            continue;
        };
        let Some(parent_id) = model.nodes[idx].parent_id.clone() else {
            continue; // top-level node: no parent to carry the claims
        };
        let mut moving = Vec::new();
        model.nodes[idx].responsibilities.retain(|r| {
            if r.vagrant == Some(true) {
                true
            } else {
                moving.push(r.clone());
                false
            }
        });
        if moving.is_empty() {
            continue;
        }
        match model.nodes.iter_mut().find(|n| n.id == parent_id) {
            Some(parent) => {
                relocated += moving.len();
                parent.responsibilities.extend(moving);
            }
            // Parent absent (shouldn't happen) — restore rather than lose the claims.
            None => model.nodes[idx].responsibilities.extend(moving),
        }
    }

    // 2) Expand to the full subtree of every target.
    let mut to_remove: HashSet<String> = target_ids.iter().cloned().collect();
    let mut frontier: Vec<String> = target_ids.to_vec();
    while let Some(id) = frontier.pop() {
        for child in model.nodes.iter().filter(|n| n.parent_id.as_deref() == Some(&id)) {
            if to_remove.insert(child.id.clone()) {
                frontier.push(child.id.clone());
            }
        }
    }

    // Claims on removed descendants (not the targets themselves) are lost — count them.
    let dropped = model
        .nodes
        .iter()
        .filter(|n| to_remove.contains(&n.id) && !target_ids.iter().any(|t| t == &n.id))
        .map(|n| n.responsibilities.len())
        .sum();

    let before = model.nodes.len();
    prune_code_map(model, &to_remove);
    model.nodes.retain(|n| !to_remove.contains(&n.id));
    model
        .links
        .retain(|l| !to_remove.contains(&l.src) && !to_remove.contains(&l.dst));
    for g in model.groups.iter_mut() {
        g.member_ids.retain(|m| !to_remove.contains(m));
    }
    (relocated, before - model.nodes.len(), dropped)
}

#[tool_router(router = tool_router_nodes, vis = "pub(crate)")]
impl ScryerServer {
    #[tool(
        description = "GENERATION-PIPELINE primitive — replace the ENTIRE model in one write (used during codebase→model generation to seed the system + container skeleton). Writes both the plan and the committed model. The JSON payload must include `version: \"0.3\"`, `nodes`, `links`, and optional `groups` and `sourceMap`. Validation warnings are returned but the write is committed regardless — fix the warnings in a follow-up call. For interactive editing, use the typed add_*/update_*/move_* tools instead."
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

        // A full-state set is code→model generation: write the plan, then commit
        // it (planned and model land equal, so the plan diff is empty afterward).
        if let Err(e) = scryer_core::write_planned_at(&model_ref, &model) {
            return Ok(CallToolResult::error(vec![Content::text(e)]));
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
        description = "Patch one or more existing nodes by id. Only fields present in each item are changed. Pass `responsibilities` or `properties` to replace the whole array (pass an empty array to clear). Code-side mapping (line-precise locations per responsibility, and boundary globs per node) is written separately via `update_source_map`."
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
        let mut model = match scryer_core::read_planned_seeded_at(&model_ref) {
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
            if !model.nodes.iter().any(|n| n.id == u.node_id) {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Node '{}' not found",
                    u.node_id
                ))]));
            }

            // A reparent must not create a cycle: the new parent cannot be the
            // node itself or anywhere inside its own subtree. `move_nodes`
            // enforces this at nodes.rs:453; `update_nodes` must not be a
            // backdoor that plants a parent-chain loop (which would then hang
            // every ancestor walker in core). Validate against the model as it
            // stands (including any reparents applied earlier in this batch)
            // BEFORE mutating. The `seen` set keeps this walk terminating even
            // if the model already holds a malformed chain.
            if let Some(v) = &u.parent_id {
                let mut cur = Some(v.clone());
                let mut seen: std::collections::HashSet<String> =
                    std::collections::HashSet::new();
                while let Some(id) = cur {
                    if id == u.node_id {
                        return Ok(CallToolResult::error(vec![Content::text(format!(
                            "Cannot set parent of '{}' to '{}': that node is inside its own \
                             subtree, so the move would create a cycle",
                            u.node_id, v
                        ))]));
                    }
                    if !seen.insert(id.clone()) {
                        break;
                    }
                    cur = model
                        .nodes
                        .iter()
                        .find(|n| n.id == id)
                        .and_then(|n| n.parent_id.clone());
                }
            }

            let n = model
                .nodes
                .iter_mut()
                .find(|n| n.id == u.node_id)
                .expect("existence checked above");

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
            if let Some(v) = &u.parent_id {
                n.parent_id = Some(v.clone());
            }
            updated += 1;
        }
        enforce_readonly_directives(&mut model, &prior);

        if let Err(e) = scryer_core::write_planned_at(&model_ref, &model) {
            return Ok(CallToolResult::error(vec![Content::text(e)]));
        }

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Updated {} node(s)",
            updated
        ))]))
    }

    #[tool(
        description = "Fold a node's outstanding planned work into the committed model after you've written the code — the counterpart to `get_pending`, which closes the loop. Folding overwrites the committed claim with the clean planned copy, clearing the `stale` drift flag on anything it folds (re-implementation is the verdict that resolves it). With no `responsibilityIds`, folds every planned responsibility and property on the node, plus the appearance (the visual) — EXCEPT vagrant (code-discovered) claims and properties, which are left in the plan awaiting an explicit adopt/reject verdict and never bypass into the committed model. Pass `responsibilityIds` to fold only those responsibilities. A whole-node fold also pulls in the plan links touching this node once BOTH their endpoints are committed, and any group this node completes (every member committed) — links and groups have no id of their own to fold by, so they ride in on the node fold that makes them whole. Call this when you finish implementing, so the plan clears and the model stops reporting the work as outstanding. If you DELETED a node in the plan (intending the code to go away) and have now removed that code, call this with the node id to fold the deletion into the committed model. NOTE: this is for code you actually changed — to drop something from the model WITHOUT touching code, use `descope` instead."
    )]
    fn mark_implemented(
        &self,
        Parameters(req): Parameters<MarkImplementedRequest>,
    ) -> Result<CallToolResult, McpError> {
        use scryer_core::diff::ElementKind;
        let model_ref = resolve_model_ref(req.project.as_deref())?;
        let _lock = match lock_or_err(&model_ref) {
            Ok(l) => l,
            Err(e) => return Ok(e),
        };

        // The plan (draft) is the source of the work being closed out: marking
        // implemented FOLDS the named elements from `planned` into the committed
        // `model` via the auto-commit fold. The element must exist in the plan.
        let planned = match scryer_core::read_planned_at(&model_ref) {
            Ok(p) => p,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to read plan at {}: {}",
                    model_ref, e
                ))]));
            }
        };
        if !planned.nodes.iter().any(|n| n.id == req.node_id) {
            // Gone from the plan but still in committed = a planned DELETION to fold:
            // you removed the code, now remove the claim from the committed model.
            // `commit_element` deletes a committed node whose planned copy is absent.
            let in_committed = scryer_core::read_model_at(&model_ref)
                .map(|m| m.nodes.iter().any(|n| n.id == req.node_id))
                .unwrap_or(false);
            if in_committed {
                if let Err(e) =
                    scryer_core::commit_element(&model_ref, ElementKind::Node, None, &req.node_id)
                {
                    return Ok(CallToolResult::error(vec![Content::text(e)]));
                }
                if let Ok(after) = scryer_core::read_model_at(&model_ref) {
                    let _ = scryer_core::save_baseline_at(&model_ref, &after);
                }
                return Ok(CallToolResult::success(vec![Content::text(format!(
                    "Committed the removal of '{}' from the model.",
                    req.node_id
                ))]));
            }
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "Node '{}' not found in the plan",
                req.node_id
            ))]));
        }

        // Snapshot the node's committed responsibilities so the history event can
        // show exactly what THIS fold added or reworded, not claims committed in a
        // prior pass (a whole-node commit re-folds the node's full planned state).
        let before_stmts: HashMap<String, String> = scryer_core::read_model_at(&model_ref)
            .ok()
            .map(|m| {
                m.nodes
                    .iter()
                    .flat_map(|n| n.responsibilities.iter())
                    .map(|r| (r.id.clone(), r.statement.clone()))
                    .collect()
            })
            .unwrap_or_default();

        let summary = match &req.responsibility_ids {
            // Scoped: commit exactly the named responsibilities. Their host node
            // must already be committed (commit the whole node first otherwise).
            Some(ids) => {
                for id in ids {
                    if let Err(e) = scryer_core::commit_element(
                        &model_ref,
                        ElementKind::Responsibility,
                        None,
                        id,
                    ) {
                        return Ok(CallToolResult::error(vec![Content::text(e)]));
                    }
                }
                let n = ids.len();
                format!(
                    "Committed {} responsibilit{} on '{}' into the model.",
                    n,
                    if n == 1 { "y" } else { "ies" },
                    req.node_id
                )
            }
            // Whole node: commit the node, folding its whole planned state
            // (responsibilities, properties, appearance) into the model.
            None => {
                if let Err(e) =
                    scryer_core::commit_element(&model_ref, ElementKind::Node, None, &req.node_id)
                {
                    return Ok(CallToolResult::error(vec![Content::text(e)]));
                }
                // Pull in links/groups this node's commit just made foldable —
                // they have no node id of their own to fold by (item A).
                if let Err(e) =
                    scryer_core::commit_ready_dependents(&model_ref, &req.node_id)
                {
                    return Ok(CallToolResult::error(vec![Content::text(e)]));
                }
                format!("Committed '{}' into the model.", req.node_id)
            }
        };

        // Keep the legacy baseline snapshot in step with the committed model, and
        // record the fold as an `impl` event listing the claims it discharged.
        if let Ok(after) = scryer_core::read_model_at(&model_ref) {
            let _ = scryer_core::save_baseline_at(&model_ref, &after);
            if let Some(node) = after.nodes.iter().find(|n| n.id == req.node_id) {
                let target: Vec<&scryer_core::Responsibility> = match &req.responsibility_ids {
                    Some(ids) => node.responsibilities.iter().filter(|r| ids.contains(&r.id)).collect(),
                    // Whole-node: only the responsibilities this fold newly added or
                    // reworded relative to the committed snapshot above.
                    None => node
                        .responsibilities
                        .iter()
                        .filter(|r| before_stmts.get(&r.id) != Some(&r.statement))
                        .collect(),
                };
                let rows: Vec<EventRow> = target
                    .iter()
                    .map(|r| {
                        let marker = if before_stmts.contains_key(&r.id) { "~" } else { "+" };
                        resp_event_row(marker, &after, r)
                    })
                    .collect();
                if !rows.is_empty() {
                    record_event(
                        &model_ref,
                        HistoryEvent::new(
                            scryer_core::drift::now_secs(),
                            EventKind::Impl,
                            &req.node_id,
                            "fill",
                        )
                        .with_rows(rows),
                    );
                }
            }
        }

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
        let mut model = match scryer_core::read_planned_seeded_at(&model_ref) {
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
                    let mut seen: std::collections::HashSet<String> =
                        std::collections::HashSet::new();
                    while let Some(id) = cur {
                        if id == mv.node_id {
                            return Ok(CallToolResult::error(vec![Content::text(format!(
                                "Cannot move '{}' under its own subtree",
                                mv.node_id
                            ))]));
                        }
                        if !seen.insert(id.clone()) {
                            break; // cycle guard — a pre-existing parent loop never hangs the walk
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
        if let Err(e) = scryer_core::write_planned_at(&model_ref, &model) {
            return Ok(CallToolResult::error(vec![Content::text(e)]));
        }

        // Timeline: a `move` event per node that actually changed parent.
        let name_of = |m: &ScryModel, id: &str| {
            m.nodes.iter().find(|n| n.id == id).map(|n| n.name.clone()).unwrap_or_else(|| id.to_string())
        };
        let now = scryer_core::drift::now_secs();
        for mv in &req.moves {
            let old_parent = prior.nodes.iter().find(|n| n.id == mv.node_id).and_then(|n| n.parent_id.clone());
            if old_parent == mv.new_parent_id {
                continue;
            }
            let from = old_parent.as_deref().map(|p| name_of(&prior, p)).unwrap_or_else(|| "top level".into());
            let to = mv.new_parent_id.as_deref().map(|p| name_of(&model, p)).unwrap_or_else(|| "top level".into());
            record_event(
                &model_ref,
                HistoryEvent::new(now, EventKind::Move, &mv.node_id, "reorganize")
                    .with_rows(vec![EventRow::new("→", format!("reparented {from} → {to}"))]),
            );
        }

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
        description = "GENERATION-PIPELINE primitive — replace a node's whole subtree in one write (used during codebase→model generation to attach a container's structure to the seeded skeleton). Writes BOTH the plan and the committed model: the subtree describes code that already exists, so it lands as built, not as a pending \"implement this whole subtree\" queue in the plan diff. The data payload is JSON `{ \"nodes\": [...], \"links\": [...] }` where every node in `nodes` has a parent chain rooted at `node_id`. All existing descendants of `node_id` are removed before the new subtree is inserted. Links replace any link whose endpoints are inside the subtree; links connecting to nodes outside the subtree are also accepted. For interactive editing, use the typed add_*/update_*/move_* tools instead."
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
        let mut model = match scryer_core::read_planned_seeded_at(&model_ref) {
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

        // Apply the subtree replacement to the plan.
        replace_subtree(&mut model, &req.node_id, &payload.nodes, &payload.links);
        enforce_readonly_directives(&mut model, &prior);

        // Generation reverse-engineers code that ALREADY EXISTS, so the same
        // subtree must land in the committed model too (mirroring set_model /
        // fill_container) — otherwise the plan diff reports the whole built
        // subtree as `added` work forever. Only when `node_id` is committed (the
        // generation skeleton); if it lives only in the plan this stays a
        // plan-only edit, and there is nothing to commit. Prepared before the
        // plan write so a `None` here just means "plan-only".
        let committed = scryer_core::read_model_at(&model_ref).ok().and_then(|mut c| {
            let cprior = c.clone();
            replace_subtree(&mut c, &req.node_id, &payload.nodes, &payload.links).then(|| {
                enforce_readonly_directives(&mut c, &cprior);
                c
            })
        });

        // Write the plan first: if the committed write then fails, committed lags
        // the plan (recoverable pending work), never leads it (a phantom deletion).
        if let Err(e) = scryer_core::write_planned_at(&model_ref, &model) {
            return Ok(CallToolResult::error(vec![Content::text(e)]));
        }
        if let Some(committed) = committed {
            if let Err(e) = scryer_core::write_model_at(&model_ref, &committed) {
                return Ok(CallToolResult::error(vec![Content::text(e)]));
            }
            let _ = scryer_core::save_baseline_at(&model_ref, &committed);
        }

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
        description = "Delete one or more nodes because the CODE they model should go away — a forward modeling intent. Each node's descendants, connected links, and source-map entries are also removed. This stages real removal work in the plan: it shows up as pending until you delete the code and call `mark_implemented`. If instead the code is fine and you just shouldn't be MODELING it (an entry-point `main`, boilerplate), use `descope` — that's a code-untouched, model-only correction."
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
        let mut model = match scryer_core::read_planned_seeded_at(&model_ref) {
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

        if let Err(e) = scryer_core::write_planned_at(&model_ref, &model) {
            return Ok(CallToolResult::error(vec![Content::text(e)]));
        }

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Deleted {} node(s) (including descendants)",
            before - model.nodes.len()
        ))]))
    }

    #[tool(
        description = "Descope nodes: remove them from the MODEL because they shouldn't be modeled — the CODE is fine and stays untouched (e.g. an entry-point `main`, a trivial wrapper, generated boilerplate). Each target's own responsibilities relocate up to its parent component, keeping their source anchors, so the parent still covers that code and no darkness appears; the node and its descendants are then removed. This is a model-only correction — it writes BOTH the plan and the committed model at once, so there is NO code work to do and it never shows up in the pending work queue. Reach for this when the model over-claims relative to code reality. To instead remove the CODE itself, use `delete_nodes`, which stages real removal work."
    )]
    fn descope(
        &self,
        Parameters(req): Parameters<DescopeRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = resolve_model_ref(req.project.as_deref())?;
        let _lock = match lock_or_err(&model_ref) {
            Ok(l) => l,
            Err(e) => return Ok(e),
        };

        let mut planned = match scryer_core::read_planned_seeded_at(&model_ref) {
            Ok(m) => m,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to read plan at {}: {}",
                    model_ref, e
                ))]));
            }
        };
        let mut committed = match scryer_core::read_model_at(&model_ref) {
            Ok(m) => m,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to read model at {}: {}",
                    model_ref, e
                ))]));
            }
        };

        // Every target must exist in at least one layer.
        for id in &req.node_ids {
            let present = planned.nodes.iter().any(|n| &n.id == id)
                || committed.nodes.iter().any(|n| &n.id == id);
            if !present {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Node '{}' not found",
                    id
                ))]));
            }
        }

        // Fold the targets out of both layers identically — descope is true in both.
        // Report whichever layer actually changed: when the plan was already folded
        // (e.g. the canvas removed them first), committed is where the work lands, so
        // take the max rather than letting a clean plan report a misleading 0.
        let (rp, remp, dp) = fold_out_layer(&mut planned, &req.node_ids);
        let (rc, remc, dc) = fold_out_layer(&mut committed, &req.node_ids);
        let (relocated, removed, dropped) = (rp.max(rc), remp.max(remc), dp.max(dc));

        if let Err(e) = scryer_core::write_planned_at(&model_ref, &planned) {
            return Ok(CallToolResult::error(vec![Content::text(e)]));
        }
        if let Err(e) = scryer_core::write_model_at(&model_ref, &committed) {
            return Ok(CallToolResult::error(vec![Content::text(e)]));
        }
        let _ = scryer_core::save_baseline_at(&model_ref, &committed);

        let mut msg = format!(
            "Descoped {} node(s) from the model — code untouched. Relocated {} responsibilit{} to parent component(s).",
            removed,
            relocated,
            if relocated == 1 { "y" } else { "ies" }
        );
        if dropped > 0 {
            msg.push_str(&format!(
                " Note: {} responsibilit{} on removed descendants were dropped.",
                dropped,
                if dropped == 1 { "y" } else { "ies" }
            ));
        }
        Ok(CallToolResult::success(vec![Content::text(msg)]))
    }

    #[tool(
        description = "Move responsibilities between nodes. A responsibility keeps its id and is reparented onto the destination node; the plan diff records the move (shown as `moved`). Vagrant responsibilities cannot be moved."
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
        let mut model = match scryer_core::read_planned_seeded_at(&model_ref) {
            Ok(m) => m,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to read model at {}: {}",
                    model_ref, e
                ))]));
            }
        };

        let mut moved = 0usize;
        // (destination node id, row text) for the timeline `move` events below.
        let mut reloc_rows: Vec<(String, String)> = Vec::new();
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

            // Plain reparent: keep the same id so the plan diff matches the
            // claim by id and renders the move as `moved` (R). No ghost/locked
            // copy at the source — the diff is the record of the relocation.
            let statement = resp.statement.clone();
            let from_name = model
                .nodes
                .iter()
                .find(|n| n.id == mv.from_node_id)
                .map(|n| n.name.clone())
                .unwrap_or_else(|| mv.from_node_id.clone());
            let from = model.nodes.iter_mut().find(|n| n.id == mv.from_node_id).unwrap();
            from.responsibilities.retain(|r| r.id != mv.responsibility_id);
            let to = model.nodes.iter_mut().find(|n| n.id == mv.to_node_id).unwrap();
            to.responsibilities.push(resp);
            reloc_rows.push((
                mv.to_node_id.clone(),
                format!("relocated “{}” from {}", statement, from_name),
            ));
            moved += 1;
        }

        if let Err(e) = scryer_core::write_planned_at(&model_ref, &model) {
            return Ok(CallToolResult::error(vec![Content::text(e)]));
        }

        // Timeline: a `move` event on each destination node.
        let now = scryer_core::drift::now_secs();
        for (to_node_id, text) in reloc_rows {
            record_event(
                &model_ref,
                HistoryEvent::new(now, EventKind::Move, &to_node_id, "reorganize")
                    .with_rows(vec![EventRow::new("→", text)]),
            );
        }

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
    use scryer_core::{Appearance, ModelRef, RenderState, Responsibility};

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

    fn resp(id: &str) -> Responsibility {
        Responsibility {
            id: id.into(),
            statement: format!("does {id}"),
            vagrant: None,
            stale: None,
            stale_proposal: None,
            directives: Vec::new(),
            last_touched_at: None,
        }
    }

    /// Descope: a symbol that shouldn't be modeled is removed from BOTH layers, its
    /// responsibility relocates to the parent with its source anchor intact (so the
    /// file stays lit and no darkness appears), and the code is never consulted.
    #[test]
    fn descope_relocates_responsibility_and_writes_both_layers() {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());

        let mut m = ScryModel::new();
        m.nodes.push(node("node-1", Kind::Component, "Harness", None));
        let mut main = node("node-2", Kind::Symbol, "main", Some("node-1"));
        main.responsibilities = vec![resp("r-main")];
        m.nodes.push(main);
        m.source_map.insert(
            "r-main".into(),
            vec![scryer_core::SourceLocation {
                pattern: "examples/bench.rs".into(),
                symbol: Some("main".into()),
                line: None,
                end_line: None,
                command: None,
            }],
        );
        scryer_core::write_model_at(&model_ref, &m).unwrap();
        scryer_core::write_planned_at(&model_ref, &m).unwrap(); // plan mirrors committed

        let server = ScryerServer::new();
        server
            .descope(Parameters(DescopeRequest {
                project: Some(dir.path().to_string_lossy().to_string()),
                node_ids: vec!["node-2".into()],
            }))
            .unwrap();

        // Both layers must show the same model-only correction.
        for layer in [
            scryer_core::read_model_at(&model_ref).unwrap(),
            scryer_core::read_planned_at(&model_ref).unwrap(),
        ] {
            assert!(layer.nodes.iter().all(|n| n.id != "node-2"), "main removed");
            let parent = layer.nodes.iter().find(|n| n.id == "node-1").unwrap();
            assert!(
                parent.responsibilities.iter().any(|r| r.id == "r-main"),
                "responsibility relocated to parent"
            );
            assert_eq!(
                layer.source_map.get("r-main").unwrap()[0].pattern,
                "examples/bench.rs",
                "source anchor preserved — file stays lit"
            );
        }
    }

    /// set_node is a generation primitive describing code that ALREADY exists, so
    /// the subtree it attaches must land in BOTH layers — otherwise `get_pending`
    /// reports the whole built subtree as `added` work forever (the phantom
    /// queue). After the write, committed == planned, so the plan diff is empty.
    #[test]
    fn set_node_commits_the_generated_subtree_to_both_layers() {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());

        // Generation skeleton: a system, mirrored into both layers (as set_model
        // leaves them).
        let mut m = ScryModel::new();
        m.nodes.push(node("node-1", Kind::System, "Acme", None));
        scryer_core::write_model_at(&model_ref, &m).unwrap();
        scryer_core::write_planned_at(&model_ref, &m).unwrap();

        // Attach a container under the system.
        let payload = serde_json::json!({
            "nodes": [
                { "id": "node-2", "kind": "container", "name": "API", "parentId": "node-1",
                  "responsibilities": [{ "id": "resp-1", "statement": "serves requests" }] }
            ],
            "links": []
        });
        let server = ScryerServer::new();
        server
            .set_node(Parameters(SetNodeRequest {
                project: Some(dir.path().to_string_lossy().to_string()),
                node_id: "node-1".into(),
                data: payload.to_string(),
            }))
            .unwrap();

        let committed = scryer_core::read_model_at(&model_ref).unwrap();
        let planned = scryer_core::read_planned_at(&model_ref).unwrap();
        assert!(
            committed.nodes.iter().any(|n| n.id == "node-2"),
            "the generated container lands in the committed model, not just the plan"
        );
        assert!(planned.nodes.iter().any(|n| n.id == "node-2"), "and in the plan");
        assert!(
            scryer_core::diff::diff(&committed, &planned).is_empty(),
            "committed == planned, so the plan diff is empty — no phantom subtree queue"
        );
    }

    /// mark_implemented folds a planned DELETION: a node removed from the plan (its
    /// code now gone) is dropped from the committed model.
    #[test]
    fn mark_implemented_folds_a_planned_deletion() {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());

        let mut m = ScryModel::new();
        m.nodes.push(node("node-1", Kind::Component, "Root", None));
        m.nodes.push(node("node-2", Kind::Symbol, "gone", Some("node-1")));
        scryer_core::write_model_at(&model_ref, &m).unwrap();
        // Plan no longer has node-2 (the agent deleted it, then removed the code).
        let mut planned = m.clone();
        planned.nodes.retain(|n| n.id != "node-2");
        scryer_core::write_planned_at(&model_ref, &planned).unwrap();

        let server = ScryerServer::new();
        let r = server
            .mark_implemented(Parameters(MarkImplementedRequest {
                project: Some(dir.path().to_string_lossy().to_string()),
                node_id: "node-2".into(),
                responsibility_ids: None,
            }))
            .unwrap();
        assert!(serde_json::to_string(&r.content).unwrap().contains("removal"));

        let committed = scryer_core::read_model_at(&model_ref).unwrap();
        assert!(
            committed.nodes.iter().all(|n| n.id != "node-2"),
            "deletion folded into the committed model"
        );
    }

    /// Whole-node: marking implemented folds the node's entire planned state
    /// (responsibilities + appearance) into the committed model, and the plan
    /// for that node clears.
    #[test]
    fn mark_implemented_commits_whole_node_from_plan() {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());

        // Committed model: the node exists but is empty.
        let mut m = ScryModel::new();
        m.nodes.push(node("node-1", Kind::Component, "ModelTree", None));
        scryer_core::write_model_at(&model_ref, &m).unwrap();

        // Plan (draft): the node gains responsibilities and an appearance.
        let mut planned = m.clone();
        planned.nodes[0].responsibilities =
            vec![resp("r-a"), resp("r-b")];
        planned.nodes[0].appearance = Some(Appearance {
            status: Some(RenderState::Proposed),
            dist_path: None,
            built_at: None,
            source_hash: None,
        });
        scryer_core::write_planned_at(&model_ref, &planned).unwrap();

        let server = ScryerServer::new();
        server
            .mark_implemented(Parameters(MarkImplementedRequest {
                project: Some(dir.path().to_string_lossy().to_string()),
                node_id: "node-1".into(),
                responsibility_ids: None,
            }))
            .unwrap();

        let m = scryer_core::read_model_at(&model_ref).unwrap();
        let n = &m.nodes[0];
        assert_eq!(n.responsibilities.len(), 2, "responsibilities committed");
        assert!(n.appearance.is_some(), "appearance committed");
        assert!(
            scryer_core::plan_diff_at(&model_ref).unwrap().is_empty(),
            "plan clears after commit"
        );

        // The fold is recorded as an `impl` event listing both folded claims.
        let log = scryer_core::history::read_history(&model_ref);
        assert_eq!(log.len(), 1, "one impl event");
        assert_eq!(log[0].kind, scryer_core::history::EventKind::Impl);
        assert_eq!(log[0].node_id, "node-1");
        assert_eq!(log[0].rows.len(), 2, "both newly-folded claims listed");
    }

    /// End-to-end: `delete_nodes` stages a subtree deletion in the plan, then
    /// `mark_implemented` folds it — and committed loses the whole subtree and its
    /// dangling link, not just the target node. Item C.
    #[test]
    fn delete_nodes_then_mark_implemented_cascades_committed() {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());
        let project = Some(dir.path().to_string_lossy().to_string());

        // Committed: parent → child, plus an untouched sibling and a link into it.
        let mut m = ScryModel::new();
        m.nodes.push(node("parent-1", Kind::Container, "Parent", None));
        m.nodes.push(node("child-1", Kind::Component, "Child", Some("parent-1")));
        m.nodes.push(node("keep-1", Kind::Component, "Keep", None));
        m.links.push(Link {
            id: "l1".into(),
            src: "child-1".into(),
            dst: "keep-1".into(),
            label: "calls".into(),
            method: None,
        });
        scryer_core::write_model_at(&model_ref, &m).unwrap();
        scryer_core::write_planned_at(&model_ref, &m).unwrap();

        let server = ScryerServer::new();
        // Stage the deletion of the whole `parent-1` subtree in the plan.
        server
            .delete_nodes(Parameters(DeleteNodeRequest {
                project: project.clone(),
                node_ids: vec!["parent-1".into()],
            }))
            .unwrap();
        // Code removed → fold the deletion.
        server
            .mark_implemented(Parameters(MarkImplementedRequest {
                project: project.clone(),
                node_id: "parent-1".into(),
                responsibility_ids: None,
            }))
            .unwrap();

        let m = scryer_core::read_model_at(&model_ref).unwrap();
        assert!(
            !m.nodes.iter().any(|n| n.id == "parent-1" || n.id == "child-1"),
            "whole subtree removed from committed, not just the target"
        );
        assert!(m.nodes.iter().any(|n| n.id == "keep-1"), "sibling untouched");
        assert!(m.links.is_empty(), "link into the deleted subtree dropped");
        assert!(
            scryer_core::plan_diff_at(&model_ref).unwrap().is_empty(),
            "no pending deletions left stranded"
        );
    }

    /// A plan carrying `add_links` output closes fully through `mark_implemented`:
    /// the link has no node id to fold by, so it rides in on the fold of its
    /// second endpoint. After both nodes are folded, nothing is left pending —
    /// the CLOSE loop terminates. Item A.
    #[test]
    fn mark_implemented_folds_a_nodes_incident_links() {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());
        let project = Some(dir.path().to_string_lossy().to_string());

        // Committed: empty. Plan: two new nodes and a link between them.
        scryer_core::write_model_at(&model_ref, &ScryModel::new()).unwrap();
        let mut planned = ScryModel::new();
        planned.nodes.push(node("node-1", Kind::Component, "A", None));
        planned.nodes.push(node("node-2", Kind::Component, "B", None));
        planned.links.push(Link {
            id: "l1".into(),
            src: "node-1".into(),
            dst: "node-2".into(),
            label: "calls".into(),
            method: None,
        });
        scryer_core::write_planned_at(&model_ref, &planned).unwrap();

        let server = ScryerServer::new();
        let mark = |id: &str| {
            server
                .mark_implemented(Parameters(MarkImplementedRequest {
                    project: project.clone(),
                    node_id: id.into(),
                    responsibility_ids: None,
                }))
                .unwrap();
        };

        // Folding the first node leaves the link pending — its far end isn't built.
        mark("node-1");
        assert!(
            !scryer_core::read_model_at(&model_ref).unwrap().links.iter().any(|l| l.id == "l1"),
            "link waits for its second endpoint"
        );

        // Folding the second node pulls the link in; the plan diff reaches empty.
        mark("node-2");
        assert!(
            scryer_core::read_model_at(&model_ref).unwrap().links.iter().any(|l| l.id == "l1"),
            "link folded with its second endpoint"
        );
        assert!(
            scryer_core::plan_diff_at(&model_ref).unwrap().is_empty(),
            "no pending work remains — CLOSE terminates"
        );
    }

    /// Scoped: marking a single newly-planned responsibility folds just it onto
    /// its (already-committed) host node, leaving the rest of the plan untouched.
    #[test]
    fn mark_implemented_commits_named_responsibility_from_plan() {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());

        // Committed model: node-1 already owns r-a.
        let mut m = ScryModel::new();
        let mut c = node("node-1", Kind::Component, "Billing", None);
        c.responsibilities = vec![resp("r-a")];
        m.nodes.push(c);
        scryer_core::write_model_at(&model_ref, &m).unwrap();

        // Plan: a new r-b is proposed on the same node; r-c is proposed too but
        // not named in the call, so it must stay in the plan.
        let mut planned = m.clone();
        planned.nodes[0]
            .responsibilities
            .push(resp("r-b"));
        planned.nodes[0]
            .responsibilities
            .push(resp("r-c"));
        scryer_core::write_planned_at(&model_ref, &planned).unwrap();

        let server = ScryerServer::new();
        server
            .mark_implemented(Parameters(MarkImplementedRequest {
                project: Some(dir.path().to_string_lossy().to_string()),
                node_id: "node-1".into(),
                responsibility_ids: Some(vec!["r-b".into()]),
            }))
            .unwrap();

        let m = scryer_core::read_model_at(&model_ref).unwrap();
        let n = &m.nodes[0];
        assert!(n.responsibilities.iter().any(|r| r.id == "r-b"), "r-b committed");
        assert!(!n.responsibilities.iter().any(|r| r.id == "r-c"), "r-c left uncommitted");

        // Only r-c remains as a pending plan entry (Added).
        let plan = scryer_core::plan_diff_at(&model_ref).unwrap();
        assert_eq!(plan.changes.len(), 1);
        assert_eq!(plan.changes[0].id, "r-c");
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
        // move_nodes authors into the plan; assert on the planned (draft) model.
        let m = scryer_core::read_planned_at(&model_ref).unwrap();
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

    /// update_nodes reparents too, and must reject a cycle-creating parent the
    /// same way move_nodes does (audit #6) — otherwise it's a backdoor that
    /// plants a parent-chain loop and hangs every ancestor walker in core. The
    /// rejected write must leave the plan untouched.
    #[test]
    fn update_nodes_rejects_a_cycle_creating_reparent() {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());
        let mut m = ScryModel::new();
        m.nodes.push(node("sys", Kind::System, "Sys", None));
        m.nodes.push(node("ca", Kind::Container, "A", Some("sys")));
        m.nodes.push(node("comp", Kind::Component, "Comp", Some("ca")));
        scryer_core::write_planned_at(&model_ref, &m).unwrap();
        let server = ScryerServer::new();
        let project = dir.path().to_string_lossy().to_string();

        let reparent = |node_id: &str, parent: &str| UpdateNodeItem {
            node_id: node_id.into(),
            parent_id: Some(parent.into()),
            kind: None,
            name: None,
            description: None,
            technology: None,
            external: None,
            responsibilities: None,
            properties: None,
            visual: None,
        };

        // Re-parent the System under its own grandchild: a cycle.
        let r = server
            .update_nodes(Parameters(UpdateNodeRequest {
                project: Some(project.clone()),
                nodes: vec![reparent("sys", "comp")],
            }))
            .unwrap();
        assert!(r.is_error.unwrap_or(false), "cycle reparent rejected");

        // Rejected wholesale: the plan is untouched, no loop persisted.
        let after = scryer_core::read_planned_at(&model_ref).unwrap();
        let sys = after.nodes.iter().find(|n| n.id == "sys").unwrap();
        assert_eq!(sys.parent_id, None, "parent unchanged after rejection");

        // A node set as its own parent is likewise rejected.
        let r = server
            .update_nodes(Parameters(UpdateNodeRequest {
                project: Some(project),
                nodes: vec![reparent("ca", "ca")],
            }))
            .unwrap();
        assert!(r.is_error.unwrap_or(false), "self-parent rejected");
    }
}
