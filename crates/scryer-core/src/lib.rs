pub mod drift;
pub mod rules;
pub mod scan;

use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Deserialize status leniently — unknown values become None instead of failing.
fn deserialize_status_lenient<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Status>, D::Error> {
    let opt: Option<String> = Option::deserialize(deserializer)?;
    Ok(opt.and_then(|s| match s.as_str() {
        "proposed" => Some(Status::Proposed),
        "implemented" => Some(Status::Implemented),
        "verified" => Some(Status::Verified),
        "vagrant" => Some(Status::Vagrant),
        _ => None,
    }))
}

/// Deserialize notes from either a single string (old format) or array of strings (new format).
fn deserialize_notes<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<String>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum NotesCompat {
        Single(String),
        List(Vec<String>),
    }
    match NotesCompat::deserialize(deserializer)? {
        NotesCompat::Single(s) => {
            if s.is_empty() { Ok(Vec::new()) }
            else { Ok(s.lines().map(|l| l.to_string()).collect()) }
        }
        NotesCompat::List(v) => Ok(v),
    }
}

// --- Types (matching src/types.ts) ---

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum C4Kind {
    Person,
    System,
    Container,
    Component,
    Operation,
    Process,
    Model,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum C4Shape {
    Rectangle,
    Person,
    Cylinder,
    Pipe,
    Trapezoid,
    Bucket,
    Hexagon,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    Proposed,
    Implemented,
    Verified,
    Vagrant,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct Position {
    pub x: f64,
    pub y: f64,
}


#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Reference {
    /// Glob pattern for matching files, e.g. "src/auth/**/*.rs"
    pub pattern: String,
    /// What these files do in the context of this node
    pub comment: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, schemars::JsonSchema)]
#[serde(untagged)]
pub enum ContractItem {
    /// New format: { text, passed?, url?, image? }
    Full {
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        passed: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        url: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        image: Option<ContractImage>,
    },
    /// Legacy format: plain string
    Plain(String),
}

impl std::fmt::Display for ContractItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let prefix = match self.passed() {
            Some(true) => "[x] ",
            Some(false) => "[ ] ",
            None => "",
        };
        write!(f, "{}{}", prefix, self.text())?;
        if let ContractItem::Full { url: Some(url), .. } = self {
            if self.text().is_empty() {
                write!(f, "{}", url)?;
            } else {
                write!(f, " ({})", url)?;
            }
        }
        if let ContractItem::Full { image: Some(img), .. } = self {
            write!(f, " [image: {}]", img.filename)?;
        }
        Ok(())
    }
}

