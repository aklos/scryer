use crate::helpers::*;
use crate::server::ScryerServer;
use crate::types::*;
use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, Content},
    tool, tool_router, ErrorData as McpError,
};
use scryer_core::{C4Kind, Flow, Group};
use std::collections::HashSet;

#[tool_router(router = tool_router_misc, vis = "pub(crate)")]
impl ScryerServer {
    #[tool(
        description = "Set source file locations for one or more nodes or flows. Used to map operation nodes to their source code and to link flows to test files. Pass an empty locations array to clear a node's source map. Each location has a required `pattern` (file glob) and optional `line`/`endLine`. When linking a flow to a test, use the flow ID as the `node_id`."
    )]
    fn update_source_map(
        &self,
        Parameters(req): Parameters<UpdateSourceMapRequest>,
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

        for entry in &req.entries {
            let is_node = model.nodes.iter().any(|n| n.id == entry.node_id);
            let is_flow = model.flows.iter().any(|f| f.id == entry.node_id);
            if !is_node && !is_flow {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Node or flow '{}' not found",
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

        match scryer_core::write_model_at(&model_ref, &model) {
            Ok(()) => {
                let _ = scryer_core::save_baseline_at(&model_ref, &model);
                Ok(CallToolResult::success(vec![Content::text(format!(
                    "Updated source map for {} node(s)",
                    count
                ))]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e)])),
        }
    }

    #[tool(
        description = "Create or replace one or more flows. Pass a single flow object or an array of flows — use an array to create multiple flows in one call. If a flow with the given ID exists, it is replaced; otherwise it is appended.\n\nFlows describe behavioral sequences — user journeys, data syncs, deploy pipelines, cron jobs. Each flow has an ordered list of steps.\n\nStep granularity: each step = one meaningful system interaction, NOT a UI gesture. Good: 'System validates credentials'. Bad: 'User clicks button'.\n\nStep schema: {id, description, branches?}. Use `description` for step text — numbering is auto-computed. Step IDs: 'step-N'. Flow IDs: 'scenario-N'.\n\nBranching: steps can have a `branches` array of {condition, steps[]} objects to model decision points. Each branch has a condition label (e.g. \"if: valid\", \"else:\") and its own ordered list of sub-steps. Branches can nest recursively.\n\nTo reference architecture nodes in step descriptions, use @[Name] mentions (e.g. \"@[AuthService] validates the JWT token\").\n\nFlows are integration test specs. Each flow describes what should happen end-to-end. Use `update_source_map` to link a flow to its test file.\n\nOld format (flat transitions array) is still accepted for backward compatibility but transitions are ignored — use step ordering and branches instead."
    )]
    fn set_flows(
        &self,
        Parameters(req): Parameters<SetFlowRequest>,
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

        // Parse as single flow or array of flows
        let flows: Vec<Flow> = match serde_json::from_str::<Vec<Flow>>(&req.data) {
            Ok(arr) => arr,
            Err(_) => match serde_json::from_str::<Flow>(&req.data) {
                Ok(s) => vec![s],
                Err(e) => {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "Invalid flow JSON: {}",
                        e
                    ))]));
                }
            },
        };

        if flows.is_empty() {
            return Ok(CallToolResult::error(vec![Content::text(
                "Empty flow array",
            )]));
        }

        for flow in &flows {
            // Validate step ID uniqueness (recursive)
            let all_ids = scryer_core::collect_step_ids(&flow.steps);
            let mut step_ids = HashSet::new();
            for id in &all_ids {
                if !step_ids.insert(*id) {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "Duplicate step ID '{}' in flow '{}'",
                        id, flow.name
                    ))]));
                }
            }

            // Migrate: if a step has label but no description, move label → description
            // (AI agents often use "label" for step text, but the UI renders "description")
            let mut flow = flow.clone();
            migrate_flow_labels(&mut flow.steps);

            // Replace or append
            if let Some(existing) = model.flows.iter_mut().find(|s| s.id == flow.id) {
                *existing = flow;
            } else {
                model.flows.push(flow);
            }
        }

        match scryer_core::write_model_at(&model_ref, &model) {
            Ok(()) => {
                let _ = scryer_core::save_baseline_at(&model_ref, &model);
                let summary: Vec<String> = flows
                    .iter()
                    .map(|s| format!("'{}' ({} steps)", s.name, s.steps.len()))
                    .collect();
                Ok(CallToolResult::success(vec![Content::text(format!(
                    "Set {} flow(s): {}",
                    flows.len(),
                    summary.join(", ")
                ))]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e)])),
        }
    }

    #[tool(description = "Delete a flow by ID")]
    fn delete_flow(
        &self,
        Parameters(req): Parameters<DeleteFlowRequest>,
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

        let before = model.flows.len();
        model.flows.retain(|s| s.id != req.flow_id);
        if model.flows.len() == before {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "Flow '{}' not found",
                req.flow_id
            ))]));
        }

        match scryer_core::write_model_at(&model_ref, &model) {
            Ok(()) => {
                let _ = scryer_core::save_baseline_at(&model_ref, &model);
                Ok(CallToolResult::success(vec![Content::text(format!(
                    "Deleted flow '{}'",
                    req.flow_id
                ))]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e)])),
        }
    }

    #[tool(
        description = "Create or replace one or more groups. Groups organize nodes that share something beyond topology — common uses: a deployment unit (containers that ship together), a package/module (components in the same folder/bundle), or an ownership boundary. If a group with the given ID exists, it is replaced; otherwise it is appended.\n\nGroups can be nested via `parentGroupId`. Parent and child must contain members at the same C4 level.\n\nRules:\n- A node belongs to at most one group.\n- All `memberIds` in a group must refer to nodes at the same C4 level (containers OR components — other levels not supported).\n- `parentGroupId`, if set, must reference an existing group whose members are at the same C4 level; parent chains must not cycle.\n\nWrite the intent (e.g. \"deploys to Fly.io\", \"bundled as cms module\") into the group's `name` and `description` — that's what agents read.\n\nGroup schema: {id, name, memberIds, description?, parentGroupId?, contract?}."
    )]
    fn set_groups(
        &self,
        Parameters(req): Parameters<SetGroupsRequest>,
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

        let node_ids: HashSet<&str> = model.nodes.iter().map(|n| n.id.as_str()).collect();
        let node_kind = |id: &str| model.nodes.iter().find(|n| n.id == id).map(|n| n.data.kind);

        // Determine the effective C4 level of a group by scanning its members.
        // Only containers and components may be grouped. All members in a group
        // must share the same level.
        let group_level = |g: &Group| -> Result<Option<C4Kind>, String> {
            let mut level: Option<C4Kind> = None;
            for mid in &g.member_ids {
                let k = node_kind(mid).ok_or_else(|| {
                    format!("Member '{}' in group '{}' not found in model", mid, g.name)
                })?;
                if !matches!(k, C4Kind::Container | C4Kind::Component) {
                    return Err(format!(
                        "Group '{}' contains a {:?} node; only containers and components can be grouped.",
                        g.name, k
                    ));
                }
                match level {
                    None => level = Some(k),
                    Some(l) if l != k => {
                        return Err(format!(
                            "Group '{}' mixes {:?} and {:?} members; all members must be at the same C4 level.",
                            g.name, l, k
                        ));
                    }
                    _ => {}
                }
            }
            Ok(level)
        };

        // Stage the post-update group list so we can validate nesting holistically.
        let mut staged: Vec<Group> = model.groups.clone();
        for group in &groups {
            if let Some(existing) = staged.iter_mut().find(|g| g.id == group.id) {
                *existing = group.clone();
            } else {
                staged.push(group.clone());
            }
        }

        for group in &groups {
            // Validate member IDs exist
            for mid in &group.member_ids {
                if !node_ids.contains(mid.as_str()) {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "Member '{}' in group '{}' not found in model",
                        mid, group.name
                    ))]));
                }
            }

            // Same-level invariant (and containers/components only).
            let child_level = match group_level(group) {
                Ok(l) => l,
                Err(msg) => return Ok(CallToolResult::error(vec![Content::text(msg)])),
            };

            // Nesting: parent must exist, must share level, no cycles.
            if let Some(parent_id) = &group.parent_group_id {
                if parent_id == &group.id {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "Group '{}' cannot be its own parent.",
                        group.name
                    ))]));
                }
                let mut seen: HashSet<&str> = HashSet::new();
                seen.insert(group.id.as_str());
                let mut cursor: Option<&str> = Some(parent_id.as_str());
                while let Some(cid) = cursor {
                    if !seen.insert(cid) {
                        return Ok(CallToolResult::error(vec![Content::text(format!(
                            "Group '{}' introduces a cycle through parent '{}'.",
                            group.name, parent_id
                        ))]));
                    }
                    let parent = match staged.iter().find(|g| g.id == cid) {
                        Some(p) => p,
                        None => {
                            return Ok(CallToolResult::error(vec![Content::text(format!(
                                "Group '{}' references unknown parent '{}'.",
                                group.name, cid
                            ))]));
                        }
                    };
                    let parent_level = match group_level(parent) {
                        Ok(l) => l,
                        Err(msg) => return Ok(CallToolResult::error(vec![Content::text(msg)])),
                    };
                    if let (Some(c), Some(p)) = (child_level, parent_level) {
                        if c != p {
                            return Ok(CallToolResult::error(vec![Content::text(format!(
                                "Group '{}' ({:?}) cannot nest under '{}' ({:?}) — parent and child must share the same C4 level.",
                                group.name, c, parent.name, p
                            ))]));
                        }
                    }
                    cursor = parent.parent_group_id.as_deref();
                }
            }

            // Replace or append in the real model list.
            if let Some(existing) = model.groups.iter_mut().find(|g| g.id == group.id) {
                *existing = group.clone();
            } else {
                model.groups.push(group.clone());
            }
        }

        // Enforce exclusive membership: remove members from other groups
        for group in &groups {
            let member_set: HashSet<&str> = group.member_ids.iter().map(|s| s.as_str()).collect();
            for other in model.groups.iter_mut() {
                if other.id != group.id {
                    other
                        .member_ids
                        .retain(|mid| !member_set.contains(mid.as_str()));
                }
            }
        }
        // Remove empty groups
        model.groups.retain(|g| !g.member_ids.is_empty());

        let count = groups.len();
        let names: Vec<&str> = groups.iter().map(|g| g.name.as_str()).collect();
        match scryer_core::write_model_at(&model_ref, &model) {
            Ok(()) => {
                let _ = scryer_core::save_baseline_at(&model_ref, &model);
                Ok(CallToolResult::success(vec![Content::text(format!(
                    "Set {} group(s): {}",
                    count,
                    names.join(", ")
                ))]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e)])),
        }
    }

    #[tool(
        description = "Suppress or resume drift detection for a model. Call with active=true before starting implementation from a model to prevent the UI from showing drift notifications while you are writing code. Call with active=false when implementation is complete."
    )]
    fn set_implementing(
        &self,
        Parameters(req): Parameters<SetImplementingRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = match self.resolve_model(req.model) {
            Ok(r) => r,
            Err(e) => return Ok(e),
        };
        scryer_core::set_implementing_at(&model_ref, req.active)
            .map_err(|e| McpError::internal_error(e, None))?;
        let msg = if req.active {
            format!("Drift detection suppressed for '{}'", model_ref)
        } else {
            format!("Drift detection resumed for '{}'", model_ref)
        };
        Ok(CallToolResult::success(vec![Content::text(msg)]))
    }

    #[tool(description = "Delete a group by ID. Members are ungrouped, not deleted.")]
    fn delete_group(
        &self,
        Parameters(req): Parameters<DeleteGroupRequest>,
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

        let before = model.groups.len();
        model.groups.retain(|g| g.id != req.group_id);
        if model.groups.len() == before {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "Group '{}' not found",
                req.group_id
            ))]));
        }

        match scryer_core::write_model_at(&model_ref, &model) {
            Ok(()) => {
                let _ = scryer_core::save_baseline_at(&model_ref, &model);
                Ok(CallToolResult::success(vec![Content::text(format!(
                    "Deleted group '{}'",
                    req.group_id
                ))]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e)])),
        }
    }
}
