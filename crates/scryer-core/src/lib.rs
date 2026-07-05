pub mod build_edges;
pub mod diff;
pub mod drift;
pub mod health;
pub mod history;
pub mod ownership;
pub mod rules;
pub mod scan;
pub mod seed;
pub mod validate;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// On-disk schema version. Files with a different `version` field are refused at load time.
pub const SCRY_VERSION: &str = "0.3";

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
    pub statement: String,
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
    /// the implementation has to satisfy. User-authored: read-only to the agent,
    /// so hidden from write-tool input schemas (`schemars(skip)`) while still
    /// serialized for storage and surfaced on read. Not part of conformance.
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
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Source {
    /// Glob pattern for matching files, e.g. "src/auth/**/*.rs"
    pub pattern: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
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
    /// Node-level prescriptive HOW-constraints — verb-led "must"/"never" rules
    /// the implementation must satisfy, the node-altitude twin of a
    /// responsibility's `directives`. These CARRY DOWN: a node is bound by its
    /// own directives plus every ancestor's, computed at read time (never copied
    /// onto descendants), so editing a container's directive instantly re-binds
    /// its whole subtree. User-authored: read-only to the agent, so hidden from
    /// write-tool input schemas (`schemars(skip)`) while still serialized for
    /// storage and surfaced (own + inherited) on read. Plain text — not part of
    /// conformance.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[schemars(skip)]
    pub directives: Vec<String>,
}

/// The `empty` flag — a SYMBOL that carries no semantic content of its own: no
/// responsibilities, no properties, no rendered appearance, and not external.
/// Derived, never stored. Mirrors `isNodeEmpty` in the frontend (`src/rollup.ts`)
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
    /// Maps **node id** → boundary globs: the region of code a node owns (the
    /// coverage denominator + extraction scope). A child's boundary should sit
    /// within its parent's. Agent-produced and regenerable; never hand-authored.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub boundaries: HashMap<String, Vec<Source>>,
}

impl ScryModel {
    pub fn new() -> Self {
        Self {
            version: SCRY_VERSION.to_string(),
            nodes: Vec::new(),
            links: Vec::new(),
            groups: Vec::new(),
            source_map: HashMap::new(),
            boundaries: HashMap::new(),
        }
    }
}

impl Default for ScryModel {
    fn default() -> Self {
        Self::new()
    }
}

// --- ModelRef (project-local only in v0.3) ---

/// Identifies a model's storage location. v0.3 supports project-local models only;
/// the global `~/.scryer/{name}.scry` location from v0.2.x is gone.
#[derive(Debug, Clone, PartialEq)]
pub enum ModelRef {
    ProjectLocal(PathBuf),
}

impl ModelRef {
    /// Parse a ref string. Only `project:{path}` is accepted in v0.3.
    pub fn parse(s: &str) -> Result<Self, String> {
        if let Some(path) = s.strip_prefix("project:") {
            Ok(ModelRef::ProjectLocal(PathBuf::from(path)))
        } else {
            Err(format!(
                "Invalid model ref '{}'. Expected 'project:<path>'",
                s
            ))
        }
    }

    pub fn to_ref_string(&self) -> String {
        match self {
            ModelRef::ProjectLocal(path) => format!("project:{}", path.display()),
        }
    }

    pub fn project_path(&self) -> &Path {
        match self {
            ModelRef::ProjectLocal(path) => path,
        }
    }

    pub fn model_path(&self) -> PathBuf {
        match self {
            ModelRef::ProjectLocal(path) => path.join(".scryer").join("model.scry"),
        }
    }

    pub fn baseline_path(&self) -> PathBuf {
        match self {
            ModelRef::ProjectLocal(path) => path.join(".scryer").join("model.baseline.scry"),
        }
    }

    /// The PLANNED (draft) model — the intent the canvas and agent edit. The
    /// diff against `model.scry` (the committed source of truth) is the planning
    /// substrate. Absent until the first edit diverges from the model.
    pub fn planned_path(&self) -> PathBuf {
        match self {
            ModelRef::ProjectLocal(path) => path.join(".scryer").join("planned.scry"),
        }
    }

    pub fn lock_path(&self) -> PathBuf {
        match self {
            ModelRef::ProjectLocal(path) => path.join(".scryer").join(".lock"),
        }
    }

    pub fn sync_path(&self) -> PathBuf {
        match self {
            ModelRef::ProjectLocal(path) => path.join(".scryer").join(".sync"),
        }
    }

    /// The durable committed-model event log (append-only JSONL). Git-tracked
    /// like the model — see [`crate::history`].
    pub fn history_path(&self) -> PathBuf {
        match self {
            ModelRef::ProjectLocal(path) => path.join(".scryer").join("history.jsonl"),
        }
    }

    /// Where the deterministic codebase dependency graph is cached for the
    /// duration of a model build, so the MCP `fill_container` tool (a
    /// separate process from the build orchestrator) can wire code-level links
    /// from the same edges the agent saw — without re-parsing the project.
    pub fn build_edges_path(&self) -> PathBuf {
        match self {
            ModelRef::ProjectLocal(path) => path.join(".scryer").join(".build_edges.json"),
        }
    }

    /// The anchor fingerprint baseline — what every sourceMap anchor's span
    /// contained at the last reconcile (see `scryer_extract::anchors`).
    /// Regenerable, git-free, never hand-authored.
    pub fn anchors_path(&self) -> PathBuf {
        match self {
            ModelRef::ProjectLocal(path) => path.join(".scryer").join(".anchors.json"),
        }
    }

    pub fn dir(&self) -> PathBuf {
        match self {
            ModelRef::ProjectLocal(path) => path.join(".scryer"),
        }
    }

    pub fn display_name(&self) -> String {
        match self {
            ModelRef::ProjectLocal(path) => path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| path.display().to_string()),
        }
    }
}

impl std::fmt::Display for ModelRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_ref_string())
    }
}

// --- Storage ---

fn ensure_project_gitignore(scryer_dir: &Path) -> Result<(), String> {
    let gitignore = scryer_dir.join(".gitignore");
    if !gitignore.exists() {
        fs::write(
            &gitignore,
            "*.baseline.scry\n.sync\n.tmp.*\n.lock\n.anchors.json\n.build_edges.json\npreview/\n",
        )
        .map_err(|e| format!("Failed to create .gitignore: {}", e))?;
    }
    Ok(())
}

/// Check the `version` field on a raw model JSON; return Err with a clear
/// message if it isn't `SCRY_VERSION`.
fn check_version(v: &serde_json::Value) -> Result<(), String> {
    let version = v.get("version").and_then(|x| x.as_str()).unwrap_or("");
    if version != SCRY_VERSION {
        return Err(format!(
            "Model file uses schema version '{}', but this version of scryer requires '{}'. Legacy models cannot be loaded.",
            if version.is_empty() { "<missing>" } else { version },
            SCRY_VERSION
        ));
    }
    Ok(())
}

pub fn read_model_raw_at(r: &ModelRef) -> Result<String, String> {
    fs::read_to_string(&r.model_path()).map_err(|e| e.to_string())
}

pub fn read_model_at(r: &ModelRef) -> Result<ScryModel, String> {
    let raw = read_model_raw_at(r)?;
    let v: serde_json::Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    check_version(&v)?;
    serde_json::from_value(v).map_err(|e| e.to_string())
}

/// RAII guard holding the exclusive write lock for a model. The lock is an
/// advisory OS file lock on `.scryer/.lock`; it is released when this guard is
/// dropped (or the process exits, so a crash never strands it).
///
/// Hold it across the WHOLE read-modify-write cycle of a model edit. That
/// serializes concurrent writers — parallel agent sessions (each its own MCP
/// process) and the canvas — so they can't clobber each other, and it makes the
/// `max+1` id minters correct (the read and the write are atomic together, so
/// two writers can never observe the same max).
#[must_use = "the lock is released as soon as the guard is dropped"]
pub struct ModelLock {
    _file: fs::File,
}

/// Acquire the exclusive write lock for a model, blocking until it is available.
/// Creates the `.scryer` directory and lock file if absent.
pub fn lock_model(r: &ModelRef) -> Result<ModelLock, String> {
    let dir = r.dir();
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let file = fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(r.lock_path())
        .map_err(|e| format!("Failed to open model lock: {}", e))?;
    file.lock()
        .map_err(|e| format!("Failed to acquire model lock: {}", e))?;
    Ok(ModelLock { _file: file })
}