impl ContractItem {
    pub fn text(&self) -> &str {
        match self {
            ContractItem::Full { text, .. } => text,
            ContractItem::Plain(s) => s,
        }
    }
    pub fn passed(&self) -> Option<bool> {
        match self {
            ContractItem::Full { passed, .. } => *passed,
            ContractItem::Plain(_) => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, schemars::JsonSchema)]
pub struct Contract {
    #[serde(default, skip_serializing_if = "Vec::is_empty", alias = "always")]
    pub expect: Vec<ContractItem>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ask: Vec<ContractItem>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub never: Vec<ContractItem>,
}

impl Contract {
    pub fn is_empty(&self) -> bool {
        self.expect.is_empty() && self.ask.is_empty() && self.never.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContractImage {
    pub filename: String,
    pub mime_type: String,
    pub data: String, // base64-encoded
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct C4NodeData {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_kind")]
    pub kind: C4Kind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub technology: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expanded: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shape: Option<C4Shape>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<Reference>,
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "deserialize_status_lenient")]
    pub status: Option<Status>,
    /// Reason the agent gave for the current status (e.g. "Scaffolded handler with TODO for auth")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Contract::is_empty")]
    pub contract: Contract,
    /// Freeform notes: conventions, context, rationale
    #[serde(default, skip_serializing_if = "Vec::is_empty", deserialize_with = "deserialize_notes")]
    pub notes: Vec<String>,
    /// Properties for Model-kind nodes (label/description pairs)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub properties: Vec<ModelProperty>,
}

/// A node in the model. Matches ReactFlow's Node structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct C4Node {
    pub id: String,
    #[serde(rename = "type", default = "default_node_type")]
    pub node_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<Position>,
    pub data: C4NodeData,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
}

fn default_node_type() -> String {
    "c4".to_string()
}

fn default_kind() -> C4Kind {
    C4Kind::System
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct C4EdgeData {
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
}

/// An edge in the model. Matches ReactFlow's Edge structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct C4Edge {
    pub id: String,
    pub source: String,
    pub target: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<C4EdgeData>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum StartingLevel {
    System,
    Container,
    Component,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SourceLocation {
    pub pattern: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModelProperty {
    pub label: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum GroupKind {
    Deployment,
    Package,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    pub id: String,
    #[serde(default = "default_group_kind")]
    pub kind: GroupKind,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub member_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Contract::is_empty")]
    pub contract: Contract,
}

fn default_group_kind() -> GroupKind {
    GroupKind::Deployment
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FlowBranch {
    #[serde(default)]
    pub condition: String,
    #[serde(default)]
    pub steps: Vec<FlowStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FlowStep {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Backward compat: kept for deserialization of old files, not serialized.
    #[serde(default, skip_serializing)]
    pub position: Option<Position>,
    /// If present, this step is a decision point with branching paths.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub branches: Vec<FlowBranch>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FlowTransition {
    pub source: String,
    pub target: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Flow {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub steps: Vec<FlowStep>,
    /// Backward compat: kept for deserialization of old files, not serialized.
    #[serde(default, skip_serializing)]
    pub transitions: Vec<FlowTransition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct C4ModelData {
    pub nodes: Vec<C4Node>,
    pub edges: Vec<C4Edge>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub starting_level: Option<StartingLevel>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub source_map: HashMap<String, Vec<SourceLocation>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_path: Option<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub ref_positions: HashMap<String, Position>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<Group>,
    #[serde(default, skip_serializing_if = "Vec::is_empty", alias = "scenarios")]
    pub flows: Vec<Flow>,
}

// --- Model Reference ---

/// Identifies a model's storage location: either a named global model in
/// `~/.scryer/` or a project-local model at `{project}/.scryer/model.scry`.
#[derive(Debug, Clone, PartialEq)]
pub enum ModelRef {
    /// Global model stored at `~/.scryer/{name}.scry`
    Global(String),
    /// Project-local model stored at `{path}/.scryer/model.scry`
    ProjectLocal(PathBuf),
}

impl ModelRef {
    /// Parse a ref string. Bare name → Global, `project:{path}` → ProjectLocal.
    pub fn parse(s: &str) -> Self {
        if let Some(path) = s.strip_prefix("project:") {
            ModelRef::ProjectLocal(PathBuf::from(path))
        } else {
            ModelRef::Global(s.to_string())
        }
    }

    /// Serialize to a ref string for API boundaries.
    pub fn to_ref_string(&self) -> String {
        match self {
            ModelRef::Global(name) => name.clone(),
            ModelRef::ProjectLocal(path) => format!("project:{}", path.display()),
        }
    }

    /// Path to the `.scry` model file.
    pub fn model_path(&self) -> PathBuf {
        match self {
            ModelRef::Global(name) => models_dir().join(format!("{}.scry", name)),
            ModelRef::ProjectLocal(path) => path.join(".scryer").join("model.scry"),
        }
    }

    /// Path to the baseline snapshot file.
    pub fn baseline_path(&self) -> PathBuf {
        match self {
            ModelRef::Global(name) => models_dir().join(format!("{}.baseline.scry", name)),
            ModelRef::ProjectLocal(path) => path.join(".scryer").join("model.baseline.scry"),
        }
    }

    /// Path to the implementing lock file.
    pub fn implementing_path(&self) -> PathBuf {
        match self {
            ModelRef::Global(name) => models_dir().join(format!(".implementing-{}", name)),
            ModelRef::ProjectLocal(path) => path.join(".scryer").join(".implementing"),
        }
    }

    /// The `.scryer/` directory containing this model's files.
    pub fn dir(&self) -> PathBuf {
        match self {
            ModelRef::Global(_) => models_dir(),
            ModelRef::ProjectLocal(path) => path.join(".scryer"),
        }
    }

    /// Human-readable display name.
    pub fn display_name(&self) -> String {
        match self {
            ModelRef::Global(name) => name.clone(),
            ModelRef::ProjectLocal(path) => path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| path.display().to_string()),
        }
    }

    pub fn is_project_local(&self) -> bool {
        matches!(self, ModelRef::ProjectLocal(_))
    }
}

impl std::fmt::Display for ModelRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_ref_string())
    }
}

/// Entry in the combined model list (global + project-local).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelListEntry {
    /// Ref string: bare name (global) or `project:{path}` (project-local)
    pub ref_str: String,
    /// Human-readable name for display
    pub display_name: String,
    /// Project path if known
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_path: Option<String>,
    /// Whether this is a project-local model
    pub is_local: bool,
}

// --- Storage ---

/// Resolve the global models directory (~/.scryer/).
pub fn models_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".scryer")
}

/// Path to the implementing lock file for a model.
pub fn implementing_path(model_name: &str) -> PathBuf {
    models_dir().join(format!(".implementing-{}", model_name))
}

/// Check if a model is currently being implemented by an agent.
pub fn is_implementing(model_name: &str) -> bool {
    implementing_path(model_name).exists()
}

/// Set or clear the implementing flag for a model.
pub fn set_implementing(model_name: &str, active: bool) -> Result<(), String> {
    let path = implementing_path(model_name);
    if active {
        fs::write(&path, "").map_err(|e| format!("Failed to set implementing flag: {}", e))
    } else {
        if path.exists() {
            fs::remove_file(&path).map_err(|e| format!("Failed to clear implementing flag: {}", e))
        } else {
            Ok(())
        }
    }
}

/// List all model names (without .scry extension), sorted.
pub fn list_models() -> Result<Vec<String>, String> {
    let dir = models_dir();
    if !dir.exists() {
        return Ok(vec![]);
    }
    let mut names: Vec<String> = fs::read_dir(&dir)
        .map_err(|e| e.to_string())?
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let name = entry.file_name().to_string_lossy().to_string();
            name.strip_suffix(".scry")
                .filter(|n| !n.ends_with(".baseline"))
                .map(|n| n.to_string())
        })
        .collect();
    names.sort();
    Ok(names)
}

