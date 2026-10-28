//! A project from before claims lived in tests: whether it still holds an
//! authored model, moving that model into the code (`scryer-mcp migrate`),
//! how much of the review that leaves is open, and removing the files
//! nothing reads any more.

use scryer_extract::claim_files;
use std::path::Path;

/// The old model's files in `.scryer/`, read by nothing since the tree is
/// derived from the code.
const OLD_FILES: &[&str] = &[
    "model.scry",
    "planned.scry",
    "model.baseline.scry",
    "history.jsonl",
    ".fold-refusals.json",
    ".sync",
    ".anchors.json",
];

/// Where the move from an authored model stands.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AuthoredModel {
    /// `.scryer/model.scry` is still there.
    present: bool,
    /// The migration checklist's open items, when there is one.
    open_items: Option<usize>,
}

#[tauri::command]
pub(crate) fn authored_model(project_path: String) -> AuthoredModel {
    let project = Path::new(&project_path);
    AuthoredModel {
        present: project.join(".scryer").join("model.scry").is_file(),
        open_items: claim_files::migration_open_items(project),
    }
}

/// Move the authored model into the code: `scryer-mcp migrate`. It parses
/// the whole repo (a minute on a big one), so it runs off the UI thread.
#[tauri::command]
pub(crate) async fn migrate_authored_model(project_path: String) -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let report = claim_files::migrate(Path::new(&project_path))?;
        serde_json::to_value(report).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Delete the old model's files, leaving the rest of `.scryer/` alone.
#[tauri::command]
pub(crate) fn remove_authored_model(project_path: String) -> Result<(), String> {
    let dir = Path::new(&project_path).join(".scryer");
    for f in OLD_FILES {
        match std::fs::remove_file(dir.join(f)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(format!("{f}: {e}")),
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// While a migration checklist exists, counts its open items.
    #[test]
    fn counts_the_checklists_open_items() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().to_string_lossy().to_string();
        assert_eq!(authored_model(project.clone()).open_items, None);
        std::fs::create_dir_all(dir.path().join(".scryer")).unwrap();
        std::fs::write(dir.path().join(".scryer/migration.md"), "- [x] a\n- [ ] b\n  - [ ] c\n").unwrap();
        assert_eq!(authored_model(project).open_items, Some(2));
    }

    /// Removes the authored model's files and nothing else in `.scryer`.
    #[test]
    fn removes_the_old_model_files_only() {
        let dir = tempfile::tempdir().unwrap();
        let scryer = dir.path().join(".scryer");
        std::fs::create_dir_all(scryer.join("sessions")).unwrap();
        for f in ["model.scry", "planned.scry", "history.jsonl", "structure.toml", ".gitignore", "sessions/s.json", "migration.md"] {
            std::fs::write(scryer.join(f), "x").unwrap();
        }
        let project = dir.path().to_string_lossy().to_string();
        assert!(authored_model(project.clone()).present);
        remove_authored_model(project.clone()).unwrap();
        assert!(!authored_model(project).present);
        for gone in ["model.scry", "planned.scry", "history.jsonl"] {
            assert!(!scryer.join(gone).exists(), "{gone}");
        }
        for kept in ["structure.toml", ".gitignore", "sessions/s.json", "migration.md"] {
            assert!(scryer.join(kept).exists(), "{kept}");
        }
    }
}
