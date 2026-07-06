//! Loopback session-hook endpoint.
//!
//! While a project is open in the app, a tiny HTTP server on 127.0.0.1 answers
//! the questions a coding-agent session hook asks: "what's the model's status?"
//! (SessionStart), "what intent governs this file?" (post-Read overlay), "note
//! that I touched this symbol" (post-Edit), and "what's unreconciled?" (Stop).
//! The server is advertised through `.scryer/hook.json` — hooks that find no
//! live endpoint exit silently, so the whole hook surface is inert unless the
//! app is running: opening Scryer IS the opt-in, closing it the opt-out.
//!
//! Deliberately minimal HTTP/1.1 (loopback, one tiny JSON exchange per
//! request, `Connection: close`) so no server dependency is pulled in.

use serde::Serialize;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// Managed Tauri state: the endpoint for the currently open project, if any.
pub struct HookState(pub Mutex<Option<HookServer>>);

/// One recorded edit: an agent session touched (file, symbol?).
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Touch {
    pub session: String,
    pub file: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
}

/// The live endpoint for one open project. Dropping it stops the listener
/// thread and removes the discovery file, so hooks fall silent the moment the
/// project closes.
pub struct HookServer {
    pub port: u16,
    project: PathBuf,
    shutdown: Arc<AtomicBool>,
}

impl HookServer {
    pub fn project(&self) -> &Path {
        &self.project
    }
}

impl Drop for HookServer {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
        let _ = std::fs::remove_file(discovery_path(&self.project));
    }
}

fn discovery_path(project: &Path) -> PathBuf {
    project.join(".scryer").join("hook.json")
}

/// A 128-bit unguessable token from two independently seeded SipHash states.
/// Loopback-only defense: another local user can reach 127.0.0.1, but without
/// the token (readable only from the project's own `.scryer/`) requests bounce.
fn mint_token() -> String {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    let word = |seed: u64| {
        let mut h = RandomState::new().build_hasher();
        h.write_u64(seed);
        h.finish()
    };
    format!("{:016x}{:016x}", word(1), word(2))
}

/// Start the endpoint for `project`: bind an ephemeral loopback port, write the
/// discovery file, and serve until the returned server is dropped.
pub fn start(project: &Path) -> Result<HookServer, String> {
    let listener = TcpListener::bind(("127.0.0.1", 0)).map_err(|e| e.to_string())?;
    listener
        .set_nonblocking(true)
        .map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let token = mint_token();

    let scryer_dir = project.join(".scryer");
    std::fs::create_dir_all(&scryer_dir).map_err(|e| e.to_string())?;
    let discovery = serde_json::json!({
        "port": port,
        "token": token,
        "pid": std::process::id(),
    });
    std::fs::write(
        discovery_path(project),
        serde_json::to_string_pretty(&discovery).unwrap_or_default(),
    )
    .map_err(|e| e.to_string())?;

    let shutdown = Arc::new(AtomicBool::new(false));
    let touches: Arc<Mutex<Vec<Touch>>> = Arc::new(Mutex::new(Vec::new()));

    {
        let shutdown = shutdown.clone();
        let project = project.to_path_buf();
        std::thread::spawn(move || {
            while !shutdown.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let _ = stream.set_nodelay(true);
                        handle_request(stream, &project, &token, &touches);
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(std::time::Duration::from_millis(50));
                    }
                    Err(_) => break,
                }
            }
        });
    }

    Ok(HookServer {
        port,
        project: project.to_path_buf(),
        shutdown,
    })
}

