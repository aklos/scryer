//! The C4 model's shape: nodes (person, system, container, component, symbol),
//! the links between them, each node's claims and data-shape
//! properties, the maps that tie claims and nodes to code (`source_map`,
//! `test_map`, `boundaries`), and the concern registry. The derived tree is a
//! [`ScryModel`]; the one fact read straight off one, an empty symbol, lives
//! beside it.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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

/// A pure business-responsibility statement. The `statement` field is the spec.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[schemars(description = "A responsibility: a verb-led business statement with optional concern tag.")]
pub struct Responsibility {
    pub id: String,
    /// Verb-led business statement of accountability. No mechanism words.
    /// EARS-shaped (condition first, response last) and may carry display
    /// markup — `**bold**` on the keyword and response verb — which the UI
    /// renders and strips for comparison (statement-ears).
    #[schemars(description = "Verb-led EARS statement, **bold** on the keyword and response verb.")]
    pub statement: String,
    /// The cross-cutting concern this responsibility serves — at most ONE
    /// kebab-case slug (e.g. "auth", "idempotency"), referencing an entry in
    /// the model's concern registry ([`ScryModel::concerns`]; entries are
    /// minted automatically on first use). Untagged means core domain flow —
    /// that absence is signal, not an omission. Metadata beside the statement,
    /// not part of it: no conformance role. See the concerns rule.
    #[schemars(description = "ONE kebab-case concern slug; omit for core domain flow.")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub concern: Option<String>,
    /// Why the behaviour is this one (what was rejected, what must stay
    /// true), as markdown: its test's doc after the claim. Derived, like a
    /// node's `decisions`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    pub decisions: Option<String>,
}

// --- Code-level data ---

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SchemaProperty {
    pub label: String,
    #[serde(default)]
    pub description: String,
    /// The declared type, as written in the source (`Option<String>`,
    /// `(id: string) => void`). Absent when the code declares none.
    #[serde(default, rename = "type", skip_serializing_if = "Option::is_none")]
    pub ty: Option<String>,
}

/// A source-file pointer attached to a node. Wide glob + optional comment;
/// distinct from [`SourceLocation`], which carries precise line numbers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Source {
    /// Glob pattern for matching files, e.g. "src/auth/**/*.rs"
    #[schemars(description = "Glob pattern, e.g. src/auth/**/*.rs.")]
    pub pattern: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SourceLocation {
    /// File path (relative to the project) the responsibility maps into.
    #[schemars(description = "Project-relative file path.")]
    pub pattern: String,
    /// Durable anchor: the identifier (function/handler/type/component name)
    /// that discharges the responsibility. The exact line range is resolved
    /// from this on demand, so it survives edits that shift line numbers.
    #[schemars(description = "Enclosing definition name; the durable anchor.")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_line: Option<u32>,
}

// --- Nodes and links ---

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
    /// Where the user dragged this node on its parent's map surface. Unset means
    /// auto-layout owns the placement; set means the canvas pins the node there
    /// and layout only routes around it. User-authored via the canvas: hidden
    /// from the agent's write-tool schemas (`schemars(skip)`) and restored from
    /// the prior model across raw whole-node writes. Cleared
    /// on reparent — coordinates are per-surface and don't survive the move.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    pub position: Option<Position>,
    /// The style governing this subtree: a known style name on a container (a component may override). Absent on person, system and symbol.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
    /// The layer this component plays in its governing style, from the style's layer list. Components only; symbols inherit it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer: Option<String>,
    /// The claims (responsibility ids) whose tests exercise this symbol: the
    /// test's body names it, and it is defined in the test's file or in a file
    /// the test imports. Derived; the claims stay on their component.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[schemars(skip)]
    pub exercised_by: Vec<String>,
    /// A container declared dev tooling in structure.toml (`tooling = true`):
    /// on the map, but no product. It needs no style, and its imports into
    /// other containers are never forbidden; its own cycles still count. A
    /// component outside its container's source root (examples, benches,
    /// build scripts) is tooling too, and needs no layer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    pub tooling: Option<bool>,
    /// The containers this container uses as one of its own layers, by
    /// container id (`as_layer` in structure.toml): a shared simulation a
    /// server hosts as its domain. Its imports of them are checked against
    /// its own matrix, and they are held to what that layer may depend on.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[schemars(skip)]
    pub as_layer: BTreeMap<String, String>,
    /// The decisions a later reader needs (why this shape, what was
    /// rejected, what must stay true), as markdown: a module doc after its
    /// first paragraph, or `ARCHITECTURE.md` after its first for the system.
    /// Derived, like the description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    pub decisions: Option<String>,
}

