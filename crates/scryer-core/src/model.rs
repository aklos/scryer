use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::changes;
use crate::SCRY_VERSION;

// --- Core enums ---

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    Person,
    System,
    Container,
    Component,
    /// A single addressable code definition — function, method, handler, hook,
    /// React component, class, struct, interface, or type. One leaf = one
    /// symbol. A symbol may discharge responsibilities, declare a data shape
    /// (via `properties`), or both. A pure data type is a symbol that carries
    /// only properties.
    #[serde(alias = "schema")]
    Symbol,
}

// --- Responsibility ---

/// A pure business-responsibility statement. The `statement` field is the spec;
/// `directives` are optional prescriptive HOW-constraints and have no
/// conformance role.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Responsibility {
    pub id: String,
    /// Verb-led business statement of accountability. No mechanism words.
    /// EARS-shaped (condition first, response last) and may carry display
    /// markup — `**bold**` on the keyword and response verb — which the UI
    /// renders and strips for comparison (rule 21).
    pub statement: String,
    /// The cross-cutting concern this responsibility serves — at most ONE
    /// kebab-case slug (e.g. "auth", "idempotency"), referencing an entry in
    /// the model's concern registry ([`ScryModel::concerns`]; entries are
    /// minted automatically on first use). Untagged means core domain flow —
    /// that absence is signal, not an omission. Metadata beside the statement,
    /// not part of it: no conformance role, and a tag change never re-dates
    /// `last_touched_at`. See rule 20.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub concern: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vagrant: Option<bool>,
    /// Drift observation: the semantic check judged that the code no longer
    /// discharges this claim. Like `vagrant`, a flag awaiting a human/agent
    /// verdict (re-implement, reword, or drop) — the status itself is the
    /// prescription and stays untouched until that verdict. Cleared by
    /// `mark_implemented` or by editing the claim.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stale: Option<bool>,
    /// Drift's proposed correction for a `stale` claim: the statement that would
    /// match what the code now does. Set by `flag_drift` alongside `stale` when
    /// the behaviour didn't vanish but diverged — the user accepts it (folding
    /// the new wording into the model), edits it, or ignores it for the
    /// re-implement/drop verdicts. A localized hint, not a plan work item:
    /// `diff` ignores it, so a reword awaiting a verdict never enters the queue.
    /// Cleared with `stale` on any verdict or edit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stale_proposal: Option<String>,
    /// Optional prescriptive HOW-constraints — verb-led "must"/"never" rules
    /// the implementation has to satisfy. User-authored: read-only to the
    /// agent's ordinary writes, so hidden from write-tool input schemas
    /// (`schemars(skip)`) while still serialized for storage and surfaced on
    /// read; `set_directives` is the one deliberate write path, reserved for
    /// edits the user explicitly requested. Not part of conformance.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[schemars(skip)]
    pub directives: Vec<String>,
    /// Unix seconds of the last truth-bearing edit (statement / status / flags /
    /// directives). Drives the canvas "fossilization" patina: a fresh edit
    /// glistens, long-untouched code-backed responsibilities weather to stone.
    /// Stamped automatically by the write path (agent edits) and the canvas
    /// mutation helpers (user edits) — never hand-authored, so it's hidden from
    /// the agent's write-tool input schemas.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    pub last_touched_at: Option<u64>,
}

// --- Code-level data ---

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SchemaProperty {
    pub label: String,
    #[serde(default)]
    pub description: String,
    /// Drift adoption marker, the property-level twin of [`Responsibility::vagrant`]:
    /// `flag_drift` discovered a declared data field that no property described and
    /// proposed it into the PLAN. Awaits a human verdict — adopt (the field exists,
    /// fold it in) or reject (mark the field for deletion). Hidden from the agent's
    /// write-tool schemas — vagrancy is set only by `flag_drift`, never authored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    pub vagrant: Option<bool>,
    /// Drift regression marker, the property-level twin of [`Responsibility::stale`]:
    /// the semantic check judged the field backing this property gone or materially
    /// changed. A flag awaiting a verdict (re-implement or drop); the property itself
    /// stays untouched until then. Cleared by adoption/commit or by editing the
    /// property. Hidden from the agent's write-tool schemas — set only by `flag_drift`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    pub stale: Option<bool>,
    /// Unix seconds of the last truth-bearing edit (label / description).
    /// Drives the fossilization patina, exactly like
    /// [`Responsibility::last_touched_at`]; stamped automatically, never authored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    pub last_touched_at: Option<u64>,
}

