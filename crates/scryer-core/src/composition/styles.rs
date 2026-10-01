//! The style table a project actually uses.

use crate::domain::style::{StyleDef, Styles};
use std::path::Path;

/// The built-ins plus the project's own `.scryer/styles/<name>.json` files.
/// An unreadable file is skipped, never fatal — a bad custom style must not
/// take the model down with it — and recorded, so health can say why the
/// style it meant to declare is unknown.
pub fn load_styles(project: &Path) -> Styles {
    let mut styles = Styles::builtin();
    let dir = project.join(".scryer").join("styles");
    let Ok(entries) = std::fs::read_dir(&dir) else { return styles };
    let mut paths: Vec<_> = entries.flatten().map(|e| e.path()).collect();
    paths.sort();
    for path in paths {
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let shown = path.strip_prefix(project).unwrap_or(&path).display().to_string();
        let parsed = std::fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|text| serde_json::from_str::<StyleDef>(&text).map_err(|e| e.to_string()));
        match parsed {
            Ok(def) => styles.insert(def),
            Err(e) => styles.push_error(format!("style file {shown} was not loaded: {e}")),
        }
    }
    styles
}
