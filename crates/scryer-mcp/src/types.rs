use scryer_core::{Responsibility, SchemaProperty, Source, SourceLocation};
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
    /// The node's identity in a few words — what it IS as software, NOT a summary of its responsibilities (those are a
    /// separate field). If it reads as a comma-list of the responsibilities, omit it. Optional.
    pub description: Option<String>,
    /// Node kind: "person", "system", "container", "component", or "symbol".
    pub kind: String,
    /// ID of the parent node. Required for container/component/symbol; omit for person/system.
    pub parent_id: Option<String>,
    /// Technology label — what the node IS as software (e.g. "Payload 3.0", "PostgreSQL 16"). Not for persons.
    pub technology: Option<String>,
    /// Whether this is an external system (systems/containers only).
    pub external: Option<bool>,
    /// Pure business-responsibility statements — one terse verb-led clause each. Lead with the distinguishing verb + object,
    /// then stop: no mechanism vocabulary, no trailing "by/where/so that" tails, no repeating the obvious domain on every line.
    pub responsibilities: Option<Vec<Responsibility>>,
    /// Field declarations, when this symbol defines a data shape (struct, class,
    /// interface, type). A symbol may carry both responsibilities and properties.
    pub properties: Option<Vec<SchemaProperty>>,
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
    /// Node kind: "person", "system", "container", "component", or "symbol".
    pub kind: Option<String>,
    pub name: Option<String>,
    pub description: Option<String>,
    pub technology: Option<String>,
    pub external: Option<bool>,
    /// Full replacement of responsibilities. Pass an empty array to clear.
    pub responsibilities: Option<Vec<Responsibility>>,
    /// Full replacement of field declarations for a data-shape symbol. Pass an empty array to clear.
    pub properties: Option<Vec<SchemaProperty>>,
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
    /// ID of the responsibility to set line-precise source locations for.
    pub responsibility_id: String,
    /// Source locations. Empty array clears the entry.
    pub locations: Vec<SourceLocation>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct BoundaryEntry {
    /// ID of the node to set boundary globs for.
    pub node_id: String,
    /// Boundary globs (the code region this node owns). Empty array clears it.
    pub sources: Vec<Source>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct SchemaSourceEntry {
    /// ID of the schema-kind node to set its declaration location for.
    pub node_id: String,
    /// Where the type is declared — normally one location: `pattern` = file,
    /// `symbol` = the type name, `line`/`endLine` = the declaration range. Empty
    /// array clears the entry.
    pub locations: Vec<SourceLocation>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct UpdateSourceMapRequest {
    pub project: Option<String>,
    /// Line-precise locations keyed by responsibility id.
    #[serde(default)]
    pub entries: Vec<SourceMapEntry>,
    /// Declaration locations keyed by schema node id.
    #[serde(default)]
    pub schemas: Vec<SchemaSourceEntry>,
    /// Boundary globs keyed by node id.
    #[serde(default)]
    pub boundaries: Vec<BoundaryEntry>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct SetGroupsRequest {
    pub project: Option<String>,
    /// JSON: a single group object or an array of groups. Each group has id, name, and memberIds
    /// (the node ids it groups — never leave empty). Set `parentNodeId` to the node whose children
    /// the members are: it anchors the group to that node's level so it renders inside that node's
    /// diagram (e.g. a deployment group over containers needs parentNodeId set to their parent system).
    /// Optional: description, parentGroupId (to nest under another group), responsibilities, cell, size.
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

