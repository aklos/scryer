pub mod build_edges;
pub mod drift;
pub mod health;
pub mod rules;
pub mod scan;
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

/// The PRESCRIPTIVE lifecycle — what the model says about the work. A status is
/// moved deliberately (by the user, or by an agent closing out work); it is
/// never a machine observation. Observations about the lens — vagrant
/// behaviour, stale claims, broken/missing anchors — are FLAGS (or derived
/// health data), a separate axis on top of the status.
///
/// `Changed` means exactly one thing: the spec was edited after the claim was
/// implemented, so the code must catch up. (It is NOT a drift verdict — the
/// drift check sets the `stale` flag instead.)
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    Proposed,
    Implemented,
    Verified,
    Changed,
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
    pub status: Option<Status>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vagrant: Option<bool>,
    /// Drift observation: the semantic check judged that the code no longer
    /// discharges this claim. Like `vagrant`, a flag awaiting a human/agent
    /// verdict (re-implement, reword, or drop) — the status itself is the
    /// prescription and stays untouched until that verdict. Cleared by
    /// `mark_implemented` or by editing the claim.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stale: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locked: Option<bool>,
    /// Source side: node ID the responsibility was moved to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relocated_to: Option<String>,
    /// Destination side: node ID the responsibility came from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relocated_from: Option<String>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<Status>,
    /// Unix seconds of the last truth-bearing edit (label / description /
    /// status). Drives the fossilization patina, exactly like
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