/// Write raw JSON to the model path. Uses an atomic temp-file + rename so the
/// frontend file watcher sees a single inotify event.
pub fn write_model_raw_at(r: &ModelRef, data: &str) -> Result<(), String> {
    let dir = r.dir();
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    ensure_project_gitignore(&dir)?;
    let model_path = r.model_path();
    let tmp = dir.join(".tmp.model.scry");
    fs::write(&tmp, data).map_err(|e| e.to_string())?;
    fs::rename(&tmp, &model_path).map_err(|e| e.to_string())
}

pub fn write_model_at(r: &ModelRef, model: &ScryModel) -> Result<(), String> {
    // Date each responsibility/property against the version currently on disk
    // (the prior, read under the same lock the caller holds), then write. This
    // is the agent-side fossilization clock; the canvas stamps its own edits in
    // the frontend mutation helpers before it writes raw JSON.
    let prior = read_model_at(r).ok();
    let mut stamped = model.clone();
    stamp_touches(&mut stamped, prior.as_ref(), drift::now_secs());
    let json = serde_json::to_string_pretty(&stamped).map_err(|e| e.to_string())?;
    write_model_raw_at(r, &json)
}

/// Whether two responsibilities differ in any *truth-bearing* field — the spec
/// statement, drift flags, or directives. Excludes `last_touched_at`
/// itself (that's the output) so an unchanged responsibility keeps its date.
fn resp_truth_changed(a: &Responsibility, b: &Responsibility) -> bool {
    a.statement != b.statement
        || a.vagrant != b.vagrant
        || a.stale != b.stale
        || a.directives != b.directives
}

/// Whether two properties differ in any truth-bearing field (label /
/// description). Excludes `last_touched_at`.
fn prop_truth_changed(a: &SchemaProperty, b: &SchemaProperty) -> bool {
    a.label != b.label
        || a.description != b.description
        || a.vagrant != b.vagrant
        || a.stale != b.stale
}

/// Stamp `last_touched_at = now` on every responsibility/property whose
/// truth-bearing content is new or changed relative to `prior`; carry the prior
/// date forward when the content is unchanged. With no prior (the very first
/// write of a model file) everything is stamped. Responsibilities are matched
/// per host (node/group) by id — ids are only unique within a host — and
/// properties per node by label. No layout lives on a responsibility/property,
/// so a pure position change can't reach here and won't re-date anything.
fn stamp_touches(model: &mut ScryModel, prior: Option<&ScryModel>, now: u64) {
    let node_resps: HashMap<&str, HashMap<&str, &Responsibility>> = prior
        .map(|p| {
            p.nodes
                .iter()
                .map(|n| {
                    let m: HashMap<&str, &Responsibility> =
                        n.responsibilities.iter().map(|r| (r.id.as_str(), r)).collect();
                    (n.id.as_str(), m)
                })
                .collect()
        })
        .unwrap_or_default();
    let node_props: HashMap<&str, HashMap<&str, &SchemaProperty>> = prior
        .map(|p| {
            p.nodes
                .iter()
                .map(|n| {
                    let m: HashMap<&str, &SchemaProperty> =
                        n.properties.iter().map(|pr| (pr.label.as_str(), pr)).collect();
                    (n.id.as_str(), m)
                })
                .collect()
        })
        .unwrap_or_default();
    let group_resps: HashMap<&str, HashMap<&str, &Responsibility>> = prior
        .map(|p| {
            p.groups
                .iter()
                .map(|g| {
                    let m: HashMap<&str, &Responsibility> =
                        g.responsibilities.iter().map(|r| (r.id.as_str(), r)).collect();
                    (g.id.as_str(), m)
                })
                .collect()
        })
        .unwrap_or_default();

    let date_resp = |r: &mut Responsibility, host: Option<&HashMap<&str, &Responsibility>>| {
        let prev = host.and_then(|m| m.get(r.id.as_str()).copied());
        r.last_touched_at = match prev {
            Some(pv) if !resp_truth_changed(pv, r) => pv.last_touched_at,
            _ => Some(now),
        };
    };

    for n in &mut model.nodes {
        let hr = node_resps.get(n.id.as_str());
        for r in &mut n.responsibilities {
            date_resp(r, hr);
        }
        let hp = node_props.get(n.id.as_str());
        for pr in &mut n.properties {
            let prev = hp.and_then(|m| m.get(pr.label.as_str()).copied());
            pr.last_touched_at = match prev {
                Some(pv) if !prop_truth_changed(pv, pr) => pv.last_touched_at,
                _ => Some(now),
            };
        }
    }
    for g in &mut model.groups {
        let hr = group_resps.get(g.id.as_str());
        for r in &mut g.responsibilities {
            date_resp(r, hr);
        }
    }
}

// --- Baseline snapshots (for MCP diff) ---

pub fn save_baseline_at(r: &ModelRef, model: &ScryModel) -> Result<(), String> {
    let dir = r.dir();
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(model).map_err(|e| e.to_string())?;
    fs::write(&r.baseline_path(), json).map_err(|e| e.to_string())
}

// --- Reconcile (drift) sync anchor ---

