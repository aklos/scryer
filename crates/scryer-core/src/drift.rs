use std::path::Path;
use std::time::SystemTime;

use crate::C4ModelData;

/// A node whose source files have been modified since the model was last saved.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriftedNode {
    pub node_id: String,
    pub node_name: String,
    /// The source patterns that matched newer files.
    pub patterns: Vec<String>,
}

/// Overall drift report for a model.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriftReport {
    /// Nodes whose source-mapped files changed.
    pub nodes: Vec<DriftedNode>,
    /// Project structure changed (new/deleted files or directories).
    pub structure_changed: bool,
}

/// Check drift: source-mapped nodes + project structure changes.
///
/// `model` — the parsed model data
/// `model_mtime` — baseline timestamp (max of model mtime and last sync time)
/// `project_path` — the root directory to resolve source patterns against
pub fn check_drift(
    model: &C4ModelData,
    model_mtime: SystemTime,
    project_path: &Path,
) -> DriftReport {
    let nodes = check_source_drift(model, model_mtime, project_path);
    let structure_changed = check_structure_drift(model_mtime, project_path);
    DriftReport { nodes, structure_changed }
}

/// Check which source-mapped nodes have drifted (source files newer than the model).
fn check_source_drift(
    model: &C4ModelData,
    model_mtime: SystemTime,
    project_path: &Path,
) -> Vec<DriftedNode> {
    let mut drifted = Vec::new();

    for (node_id, locations) in &model.source_map {
        let mut hit_patterns = Vec::new();

        for loc in locations {
            let pat = &loc.pattern;
            let full_pattern = project_path.join(pat).to_string_lossy().to_string();

            let paths = match glob::glob(&full_pattern) {
                Ok(paths) => paths,
                Err(_) => continue,
            };

            for entry in paths.flatten() {
                if let Ok(meta) = entry.metadata() {
                    if let Ok(mtime) = meta.modified() {
                        if mtime > model_mtime {
                            hit_patterns.push(pat.clone());
                            break; // one newer file per pattern is enough
                        }
                    }
                }
            }
        }

        if !hit_patterns.is_empty() {
            let node_name = model
                .nodes
                .iter()
                .find(|n| n.id == *node_id)
                .map(|n| n.data.name.clone())
                .unwrap_or_default();

            drifted.push(DriftedNode {
                node_id: node_id.clone(),
                node_name,
                patterns: hit_patterns,
            });
        }
    }

    drifted
}

/// Check if the project structure changed — any file created or deleted since baseline.
/// Uses the same directory skipping as `get_structure` (SKIP_DIRS, SKIP_BUILD_DIRS, .gitignore).
fn check_structure_drift(baseline: SystemTime, project_path: &Path) -> bool {
    use crate::scan::{SKIP_DIRS, SKIP_BUILD_DIRS};

    let walker = ignore::WalkBuilder::new(project_path)
        .hidden(false)
        .filter_entry(|entry| {
            if entry.file_type().is_some_and(|ft| ft.is_dir()) {
                let name = entry.file_name().to_string_lossy();
                if SKIP_DIRS.iter().any(|&s| name == s) {
                    return false;
                }
                if SKIP_BUILD_DIRS.iter().any(|&s| name == s) {
                    return false;
                }
            }
            true
        })
        .build();

    for entry in walker.flatten() {
        if entry.file_type().is_some_and(|ft| ft.is_file()) {
            if let Ok(meta) = entry.metadata() {
                if let Ok(created) = meta.created() {
                    if created > baseline {
                        return true;
                    }
                }
            }
        }
    }

    false
}