/// What a UI component is accountable for in how it LOOKS — a status-bearing
/// contract alongside `responsibilities` (behavior) and `properties` (data).
/// Same lifecycle: `implemented` when synced from code, `proposed` when
/// planned, `changed` when the code drifts from the modeled look. Carries the
/// built render artifact (`dist_path` + `source_hash`) used to detect that drift.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Appearance {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<Status>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relocated: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locked: Option<bool>,
    /// Ghost at old parent: node ID of the moved node.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relocated_to: Option<String>,
    /// Moved node: node ID of the ghost left behind.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relocated_from: Option<String>,
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

    pub fn implementing_path(&self) -> PathBuf {
        match self {
            ModelRef::ProjectLocal(path) => path.join(".scryer").join(".implementing"),
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

    /// Where the deterministic codebase dependency graph is cached for the
    /// duration of a model build, so the MCP `commit_container_model` tool (a
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
            "*.baseline.scry\n.implementing\n.sync\n.tmp.*\n.lock\n.anchors.json\n.build_edges.json\npreview/\n",
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
/// statement, status, refactoring flags, or directives. Excludes `last_touched_at`
/// itself (that's the output) so an unchanged responsibility keeps its date.
fn resp_truth_changed(a: &Responsibility, b: &Responsibility) -> bool {
    a.statement != b.statement
        || a.status != b.status
        || a.vagrant != b.vagrant
        || a.stale != b.stale
        || a.locked != b.locked
        || a.relocated_to != b.relocated_to
        || a.relocated_from != b.relocated_from
        || a.directives != b.directives
}

/// Whether two properties differ in any truth-bearing field (label / description
/// / status). Excludes `last_touched_at`.
fn prop_truth_changed(a: &SchemaProperty, b: &SchemaProperty) -> bool {
    a.label != b.label || a.description != b.description || a.status != b.status
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

// --- Implementing flag ---

pub fn is_implementing_at(r: &ModelRef) -> bool {
    r.implementing_path().exists()
}

pub fn set_implementing_at(r: &ModelRef, active: bool) -> Result<(), String> {
    let path = r.implementing_path();
    if active {
        let dir = r.dir();
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        fs::write(&path, "").map_err(|e| format!("Failed to set implementing flag: {}", e))
    } else if path.exists() {
        fs::remove_file(&path).map_err(|e| format!("Failed to clear implementing flag: {}", e))
    } else {
        Ok(())
    }
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
    let imp = r.implementing_path();
    if imp.exists() {
        let _ = fs::remove_file(&imp);
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

// --- Wikilinks ---

/// Rewrite `[[old]]` / `[[old|label]]` wikilink targets in one text. Target
/// match is trimmed, case-insensitive — the same resolution the UI renderer
/// uses. Returns the input unchanged when nothing matches.
fn rewrite_wikilink_text(text: &str, old_name: &str, new_name: &str) -> String {
    let target = old_name.trim().to_lowercase();
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("[[") {
        let Some(end_rel) = rest[start + 2..].find("]]") else { break };
        let inner = &rest[start + 2..start + 2 + end_rel];
        out.push_str(&rest[..start]);
        let (name, label) = match inner.split_once('|') {
            Some((n, l)) => (n, Some(l)),
            None => (inner, None),
        };
        if !inner.contains('[') && !inner.contains(']') && name.trim().to_lowercase() == target {
            out.push_str("[[");
            out.push_str(new_name);
            if let Some(l) = label {
                out.push('|');
                out.push_str(l);
            }
            out.push_str("]]");
        } else {
            out.push_str(&rest[start..start + 2 + end_rel + 2]);
        }
        rest = &rest[start + 2 + end_rel + 2..];
    }
    out.push_str(rest);
    out
}

/// Diff node names by id against `prior` and rewrite wikilinks for every
/// rename found — the post-write hook for any tool that can rename nodes.
pub fn rewrite_renamed_wikilinks(model: &mut ScryModel, prior: &ScryModel) {
    let renames: Vec<(String, String)> = prior
        .nodes
        .iter()
        .filter_map(|p| {
            let n = model.nodes.iter().find(|n| n.id == p.id)?;
            (n.name != p.name).then(|| (p.name.clone(), n.name.clone()))
        })
        .collect();
    for (old, new) in renames {
        rewrite_wikilinks(model, &old, &new);
    }
}

/// After a node rename, repoint every `[[Old Name]]` prose mention — node and
/// group descriptions, responsibility statements, directives — at the new
/// name so wikilinks never dangle.
pub fn rewrite_wikilinks(model: &mut ScryModel, old_name: &str, new_name: &str) {
    if old_name.trim().is_empty() || new_name.trim().is_empty() || old_name == new_name {
        return;
    }
    let fix = |t: &mut String| {
        let next = rewrite_wikilink_text(t, old_name, new_name);
        if next != *t {
            *t = next;
        }
    };
    let fix_resps = |resps: &mut Vec<Responsibility>| {
        for r in resps {
            fix(&mut r.statement);
            for d in &mut r.directives {
                fix(d);
            }
        }
    };
    for n in &mut model.nodes {
        if let Some(d) = &mut n.description {
            fix(d);
        }
        fix_resps(&mut n.responsibilities);
    }
    for g in &mut model.groups {
        if let Some(d) = &mut g.description {
            fix(d);
        }
        fix_resps(&mut g.responsibilities);
    }
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
}

impl Default for SubagentSettings {
    fn default() -> Self {
        Self {
            agent: default_agent(),
            claude: AgentSettings::default(),
            codex: AgentSettings::default(),
        }
    }
}

fn default_agent() -> String {
    "auto".to_string()
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

    #[test]
    fn wikilink_rewrite_handles_plain_label_and_case() {
        let t = "Talks to [[Auth Service]] and [[auth service|the auth layer]], not [[Billing]].";
        let out = rewrite_wikilink_text(t, "Auth Service", "Identity Service");
        assert_eq!(
            out,
            "Talks to [[Identity Service]] and [[Identity Service|the auth layer]], not [[Billing]]."
        );
        // No match → unchanged, including unclosed/malformed brackets.
        assert_eq!(rewrite_wikilink_text("see [[Other]]", "Auth", "X"), "see [[Other]]");
        assert_eq!(rewrite_wikilink_text("broken [[Auth", "Auth", "X"), "broken [[Auth");
    }

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
            parent_id: None,
            external: None,
            technology: None,
            description: None,
            responsibilities: vec![Responsibility {
                id: "r1".into(),
                statement: statement.into(),
                status: Some(Status::Implemented),
                vagrant: None,
                stale: None,
                locked: None,
                relocated_to: None,
                relocated_from: None,
                directives: Vec::new(),
                last_touched_at: None,
            }],
            properties: Vec::new(),
            icon: None,
            visual: None,
            appearance: None,
            deprecated: None,
            relocated: None,
            locked: None,
            relocated_to: None,
            relocated_from: None,
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
                        parent_id: None,
                        external: None,
                        technology: None,
                        description: None,
                        responsibilities: Vec::new(),
                        properties: Vec::new(),
                        icon: None,
                        visual: None,
                        appearance: None,
                        deprecated: None,
                        relocated: None,
                        locked: None,
                        relocated_to: None,
                        relocated_from: None,
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
}
