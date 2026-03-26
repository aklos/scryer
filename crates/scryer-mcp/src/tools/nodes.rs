use crate::helpers::*;
use crate::server::ScryerServer;
use crate::types::*;
use crate::validate::*;
use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, Content},
    tool, tool_router, ErrorData as McpError,
};
use scryer_core::{
    C4Edge, C4Kind, C4ModelData, C4Node, C4NodeData,
    Status,
};
use serde::Deserialize;
use std::collections::HashSet;

#[tool_router(router = tool_router_nodes, vis = "pub(crate)")]
impl ScryerServer {
    #[tool(
        description = "Create or overwrite a model with complete data in one call. Use for initial model creation or full rewrites. Pass the full model JSON with all nodes and edges. Node positions are handled automatically by the UI — do not include position data.\n\nJSON format:\n- Containers MUST have `parentId` set to a system node's ID. Components MUST have `parentId` set to a container's ID. Without `parentId`, nodes render as flat siblings instead of nested.\n- Include `sources`, `technology`, `shape`, and `status` directly in each node's data — do NOT add them in a separate pass.\n- `position` and `type` can be omitted (default to auto-layout and \"c4\").\n- Edge IDs follow the pattern `edge-{source}-{target}`.\n- Edge labels MUST be short (max 30 characters). One verb phrase per edge.\n\nExample:\n{\"nodes\": [\n  {\"id\": \"node-1\", \"data\": {\"name\": \"User\", \"description\": \"End user\", \"kind\": \"person\", \"status\": \"proposed\"}},\n  {\"id\": \"node-2\", \"data\": {\"name\": \"My System\", \"description\": \"Main system\", \"kind\": \"system\", \"status\": \"proposed\"}},\n  {\"id\": \"node-3\", \"parentId\": \"node-2\", \"data\": {\"name\": \"Web App\", \"description\": \"Frontend SPA\", \"kind\": \"container\", \"technology\": \"React\", \"status\": \"proposed\"}},\n  {\"id\": \"node-4\", \"parentId\": \"node-2\", \"data\": {\"name\": \"Database\", \"description\": \"Primary data store\", \"kind\": \"container\", \"technology\": \"PostgreSQL\", \"shape\": \"cylinder\", \"status\": \"proposed\"}}\n], \"edges\": [\n  {\"id\": \"edge-node-1-node-2\", \"source\": \"node-1\", \"target\": \"node-2\", \"data\": {\"label\": \"uses\"}},\n  {\"id\": \"edge-node-3-node-4\", \"source\": \"node-3\", \"target\": \"node-4\", \"data\": {\"label\": \"reads from\", \"method\": \"SQL\"}}\n]}"
    )]
    fn set_model(
        &self,
        Parameters(req): Parameters<SetModelRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = match self.resolve_model(req.name) {
            Ok(r) => r,
            Err(e) => return Ok(e),
        };
        let mut model: C4ModelData = match serde_json::from_str(&req.data) {
            Ok(m) => m,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Invalid model JSON: {}",
                    e
                ))]));
            }
        };

        // Validate nodes
        for node in &model.nodes {
            if node.data.description.len() > 200
                && !matches!(
                    node.data.kind,
                    C4Kind::Operation | C4Kind::Process | C4Kind::Model
                )
            {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Description for '{}' must be 200 characters or less",
                    node.data.name
                ))]));
            }
            if let Some(tech) = &node.data.technology {
                if tech.len() > 28 {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "Technology '{}' on '{}' exceeds 28 character limit",
                        tech, node.data.name
                    ))]));
                }
            }
            if node.data.kind == C4Kind::Operation {
                if let Err(e) = validate_identifier(
                    &node.data.name,
                    &format!("{:?} '{}'", node.data.kind, node.id),
                ) {
                    return Ok(CallToolResult::error(vec![Content::text(e)]));
                }
            }
            if node.data.kind == C4Kind::Model {
                if let Err(e) = validate_type_name(
                    &node.data.name,
                    &format!("{:?} '{}'", node.data.kind, node.id),
                ) {
                    return Ok(CallToolResult::error(vec![Content::text(e)]));
                }
            }
            if !node.data.properties.is_empty() {
                if let Err(e) =
                    validate_property_labels(&node.data.properties, &format!("node '{}'", node.id))
                {
                    return Ok(CallToolResult::error(vec![Content::text(e)]));
                }
            }
        }

        // Validate no children under external systems
        if let Err(e) = validate_no_children_of_external(&model.nodes) {
            return Ok(CallToolResult::error(vec![Content::text(e)]));
        }

        // Validate edge labels
        for edge in &model.edges {
            if let Some(data) = &edge.data {
                if data.label.len() > 30 {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "Edge label '{}' exceeds 30 character limit",
                        data.label
                    ))]));
                }
            }
        }

        // Set project_path to cwd if not already set — needed for source map → editor linking
        if model.project_path.is_none() {
            if let Ok(cwd) = std::env::current_dir() {
                model.project_path = Some(cwd.to_string_lossy().to_string());
            }
        }

        // Strip all positions — layout is a UI concern
        for node in &mut model.nodes {
            node.position = None;
        }

        // Deduplicate edges by ID (keep first occurrence)
        {
            let mut seen = HashSet::new();
            model.edges.retain(|e| seen.insert(e.id.clone()));
        }

        let node_count = model.nodes.len();
        let edge_count = model.edges.len();
        let cross_level_warnings = check_disconnected_nodes(&model);
        let bidir_warnings = check_bidirectional_edges(&model);
        let mention_warnings = check_mention_edges(&model);
        let cross_container_warnings = check_cross_container_edges(&model);
        match scryer_core::write_model_at(&model_ref, &model) {
            Ok(()) => {
                let _ = scryer_core::save_baseline_at(&model_ref, &model);
                // Register the project if project-local
                if let scryer_core::ModelRef::ProjectLocal(ref path) = model_ref {
                    let _ = scryer_core::register_project(path);
                }
                let mut msg = format!(
                    "Set model '{}' ({} nodes, {} edges)",
                    model_ref, node_count, edge_count
                );
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

    #[tool(
        description = "Add one or more nodes to a model. Hierarchy: person/system (top-level), container (parent=system), component (parent=container), operation/process/model (parent=component). All nodes use type 'c4'."
    )]
    fn add_nodes(
        &self,
        Parameters(req): Parameters<AddNodeRequest>,
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

        let mut added_ids = Vec::new();
        for item in &req.nodes {
            let kind = parse_kind(&item.kind)?;

            if item.description.len() > 200
                && !matches!(kind, C4Kind::Operation | C4Kind::Process | C4Kind::Model)
            {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Description for '{}' must be 200 characters or less",
                    item.name
                ))]));
            }
            if let Some(tech) = &item.technology {
                if tech.len() > 28 {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "Technology '{}' on '{}' exceeds 28 character limit",
                        tech, item.name
                    ))]));
                }
            }

            if kind == C4Kind::Operation {
                if let Err(e) = validate_identifier(&item.name, &format!("{:?}", kind)) {
                    return Ok(CallToolResult::error(vec![Content::text(e)]));
                }
            }
            if kind == C4Kind::Model {
                if let Err(e) = validate_type_name(&item.name, &format!("{:?}", kind)) {
                    return Ok(CallToolResult::error(vec![Content::text(e)]));
                }
            }
            if let Some(props) = &item.properties {
                if let Err(e) = validate_property_labels(props, &format!("node '{}'", item.name)) {
                    return Ok(CallToolResult::error(vec![Content::text(e)]));
                }
            }

            if let Err(e) = validate_parent(&model, &kind, item.parent_id.as_deref()) {
                return Ok(CallToolResult::error(vec![Content::text(e)]));
            }

            let id = scryer_core::next_node_id(&model);
            let shape = item.shape.as_deref().and_then(parse_shape);
            let status = if kind == C4Kind::Person {
                None
            } else {
                item.status.as_deref().and_then(parse_status)
            };

            let node_type = match kind {
                C4Kind::Operation => "operation",
                C4Kind::Process => "process",
                C4Kind::Model => "model",
                _ => "c4",
            };
            model.nodes.push(C4Node {
                id: id.clone(),
                node_type: node_type.to_string(),
                position: None,
                data: C4NodeData {
                    name: item.name.clone(),
                    description: item.description.clone(),
                    kind,
                    technology: item.technology.clone(),
                    external: item.external,
                    expanded: None,
                    shape,
                    sources: item.sources.clone().unwrap_or_default(),
                    status,
                    status_reason: None,
                    contract: item.contract.clone().unwrap_or_default(),
                    notes: item.notes.clone().unwrap_or_default(),
                    properties: item.properties.clone().unwrap_or_default(),
                },
                parent_id: item.parent_id.clone(),
            });
            added_ids.push(id);
        }

        match scryer_core::write_model_at(&model_ref, &model) {
            Ok(()) => {
                let _ = scryer_core::save_baseline_at(&model_ref, &model);
                Ok(CallToolResult::success(vec![Content::text(format!(
                    "Added {} node(s): {}",
                    added_ids.len(),
                    added_ids.join(", ")
                ))]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e)])),
        }
    }

    #[tool(
        description = "Replace all descendants of an existing node in one call. Removes existing children and their edges, then inserts the provided nodes and edges. Use this to detail a system (add containers), a container (add components), etc. without calling add_node repeatedly. The target node must already exist. All nodes in data must have parentId chains leading back to node_id. Edges can reference any node in the model."
    )]
    fn set_node(
        &self,
        Parameters(req): Parameters<SetNodeRequest>,
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

        // Verify target node exists
        if !model.nodes.iter().any(|n| n.id == req.node_id) {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "Node '{}' not found",
                req.node_id
            ))]));
        }

        // Parse incoming subtree
        #[derive(Deserialize)]
        struct SubtreeData {
            #[serde(default)]
            nodes: Vec<C4Node>,
            #[serde(default)]
            edges: Vec<C4Edge>,
        }
        let subtree: SubtreeData = match serde_json::from_str(&req.data) {
            Ok(s) => s,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Invalid subtree JSON: {}",
                    e
                ))]));
            }
        };

        // Validate subtree nodes
        for node in &subtree.nodes {
            if node.data.description.len() > 200
                && !matches!(
                    node.data.kind,
                    C4Kind::Operation | C4Kind::Process | C4Kind::Model
                )
            {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Description for '{}' must be 200 characters or less",
                    node.data.name
                ))]));
            }
            if let Some(tech) = &node.data.technology {
                if tech.len() > 28 {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "Technology '{}' on '{}' exceeds 28 character limit",
                        tech, node.data.name
                    ))]));
                }
            }
            if node.data.kind == C4Kind::Operation {
                if let Err(e) = validate_identifier(
                    &node.data.name,
                    &format!("{:?} '{}'", node.data.kind, node.id),
                ) {
                    return Ok(CallToolResult::error(vec![Content::text(e)]));
                }
            }
            if node.data.kind == C4Kind::Model {
                if let Err(e) = validate_type_name(
                    &node.data.name,
                    &format!("{:?} '{}'", node.data.kind, node.id),
                ) {
                    return Ok(CallToolResult::error(vec![Content::text(e)]));
                }
            }
            if !node.data.properties.is_empty() {
                if let Err(e) =
                    validate_property_labels(&node.data.properties, &format!("node '{}'", node.id))
                {
                    return Ok(CallToolResult::error(vec![Content::text(e)]));
                }
            }
        }

        // Validate edge labels
        for edge in &subtree.edges {
            if let Some(data) = &edge.data {
                if data.label.len() > 30 {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "Edge label '{}' exceeds 30 character limit",
                        data.label
                    ))]));
                }
            }
        }

        // Collect all existing descendant IDs of node_id
        let mut old_descendants = HashSet::new();
        let mut changed = true;
        while changed {
            changed = false;
            for n in &model.nodes {
                if let Some(pid) = &n.parent_id {
                    let is_child = *pid == req.node_id || old_descendants.contains(pid);
                    if is_child && !old_descendants.contains(&n.id) {
                        old_descendants.insert(n.id.clone());
                        changed = true;
                    }
                }
            }
        }

        // Remove old descendants and edges referencing them
        model.nodes.retain(|n| !old_descendants.contains(&n.id));
        model.edges.retain(|e| {
            !old_descendants.contains(&e.source) && !old_descendants.contains(&e.target)
        });

        // Validate all incoming nodes have parent chains leading to node_id
        let incoming_ids: HashSet<_> = subtree.nodes.iter().map(|n| n.id.clone()).collect();
        for node in &subtree.nodes {
            match &node.parent_id {
                Some(pid) if *pid == req.node_id || incoming_ids.contains(pid) => {}
                Some(pid) => {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "Node '{}' has parentId '{}' which is not in the subtree or the target node",
                        node.id, pid
                    ))]));
                }
                None => {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "Node '{}' has no parentId — all nodes must be descendants of '{}'",
                        node.id, req.node_id
                    ))]));
                }
            }
        }

        // Check for ID collisions with remaining model nodes
        let existing_ids: HashSet<_> = model.nodes.iter().map(|n| n.id.clone()).collect();
        for node in &subtree.nodes {
            if existing_ids.contains(&node.id) {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Node ID '{}' already exists in the model",
                    node.id
                ))]));
            }
        }

        // Validate parent hierarchy (kind rules)
        for node in &subtree.nodes {
            let pid = node.parent_id.as_deref().unwrap(); // validated above
            let parent_kind = if pid == req.node_id {
                model
                    .nodes
                    .iter()
                    .find(|n| n.id == req.node_id)
                    .map(|n| &n.data.kind)
            } else {
                subtree
                    .nodes
                    .iter()
                    .find(|n| n.id == pid)
                    .map(|n| &n.data.kind)
            };
            if let Some(pk) = parent_kind {
                match (&node.data.kind, pk) {
                    (C4Kind::Container, C4Kind::System) => {}
                    (C4Kind::Component, C4Kind::Container) => {}
                    (C4Kind::Operation, C4Kind::Component) => {}
                    (C4Kind::Process, C4Kind::Component) => {}
                    (C4Kind::Model, C4Kind::Component) => {}
                    (kind, pk) => {
                        return Ok(CallToolResult::error(vec![Content::text(format!(
                            "Node '{}' (kind {:?}) cannot be a child of '{}' (kind {:?})",
                            node.id, kind, pid, pk
                        ))]));
                    }
                }
            }
        }

        // Strip all positions — layout is a UI concern
        let node_count = subtree.nodes.len();
        let edge_count = subtree.edges.len();
        let mut new_nodes = subtree.nodes;
        for node in &mut new_nodes {
            node.position = None;
        }
        model.nodes.extend(new_nodes);

        // Validate edges reference existing nodes
        let all_ids: HashSet<_> = model.nodes.iter().map(|n| n.id.as_str()).collect();
        for edge in &subtree.edges {
            if !all_ids.contains(edge.source.as_str()) {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Edge source '{}' not found",
                    edge.source
                ))]));
            }
            if !all_ids.contains(edge.target.as_str()) {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Edge target '{}' not found",
                    edge.target
                ))]));
            }
        }
        // Skip subtree edges whose ID already exists in the model (warn the agent)
        let existing_edge_ids: HashSet<_> = model.edges.iter().map(|e| e.id.clone()).collect();
        let mut skipped_edges = Vec::new();
        for edge in subtree.edges {
            if existing_edge_ids.contains(&edge.id) {
                skipped_edges.push(edge.id);
            } else {
                model.edges.push(edge);
            }
        }

        // Check for missing edges when detailing a system or container (not components —
        // operation/process/model nodes are code-level and don't need architectural edges).
        let parent_kind = model.nodes.iter().find(|n| n.id == req.node_id).map(|n| &n.data.kind);
        let check_edges = matches!(parent_kind, Some(C4Kind::System) | Some(C4Kind::Container));
        let new_subtree_ids: HashSet<&str> = incoming_ids.iter().map(|s| s.as_str()).collect();
        let mut missing_externals: Vec<String> = Vec::new();
        if check_edges {
            for edge in &model.edges {
                // Find edges where the parent node itself is source or target
                let external_id = if edge.source == req.node_id && !new_subtree_ids.contains(edge.target.as_str()) && edge.target != req.node_id {
                    Some(&edge.target)
                } else if edge.target == req.node_id && !new_subtree_ids.contains(edge.source.as_str()) && edge.source != req.node_id {
                    Some(&edge.source)
                } else {
                    None
                };
                if let Some(ext_id) = external_id {
                    let has_subtree_edge = model.edges.iter().any(|e| {
                        let src_in = new_subtree_ids.contains(e.source.as_str());
                        let tgt_in = new_subtree_ids.contains(e.target.as_str());
                        (src_in && e.target == *ext_id) || (tgt_in && e.source == *ext_id)
                    });
                    if !has_subtree_edge {
                        if let Some(ext_node) = model.nodes.iter().find(|n| n.id == *ext_id) {
                            let name = format!("{} ({})", ext_node.data.name, kind_str(&ext_node.data.kind));
                            if !missing_externals.contains(&name) {
                                missing_externals.push(name);
                            }
                        }
                    }
                }
            }
        }

        match scryer_core::write_model_at(&model_ref, &model) {
            Ok(()) => {
                let _ = scryer_core::save_baseline_at(&model_ref, &model);
                let mut msg = format!(
                    "Set {} descendant node(s) and {} edge(s) under '{}'",
                    node_count, edge_count, req.node_id
                );
                if !skipped_edges.is_empty() {
                    msg.push_str(&format!(
                        "\n\n⚠️ SKIPPED {} DUPLICATE EDGE(S): {} — these edge IDs already exist in the model.",
                        skipped_edges.len(),
                        skipped_edges.join(", ")
                    ));
                }
                if !missing_externals.is_empty() {
                    msg.push_str(&format!(
                        "\n\n⚠️ MISSING EDGES: The parent node '{}' has edges to external nodes, \
                        but none of the new children have edges connecting to: {}. \
                        C4 requires edges at every abstraction level — use add_edges to connect \
                        the appropriate children to these nodes.",
                        req.node_id,
                        missing_externals.join(", ")
                    ));
                }
                let mention_warnings = check_mention_edges(&model);
        let cross_container_warnings = check_cross_container_edges(&model);
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

    #[tool(description = "Patch one or more existing nodes. This is a partial update — only include fields you want to change. Omitted fields are left unchanged. Do NOT use set_node or set_model just to change a few properties.\n\nCommon uses:\n- Change status: {\"node_id\": \"node-5\", \"status\": \"implemented\", \"reason\": \"Built handler and tests\"}\n- Update description: {\"node_id\": \"node-3\", \"description\": \"New description\"}\n- Set source map: {\"node_id\": \"node-5\", \"source\": [{\"pattern\": \"src/handler.ts\", \"line\": 10, \"endLine\": 30}]}\n- Multiple nodes at once: pass an array of patches to the nodes parameter")]
    fn update_nodes(
        &self,
        Parameters(req): Parameters<UpdateNodeRequest>,
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

        let mut updated = Vec::new();
        for item in req.nodes {
            let node_idx = match model.nodes.iter().position(|n| n.id == item.node_id) {
                Some(i) => i,
                None => {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "Node '{}' not found",
                        item.node_id
                    ))]));
                }
            };

            // Pre-validate verified gate before taking mutable borrow
            if let Some(ref s) = item.status {
                let new_status = parse_status(s);
                if new_status == Some(Status::Verified) && model.nodes[node_idx].data.kind != C4Kind::Person {
                    let parent_id = model.nodes[node_idx].parent_id.clone();
                    let own_contract = item.contract.as_ref().unwrap_or(&model.nodes[node_idx].data.contract).clone();
                    let unmet = check_verified_gate(&model.nodes, &model.groups, &item.node_id, &parent_id, &own_contract);
                    if !unmet.is_empty() {
                        return Ok(CallToolResult::error(vec![Content::text(format!(
                            "Cannot set '{}' to verified. These expect contract items are not yet passed:\n{}\n\nMark each as passed (passed: true) or set status to 'implemented' instead.",
                            item.node_id, unmet.join("\n")
                        ))]));
                    }
                }
            }

            let node = &mut model.nodes[node_idx];

            if let Some(name) = item.name {
                if node.data.kind == C4Kind::Operation {
                    if let Err(e) = validate_identifier(
                        &name,
                        &format!("{:?} '{}'", node.data.kind, item.node_id),
                    ) {
                        return Ok(CallToolResult::error(vec![Content::text(e)]));
                    }
                }
                if node.data.kind == C4Kind::Model {
                    if let Err(e) = validate_type_name(
                        &name,
                        &format!("{:?} '{}'", node.data.kind, item.node_id),
                    ) {
                        return Ok(CallToolResult::error(vec![Content::text(e)]));
                    }
                }
                node.data.name = name;
            }
            if let Some(desc) = item.description {
                if desc.len() > 200
                    && !matches!(
                        node.data.kind,
                        C4Kind::Operation | C4Kind::Process | C4Kind::Model
                    )
                {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "Description for '{}' must be 200 characters or less",
                        item.node_id
                    ))]));
                }
                node.data.description = desc;
            }
            if let Some(tech) = item.technology {
                if tech.len() > 28 {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "Technology '{}' on '{}' exceeds 28 character limit",
                        tech, item.node_id
                    ))]));
                }
                node.data.technology = Some(tech);
            }
            if let Some(ext) = item.external {
                node.data.external = Some(ext);
            }
            if let Some(s) = item.shape {
                node.data.shape = parse_shape(&s);
            }
            if let Some(sources) = item.sources {
                node.data.sources = sources;
            }
            if let Some(ref s) = item.status {
                if node.data.kind != C4Kind::Person {
                    let new_status = parse_status(s);
                    // Require reason when changing status
                    let reason = item.reason.as_deref().unwrap_or("").trim();
                    if reason.is_empty() {
                        return Ok(CallToolResult::error(vec![Content::text(format!(
                            "Node '{}': `reason` is required when changing status. Explain why you're setting status to '{}'.",
                            item.node_id, s
                        ))]));
                    }
                    // Verified gate already validated above (before mutable borrow)
                    node.data.status = new_status;
                    node.data.status_reason = Some(reason.to_string());
                }
            }
            if let Some(g) = item.contract {
                node.data.contract = g;
            }
            if let Some(d) = item.notes {
                node.data.notes = d;
            }
            if let Some(p) = item.properties {
                if let Err(e) = validate_property_labels(&p, &format!("node '{}'", item.node_id)) {
                    return Ok(CallToolResult::error(vec![Content::text(e)]));
                }
                node.data.properties = p;
            }
            if let Some(locations) = item.source {
                if locations.is_empty() {
                    model.source_map.remove(&item.node_id);
                } else {
                    model.source_map.insert(item.node_id.clone(), locations);
                }
            }
            updated.push(item.node_id);
        }

        match scryer_core::write_model_at(&model_ref, &model) {
            Ok(()) => {
                let _ = scryer_core::save_baseline_at(&model_ref, &model);
                Ok(CallToolResult::success(vec![Content::text(format!(
                    "Updated {} node(s)",
                    updated.len()
                ))]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e)])),
        }
    }

    #[tool(
        description = "Delete one or more nodes and all their descendants. Connected edges are also removed."
    )]
    fn delete_nodes(
        &self,
        Parameters(req): Parameters<DeleteNodeRequest>,
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

        let mut to_delete = HashSet::new();
        for nid in &req.node_ids {
            to_delete.insert(nid.clone());
        }
        let mut changed = true;
        while changed {
            changed = false;
            for n in &model.nodes {
                if let Some(pid) = &n.parent_id {
                    if to_delete.contains(pid) && !to_delete.contains(&n.id) {
                        to_delete.insert(n.id.clone());
                        changed = true;
                    }
                }
            }
        }

        let before = model.nodes.len();
        model.nodes.retain(|n| !to_delete.contains(&n.id));
        model
            .edges
            .retain(|e| !to_delete.contains(&e.source) && !to_delete.contains(&e.target));
        let removed = before - model.nodes.len();

        match scryer_core::write_model_at(&model_ref, &model) {
            Ok(()) => {
                let _ = scryer_core::save_baseline_at(&model_ref, &model);
                Ok(CallToolResult::success(vec![Content::text(format!(
                    "Deleted {} node(s)",
                    removed
                ))]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e)])),
        }
    }

}
