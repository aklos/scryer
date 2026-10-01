//! The style table a project actually uses.

use crate::domain::style::{StyleDef, Styles};
use std::path::Path;

/// The built-ins plus the project's own `.scryer/styles/<name>.json` files.
/// Unreadable files are skipped, never fatal — a bad custom style must not
/// take the model down with it.
pub fn load_styles(project: &Path) -> Styles {
    let mut styles = Styles::builtin();
    let dir = project.join(".scryer").join("styles");
    let Ok(entries) = std::fs::read_dir(&dir) else { return styles };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        let Ok(def) = serde_json::from_str::<StyleDef>(&text) else { continue };
        styles.insert(def);
    }
    styles
}
