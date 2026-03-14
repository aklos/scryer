use crate::helpers::*;
use crate::instructions::TASK_INSTRUCTIONS;
use crate::server::ScryerServer;
use crate::types::*;
use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, Content},
    tool, tool_router, ErrorData as McpError,
};
use scryer_core::{C4Kind, C4Node, Contract, Status};

#[tool_router(router = tool_router_task, vis = "pub(crate)")]
impl ScryerServer {
    #[tool(
        description = "Get the next implementation task. Returns one logical work unit at a time, ordered by dependencies. Workflow: call get_task → build the returned task → mark nodes as wip via update_nodes (with a reason) → call get_task again for the next task. Pass node_id to scope to a subtree."
    )]
    fn get_task(
        &self,
        Parameters(req): Parameters<GetTaskRequest>,
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

        let scope_filter: Option<&str> = req.node_id.as_deref();

        // Helper: check if node_id is a descendant of ancestor_id
        let is_descendant_of = |node_id: &str, ancestor_id: &str| -> bool {
            let mut cur = node_id.to_string();
            loop {
                let parent = model
                    .nodes
                    .iter()
                    .find(|n| n.id == cur)
                    .and_then(|n| n.parent_id.clone());
                match parent {
                    Some(pid) if pid == ancestor_id => return true,
                    Some(pid) => cur = pid,
                    None => return false,
                }
            }
        };

        // Helper: get ancestor chain from node up to root (excluding the node itself)
        let get_ancestor_chain = |node_id: &str| -> Vec<&C4Node> {
            let mut chain = Vec::new();
            let mut cur = node_id.to_string();
            loop {
                let parent = model
                    .nodes
                    .iter()
                    .find(|n| n.id == cur)
                    .and_then(|n| n.parent_id.clone());
                match parent {
                    Some(pid) => {
                        if let Some(pnode) = model.nodes.iter().find(|n| n.id == pid) {
                            chain.push(pnode);
                            cur = pid;
                        } else {
                            break;
                        }
                    }
                    None => break,
                }
            }
            chain.reverse();
            chain
        };

        // Helper: merge contract (additive — ancestors + node)
        let merge_contract = |chain: &[&C4Node], node: &C4Node| -> Contract {
            let mut merged = Contract::default();
            for ancestor in chain {
                merged.expect.extend(ancestor.data.contract.expect.iter().cloned());
                merged.ask.extend(ancestor.data.contract.ask.iter().cloned());
                merged.never.extend(ancestor.data.contract.never.iter().cloned());
            }
            merged.expect.extend(node.data.contract.expect.iter().cloned());
            merged.ask.extend(node.data.contract.ask.iter().cloned());
            merged.never.extend(node.data.contract.never.iter().cloned());
            merged
        };

        // Helper: collect notes from ancestors + node
        let collect_notes = |chain: &[&C4Node], node: &C4Node| -> Vec<String> {
            let mut collected = Vec::new();
            for ancestor in chain {
                if !ancestor.data.notes.is_empty() {
                    for n in &ancestor.data.notes {
                        collected.push(format!("{}: {}", ancestor.data.name, n));
                    }
                }
            }
            collected.extend(node.data.notes.iter().cloned());
            collected
        };

        // Helper: check if a node has children with status (task-eligible children)
        let has_status_children = |node: &C4Node| -> bool {
            model.nodes.iter().any(|n| {
                n.parent_id.as_deref() == Some(&node.id)
                    && n.data.status.is_some()
                    && match node.data.kind {
                        C4Kind::Container => n.data.kind == C4Kind::Component,
                        C4Kind::System => n.data.kind == C4Kind::Container,
                        _ => false,
                    }
            })
        };

        // Helper: check if all status-bearing children are done (wip or ready)
        let children_all_done = |node: &C4Node| -> bool {
            let child_kind = match node.data.kind {
                C4Kind::Container => C4Kind::Component,
                C4Kind::System => C4Kind::Container,
                _ => return true,
            };
            model.nodes.iter()
                .filter(|n| {
                    n.parent_id.as_deref() == Some(&node.id)
                        && n.data.kind == child_kind
                        && n.data.status.is_some()
                })
                .all(|n| matches!(n.data.status, Some(Status::Wip) | Some(Status::Ready)))
        };

        // Classify nodes: satisfied vs needs-work
        // For containers with component children (or systems with container children),
        // satisfaction requires ALL children to be done (wip/ready) — not just the container itself.
        let is_satisfied = |node: &C4Node| -> bool {
            if node.data.external == Some(true) {
                return true;
            }
            if has_status_children(node) {
                return children_all_done(node);
            }
            matches!(node.data.status, Some(Status::Wip) | Some(Status::Ready) | None)
        };

        // Collect task-eligible nodes: containers and components (excluding None-status)
        // Containers that have component children with status are NOT tasks themselves — their components are.
        let task_nodes: Vec<&C4Node> = model
            .nodes
            .iter()
            .filter(|n| {
                let eligible = matches!(n.data.kind, C4Kind::Container | C4Kind::Component);
                if !eligible {
                    return false;
                }
                // Skip None-status nodes (context/framework defaults)
                if n.data.status.is_none() {
                    return false;
                }
                // Skip external systems' children
                if let Some(pid) = &n.parent_id {
                    if let Some(parent) = model.nodes.iter().find(|p| p.id == *pid) {
                        if parent.data.external == Some(true) {
                            return false;
                        }
                    }
                }
                // Skip containers whose components are the real tasks
                if n.data.kind == C4Kind::Container && has_status_children(n) {
                    return false;
                }
                if let Some(scope) = scope_filter {
                    n.id == scope || is_descendant_of(&n.id, scope)
                } else {
                    true
                }
            })
            .collect();

        if task_nodes.is_empty() {
            return Ok(CallToolResult::success(vec![Content::text(
                format_done_message(&model),
            )]));
        }

        // Nodes that need work (proposed or changed)
        let work_nodes: Vec<&C4Node> = task_nodes
            .iter()
            .filter(|n| !is_satisfied(n))
            .copied()
            .collect();

        if work_nodes.is_empty() {
            let completed = task_nodes.iter().filter(|n| is_satisfied(n)).count();

            // Check for containers/systems that should be marked wip
            let mut propagate_nodes: Vec<(&str, &str)> = Vec::new(); // (id, name)
            for node in &model.nodes {
                if !matches!(node.data.kind, C4Kind::Container | C4Kind::System) {
                    continue;
                }
                if matches!(node.data.status, Some(Status::Wip) | Some(Status::Ready)) {
                    continue;
                }
                if !has_status_children(node) {
                    continue;
                }
                if children_all_done(node) {
                    propagate_nodes.push((&node.id, &node.data.name));
                }
            }

            if propagate_nodes.is_empty() {
                return Ok(CallToolResult::success(vec![Content::text(
                    format_done_message(&model),
                )]));
            }

            let mut output = format!(
                "All {} tasks complete.\n\nMark these parent nodes as wip:\n```\nupdate_nodes(model: \"{}\", nodes: [{}])\n```",
                completed,
                req.name,
                propagate_nodes.iter()
                    .map(|(id, _)| format!("{{node_id: \"{}\", status: \"wip\", reason: \"Needs review\", source: [{{pattern: \"src/module/**/*.ts\"}}]}}", id))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            for (_, name) in &propagate_nodes {
                output.push_str(&format!("\n- {}", name));
            }

            // Check for member nodes (operations/processes/models) that are still proposed
            let mut pending_members: Vec<(&C4Node, &str)> = Vec::new();
            for node in &model.nodes {
                if node.data.kind != C4Kind::Component {
                    continue;
                }
                if !matches!(node.data.status, Some(Status::Wip) | Some(Status::Ready)) {
                    continue;
                }
                for member in model.nodes.iter().filter(|n| {
                    n.parent_id.as_deref() == Some(&node.id)
                        && matches!(n.data.kind, C4Kind::Operation | C4Kind::Process | C4Kind::Model)
                        && n.data.status.is_some()
                        && matches!(n.data.status, Some(Status::Proposed))

                }) {
                    pending_members.push((member, &node.data.name));
                }
            }
            if !pending_members.is_empty() {
                output.push_str("\n\nThese member nodes are still proposed — mark as `wip` with a reason explaining what was built:\n");
                for (member, parent_name) in &pending_members {
                    output.push_str(&format!(
                        "  - {} [{}] ({}, {}) in {}\n",
                        member.data.name,
                        member.id,
                        kind_str(&member.data.kind),
                        status_str(&member.data.status),
                        parent_name
                    ));
                }
            }

            if !model.flows.is_empty() {
                output.push_str("\n\nThen call `get_task` again to validate flows.");
            }

            return Ok(CallToolResult::success(vec![Content::text(output)]));
        }

        let total_tasks = task_nodes.len();
        let completed_tasks = task_nodes.iter().filter(|n| is_satisfied(n)).count();

        // Check if all edge dependencies are satisfied
        // Edge direction: source → target means source depends on target
        let deps_satisfied = |node: &C4Node| -> bool {
            for edge in &model.edges {
                if edge.source == node.id {
                    // This node depends on edge.target — check if target is satisfied
                    // But only check targets that are task-eligible (containers/components)
                    if let Some(target) = model.nodes.iter().find(|n| n.id == edge.target) {
                        if matches!(target.data.kind, C4Kind::Container | C4Kind::Component) {
                            if !is_satisfied(target) {
                                return false;
                            }
                        }
                    }
                }
            }
            true
        };

        // Classify work nodes into ready vs blocked
        let mut ready_nodes: Vec<&C4Node> = Vec::new();
        let mut blocked_nodes: Vec<&C4Node> = Vec::new();

        for node in &work_nodes {
            if deps_satisfied(node) {
                ready_nodes.push(node);
            } else {
                blocked_nodes.push(node);
            }
        }

        // Cycle detection: if nothing is ready but work remains, we have a cycle
        if ready_nodes.is_empty() && !blocked_nodes.is_empty() {
            let cycle_names: Vec<String> = blocked_nodes
                .iter()
                .map(|n| format!("  - {} [{}]", n.data.name, n.id))
                .collect();
            return Ok(CallToolResult::success(vec![Content::text(format!(
                "Dependency cycle detected. The following nodes all block each other:\n\n{}\n\nFix the model by removing or redirecting edges to break the cycle.",
                cycle_names.join("\n")
            ))]));
        }

        // Group ready nodes into work units
        // Phase 1: Scaffold — deployment groups where ALL member containers are proposed
        // Phase 2: Individual containers not in groups that are proposed
        // Phase 3: Components (grouped by parent if siblings have no inter-deps)

        // Check for scaffold tasks: deployment groups with all members proposed
        for group in &model.groups {
            if group.kind != scryer_core::GroupKind::Deployment {
                continue;
            }
            let member_containers: Vec<&C4Node> = ready_nodes
                .iter()
                .filter(|n| {
                    n.data.kind == C4Kind::Container && group.member_ids.contains(&n.id)
                })
                .copied()
                .collect();

            // All group members must be proposed (not just the ready ones)
            let all_members_proposed = group.member_ids.iter().all(|mid| {
                model.nodes.iter().find(|n| n.id == *mid).map_or(false, |n| {
                    matches!(n.data.status, Some(Status::Proposed))

                })
            });

            if !member_containers.is_empty() && all_members_proposed {
                // Scaffold task for this deployment group
                let task_num = completed_tasks + 1;
                let mut output = format!(
                    "# Task {} of {}\n\n## Scaffold: {}\n\n",
                    task_num, total_tasks, group.name
                );
                if let Some(desc) = &group.description {
                    output.push_str(&format!("{}\n\n", desc));
                }
                output.push_str("Set up the project structure for these containers:\n\n");
                for mc in &member_containers {
                    output.push_str(&format!("- **{}** [{}]", mc.data.name, mc.id));
                    if let Some(tech) = &mc.data.technology {
                        output.push_str(&format!(" — {}", tech));
                    }
                    output.push('\n');
                    if !mc.data.description.is_empty() {
                        output.push_str(&format!("  {}\n", mc.data.description));
                    }
                }

                // Include group contract if present
                if !group.contract.is_empty() {
                    output.push_str(&format!("\n{} — Group Contract (MUST follow):\n", group.name));
                    if !group.contract.expect.is_empty() {
                        output.push_str("  MUST:\n");
                        for item in &group.contract.expect {
                            output.push_str(&format!("    - {}\n", item));
                        }
                    }
                    if !group.contract.ask.is_empty() {
                        output.push_str("  ASK USER FIRST:\n");
                        for item in &group.contract.ask {
                            output.push_str(&format!("    - {}\n", item));
                        }
                    }
                    if !group.contract.never.is_empty() {
                        output.push_str("  NEVER:\n");
                        for item in &group.contract.never {
                            output.push_str(&format!("    - {}\n", item));
                        }
                    }
                }

                // Include contract/notes from the containers
                for mc in &member_containers {
                    let ancestors = get_ancestor_chain(&mc.id);
                    let contract = merge_contract(&ancestors, mc);
                    let notes = collect_notes(&ancestors, mc);
                    output.push_str(&format_contract_and_notes(
                        &mc.data.name, &contract, &notes,
                    ));
                }

                output.push_str(&format!("\n---\n\n{}\n\n", TASK_INSTRUCTIONS));

                // Node IDs to mark wip
                let ids: Vec<&str> = member_containers.iter().map(|n| n.id.as_str()).collect();
                output.push_str(&format!(
                    "After scaffolding, mark these as wip with a reason explaining what was scaffolded:\n```\nupdate_nodes(model: \"{}\", nodes: [{}])\n```\n",
                    req.name,
                    ids.iter().map(|id| format!("{{node_id: \"{}\", status: \"wip\", reason: \"Needs implementation\"}}", id)).collect::<Vec<_>>().join(", ")
                ));

                // Next up
                let next_name = find_next_name(&blocked_nodes, &ready_nodes, &member_containers);
                output.push_str(&format!(
                    "\n---\nProgress: {}/{} tasks complete{}",
                    completed_tasks, total_tasks,
                    if let Some(name) = next_name { format!(" | Next up: {}", name) } else { String::new() }
                ));

                return Ok(CallToolResult::success(vec![Content::text(output)]));
            }
        }

        // No scaffold task — find ready containers first, then components
        let ready_containers: Vec<&C4Node> = ready_nodes
            .iter()
            .filter(|n| n.data.kind == C4Kind::Container)
            .copied()
            .collect();

        let ready_components: Vec<&C4Node> = ready_nodes
            .iter()
            .filter(|n| n.data.kind == C4Kind::Component)
            .copied()
            .collect();

        // Pick the work unit: prefer containers, then components
        let work_unit: Vec<&C4Node> = if !ready_containers.is_empty() {
            // Group containers by parent system — take the first system's containers
            let first_parent = ready_containers[0].parent_id.as_deref();
            ready_containers
                .iter()
                .filter(|n| n.parent_id.as_deref() == first_parent)
                .copied()
                .collect()
        } else {
            // Group sibling components (same parent container) with no inter-dependencies
            let first_parent = ready_components[0].parent_id.as_deref();
            let siblings: Vec<&C4Node> = ready_components
                .iter()
                .filter(|n| n.parent_id.as_deref() == first_parent)
                .copied()
                .collect();

            // Check for inter-dependencies among siblings
            let sibling_ids: std::collections::HashSet<&str> =
                siblings.iter().map(|n| n.id.as_str()).collect();
            let has_inter_deps = model.edges.iter().any(|e| {
                sibling_ids.contains(e.source.as_str())
                    && sibling_ids.contains(e.target.as_str())
            });

            if has_inter_deps {
                // Return only the first sibling that has no deps on other siblings
                siblings
                    .iter()
                    .filter(|n| {
                        !model.edges.iter().any(|e| {
                            e.source == n.id && sibling_ids.contains(e.target.as_str())
                        })
                    })
                    .copied()
                    .collect::<Vec<_>>()
                    .into_iter()
                    .take(1)
                    .chain(std::iter::empty())
                    .collect()
            } else {
                siblings
            }
        };

        if work_unit.is_empty() {
            // Shouldn't happen but safety fallback
            return Ok(CallToolResult::success(vec![Content::text(
                "All tasks complete. Nothing to build.",
            )]));
        }

        // Format the work unit
        let task_num = completed_tasks + 1;
        let is_scaffold = work_unit.iter().all(|n| {
            n.data.kind == C4Kind::Container && matches!(n.data.status, Some(Status::Proposed))

        }) && work_unit.len() > 1;

        let unit_label = if is_scaffold {
            // Find deployment group name if any
            let group_name = model.groups.iter().find(|g| {
                work_unit.iter().any(|n| g.member_ids.contains(&n.id))
            }).map(|g| g.name.clone());
            format!("Scaffold: {}", group_name.unwrap_or_else(|| work_unit[0].data.name.clone()))
        } else if work_unit.len() == 1 {
            format!("Build: {}", work_unit[0].data.name)
        } else {
            let names: Vec<&str> = work_unit.iter().map(|n| n.data.name.as_str()).collect();
            format!("Build: {}", names.join(" + "))
        };

        let mut output = format!(
            "# Task {} of {}\n\n## {}\n\n",
            task_num, total_tasks, unit_label
        );

        for node in &work_unit {
            let ancestors = get_ancestor_chain(&node.id);
            let contract = merge_contract(&ancestors, node);
            let notes = collect_notes(&ancestors, node);

            if work_unit.len() > 1 {
                output.push_str(&format!("### {} [{}]\n", node.data.name, node.id));
            } else {
                output.push_str(&format!("[{}]\n", node.id));
            }

            if !node.data.description.is_empty() {
                output.push_str(&format!("{}\n", node.data.description));
            }
            if let Some(tech) = &node.data.technology {
                output.push_str(&format!("Technology: {}\n", tech));
            }
            output.push_str(&format!("Status: {}\n", status_str(&node.data.status)));

            // Contract — framed as binding requirements so agents don't skip them
            if !contract.is_empty() {
                output.push_str("\nContract (you MUST follow these requirements):\n");
                if !contract.expect.is_empty() {
                    output.push_str("  MUST:\n");
                    for item in &contract.expect {
                        output.push_str(&format!("    - {}\n", item));
                    }
                }
                if !contract.ask.is_empty() {
                    output.push_str("  ASK USER FIRST:\n");
                    for item in &contract.ask {
                        output.push_str(&format!("    - {}\n", item));
                    }
                }
                if !contract.never.is_empty() {
                    output.push_str("  NEVER:\n");
                    for item in &contract.never {
                        output.push_str(&format!("    - {}\n", item));
                    }
                }
            }

            // Notes
            if !notes.is_empty() {
                output.push_str("\nNotes:\n");
                for d in &notes {
                    output.push_str(&format!("  - {}\n", d));
                }
            }

            // Child processes
            let child_processes: Vec<&C4Node> = model
                .nodes
                .iter()
                .filter(|n| {
                    n.parent_id.as_deref() == Some(&node.id) && n.data.kind == C4Kind::Process
                })
                .collect();
            if !child_processes.is_empty() {
                output.push_str("\nProcesses:\n");
                for p in &child_processes {
                    output.push_str(&format!(
                        "  - {} [{}] ({})\n",
                        p.data.name,
                        p.id,
                        status_str(&p.data.status)
                    ));
                    if !p.data.description.is_empty() {
                        output.push_str(&format!("    {}\n", p.data.description));
                    }
                }
            }

            // Child models
            let child_models: Vec<&C4Node> = model
                .nodes
                .iter()
                .filter(|n| {
                    n.parent_id.as_deref() == Some(&node.id) && n.data.kind == C4Kind::Model
                })
                .collect();
            if !child_models.is_empty() {
                output.push_str("\nModels:\n");
                for m in &child_models {
                    output.push_str(&format!(
                        "  - {} [{}] ({})\n",
                        m.data.name,
                        m.id,
                        status_str(&m.data.status)
                    ));
                    if !m.data.description.is_empty() {
                        output.push_str(&format!("    {}\n", m.data.description));
                    }
                    if !m.data.properties.is_empty() {
                        for prop in &m.data.properties {
                            output.push_str(&format!("    .{}", prop.label));
                            if !prop.description.is_empty() {
                                output.push_str(&format!(" — {}", prop.description));
                            }
                            output.push('\n');
                        }
                    }
                }
            }

            // Operations
            let operations: Vec<&C4Node> = model
                .nodes
                .iter()
                .filter(|n| {
                    n.parent_id.as_deref() == Some(&node.id) && n.data.kind == C4Kind::Operation
                })
                .collect();
            if !operations.is_empty() {
                output.push_str("\nOperations:\n");
                for op in &operations {
                    output.push_str(&format!(
                        "  - {} [{}] ({})\n",
                        op.data.name,
                        op.id,
                        status_str(&op.data.status)
                    ));
                    if !op.data.description.is_empty() {
                        output.push_str(&format!("    {}\n", op.data.description));
                    }
                }
            }

            // Sources
            if !node.data.sources.is_empty() {
                output.push_str("\nSources:\n");
                for r in &node.data.sources {
                    output.push_str(&format!("  - {} — {}\n", r.pattern, r.comment));
                }
            }

            // Dependencies (edges involving this node)
            let deps: Vec<String> = model
                .edges
                .iter()
                .filter_map(|e| {
                    if e.source == node.id {
                        let target = model.nodes.iter().find(|n| n.id == e.target);
                        let label = e.data.as_ref().map(|d| d.label.as_str()).unwrap_or("");
                        target.map(|t| {
                            format!(
                                "  -> {} \"{}\" ({})",
                                t.data.name,
                                label,
                                kind_str(&t.data.kind)
                            )
                        })
                    } else if e.target == node.id {
                        let source = model.nodes.iter().find(|n| n.id == e.source);
                        let label = e.data.as_ref().map(|d| d.label.as_str()).unwrap_or("");
                        source.map(|s| {
                            format!(
                                "  <- {} \"{}\" ({})",
                                s.data.name,
                                label,
                                kind_str(&s.data.kind)
                            )
                        })
                    } else {
                        None
                    }
                })
                .collect();

            if !deps.is_empty() {
                output.push_str("\nDependencies:\n");
                for dep in &deps {
                    output.push_str(&format!("{}\n", dep));
                }
            }

            output.push('\n');
        }

        output.push_str(&format!("---\n\n{}\n\n", TASK_INSTRUCTIONS));

        // Mark-as-wip hint
        let ids: Vec<&str> = work_unit.iter().map(|n| n.id.as_str()).collect();
        output.push_str(&format!(
            "After building, mark as wip with a reason and set source locations:\n```\nupdate_nodes(model: \"{}\", nodes: [{}])\n```\n",
            req.name,
            ids.iter().map(|id| format!("{{node_id: \"{}\", status: \"wip\", reason: \"Needs error handling\", source: [{{pattern: \"src/module/file.ts\", line: 1, endLine: 50}}]}}", id)).collect::<Vec<_>>().join(", ")
        ));

        // Member status confirmation: collect operations/processes/models still proposed
        let mut pending_members: Vec<(&C4Node, &str)> = Vec::new(); // (node, parent_name)
        for node in &work_unit {
            if node.data.kind == C4Kind::Component {
                for member in model.nodes.iter().filter(|n| {
                    n.parent_id.as_deref() == Some(&node.id)
                        && matches!(n.data.kind, C4Kind::Operation | C4Kind::Process | C4Kind::Model)
                        && matches!(n.data.status, Some(Status::Proposed))
                }) {
                    pending_members.push((member, &node.data.name));
                }
            }
        }
        if !pending_members.is_empty() {
            output.push_str("\nAlso mark these member nodes as `wip` with a reason explaining what was built:\n");
            for (member, parent_name) in &pending_members {
                output.push_str(&format!(
                    "  - {} [{}] ({}, {}) in {}\n",
                    member.data.name,
                    member.id,
                    kind_str(&member.data.kind),
                    status_str(&member.data.status),
                    parent_name
                ));
            }
        }

        // Next up
        let next_name = find_next_name(&blocked_nodes, &ready_nodes, &work_unit);
        output.push_str(&format!(
            "\n---\nProgress: {}/{} tasks complete{}",
            completed_tasks, total_tasks,
            if let Some(name) = next_name { format!(" | Next up: {}", name) } else { String::new() }
        ));

        Ok(CallToolResult::success(vec![Content::text(output)]))
    }
}
