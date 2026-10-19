//! `scryer-mcp export` as a script runs it: the real binary, one JSON
//! document on stdout in the app's shapes, exit 2 when there is no project.

use std::path::Path;
use std::process::Command;

fn write(dir: &Path, rel: &str, text: &str) {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

/// A core-shell crate with one unit test, and one session that touched it.
fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    write(d, "Cargo.toml", "[workspace]\nmembers = [\"crates/*\"]\n");
    write(d, "crates/notes/Cargo.toml", "[package]\nname = \"notes\"\nversion = \"0.1.0\"\n");
    write(
        d,
        "crates/notes/src/lib.rs",
        "pub fn save(t: &str) -> bool { !t.is_empty() }\n#[cfg(test)]\nmod tests {\n    /// If the text is empty, then refuses to save it\n    #[test]\n    fn refuses_empty() {}\n}\n",
    );
    write(d, ".scryer/structure.toml", "[containers.notes]\nstyle = \"core-shell\"\n");
    write(
        d,
        ".scryer/sessions/s1.jsonl",
        "{\"at\":1,\"event\":\"prompt\",\"text\":\"my private words\"}\n\
         {\"at\":2,\"event\":\"task\",\"text\":\"guard empty saves\"}\n\
         {\"at\":3,\"event\":\"touch\",\"file\":\"crates/notes/src/lib.rs\"}\n",
    );
    dir
}

fn export(dir: &Path) -> (i32, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_scryer-mcp"))
        .args(["export", "--project"])
        .arg(dir)
        .output()
        .unwrap();
    (out.status.code().unwrap(), String::from_utf8_lossy(&out.stdout).to_string())
}

/// When a project is exported, prints its tree, status, structure, sessions and layout as one JSON document
#[test]
fn exports_the_derived_data() {
    let dir = project();
    let (code, out) = export(dir.path());
    assert_eq!(code, 0);
    let doc: serde_json::Value = serde_json::from_str(&out).unwrap();
    let claims = doc["tree"]["claims"].as_array().unwrap();
    assert_eq!(claims.len(), 1);
    assert_eq!(claims[0]["description"], "If the text is empty, then refuses to save it");
    assert_eq!(doc["status"]["totals"]["claims"], 1);
    assert_eq!(doc["structure"]["containers"]["notes"]["style"], "core-shell");
    assert_eq!(doc["sessions"][0]["id"], "s1");
    assert!(doc["layout"].is_object());
}

/// When a session log holds the developer's prompt, the export leaves it out
#[test]
fn never_exports_a_prompt() {
    let dir = project();
    let (_, out) = export(dir.path());
    assert!(!out.contains("my private words"));
    let doc: serde_json::Value = serde_json::from_str(&out).unwrap();
    let events: Vec<_> = doc["sessions"][0]["entries"].as_array().unwrap().iter().map(|e| e["event"].clone()).collect();
    assert_eq!(events, ["task", "touch"]);
}

/// If no project is at or above the path, then exits 2 and prints nothing
#[test]
fn exits_2_without_a_project() {
    let dir = tempfile::tempdir().unwrap();
    let (code, out) = export(dir.path());
    assert_eq!(code, 2);
    assert!(out.is_empty());
}
