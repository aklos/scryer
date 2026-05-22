use scryer_core::{ModelProperty, Responsibility, Source, SourceLocation};
use serde::Deserialize;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct GetModelRequest {
    /// Absolute path to the project root. If omitted, uses the current working directory.
    pub project: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct GetNodeRequest {
    /// Absolute path to the project root. If omitted, uses the current working directory.
    pub project: Option<String>,
    /// ID of the node to inspect (e.g. "node-3"). Returns this node, all its descendants, links between them,
    /// and links connecting them to external nodes (with external node names + kinds for context).
    pub node_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct GetChangesRequest {
    pub project: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct GetStructureRequest {
    /// Absolute path to the project directory to scan.
    pub path: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct ValidateModelRequest {
    pub project: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct SetModelRequest {
    pub project: Option<String>,
    /// The complete model as a JSON string. Must be a valid ScryModel object with version, nodes, links, groups.
    /// See get_model output for the exact schema.
    pub data: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct AddNodeItem {
    /// Display name for the node.
    pub name: String,
    /// Short description of what this node IS — its identity, not its responsibilities. Optional.
    pub description: Option<String>,
    /// Node kind: "person", "system", "container", "component", "operation", or "model".
    pub kind: String,
    /// ID of the parent node. Required for container/component/operation/model; omit for person/system.
    pub parent_id: Option<String>,
    /// Technology label — what the node IS as software (e.g. "Payload 3.0", "PostgreSQL 16"). Not for persons.
    pub technology: Option<String>,
    /// Whether this is an external system (systems/containers only).
    pub external: Option<bool>,
    /// Source-file globs attached to this node.
    pub sources: Option<Vec<Source>>,
    /// Pure business-responsibility statements. No mechanism vocabulary in the text.
    pub responsibilities: Option<Vec<Responsibility>>,
    /// Properties (model-kind nodes only).
    pub properties: Option<Vec<ModelProperty>>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct AddNodeRequest {
    pub project: Option<String>,
    /// Array of nodes to add.
    pub nodes: Vec<AddNodeItem>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct UpdateNodeItem {
    /// ID of the node to update.
    pub node_id: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub technology: Option<String>,
    pub external: Option<bool>,
    pub sources: Option<Vec<Source>>,
    /// Full replacement of responsibilities. Pass an empty array to clear.
    pub responsibilities: Option<Vec<Responsibility>>,
    /// Full replacement of properties (model-kind nodes only).
    pub properties: Option<Vec<ModelProperty>>,
    /// Source-map locations for this node (line-precise). Pass an empty array to clear.
    /// Glob-style source pointers go in `sources` instead.
    pub source: Option<Vec<SourceLocation>>,
    /// Mark node as planned for removal. Set true to deprecate, false to clear.
    pub deprecated: Option<bool>,
    /// Mark node as reparented (code needs to move). Set true to flag, false to clear.
    pub relocated: Option<bool>,
    /// New parent node ID. Changes the node's parent (reparent operation).
    pub parent_id: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct MoveResponsibilityItem {
    /// ID of the responsibility to move.
    pub responsibility_id: String,
    /// Node ID to move the responsibility from.
    pub from_node_id: String,
    /// Node ID to move the responsibility to.
    pub to_node_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct MoveResponsibilitiesRequest {
    pub project: Option<String>,
    /// Array of moves to perform.
    pub moves: Vec<MoveResponsibilityItem>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct UpdateNodeRequest {
    pub project: Option<String>,
    pub nodes: Vec<UpdateNodeItem>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct SetNodeRequest {
    pub project: Option<String>,
    /// ID of the existing node whose subtree is being replaced.
    pub node_id: String,
    /// JSON object with "nodes" (array of descendants, all rooted at node_id) and "links" (array of links
    /// between any of those nodes or to nodes outside the subtree). All previously-existing descendants
    /// of node_id are removed before the new ones are inserted.
    pub data: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct DeleteNodeRequest {
    pub project: Option<String>,
    /// IDs of nodes to delete. Descendants and connected links are also removed.
    pub node_ids: Vec<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct AddLinkItem {
    /// Source node ID.
    pub src: String,
    /// Destination node ID.
    pub dst: String,
    /// Short relationship label (max 30 characters), e.g. "reads from", "sends events".
    pub label: String,
    /// Method/protocol annotation, e.g. "REST/JSON", "gRPC".
    pub method: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct AddLinkRequest {
    pub project: Option<String>,
    pub links: Vec<AddLinkItem>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct UpdateLinkItem {
    /// ID of the link to update.
    pub link_id: String,
    pub label: Option<String>,
    pub method: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct UpdateLinkRequest {
    pub project: Option<String>,
    pub links: Vec<UpdateLinkItem>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct DeleteLinkRequest {
    pub project: Option<String>,
    pub link_ids: Vec<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct SourceMapEntry {
    /// ID of the node to set source locations for.
    pub node_id: String,
    /// Source locations. Empty array clears the entry.
    pub locations: Vec<SourceLocation>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct UpdateSourceMapRequest {
    pub project: Option<String>,
    pub entries: Vec<SourceMapEntry>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct SetGroupsRequest {
    pub project: Option<String>,
    /// JSON: a single group object or an array of groups. Each group has id, name, memberIds.
    /// Optional: description, parentGroupId, responsibilities, cell, size.
    pub data: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct DeleteGroupRequest {
    pub project: Option<String>,
    pub group_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct SetImplementingRequest {
    pub project: Option<String>,
    /// true to suppress drift detection while implementing, false to resume.
    pub active: bool,
}

