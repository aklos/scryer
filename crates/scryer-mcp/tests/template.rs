//! `scryer-mcp template` as a script runs it: export a subtree as a
//! template, import it into another project.

use std::path::Path;
use std::process::Command;

fn write(dir: &Path, rel: &str, text: &str) {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

/// A styled notes crate with a component, a unit and an integration claim.
fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    write(d, "Cargo.toml", "[workspace]\nmembers = [\"crates/*\"]\n");
    write(d, "crates/notes/Cargo.toml", "[package]\nname = \"notes\"\nversion = \"0.1.0\"\n");
    write(d, "crates/notes/src/lib.rs", "//! Keeps a writer's notes.\npub mod store;\n");
    write(
        d,
        "crates/notes/src/store.rs",
        "//! Persists notes. [infrastructure]\npub fn put(t: &str) -> bool { !t.is_empty() }\n\
         #[cfg(test)]\nmod tests {\n    /// If the text is empty, then refuses to save it\n    #[test]\n    fn refuses_empty() {}\n}\n",
    );
    write(d, "crates/notes/tests/claims.rs", "/// When a note is saved, keeps it across restarts [persistence]\n#[test]\nfn keeps() {}\n");
    write(d, ".scryer/structure.toml", "[containers.\"crates/notes\"]\nstyle = \"hexagonal\"\n");
    dir
}

fn run(args: &[&str], dir: &Path) -> (i32, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_scryer-mcp"))
        .arg("template")
        .args(args)
        .arg("--project")
        .arg(dir)
        .output()
        .unwrap();
    (out.status.code().unwrap(), String::from_utf8_lossy(&out.stdout).to_string())
}

/// When an exported template is imported into an empty directory, exports the same template from it
#[test]
fn imports_what_it_exports() {
    let src = project();
    let (code, template) = run(&["export", "crates/notes"], src.path());
    assert_eq!(code, 0);
    let file = src.path().join("notes.template.json");
    std::fs::write(&file, &template).unwrap();

    let dest = tempfile::tempdir().unwrap();
    let (code, wrote) = run(&["import", file.to_str().unwrap()], dest.path());
    assert_eq!(code, 0, "{wrote}");
    let wrote: serde_json::Value = serde_json::from_str(&wrote).unwrap();
    assert!(wrote["written"].as_array().unwrap().iter().any(|f| f == "crates/notes/src/store.rs"));

    let (code, again) = run(&["export", "crates/notes"], dest.path());
    assert_eq!(code, 0);
    let a: serde_json::Value = serde_json::from_str(&template).unwrap();
    let b: serde_json::Value = serde_json::from_str(&again).unwrap();
    assert_eq!(a, b);
    assert_eq!(a["containers"][0]["components"][0]["name"], "store");
}

/// If the node names nothing in the tree, then exits 2 and prints nothing
#[test]
fn exits_2_for_an_unknown_node() {
    let src = project();
    let (code, out) = run(&["export", "nowhere"], src.path());
    assert_eq!(code, 2);
    assert!(out.is_empty());
}

/// When a template is imported with a directory, scaffolds it there and tells the agent to reword it
#[test]
fn imports_into_another_directory() {
    let src = project();
    let (_, template) = run(&["export", "crates/notes"], src.path());
    let file = src.path().join("notes.template.json");
    std::fs::write(&file, &template).unwrap();

    let dest = tempfile::tempdir().unwrap();
    let (code, wrote) = run(&["import", file.to_str().unwrap(), "--as", "apps/notes"], dest.path());
    assert_eq!(code, 0, "{wrote}");
    let wrote: serde_json::Value = serde_json::from_str(&wrote).unwrap();
    assert!(wrote["notes"].as_array().unwrap().iter().any(|n| n.as_str().unwrap().contains("reword")), "{wrote}");
    let d = dest.path();
    assert!(d.join("apps/notes/src/store.rs").exists());
    assert!(std::fs::read_to_string(d.join("apps/notes/tests/claims.rs")).unwrap().contains("/// When a note is saved"));
    assert!(std::fs::read_to_string(d.join(".scryer/structure.toml")).unwrap().contains("[containers.\"apps/notes\"]"));
}
