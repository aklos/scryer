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
    let model: scryer_core::ScryModel = serde_json::from_value(serde_json::json!({
        "version": "0.3", "links": [],
        "nodes": [{ "id": "api", "kind": "container", "name": "API",
                    "responsibilities": [{ "id": "r-1", "statement": "serves requests" }] }],
        "sourceMap": { "r-1": [{ "pattern": "src/api.rs", "symbol": "serve" }] }
    }))
    .unwrap();
    scryer_core::write_model_at(&r, &model).unwrap();
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
