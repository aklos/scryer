//! Where a project's scryer files live: `.scryer/` beside the code, holding
//! `structure.toml`, the session logs and the regenerable caches.

use std::path::{Path, PathBuf};


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

    /// Where the deterministic codebase dependency graph is cached for the
    /// duration of a model build, so the MCP `fill_container` tool (a
    /// separate process from the build orchestrator) can wire code-level links
    /// from the same edges the agent saw — without re-parsing the project.
    pub fn build_edges_path(&self) -> PathBuf {
        match self {
            ModelRef::ProjectLocal(path) => path.join(".scryer").join(".build_edges.json"),
        }
    }

    /// The derived C4 tree (see `scryer_extract::derived_tree`), keyed on the
    /// mtimes it was derived from. Regenerable, never hand-authored.
    pub fn derived_tree_path(&self) -> PathBuf {
        match self {
            ModelRef::ProjectLocal(path) => path.join(".scryer").join(".derived_tree.json"),
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

    /// The last ingested mutation report per file and per test (see
    /// `scryer_extract::test_status::ingest_mutation_report`). Regenerable.
    pub fn mutations_path(&self) -> PathBuf {
        match self {
            ModelRef::ProjectLocal(path) => path.join(".scryer").join(".mutations.json"),
        }
    }

    /// Per-session logs, one append-only `<session_id>.jsonl` each (see
    /// `crate::session_log`). Local to this checkout, never committed.
    pub fn sessions_dir(&self) -> PathBuf {
        match self {
            ModelRef::ProjectLocal(path) => path.join(".scryer").join("sessions"),
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

/// The project at `project_path`, when it has a `.scryer/` directory (its
/// `structure.toml`, sessions and caches live there).
pub fn resolve_project(project_path: &Path) -> Option<ModelRef> {
    project_path.join(".scryer").is_dir().then(|| ModelRef::ProjectLocal(project_path.to_path_buf()))
}
