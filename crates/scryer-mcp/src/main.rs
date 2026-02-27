use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, Content, ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router, ErrorData as McpError, ServerHandler, ServiceExt,
};
use scryer_core::{
    C4Edge, C4EdgeData, C4Kind, C4ModelData, C4Node, C4NodeData, C4Shape, Flow, Group, GroupKind,
    Contract, ModelProperty, Position, SourceLocation, Status,
};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};

/// Check that a name is a valid identifier: starts with lowercase letter, then [a-zA-Z0-9_]
fn is_valid_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn validate_identifier(name: &str, node_label: &str) -> Result<(), String> {
    if !is_valid_identifier(name) {
        Err(format!(
            "Name '{}' for {} must be a valid identifier (camelCase or snake_case: start with lowercase letter, then [a-zA-Z0-9_])",
            name, node_label
        ))
    } else {
        Ok(())
    }
}

fn validate_property_labels(properties: &[ModelProperty], node_label: &str) -> Result<(), String> {
    for prop in properties {
        if !is_valid_identifier(&prop.label) {
            return Err(format!(
                "Property label '{}' on {} must be a valid identifier (camelCase or snake_case: start with lowercase letter, then [a-zA-Z0-9_])",
                prop.label, node_label
            ));
        }
    }
    Ok(())
}

/// Check that no node is parented under an external system.
fn validate_no_children_of_external(nodes: &[C4Node]) -> Result<(), String> {
    let external_ids: HashSet<&str> = nodes
        .iter()
        .filter(|n| n.data.kind == C4Kind::System && n.data.external.unwrap_or(false))
        .map(|n| n.id.as_str())
        .collect();
    for node in nodes {
        if let Some(pid) = &node.parent_id {
            if external_ids.contains(pid.as_str()) {
                return Err(format!(
                    "Cannot add '{}' inside external system '{}'. External systems are opaque and must not have child nodes.",
                    node.data.name,
                    nodes.iter().find(|n| n.id == *pid).map(|n| n.data.name.as_str()).unwrap_or(pid)
                ));
            }
        }
    }
    Ok(())
}

