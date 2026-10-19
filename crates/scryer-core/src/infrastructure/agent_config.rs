//! The agent tools' config files on disk, and the tools on PATH.

use std::path::Path;

/// Read a JSON config file. Missing is empty; a file that doesn't parse is an
/// error, never a blank slate — a stray comma in the user's config must not
/// cost them everything else the file holds.
pub fn read_json(path: &Path) -> Result<Option<serde_json::Value>, String> {
    let Some(text) = read_text(path)? else { return Ok(None) };
    serde_json::from_str(&text).map(Some).map_err(|e| {
        format!("{} is not valid JSON ({e}); refusing to change it — fix the file and retry.", path.display())
    })
}

/// Read a TOML config file, with the same refusal as [`read_json`].
pub fn read_toml(path: &Path) -> Result<Option<toml_edit::DocumentMut>, String> {
    let Some(text) = read_text(path)? else { return Ok(None) };
    text.parse().map(Some).map_err(|e| {
        format!("{} is not valid TOML ({e}); refusing to change it — fix the file and retry.", path.display())
    })
}

pub fn read_text(path: &Path) -> Result<Option<String>, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

pub fn write_text(path: &Path, text: &str) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
}

/// Remove `path`, then each directory above it, up to `root`, that this
/// left empty.
pub fn remove_file(root: &Path, path: &Path) -> Result<(), String> {
    match std::fs::remove_file(path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(format!("{}: {e}", path.display())),
        _ => {}
    }
    let mut dir = path.parent();
    while let Some(d) = dir.filter(|d| d.starts_with(root) && *d != root) {
        if std::fs::remove_dir(d).is_err() {
            break;
        }
        dir = d.parent();
    }
    Ok(())
}

/// Whether `name` is an executable on PATH, the way the user's shell finds it.
pub fn on_path(name: &str) -> bool {
    let Some(paths) = std::env::var_os("PATH") else { return false };
    std::env::split_paths(&paths).any(|dir| dir.join(name).is_file() || dir.join(format!("{name}.exe")).is_file())
}

/// Whether the binary a command names still exists: a path is checked as
/// such, a bare name on PATH.
pub fn binary_exists(binary: &str) -> bool {
    if binary.contains('/') || binary.contains('\\') {
        Path::new(binary).exists()
    } else {
        on_path(binary)
    }
}
