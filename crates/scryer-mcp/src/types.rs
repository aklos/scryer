use scryer_core::{Responsibility, SchemaProperty, Source, SourceLocation};
use serde::Deserialize;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct ReadModelRequest {
    /// Absolute path to the project root. If omitted, uses the current working directory.
    pub project: Option<String>,
    /// Optional node id to scope to. OMIT for the architecture overview — the whole tree down to
    /// components (symbols excluded), with counts; small and safe to read. Pass a node id to read
    /// THAT node's full subtree: its descendants (including symbols), responsibilities, properties,
    /// links, and source anchors. Drill into a component to see its symbols.
    pub node: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct SearchModelRequest {
    /// Absolute path to the project root. If omitted, uses the current working directory.
    pub project: Option<String>,
    /// Case-insensitive text to find. Matched against node names, descriptions, technology,
    /// responsibility statements, and property labels. Space-separated terms must ALL match
    /// (AND) somewhere on the node.
    pub query: String,
    /// Optional kind filter: "person", "system", "container", "component", or "symbol".
    pub kind: Option<String>,
}

/// One predicate in a `query_model` request: a `field`, a comparison `op`, and
/// (for everything but `exists`/`absent`) a `value`. Fields and operators are
/// orthogonal — compose them into any node-shape question.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct QueryCondition {
    /// Queryable node field. One of:
    /// `kind`, `name`, `description`, `technology` (strings);
    /// `external`, `relocated`, `visual`, `empty`, `vagrant` (booleans —
    /// `empty` = a symbol with no responsibility/property/appearance, `vagrant` = carries a
    /// discovered-in-code responsibility awaiting review);
    /// `responsibilityCount`, `propertyCount`, `childCount` (numbers).
    pub field: String,
    /// Comparison operator. `eq`/`ne` (any type); `gt`/`gte`/`lt`/`lte` (numbers);
    /// `contains` (case-insensitive substring, strings); `exists`/`absent`
    /// (string is set & non-empty, or count > 0 / == 0 — no `value` needed).
    pub op: String,
    /// The value to compare against — a string, number, or boolean matching the
    /// field's type. Omit for `exists`/`absent`.
    #[serde(default)]
    pub value: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct QueryModelRequest {
    /// Absolute path to the project root. If omitted, uses the current working directory.
    pub project: Option<String>,
    /// Predicates that must ALL hold for a node to match (AND). At least one
    /// required. E.g. empty symbols = `[{field:"kind",op:"eq",value:"symbol"},
    /// {field:"empty",op:"eq",value:true}]`; fat components =
    /// `[{field:"kind",op:"eq",value:"component"},{field:"responsibilityCount",op:"gt",value:8}]`.
    #[serde(rename = "where", alias = "conditions")]
    pub conditions: Vec<QueryCondition>,
    /// Restrict results to the subtree rooted at this node id (the node and its
    /// descendants). Omit to query the whole model.
    pub under: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct GetUnimplementedRequest {
    /// Absolute path to the project root. If omitted, uses the current working directory.
    pub project: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct GetDriftRequest {
    /// Absolute path to the project root. If omitted, uses the current working directory.
    pub project: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct NodeMove {
    /// The node to re-parent. Its whole subtree moves with it.
    pub node_id: String,
    /// The new parent. Must satisfy the kind hierarchy (system→container→
    /// component→symbol). Omit to make the node top-level (system/person only).
    pub new_parent_id: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct MoveNodesRequest {
    /// Absolute path to the project root. If omitted, uses the current working directory.
    pub project: Option<String>,
    pub moves: Vec<NodeMove>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct GetHealthRequest {
    /// Absolute path to the project root. If omitted, uses the current working directory.
    pub project: Option<String>,
    /// Scope the report to one node's subtree (own + rolled-up counts, children
    /// summaries, per-anchor drift, link audit). Omit for the whole-model summary.
    pub node_id: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct ReconcileDriftRequest {
    /// Absolute path to the project root. If omitted, uses the current working directory.
    pub project: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct MarkImplementedRequest {
    /// Absolute path to the project root. If omitted, uses the current working directory.
    pub project: Option<String>,
    /// The node whose outstanding work you just implemented.
    pub node_id: String,
    /// Optional: specific responsibility ids to mark implemented. Omit to advance EVERYTHING
    /// outstanding on the node — every `proposed`/`changed` responsibility and property, plus a
    /// `proposed`/`changed` appearance — to `implemented`.
    pub responsibility_ids: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct GetRulesRequest {
    /// Topic to look up — free text matched against each rule's title and tags
    /// (e.g. "symbol", "group", "responsibility altitude", "links"). Returns the
    /// matching rules in full. OMIT to get the compact index of every rule (id,
    /// title, tags) to see what's available, then drill in by topic.
    pub topic: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct ReadCodebaseRequest {
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
    /// See read_model output for the exact schema.
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
    /// true for a visual/UI component (React component, UI element). Enables
    /// the preview rendering workflow on the node's page.
    pub visual: Option<bool>,
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
    /// Optional: description, parentGroupId (to nest under another group), responsibilities, icon.
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

// --- Intent write tools ---
//
// These build nodes from INTENT: the agent supplies meaning (name, plain
// responsibility statements, the source location it already has from the
// codebase context), and the tool mints the node id + responsibility ids,
// fixes the kind from the parent, defaults status to `implemented`, and (for
// symbols) writes the source map. The agent never constructs the JSON shape.

/// Person/actor to add at the top level.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct PersonItem {
    /// Display name of the person/actor.
    pub name: String,
    /// Their identity in a few words (what they ARE), not a re-list of responsibilities. Optional.
    pub description: Option<String>,
    /// Pure business-responsibility statements — one terse verb-led clause each, no mechanism vocabulary. Status defaults to implemented.
    #[serde(default)]
    pub responsibilities: Vec<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct AddPersonRequest {
    pub project: Option<String>,
    /// One or more persons to add.
    pub items: Vec<PersonItem>,
}

/// System to add at the top level — either the system being modeled or an
/// external third-party system it depends on.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct SystemItem {
    pub name: String,
    /// Identity in a few words (what it IS). Optional.
    pub description: Option<String>,
    /// Technology identity, mainly for externals (e.g. "Stripe", "S3"). Omit for the system you are modeling.
    pub technology: Option<String>,
    /// true for a third-party system your system depends on; omit for the system being modeled.
    #[serde(default)]
    pub external: bool,
    /// Pure business-responsibility statements. On an external, these read as expectations OF that external.
    #[serde(default)]
    pub responsibilities: Vec<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct AddSystemRequest {
    pub project: Option<String>,
    /// One or more systems to add.
    pub items: Vec<SystemItem>,
}

/// Container to add under a system.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct ContainerItem {
    /// ID of the parent system (must be a system node).
    pub parent_id: String,
    pub name: String,
    /// What it IS as software (e.g. "Next.js 14", "PostgreSQL 16", "S3 Bucket"). No mechanism vocabulary in responsibilities — put it here.
    pub technology: Option<String>,
    pub description: Option<String>,
    /// true for an external/third-party container.
    #[serde(default)]
    pub external: bool,
    /// Pure business-responsibility statements. Status defaults to implemented.
    #[serde(default)]
    pub responsibilities: Vec<String>,
    /// Project-relative directory this container owns (from the codebase context). Sets a boundary glob "{dir}/**/*" automatically — no separate update_source_map call needed.
    pub boundary_dir: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct AddContainerRequest {
    pub project: Option<String>,
    /// One or more containers to add.
    pub items: Vec<ContainerItem>,
}

/// Component to add under a container. Cluster components from code cohesion +
/// the dependency graph — NOT one-per-file.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct ComponentItem {
    /// ID of the parent container (must be a container node).
    pub parent_id: String,
    pub name: String,
    pub description: Option<String>,
    /// Pure business-responsibility statements. Status defaults to implemented.
    #[serde(default)]
    pub responsibilities: Vec<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct AddComponentRequest {
    pub project: Option<String>,
    /// One or more components to add.
    pub items: Vec<ComponentItem>,
}

/// A group: sibling nodes that ship or package together — a SECONDARY axis,
/// never a substitute for decomposition.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct GroupItem {
    /// ID of the node whose children are being grouped (the system for a group
    /// of containers; a container for a group of components).
    pub parent_id: String,
    /// Name of the deployment/package unit (e.g. "Integrations", "CMS").
    pub name: String,
    /// Identity in a few words. Optional.
    pub description: Option<String>,
    /// Node ids to enclose — all must be children of `parent_id`, same level. 2+ members.
    #[serde(default)]
    pub member_ids: Vec<String>,
    /// Optional unit-level responsibility statements (e.g. "deploys atomically"). Status defaults to implemented.
    #[serde(default)]
    pub responsibilities: Vec<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct AddGroupRequest {
    pub project: Option<String>,
    /// One or more groups to create.
    pub items: Vec<GroupItem>,
}

/// One declared field of a data-shape symbol.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct PropertyInput {
    /// Field/variant name as it appears in the declaration.
    pub label: String,
    /// What the field holds, in business terms. Optional.
    #[serde(default)]
    pub description: String,
}

/// A responsibility with optional line-range anchor. Accepts either a plain
/// string `"statement"` or an object `{statement, line?, endLine?}`.
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(untagged)]
pub(crate) enum ResponsibilityInput {
    /// `{statement, line?, endLine?}` — responsibility with specific line range within the symbol.
    Rich {
        /// The business-responsibility statement.
        statement: String,
        /// 1-based start line of the code that discharges this responsibility.
        line: Option<u32>,
        /// 1-based end line.
        #[serde(alias = "endLine")]
        end_line: Option<u32>,
    },
    /// Plain string — responsibility with no sub-range (whole symbol).
    Plain(String),
}

impl ResponsibilityInput {
    pub fn statement(&self) -> &str {
        match self {
            Self::Rich { statement, .. } => statement,
            Self::Plain(s) => s,
        }
    }
    pub fn line(&self) -> Option<u32> {
        match self {
            Self::Rich { line, .. } => *line,
            Self::Plain(_) => None,
        }
    }
    pub fn end_line(&self) -> Option<u32> {
        match self {
            Self::Rich { end_line, .. } => *end_line,
            Self::Plain(_) => None,
        }
    }
}

/// Symbol (one code definition) to add under a component.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct SymbolItem {
    /// ID of the parent component (must be a component node).
    pub parent_id: String,
    /// The identifier exactly as it appears in the source.
    pub name: String,
    /// Project-relative file the symbol is defined in (from the codebase context). The source map is anchored to this file + symbol name automatically.
    pub source_file: String,
    /// 1-based start line of the definition (from the codebase context). Used for the data-shape declaration anchor.
    pub line: Option<u32>,
    /// 1-based end line of the definition.
    pub end_line: Option<u32>,
    /// Responsibilities this symbol discharges. Each can be a plain string or `{statement, line?, endLine?}` with the specific line range within the symbol that does the work. Status defaults to implemented.
    #[serde(default)]
    pub responsibilities: Vec<ResponsibilityInput>,
    /// Field declarations when this symbol declares a data shape (struct/class/interface/type/config object). One per field. Status defaults to implemented.
    #[serde(default)]
    pub properties: Vec<PropertyInput>,
    /// true for a visual/UI component (React component, UI element). Enables
    /// the preview rendering workflow on the node's page.
    #[serde(default)]
    pub visual: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct AddSymbolRequest {
    pub project: Option<String>,
    /// One or more symbols to add.
    pub items: Vec<SymbolItem>,
}

// --- Atomic codebase-to-model generation ---

/// A component in an atomic container proposal. `key` is local to this request
/// and lets links/groups refer to the component before the server mints node ids.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct ProposedComponent {
    /// Unique request-local key, e.g. "authentication".
    pub key: String,
    pub name: String,
    pub description: Option<String>,
    /// Responsibilities at the component's C4 altitude.
    #[serde(default)]
    pub responsibilities: Vec<String>,
    /// Architecturally meaningful code definitions owned by this component.
    /// Code-bearing components must include at least one symbol.
    pub symbols: Vec<ProposedSymbol>,
}

/// One mandatory code-level symbol nested under a proposed component.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct ProposedSymbol {
    /// Unique request-local key used by links, e.g. "auth.login".
    pub key: String,
    /// Identifier exactly as it appears in source.
    pub name: String,
    /// Project-relative source file.
    pub source_file: String,
    /// Inclusive definition range.
    pub line: Option<u32>,
    pub end_line: Option<u32>,
    #[serde(default)]
    pub responsibilities: Vec<ResponsibilityInput>,
    #[serde(default)]
    pub properties: Vec<PropertyInput>,
    #[serde(default)]
    pub visual: Option<bool>,
}

/// An OPTIONAL cross-boundary relationship in a container proposal — used only
/// for links the deterministic dependency graph can't infer (to an external or
/// other-container existing node id). Code-level component→component and
/// symbol→symbol links are wired by the server; do NOT author them here.
/// Endpoints are a component/symbol request-local key or an existing node id. A
/// link that can't be placed legally is dropped and reported, never fatal.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct ProposedLink {
    pub src: String,
    pub dst: String,
    pub label: String,
    pub method: Option<String>,
}

/// Optional secondary grouping of proposed components.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct ProposedGroup {
    pub name: String,
    pub description: Option<String>,
    /// Request-local component keys.
    pub member_keys: Vec<String>,
    #[serde(default)]
    pub responsibilities: Vec<String>,
}

/// Commit the complete component + symbol subtree for one container in one
/// validated read-modify-write operation.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct CommitContainerModelRequest {
    pub project: Option<String>,
    pub container_id: String,
    pub components: Vec<ProposedComponent>,
    #[serde(default)]
    pub links: Vec<ProposedLink>,
    #[serde(default)]
    pub groups: Vec<ProposedGroup>,
}

/// A behaviour the code has that no responsibility describes — semantic drift.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct UndescribedItem {
    /// Terse business statement of what the code does that the model omits.
    pub statement: String,
    /// File the behaviour lives in (project-relative).
    pub source_file: String,
    /// Enclosing definition name, if any (durable source anchor).
    pub symbol: Option<String>,
    /// 1-based start line of the code that exhibits this behaviour.
    pub line: Option<u32>,
    /// 1-based end line.
    #[serde(alias = "endLine")]
    pub end_line: Option<u32>,
}

/// An existing responsibility whose code no longer discharges it.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct StaleResponsibility {
    /// ID of the responsibility that no longer matches its code.
    pub responsibility_id: String,
    /// Short factual note on how the code diverged (for the review queue).
    pub reason: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct FlagDriftRequest {
    pub project: Option<String>,
    /// The node whose boundary the drift was found in.
    pub node_id: String,
    /// Behaviours present in the code that NO responsibility describes — each is
    /// recorded as a vagrant responsibility on the node for the user to adopt or
    /// reject. Do NOT include mere code changes that still satisfy an existing
    /// responsibility.
    #[serde(default)]
    pub undescribed: Vec<UndescribedItem>,
    /// Existing responsibilities whose code no longer discharges them — marked
    /// `changed` for review.
    #[serde(default)]
    pub stale: Vec<StaleResponsibility>,
}