/// Serve one request: parse the minimal HTTP exchange, check the token, route.
fn handle_request(
    mut stream: std::net::TcpStream,
    project: &Path,
    token: &str,
    touches: &Arc<Mutex<Vec<Touch>>>,
) {
    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(2)));

    // Read until the header/body split, then the Content-Length'd body.
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let (head, mut body) = loop {
        match stream.read(&mut chunk) {
            Ok(0) => return,
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                if let Some(split) = find_header_end(&buf) {
                    let head = String::from_utf8_lossy(&buf[..split]).to_string();
                    break (head, buf[split + 4..].to_vec());
                }
                if buf.len() > 64 * 1024 {
                    return; // no legitimate hook request is this large
                }
            }
            Err(_) => return,
        }
    };

    let mut lines = head.lines();
    let request_line = lines.next().unwrap_or_default();
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let target = parts.next().unwrap_or_default().to_string();

    let mut req_token = String::new();
    let mut content_length = 0usize;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else { continue };
        match name.trim().to_ascii_lowercase().as_str() {
            "x-scryer-token" => req_token = value.trim().to_string(),
            "content-length" => content_length = value.trim().parse().unwrap_or(0),
            _ => {}
        }
    }
    while body.len() < content_length.min(64 * 1024) {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => body.extend_from_slice(&chunk[..n]),
            Err(_) => return,
        }
    }

    if req_token != token {
        respond(&mut stream, 401, &serde_json::json!({ "error": "bad or missing x-scryer-token" }));
        return;
    }

    let (path, query) = match target.split_once('?') {
        Some((p, q)) => (p, q),
        None => (target.as_str(), ""),
    };
    let param = |key: &str| -> Option<String> {
        query.split('&').find_map(|kv| {
            let (k, v) = kv.split_once('=')?;
            (k == key).then(|| percent_decode(v))
        })
    };

    match (method.as_str(), path) {
        ("GET", "/status") => match status_payload(project) {
            Ok(v) => respond(&mut stream, 200, &v),
            Err(e) => respond(&mut stream, 500, &serde_json::json!({ "error": e })),
        },
        ("GET", "/overlay") => {
            let Some(file) = param("file") else {
                respond(&mut stream, 400, &serde_json::json!({ "error": "missing ?file=" }));
                return;
            };
            let file = relativize(project, &file);
            let r = scryer_core::ModelRef::ProjectLocal(project.to_path_buf());
            match scryer_core::locate::locate_at(&r, &file, param("symbol").as_deref()) {
                Ok(report) => {
                    let mut v = serde_json::to_value(&report).unwrap_or_default();
                    if let serde_json::Value::Object(map) = &mut v {
                        map.insert("file".into(), serde_json::json!(file));
                    }
                    respond(&mut stream, 200, &v)
                }
                Err(e) => respond(&mut stream, 500, &serde_json::json!({ "error": e })),
            }
        }
        ("POST", "/touch") => {
            let Ok(v) = serde_json::from_slice::<serde_json::Value>(&body) else {
                respond(&mut stream, 400, &serde_json::json!({ "error": "body must be JSON" }));
                return;
            };
            let (Some(session), Some(file)) =
                (v["session"].as_str(), v["file"].as_str())
            else {
                respond(
                    &mut stream,
                    400,
                    &serde_json::json!({ "error": "body needs {session, file, symbol?}" }),
                );
                return;
            };
            let touch = Touch {
                session: session.to_string(),
                file: relativize(project, file),
                symbol: v["symbol"].as_str().map(str::to_string),
            };
            let mut log = touches.lock().unwrap();
            if !log.contains(&touch) {
                log.push(touch);
            }
            respond(&mut stream, 200, &serde_json::json!({ "recorded": log.len() }));
        }
        ("GET", "/close") => {
            let session = param("session").unwrap_or_default();
            let log = touches.lock().unwrap();
            let touched: Vec<Touch> = log
                .iter()
                .filter(|t| session.is_empty() || t.session == session)
                .cloned()
                .collect();
            drop(log);
            respond(&mut stream, 200, &close_payload(project, &touched));
        }
        _ => respond(
            &mut stream,
            404,
            &serde_json::json!({
                "error": format!("unknown route {method} {path}"),
                "routes": ["GET /status", "GET /overlay?file=&symbol=", "POST /touch", "GET /close?session="],
            }),
        ),
    }
}

fn find_header_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n")
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
                match u8::from_str_radix(hex, 16) {
                    Ok(b) => {
                        out.push(b);
                        i += 3;
                    }
                    Err(_) => {
                        out.push(bytes[i]);
                        i += 1;
                    }
                }
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Hooks pass whatever path the harness gave them — normalize to the model's
/// project-relative, `/`-separated convention.
fn relativize(project: &Path, file: &str) -> String {
    let file = file.replace('\\', "/");
    let root = project.to_string_lossy().replace('\\', "/");
    let rel = file
        .strip_prefix(root.as_str())
        .map(|r| r.trim_start_matches('/'))
        .unwrap_or(&file);
    rel.trim_start_matches("./").to_string()
}

