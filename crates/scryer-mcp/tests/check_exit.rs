//! `scryer-mcp check` and `lint` as CI runs them: the real binary, its exit
//! code. 0 clean, 1 findings, 2 unusable.

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Stdio};

fn write(dir: &Path, rel: &str, text: &str) {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

const LIB: &str = "crates/notes/src/lib.rs";

/// A core-shell crate with one well described unit test in its root.
fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    write(d, "Cargo.toml", "[workspace]\nmembers = [\"crates/*\"]\n");
    write(d, "crates/notes/Cargo.toml", "[package]\nname = \"notes\"\nversion = \"0.1.0\"\n");
    write(
        d,
        LIB,
        "pub fn save(t: &str) -> bool { !t.is_empty() }\n#[cfg(test)]\nmod tests {\n    /// If the text is empty, then refuses to save it\n    #[test]\n    fn refuses_empty() {}\n}\n",
    );
    write(d, ".scryer/structure.toml", "[containers.notes]\nstyle = \"core-shell\"\n");
    dir
}

fn run(dir: &Path, args: &[&str]) -> (i32, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_scryer-mcp"))
        .args(args)
        .arg(dir)
        .output()
        .unwrap();
    (out.status.code().unwrap(), String::from_utf8_lossy(&out.stdout).to_string())
}

/// Ingest a report the way an agent does: through the server, which keys
/// verdicts on the derived tree.
fn ingest(dir: &Path, tool: &str, report: &str, text: &str) {
    write(dir, report, text);
    let mut child = Command::new(env!("CARGO_BIN_EXE_scryer-mcp"))
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let mut send = |msg: serde_json::Value| -> Option<serde_json::Value> {
        writeln!(stdin, "{msg}").unwrap();
        let id = msg.get("id")?.clone();
        loop {
            let mut line = String::new();
            assert!(stdout.read_line(&mut line).unwrap() > 0, "server closed");
            let v: serde_json::Value = serde_json::from_str(&line).unwrap();
            if v["id"] == id {
                return Some(v);
            }
        }
    };
    send(serde_json::json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": { "protocolVersion": "2025-03-26", "capabilities": {}, "clientInfo": { "name": "t", "version": "0" } } }));
    send(serde_json::json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }));
    let v = send(serde_json::json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/call",
        "params": { "name": tool, "arguments": { "path": report } } }))
    .unwrap();
    assert_ne!(v["result"]["isError"], true, "{v}");
    let _ = child.kill();
}

/// A styled crate with a well-worded, unverified claim passes, its stale
/// claim a note; --fail-on-stale turns it into a failure.
#[test]
fn check_passes_a_clean_project_and_gates_stale_on_request() {
    let dir = project();
    let (code, out) = run(dir.path(), &["check"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("1 stale claim(s) (not gating"), "{out}");
    let (code, out) = run(dir.path(), &["check", "--fail-on-stale"]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("stale: crates/notes/src/lib.rs"), "{out}");
}

/// A red test fails the gate.
#[test]
fn check_fails_on_a_failing_claim() {
    let dir = project();
    ingest(
        dir.path(),
        "ingest_test_report",
        "report.xml",
        r#"<testsuite name="notes"><testcase classname="notes" name="tests::refuses_empty"><failure message="no"/></testcase></testsuite>"#,
    );
    let (code, out) = run(dir.path(), &["check"]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("failing: crates/notes/src/lib.rs"), "{out}");
}

/// A surviving mutant is a note, and a failure under --fail-on-mutants.
#[test]
fn check_gates_surviving_mutants_on_request() {
    let dir = project();
    ingest(
        dir.path(),
        "ingest_mutation_report",
        "mutants.out/outcomes.json",
        r#"{"outcomes":[{"scenario":{"Mutant":{"package":"notes","file":"src/lib.rs","function":{"function_name":"save"},"span":{"start":{"line":1,"column":1}},"replacement":"true"}},"summary":"MissedMutant"}]}"#,
    );
    let (code, out) = run(dir.path(), &["check"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("surviving mutant or break (not gating"), "{out}");
    let (code, out) = run(dir.path(), &["check", "--fail-on-mutants"]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("surviving mutant: "), "{out}");
}

/// A description that breaks the claim grammar fails lint and check; an
/// unknown flag is a usage error.
#[test]
fn lint_and_check_fail_on_a_malformed_claim() {
    let dir = project();
    assert_eq!(run(dir.path(), &["lint"]).0, 0);
    write(
        dir.path(),
        LIB,
        "pub fn save(t: &str) -> bool { !t.is_empty() }\n#[cfg(test)]\nmod tests {\n    /// Refuses to save when the text is empty\n    #[test]\n    fn refuses_empty() {}\n}\n",
    );
    let (code, out) = run(dir.path(), &["lint"]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("crates/notes/src/lib.rs:6: trailing-condition"), "{out}");
    let (code, out) = run(dir.path(), &["check"]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("lint: "), "{out}");
    assert_eq!(run(dir.path(), &["check", "--fail-on-drift"]).0, 2);
}

/// If the tree cannot be derived, then check exits 2 and says why.
#[test]
#[ignore]
fn check_exits_2_when_the_tree_cannot_be_derived() {
    todo!()
}
