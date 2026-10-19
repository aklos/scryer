//! The test loop as an agent drives it: the real server over stdio, on a
//! project whose tree is derived from its code. The server keys verdicts on
//! the derived tree by itself, so a report it ingests makes the test proven,
//! a probe opens on it by name, and closing resets the worktree.

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

fn write(dir: &Path, rel: &str, text: &str) {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git").args(args).current_dir(dir).output().unwrap();
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
}

struct Server {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next: u64,
}

impl Server {
    fn start(project: &Path, probes: &Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_scryer-mcp"))
            .current_dir(project)
                .env("SCRYER_PROBES_DIR", probes)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        let mut s = Server { child, stdin, stdout, next: 1 };
        s.request(
            "initialize",
            serde_json::json!({ "protocolVersion": "2025-03-26", "capabilities": {}, "clientInfo": { "name": "t", "version": "0" } }),
        );
        s.send(serde_json::json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }));
        s
    }

    fn send(&mut self, msg: serde_json::Value) {
        writeln!(self.stdin, "{msg}").unwrap();
        self.stdin.flush().unwrap();
    }

    fn request(&mut self, method: &str, params: serde_json::Value) -> serde_json::Value {
        let id = self.next;
        self.next += 1;
        self.send(serde_json::json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
        loop {
            let mut line = String::new();
            assert!(self.stdout.read_line(&mut line).unwrap() > 0, "server closed");
            let v: serde_json::Value = serde_json::from_str(&line).unwrap();
            if v["id"] == id {
                return v;
            }
        }
    }

    /// Call a tool; the text it answers with, and whether it was an error.
    fn call(&mut self, tool: &str, args: serde_json::Value) -> (String, bool) {
        let v = self.request("tools/call", serde_json::json!({ "name": tool, "arguments": args }));
        let text = v["result"]["content"][0]["text"].as_str().unwrap_or_default().to_string();
        (text, v["result"]["isError"] == true)
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

/// When a JUnit report says a test passed, the server records it against the
/// derived claim; a probe then opens on that test by name in an isolated
/// worktree, and closing it with a break left there records the round and
/// resets the worktree, never touching the developer's file.
#[test]
fn a_passing_test_is_ingested_probed_and_reset() {
    let dir = tempfile::tempdir().unwrap();
    let probes = tempfile::tempdir().unwrap();
    let d = dir.path();
    write(d, "Cargo.toml", "[workspace]\nmembers = [\"crates/*\"]\n");
    write(d, "crates/notes/Cargo.toml", "[package]\nname = \"notes\"\nversion = \"0.1.0\"\n");
    write(d, "crates/notes/src/lib.rs", "pub mod draft;\n");
    let draft = "pub fn save(text: &str) -> bool { !text.is_empty() }\n\
                 #[cfg(test)]\nmod tests {\n    /// If the text is empty, then refuses to save it\n    #[test]\n    fn refuses_empty() {}\n}\n";
    write(d, "crates/notes/src/draft.rs", draft);
    git(d, &["init", "-q"]);
    git(d, &["config", "user.email", "t@t.t"]);
    git(d, &["config", "user.name", "t"]);
    git(d, &["add", "-A"]);
    git(d, &["commit", "-qm", "init"]);
    // A verdict must postdate the files it fingerprints.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    write(
        d,
        "report.xml",
        r#"<testsuite name="notes"><testcase classname="notes" name="draft::tests::refuses_empty"/></testsuite>"#,
    );

    let mut server = Server::start(d, probes.path());
    let (text, err) = server.call("ingest_test_report", serde_json::json!({ "path": "report.xml" }));
    assert!(!err && text.contains("verdicts recorded for 1 claim(s)"), "{text}");

    let (text, err) = server.call("locate", serde_json::json!({ "file": "crates/notes/src/draft.rs" }));
    assert!(!err && text.contains("\"status\":\"proven\""), "{text}");

    let (text, err) = server.call("open_probe", serde_json::json!({ "test": "refuses_empty" }));
    assert!(!err, "{text}");
    assert!(text.contains("Break in: crates/notes/src/draft.rs"), "{text}");
    let wt = text.lines().nth(1).unwrap().trim().to_string();
    std::fs::write(Path::new(&wt).join("crates/notes/src/draft.rs"), draft.replace('!', "")).unwrap();

    let (text, err) = server.call(
        "close_probe",
        serde_json::json!({ "test": "refuses_empty", "probes": 1, "survivors": ["dropping the emptiness check went unnoticed"] }),
    );
    assert!(!err && text.contains("1 break(s) tried, 1 survived"), "{text}");
    assert!(!text.contains("could not be recorded"), "{text}");
    assert_eq!(std::fs::read_to_string(Path::new(&wt).join("crates/notes/src/draft.rs")).unwrap(), draft);
    assert_eq!(std::fs::read_to_string(d.join("crates/notes/src/draft.rs")).unwrap(), draft);

    let (text, _) = server.call("locate", serde_json::json!({ "file": "crates/notes/src/draft.rs" }));
    assert!(text.contains("a break survived"), "the finding shows on the claim: {text}");
}