/// Read the drift reconcile anchor (`.scryer/.sync`). Returns the default
/// (epoch 0, no commit) when absent or unparseable — which makes a first drift
/// check examine everything.
pub fn read_sync_state(r: &ModelRef) -> drift::SyncState {
    fs::read_to_string(r.sync_path())
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

/// Write the drift reconcile anchor. Best-effort, non-atomic (like the baseline).
pub fn write_sync_state(r: &ModelRef, state: &drift::SyncState) -> Result<(), String> {
    let dir = r.dir();
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(state).map_err(|e| e.to_string())?;
    fs::write(r.sync_path(), json).map_err(|e| e.to_string())
}

/// Read the baseline snapshot. Returns None if absent or version-mismatched.
pub fn read_baseline_at(r: &ModelRef) -> Option<ScryModel> {
    let raw = fs::read_to_string(&r.baseline_path()).ok()?;
    let v: serde_json::Value = serde_json::from_str(&raw).ok()?;
    if check_version(&v).is_err() {
        return None;
    }
    serde_json::from_value(v).ok()
}

// --- Planned (draft) layer + plan diff ---

/// Write raw JSON to the planned path. Atomic temp-file + rename, like the model
/// write, so the frontend watcher sees a single event.
pub fn write_planned_raw_at(r: &ModelRef, data: &str) -> Result<(), String> {
    let dir = r.dir();
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    ensure_project_gitignore(&dir)?;
    let tmp = dir.join(".tmp.planned.scry");
    fs::write(&tmp, data).map_err(|e| e.to_string())?;
    fs::rename(&tmp, &r.planned_path()).map_err(|e| e.to_string())
}

/// Read the raw planned JSON, byte-for-byte. Falls back to the committed model's
/// raw bytes when no planned file exists yet (planned == model), so the frontend
/// can echo-dedup its own writes against exactly what it wrote.
pub fn read_planned_raw_at(r: &ModelRef) -> Result<String, String> {
    let path = r.planned_path();
    if !path.exists() {
        return read_model_raw_at(r);
    }
    fs::read_to_string(&path).map_err(|e| e.to_string())
}

/// Read the planned (draft) model. Falls back to the committed model when no
/// planned file exists yet — a fresh project has an empty plan (planned == model),
/// so the plan diff is empty.
pub fn read_planned_at(r: &ModelRef) -> Result<ScryModel, String> {
    let path = r.planned_path();
    if !path.exists() {
        return read_model_at(r);
    }
    let raw = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let v: serde_json::Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    check_version(&v)?;
    serde_json::from_value(v).map_err(|e| e.to_string())
}

/// Write the planned (draft) model, stamping fossilization dates against the
/// prior planned version (mirrors [`write_model_at`]). Hold the model lock across
/// the read-modify-write, exactly as for the committed model.
pub fn write_planned_at(r: &ModelRef, model: &ScryModel) -> Result<(), String> {
    let prior = read_planned_at(r).ok();
    let mut stamped = model.clone();
    stamp_touches(&mut stamped, prior.as_ref(), drift::now_secs());
    let json = serde_json::to_string_pretty(&stamped).map_err(|e| e.to_string())?;
    write_planned_raw_at(r, &json)
}

/// Seed the planned file from the committed model when absent, so the plan starts
/// empty (planned == model). No-op if planned already exists.
pub fn ensure_planned_at(r: &ModelRef) -> Result<(), String> {
    if r.planned_path().exists() {
        return Ok(());
    }
    let mut model = read_model_at(r)?;
    // Code-side mapping has a single home: the committed model owns every
    // committed element's anchor, and the plan overlays anchors only for the
    // elements it later adds. A fresh plan adds nothing, so it starts with no
    // anchors of its own — the working view reads committed's directly.
    model.source_map.clear();
    model.boundaries.clear();
    let json = serde_json::to_string_pretty(&model).map_err(|e| e.to_string())?;
    write_planned_raw_at(r, &json)
}

/// The plan diff: how the draft (`planned`) diverges from the committed `model` —
/// the planning substrate. Empty when there is no pending plan.
pub fn plan_diff_at(r: &ModelRef) -> Result<diff::ModelDiff, String> {
    let model = read_model_at(r)?;
    let planned = read_planned_at(r)?;
    Ok(diff::diff(&model, &planned))
}

/// Locate a responsibility by id anywhere in a model, returning its host id
/// (node or group) and a clone. Responsibility ids are globally unique (the
/// minters seed past every node- and group-owned id), so this is unambiguous.
fn find_responsibility(model: &ScryModel, id: &str) -> Option<(String, Responsibility)> {
    for n in &model.nodes {
        if let Some(r) = n.responsibilities.iter().find(|r| r.id == id) {
            return Some((n.id.clone(), r.clone()));
        }
    }
    for g in &model.groups {
        if let Some(r) = g.responsibilities.iter().find(|r| r.id == id) {
            return Some((g.id.clone(), r.clone()));
        }
    }
    None
}

/// Auto-commit a single planned element into the committed model — the fold that
/// fires when an element's code is implemented (planned → model). Remove-then-
/// insert, so one path handles add, update, move, AND delete:
///
///   - planned still holds the element → upsert it into the model at its planned
///     home (a reparent/move comes along for free: the planned copy carries its
///     new `parent_id` / host).
///   - planned no longer holds it → a committed deletion: drop it from the model.
///
/// On a committed deletion the element is also purged from the planned mirror, so
/// the plan clears. On an upsert, planned already mirrors the element, so it is
/// left as-is (the diff for it goes empty automatically).
///
/// `owner_id` is required only for properties (their `(owner node, label)`
/// identity); for responsibilities the host is derived from planned. Hold the
/// model lock across the call.
///
/// (When the explicit delete tombstone lands, a tombstoned element routes through
/// the same delete branch — one added `.filter(|x| !deleted)` at each lookup.)
/// Strip planned-layer review markers from a responsibility entering the
/// committed model. The committed model is the source of truth and carries
/// neither the `vagrant` adoption marker nor the `stale`/`stale_proposal` drift
/// markers — a fold IS the verdict that resolves them (re-implementation clears
/// stale; an explicit fold adopts). Audit #5.
fn clean_committed_resp(mut resp: Responsibility) -> Responsibility {
    resp.vagrant = None;
    resp.stale = None;
    resp.stale_proposal = None;
    resp
}

/// The committed copy of a planned node folded by `mark_implemented` (whole-node
/// fold). Enforces the "committed never carries review state" invariant: clears
/// the node's own `vagrant`/`stale` markers, DROPS un-adjudicated `vagrant`
/// responsibilities and properties (a bulk fold must not silently commit
/// code-discovered claims that still await an explicit adopt/reject verdict —
/// they stay in the plan), and clears the `stale`/`stale_proposal` drift markers
/// on everything that does fold. Audit #5.
fn committed_node_copy(n: &Node) -> Node {
    let mut copy = n.clone();
    copy.vagrant = None;
    copy.stale = None;
    copy.responsibilities = n
        .responsibilities
        .iter()
        .filter(|r| r.vagrant != Some(true))
        .cloned()
        .map(clean_committed_resp)
        .collect();
    copy.properties = n
        .properties
        .iter()
        .filter(|p| p.vagrant != Some(true))
        .cloned()
        .map(|mut p| {
            p.stale = None;
            p
        })
        .collect();
    copy
}

/// The committed copy of a planned group folded into the model. A group has no
/// review markers of its own, but it CAN carry responsibilities (a container
/// group's shared claims — "both surfaces deploy as one Next.js app"), so it
/// gets the same treatment `committed_node_copy` gives a node: drop
/// un-adjudicated `vagrant` claims (they stay in the plan awaiting a verdict)
/// and clear `stale`/`stale_proposal` on everything that folds. Audit #5 / item A.
fn committed_group_copy(g: &Group) -> Group {
    let mut copy = g.clone();
    copy.responsibilities = g
        .responsibilities
        .iter()
        .filter(|r| r.vagrant != Some(true))
        .cloned()
        .map(clean_committed_resp)
        .collect();
    copy
}

pub fn commit_element(
    r: &ModelRef,
    kind: diff::ElementKind,
    owner_id: Option<&str>,
    id: &str,
) -> Result<(), String> {
    let mut model = read_model_at(r)?;
    let planned = read_planned_at(r)?;
    let mut purge_from_planned = false;
    // Node ids removed by a DELETE fold — the target plus the subtree/links the
    // plan agrees are gone (item C) — and the responsibility ids they carried.
    // Held so the anchor-lockstep step below can GC their orphaned source-map
    // entries (the elements vanish, but their anchors are keyed separately and
    // would otherwise leak).
    let mut deleted_node_ids: Vec<String> = Vec::new();
    let mut deleted_node_resp_ids: Vec<String> = Vec::new();

    match kind {
        diff::ElementKind::Node => {
            match planned.nodes.iter().find(|n| n.id == id) {
                Some(n) => {
                    // An add/reword fold. The node's parent must already live in
                    // committed, or the folded node dangles off a plan-only id:
                    // outline_tree can't reach it from any root, so it vanishes
                    // from every committed read. Fold top-down (the Responsibility
                    // branch makes the same host-residence check). Item B.
                    if let Some(pid) = &n.parent_id {
                        if !model.nodes.iter().any(|p| &p.id == pid && p.id != *id) {
                            return Err(format!(
                                "cannot commit node '{id}': its parent '{pid}' is not in the \
                                 committed model yet (commit the parent first)"
                            ));
                        }
                    }
                    model.nodes.retain(|n| n.id != id);
                    model.nodes.push(committed_node_copy(n));
                }
                None => {
                    // A DELETE fold. delete_nodes removed the node, its whole
                    // subtree, the links touching it, and its group memberships
                    // from the PLAN; the fold must mirror that on committed or the
                    // children reparent to a dead id (silently promoted to health
                    // roots), links dangle, and group refs go stale — the exact
                    // orphaning of item C. Scope removal to the subtree the plan
                    // AGREES is gone (absent from the plan), so a still-present
                    // child isn't clobbered into a phantom re-add.
                    let removed: std::collections::HashSet<String> =
                        drift::subtree_ids(&model, id)
                            .into_iter()
                            .filter(|nid| !planned.nodes.iter().any(|n| &n.id == nid))
                            .collect();
                    deleted_node_resp_ids = model
                        .nodes
                        .iter()
                        .filter(|n| removed.contains(&n.id))
                        .flat_map(|n| n.responsibilities.iter().map(|r| r.id.clone()))
                        .collect();
                    model.nodes.retain(|n| !removed.contains(&n.id));
                    model
                        .links
                        .retain(|l| !removed.contains(&l.src) && !removed.contains(&l.dst));
                    for g in &mut model.groups {
                        g.member_ids.retain(|m| !removed.contains(m));
                    }
                    model.boundaries.retain(|k, _| !removed.contains(k));
                    deleted_node_ids = removed.into_iter().collect();
                    purge_from_planned = true;
                }
            }
        }
        diff::ElementKind::Link => {
            model.links.retain(|l| l.id != id);
            match planned.links.iter().find(|l| l.id == id) {
                Some(l) => model.links.push(l.clone()),
                None => purge_from_planned = true,
            }
        }
        diff::ElementKind::Group => {
            // A group deletion orphans the anchors of the claims it carried, the
            // same way a node deletion does — hold their ids for the GC below.
            if !planned.groups.iter().any(|g| g.id == id) {
                if let Some(g) = model.groups.iter().find(|g| g.id == id) {
                    deleted_node_resp_ids =
                        g.responsibilities.iter().map(|r| r.id.clone()).collect();
                }
            }
            model.groups.retain(|g| g.id != id);
            match planned.groups.iter().find(|g| g.id == id) {
                Some(g) => model.groups.push(committed_group_copy(g)),
                None => purge_from_planned = true,
            }
        }
        diff::ElementKind::Responsibility => {
            for n in &mut model.nodes {
                n.responsibilities.retain(|x| x.id != id);
            }
            for g in &mut model.groups {
                g.responsibilities.retain(|x| x.id != id);
            }
            match find_responsibility(&planned, id) {
                Some((host, resp)) => {
                    let resp = clean_committed_resp(resp);
                    if let Some(n) = model.nodes.iter_mut().find(|n| n.id == host) {
                        n.responsibilities.push(resp);
                    } else if let Some(g) = model.groups.iter_mut().find(|g| g.id == host) {
                        g.responsibilities.push(resp);
                    } else {
                        return Err(format!(
                            "cannot commit responsibility '{id}': its host '{host}' is not in \
                             the committed model yet (commit the host node/group first)"
                        ));
                    }
                }
                None => purge_from_planned = true,
            }
        }
        diff::ElementKind::Property => {
            let owner = owner_id
                .ok_or_else(|| "committing a property requires its owner node id".to_string())?;
            let node = model
                .nodes
                .iter_mut()
                .find(|n| n.id == owner)
                .ok_or_else(|| {
                    format!("cannot commit property '{id}': owner node '{owner}' not in the model")
                })?;
            node.properties.retain(|p| p.label != id);
            // Upsert from planned if present there; absence is a committed delete,
            // already handled by the retain above.
            if let Some(p) = planned
                .nodes
                .iter()
                .find(|n| n.id == owner)
                .and_then(|n| n.properties.iter().find(|p| p.label == id))
            {
                // Committed carries no review markers — an explicit property fold
                // adopts it and resolves any drift flag. Audit #5.
                node.properties.push(SchemaProperty {
                    vagrant: None,
                    stale: None,
                    ..p.clone()
                });
            }
        }
    }

    // Keep the code-side anchor in lockstep with the element being folded.
    // Anchors have a single home: committed owns committed elements', the draft
    // owns only the elements it adds. So folding MOVES a plan-added element's
    // anchor into committed and strips it from the draft; a committed element
    // already keeps its anchor in committed, so it's left untouched — NOT removed
    // just because the draft doesn't carry it (that would silently unanchor a
    // reworded claim). A deletion drops the anchor from committed outright.
    let mut planned_anchor_strip: Vec<String> = Vec::new();
    match kind {
        diff::ElementKind::Responsibility => {
            if purge_from_planned {
                model.source_map.remove(id);
            } else if let Some(locs) = planned.source_map.get(id) {
                model.source_map.insert(id.to_string(), locs.clone());
                planned_anchor_strip.push(id.to_string());
            }
        }
        diff::ElementKind::Node => {
            if purge_from_planned {
                // Deletion: drop the declaration anchor of every removed node in
                // the subtree AND the anchors of every responsibility they carried
                // (orphaned otherwise). Item C.
                for nid in &deleted_node_ids {
                    model.source_map.remove(nid);
                }
                for rid in &deleted_node_resp_ids {
                    model.source_map.remove(rid);
                }
            } else if let Some(n) = planned.nodes.iter().find(|n| n.id == id) {
                // The node's own declaration anchor, plus every responsibility it
                // carries — committing the node moves the draft's across. Vagrant
                // claims don't fold (committed_node_copy drops them), so their
                // anchors stay in the draft alongside them. Audit #5.
                for k in std::iter::once(id.to_string()).chain(
                    n.responsibilities
                        .iter()
                        .filter(|r| r.vagrant != Some(true))
                        .map(|r| r.id.clone()),
                ) {
                    if let Some(locs) = planned.source_map.get(&k) {
                        model.source_map.insert(k.clone(), locs.clone());
                        planned_anchor_strip.push(k);
                    }
                }
            }
        }
        diff::ElementKind::Group => {
            // A group carries no declaration anchor of its own, but its
            // responsibilities do — move the non-vagrant ones across (or GC them
            // all on a group deletion), mirroring the Node branch. Item A.
            if purge_from_planned {
                for rid in &deleted_node_resp_ids {
                    model.source_map.remove(rid);
                }
            } else if let Some(g) = planned.groups.iter().find(|g| g.id == id) {
                for r in g.responsibilities.iter().filter(|r| r.vagrant != Some(true)) {
                    if let Some(locs) = planned.source_map.get(&r.id) {
                        model.source_map.insert(r.id.clone(), locs.clone());
                        planned_anchor_strip.push(r.id.clone());
                    }
                }
            }
        }
        _ => {}
    }

    write_model_at(r, &model)?;

    // Rewrite the draft when the fold removes the element (a committed deletion)
    // OR when it moved an anchor out of the draft into committed — either way the
    // draft must no longer carry it, so the single-home invariant holds.
    if purge_from_planned || !planned_anchor_strip.is_empty() {
        let mut p = planned;
        if purge_from_planned {
            match kind {
                diff::ElementKind::Node => p.nodes.retain(|n| n.id != id),
                diff::ElementKind::Link => p.links.retain(|l| l.id != id),
                diff::ElementKind::Group => p.groups.retain(|g| g.id != id),
                diff::ElementKind::Responsibility => {
                    for n in &mut p.nodes {
                        n.responsibilities.retain(|x| x.id != id);
                    }
                    for g in &mut p.groups {
                        g.responsibilities.retain(|x| x.id != id);
                    }
                }
                diff::ElementKind::Property => {}
            }
        }
        for k in &planned_anchor_strip {
            p.source_map.remove(k);
        }
        let json = serde_json::to_string_pretty(&p).map_err(|e| e.to_string())?;
        write_planned_raw_at(r, &json)?;
    }

    Ok(())
}

/// After a node is folded into the committed model, pull in the plan-added links
/// and groups that THIS node's commit has just made "ready". Links and groups
/// have no node id of their own, so `mark_implemented` (keyed by node) can never
/// fold them directly — without this a plan carrying `add_links` output or a
/// group keeps `toImplement` above zero forever and the CLOSE loop never
/// terminates (audit Theme 1 / item A).
///
/// "Ready" is deliberately scoped to dependents INCIDENT to the just-committed
/// node, and only when their other endpoints/members are already committed:
/// - a link is folded when it touches `node_id` (as src or dst) and both of its
///   endpoints live in committed — the edge's code rides with its endpoints, so
///   folding one of them is the natural moment to fold the edge;
/// - a group is folded when it contains `node_id` and every member is committed.
///
/// Deletions are intentionally excluded: folding a link/group removal on an
/// unrelated node fold would commit a removal whose code may not be gone yet.
/// A node-scoped delete cascade owns that path.
pub fn commit_ready_dependents(r: &ModelRef, node_id: &str) -> Result<(), String> {
    let committed = read_model_at(r)?;
    let committed_ids: std::collections::HashSet<&str> =
        committed.nodes.iter().map(|n| n.id.as_str()).collect();
    // Nothing became reachable if the node itself isn't committed (e.g. this was
    // a deletion fold, which removes rather than adds).
    if !committed_ids.contains(node_id) {
        return Ok(());
    }
    let planned = read_planned_at(r)?;
    let plan = diff::diff(&committed, &planned);

    let is_deletion =
        |c: &diff::ElementChange| c.changes.iter().any(|ch| matches!(ch, diff::Change::Deleted));

    let ready_links: Vec<String> = plan
        .changes
        .iter()
        .filter(|c| c.kind == diff::ElementKind::Link && !is_deletion(c))
        .filter_map(|c| planned.links.iter().find(|l| l.id == c.id))
        .filter(|l| l.src == node_id || l.dst == node_id)
        .filter(|l| {
            committed_ids.contains(l.src.as_str()) && committed_ids.contains(l.dst.as_str())
        })
        .map(|l| l.id.clone())
        .collect();
    for id in ready_links {
        commit_element(r, diff::ElementKind::Link, None, &id)?;
    }

    let ready_groups: Vec<String> = plan
        .changes
        .iter()
        .filter(|c| c.kind == diff::ElementKind::Group && !is_deletion(c))
        .filter_map(|c| planned.groups.iter().find(|g| g.id == c.id))
        .filter(|g| g.member_ids.iter().any(|m| m == node_id))
        .filter(|g| g.member_ids.iter().all(|m| committed_ids.contains(m.as_str())))
        .map(|g| g.id.clone())
        .collect();
    for id in ready_groups {
        commit_element(r, diff::ElementKind::Group, None, &id)?;
    }

    Ok(())
}

pub fn delete_model_at(r: &ModelRef) -> Result<(), String> {
    let model_path = r.model_path();
    if model_path.exists() {
        fs::remove_file(&model_path).map_err(|e| e.to_string())?;
    }
    let baseline = r.baseline_path();
    if baseline.exists() {
        let _ = fs::remove_file(&baseline);
    }
    let sync = r.sync_path();
    if sync.exists() {
        let _ = fs::remove_file(&sync);
    }
    Ok(())
}

// --- Project model resolution ---

/// Returns `Some(ProjectLocal)` if `{project}/.scryer/model.scry` exists.
/// Does NOT validate the file's version — use [`is_legacy_model`] for that.
pub fn resolve_project_model(project_path: &Path) -> Option<ModelRef> {
    if project_path.join(".scryer").join("model.scry").exists() {
        Some(ModelRef::ProjectLocal(project_path.to_path_buf()))
    } else {
        None
    }
}

/// True iff `{project}/.scryer/model.scry` exists with a version other than the
/// current `SCRY_VERSION` (or no version field at all, or unparseable JSON).
pub fn is_legacy_model(project_path: &Path) -> bool {
    let model_path = project_path.join(".scryer").join("model.scry");
    let Ok(raw) = fs::read_to_string(&model_path) else {
        return false;
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return true;
    };
    v.get("version").and_then(|x| x.as_str()).unwrap_or("") != SCRY_VERSION
}

// --- ID helpers ---

pub fn next_node_id(model: &ScryModel) -> String {
    let max = model
        .nodes
        .iter()
        .filter_map(|n| n.id.strip_prefix("node-").and_then(|s| s.parse::<u64>().ok()))
        .max()
        .unwrap_or(0);
    format!("node-{}", max + 1)
}

pub fn make_link_id(src: &str, dst: &str) -> String {
    format!("link-{}-{}", src, dst)
}

pub fn next_link_id(model: &ScryModel) -> String {
    let max = model
        .links
        .iter()
        .filter_map(|l| l.id.strip_prefix("link-").and_then(|s| s.parse::<u64>().ok()))
        .max()
        .unwrap_or(0);
    format!("link-{}", max + 1)
}

pub fn next_group_id(model: &ScryModel) -> String {
    let max = model
        .groups
        .iter()
        .filter_map(|g| g.id.strip_prefix("group-").and_then(|s| s.parse::<u64>().ok()))
        .max()
        .unwrap_or(0);
    format!("group-{}", max + 1)
}

pub fn next_responsibility_id(existing: &[Responsibility]) -> String {
    let max = existing
        .iter()
        .filter_map(|r| r.id.strip_prefix("resp-").and_then(|s| s.parse::<u64>().ok()))
        .max()
        .unwrap_or(0);
    format!("resp-{}", max + 1)
}


// --- Subagent settings (global, ~/.scryer/settings.json) ---

/// Global scryer config directory (`~/.scryer`). Distinct from each project's
/// own `.scryer/` directory, which holds that project's `model.scry`.
pub fn global_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".scryer")
}

