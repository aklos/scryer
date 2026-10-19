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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::style::core_shell;

    /// Loads the project's own style files beside the built-ins; if a file
    /// cannot be read, then skips it and reports why.
    #[test]
    fn project_styles_load_beside_the_builtins() {
        let dir = tempfile::tempdir().unwrap();
        let styles_dir = dir.path().join(".scryer").join("styles");
        std::fs::create_dir_all(&styles_dir).unwrap();
        let mut custom = core_shell();
        custom.name = "two-tier".into();
        std::fs::write(styles_dir.join("two-tier.json"), serde_json::to_string(&custom).unwrap()).unwrap();
        std::fs::write(styles_dir.join("broken.json"), "{ not json").unwrap();
        let styles = load_styles(dir.path());
        assert!(styles.get("two-tier").is_some());
        assert!(styles.get("hexagonal").is_some());
        assert_eq!(styles.names().len(), 8);
        assert_eq!(styles.errors().len(), 1);
        assert!(styles.errors()[0].contains("broken.json"), "{:?}", styles.errors());
    }
}