/// Find the model linked to a given project path.
/// Scans all models and returns the name of the first one whose `project_path`
/// matches (via canonical path comparison).
pub fn resolve_model_for_project(project_path: &std::path::Path) -> Option<String> {
    let canonical = std::fs::canonicalize(project_path).ok()?;
    let names = list_models().ok()?;
    for name in names {
        if let Ok(model) = read_model(&name) {
            if let Some(ref pp) = model.project_path {
                if let Ok(model_canonical) = std::fs::canonicalize(pp) {
                    if model_canonical == canonical {
                        return Some(name);
                    }
                }
            }
        }
    }
    None
}

/// Read a model as raw JSON string (for Tauri frontend compatibility).
pub fn read_model_raw(name: &str) -> Result<String, String> {
    let path = models_dir().join(format!("{}.scry", name));
    fs::read_to_string(&path).map_err(|e| e.to_string())
}

/// Read a model as typed C4ModelData.
pub fn read_model(name: &str) -> Result<C4ModelData, String> {
    let raw = read_model_raw(name)?;
    serde_json::from_str(&raw).map_err(|e| e.to_string())
}

/// Write a model from raw JSON string (for Tauri frontend compatibility).
///
/// Uses atomic write (temp file + rename) so the file watcher sees a single
/// inotify event instead of truncate + write, which lets `SelfWrites`
/// reliably suppress UI-initiated saves without a timestamp window that
/// could accidentally suppress MCP writes.
pub fn write_model_raw(name: &str, data: &str) -> Result<(), String> {
    let dir = models_dir();
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let tmp = dir.join(format!(".{}.scry.tmp", name));
    let path = dir.join(format!("{}.scry", name));
    fs::write(&tmp, data).map_err(|e| e.to_string())?;
    fs::rename(&tmp, &path).map_err(|e| e.to_string())
}