/// The `empty` flag — a SYMBOL that carries no semantic content of its own: no
/// responsibilities, no properties, and not external. Derived, never stored. Mirrors `isNodeEmpty` in the frontend (`src/viewmodel.ts`)
/// — keep the two in lockstep. Scoped to symbols: structural nodes
/// (system/container/component) carry their meaning through their children, so a
/// parent without its own responsibilities is not "empty" in this sense.
pub fn is_node_empty(node: &Node) -> bool {
    node.kind == Kind::Symbol
        && node.external != Some(true)
        && node.responsibilities.is_empty()
        && node.properties.is_empty()
}

/// What a link between two styled nodes IS; inside a styled container the kind is the meaning and `label` is optional prose.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum LinkKind {
    /// An adapter realises a port: infrastructure → the application interface it fulfils.
    Implements,
    /// A synchronous call into another node's public surface.
    Calls,
    /// A same-layer sibling reached through its public surface.
    Uses,
    /// A dependency on a type or value (an import that is not a call).
    Depends,
}

impl LinkKind {
    pub fn as_str(self) -> &'static str {
        match self {
            LinkKind::Implements => "implements",
            LinkKind::Calls => "calls",
            LinkKind::Uses => "uses",
            LinkKind::Depends => "depends",
        }
    }
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
    /// Required when both endpoints sit inside styled containers (component or symbol level); optional on prose links at system and container level.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<LinkKind>,
}

// --- Model ---

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ScryModel {
    pub version: String,
    pub nodes: Vec<Node>,
    pub links: Vec<Link>,
    /// Maps **responsibility id** → line-precise source locations (where reality
    /// discharges that responsibility — the conformance numerator), or **schema
    /// node id** → that type's declaration location. Agent-produced and
    /// regenerable; never hand-authored.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub source_map: BTreeMap<String, Vec<SourceLocation>>,
    /// Maps **responsibility id** → the locations of the tests attached to
    /// that claim. A separate dimension from `source_map` — where a claim is
    /// implemented vs. which tests are attached to it — and a claim may carry
    /// either, both, or neither. Attachment is the only fact recorded: scryer
    /// never runs the tests and never judges what they prove. Follows
    /// `source_map`'s single-home layer rule: committed owns committed claims'
    /// entries, the draft holds only plan-added ones. Agent-produced.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub test_map: BTreeMap<String, Vec<SourceLocation>>,
    /// Maps **node id** → boundary globs: the region of code a node owns (the
    /// coverage denominator + extraction scope). A child's boundary should sit
    /// within its parent's. Agent-produced and regenerable; never hand-authored.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub boundaries: BTreeMap<String, Vec<Source>>,
    /// The concern registry — one entry per concern slug used by any
    /// responsibility (see [`Responsibility::concern`]). Minted automatically
    /// on write ([`crate::concerns::register_concerns`]), curated by the user
    /// (description, icon, renames), never pruned automatically.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub concerns: Vec<ConcernDef>,
}

impl ScryModel {
    pub fn new() -> Self {
        Self {
            version: SCRY_VERSION.to_string(),
            nodes: Vec::new(),
            links: Vec::new(),
            source_map: BTreeMap::new(),
            test_map: BTreeMap::new(),
            boundaries: BTreeMap::new(),
            concerns: Vec::new(),
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

    /// A symbol with no responsibilities or properties — and not external — is
    /// flagged empty. Any one of those, or being a
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
            r#"{ "id": "n1", "kind": "symbol", "name": "S", "external": true }"#
        )));
        // Structural nodes carry meaning through their children — never "empty".
        assert!(!is_node_empty(&node(r#"{ "id": "n1", "kind": "component", "name": "C" }"#)));
    }
}

/// One entry in the model's concern registry: the single place a concern is
/// named and decorated. Responsibilities reference it by `slug`; renaming a
/// concern means rewriting the slug here AND on every tagged responsibility
/// (the registry entry is the concept, not per-responsibility text).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConcernDef {
    /// Kebab-case identifier, e.g. "auth", "failure-handling". Displayed as-is.
    pub slug: String,
    /// One line on what accountability the concern covers. Optional.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Lucide icon name (PascalCase, e.g. "Shield") — the glyph that prefixes
    /// every responsibility tagged with this concern. Falls back to a
    /// deterministic pick from `slug` when unset. Frontend-only meaning.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

