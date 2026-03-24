use scryer_core::{Contract, ModelProperty, SourceLocation};
use serde::Deserialize;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct GetModelRequest {
    /// Name of the model to retrieve. If omitted, resolves the model linked to the current working directory.
    #[serde(alias = "model")]
    pub name: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct GetNodeRequest {
    /// Name of the model
    #[serde(alias = "model")]
    pub name: String,
    /// ID of the node to inspect (e.g. "node-3"). Returns this node, all its descendants, edges between them, and edges connecting them to external nodes (with external node names for context).
    pub node_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct SetModelRequest {
    /// Name of the model to create or overwrite
    #[serde(alias = "model")]
    pub name: String,
    /// The complete model as a JSON string. Must be a valid C4ModelData object with nodes, edges, and optional startingLevel. See get_model output for the exact schema.
    pub data: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct AddNodeItem {
    /// Display name for the node
    pub name: String,
    /// Description of what this node does or represents (max 200 characters, no limit for operation/process/model nodes)
    pub description: String,
    /// Node kind: "person", "system", "container", "component", "operation", "process", or "model"
    pub kind: String,
    /// ID of the parent node. Required for container (parent=system), component (parent=container), operation (parent=component). Omit for person/system.
    pub parent_id: Option<String>,
    /// Technology label (containers and components only, max 28 characters), e.g. "REST API", "PostgreSQL"
    pub technology: Option<String>,
    /// Whether this is an external system (systems only)
    pub external: Option<bool>,
    /// Visual shape override: "rectangle", "cylinder", "pipe", "trapezoid", "bucket", "hexagon"
    pub shape: Option<String>,
    /// Source file locations as JSON array of {"pattern": "glob", "comment": "description"} objects. Pattern is a file glob (e.g. "src/auth/**/*.rs"), comment describes what those files do.
    pub sources: Option<Vec<scryer_core::Reference>>,
    /// Status: "proposed", "implemented", "verified", or "vagrant"
    pub status: Option<String>,
    /// Implementation contract: expect/ask/never rules
    pub contract: Option<Contract>,
    /// Freeform notes (array of strings): conventions, context, rationale. Inherited by descendants via get_task.
    pub notes: Option<Vec<String>>,
    /// Properties (model-kind nodes only): label/description pairs
    pub properties: Option<Vec<ModelProperty>>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct AddNodeRequest {
    /// Name of the model to add nodes to
    pub model: String,
    /// Array of nodes to add
    pub nodes: Vec<AddNodeItem>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct UpdateNodeItem {
    /// ID of the node to update (e.g. "node-3")
    pub node_id: String,
    /// New display name
    pub name: Option<String>,
    /// New description (max 200 characters)
    pub description: Option<String>,
    /// New technology label (max 28 characters)
    pub technology: Option<String>,
    /// New external flag
    pub external: Option<bool>,
    /// New shape
    pub shape: Option<String>,
    /// New source file locations as JSON array of {"pattern": "glob", "comment": "description"} objects
    pub sources: Option<Vec<scryer_core::Reference>>,
    /// New status: "proposed", "implemented", "verified", or "vagrant". "verified" requires all inherited expect contract items to have passed: true.
    pub status: Option<String>,
    /// Required when changing status. State what's still missing or what was just completed — e.g. "Needs auth middleware and rate limiting", "Missing error handling". For verified: "All contract items pass". Keep it short and factual.
    pub reason: Option<String>,
    /// Updated implementation contract
    pub contract: Option<Contract>,
    /// Updated notes (array of strings): conventions, context, rationale
    pub notes: Option<Vec<String>>,
    /// Updated properties (model-kind nodes only)
    pub properties: Option<Vec<ModelProperty>>,
    /// Source code location(s) for this node. Sets the source map entry.
    /// Example: [{"pattern": "src/auth/handler.ts", "line": 15, "endLine": 42}]
    /// For containers/components, a glob: [{"pattern": "src/auth/**/*.ts"}]
    /// Pass an empty array to clear.
    pub source: Option<Vec<SourceLocation>>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct UpdateNodeRequest {
    /// Name of the model
    pub model: String,
    /// Array of node updates to apply
    pub nodes: Vec<UpdateNodeItem>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct SetNodeRequest {
    /// Name of the model
    pub model: String,
    /// ID of the existing node to populate. All existing descendants are replaced.
    pub node_id: String,
    /// JSON object with "nodes" (array of descendant nodes to place inside node_id) and "edges" (array of edges). Every node must have a parentId chain leading to node_id. Node "type" defaults to "c4" and "position" is auto-laid out if omitted. See set_model for the node/edge JSON format.
    pub data: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct DeleteNodeRequest {
    /// Name of the model
    pub model: String,
    /// IDs of nodes to delete. Each node's descendants and connected edges are also removed.
    pub node_ids: Vec<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct AddEdgeItem {
    /// Source node ID
    pub source: String,
    /// Target node ID
    pub target: String,
    /// Short relationship label (max 30 characters), e.g. "reads from", "sends events"
    pub label: String,
    /// Method/protocol, e.g. "REST/JSON", "gRPC"
    pub method: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct AddEdgeRequest {
    /// Name of the model
    pub model: String,
    /// Array of edges to add
    pub edges: Vec<AddEdgeItem>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct UpdateEdgeItem {
    /// ID of the edge to update
    pub edge_id: String,
    /// New label
    pub label: Option<String>,
    /// New method
    pub method: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct UpdateEdgeRequest {
    /// Name of the model
    pub model: String,
    /// Array of edge updates to apply
    pub edges: Vec<UpdateEdgeItem>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct DeleteEdgeRequest {
    /// Name of the model
    pub model: String,
    /// IDs of edges to delete
    pub edge_ids: Vec<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct SourceMapEntry {
    /// ID of the node or flow to set source locations for
    pub node_id: String,
    /// Array of source locations. Each has "pattern" (glob), optional "line", optional "endLine". Empty array clears.
    pub locations: Vec<SourceLocation>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct UpdateSourceMapRequest {
    /// Name of the model
    pub model: String,
    /// Array of source map entries to set
    pub entries: Vec<SourceMapEntry>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct GetChangesRequest {
    /// Name of the model to check for changes
    #[serde(alias = "model")]
    pub name: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct GetTaskRequest {
    /// Name of the model to derive tasks from
    #[serde(alias = "model")]
    pub name: String,
    /// Optional node ID to scope tasks to a subtree. If omitted, derives tasks for the entire model.
    pub node_id: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct SetFlowRequest {
    /// Name of the model
    pub model: String,
    /// One or more flows as a JSON string. Pass a single flow object or an array of flows. Each must have id, name, steps[]. Step IDs must be unique within each flow. Steps can have branches[] for decision points. Transition source/target must reference existing step IDs.
    pub data: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct DeleteFlowRequest {
    /// Name of the model
    pub model: String,
    /// ID of the flow to delete
    pub flow_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct SetGroupsRequest {
    /// Name of the model
    pub model: String,
    /// JSON string: a single group object or array of groups. Each group has: id, kind ("deployment" or "package"), name, memberIds (array of node IDs). Optional: description, contract (same format as node contracts: {expect, ask, never}).
    pub data: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct DeleteGroupRequest {
    /// Name of the model
    pub model: String,
    /// ID of the group to delete
    pub group_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct GetStructureRequest {
    /// Absolute path to the project directory to scan
    pub path: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct SetImplementingRequest {
    /// Name of the model
    pub model: String,
    /// true to suppress drift detection, false to resume it
    pub active: bool,
}