fn respond(stream: &mut std::net::TcpStream, status: u16, body: &serde_json::Value) {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        _ => "Internal Server Error",
    };
    let body = serde_json::to_string_pretty(body).unwrap_or_else(|_| "{}".into());
    let _ = write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len(),
    );
    let _ = stream.flush();
}

/// The ambient session status: pending plan entries, drift scopes, and anchor
/// health — as counts plus the one-liner hooks inject verbatim.
fn status_payload(project: &Path) -> Result<serde_json::Value, String> {
    let r = scryer_core::ModelRef::ProjectLocal(project.to_path_buf());
    let model = scryer_core::read_model_at(&r)?;

    let pending = scryer_core::plan_diff_at(&r)?.changes.len();

    // A model never reconciled has no anchor to measure against — reporting
    // "everything drifted" would be the false alarm this endpoint exists to
    // avoid (get_drift/get_health seed the anchor; a status probe must not).
    let drift = if r.sync_path().exists() {
        let sync = scryer_core::read_sync_state(&r);
        scryer_core::drift::drifted_scopes(&model, project, &sync).len()
    } else {
        0
    };

    // Anchor states from the git-free fingerprint check (may silently re-anchor
    // moved symbols, exactly like get_health).
    let (broken, changed) = match scryer_extract::anchors::check_anchors(&r) {
        Ok(check) => {
            use scryer_extract::anchors::AnchorState;
            let broken = check
                .observations
                .iter()
                .filter(|o| matches!(o.state, AnchorState::Broken | AnchorState::FileMissing))
                .count();
            let changed = check
                .observations
                .iter()
                .filter(|o| matches!(o.state, AnchorState::Changed))
                .count();
            (broken, changed)
        }
        Err(_) => (0, 0),
    };

    let status_line = format!(
        "scryer: {pending} pending · {drift} drift scope(s) · anchors: {broken} broken, {changed} changed"
    );
    Ok(serde_json::json!({
        "pending": pending,
        "driftScopes": drift,
        "anchorsBroken": broken,
        "anchorsChanged": changed,
        "statusLine": status_line,
    }))
}