/// Per-agent model + reasoning effort. An empty model means "use the agent
/// CLI's own default". Effort values are agent-specific (Claude accepts
/// low/medium/high/xhigh/max; Codex accepts minimal/low/medium/high).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSettings {
    #[serde(default)]
    pub model: String,
    #[serde(default = "default_effort")]
    pub effort: String,
}

impl Default for AgentSettings {
    fn default() -> Self {
        Self {
            model: String::new(),
            effort: default_effort(),
        }
    }
}

/// Agent preference + each agent's own settings, applied to spawned fill
/// sessions. Field-level serde defaults keep older/partial settings.json files
/// loadable.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubagentSettings {
    /// Which agent to launch: "auto" | "claudeCode" | "codex".
    #[serde(default = "default_agent")]
    pub agent: String,
    #[serde(default)]
    pub claude: AgentSettings,
    #[serde(default)]
    pub codex: AgentSettings,
    /// Confirm before any UI action launches an agent (a billable run). Lets the
    /// user see which agent + model + effort will run; "don't ask again" clears
    /// it. Defaults to true so the gate is opt-out, not opt-in.
    #[serde(default = "default_confirm_launch")]
    pub confirm_launch: bool,
}

impl Default for SubagentSettings {
    fn default() -> Self {
        Self {
            agent: default_agent(),
            claude: AgentSettings::default(),
            codex: AgentSettings::default(),
            confirm_launch: default_confirm_launch(),
        }
    }
}

