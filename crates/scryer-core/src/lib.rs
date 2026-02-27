pub mod rules;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

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
    Implemented,
    Proposed,
    Changed,
    Deprecated,
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

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, schemars::JsonSchema)]
pub struct Contract {
    #[serde(default, skip_serializing_if = "Vec::is_empty", alias = "always")]
    pub expect: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ask: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub never: Vec<String>,
}

impl Contract {
    pub fn is_empty(&self) -> bool {
        self.expect.is_empty() && self.ask.is_empty() && self.never.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Attachment {
    pub id: String,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<Status>,
    #[serde(default, skip_serializing_if = "Contract::is_empty")]
    pub contract: Contract,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub accepts: Vec<String>,
    /// Short rationale for why this node exists or is structured this way
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decisions: Option<String>,
    /// Properties for Model-kind nodes (label/description pairs)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub properties: Vec<ModelProperty>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<Attachment>,
}

/// A node in the model. Matches ReactFlow's Node structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct C4Node {
    pub id: String,
    #[serde(rename = "type", default = "default_node_type")]
    pub node_type: String,
    #[serde(default)]
    pub position: Position,
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
    pub file: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_line: Option<u32>,
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
}

fn default_group_kind() -> GroupKind {
    GroupKind::Deployment
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FlowStep {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<Position>,
    /// IDs of processes this step exercises. Set by the AI agent to link flow steps to architecture.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub process_ids: Vec<String>,
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
    #[serde(default)]
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
    #[serde(default, skip_serializing_if = "Contract::is_empty")]
    pub contract: Contract,
    #[serde(default, skip_serializing_if = "Vec::is_empty", alias = "scenarios")]
    pub flows: Vec<Flow>,
}

// --- Storage ---

/// Resolve the global models directory (~/.scryer/).
pub fn models_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".scryer")
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
    let path = models_dir().join(format!("{}.scry", name));
    if path.exists() {
        fs::remove_file(&path).map_err(|e| e.to_string())
    } else {
        Ok(())
    }
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

/// Generate the next step ID by scanning all steps across all flows.
pub fn next_step_id(model: &C4ModelData) -> String {
    let max = model
        .flows
        .iter()
        .flat_map(|s| &s.steps)
        .filter_map(|st| st.id.strip_prefix("step-").and_then(|n| n.parse::<u64>().ok()))
        .max()
        .unwrap_or(0);
    format!("step-{}", max + 1)
}

