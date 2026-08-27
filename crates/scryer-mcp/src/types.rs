use scryer_core::{Responsibility, SchemaProperty, Source, SourceLocation};
use serde::Deserialize;

/// Which layer of the model a read returns.
#[derive(Debug, Clone, Copy, Default, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Layer {
    /// The editable draft — what you author and what the canvas shows. The default,
    /// and what you almost always want: a read reflects your own pending edits.
    #[default]
    Plan,
    /// The committed model the code is currently believed to satisfy. Read this only
    /// to inspect the source of truth behind the plan (e.g. to see what's not yet built).
    Committed,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct DescopeRequest {
    /// Absolute path to the project root. If omitted, uses the current working directory.
    pub project: Option<String>,
    /// Node ids to remove from the MODEL because they shouldn't be modeled — the code stays.
    /// Each node's own responsibilities relocate to its parent (anchors preserved); the node and
    /// its descendants are then removed. Operates on both the plan and the committed model at once.
    pub node_ids: Vec<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct ReadModelRequest {
    /// Absolute path to the project root. If omitted, uses the current working directory.
    pub project: Option<String>,
    /// Optional node id to scope to. OMIT for the architecture overview — the whole tree down to
    /// components (symbols excluded), with counts; small and safe to read. Pass a node id to read
    /// THAT node's full subtree: its descendants (including symbols), responsibilities, properties,
    /// links, and source anchors. Drill into a component to see its symbols.
    pub node: Option<String>,
    /// Which layer to read: "plan" (default — your editable draft, what the canvas shows) or
    /// "committed" (the model the code currently satisfies). Leave unset unless you specifically
    /// need the committed source of truth.
    #[serde(default)]
    pub layer: Layer,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct LocateRequest {
    /// Absolute path to the project root. If omitted, uses the current working directory.
    pub project: Option<String>,
    /// The source file to look up, relative to the project root (an absolute path
    /// inside the project is accepted and normalized).
    pub file: String,
    /// Optional identifier (function/type/component name) to narrow to: returns only
    /// the claims anchored to that symbol when any are, the whole file's otherwise.
    pub symbol: Option<String>,
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
    /// Which layer to search: "plan" (default — your editable draft) or "committed" (the model
    /// the code currently satisfies). Leave unset unless you specifically need committed.
    #[serde(default)]
    pub layer: Layer,
}

/// One predicate in a `query_model` request: a `field`, a comparison `op`, and
/// (for everything but `exists`/`absent`) a `value`. Fields and operators are
/// orthogonal — compose them into any node-shape question.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct QueryCondition {
    /// Queryable node field. One of:
    /// `kind`, `name`, `description`, `technology` (strings);
    /// `external`, `visual`, `empty`, `vagrant` (booleans —
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
    /// Which layer to query: "plan" (default — your editable draft) or "committed" (the model
    /// the code currently satisfies). Leave unset unless you specifically need committed.
    #[serde(default)]
    pub layer: Layer,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct GetPendingRequest {
    /// Absolute path to the project root. If omitted, uses the current working directory.
    pub project: Option<String>,
    /// Filter the queue to ONE change's entries — a change id from `openChanges`
    /// (or from `set_change`), or the literal "unfiled" for entries belonging to
    /// no change. Omit for the whole queue.
    pub change: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct SetChangeRequest {
    /// Absolute path to the project root. If omitted, uses the current working directory.
    pub project: Option<String>,
    /// Open a NEW change: the task in one sentence, as the dev put it (e.g.
    /// "make drift track vagrant properties too"). This rationale is the
    /// change's durable identity — it survives the fold in the history log.
    pub rationale: Option<String>,
    /// Resume an EXISTING open change by id (see `get_pending`'s `openChanges`).
    /// A new session picks up the change object, not archaeology.
    pub change_id: Option<String>,
    /// Detach from the current change: subsequent writes this session go
    /// unfiled (the serial workflow).
    pub clear: Option<bool>,
    /// Close an EMPTY open change by id — the escape hatch for a stranded
    /// ledger whose work ended up tagged or folded elsewhere. Refused while
    /// the change still has tagged entries (those close by folding or
    /// reverting). Recorded in history as "abandoned".
    pub close: Option<String>,
    /// MOVE existing pending work into another change — the repair for work
    /// filed under the wrong change, or a task that turned out to be two.
    /// Bare ids, as you already hold them: a node or group id moves that
    /// carrier and every pending element under it (the unit `get_pending`
    /// shows); a responsibility or link id moves just that element; a `chg-N`
    /// id moves everything currently filed under it; `"unfiled"` moves
    /// everything untagged. Pass `to` for the destination.
    pub retag: Option<Vec<String>>,
    /// Where `retag` sends its targets: an open change id, or "unfiled" to
    /// detach them. Omit to send them to the session's current change.
    pub to: Option<String>,
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
pub(crate) struct IngestTestReportRequest {
    /// Absolute path to the project root. If omitted, uses the current working directory.
    pub project: Option<String>,
    /// The JUnit XML report file a test run just wrote — absolute, or relative
    /// to the project root. One call ingests the whole file.
    pub path: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct GetTestRadiusRequest {
    /// Absolute path to the project root. If omitted, uses the current working directory.
    pub project: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct ProbeClaimRequest {
    /// Absolute path to the project root. If omitted, uses the current working directory.
    pub project: Option<String>,
    /// The claim to probe. It must have an attached test holding a current,
    /// passing verdict — otherwise the probe is refused with the reason.
    pub resp_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct EndProbeRequest {
    /// Absolute path to the project root. If omitted, uses the current working directory.
    pub project: Option<String>,
    /// The claim whose probe is open.
    pub resp_id: String,
    /// How many deliberate breaks you tried in total, survivors included.
    pub probes: u32,
    /// One entry per break the test did NOT catch, describing what you changed
    /// (e.g. "flipped the boundary at line 356 from > to >="). Empty means every
    /// break was caught. These are the audit trail — write what you actually did,
    /// never a summary.
    #[serde(default)]
    pub survivors: Vec<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct MarkImplementedRequest {
    /// Absolute path to the project root. If omitted, uses the current working directory.
    pub project: Option<String>,
    /// The node whose outstanding work you just implemented. Optional when folding only
    /// links/groups by id — at least one of node_id / link_ids / group_ids is required.
    pub node_id: Option<String>,
    /// Optional: specific responsibility ids to fold into the committed model (requires
    /// node_id). Omit (along with property_labels) to fold EVERYTHING outstanding on the
    /// node — every planned responsibility and property, plus the appearance.
    pub responsibility_ids: Option<Vec<String>>,
    /// Optional: specific property labels to fold (requires node_id) — the partial-fold
    /// counterpart of responsibility_ids for data fields, which are identified by label.
    pub property_labels: Option<Vec<String>>,
    /// Optional: link ids to fold. The only way to commit a standalone link change or
    /// DELETION — a link deletion never rides a node fold, so a plan that removes a link
    /// between two surviving nodes stays pending until it is folded here.
    pub link_ids: Option<Vec<String>>,
    /// Optional: group ids to fold. Likewise the only way to commit a standalone group
    /// change or deletion.
    pub group_ids: Option<Vec<String>>,
    /// Optional: also commit the node's plan-only ANCESTORS, structure-only, before the
    /// fold — the design-first escape. In a model that has never been committed, a fold
    /// otherwise dead-ends on "commit the parent first", and folding an ancestor whole
    /// would mark its unbuilt claims implemented. Structure-only commits an ancestor's
    /// identity, kind, parent, and boundary while its responsibilities and properties
    /// stay pending in the plan. With responsibility_ids, the host node itself is also
    /// committed structure-only, so a partial implementation folds honestly.
    pub commit_ancestors: Option<bool>,
    /// Optional: anchor claims to code in the SAME call — "here's what I built and
    /// where it lives" as one atomic statement, instead of a separate
    /// update_source_map you can forget (an unanchored claim reads as scaffolding
    /// and carries no drift tripwire). Same shape as update_source_map `entries`:
    /// locations keyed by responsibility id, each the SPECIFIC line range that does
    /// the work (`pattern` file, `symbol` enclosing definition, `line`/`endLine` a
    /// PROPER subset of it — omit them to mean the whole definition).
    pub anchors: Option<Vec<SourceMapEntry>>,
    /// ATTACH TESTS to the claims you fold, in the same call — "and this test
    /// exercises it" alongside the fold and anchors. Same shape as `anchors`:
    /// locations keyed by responsibility id, `pattern` = test file, `symbol` =
    /// the test's NAME — the `it("…")`/`test("…")` description string or the
    /// test function's identifier; both resolve and fingerprint (symbol-only
    /// means the whole test); set `command` to record how to run it (never
    /// executed). For a When/While/If claim the
    /// test is EXPECTED — mandatory on symbol hosts (rule 22); a fold that
    /// leaves a testable claim with no test attached succeeds but is called
    /// out in the response. When you just wrote the test, this field is the
    /// cheapest moment to attach it.
    pub tests: Option<Vec<SourceMapEntry>>,
    /// Fold an ENTIRE change by id: every plan entry tagged to it, in dependency
    /// order (nodes root-ward, then claims/properties, then groups and links,
    /// then deletions). Standalone — do not combine with node_id / link_ids /
    /// group_ids; `commit_ancestors` composes (plan-only hosts fold
    /// structure-only first). When the fold takes the change's last entry, the
    /// change closes and its rationale lands in the history log.
    pub change: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct OrientRequest {
    /// Absolute path to the project root. If omitted, uses the current working directory.
    pub project: Option<String>,
    /// The task in a few words (e.g. "fix the save race in model storage") — matched
    /// against the model's nodes and the modeling rules. Give this, `files`, or both.
    pub task: Option<String>,
    /// Project-relative files the task touches — each is reverse-looked-up into its
    /// governing nodes, anchored claims, and binding directives (same as `locate`).
    pub files: Option<Vec<String>>,
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
pub(crate) struct UpdateGroupItem {
    /// ID of the group to patch.
    pub group_id: String,
    /// New display name. Omit to leave unchanged.
    pub name: Option<String>,
    /// New description. Omit to leave unchanged.
    pub description: Option<String>,
    /// Replacement member node ids (2+, all children of the group's parent node — same C4 level).
    /// Omit to leave the membership unchanged.
    pub member_ids: Option<Vec<String>>,
    /// Replacement responsibilities for the group. Omit to leave unchanged; pass an empty array to clear.
    pub responsibilities: Option<Vec<Responsibility>>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct UpdateGroupRequest {
    /// Absolute path to the project root. If omitted, uses the current working directory.
    pub project: Option<String>,
    /// Groups to patch by id. Only fields present in each item are changed.
    pub items: Vec<UpdateGroupItem>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct UpdateNodeItem {
    /// ID of the node to update.
    pub node_id: String,
    /// Node kind: "person", "system", "container", "component", or "symbol".
    pub kind: Option<String>,
    pub name: Option<String>,
    /// New description. Pass an empty string to CLEAR it (omit to leave unchanged).
    pub description: Option<String>,
    /// Short badge naming the stack (e.g. "Next.js 14", "Tauri 2 + React"), a few
    /// words at most — explanatory prose belongs in `description`. Pass an empty
    /// string to CLEAR it.
    pub technology: Option<String>,
    /// Pass false to clear the external marking.
    pub external: Option<bool>,
    /// Full replacement of responsibilities. Pass an empty array to clear. Vagrant
    /// (code-discovered) claims awaiting a verdict survive a replacement that omits
    /// them — they leave only through an explicit adopt/reject.
    pub responsibilities: Option<Vec<Responsibility>>,
    /// Full replacement of field declarations for a data-shape symbol. Pass an empty array to clear.
    pub properties: Option<Vec<SchemaProperty>>,
    /// true for a visual/UI component (React component, UI element). Enables
    /// the preview rendering workflow on the node's page.
    pub visual: Option<bool>,
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
pub(crate) struct SetDirectivesItem {
    /// Node id — replaces that node's own node-level directives (which carry down,
    /// binding its whole subtree). Exactly one of `node_id` / `responsibility_id` per item.
    pub node_id: Option<String>,
    /// Responsibility id — replaces that claim's directives. The claim may live on a
    /// node or on a group. Exactly one of `node_id` / `responsibility_id` per item.
    pub responsibility_id: Option<String>,
    /// Full replacement list of directives — verb-led "must"/"never" constraints.
    /// Pass an empty array to clear.
    pub directives: Vec<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct SetDirectivesRequest {
    pub project: Option<String>,
    /// Directive replacements to apply. Batch-friendly: one item per node or responsibility.
    pub items: Vec<SetDirectivesItem>,
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
    /// ATTACHED TESTS keyed by responsibility id — which tests are attached
    /// to which claims. Same shape as `entries` (`pattern` = test file,
    /// `symbol` = the test's NAME — the `it("…")`/`test("…")` description
    /// string or the test function's identifier; both resolve and fingerprint;
    /// a symbol-only anchor means the whole test). Optionally set `command` on a location to record how to run it
    /// (e.g. `cargo test parse::roundtrip`) — recorded, never executed. A
    /// separate dimension from `entries`: where a claim is implemented vs.
    /// which tests are attached to it. Empty `locations` clears.
    #[serde(default)]
    pub test_entries: Vec<SourceMapEntry>,
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

// --- Intent write tools ---
//
// These build nodes from INTENT: the agent supplies meaning (name, plain
// responsibility statements, the source location it already has from the
// codebase context), and the tool mints the node id + responsibility ids,
// fixes the kind from the parent, and (for
// symbols) writes the source map. The agent never constructs the JSON shape.

/// Person/actor to add at the top level.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct PersonItem {
    /// Display name of the person/actor — plain domain vocabulary a newcomer reads instantly; no codenames or abbreviations (rule 17).
    pub name: String,
    /// Their identity in a few words (what they ARE), not a re-list of responsibilities. Optional.
    pub description: Option<String>,
    /// Pure business-responsibility statements — each a plain string or `{statement, concern?}`.
    #[serde(default)]
    pub responsibilities: Vec<StatementInput>,
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
    /// Plain domain vocabulary a newcomer reads instantly — what the system IS in the domain's own terms; no codenames, abbreviations, or cleverness (rule 17).
    pub name: String,
    /// Identity in a few words (what it IS). Optional.
    pub description: Option<String>,
    /// Technology identity as a short badge, mainly for externals (e.g. "Stripe", "S3"). Omit for the system you are modeling.
    pub technology: Option<String>,
    /// true for a third-party system your system depends on; omit for the system being modeled.
    #[serde(default)]
    pub external: bool,
    /// Pure business-responsibility statements — each a plain string or `{statement, concern?}`. On an external, these read as expectations OF that external.
    #[serde(default)]
    pub responsibilities: Vec<StatementInput>,
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
    /// Plain domain vocabulary a newcomer reads instantly — no codenames, abbreviations, or cleverness (rule 17).
    pub name: String,
    /// What it IS as software, as a short badge (e.g. "Next.js 14", "PostgreSQL 16", "S3 Bucket") — a few words, not a sentence. Keep mechanism vocabulary out of responsibilities; NAME the stack here and put any explanatory prose in `description`.
    pub technology: Option<String>,
    pub description: Option<String>,
    /// true for an external/third-party container.
    #[serde(default)]
    pub external: bool,
    /// Pure business-responsibility statements — each a plain string or `{statement, concern?}`.
    #[serde(default)]
    pub responsibilities: Vec<StatementInput>,
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
    /// Plain domain vocabulary a newcomer reads instantly — no codenames, abbreviations, or cleverness (rule 17).
    pub name: String,
    pub description: Option<String>,
    /// Pure business-responsibility statements — each a plain string or `{statement, concern?}`.
    #[serde(default)]
    pub responsibilities: Vec<StatementInput>,
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
    /// Optional unit-level responsibility statements (e.g. "deploys atomically") — each a plain string or `{statement, concern?}`.
    #[serde(default)]
    pub responsibilities: Vec<StatementInput>,
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

/// A responsibility with optional concern tag and line-range anchor. Accepts
/// either a plain string `"statement"` or an object
/// `{statement, concern?, line?, endLine?}`. The statement is ONE terse
/// verb-led clause in the plainest words that are still precise — no mechanism
/// vocabulary, no trailing "so that…" purpose clause — in EARS form: condition
/// first, response last, with `**bold**` on the keyword and response verb
/// ("**When** a callback arrives, **append** …"; rules 15, 17, 21).
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(untagged)]
pub(crate) enum ResponsibilityInput {
    /// `{statement, concern?, line?, endLine?}` — responsibility with a concern tag and/or a specific line range within the symbol.
    Rich {
        /// The business-responsibility statement.
        statement: String,
        /// Cross-cutting concern this responsibility serves — ONE kebab-case slug (e.g. "auth", "idempotency"). Reuse the model's registry / standard slugs before minting; omit for core domain flow (rule 20).
        concern: Option<String>,
        /// 1-based start line of the code that discharges this responsibility.
        line: Option<u32>,
        /// 1-based end line.
        #[serde(alias = "endLine")]
        end_line: Option<u32>,
    },
    /// Plain string — untagged responsibility with no sub-range (whole symbol).
    Plain(String),
}

impl ResponsibilityInput {
    pub fn statement(&self) -> &str {
        match self {
            Self::Rich { statement, .. } => statement,
            Self::Plain(s) => s,
        }
    }
    pub fn concern(&self) -> Option<&str> {
        match self {
            Self::Rich { concern, .. } => concern.as_deref(),
            Self::Plain(_) => None,
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

/// A responsibility statement for a structural node (person/system/container/
/// component/group), optionally tagged with a concern. Accepts a plain string
/// `"statement"` or an object `{statement, concern?}`. The statement is ONE
/// terse verb-led clause in the plainest words that are still precise — no
/// mechanism vocabulary, no trailing "so that…" purpose clause — in EARS form:
/// condition first, response last, with `**bold**` on the keyword and
/// response verb (rules 15, 17, 21).
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(untagged)]
pub(crate) enum StatementInput {
    /// `{statement, concern?}` — responsibility tagged with the cross-cutting concern it serves.
    Rich {
        /// The business-responsibility statement.
        statement: String,
        /// Cross-cutting concern this responsibility serves — ONE kebab-case slug (e.g. "auth", "idempotency"). Reuse the model's registry / standard slugs before minting; omit for core domain flow (rule 20).
        concern: Option<String>,
    },
    /// Plain string — untagged responsibility (core domain flow).
    Plain(String),
}

impl StatementInput {
    pub fn statement(&self) -> &str {
        match self {
            Self::Rich { statement, .. } => statement,
            Self::Plain(s) => s,
        }
    }
    pub fn concern(&self) -> Option<&str> {
        match self {
            Self::Rich { concern, .. } => concern.as_deref(),
            Self::Plain(_) => None,
        }
    }
}

impl From<&str> for StatementInput {
    fn from(s: &str) -> Self {
        Self::Plain(s.to_string())
    }
}

impl From<String> for StatementInput {
    fn from(s: String) -> Self {
        Self::Plain(s)
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
    /// Responsibilities this symbol discharges. Each can be a plain string or `{statement, line?, endLine?}` with the specific line range within the symbol that does the work.
    #[serde(default)]
    pub responsibilities: Vec<ResponsibilityInput>,
    /// Field declarations when this symbol declares a data shape (struct/class/interface/type/config object). One per field.
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
    /// Plain domain vocabulary a newcomer reads instantly — no codenames, abbreviations, or cleverness (rule 17).
    pub name: String,
    pub description: Option<String>,
    /// Responsibilities at the component's C4 altitude — each a plain string or `{statement, concern?}`.
    #[serde(default)]
    pub responsibilities: Vec<StatementInput>,
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
    /// Unit-level responsibility statements — each a plain string or `{statement, concern?}`.
    #[serde(default)]
    pub responsibilities: Vec<StatementInput>,
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
    /// Attach this behaviour's responsibility to an EXISTING node, by id — when a
    /// symbol/component already models this code. Leave node_id AND node_key
    /// unset to route automatically to the finest node the source map already
    /// ties the file to (falling back to the reviewed container).
    #[serde(default, alias = "nodeId")]
    pub node_id: Option<String>,
    /// Attach to a node MINTED in this call — the `key` of a `newNodes` entry.
    /// Use when the behaviour is a brand-new definition the model has no node
    /// for. Takes precedence over node_id.
    #[serde(default, alias = "nodeKey")]
    pub node_key: Option<String>,
}

/// A declared data field the code has that no property describes — the
/// property-level twin of [`UndescribedItem`]. Recorded as a vagrant property
/// on the data-shape node for the user to adopt (the field exists) or reject
/// (mark the field for deletion).
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct UndescribedProperty {
    /// The field's name — the property label, the EXACT identifier in code.
    pub label: String,
    /// Terse description of what the field holds. Omit if self-evident.
    #[serde(default)]
    pub description: String,
    /// File the field is declared in (project-relative).
    pub source_file: String,
    /// Enclosing data-type definition name — the durable anchor that routes the
    /// field to the node already modeling that type.
    pub symbol: Option<String>,
    /// Attach to an EXISTING node by id — when the data type is already modeled.
    /// Leave node_id AND node_key unset to route automatically to the finest node
    /// the source map ties `symbol`/`source_file` to.
    #[serde(default, alias = "nodeId")]
    pub node_id: Option<String>,
    /// Attach to a node MINTED in this call — the `key` of a `newNodes` entry.
    /// Use when the field belongs to a brand-new type the model has no node for.
    /// Takes precedence over node_id.
    #[serde(default, alias = "nodeKey")]
    pub node_key: Option<String>,
}

/// An existing property whose backing field is gone or materially changed — the
/// property-level twin of [`StaleResponsibility`]. Properties have no id, so it
/// is addressed by its owning node plus its `label`.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct StaleProperty {
    /// ID of the node that carries the property.
    pub node_id: String,
    /// Label of the property whose field no longer matches the code.
    pub label: String,
    /// Short factual note on how the field diverged (for the review queue).
    pub reason: String,
}

/// A node the drift check MINTS to home code the model has no node for — a
/// missing rung in the tree (a component, or a symbol for a brand-new
/// definition). Minted vagrant in the PLAN: it folds into the committed model
/// when a responsibility hung on it is adopted, and is dropped on reject.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct NewNode {
    /// Temporary key, unique within THIS call — referenced before the real id is
    /// assigned, from an `undescribed` item's `nodeKey` or a deeper node's
    /// `parentKey`.
    pub key: String,
    /// Node kind — typically "component" or "symbol".
    pub kind: String,
    /// The node's name. For a symbol, the EXACT code identifier.
    pub name: String,
    /// Parent: an EXISTING node id to nest under. Mutually exclusive with
    /// `parentKey` — set exactly one.
    #[serde(default, alias = "parentId")]
    pub parent_id: Option<String>,
    /// Parent: the `key` of another node minted in THIS call (a shallower rung).
    /// List ancestors before descendants so the parent is declared first.
    #[serde(default, alias = "parentKey")]
    pub parent_key: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub technology: Option<String>,
}

/// An existing responsibility whose code no longer discharges it.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct StaleResponsibility {
    /// ID of the responsibility that no longer matches its code.
    pub responsibility_id: String,
    /// Short factual note on how the code diverged (for the review queue).
    pub reason: String,
    /// Optional corrected statement: when the behaviour didn't vanish but
    /// DIVERGED, the responsibility wording that matches what the code now does.
    /// Surfaced to the user as a proposed reword they can accept (folding it into
    /// the model — no code work, the code already does this), edit, or ignore in
    /// favour of re-implement/drop. Omit when the behaviour is truly gone.
    #[serde(default, alias = "proposedStatement")]
    pub proposed_statement: Option<String>,
}

/// An existing NODE whose backing code is GONE — a symbol, a component, or a
/// whole container subtree the model still asserts but a deleted file/folder
/// wiped out. The node and everything under it is flagged stale as a unit; the
/// user re-implements the subtree or drops it.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct StaleNode {
    /// ID of the node whose backing code no longer exists.
    pub node_id: String,
    /// Short factual note on what was removed (for the review queue).
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
    /// Nodes to MINT (vagrant) to home undescribed behaviour the model has no
    /// node for — e.g. a new component plus the symbol for a new function. Each
    /// carries a temp `key` referenced from `undescribed.nodeKey`; build whole
    /// chains by pointing a node's `parentKey` at a shallower one (list ancestors
    /// first). Omit when every finding already has a home node.
    #[serde(default, alias = "newNodes")]
    pub new_nodes: Vec<NewNode>,
    /// Declared data fields present in the code that NO property describes — each
    /// is recorded as a vagrant property on the data-shape node for the user to
    /// adopt or reject. Use this (not `undescribed`) for a new struct field /
    /// interface member: it is data the type carries, never a behaviour.
    #[serde(default, alias = "undescribedProperties")]
    pub undescribed_properties: Vec<UndescribedProperty>,
    /// Existing responsibilities whose code no longer discharges them — marked
    /// `changed` for review.
    #[serde(default)]
    pub stale: Vec<StaleResponsibility>,
    /// Existing properties whose backing field is gone or materially changed —
    /// marked stale for review. The property-level mirror of `stale`.
    #[serde(default, alias = "staleProperties")]
    pub stale_properties: Vec<StaleProperty>,
    /// Existing NODES whose backing code is entirely GONE (a deleted file/folder
    /// wiped out a symbol, component, or container subtree). Each is flagged
    /// stale as a unit — the whole subtree — for the user to re-implement or
    /// drop. Use this instead of `stale` when it's the node, not just one of its
    /// claims, that lost its code.
    #[serde(default, alias = "staleNodes")]
    pub stale_nodes: Vec<StaleNode>,
}
