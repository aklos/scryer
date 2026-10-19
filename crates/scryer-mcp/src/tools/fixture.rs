//! A small project whose tree is derived from its code and tests, for the
//! tool tests: a Cargo crate with a unit-tested component (one test written,
//! one todo), an integration test and an e2e test.

use rmcp::model::CallToolResult;
use std::path::Path;

pub(crate) fn write(dir: &Path, rel: &str, text: &str) {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

pub(crate) fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    write(d, "Cargo.toml", "[workspace]\nmembers = [\"crates/*\"]\n");
    write(d, "crates/notes/Cargo.toml", "[package]\nname = \"notes\"\nversion = \"0.1.0\"\n");
    write(d, "crates/notes/src/lib.rs", "//! Keeps a writer's notes.\npub mod draft;\npub mod store;\n");
    write(
        d,
        "crates/notes/src/draft.rs",
        r#"//! Holds the user's draft.
use crate::store::Store;
pub struct Draft { pub text: String }
pub fn save(s: &Store, d: &Draft) -> bool { s.put(&d.text) }

#[cfg(test)]
mod tests {
    /// When a save fails, keeps the draft [persistence]
    #[test]
    fn keeps_draft() {}

    /// Merges two drafts
    #[test]
    fn merges() { todo!() }
}
"#,
    );
    write(
        d,
        "crates/notes/src/store/mod.rs",
        "//! Persists text.\npub struct Store;\nimpl Store { pub fn put(&self, _t: &str) -> bool { true } }\n",
    );
    write(d, "crates/notes/src/sync.rs", "pub fn sync() {}\n");
    write(
        d,
        "crates/notes/tests/roundtrip.rs",
        "use notes::store::Store;\n/// Saves and reloads a draft unchanged\n#[test]\nfn roundtrip() { Store.put(\"x\"); }\n",
    );
    write(d, ".scryer/structure.toml", "[system]\nname = \"notes\"\n\n[levels]\ne2e = [\"e2e/**\"]\n");
    write(
        d,
        "e2e/tests/flow.rs",
        "use notes::store::Store;\n/// Writes and publishes a post\n#[test]\nfn flow() { Store.put(\"y\"); }\n",
    );
    dir
}

/// Make the fixture a git repository with everything committed — probes need
/// one for their worktree.
pub(crate) fn git_init(dir: &Path) {
    for args in [
        &["init", "-q"][..],
        &["config", "user.email", "t@t.t"],
        &["config", "user.name", "t"],
        &["add", "-A"],
        &["commit", "-qm", "init"],
    ] {
        let out = std::process::Command::new("git").args(args).current_dir(dir).output().unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    }
}

/// Keep probe worktrees out of the home directory of whoever runs the suite.
/// Set once, before any test reads it — `set_var` is process-global.
pub(crate) fn isolate_probes_root() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        std::env::set_var("SCRYER_PROBES_DIR", std::env::temp_dir().join("scryer-mcp-probe-tests"));
    });
}

pub(crate) fn project_arg(dir: &tempfile::TempDir) -> Option<String> {
    Some(dir.path().to_string_lossy().to_string())
}

pub(crate) fn text_of(result: &CallToolResult) -> String {
    result.content.iter().find_map(|c| c.as_text().map(|t| t.text.clone())).unwrap()
}

pub(crate) fn json_of(result: &CallToolResult) -> serde_json::Value {
    serde_json::from_str(&text_of(result)).unwrap()
}