/// A source-file pointer attached to a node. Wide glob + optional comment;
/// distinct from [`SourceLocation`], which carries precise line numbers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Source {
    /// Glob pattern for matching files, e.g. "src/auth/**/*.rs"
    pub pattern: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SourceLocation {
    /// File path (relative to the project) the responsibility maps into.
    pub pattern: String,
    /// Durable anchor: the identifier (function/handler/type/component name)
    /// that discharges the responsibility. The exact line range is resolved
    /// from this on demand, so it survives edits that shift line numbers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_line: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
}

// --- Appearance (the look of a UI component) ---

/// The render-artifact lifecycle of a visual component's look. Its own axis,
/// independent of the model→code plan (the diff between committed and planned):
/// `Proposed` when the look is planned, `Implemented` when synced from / built
/// off the code, `Changed` when the code drifts from the modeled look.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum RenderState {
    Proposed,
    Implemented,
    Changed,
}

/// What a UI component is accountable for in how it LOOKS — a contract alongside
/// `responsibilities` (behavior) and `properties` (data). Carries the built
/// render artifact (`dist_path` + `source_hash`) used to detect drift from the
/// look, plus the render lifecycle [`RenderState`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Appearance {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<RenderState>,
    /// Project-relative path to the built render output directory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dist_path: Option<String>,
    /// Unix seconds when the render was last built.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub built_at: Option<u64>,
    /// Hash of the source at render time — used to detect drift from the look.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_hash: Option<String>,
}

// --- Nodes, links, groups ---

/// A manual canvas placement — the node's center on its parent's map surface,
/// in that surface's coordinate space. Pure cosmetics with no conformance role:
/// `diff` never compares it, so a drag is not a plan change and never re-dates
/// anything.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Position {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    pub id: String,
    pub kind: Kind,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub technology: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Drift adoption marker: this node was MINTED by a drift check to home
    /// code-discovered behaviour that no existing node described — it lives in
    /// the PLAN only, awaiting a human verdict. Like a vagrant responsibility
    /// ("code already does this, adopt?"), NOT planned intent ahead of code
    /// ("implement this!"): a vagrant node is excluded from the implement queue
    /// and folds into the committed model when its responsibility is adopted
    /// (which clears this flag). Hidden from the agent's write-tool schemas —
    /// vagrancy is set only by `flag_drift`, never authored directly.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    pub vagrant: Option<bool>,
    /// Drift regression marker (the mirror of `vagrant`): the code that backed
    /// this whole node — a symbol, a component, an entire container subtree — is
    /// GONE, but the model still asserts it. Set by `flag_drift` on the PLAN node
    /// (where the UI reads it) when a deleted folder/file leaves a modeled node
    /// with no code; it rides the working claim until the user gives a verdict —
    /// re-implement (rebuild the subtree → it becomes a to-do) or drop (the area
    /// was removed on purpose → the subtree leaves the model). `diff` ignores the
    /// flag, so a stale node awaiting a verdict is not itself a plan work item.
    /// Hidden from the agent's write-tool schemas — set only by `flag_drift`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    pub stale: Option<bool>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub responsibilities: Vec<Responsibility>,
    /// Field declarations, when this symbol defines a data shape (struct,
    /// class, interface, type). Empty for behavior-only symbols.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub properties: Vec<SchemaProperty>,
    /// Optional lucide-react icon name override. Falls back to a deterministic
    /// icon picked from `id` when unset. Frontend-only meaning; backend just
    /// passes the string through.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// Marks this node as a visual component (React component, UI element):
    /// the flag that says "this node has a look the render tool can build."
    /// Set by the agent during model generation or toggled by the user.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visual: Option<bool>,
    /// What the component is accountable for in how it LOOKS — a status-bearing
    /// contract like a responsibility, but visual instead of textual. (Formerly
    /// `preview`; the alias keeps older `.scry` files loading.)
    #[serde(default, alias = "preview", skip_serializing_if = "Option::is_none")]
    pub appearance: Option<Appearance>,
    /// User-authored freeform notes for this node — self-context, traversal
    /// aids, reminders to self. Distinct from `description` (what the node IS)
    /// and from this node's `directives` (HOW-constraints): notes carry
    /// no spec or conformance role. Plain text. User-only: hidden from the
    /// agent's write-tool schemas (`schemars(skip)`) but serialized and
    /// surfaced on read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    pub notes: Option<String>,
    /// Where the user dragged this node on its parent's map surface. Unset means
    /// auto-layout owns the placement; set means the canvas pins the node there
    /// and layout only routes around it. User-authored via the canvas: hidden
    /// from the agent's write-tool schemas (`schemars(skip)`) and restored from
    /// the prior model across raw whole-node writes, like `directives`. Cleared
    /// on reparent — coordinates are per-surface and don't survive the move.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    pub position: Option<Position>,
    /// Node-level prescriptive HOW-constraints — verb-led "must"/"never" rules
    /// the implementation must satisfy, the node-altitude twin of a
    /// responsibility's `directives`. These CARRY DOWN: a node is bound by its
    /// own directives plus every ancestor's, computed at read time (never copied
    /// onto descendants), so editing a container's directive instantly re-binds
    /// its whole subtree. User-authored: read-only to the agent's ordinary
    /// writes, so hidden from write-tool input schemas (`schemars(skip)`) while
    /// still serialized for storage and surfaced (own + inherited) on read;
    /// `set_directives` is the one deliberate write path, reserved for edits
    /// the user explicitly requested. Plain text — not part of conformance.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[schemars(skip)]
    pub directives: Vec<String>,
}