/// The close-gate view: every touched file with the claims anchored to it, so
/// a Stop hook (or the piece-5 reconcile flow) can ask "does the model still
/// describe what you just did?" about exactly the right slice.
fn close_payload(project: &Path, touched: &[Touch]) -> serde_json::Value {
    let r = scryer_core::ModelRef::ProjectLocal(project.to_path_buf());
    let files: Vec<serde_json::Value> = {
        let mut seen = std::collections::BTreeSet::new();
        touched
            .iter()
            .filter(|t| seen.insert(t.file.clone()))
            .map(|t| {
                let claims = scryer_core::locate::locate_at(&r, &t.file, None)
                    .map(|rep| {
                        rep.result
                            .claims
                            .iter()
                            .map(|c| {
                                serde_json::json!({
                                    "id": c.id,
                                    "statement": c.statement,
                                    "host": c.host_name,
                                    "symbol": c.anchor.symbol,
                                })
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                serde_json::json!({ "file": t.file, "claims": claims })
            })
            .collect()
    };
    serde_json::json!({
        "touched": touched,
        "files": files,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use scryer_core::{Kind, ModelRef, Node, ScryModel};

    fn node(id: &str, kind: Kind, name: &str, parent: Option<&str>) -> Node {
        serde_json::from_value(serde_json::json!({
            "id": id, "kind": kind_str(kind), "name": name, "parentId": parent,
        }))
        .unwrap()
    }
    fn kind_str(k: Kind) -> &'static str {
        match k {
            Kind::Person => "person",
            Kind::System => "system",
            Kind::Container => "container",
            Kind::Component => "component",
            Kind::Symbol => "symbol",
        }
    }

    /// System > Container with a claim anchored in src/auth.rs.
    fn temp_project() -> (tempfile::TempDir, String) {
        let dir = tempfile::tempdir().unwrap();
        let r = ModelRef::ProjectLocal(dir.path().to_path_buf());
        let mut m = ScryModel::new();
        m.nodes.push(node("sys", Kind::System, "Acme", None));
        let mut api = node("api", Kind::Container, "API", Some("sys"));
        api.responsibilities = vec![serde_json::from_value(
            serde_json::json!({ "id": "r-1", "statement": "serves requests" }),
        )
        .unwrap()];
        m.nodes.push(api);
        m.source_map.insert(
            "r-1".into(),
            vec![serde_json::from_value(
                serde_json::json!({ "pattern": "src/auth.rs", "symbol": "verify" }),
            )
            .unwrap()],
        );
        scryer_core::write_model_at(&r, &m).unwrap();
        let token = {
            let raw = std::fs::read_to_string(dir.path().join(".scryer/hook.json"));
            raw.ok()
        };
        assert!(token.is_none(), "no discovery file before start");
        (dir, String::new())
    }

    fn request(port: u16, token: &str, method: &str, target: &str, body: &str) -> (u16, serde_json::Value) {
        let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        write!(
            s,
            "{method} {target} HTTP/1.1\r\nHost: localhost\r\nx-scryer-token: {token}\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        let mut out = String::new();
        s.read_to_string(&mut out).unwrap();
        let status: u16 = out
            .split_whitespace()
            .nth(1)
            .and_then(|c| c.parse().ok())
            .unwrap_or(0);
        let json_start = out.find("\r\n\r\n").map(|i| i + 4).unwrap_or(0);
        let value = serde_json::from_str(&out[json_start..]).unwrap_or(serde_json::Value::Null);
        (status, value)
    }

    #[test]
    fn serves_status_overlay_touch_and_close_with_token_gate() {
        let (dir, _) = temp_project();
        let server = start(dir.path()).unwrap();

        // Discovery file advertises the live endpoint.
        let disc: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(dir.path().join(".scryer/hook.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(disc["port"].as_u64().unwrap() as u16, server.port);
        let token = disc["token"].as_str().unwrap().to_string();

        // Wrong token bounces.
        let (status, _) = request(server.port, "nope", "GET", "/status", "");
        assert_eq!(status, 401);

        // Status carries counts + the injectable one-liner.
        let (status, v) = request(server.port, &token, "GET", "/status", "");
        assert_eq!(status, 200);
        assert!(v["statusLine"].as_str().unwrap().starts_with("scryer:"));

        // Overlay resolves a file to its claims (absolute path normalized).
        let abs = format!("{}/src/auth.rs", dir.path().display());
        let (status, v) = request(
            server.port,
            &token,
            "GET",
            &format!("/overlay?file={}", abs.replace('/', "%2F")),
            "",
        );
        assert_eq!(status, 200);
        assert_eq!(v["claims"][0]["id"], "r-1");
        assert_eq!(v["file"], "src/auth.rs");

        // Touches record (deduped) and come back per session on /close.
        let body = r#"{"session":"s1","file":"src/auth.rs","symbol":"verify"}"#;
        request(server.port, &token, "POST", "/touch", body);
        request(server.port, &token, "POST", "/touch", body);
        let (_, v) = request(server.port, &token, "GET", "/close?session=s1", "");
        assert_eq!(v["touched"].as_array().unwrap().len(), 1, "deduped");
        assert_eq!(v["files"][0]["claims"][0]["id"], "r-1");
        let (_, v) = request(server.port, &token, "GET", "/close?session=other", "");
        assert_eq!(v["touched"].as_array().unwrap().len(), 0);

        // Dropping the server removes the discovery file and stops serving.
        let port = server.port;
        drop(server);
        assert!(!dir.path().join(".scryer/hook.json").exists());
        std::thread::sleep(std::time::Duration::from_millis(150));
        assert!(
            std::net::TcpStream::connect(("127.0.0.1", port)).is_err()
                || request(port, &token, "GET", "/status", "").0 == 0,
            "endpoint is inert after drop"
        );
    }
}
