//! The Stop hook as the harness runs it: the real binary, an event on stdin.

use std::io::Write;
use std::process::{Command, Stdio};

fn hook(project: &std::path::Path, event: serde_json::Value, args: &[&str]) -> String {
    let mut child = Command::new(env!("CARGO_BIN_EXE_scryer-mcp"))
        .arg("hook")
        .args(args)
        .env_remove("CLAUDE_PROJECT_DIR")
        .current_dir(project)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(event.to_string().as_bytes()).unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    String::from_utf8(out.stdout).unwrap()
}

/// When the agent stops, the hook closes the session's record and prints
/// nothing — no block, no summary — on every harness, however much the
/// session left behind.
#[test]
fn stop_emits_nothing_and_closes_the_record() {
    let dir = tempfile::tempdir().unwrap();
    let r = scryer_core::ModelRef::ProjectLocal(dir.path().to_path_buf());
    std::fs::create_dir_all(dir.path().join("src")).unwrap();
    std::fs::write(dir.path().join("Cargo.toml"), "[package]\nname = \"api\"\nversion = \"0.1.0\"\n").unwrap();
    std::fs::write(dir.path().join("src/api.rs"), "pub fn serve() {}\n").unwrap();
    std::fs::create_dir_all(dir.path().join(".scryer")).unwrap();
    scryer_core::session::record_touch(&r, "s", "src/api.rs").unwrap();
    scryer_core::session::record_touch(&r, "s", "src/api_test.rs").unwrap();

    let stop = serde_json::json!({ "hook_event_name": "Stop", "session_id": "s", "cwd": dir.path() });
    for args in [&[][..], &["--copilot"][..]] {
        assert_eq!(hook(dir.path(), stop.clone(), args), "", "{args:?}");
    }
    let stops = scryer_core::session::read_session(&r, "s")
        .into_iter()
        .filter(|e| e.event == scryer_core::session::SessionEvent::Stop)
        .count();
    assert_eq!(stops, 2);
}

/// If the event is not JSON or no project is at or above its directory, then the hook exits 0 and prints nothing.
#[test]
#[ignore]
fn bad_json_or_no_project_exits_silently() {
    todo!()
}

/// When a hook fires below the project root, finds the project by walking up from `$CLAUDE_PROJECT_DIR`, else from the event's cwd.
#[test]
#[ignore]
fn finds_the_project_from_below_its_root() {
    todo!()
}

/// When the agent reads a file the tree speaks for, injects the file's overlay in its harness's envelope, `hookSpecificOutput` for Claude Code and Codex and a top-level `additionalContext` for Copilot.
#[test]
#[ignore]
fn read_injects_the_overlay_in_each_harness_envelope() {
    todo!()
}
