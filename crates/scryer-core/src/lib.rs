pub mod rules;
pub mod scan;

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
    Operation,
    Model,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    Proposed,
    Implemented,
    Verified,
    Changed,
    Relocated,
}

// --- Layout ---

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq, schemars::JsonSchema)]
pub struct Cell {
    pub row: i32,
    pub col: i32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq, schemars::JsonSchema)]
pub struct GroupSize {
    pub cols: u32,
    pub rows: u32,
}

// --- Responsibility ---

/// A pure business-responsibility statement. The `statement` field is the spec;
/// `implementationRules` is optional informational "how" metadata and has no
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locked: Option<bool>,
    /// Source side: node ID the responsibility was moved to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relocated_to: Option<String>,
    /// Destination side: node ID the responsibility came from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relocated_from: Option<String>,
    /// Optional informational "how" — implementation-detail notes that sit
    /// beside the responsibility. Not part of conformance.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub implementation_rules: Vec<String>,
}

// --- Code-level data ---

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, schemars::JsonSchema)]
pub struct ModelProperty {
    pub label: String,
    #[serde(default)]
    pub description: String,
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
    pub pattern: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_line: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
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
    /// Properties for `Model`-kind nodes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub properties: Vec<ModelProperty>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<Source>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cell: Option<Cell>,
    /// Optional lucide-react icon name override. Falls back to a deterministic
    /// icon picked from `id` when unset. Frontend-only meaning; backend just
    /// passes the string through.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cell: Option<Cell>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<GroupSize>,
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
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub source_map: HashMap<String, Vec<SourceLocation>>,
}

impl ScryModel {
    pub fn new() -> Self {
        Self {
            version: SCRY_VERSION.to_string(),
            nodes: Vec::new(),
            links: Vec::new(),
            groups: Vec::new(),
            source_map: HashMap::new(),
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
            "*.baseline.scry\n.implementing\n.sync\n.tmp.*\n",
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
    let json = serde_json::to_string_pretty(model).map_err(|e| e.to_string())?;
    write_model_raw_at(r, &json)
}

// --- Baseline snapshots (for MCP diff) ---

pub fn save_baseline_at(r: &ModelRef, model: &ScryModel) -> Result<(), String> {
    let dir = r.dir();
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(model).map_err(|e| e.to_string())?;
    fs::write(&r.baseline_path(), json).map_err(|e| e.to_string())
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