/// The `empty` flag — a SYMBOL that carries no semantic content of its own: no
/// responsibilities, no properties, no rendered appearance, and not external.
/// Derived, never stored. Mirrors `isNodeEmpty` in the frontend (`src/viewmodel.ts`)
/// — keep the two in lockstep. Scoped to symbols: structural nodes
/// (system/container/component) carry their meaning through their children, so a
/// parent without its own responsibilities is not "empty" in this sense.
pub fn is_node_empty(node: &Node) -> bool {
    node.kind == Kind::Symbol
        && node.external != Some(true)
        && node.responsibilities.is_empty()
        && node.properties.is_empty()
        && node.appearance.as_ref().and_then(|a| a.status).is_none()
}

/// One ancestor's contribution to a node's inherited directives — the source
/// node's id and name alongside the directives it carries down. Lets a reader
/// attribute every inherited constraint to where the user authored it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InheritedDirectives {
    pub node_id: String,
    pub name: String,
    pub directives: Vec<String>,
}

/// The directives a node inherits from its ancestry: every ancestor's
/// node-level `directives`, NEAREST ancestor first, walking up to the root.
/// A node's OWN directives are excluded (they live on the node itself) — the
/// full binding set is `node.directives` followed by this. Ancestors with no
/// directives are skipped. Mirrors `inheritedDirectives` in the frontend
/// (`src/viewmodel.ts`) — keep the two in lockstep.
pub fn inherited_directives(model: &ScryModel, node_id: &str) -> Vec<InheritedDirectives> {
    let by_id: HashMap<&str, &Node> = model.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    let mut out = Vec::new();
    let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
    let mut cur = by_id.get(node_id).and_then(|n| n.parent_id.as_deref());
    while let Some(pid) = cur {
        if !seen.insert(pid) {
            break; // cycle guard — a malformed parent chain never loops forever
        }
        let Some(p) = by_id.get(pid) else { break };
        if !p.directives.is_empty() {
            out.push(InheritedDirectives {
                node_id: p.id.clone(),
                name: p.name.clone(),
                directives: p.directives.clone(),
            });
        }
        cur = p.parent_id.as_deref();
    }
    out
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Link {
    pub id: String,
    pub src: String,
    pub dst: String,
    #[serde(default)]
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub member_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_group_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_node_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub responsibilities: Vec<Responsibility>,
    /// Optional lucide-react icon name override. Frontend-only meaning.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

// --- Model ---

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ScryModel {
    pub version: String,
    pub nodes: Vec<Node>,
    pub links: Vec<Link>,
    #[serde(default)]
    pub groups: Vec<Group>,
    /// Maps **responsibility id** → line-precise source locations (where reality
    /// discharges that responsibility — the conformance numerator), or **schema
    /// node id** → that type's declaration location. Agent-produced and
    /// regenerable; never hand-authored.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub source_map: HashMap<String, Vec<SourceLocation>>,
    /// Maps **responsibility id** → the locations of the tests attached to
    /// that claim. A separate dimension from `source_map` — where a claim is
    /// implemented vs. which tests are attached to it — and a claim may carry
    /// either, both, or neither. Attachment is the only fact recorded: scryer
    /// never runs the tests and never judges what they prove. Follows
    /// `source_map`'s single-home layer rule: committed owns committed claims'
    /// entries, the draft holds only plan-added ones. Agent-produced.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub test_map: HashMap<String, Vec<SourceLocation>>,
    /// Maps **node id** → boundary globs: the region of code a node owns (the
    /// coverage denominator + extraction scope). A child's boundary should sit
    /// within its parent's. Agent-produced and regenerable; never hand-authored.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub boundaries: HashMap<String, Vec<Source>>,
    /// The concern registry — one entry per concern slug used by any
    /// responsibility (see [`Responsibility::concern`]). Minted automatically
    /// on write ([`crate::concerns::register_concerns`]), curated by the user
    /// (description, icon, renames), never pruned automatically.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub concerns: Vec<crate::concerns::ConcernDef>,
    /// The open-change registry — named partitions of the plan, each carrying
    /// the dev's rationale. PLAN-LAYER ONLY: the committed model never carries
    /// change state ([`write_model_at`] strips it). See [`crate::changes`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub changes: Vec<changes::ChangeMeta>,
    /// Maps **element key** ([`changes::element_key`]) → change id: which
    /// change each pending plan entry belongs to. Untagged entries are the
    /// unfiled bucket (the zero-friction serial workflow). Plan-layer only,
    /// like `changes`; kept honest by [`changes::gc`].
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub change_map: HashMap<String, String>,
}

impl ScryModel {
    pub fn new() -> Self {
        Self {
            version: SCRY_VERSION.to_string(),
            nodes: Vec::new(),
            links: Vec::new(),
            groups: Vec::new(),
            source_map: HashMap::new(),
            test_map: HashMap::new(),
            boundaries: HashMap::new(),
            concerns: Vec::new(),
            changes: Vec::new(),
            change_map: HashMap::new(),
        }
    }
}

impl Default for ScryModel {
    fn default() -> Self {
        Self::new()
    }
}

// --- Test-anchor key namespace ---

/// Prefix distinguishing a `test_map` anchor from a `source_map` anchor in
/// shared key spaces (the anchor-fingerprint baseline, anchor observations).
/// Model maps themselves never carry it — `test_map` keys are bare
/// responsibility ids; the prefix exists so one baseline can fingerprint both
/// dimensions without a second file.
pub const TEST_KEY_PREFIX: &str = "test:";

/// The baseline/observation key for a responsibility's attached-test anchor.
pub fn test_key(resp_id: &str) -> String {
    format!("{TEST_KEY_PREFIX}{resp_id}")
}

/// The responsibility id behind a test-namespaced key, or `None` for a
/// plain source-map key.
pub fn test_resp_id(key: &str) -> Option<&str> {
    key.strip_prefix(TEST_KEY_PREFIX)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Legacy `.scry` files written before the schema/symbol merge stored data
    /// shapes as `"kind":"schema"`. The serde alias must load them as symbols
    /// with their properties intact — there is no migration step.
    #[test]
    fn legacy_schema_kind_loads_as_symbol() {
        let json = r#"{
            "id": "n1",
            "kind": "schema",
            "name": "LeadData",
            "properties": [{ "label": "phone", "status": "implemented" }]
        }"#;
        let node: Node = serde_json::from_str(json).unwrap();
        assert_eq!(node.kind, Kind::Symbol);
        assert_eq!(node.properties.len(), 1);
        assert_eq!(node.properties[0].label, "phone");
        // And it re-serializes under the canonical name.
        let out = serde_json::to_string(&node).unwrap();
        assert!(out.contains("\"kind\":\"symbol\""));
        assert!(!out.contains("schema"));
    }

    #[test]
    fn symbol_carries_both_responsibilities_and_properties() {
        let json = r#"{
            "id": "n2",
            "kind": "symbol",
            "name": "Projects",
            "responsibilities": [{ "id": "r1", "statement": "configures projects" }],
            "properties": [{ "label": "odooMapping" }]
        }"#;
        let node: Node = serde_json::from_str(json).unwrap();
        assert_eq!(node.kind, Kind::Symbol);
        assert_eq!(node.responsibilities.len(), 1);
        assert_eq!(node.properties.len(), 1);
    }

    fn mk_node(id: &str, name: &str, parent: Option<&str>) -> Node {
        Node {
            id: id.into(),
            kind: Kind::Component,
            name: name.into(),
            vagrant: None,
            stale: None,
            parent_id: parent.map(|s| s.into()),
            external: None,
            technology: None,
            description: None,
            responsibilities: Vec::new(),
            properties: Vec::new(),
            icon: None,
            visual: None,
            appearance: None,
            notes: None,
            position: None,
            directives: Vec::new(),
        }
    }

    #[test]
    fn inherited_directives_walks_ancestors_nearest_first_skipping_empty() {
        let mut m = ScryModel::default();
        let mut root = mk_node("root", "Root", None);
        root.directives = vec!["never log secrets".into()];
        let mid = mk_node("mid", "Mid", Some("root")); // no directives — skipped
        let mut leaf = mk_node("leaf", "Leaf", Some("mid"));
        leaf.directives = vec!["leaf own rule".into()]; // own, must be excluded
        m.nodes = vec![root, mid, leaf];

        let inh = inherited_directives(&m, "leaf");
        // Own directives excluded; empty `mid` skipped; only `root` contributes.
        assert_eq!(inh.len(), 1);
        assert_eq!(inh[0].node_id, "root");
        assert_eq!(inh[0].directives, vec!["never log secrets".to_string()]);

        // A root node inherits nothing.
        assert!(inherited_directives(&m, "root").is_empty());
    }

    #[test]
    fn inherited_directives_orders_nearest_ancestor_first() {
        let mut m = ScryModel::default();
        let mut root = mk_node("root", "Root", None);
        root.directives = vec!["root rule".into()];
        let mut mid = mk_node("mid", "Mid", Some("root"));
        mid.directives = vec!["mid rule".into()];
        let leaf = mk_node("leaf", "Leaf", Some("mid"));
        m.nodes = vec![root, mid, leaf];

        let inh = inherited_directives(&m, "leaf");
        assert_eq!(inh.iter().map(|i| i.node_id.as_str()).collect::<Vec<_>>(), vec!["mid", "root"]);
    }

    /// A symbol with no responsibilities, properties, or rendered appearance —
    /// and not external — is flagged empty. Any one of those, or being a
    /// structural node, gives it content of its own.
    #[test]
    fn a_contentless_internal_symbol_is_empty() {
        let node = |json: &str| -> Node { serde_json::from_str(json).unwrap() };
        assert!(is_node_empty(&node(r#"{ "id": "n1", "kind": "symbol", "name": "S" }"#)));

        assert!(!is_node_empty(&node(
            r#"{ "id": "n1", "kind": "symbol", "name": "S",
                 "responsibilities": [{ "id": "r1", "statement": "does X" }] }"#
        )));
        assert!(!is_node_empty(&node(
            r#"{ "id": "n1", "kind": "symbol", "name": "S",
                 "properties": [{ "label": "phone" }] }"#
        )));
        assert!(!is_node_empty(&node(
            r#"{ "id": "n1", "kind": "symbol", "name": "S",
                 "appearance": { "status": "implemented" } }"#
        )));
        assert!(!is_node_empty(&node(
            r#"{ "id": "n1", "kind": "symbol", "name": "S", "external": true }"#
        )));
        // Structural nodes carry meaning through their children — never "empty".
        assert!(!is_node_empty(&node(r#"{ "id": "n1", "kind": "component", "name": "C" }"#)));
    }
}