fn default_agent() -> String {
    "auto".to_string()
}

fn default_confirm_launch() -> bool {
    true
}

fn default_effort() -> String {
    "medium".to_string()
}

fn settings_path() -> PathBuf {
    global_dir().join("settings.json")
}

pub fn read_subagent_settings() -> SubagentSettings {
    let path = settings_path();
    if !path.exists() {
        return SubagentSettings::default();
    }
    fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn write_subagent_settings(settings: &SubagentSettings) -> Result<(), String> {
    let dir = global_dir();
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    fs::write(settings_path(), json).map_err(|e| e.to_string())
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

    fn one_resp_model(statement: &str) -> ScryModel {
        let mut m = ScryModel::new();
        m.nodes.push(Node {
            id: "n1".into(),
            kind: Kind::Component,
            name: "C".into(),
            vagrant: None,
            stale: None,
            parent_id: None,
            external: None,
            technology: None,
            description: None,
            responsibilities: vec![Responsibility {
                id: "r1".into(),
                statement: statement.into(),
                vagrant: None,
                stale: None,
                stale_proposal: None,
                directives: Vec::new(),
                last_touched_at: None,
            }],
            properties: Vec::new(),
            icon: None,
            visual: None,
            appearance: None,
            notes: None,
            directives: Vec::new(),
        });
        m
    }

    /// The fossilization clock: a responsibility is dated when first written and
    /// when its truth changes, but a cosmetic edit carries the date forward.
    #[test]
    fn stamp_touches_dates_only_truth_changes() {
        // First write (no prior): the responsibility gets dated.
        let mut m = one_resp_model("does X");
        stamp_touches(&mut m, None, 100);
        assert_eq!(m.nodes[0].responsibilities[0].last_touched_at, Some(100));

        // Re-write with identical truth but a cosmetic change (icon): the date is
        // carried forward, not bumped — a non-truth edit is not a touch.
        let prior = m.clone();
        let mut moved = m.clone();
        moved.nodes[0].icon = Some("Box".into());
        stamp_touches(&mut moved, Some(&prior), 200);
        assert_eq!(
            moved.nodes[0].responsibilities[0].last_touched_at,
            Some(100),
            "a cosmetic-only change must not re-date the responsibility"
        );

        // Edit the statement: the responsibility is re-dated to now.
        let prior = moved.clone();
        let mut edited = one_resp_model("does Y");
        stamp_touches(&mut edited, Some(&prior), 300);
        assert_eq!(
            edited.nodes[0].responsibilities[0].last_touched_at,
            Some(300),
            "a changed statement re-dates the responsibility"
        );
    }
}

#[cfg(test)]
mod lock_tests {
    use super::*;

    fn temp_ref() -> (tempfile::TempDir, ModelRef) {
        let dir = tempfile::tempdir().unwrap();
        let r = ModelRef::ProjectLocal(dir.path().to_path_buf());
        (dir, r)
    }

    /// A second independent handle must fail to take the lock while a guard is
    /// held, and succeed once it is released.
    #[test]
    fn lock_is_exclusive_across_handles() {
        let (_dir, r) = temp_ref();
        let guard = lock_model(&r).unwrap();
        let other = fs::OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(r.lock_path())
            .unwrap();
        assert!(
            other.try_lock().is_err(),
            "a second handle must not acquire the lock while it is held"
        );
        drop(guard);
        assert!(
            other.try_lock().is_ok(),
            "the lock is free once the guard is dropped"
        );
    }

    /// Concurrent read-modify-write under the lock never loses an update: N
    /// writers each add one node, and all N land with unique ids — exactly the
    /// parallel-agent-writes case that clobbered without the lock (two writers
    /// reading the same model would both mint the same `next_node_id`).
    #[test]
    fn concurrent_writes_do_not_clobber() {
        let (_dir, r) = temp_ref();
        write_model_at(&r, &ScryModel::new()).unwrap();

        const N: usize = 12;
        std::thread::scope(|s| {
            for i in 0..N {
                let r = r.clone();
                s.spawn(move || {
                    let _lock = lock_model(&r).unwrap();
                    let mut m = read_model_at(&r).unwrap();
                    let id = next_node_id(&m);
                    let node = Node {
                        id,
                        kind: Kind::System,
                        name: format!("n{i}"),
                        vagrant: None,
                        stale: None,
                        parent_id: None,
                        external: None,
                        technology: None,
                        description: None,
                        responsibilities: Vec::new(),
                        properties: Vec::new(),
                        icon: None,
                        visual: None,
                        appearance: None,
                        notes: None,
                        directives: Vec::new(),
                    };
                    m.nodes.push(node);
                    write_model_at(&r, &m).unwrap();
                });
            }
        });

        let m = read_model_at(&r).unwrap();
        assert_eq!(m.nodes.len(), N, "every concurrent write landed (no lost updates)");
        let ids: std::collections::HashSet<&str> =
            m.nodes.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(ids.len(), N, "all minted ids are unique (no collision)");
    }

    /// With no planned file, the plan diff is empty (planned falls back to model).
    /// After seeding and diverging the draft, the diff reports the change.
    #[test]
    fn plan_diff_tracks_draft_divergence() {
        let (_dir, r) = temp_ref();
        let mut m = ScryModel::new();
        m.nodes.push(Node {
            id: "n1".into(),
            kind: Kind::System,
            name: "Auth".into(),
            vagrant: None,
            stale: None,
            parent_id: None,
            external: None,
            technology: None,
            description: None,
            responsibilities: Vec::new(),
            properties: Vec::new(),
            icon: None,
            visual: None,
            appearance: None,
            notes: None,
            directives: Vec::new(),
        });
        write_model_at(&r, &m).unwrap();

        // No planned file yet → empty plan.
        assert!(plan_diff_at(&r).unwrap().is_empty());

        // Seed the draft from the model, then add a node to the draft.
        ensure_planned_at(&r).unwrap();
        let mut planned = read_planned_at(&r).unwrap();
        planned.nodes.push(Node {
            id: "n2".into(),
            kind: Kind::System,
            name: "Billing".into(),
            vagrant: None,
            stale: None,
            parent_id: None,
            external: None,
            technology: None,
            description: None,
            responsibilities: Vec::new(),
            properties: Vec::new(),
            icon: None,
            visual: None,
            appearance: None,
            notes: None,
            directives: Vec::new(),
        });
        write_planned_at(&r, &planned).unwrap();

        let d = plan_diff_at(&r).unwrap();
        assert_eq!(d.changes.len(), 1);
        assert_eq!(d.changes[0].id, "n2");
        assert_eq!(d.changes[0].changes, vec![diff::Change::Added]);
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
            directives: Vec::new(),
        }
    }

    fn mk_resp(id: &str, statement: &str) -> Responsibility {
        Responsibility {
            id: id.into(),
            statement: statement.into(),
            vagrant: None,
            stale: None,
            stale_proposal: None,
            directives: Vec::new(),
            last_touched_at: None,
        }
    }

    /// Committing an added/renamed node folds the draft into the model; once
    /// committed the plan diff for it goes empty.
    #[test]
    fn commit_node_add_and_update() {
        let (_dir, r) = temp_ref();
        let mut m = ScryModel::new();
        m.nodes.push(mk_node("n1", "Old", None));
        write_model_at(&r, &m).unwrap();

        ensure_planned_at(&r).unwrap();
        let mut planned = read_planned_at(&r).unwrap();
        planned.nodes[0].name = "New".into(); // rename n1
        planned.nodes.push(mk_node("n2", "Billing", None)); // add n2
        write_planned_at(&r, &planned).unwrap();

        commit_element(&r, diff::ElementKind::Node, None, "n1").unwrap();
        commit_element(&r, diff::ElementKind::Node, None, "n2").unwrap();

        let model = read_model_at(&r).unwrap();
        assert_eq!(model.nodes.iter().find(|n| n.id == "n1").unwrap().name, "New");
        assert!(model.nodes.iter().any(|n| n.id == "n2"));
        assert!(plan_diff_at(&r).unwrap().is_empty(), "plan clears after commit");
    }

    /// Committing a node that the draft dropped removes it from the model and
    /// purges it from the plan.
    #[test]
    fn commit_node_delete() {
        let (_dir, r) = temp_ref();
        let mut m = ScryModel::new();
        m.nodes.push(mk_node("n1", "Keep", None));
        m.nodes.push(mk_node("n2", "Drop", None));
        write_model_at(&r, &m).unwrap();

        ensure_planned_at(&r).unwrap();
        let mut planned = read_planned_at(&r).unwrap();
        planned.nodes.retain(|n| n.id != "n2"); // delete n2 in the draft
        write_planned_at(&r, &planned).unwrap();

        commit_element(&r, diff::ElementKind::Node, None, "n2").unwrap();

        let model = read_model_at(&r).unwrap();
        assert!(model.nodes.iter().any(|n| n.id == "n1"));
        assert!(!model.nodes.iter().any(|n| n.id == "n2"), "n2 removed from model");
        assert!(plan_diff_at(&r).unwrap().is_empty());
    }

    /// Folding a node DELETION cascades to committed the same way `delete_nodes`
    /// cascaded to the plan: the whole subtree, the links touching it, its group
    /// memberships, boundaries and anchors all go — no orphaned children left to
    /// reparent onto a dead id (promoted to phantom health roots), no dangling
    /// links. Item C.
    #[test]
    fn commit_node_delete_cascades_subtree_links_and_group_refs() {
        let (_dir, r) = temp_ref();
        let mut m = ScryModel::new();
        m.nodes.push(mk_node("p", "Parent", None));
        let mut c = mk_node("c", "Child", Some("p"));
        c.responsibilities.push(mk_resp("r-c", "does a thing"));
        m.nodes.push(c);
        m.nodes.push(mk_node("keep", "Keep", None));
        m.links.push(Link {
            id: "l1".into(),
            src: "c".into(),
            dst: "keep".into(),
            label: "calls".into(),
            method: None,
        });
        m.groups.push(Group {
            id: "grp".into(),
            name: "G".into(),
            description: None,
            member_ids: vec!["c".into(), "keep".into()],
            parent_group_id: None,
            parent_node_id: None,
            responsibilities: Vec::new(),
            icon: None,
        });
        m.source_map.insert(
            "r-c".into(),
            vec![serde_json::from_value(serde_json::json!({ "pattern": "c.rs" })).unwrap()],
        );
        m.boundaries.insert("c".into(), vec![Source { pattern: "c/**".into(), comment: None }]);
        write_model_at(&r, &m).unwrap();

        // Plan: the whole `p` subtree deleted (mirrors delete_nodes on the plan).
        ensure_planned_at(&r).unwrap();
        let mut planned = read_planned_at(&r).unwrap();
        planned.nodes.retain(|n| n.id != "p" && n.id != "c");
        planned.links.clear();
        planned.groups[0].member_ids.retain(|x| x == "keep");
        planned.source_map.remove("r-c");
        planned.boundaries.remove("c");
        write_planned_at(&r, &planned).unwrap();

        // Fold the deletion of the subtree root.
        commit_element(&r, diff::ElementKind::Node, None, "p").unwrap();

        let model = read_model_at(&r).unwrap();
        assert!(!model.nodes.iter().any(|n| n.id == "p" || n.id == "c"), "subtree removed");
        assert!(model.nodes.iter().any(|n| n.id == "keep"), "untouched sibling kept");
        assert!(model.links.is_empty(), "dangling link dropped");
        assert_eq!(model.groups[0].member_ids, vec!["keep"], "dead group ref pruned");
        assert!(!model.source_map.contains_key("r-c"), "orphaned anchor GC'd");
        assert!(!model.boundaries.contains_key("c"), "deleted node's boundary GC'd");
        assert!(plan_diff_at(&r).unwrap().is_empty(), "committed reconciled to the plan");
    }

    /// The delete cascade only removes what the plan AGREES is gone: a child kept
    /// in the plan (e.g. reparented out before the delete) survives the fold of
    /// its old parent rather than being clobbered into a phantom re-add. Item C.
    #[test]
    fn commit_node_delete_spares_a_child_still_in_the_plan() {
        let (_dir, r) = temp_ref();
        let mut m = ScryModel::new();
        m.nodes.push(mk_node("p", "Parent", None));
        m.nodes.push(mk_node("c", "Child", Some("p")));
        write_model_at(&r, &m).unwrap();

        // Plan: `p` deleted, but `c` reparented to root and kept.
        ensure_planned_at(&r).unwrap();
        let mut planned = read_planned_at(&r).unwrap();
        planned.nodes.retain(|n| n.id != "p");
        planned.nodes.iter_mut().find(|n| n.id == "c").unwrap().parent_id = None;
        write_planned_at(&r, &planned).unwrap();

        commit_element(&r, diff::ElementKind::Node, None, "p").unwrap();

        let model = read_model_at(&r).unwrap();
        assert!(!model.nodes.iter().any(|n| n.id == "p"), "parent deleted");
        assert!(model.nodes.iter().any(|n| n.id == "c"), "kept child not clobbered");
    }

    /// A node can't fold before its parent: committing a child whose parent is
    /// still plan-only would dangle the child off a non-existent committed id
    /// (invisible to outline_tree). The fold errors; folding parent-then-child
    /// succeeds. Item B.
    #[test]
    fn commit_node_requires_parent_in_committed() {
        let (_dir, r) = temp_ref();
        write_model_at(&r, &ScryModel::new()).unwrap();

        ensure_planned_at(&r).unwrap();
        let mut planned = read_planned_at(&r).unwrap();
        planned.nodes.push(mk_node("p", "Parent", None));
        planned.nodes.push(mk_node("c", "Child", Some("p")));
        write_planned_at(&r, &planned).unwrap();

        // Child first: rejected — its parent isn't committed yet.
        let err = commit_element(&r, diff::ElementKind::Node, None, "c").unwrap_err();
        assert!(err.contains("parent 'p'"), "error names the missing parent: {err}");
        assert!(
            !read_model_at(&r).unwrap().nodes.iter().any(|n| n.id == "c"),
            "child not committed while its parent is plan-only"
        );

        // Parent then child: both land, and the plan clears.
        commit_element(&r, diff::ElementKind::Node, None, "p").unwrap();
        commit_element(&r, diff::ElementKind::Node, None, "c").unwrap();
        let model = read_model_at(&r).unwrap();
        assert!(model.nodes.iter().any(|n| n.id == "p"));
        assert!(model.nodes.iter().any(|n| n.id == "c"));
        assert!(plan_diff_at(&r).unwrap().is_empty());
    }

    /// Committing a responsibility that the draft moved to another host lands it
    /// under the new host in the model and removes it from the old one.
    #[test]
    fn commit_responsibility_move() {
        let (_dir, r) = temp_ref();
        let mut m = ScryModel::new();
        let mut a = mk_node("a", "A", None);
        a.responsibilities.push(mk_resp("resp-1", "do the thing"));
        m.nodes.push(a);
        m.nodes.push(mk_node("b", "B", None));
        write_model_at(&r, &m).unwrap();

        ensure_planned_at(&r).unwrap();
        let mut planned = read_planned_at(&r).unwrap();
        // move resp-1 from a to b in the draft
        let resp = planned.nodes[0].responsibilities.remove(0);
        planned.nodes[1].responsibilities.push(resp);
        write_planned_at(&r, &planned).unwrap();

        commit_element(&r, diff::ElementKind::Responsibility, None, "resp-1").unwrap();

        let model = read_model_at(&r).unwrap();
        let a = model.nodes.iter().find(|n| n.id == "a").unwrap();
        let b = model.nodes.iter().find(|n| n.id == "b").unwrap();
        assert!(a.responsibilities.is_empty(), "resp left the old host");
        assert_eq!(b.responsibilities.len(), 1, "resp landed on the new host");
        assert!(plan_diff_at(&r).unwrap().is_empty());
    }

    /// Committing a property upserts it by `(owner, label)`.
    #[test]
    fn commit_property_update() {
        let (_dir, r) = temp_ref();
        let mut m = ScryModel::new();
        let mut a = mk_node("a", "A", None);
        a.properties.push(SchemaProperty {
            label: "email".into(),
            description: "old".into(),
            vagrant: None,
            stale: None,
            last_touched_at: None,
        });
        m.nodes.push(a);
        write_model_at(&r, &m).unwrap();

        ensure_planned_at(&r).unwrap();
        let mut planned = read_planned_at(&r).unwrap();
        planned.nodes[0].properties[0].description = "new".into();
        write_planned_at(&r, &planned).unwrap();

        commit_element(&r, diff::ElementKind::Property, Some("a"), "email").unwrap();

        let model = read_model_at(&r).unwrap();
        assert_eq!(model.nodes[0].properties[0].description, "new");
        assert!(plan_diff_at(&r).unwrap().is_empty());
    }

    /// A whole-node fold must not carry un-adjudicated review state into the
    /// source of truth (audit #5): `stale` drift flags clear on the claims that
    /// fold, and `vagrant` code-discovered claims/properties are LEFT in the plan
    /// awaiting an explicit adopt/reject verdict — not silently committed.
    #[test]
    fn commit_node_clears_stale_and_leaves_vagrant_pending() {
        let (_dir, r) = temp_ref();
        let mut m = ScryModel::new();
        let mut n = mk_node("n", "Svc", None);
        n.responsibilities.push(mk_resp("resp-1", "serves requests"));
        m.nodes.push(n);
        write_model_at(&r, &m).unwrap();

        // Plan: resp-1 went stale (code regressed, then re-implemented), a vagrant
        // claim resp-2 was drift-discovered, and a vagrant property was too.
        ensure_planned_at(&r).unwrap();
        let mut planned = read_planned_at(&r).unwrap();
        {
            let pn = &mut planned.nodes[0];
            pn.responsibilities[0].stale = Some(true);
            pn.responsibilities[0].stale_proposal = Some("serves v2 requests".into());
            let mut vagrant = mk_resp("resp-2", "also logs metrics");
            vagrant.vagrant = Some(true);
            pn.responsibilities.push(vagrant);
            pn.properties.push(SchemaProperty {
                label: "region".into(),
                description: String::new(),
                vagrant: Some(true),
                stale: None,
                last_touched_at: None,
            });
        }
        write_planned_at(&r, &planned).unwrap();

        commit_element(&r, diff::ElementKind::Node, None, "n").unwrap();

        let model = read_model_at(&r).unwrap();
        let cn = model.nodes.iter().find(|x| x.id == "n").unwrap();
        // The stale claim folded, with its drift markers cleared.
        let r1 = cn.responsibilities.iter().find(|x| x.id == "resp-1").unwrap();
        assert_eq!(r1.stale, None, "stale flag cleared on fold");
        assert_eq!(r1.stale_proposal, None, "stale proposal cleared on fold");
        // The vagrant claim and property did NOT bypass review into committed.
        assert!(
            !cn.responsibilities.iter().any(|x| x.id == "resp-2"),
            "vagrant claim not silently committed"
        );
        assert!(cn.properties.is_empty(), "vagrant property not silently committed");

        // They stay in the plan, still pending an adopt/reject verdict.
        let plan = read_planned_at(&r).unwrap();
        let pn = plan.nodes.iter().find(|x| x.id == "n").unwrap();
        assert!(
            pn.responsibilities.iter().any(|x| x.id == "resp-2" && x.vagrant == Some(true)),
            "vagrant claim still pending in the plan"
        );
        assert!(
            pn.properties.iter().any(|p| p.label == "region" && p.vagrant == Some(true)),
            "vagrant property still pending in the plan"
        );
    }

    /// A plan-added link folds only once BOTH its endpoints are committed, and it
    /// folds as a side effect of the node fold — no separate id to fold by. This
    /// is what makes the CLOSE loop terminable for a plan carrying `add_links`
    /// output: after folding both nodes, the plan diff reaches empty. Item A.
    #[test]
    fn ready_link_folds_when_its_second_endpoint_commits() {
        let (_dir, r) = temp_ref();
        write_model_at(&r, &ScryModel::new()).unwrap();

        ensure_planned_at(&r).unwrap();
        let mut planned = read_planned_at(&r).unwrap();
        planned.nodes.push(mk_node("a", "A", None));
        planned.nodes.push(mk_node("b", "B", None));
        planned.links.push(Link {
            id: "l1".into(),
            src: "a".into(),
            dst: "b".into(),
            label: "calls".into(),
            method: None,
        });
        write_planned_at(&r, &planned).unwrap();

        // Fold node `a`. The link is incident but `b` isn't committed yet, so it
        // stays pending — folding an edge whose far end has no code would be wrong.
        commit_element(&r, diff::ElementKind::Node, None, "a").unwrap();
        commit_ready_dependents(&r, "a").unwrap();
        assert!(
            !read_model_at(&r).unwrap().links.iter().any(|l| l.id == "l1"),
            "link waits until both endpoints are committed"
        );

        // Fold node `b` — now both endpoints live in committed, so the link rides
        // in on this fold and the plan diff clears.
        commit_element(&r, diff::ElementKind::Node, None, "b").unwrap();
        commit_ready_dependents(&r, "b").unwrap();
        assert!(
            read_model_at(&r).unwrap().links.iter().any(|l| l.id == "l1"),
            "link folded once its second endpoint committed"
        );
        assert!(plan_diff_at(&r).unwrap().is_empty(), "CLOSE loop terminates");
    }

    /// Folding a group (once its members are committed) carries the group's own
    /// responsibilities into committed the same way a node fold does: it drops
    /// un-adjudicated vagrant claims, clears stale markers, and moves the anchor
    /// of the folded claim across. Item A + audit #5.
    #[test]
    fn ready_group_folds_and_cleans_its_responsibilities() {
        let (_dir, r) = temp_ref();
        let mut m = ScryModel::new();
        m.nodes.push(mk_node("a", "A", None));
        write_model_at(&r, &m).unwrap();

        ensure_planned_at(&r).unwrap();
        let mut planned = read_planned_at(&r).unwrap();
        let mut claim = mk_resp("g-resp", "both surfaces deploy as one app");
        claim.stale = Some(true);
        let mut vagrant = mk_resp("g-vagrant", "drift-discovered claim");
        vagrant.vagrant = Some(true);
        planned.groups.push(Group {
            id: "grp".into(),
            name: "Payload".into(),
            description: None,
            member_ids: vec!["a".into()],
            parent_group_id: None,
            parent_node_id: None,
            responsibilities: vec![claim, vagrant],
            icon: None,
        });
        planned.source_map.insert(
            "g-resp".into(),
            vec![serde_json::from_value(serde_json::json!({
                "pattern": "app/deploy.ts", "symbol": "deploy"
            }))
            .unwrap()],
        );
        write_planned_at(&r, &planned).unwrap();

        // Fold the member node; the group rides in on that fold.
        commit_element(&r, diff::ElementKind::Node, None, "a").unwrap();
        commit_ready_dependents(&r, "a").unwrap();

        let model = read_model_at(&r).unwrap();
        let g = model.groups.iter().find(|g| g.id == "grp").expect("group folded in");
        let folded = g.responsibilities.iter().find(|x| x.id == "g-resp").unwrap();
        assert_eq!(folded.stale, None, "stale cleared on the folded claim");
        assert!(
            !g.responsibilities.iter().any(|x| x.id == "g-vagrant"),
            "vagrant claim did not bypass review into committed"
        );
        assert_eq!(
            model.source_map.get("g-resp").expect("anchor carried across")[0].pattern,
            "app/deploy.ts"
        );

        // The vagrant claim stays in the plan awaiting a verdict.
        let plan = read_planned_at(&r).unwrap();
        let pg = plan.groups.iter().find(|g| g.id == "grp").unwrap();
        assert!(
            pg.responsibilities.iter().any(|x| x.id == "g-vagrant" && x.vagrant == Some(true)),
            "vagrant group claim still pending in the plan"
        );
    }

    /// Folding a minted chain (component → symbol) then its responsibility — the
    /// adopt path — lands every rung in the committed model AND carries the code
    /// anchor across, so the adopted claim is mapped (and a later deletion work
    /// item could point at the code).
    #[test]
    fn commit_folds_chain_and_carries_source_anchor() {
        let (_dir, r) = temp_ref();
        let node = |v: serde_json::Value| serde_json::from_value::<Node>(v).unwrap();

        // Committed: just a container.
        let mut m = ScryModel::new();
        m.nodes.push(node(serde_json::json!({ "id": "c", "kind": "container", "name": "API" })));
        write_model_at(&r, &m).unwrap();

        // Plan: container + a new component + a new symbol carrying a claim,
        // anchored to code in the plan's source map.
        let mut planned = m.clone();
        planned.nodes.push(node(serde_json::json!({
            "id": "comp", "kind": "component", "name": "Admin", "parentId": "c"
        })));
        planned.nodes.push(node(serde_json::json!({
            "id": "sym", "kind": "symbol", "name": "admin_handler", "parentId": "comp",
            "responsibilities": [{ "id": "r1", "statement": "exposes admin endpoint" }],
        })));
        planned.source_map.insert(
            "r1".into(),
            vec![serde_json::from_value(serde_json::json!({
                "pattern": "api/admin.rs", "symbol": "admin_handler"
            }))
            .unwrap()],
        );
        write_planned_at(&r, &planned).unwrap();

        // Fold root→leaf, then the responsibility — the host node must exist first.
        commit_element(&r, diff::ElementKind::Node, None, "comp").unwrap();
        commit_element(&r, diff::ElementKind::Node, None, "sym").unwrap();
        commit_element(&r, diff::ElementKind::Responsibility, None, "r1").unwrap();

        let model = read_model_at(&r).unwrap();
        assert!(model.nodes.iter().any(|n| n.id == "comp"), "component folded in");
        let sym = model.nodes.iter().find(|n| n.id == "sym").expect("symbol folded in");
        assert!(sym.responsibilities.iter().any(|x| x.id == "r1"), "claim on the symbol");
        assert_eq!(
            model.source_map.get("r1").expect("anchor carried into committed")[0].pattern,
            "api/admin.rs"
        );
        assert!(plan_diff_at(&r).unwrap().is_empty(), "plan and model agree after the fold");
    }

    /// Dedup invariant: a committed claim's anchor lives only in committed, so
    /// the draft does not carry it. Folding a reworded version of that claim must
    /// KEEP the committed anchor — not drop it just because the draft has no copy
    /// (pre-dedup the draft mirrored every anchor, which masked this path).
    #[test]
    fn fold_keeps_committed_anchor_when_draft_does_not_carry_it() {
        let (_dir, r) = temp_ref();
        let node = |v: serde_json::Value| serde_json::from_value::<Node>(v).unwrap();

        // Committed: a leaf symbol with an anchored claim.
        let mut m = ScryModel::new();
        m.nodes.push(node(serde_json::json!({
            "id": "sym", "kind": "symbol", "name": "h",
            "responsibilities": [{ "id": "r1", "statement": "old wording" }],
        })));
        m.source_map.insert(
            "r1".into(),
            vec![serde_json::from_value(serde_json::json!({
                "pattern": "src/h.rs", "symbol": "h"
            }))
            .unwrap()],
        );
        write_model_at(&r, &m).unwrap();

        // Draft: the SAME claim reworded (an authored change) but with NO anchor
        // of its own — committed owns it; the draft overlays only what it adds.
        let mut planned = m.clone();
        planned.source_map.clear();
        for n in &mut planned.nodes {
            for resp in &mut n.responsibilities {
                if resp.id == "r1" {
                    resp.statement = "new wording".into();
                }
            }
        }
        write_planned_at(&r, &planned).unwrap();

        commit_element(&r, diff::ElementKind::Responsibility, None, "r1").unwrap();

        let model = read_model_at(&r).unwrap();
        let resp = model
            .nodes
            .iter()
            .flat_map(|n| &n.responsibilities)
            .find(|x| x.id == "r1")
            .expect("claim still committed");
        assert_eq!(resp.statement, "new wording", "the reword folded in");
        assert_eq!(
            model.source_map.get("r1").expect("committed anchor preserved")[0].pattern,
            "src/h.rs",
            "folding the reword must not unanchor the committed claim"
        );
    }

    /// Deleting a node folds out its own anchor AND the anchors of the
    /// responsibilities it carried — none are left orphaned in the committed
    /// source map.
    #[test]
    fn commit_node_deletion_gcs_responsibility_anchors() {
        let (_dir, r) = temp_ref();
        let node = |v: serde_json::Value| serde_json::from_value::<Node>(v).unwrap();

        // Committed: a symbol carrying a claim, both anchored to code.
        let mut m = ScryModel::new();
        m.nodes.push(node(serde_json::json!({ "id": "c", "kind": "container", "name": "API" })));
        m.nodes.push(node(serde_json::json!({
            "id": "sym", "kind": "symbol", "name": "admin_handler", "parentId": "c",
            "responsibilities": [{ "id": "r1", "statement": "exposes admin endpoint" }],
        })));
        let loc = |p: &str| vec![serde_json::from_value::<SourceLocation>(
            serde_json::json!({ "pattern": p }),
        )
        .unwrap()];
        m.source_map.insert("sym".into(), loc("api/admin.rs")); // the node's decl anchor
        m.source_map.insert("r1".into(), loc("api/admin.rs")); // the claim's anchor
        write_model_at(&r, &m).unwrap();

        // Plan drops the symbol → committing the deletion must GC both anchors.
        let mut planned = m.clone();
        planned.nodes.retain(|n| n.id != "sym");
        write_planned_at(&r, &planned).unwrap();

        commit_element(&r, diff::ElementKind::Node, None, "sym").unwrap();

        let model = read_model_at(&r).unwrap();
        assert!(!model.nodes.iter().any(|n| n.id == "sym"), "symbol deleted");
        assert!(model.source_map.get("sym").is_none(), "node anchor GC'd");
        assert!(
            model.source_map.get("r1").is_none(),
            "the deleted node's responsibility anchor must not be left orphaned"
        );
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
}