// --- Request types ---

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct GetModelRequest {
    /// Name of the model to retrieve
    name: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct GetNodeRequest {
    /// Name of the model
    name: String,
    /// ID of the node to inspect (e.g. "node-3"). Returns this node, all its descendants, edges between them, and edges connecting them to external nodes (with external node names for context).
    node_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct SetModelRequest {
    /// Name of the model to create or overwrite
    name: String,
    /// The complete model as a JSON string. Must be a valid C4ModelData object with nodes, edges, and optional startingLevel. See get_model output for the exact schema.
    data: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct AddNodeItem {
    /// Display name for the node
    name: String,
    /// Description of what this node does or represents (max 200 characters, no limit for operation/process/model nodes)
    description: String,
    /// Node kind: "person", "system", "container", "component", "operation", "process", or "model"
    kind: String,
    /// ID of the parent node. Required for container (parent=system), component (parent=container), operation (parent=component). Omit for person/system.
    parent_id: Option<String>,
    /// X position on canvas. Default: auto-grid based on sibling count.
    x: Option<f64>,
    /// Y position on canvas. Default: auto-grid based on sibling count.
    y: Option<f64>,
    /// Technology label (containers and components only), e.g. "REST API", "PostgreSQL"
    technology: Option<String>,
    /// Whether this is an external system (systems only)
    external: Option<bool>,
    /// Visual shape override: "rectangle", "cylinder", "pipe", "trapezoid", "bucket", "hexagon"
    shape: Option<String>,
    /// Source file locations as JSON array of {"pattern": "glob", "comment": "description"} objects. Pattern is a file glob (e.g. "src/auth/**/*.rs"), comment describes what those files do.
    sources: Option<Vec<scryer_core::Reference>>,
    /// Status: "implemented", "proposed", "changed", or "deprecated"
    status: Option<String>,
    /// Implementation contract: expect/ask/never rules
    contract: Option<scryer_core::Contract>,
    /// Acceptance criteria (done conditions)
    accepts: Option<Vec<String>>,
    /// Short rationale for why this node exists or is structured this way. Inherited by descendants via get_task.
    decisions: Option<String>,
    /// Properties (model-kind nodes only): label/description pairs
    properties: Option<Vec<ModelProperty>>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct AddNodeRequest {
    /// Name of the model to add nodes to
    model: String,
    /// Array of nodes to add
    nodes: Vec<AddNodeItem>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct UpdateNodeItem {
    /// ID of the node to update (e.g. "node-3")
    node_id: String,
    /// New display name
    name: Option<String>,
    /// New description (max 200 characters)
    description: Option<String>,
    /// New technology label
    technology: Option<String>,
    /// New external flag
    external: Option<bool>,
    /// New shape
    shape: Option<String>,
    /// New source file locations as JSON array of {"pattern": "glob", "comment": "description"} objects
    sources: Option<Vec<scryer_core::Reference>>,
    /// New X position
    x: Option<f64>,
    /// New Y position
    y: Option<f64>,
    /// New status: "implemented", "proposed", "changed", or "deprecated"
    status: Option<String>,
    /// Updated implementation contract
    contract: Option<scryer_core::Contract>,
    /// Updated acceptance criteria
    accepts: Option<Vec<String>>,
    /// Updated decisions (rationale for why this node exists or is structured this way)
    decisions: Option<String>,
    /// Updated properties (model-kind nodes only)
    properties: Option<Vec<ModelProperty>>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct UpdateNodeRequest {
    /// Name of the model
    model: String,
    /// Array of node updates to apply
    nodes: Vec<UpdateNodeItem>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct SetNodeRequest {
    /// Name of the model
    model: String,
    /// ID of the existing node to populate. All existing descendants are replaced.
    node_id: String,
    /// JSON object with "nodes" (array of descendant nodes to place inside node_id) and "edges" (array of edges). Every node must have a parentId chain leading to node_id. Node "type" defaults to "c4" and "position" defaults to auto-grid if omitted or (0,0). See set_model for the node/edge JSON format.
    data: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct DeleteNodeRequest {
    /// Name of the model
    model: String,
    /// IDs of nodes to delete. Each node's descendants and connected edges are also removed.
    node_ids: Vec<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct AddEdgeItem {
    /// Source node ID
    source: String,
    /// Target node ID
    target: String,
    /// Short relationship label (max 30 characters), e.g. "reads from", "sends events"
    label: String,
    /// Method/protocol, e.g. "REST/JSON", "gRPC"
    method: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct AddEdgeRequest {
    /// Name of the model
    model: String,
    /// Array of edges to add
    edges: Vec<AddEdgeItem>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct UpdateEdgeItem {
    /// ID of the edge to update
    edge_id: String,
    /// New label
    label: Option<String>,
    /// New method
    method: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct UpdateEdgeRequest {
    /// Name of the model
    model: String,
    /// Array of edge updates to apply
    edges: Vec<UpdateEdgeItem>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct DeleteEdgeRequest {
    /// Name of the model
    model: String,
    /// IDs of edges to delete
    edge_ids: Vec<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct SourceMapEntry {
    /// ID of the node to set source locations for
    node_id: String,
    /// Array of source locations. Each has "file" (path), optional "line", optional "end_line". Empty array clears.
    locations: Vec<SourceLocation>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct UpdateSourceMapRequest {
    /// Name of the model
    model: String,
    /// Array of source map entries to set
    entries: Vec<SourceMapEntry>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct GetChangesRequest {
    /// Name of the model to check for changes
    name: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct GetTaskRequest {
    /// Name of the model to derive tasks from
    name: String,
    /// Optional node ID to scope tasks to a subtree. If omitted, derives tasks for the entire model.
    node_id: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct SetFlowRequest {
    /// Name of the model
    model: String,
    /// One or more flows as a JSON string. Pass a single flow object or an array of flows. Each must have id, name, steps[], and transitions[]. Step IDs must be unique within each flow. Transition source/target must reference existing step IDs.
    data: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct DeleteFlowRequest {
    /// Name of the model
    model: String,
    /// ID of the flow to delete
    flow_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct SetGroupsRequest {
    /// Name of the model
    model: String,
    /// JSON string: a single group object or array of groups. Each group has: id, kind ("deployment" or "package"), name, memberIds (array of node IDs). Optional: description.
    data: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct DeleteGroupRequest {
    /// Name of the model
    model: String,
    /// ID of the group to delete
    group_id: String,
}

// --- Server ---

#[derive(Clone)]
pub struct ScryerServer {
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl ScryerServer {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }

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
        description = "Get the full JSON content of a model. Returns {nodes: [{id, parentId?, data: {name, description, kind, technology?, external?, shape?, status?, sources?, contract?, accepts?}}], edges: [{id, source, target, data: {label, method?}}], flows: [{id, name, description?, steps, transitions}], sourceMap: {nodeId: [{file, line?, endLine?}]}, contract?, startingLevel?}. Positions and node type are omitted (UI-only). For scoped reads, prefer get_node. For implementation, use get_task instead — it handles dependency ordering and returns one work unit at a time."
    )]
    fn get_model(
        &self,
        Parameters(req): Parameters<GetModelRequest>,
    ) -> Result<CallToolResult, McpError> {
        match scryer_core::read_model(&req.name) {
            Ok(model) => {
                let _ = scryer_core::save_baseline(&req.name, &model);
                let mut val = serde_json::to_value(&model).unwrap();
                strip_ui_fields(&mut val);

                externalize_attachments(&mut val, &req.name);
                let json = serde_json::to_string_pretty(&val)
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

    #[tool(
        description = "Create or overwrite a model with complete data in one call. Use for initial model creation or full rewrites. Pass the full model JSON with all nodes and edges. Node 'type' defaults to 'c4' and 'position' defaults to {x:0,y:0} if omitted — nodes at (0,0) are auto-laid out.\n\nJSON format:\n- Containers MUST have `parentId` set to a system node's ID. Components MUST have `parentId` set to a container's ID. Without `parentId`, nodes render as flat siblings instead of nested.\n- Include `sources`, `technology`, `shape`, and `status` directly in each node's data — do NOT add them in a separate pass.\n- `position` and `type` can be omitted (default to auto-grid and \"c4\").\n- Edge IDs follow the pattern `edge-{source}-{target}`.\n- Edge labels MUST be short (max 30 characters). One verb phrase per edge.\n\nExample:\n{\"nodes\": [\n  {\"id\": \"node-1\", \"data\": {\"name\": \"User\", \"description\": \"End user\", \"kind\": \"person\", \"status\": \"proposed\"}},\n  {\"id\": \"node-2\", \"data\": {\"name\": \"My System\", \"description\": \"Main system\", \"kind\": \"system\", \"status\": \"proposed\"}},\n  {\"id\": \"node-3\", \"parentId\": \"node-2\", \"data\": {\"name\": \"Web App\", \"description\": \"Frontend SPA\", \"kind\": \"container\", \"technology\": \"React\", \"status\": \"proposed\"}},\n  {\"id\": \"node-4\", \"parentId\": \"node-2\", \"data\": {\"name\": \"Database\", \"description\": \"Primary data store\", \"kind\": \"container\", \"technology\": \"PostgreSQL\", \"shape\": \"cylinder\", \"status\": \"proposed\"}}\n], \"edges\": [\n  {\"id\": \"edge-node-1-node-2\", \"source\": \"node-1\", \"target\": \"node-2\", \"data\": {\"label\": \"uses\"}},\n  {\"id\": \"edge-node-3-node-4\", \"source\": \"node-3\", \"target\": \"node-4\", \"data\": {\"label\": \"reads from\", \"method\": \"SQL\"}}\n]}"
    )]
    fn set_model(
        &self,
        Parameters(req): Parameters<SetModelRequest>,
    ) -> Result<CallToolResult, McpError> {
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
            if matches!(node.data.kind, C4Kind::Operation | C4Kind::Model) {
                if let Err(e) = validate_identifier(
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

        // Auto-layout nodes that have no position (defaulted to 0,0)
        for i in 0..model.nodes.len() {
            if model.nodes[i].position.x == 0.0 && model.nodes[i].position.y == 0.0 {
                let parent = model.nodes[i].parent_id.clone();
                let siblings = model.nodes[..i]
                    .iter()
                    .filter(|n| n.parent_id == parent)
                    .count();
                model.nodes[i].position.x = (siblings % 4) as f64 * 250.0 + 100.0;
                model.nodes[i].position.y = (siblings / 4) as f64 * 220.0 + 100.0;
            }
        }

        let node_count = model.nodes.len();
        let edge_count = model.edges.len();
        match scryer_core::write_model(&req.name, &model) {
            Ok(()) => {
                let _ = scryer_core::save_baseline(&req.name, &model);
                Ok(CallToolResult::success(vec![Content::text(format!(
                    "Set model '{}' ({} nodes, {} edges)",
                    req.name, node_count, edge_count
                ))]))
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
        let mut model = match scryer_core::read_model(&req.model) {
            Ok(m) => m,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to read model '{}': {}",
                    req.model, e
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

            if matches!(kind, C4Kind::Operation | C4Kind::Model) {
                if let Err(e) = validate_identifier(&item.name, &format!("{:?}", kind)) {
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
            let siblings = model
                .nodes
                .iter()
                .filter(|n| n.parent_id.as_deref() == item.parent_id.as_deref())
                .count();
            let x = item.x.unwrap_or((siblings % 4) as f64 * 250.0 + 100.0);
            let y = item.y.unwrap_or((siblings / 4) as f64 * 220.0 + 100.0);
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
                position: Position { x, y },
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
                    contract: item.contract.clone().unwrap_or_default(),
                    accepts: item.accepts.clone().unwrap_or_default(),
                    decisions: item.decisions.clone(),
                    properties: item.properties.clone().unwrap_or_default(),
                    attachments: Vec::new(),
                },
                parent_id: item.parent_id.clone(),
            });
            added_ids.push(id);
        }

        match scryer_core::write_model(&req.model, &model) {
            Ok(()) => {
                let _ = scryer_core::save_baseline(&req.model, &model);
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
        let mut model = match scryer_core::read_model(&req.model) {
            Ok(m) => m,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to read model '{}': {}",
                    req.model, e
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
            if matches!(node.data.kind, C4Kind::Operation | C4Kind::Model) {
                if let Err(e) = validate_identifier(
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

        // Auto-layout nodes at (0,0) and add to model
        let node_count = subtree.nodes.len();
        let edge_count = subtree.edges.len();
        let mut new_nodes = subtree.nodes;
        for i in 0..new_nodes.len() {
            if new_nodes[i].position.x == 0.0 && new_nodes[i].position.y == 0.0 {
                let parent = new_nodes[i].parent_id.clone();
                let siblings = new_nodes[..i]
                    .iter()
                    .filter(|n| n.parent_id == parent)
                    .count();
                new_nodes[i].position.x = (siblings % 4) as f64 * 250.0 + 100.0;
                new_nodes[i].position.y = (siblings / 4) as f64 * 220.0 + 100.0;
            }
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
        model.edges.extend(subtree.edges);

        match scryer_core::write_model(&req.model, &model) {
            Ok(()) => {
                let _ = scryer_core::save_baseline(&req.model, &model);
                Ok(CallToolResult::success(vec![Content::text(format!(
                    "Set {} descendant node(s) and {} edge(s) under '{}'",
                    node_count, edge_count, req.node_id
                ))]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e)])),
        }
    }

    #[tool(description = "Update properties of one or more existing nodes")]
    fn update_nodes(
        &self,
        Parameters(req): Parameters<UpdateNodeRequest>,
    ) -> Result<CallToolResult, McpError> {
        let mut model = match scryer_core::read_model(&req.model) {
            Ok(m) => m,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to read model '{}': {}",
                    req.model, e
                ))]));
            }
        };

        let mut updated = Vec::new();
        for item in req.nodes {
            let node = match model.nodes.iter_mut().find(|n| n.id == item.node_id) {
                Some(n) => n,
                None => {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "Node '{}' not found",
                        item.node_id
                    ))]));
                }
            };

            if let Some(name) = item.name {
                if matches!(node.data.kind, C4Kind::Operation | C4Kind::Model) {
                    if let Err(e) = validate_identifier(
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
            if let Some(x) = item.x {
                node.position.x = x;
            }
            if let Some(y) = item.y {
                node.position.y = y;
            }
            if let Some(s) = item.status {
                if node.data.kind != C4Kind::Person {
                    node.data.status = parse_status(&s);
                }
            }
            if let Some(g) = item.contract {
                node.data.contract = g;
            }
            if let Some(a) = item.accepts {
                node.data.accepts = a;
            }
            if let Some(d) = item.decisions {
                node.data.decisions = if d.is_empty() { None } else { Some(d) };
            }
            if let Some(p) = item.properties {
                if let Err(e) = validate_property_labels(&p, &format!("node '{}'", item.node_id)) {
                    return Ok(CallToolResult::error(vec![Content::text(e)]));
                }
                node.data.properties = p;
            }
            updated.push(item.node_id);
        }

        match scryer_core::write_model(&req.model, &model) {
            Ok(()) => {
                let _ = scryer_core::save_baseline(&req.model, &model);
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
        let mut model = match scryer_core::read_model(&req.model) {
            Ok(m) => m,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to read model '{}': {}",
                    req.model, e
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

        match scryer_core::write_model(&req.model, &model) {
            Ok(()) => {
                let _ = scryer_core::save_baseline(&req.model, &model);
                Ok(CallToolResult::success(vec![Content::text(format!(
                    "Deleted {} node(s)",
                    removed
                ))]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e)])),
        }
    }

    #[tool(description = "Add one or more relationship edges between nodes")]
    fn add_edges(
        &self,
        Parameters(req): Parameters<AddEdgeRequest>,
    ) -> Result<CallToolResult, McpError> {
        let mut model = match scryer_core::read_model(&req.model) {
            Ok(m) => m,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to read model '{}': {}",
                    req.model, e
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

        match scryer_core::write_model(&req.model, &model) {
            Ok(()) => {
                let _ = scryer_core::save_baseline(&req.model, &model);
                Ok(CallToolResult::success(vec![Content::text(format!(
                    "Added {} edge(s)",
                    added.len()
                ))]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e)])),
        }
    }

    #[tool(description = "Update one or more existing edges")]
    fn update_edges(
        &self,
        Parameters(req): Parameters<UpdateEdgeRequest>,
    ) -> Result<CallToolResult, McpError> {
        let mut model = match scryer_core::read_model(&req.model) {
            Ok(m) => m,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to read model '{}': {}",
                    req.model, e
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

        match scryer_core::write_model(&req.model, &model) {
            Ok(()) => {
                let _ = scryer_core::save_baseline(&req.model, &model);
                Ok(CallToolResult::success(vec![Content::text(format!(
                    "Updated {} edge(s)",
                    updated
                ))]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e)])),
        }
    }

    #[tool(description = "Get the C4 modeling rules that govern how diagrams should be structured")]
    fn get_rules(&self) -> Result<CallToolResult, McpError> {
        Ok(CallToolResult::success(vec![Content::text(
            scryer_core::rules::RULES,
        )]))
    }

    #[tool(description = "Delete one or more edges from the model")]
    fn delete_edges(
        &self,
        Parameters(req): Parameters<DeleteEdgeRequest>,
    ) -> Result<CallToolResult, McpError> {
        let mut model = match scryer_core::read_model(&req.model) {
            Ok(m) => m,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to read model '{}': {}",
                    req.model, e
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

        match scryer_core::write_model(&req.model, &model) {
            Ok(()) => {
                let _ = scryer_core::save_baseline(&req.model, &model);
                Ok(CallToolResult::success(vec![Content::text(format!(
                    "Deleted {} edge(s)",
                    req.edge_ids.len()
                ))]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e)])),
        }
    }

    #[tool(
        description = "Set source file locations for one or more nodes. Used to map operation nodes to their source code. Pass an empty locations array to clear a node's source map."
    )]
    fn update_source_map(
        &self,
        Parameters(req): Parameters<UpdateSourceMapRequest>,
    ) -> Result<CallToolResult, McpError> {
        let mut model = match scryer_core::read_model(&req.model) {
            Ok(m) => m,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to read model '{}': {}",
                    req.model, e
                ))]));
            }
        };

        for entry in &req.entries {
            if !model.nodes.iter().any(|n| n.id == entry.node_id) {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Node '{}' not found",
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

        match scryer_core::write_model(&req.model, &model) {
            Ok(()) => {
                let _ = scryer_core::save_baseline(&req.model, &model);
                Ok(CallToolResult::success(vec![Content::text(format!(
                    "Updated source map for {} node(s)",
                    count
                ))]))
            }
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

    #[tool(
        description = "Get the next implementation task. Returns one logical work unit at a time, ordered by dependencies. Workflow: call get_task → build the returned task → mark nodes as implemented via update_nodes → call get_task again for the next task. Pass node_id to scope to a subtree."
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

        // Helper: merge contract (additive — node extends model, no override)
        let merge_contract = |chain: &[&C4Node], node: &C4Node| -> Contract {
            let mut merged = model.contract.clone();
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

        // Helper: collect decisions from ancestors + node
        let collect_decisions = |chain: &[&C4Node], node: &C4Node| -> Vec<String> {
            let mut collected = Vec::new();
            for ancestor in chain {
                if let Some(d) = &ancestor.data.decisions {
                    collected.push(format!("{}: {}", ancestor.data.name, d));
                }
            }
            if let Some(d) = &node.data.decisions {
                collected.push(d.clone());
            }
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

        // Helper: check if all status-bearing children are implemented
        let children_all_implemented = |node: &C4Node| -> bool {
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
                .all(|n| matches!(n.data.status, Some(Status::Implemented)))
        };

        // Classify nodes: satisfied vs needs-work
        // For containers with component children (or systems with container children),
        // satisfaction requires ALL children to be implemented — not just the container itself.
        let is_satisfied = |node: &C4Node| -> bool {
            if node.data.external == Some(true) {
                return true;
            }
            if has_status_children(node) {
                return children_all_implemented(node);
            }
            matches!(node.data.status, Some(Status::Implemented) | None)
        };

        // Collect task-eligible nodes: containers and components (excluding deprecated and None-status)
        // Containers that have component children with status are NOT tasks themselves — their components are.
        let task_nodes: Vec<&C4Node> = model
            .nodes
            .iter()
            .filter(|n| {
                let eligible = matches!(n.data.kind, C4Kind::Container | C4Kind::Component);
                if !eligible {
                    return false;
                }
                // Skip deprecated nodes and None-status nodes (context/framework defaults)
                if matches!(n.data.status, Some(Status::Deprecated) | None) {
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

            // Check for containers/systems that should be marked implemented
            let mut propagate_nodes: Vec<(&str, &str)> = Vec::new(); // (id, name)
            for node in &model.nodes {
                if !matches!(node.data.kind, C4Kind::Container | C4Kind::System) {
                    continue;
                }
                if matches!(node.data.status, Some(Status::Implemented)) {
                    continue;
                }
                if !has_status_children(node) {
                    continue;
                }
                if children_all_implemented(node) {
                    propagate_nodes.push((&node.id, &node.data.name));
                }
            }

            if propagate_nodes.is_empty() {
                return Ok(CallToolResult::success(vec![Content::text(
                    format_done_message(&model),
                )]));
            }

            let mut output = format!(
                "All {} tasks complete.\n\nMark these parent nodes as implemented:\n```\nupdate_nodes(model: \"{}\", nodes: [{}])\n```",
                completed,
                req.name,
                propagate_nodes.iter()
                    .map(|(id, _)| format!("{{node_id: \"{}\", status: \"implemented\"}}", id))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            for (_, name) in &propagate_nodes {
                output.push_str(&format!("\n- {}", name));
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

                // Include contract/decisions from the containers
                for mc in &member_containers {
                    let ancestors = get_ancestor_chain(&mc.id);
                    let contract = merge_contract(&ancestors, mc);
                    let decisions = collect_decisions(&ancestors, mc);
                    output.push_str(&format_contract_and_decisions(
                        &mc.data.name, &contract, &decisions, &mc.data.accepts,
                    ));
                }

                output.push_str(&format!("\n---\n\n{}\n\n", TASK_INSTRUCTIONS));

                // Node IDs to mark implemented
                let ids: Vec<&str> = member_containers.iter().map(|n| n.id.as_str()).collect();
                output.push_str(&format!(
                    "After scaffolding, mark these as implemented:\n```\nupdate_nodes(model: \"{}\", nodes: [{}])\n```\n",
                    req.name,
                    ids.iter().map(|id| format!("{{node_id: \"{}\", status: \"implemented\"}}", id)).collect::<Vec<_>>().join(", ")
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
            let decisions = collect_decisions(&ancestors, node);

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

            // Acceptance criteria
            if !node.data.accepts.is_empty() {
                output.push_str("\nAcceptance criteria:\n");
                for ac in &node.data.accepts {
                    output.push_str(&format!("  - {}\n", ac));
                }
            }

            // Contract
            if !contract.is_empty() {
                output.push_str("\nContract:\n");
                if !contract.expect.is_empty() {
                    output.push_str("  EXPECTED:\n");
                    for item in &contract.expect {
                        output.push_str(&format!("    - {}\n", item));
                    }
                }
                if !contract.ask.is_empty() {
                    output.push_str("  ASK FIRST:\n");
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

            // Decisions
            if !decisions.is_empty() {
                output.push_str("\nDecisions:\n");
                for d in &decisions {
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

        // Mark-as-implemented hint
        let ids: Vec<&str> = work_unit.iter().map(|n| n.id.as_str()).collect();
        output.push_str(&format!(
            "After building, mark as implemented:\n```\nupdate_nodes(model: \"{}\", nodes: [{}])\n```\n",
            req.name,
            ids.iter().map(|id| format!("{{node_id: \"{}\", status: \"implemented\"}}", id)).collect::<Vec<_>>().join(", ")
        ));

        // Next up
        let next_name = find_next_name(&blocked_nodes, &ready_nodes, &work_unit);
        output.push_str(&format!(
            "\n---\nProgress: {}/{} tasks complete{}",
            completed_tasks, total_tasks,
            if let Some(name) = next_name { format!(" | Next up: {}", name) } else { String::new() }
        ));

        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    #[tool(
        description = "Create or replace one or more flows. Pass a single flow object or an array of flows — use an array to create multiple flows in one call. If a flow with the given ID exists, it is replaced; otherwise it is appended.\n\nFlows describe behavioral sequences — user journeys, data syncs, deploy pipelines, cron jobs. Each has steps (what happens) and transitions (ordering/branching between steps).\n\nStep granularity: each step = one meaningful system interaction, NOT a UI gesture. Good: 'System validates credentials'. Bad: 'User clicks button'.\n\nStep schema: {id, description, processIds?}. Use `description` for step text — `label` is auto-computed from DAG structure. Step IDs: 'step-N'. Flow IDs: 'scenario-N'.\n\nTransitions support forks: a step can have multiple outgoing transitions with different labels.\n\nSteps can reference process nodes via `processIds` array to connect flow behavior to C4 architecture. Not every step needs a link. The UI shows linked process names on step nodes."
    )]
    fn set_flows(
        &self,
        Parameters(req): Parameters<SetFlowRequest>,
    ) -> Result<CallToolResult, McpError> {
        let mut model = match scryer_core::read_model(&req.model) {
            Ok(m) => m,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to read model '{}': {}",
                    req.model, e
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

        let process_ids: HashSet<&str> = model
            .nodes
            .iter()
            .filter(|n| n.data.kind == C4Kind::Process)
            .map(|n| n.id.as_str())
            .collect();

        for flow in &flows {
            // Validate step ID uniqueness
            let mut step_ids = HashSet::new();
            for step in &flow.steps {
                if !step_ids.insert(&step.id) {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "Duplicate step ID '{}' in flow '{}'",
                        step.id, flow.name
                    ))]));
                }
            }

            // Validate transition source/target reference existing steps
            for t in &flow.transitions {
                if !step_ids.contains(&t.source) {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "Transition source '{}' not found in flow '{}' steps",
                        t.source, flow.name
                    ))]));
                }
                if !step_ids.contains(&t.target) {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "Transition target '{}' not found in flow '{}' steps",
                        t.target, flow.name
                    ))]));
                }
            }

            // Validate processIds reference existing process nodes
            for step in &flow.steps {
                for pid in &step.process_ids {
                    if !process_ids.contains(pid.as_str()) {
                        return Ok(CallToolResult::error(vec![Content::text(format!(
                            "Step '{}' references process '{}' which does not exist in the model",
                            step.id, pid
                        ))]));
                    }
                }
            }

            // Migrate: if a step has label but no description, move label → description
            // (AI agents often use "label" for step text, but the UI renders "description")
            let mut flow = flow.clone();
            for step in &mut flow.steps {
                if step.description.is_none() {
                    if let Some(lbl) = step.label.take() {
                        step.description = Some(lbl);
                    }
                }
            }

            // Replace or append
            if let Some(existing) = model.flows.iter_mut().find(|s| s.id == flow.id) {
                *existing = flow;
            } else {
                model.flows.push(flow);
            }
        }

        match scryer_core::write_model(&req.model, &model) {
            Ok(()) => {
                let _ = scryer_core::save_baseline(&req.model, &model);
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
        let mut model = match scryer_core::read_model(&req.model) {
            Ok(m) => m,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to read model '{}': {}",
                    req.model, e
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

        match scryer_core::write_model(&req.model, &model) {
            Ok(()) => {
                let _ = scryer_core::save_baseline(&req.model, &model);
                Ok(CallToolResult::success(vec![Content::text(format!(
                    "Deleted flow '{}'",
                    req.flow_id
                ))]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e)])),
        }
    }

    #[tool(
        description = "Create or replace one or more groups. Groups organize nodes visually on the canvas. If a group with the given ID exists, it is replaced; otherwise it is appended.\n\nTwo kinds:\n- \"deployment\": Groups containers inside a system. Represents what gets deployed together.\n- \"package\": Groups components inside a container. Represents how code should be organized.\n\nRules: A node can belong to at most one group. Deployment groups contain containers only. Package groups contain components only.\n\nGroup schema: {id, kind, name, memberIds, description?}. The `kind` must match the level: \"deployment\" for containers, \"package\" for components."
    )]
    fn set_groups(
        &self,
        Parameters(req): Parameters<SetGroupsRequest>,
    ) -> Result<CallToolResult, McpError> {
        let mut model = match scryer_core::read_model(&req.model) {
            Ok(m) => m,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to read model '{}': {}",
                    req.model, e
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

            // Validate kind matches the level of the members
            if let Some(first_member) = group.member_ids.first() {
                if let Some(member_node) = model.nodes.iter().find(|n| n.id == *first_member) {
                    let expected_kind = match member_node.data.kind {
                        C4Kind::Container => GroupKind::Deployment,
                        C4Kind::Component => GroupKind::Package,
                        _ => {
                            return Ok(CallToolResult::error(vec![Content::text(format!(
                                "Group '{}' contains {:?} nodes which cannot be grouped. Only containers (deployment groups) and components (package groups) can be grouped.",
                                group.name, member_node.data.kind
                            ))]));
                        }
                    };
                    if group.kind != expected_kind {
                        return Ok(CallToolResult::error(vec![Content::text(format!(
                            "Group '{}' has kind {:?} but contains {:?} nodes. Use {:?} for {:?} nodes.",
                            group.name, group.kind, member_node.data.kind, expected_kind, member_node.data.kind
                        ))]));
                    }
                }
            }

            // Replace or append
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
        match scryer_core::write_model(&req.model, &model) {
            Ok(()) => {
                let _ = scryer_core::save_baseline(&req.model, &model);
                Ok(CallToolResult::success(vec![Content::text(format!(
                    "Set {} group(s): {}",
                    count,
                    names.join(", ")
                ))]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e)])),
        }
    }

    #[tool(description = "Delete a group by ID. Members are ungrouped, not deleted.")]
    fn delete_group(
        &self,
        Parameters(req): Parameters<DeleteGroupRequest>,
    ) -> Result<CallToolResult, McpError> {
        let mut model = match scryer_core::read_model(&req.model) {
            Ok(m) => m,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to read model '{}': {}",
                    req.model, e
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

        match scryer_core::write_model(&req.model, &model) {
            Ok(()) => {
                let _ = scryer_core::save_baseline(&req.model, &model);
                Ok(CallToolResult::success(vec![Content::text(format!(
                    "Deleted group '{}'",
                    req.group_id
                ))]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e)])),
        }
    }
}

#[tool_handler]
impl ServerHandler for ScryerServer {
    fn get_info(&self) -> ServerInfo {
        let instructions = format!(
            "{}\n\n## C4 Modeling Rules\n{}",
            INSTRUCTIONS,
            scryer_core::rules::RULES
        );
        ServerInfo {
            instructions: Some(instructions.into()),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}

// --- Helpers ---

/// Recursively strip UI-only fields (position, type, refPositions) from a JSON value.
fn strip_ui_fields(val: &mut serde_json::Value) {
    match val {
        serde_json::Value::Object(map) => {
            map.remove("position");
            map.remove("type");
            map.remove("refPositions");
            for (_, v) in map.iter_mut() {
                strip_ui_fields(v);
            }
        }
        serde_json::Value::Array(arr) => {
            for v in arr.iter_mut() {
                strip_ui_fields(v);
            }
        }
        _ => {}
    }
}

/// Externalize attachment base64 data to temp files so AI context isn't bloated.
/// Walks the JSON looking for node objects with "attachments" arrays, writes each
/// attachment's data to a temp file, and replaces "data" with "path".
fn externalize_attachments(val: &mut serde_json::Value, model_name: &str) {
    match val {
        serde_json::Value::Object(map) => {
            // Check if this object has both "id" (node) and "attachments" (array)
            let node_id = map
                .get("id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            if let Some(ref nid) = node_id {
                if let Some(serde_json::Value::Object(data_map)) = map.get_mut("data") {
                    if let Some(serde_json::Value::Array(atts)) = data_map.get_mut("attachments") {
                        for att in atts.iter_mut() {
                            if let serde_json::Value::Object(att_map) = att {
                                let att_id = att_map
                                    .get("id")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("unknown")
                                    .to_string();
                                let mime = att_map
                                    .get("mimeType")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("image/png")
                                    .to_string();
                                let ext = match mime.as_str() {
                                    "image/jpeg" => "jpg",
                                    "image/gif" => "gif",
                                    "image/webp" => "webp",
                                    "image/svg+xml" => "svg",
                                    _ => "png",
                                };
                                if let Some(serde_json::Value::String(b64)) = att_map.remove("data")
                                {
                                    let filename =
                                        format!("scryer-{}-{}-{}.{}", model_name, nid, att_id, ext);
                                    let path = std::env::temp_dir().join(&filename);
                                    if let Ok(bytes) = base64_decode(&b64) {
                                        let _ = std::fs::write(&path, bytes);
                                    }
                                    att_map.insert(
                                        "path".to_string(),
                                        serde_json::Value::String(
                                            path.to_string_lossy().to_string(),
                                        ),
                                    );
                                }
                            }
                        }
                    }
                }
            }
            for (_, v) in map.iter_mut() {
                externalize_attachments(v, model_name);
            }
        }
        serde_json::Value::Array(arr) => {
            for v in arr.iter_mut() {
                externalize_attachments(v, model_name);
            }
        }
        _ => {}
    }
}

fn base64_decode(input: &str) -> Result<Vec<u8>, String> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(input)
        .map_err(|e| e.to_string())
}

fn parse_kind(s: &str) -> Result<C4Kind, McpError> {
    match s {
        "person" => Ok(C4Kind::Person),
        "system" => Ok(C4Kind::System),
        "container" => Ok(C4Kind::Container),
        "component" => Ok(C4Kind::Component),
        "operation" | "member" => Ok(C4Kind::Operation),
        "process" => Ok(C4Kind::Process),
        "model" => Ok(C4Kind::Model),
        _ => Err(McpError::invalid_params(
            format!(
                "Invalid kind '{}'. Must be: person, system, container, component, operation, process, model",
                s
            ),
            None,
        )),
    }
}

fn parse_status(s: &str) -> Option<Status> {
    match s {
        "implemented" => Some(Status::Implemented),
        "proposed" => Some(Status::Proposed),
        "changed" => Some(Status::Changed),
        "deprecated" | "removed" => Some(Status::Deprecated),
        _ => None,
    }
}

fn parse_shape(s: &str) -> Option<C4Shape> {
    match s {
        "rectangle" => Some(C4Shape::Rectangle),
        "person" => Some(C4Shape::Person),
        "cylinder" => Some(C4Shape::Cylinder),
        "pipe" => Some(C4Shape::Pipe),
        "trapezoid" => Some(C4Shape::Trapezoid),
        "bucket" => Some(C4Shape::Bucket),
        "hexagon" => Some(C4Shape::Hexagon),
        _ => None,
    }
}

fn validate_parent(
    model: &C4ModelData,
    kind: &C4Kind,
    parent_id: Option<&str>,
) -> Result<(), String> {
    match kind {
        C4Kind::Person | C4Kind::System => {
            if parent_id.is_some() {
                return Err("Person and system nodes must be top-level (no parent_id)".into());
            }
        }
        C4Kind::Container => {
            let pid =
                parent_id.ok_or("Container nodes require a parent_id (must be inside a system)")?;
            let parent = model
                .nodes
                .iter()
                .find(|n| n.id == pid)
                .ok_or(format!("Parent node '{}' not found", pid))?;
            if parent.data.kind != C4Kind::System {
                return Err(format!(
                    "Container parent must be a system, got {:?}",
                    parent.data.kind
                ));
            }
            if parent.data.external.unwrap_or(false) {
                return Err(format!(
                    "Cannot add containers inside external system '{}'. External systems are opaque and must not have child nodes. Instead, model each external service as its own top-level external system (e.g. separate 'S3' and 'Rekognition' systems instead of containers inside 'AWS').",
                    parent.data.name
                ));
            }
        }
        C4Kind::Component => {
            let pid = parent_id
                .ok_or("Component nodes require a parent_id (must be inside a container)")?;
            let parent = model
                .nodes
                .iter()
                .find(|n| n.id == pid)
                .ok_or(format!("Parent node '{}' not found", pid))?;
            if parent.data.kind != C4Kind::Container {
                return Err(format!(
                    "Component parent must be a container, got {:?}",
                    parent.data.kind
                ));
            }
        }
        C4Kind::Operation | C4Kind::Process | C4Kind::Model => {
            let label = kind_str(kind);
            let pid = parent_id.ok_or(format!(
                "{} nodes require a parent_id (must be inside a component)",
                label
            ))?;
            let parent = model
                .nodes
                .iter()
                .find(|n| n.id == pid)
                .ok_or(format!("Parent node '{}' not found", pid))?;
            if parent.data.kind != C4Kind::Component {
                return Err(format!(
                    "{} parent must be a component, got {:?}",
                    label, parent.data.kind
                ));
            }
        }
    }
    Ok(())
}

const TASK_INSTRUCTIONS: &str = "\
The spec above is your source of truth — it tells you WHAT to build. \
Trust your training knowledge for well-known frameworks and tools. \
Do not research standard framework setup — you already know how.

If something is unclear or the spec doesn't cover a decision you need to make, \
ask the user — don't spiral into web searches.

## After building
1. Mark ONLY the node(s) listed above as implemented using update_nodes.
2. Call get_task immediately to get the next task. Do NOT stop — there are more tasks.
3. Repeat until get_task returns \"All tasks complete.\"";

fn format_contract_and_decisions(
    name: &str,
    contract: &scryer_core::Contract,
    decisions: &[String],
    accepts: &[String],
) -> String {
    let mut out = String::new();
    if !accepts.is_empty() {
        out.push_str(&format!("\n{} — Acceptance criteria:\n", name));
        for ac in accepts {
            out.push_str(&format!("  - {}\n", ac));
        }
    }
    if !contract.is_empty() {
        out.push_str(&format!("\n{} — Contract:\n", name));
        if !contract.expect.is_empty() {
            out.push_str("  EXPECTED:\n");
            for item in &contract.expect {
                out.push_str(&format!("    - {}\n", item));
            }
        }
        if !contract.ask.is_empty() {
            out.push_str("  ASK FIRST:\n");
            for item in &contract.ask {
                out.push_str(&format!("    - {}\n", item));
            }
        }
        if !contract.never.is_empty() {
            out.push_str("  NEVER:\n");
            for item in &contract.never {
                out.push_str(&format!("    - {}\n", item));
            }
        }
    }
    if !decisions.is_empty() {
        out.push_str(&format!("\n{} — Decisions:\n", name));
        for d in decisions {
            out.push_str(&format!("  - {}\n", d));
        }
    }
    out
}

fn find_next_name<'a>(
    blocked: &[&'a scryer_core::C4Node],
    ready: &[&'a scryer_core::C4Node],
    current_unit: &[&scryer_core::C4Node],
) -> Option<&'a str> {
    // First check remaining ready nodes (not in current unit)
    let current_ids: std::collections::HashSet<&str> =
        current_unit.iter().map(|n| n.id.as_str()).collect();
    for n in ready {
        if !current_ids.contains(n.id.as_str()) {
            return Some(&n.data.name);
        }
    }
    // Then first blocked node
    blocked.first().map(|n| n.data.name.as_str())
}

fn format_done_message(model: &C4ModelData) -> String {
    if model.flows.is_empty() {
        return "All tasks complete. Nothing to build.".to_string();
    }

    let mut output = String::from("All tasks complete.\n\nPlease validate that the model's flows still accurately describe the system behavior. Update steps and process links as needed using `set_flows`.\n");

    let process_ids: HashSet<&str> = model.nodes.iter()
        .filter(|n| n.data.kind == C4Kind::Process)
        .map(|n| n.id.as_str())
        .collect();

    for flow in &model.flows {
        output.push_str(&format!("\n**{}** — {} steps\n", flow.name, flow.steps.len()));

        // Flag steps without process links
        let unlinked: Vec<&str> = flow.steps.iter()
            .filter(|s| s.process_ids.is_empty())
            .filter_map(|s| s.description.as_deref())
            .collect();
        if !unlinked.is_empty() {
            output.push_str("  Steps without process links:\n");
            for desc in &unlinked {
                output.push_str(&format!("  - {}\n", desc));
            }
        }

        // Flag broken process links
        let broken: Vec<(&str, &str)> = flow.steps.iter()
            .flat_map(|s| s.process_ids.iter().filter_map(|pid| {
                if !process_ids.contains(pid.as_str()) {
                    Some((s.description.as_deref().unwrap_or("(no description)"), pid.as_str()))
                } else {
                    None
                }
            }))
            .collect();
        if !broken.is_empty() {
            output.push_str("  Broken process links:\n");
            for (desc, pid) in &broken {
                output.push_str(&format!("  - \"{}\" references missing process {}\n", desc, pid));
            }
        }
    }

    output
}

fn kind_str(k: &C4Kind) -> &'static str {
    match k {
        C4Kind::Person => "person",
        C4Kind::System => "system",
        C4Kind::Container => "container",
        C4Kind::Component => "component",
        C4Kind::Operation => "operation",
        C4Kind::Process => "process",
        C4Kind::Model => "model",
    }
}

fn status_str(s: &Option<Status>) -> &'static str {
    match s {
        Some(Status::Implemented) => "implemented",
        Some(Status::Proposed) => "proposed",
        Some(Status::Changed) => "changed",
        Some(Status::Deprecated) => "deprecated",
        None => "none",
    }
}

fn shape_str(s: &Option<C4Shape>) -> &'static str {
    match s {
        Some(C4Shape::Rectangle) => "rectangle",
        Some(C4Shape::Person) => "person",
        Some(C4Shape::Cylinder) => "cylinder",
        Some(C4Shape::Pipe) => "pipe",
        Some(C4Shape::Trapezoid) => "trapezoid",
        Some(C4Shape::Bucket) => "bucket",
        Some(C4Shape::Hexagon) => "hexagon",
        None => "default",
    }
}

fn opt_str(s: &Option<String>) -> &str {
    s.as_deref().unwrap_or("none")
}

fn compute_diff(baseline: &C4ModelData, current: &C4ModelData) -> String {
    let base_nodes: HashMap<&str, &C4Node> =
        baseline.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    let curr_nodes: HashMap<&str, &C4Node> =
        current.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    let base_edges: HashMap<&str, &C4Edge> =
        baseline.edges.iter().map(|e| (e.id.as_str(), e)).collect();
    let curr_edges: HashMap<&str, &C4Edge> =
        current.edges.iter().map(|e| (e.id.as_str(), e)).collect();

    let mut sections: Vec<String> = Vec::new();

    // --- Nodes added ---
    let added: Vec<_> = current
        .nodes
        .iter()
        .filter(|n| !base_nodes.contains_key(n.id.as_str()))
        .collect();
    if !added.is_empty() {
        let mut lines = vec![format!("Nodes added ({}):", added.len())];
        for n in &added {
            let mut detail = format!(
                "  - {} \"{}\" ({})",
                n.id,
                n.data.name,
                kind_str(&n.data.kind)
            );
            if let Some(pid) = &n.parent_id {
                detail.push_str(&format!(", parent={}", pid));
            }
            if let Some(tech) = &n.data.technology {
                detail.push_str(&format!(", technology={}", tech));
            }
            if n.data.shape.is_some() {
                detail.push_str(&format!(", shape={}", shape_str(&n.data.shape)));
            }
            if n.data.status.is_some() {
                detail.push_str(&format!(", status={}", status_str(&n.data.status)));
            }
            if !n.data.description.is_empty() {
                let desc = if n.data.description.len() > 80 {
                    format!("{}...", &n.data.description[..77])
                } else {
                    n.data.description.clone()
                };
                detail.push_str(&format!(", description=\"{}\"", desc));
            }
            lines.push(detail);
        }
        sections.push(lines.join("\n"));
    }

    // --- Nodes removed ---
    let removed: Vec<_> = baseline
        .nodes
        .iter()
        .filter(|n| !curr_nodes.contains_key(n.id.as_str()))
        .collect();
    if !removed.is_empty() {
        let mut lines = vec![format!("Nodes removed ({}):", removed.len())];
        for n in &removed {
            lines.push(format!(
                "  - {} \"{}\" ({})",
                n.id,
                n.data.name,
                kind_str(&n.data.kind)
            ));
        }
        sections.push(lines.join("\n"));
    }

    // --- Nodes modified ---
    let mut mod_lines: Vec<String> = Vec::new();
    for (id, curr) in &curr_nodes {
        if let Some(base) = base_nodes.get(id) {
            let mut changes: Vec<String> = Vec::new();
            if base.data.name != curr.data.name {
                changes.push(format!(
                    "name \"{}\" -> \"{}\"",
                    base.data.name, curr.data.name
                ));
            }
            if base.data.description != curr.data.description {
                changes.push("description changed".to_string());
            }
            if base.data.kind != curr.data.kind {
                changes.push(format!(
                    "kind {} -> {}",
                    kind_str(&base.data.kind),
                    kind_str(&curr.data.kind)
                ));
            }
            if base.data.technology != curr.data.technology {
                changes.push(format!(
                    "technology {} -> {}",
                    opt_str(&base.data.technology),
                    opt_str(&curr.data.technology)
                ));
            }
            if base.data.external != curr.data.external {
                changes.push(format!(
                    "external {:?} -> {:?}",
                    base.data.external, curr.data.external
                ));
            }
            if base.data.shape != curr.data.shape {
                changes.push(format!(
                    "shape {} -> {}",
                    shape_str(&base.data.shape),
                    shape_str(&curr.data.shape)
                ));
            }
            if base.data.status != curr.data.status {
                changes.push(format!(
                    "status {} -> {}",
                    status_str(&base.data.status),
                    status_str(&curr.data.status)
                ));
            }
            if base.parent_id != curr.parent_id {
                changes.push(format!(
                    "parentId {} -> {}",
                    base.parent_id.as_deref().unwrap_or("none"),
                    curr.parent_id.as_deref().unwrap_or("none")
                ));
            }
            // Sources: compare by serialization
            if base.data.sources.len() != curr.data.sources.len()
                || base
                    .data
                    .sources
                    .iter()
                    .zip(curr.data.sources.iter())
                    .any(|(a, b)| a.pattern != b.pattern || a.comment != b.comment)
            {
                changes.push(format!(
                    "sources changed ({} -> {} entries)",
                    base.data.sources.len(),
                    curr.data.sources.len()
                ));
            }
            if base.data.contract != curr.data.contract {
                changes.push("contract changed".to_string());
            }
            if base.data.accepts != curr.data.accepts {
                changes.push("accepts changed".to_string());
            }
            if base.data.decisions != curr.data.decisions {
                changes.push("decisions changed".to_string());
            }
            if base.data.properties != curr.data.properties {
                changes.push(format!(
                    "properties changed ({} -> {} entries)",
                    base.data.properties.len(),
                    curr.data.properties.len()
                ));
            }
            if !changes.is_empty() {
                mod_lines.push(format!(
                    "  - {} (\"{}\"): {}",
                    id,
                    curr.data.name,
                    changes.join(", ")
                ));
            }
        }
    }
    if !mod_lines.is_empty() {
        sections.push(format!(
            "Nodes modified ({}):\n{}",
            mod_lines.len(),
            mod_lines.join("\n")
        ));
    }

    // --- Edges added ---
    let edges_added: Vec<_> = current
        .edges
        .iter()
        .filter(|e| !base_edges.contains_key(e.id.as_str()))
        .collect();
    if !edges_added.is_empty() {
        let mut lines = vec![format!("Edges added ({}):", edges_added.len())];
        for e in &edges_added {
            let label = e.data.as_ref().map(|d| d.label.as_str()).unwrap_or("");
            lines.push(format!(
                "  - {}: {} -> {} \"{}\"",
                e.id, e.source, e.target, label
            ));
        }
        sections.push(lines.join("\n"));
    }

    // --- Edges removed ---
    let edges_removed: Vec<_> = baseline
        .edges
        .iter()
        .filter(|e| !curr_edges.contains_key(e.id.as_str()))
        .collect();
    if !edges_removed.is_empty() {
        let mut lines = vec![format!("Edges removed ({}):", edges_removed.len())];
        for e in &edges_removed {
            let label = e.data.as_ref().map(|d| d.label.as_str()).unwrap_or("");
            lines.push(format!(
                "  - {}: {} -> {} \"{}\"",
                e.id, e.source, e.target, label
            ));
        }
        sections.push(lines.join("\n"));
    }

    // --- Edges modified ---
    let mut edge_mod_lines: Vec<String> = Vec::new();
    for (id, curr) in &curr_edges {
        if let Some(base) = base_edges.get(id) {
            let mut changes: Vec<String> = Vec::new();
            let base_data = base.data.as_ref();
            let curr_data = curr.data.as_ref();
            let base_label = base_data.map(|d| d.label.as_str()).unwrap_or("");
            let curr_label = curr_data.map(|d| d.label.as_str()).unwrap_or("");
            if base_label != curr_label {
                changes.push(format!("label \"{}\" -> \"{}\"", base_label, curr_label));
            }
            let base_method = base_data.and_then(|d| d.method.as_deref());
            let curr_method = curr_data.and_then(|d| d.method.as_deref());
            if base_method != curr_method {
                changes.push(format!(
                    "method {} -> {}",
                    base_method.unwrap_or("none"),
                    curr_method.unwrap_or("none")
                ));
            }
            if !changes.is_empty() {
                edge_mod_lines.push(format!("  - {}: {}", id, changes.join(", ")));
            }
        }
    }
    if !edge_mod_lines.is_empty() {
        sections.push(format!(
            "Edges modified ({}):\n{}",
            edge_mod_lines.len(),
            edge_mod_lines.join("\n")
        ));
    }

    // --- Model-level contract ---
    if baseline.contract != current.contract {
        let mut gl_changes: Vec<String> = Vec::new();
        if baseline.contract.expect != current.contract.expect {
            gl_changes.push("expect updated".to_string());
        }
        if baseline.contract.ask != current.contract.ask {
            gl_changes.push("ask updated".to_string());
        }
        if baseline.contract.never != current.contract.never {
            gl_changes.push("never updated".to_string());
        }
        sections.push(format!(
            "Model contract changed: {}",
            gl_changes.join(", ")
        ));
    }

    // --- Flows ---
    let base_flows: HashMap<&str, &Flow> =
        baseline.flows.iter().map(|s| (s.id.as_str(), s)).collect();
    let curr_flows: HashMap<&str, &Flow> =
        current.flows.iter().map(|s| (s.id.as_str(), s)).collect();

    let flows_added: Vec<_> = current
        .flows
        .iter()
        .filter(|s| !base_flows.contains_key(s.id.as_str()))
        .collect();
    if !flows_added.is_empty() {
        let mut lines = vec![format!("Flows added ({}):", flows_added.len())];
        for s in &flows_added {
            lines.push(format!(
                "  - {} \"{}\" ({} steps, {} transitions)",
                s.id,
                s.name,
                s.steps.len(),
                s.transitions.len()
            ));
        }
        sections.push(lines.join("\n"));
    }

    let flows_removed: Vec<_> = baseline
        .flows
        .iter()
        .filter(|s| !curr_flows.contains_key(s.id.as_str()))
        .collect();
    if !flows_removed.is_empty() {
        let mut lines = vec![format!("Flows removed ({}):", flows_removed.len())];
        for s in &flows_removed {
            lines.push(format!("  - {} \"{}\"", s.id, s.name));
        }
        sections.push(lines.join("\n"));
    }

    let mut flow_mod_lines: Vec<String> = Vec::new();
    for (id, curr) in &curr_flows {
        if let Some(base) = base_flows.get(id) {
            if base != curr {
                let mut changes: Vec<String> = Vec::new();
                if base.name != curr.name {
                    changes.push(format!("name \"{}\" -> \"{}\"", base.name, curr.name));
                }
                if base.steps.len() != curr.steps.len() {
                    changes.push(format!(
                        "steps {} -> {}",
                        base.steps.len(),
                        curr.steps.len()
                    ));
                }
                if base.transitions.len() != curr.transitions.len() {
                    changes.push(format!(
                        "transitions {} -> {}",
                        base.transitions.len(),
                        curr.transitions.len()
                    ));
                }
                if base.description != curr.description {
                    changes.push("description changed".to_string());
                }
                if !changes.is_empty() {
                    flow_mod_lines.push(format!(
                        "  - {} (\"{}\"): {}",
                        id,
                        curr.name,
                        changes.join(", ")
                    ));
                }
            }
        }
    }
    if !flow_mod_lines.is_empty() {
        sections.push(format!(
            "Flows modified ({}):\n{}",
            flow_mod_lines.len(),
            flow_mod_lines.join("\n")
        ));
    }

    if sections.is_empty() {
        "No changes since last seen.".to_string()
    } else {
        sections.join("\n\n")
    }
}

const INSTRUCTIONS: &str = r#"scryer is a C4 architecture diagramming tool. You are editing C4 model diagrams stored as .scry files (JSON format).

## C4 Hierarchy
- **Person**: A user or actor. Top-level node (no parent).
- **System**: A software system. Top-level node (no parent). Can be marked `external: true`.
- **Container**: An application, data store, or service inside a system. Parent must be a system node.
- **Component**: A logical component inside a container. Parent must be a container node.
- **Operation**: A behavioral unit (function, class, service, handler) inside a component. Parent must be a component node. **Name must be a valid identifier** (camelCase or snake_case).
- **Process**: A behavioral flow inside a component. Parent must be a component node. Use `type: "process"` in node data.
- **Model**: A data model inside a component. Parent must be a component node. Has optional `properties` (array of `{label, description}`). Use `type: "model"` in node data. **Name and property labels must be valid identifiers.**

## Node Types
All nodes use type `"c4"`, except: operation uses `"operation"`, process uses `"process"`, model uses `"model"`.

## Naming Rules
Operation, process, and model names must be valid identifiers: start with a lowercase letter, then `[a-zA-Z0-9_]`. Use camelCase or snake_case. Model property labels follow the same rule.

## Source Map
The model has an optional `sourceMap` field: a mapping from node ID to an array of source locations (`{file, line?, endLine?}`). Use `update_source_map` to attach file/line references to operation nodes. This is separate from `sources` (glob patterns on higher-level nodes).

## Status
Set status on nodes that represent work. Omit status for framework defaults that require no implementation effort. Nodes without status are context — visible but not actionable by `get_task`. Edges do not have status — edge color is inferred from endpoint nodes in the UI.

- **"implemented"** (green): Exists in the codebase and works.
- **"proposed"** (blue): Brand new — doesn't exist yet.
- **"changed"** (yellow): Exists but needs modification.
- **"deprecated"** (red): Technical debt — should be removed or replaced.

**Container/system status propagates upward**: when all component children of a container are implemented, `get_task` will prompt you to mark the container as implemented. Same for systems when all containers are done.

## IDs
Node IDs: "node-N" (auto-generated). Edge IDs: "edge-{source}-{target}". Use `get_model` to discover existing IDs.

## Modeling workflow
Call `get_rules` before creating or editing a model — it contains the full modeling workflow and C4 rules.

## Implementation workflow
When building code from a model, use `get_task` in a loop. Each call returns one work unit with dependency ordering, contract inheritance, and progress tracking built in.
1. Call `get_task` to get one work unit.
2. Build what the task describes. A scaffold task may cover multiple nodes at once — that's fine.
3. Mark the node(s) as implemented via `update_nodes`. Only mark nodes listed in the task.
4. **Call `get_task` again immediately.** Do not stop after one task — there are always more until it returns "All tasks complete."
The task system tracks what's done and what's next. Do not read the full model via `get_model` to derive your own implementation order."#;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Handle `scryer-mcp init` subcommand
    if std::env::args().nth(1).as_deref() == Some("init") {
        return init_project();
    }

    let service = ScryerServer::new()
        .serve(rmcp::transport::io::stdio())
        .await
        .inspect_err(|e| eprintln!("MCP server error: {}", e))?;
    service.waiting().await?;
    Ok(())
}

/// Write project-scoped MCP config files in the current directory so that
/// Claude Code and/or Codex discover scryer-mcp when working in this project.
/// Only writes config for tools that are actually installed.
fn init_project() -> Result<(), Box<dyn std::error::Error>> {
    let binary_path = std::env::current_exe()?
        .canonicalize()?
        .to_string_lossy()
        .to_string();

    let cwd = std::env::current_dir()?;

    let has_claude = which("claude");
    let has_codex = which("codex");

    if !has_claude && !has_codex {
        eprintln!("Neither `claude` nor `codex` found in PATH.");
        eprintln!("Install Claude Code or OpenAI Codex first, then re-run `scryer-mcp init`.");
        std::process::exit(1);
    }

    let mut wrote_any = false;

    if has_claude {
        init_claude_code(&cwd, &binary_path)?;
        wrote_any = true;
    }

    if has_codex {
        init_codex(&cwd, &binary_path)?;
        wrote_any = true;
    }

    if wrote_any {
        let tools: Vec<&str> = [
            if has_claude { Some("Claude Code") } else { None },
            if has_codex { Some("Codex") } else { None },
        ].into_iter().flatten().collect();
        eprintln!("\nDone. {} will use scryer in this project.", tools.join(" and "));
    }

    Ok(())
}

fn which(name: &str) -> bool {
    // Check PATH for the given binary
    std::env::var_os("PATH")
        .map(|paths| {
            std::env::split_paths(&paths).any(|dir| {
                let candidate = dir.join(name);
                candidate.is_file() || dir.join(format!("{name}.exe")).is_file()
            })
        })
        .unwrap_or(false)
}

/// Write .mcp.json for Claude Code, merging with any existing config.
fn init_claude_code(
    cwd: &std::path::Path,
    binary_path: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let mcp_json_path = cwd.join(".mcp.json");
    let mut root: serde_json::Value = if mcp_json_path.exists() {
        let contents = std::fs::read_to_string(&mcp_json_path)?;
        serde_json::from_str(&contents).unwrap_or_else(|_| serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    if !root.get("mcpServers").is_some_and(|v| v.is_object()) {
        root["mcpServers"] = serde_json::json!({});
    }
    root["mcpServers"]["scryer"] = serde_json::json!({
        "type": "stdio",
        "command": binary_path,
        "args": [],
    });

    std::fs::write(&mcp_json_path, serde_json::to_string_pretty(&root)?)?;
    eprintln!("Wrote {}", mcp_json_path.display());
    Ok(())
}

/// Write .codex/config.toml for OpenAI Codex, merging with any existing config.
fn init_codex(
    cwd: &std::path::Path,
    binary_path: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let codex_dir = cwd.join(".codex");
    let config_toml_path = codex_dir.join("config.toml");

    let mut doc: toml_edit::DocumentMut = if config_toml_path.exists() {
        std::fs::read_to_string(&config_toml_path)?
            .parse()
            .unwrap_or_default()
    } else {
        toml_edit::DocumentMut::new()
    };

    if !doc.contains_table("mcp_servers") {
        doc["mcp_servers"] = toml_edit::Item::Table(toml_edit::Table::new());
    }

    let mut server = toml_edit::Table::new();
    server.insert("command", toml_edit::value(binary_path));
    server.insert("args", toml_edit::value(toml_edit::Array::new()));
    doc["mcp_servers"]["scryer"] = toml_edit::Item::Table(server);

    std::fs::create_dir_all(&codex_dir)?;
    std::fs::write(&config_toml_path, doc.to_string())?;
    eprintln!("Wrote {}", config_toml_path.display());
    Ok(())
}
