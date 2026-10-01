use std::fs;
use std::path::{Path, PathBuf};

use crate::SCRY_VERSION;

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

    /// The claim test-status cache — each claim's last reported test outcome
    /// beside the anchor fingerprints it was computed against (see
    /// `scryer_extract::test_status`). Regenerable, git-free, never
    /// hand-authored.
    pub fn test_results_path(&self) -> PathBuf {
        match self {
            ModelRef::ProjectLocal(path) => path.join(".scryer").join(".test-results.json"),
        }
    }

    /// The fold-refusal ledger — claims `mark_implemented` last declined to
    /// fold and why (see `crate::refusals`). Regenerable, git-free.
    pub fn fold_refusals_path(&self) -> PathBuf {
        match self {
            ModelRef::ProjectLocal(path) => path.join(".scryer").join(".fold-refusals.json"),
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