/// Write a model from typed C4ModelData.
pub fn write_model(name: &str, model: &C4ModelData) -> Result<(), String> {
    let json = serde_json::to_string_pretty(model).map_err(|e| e.to_string())?;
    write_model_raw(name, &json)
}

// --- Baseline snapshots (for MCP diff) ---

/// Save a baseline snapshot of a model (used by MCP to track what the AI last saw).
pub fn save_baseline(name: &str, model: &C4ModelData) -> Result<(), String> {
    let dir = models_dir();
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(model).map_err(|e| e.to_string())?;
    let path = dir.join(format!("{}.baseline.scry", name));
    fs::write(&path, json).map_err(|e| e.to_string())
}

/// Read the baseline snapshot for a model. Returns None if no baseline exists.
pub fn read_baseline(name: &str) -> Option<C4ModelData> {
    let path = models_dir().join(format!("{}.baseline.scry", name));
    let raw = fs::read_to_string(&path).ok()?;
    serde_json::from_str(&raw).ok()
}

// --- AI Settings ---

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AiSettings {
    pub provider: String,
    pub api_key: String,
    pub model: String,
}

fn settings_path() -> PathBuf {
    models_dir().join("settings.json")
}

pub fn read_settings() -> AiSettings {
    let path = settings_path();
    if !path.exists() {
        return AiSettings::default();
    }
    fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn write_settings(settings: &AiSettings) -> Result<(), String> {
    let dir = models_dir();
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    fs::write(settings_path(), json).map_err(|e| e.to_string())
}

pub fn ai_configured(settings: &AiSettings) -> bool {
    !settings.provider.is_empty()
        && !settings.model.is_empty()
        && (settings.provider == "ollama" || !settings.api_key.is_empty())
}

/// Delete a model by name.
pub fn delete_model(name: &str) -> Result<(), String> {
    let dir = models_dir();
    let path = dir.join(format!("{}.scry", name));
    if path.exists() {
        fs::remove_file(&path).map_err(|e| e.to_string())?;
    }
    // Clean up baseline snapshot if present
    let baseline = dir.join(format!("{}.baseline.scry", name));
    if baseline.exists() {
        let _ = fs::remove_file(&baseline);
    }
    Ok(())
}

// --- ModelRef-based Storage ---

/// Ensure the `.scryer/.gitignore` exists for a project-local model directory.
/// Only `model.scry` should be committed; transient files are ignored.
fn ensure_project_gitignore(scryer_dir: &Path) -> Result<(), String> {
    let gitignore = scryer_dir.join(".gitignore");
    if !gitignore.exists() {
        fs::write(
            &gitignore,
            "*.baseline.scry\n.implementing\n.sync\n.tmp.*\n",
        )
        .map_err(|e| format!("Failed to create .gitignore: {}", e))?;
    }
    Ok(())
}

/// Read a model as raw JSON string from a ModelRef location.
pub fn read_model_raw_at(r: &ModelRef) -> Result<String, String> {
    fs::read_to_string(&r.model_path()).map_err(|e| e.to_string())
}

/// Read a model as typed C4ModelData from a ModelRef location.
pub fn read_model_at(r: &ModelRef) -> Result<C4ModelData, String> {
    let raw = read_model_raw_at(r)?;
    serde_json::from_str(&raw).map_err(|e| e.to_string())
}

/// Write a model from raw JSON string to a ModelRef location.
/// Uses atomic write (temp file + rename). Auto-creates `.gitignore` for project-local models.
pub fn write_model_raw_at(r: &ModelRef, data: &str) -> Result<(), String> {
    let dir = r.dir();
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    if r.is_project_local() {
        ensure_project_gitignore(&dir)?;
    }
    let model_path = r.model_path();
    let tmp_name = match r {
        ModelRef::Global(name) => format!(".{}.scry.tmp", name),
        ModelRef::ProjectLocal(_) => ".tmp.model.scry".to_string(),
    };
    let tmp = dir.join(tmp_name);
    fs::write(&tmp, data).map_err(|e| e.to_string())?;
    fs::rename(&tmp, &model_path).map_err(|e| e.to_string())
}

/// Write a model from typed C4ModelData to a ModelRef location.
pub fn write_model_at(r: &ModelRef, model: &C4ModelData) -> Result<(), String> {
    let json = serde_json::to_string_pretty(model).map_err(|e| e.to_string())?;
    write_model_raw_at(r, &json)
}

/// Save a baseline snapshot at a ModelRef location.
pub fn save_baseline_at(r: &ModelRef, model: &C4ModelData) -> Result<(), String> {
    let dir = r.dir();
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(model).map_err(|e| e.to_string())?;
    fs::write(&r.baseline_path(), json).map_err(|e| e.to_string())
}

/// Read the baseline snapshot at a ModelRef location.
pub fn read_baseline_at(r: &ModelRef) -> Option<C4ModelData> {
    let raw = fs::read_to_string(&r.baseline_path()).ok()?;
    serde_json::from_str(&raw).ok()
}

/// Check if a model is being implemented (by ModelRef).
pub fn is_implementing_at(r: &ModelRef) -> bool {
    r.implementing_path().exists()
}

/// Set or clear the implementing flag (by ModelRef).
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

/// Delete a model at a ModelRef location (model file + baseline).
pub fn delete_model_at(r: &ModelRef) -> Result<(), String> {
    let model_path = r.model_path();
    if model_path.exists() {
        fs::remove_file(&model_path).map_err(|e| e.to_string())?;
    }
    let baseline = r.baseline_path();
    if baseline.exists() {
        let _ = fs::remove_file(&baseline);
    }
    // Clean up implementing lock and sync marker
    let imp = r.implementing_path();
    if imp.exists() {
        let _ = fs::remove_file(&imp);
    }
    Ok(())
}

// --- Projects Registry ---

fn projects_registry_path() -> PathBuf {
    models_dir().join("projects.json")
}

/// Register a project path so the desktop app can discover its model.
pub fn register_project(project_path: &Path) -> Result<(), String> {
    let canonical = fs::canonicalize(project_path)
        .map_err(|e| format!("Cannot canonicalize project path: {}", e))?;
    let mut projects = registered_projects();
    if !projects.iter().any(|p| p == &canonical) {
        projects.push(canonical);
        let json = serde_json::to_string_pretty(&projects).map_err(|e| e.to_string())?;
        let dir = models_dir();
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        fs::write(projects_registry_path(), json).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Read the list of registered project paths, pruning any whose `.scryer/model.scry` no longer exists.
pub fn registered_projects() -> Vec<PathBuf> {
    let path = projects_registry_path();
    let raw = match fs::read_to_string(&path) {
        Ok(s) => s,
        Err(_) => return vec![],
    };
    let all: Vec<PathBuf> = serde_json::from_str(&raw).unwrap_or_default();
    let valid: Vec<PathBuf> = all
        .into_iter()
        .filter(|p| p.join(".scryer").join("model.scry").exists())
        .collect();
    // Lazily prune invalid entries
    if let Ok(json) = serde_json::to_string_pretty(&valid) {
        let _ = fs::write(&path, json);
    }
    valid
}

/// List all models: global models from `~/.scryer/` + project-local models from registry.
pub fn list_all_models() -> Result<Vec<ModelListEntry>, String> {
    let mut entries = Vec::new();

    // Global models — those with a project_path are project models (not yet migrated),
    // those without are templates.
    for name in list_models()? {
        let project_path = read_model(&name)
            .ok()
            .and_then(|m| m.project_path);
        let has_project = project_path.is_some();
        entries.push(ModelListEntry {
            ref_str: name.clone(),
            display_name: name,
            project_path,
            is_local: has_project,
        });
    }

    // Project-local models from registry
    for project_path in registered_projects() {
        let display = project_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| project_path.display().to_string());
        let ref_str = format!("project:{}", project_path.display());
        let pp_str = project_path.to_string_lossy().to_string();

        // Skip if there's already a global model pointing to this project
        // (avoids duplicates during migration period)
        if entries.iter().any(|e| e.project_path.as_deref() == Some(&pp_str)) {
            continue;
        }

        entries.push(ModelListEntry {
            ref_str,
            display_name: display,
            project_path: Some(pp_str),
            is_local: true,
        });
    }

    Ok(entries)
}

// --- Model Resolution ---

/// Find the model for a project path. Checks project-local first, then global.
/// Returns a ModelRef if found.
pub fn resolve_model_for_project_ref(project_path: &Path) -> Option<ModelRef> {
    // Check project-local first
    let local_model = project_path.join(".scryer").join("model.scry");
    if local_model.exists() {
        return Some(ModelRef::ProjectLocal(project_path.to_path_buf()));
    }

    // Fall back to scanning global models for project_path match
    let canonical = fs::canonicalize(project_path).ok()?;
    let names = list_models().ok()?;
    for name in names {
        if let Ok(model) = read_model(&name) {
            if let Some(ref pp) = model.project_path {
                if let Ok(model_canonical) = fs::canonicalize(pp) {
                    if model_canonical == canonical {
                        return Some(ModelRef::Global(name));
                    }
                }
            }
        }
    }
    None
}

/// Migrate a global model to its project directory.
/// Reads the model's `project_path`, copies to `{project}/.scryer/model.scry`,
/// registers the project, and removes the global copy.
pub fn migrate_to_local(name: &str) -> Result<ModelRef, String> {
    let model = read_model(name)?;
    let project_path = model.project_path.as_deref()
        .ok_or_else(|| format!("Model '{}' has no project_path set", name))?;
    let project = PathBuf::from(project_path);
    if !project.exists() {
        return Err(format!("Project path '{}' does not exist", project_path));
    }
    let model_ref = ModelRef::ProjectLocal(project.clone());
    write_model_at(&model_ref, &model)?;
    register_project(&project)?;
    // Copy baseline if it exists
    if let Some(baseline) = read_baseline(name) {
        let _ = save_baseline_at(&model_ref, &baseline);
    }
    delete_model(name)?;
    Ok(model_ref)
}

/// Generate the next node ID by scanning existing nodes.
/// Follows the frontend pattern: "node-{N}" with N incrementing.
pub fn next_node_id(model: &C4ModelData) -> String {
    let max = model
        .nodes
        .iter()
        .filter_map(|n| n.id.strip_prefix("node-").and_then(|s| s.parse::<u64>().ok()))
        .max()
        .unwrap_or(0);
    format!("node-{}", max + 1)
}

/// Generate an edge ID from source and target node IDs.
pub fn make_edge_id(source: &str, target: &str) -> String {
    format!("edge-{}-{}", source, target)
}

/// Generate the next flow ID by scanning existing flows.
/// Preserves "scenario-N" prefix for backward compatibility with existing .scry files.
pub fn next_flow_id(model: &C4ModelData) -> String {
    let max = model
        .flows
        .iter()
        .filter_map(|s| s.id.strip_prefix("scenario-").and_then(|n| n.parse::<u64>().ok()))
        .max()
        .unwrap_or(0);
    format!("scenario-{}", max + 1)
}

/// Collect all step IDs recursively (including branch sub-steps).
pub fn collect_step_ids(steps: &[FlowStep]) -> Vec<&str> {
    let mut ids = Vec::new();
    for step in steps {
        ids.push(step.id.as_str());
        for branch in &step.branches {
            ids.extend(collect_step_ids(&branch.steps));
        }
    }
    ids
}

/// Generate the next step ID by scanning all steps across all flows.
pub fn next_step_id(model: &C4ModelData) -> String {
    let max = model
        .flows
        .iter()
        .flat_map(|f| collect_step_ids(&f.steps))
        .filter_map(|id| id.strip_prefix("step-").and_then(|n| n.parse::<u64>().ok()))
        .max()
        .unwrap_or(0);
    format!("step-{}", max + 1)
}

