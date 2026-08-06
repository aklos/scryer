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
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Ceiling on concurrently-handled requests. A `/status` or `/close` does a full
/// model read + anchor scan (seconds on a big repo), so handling requests inline
/// on the accept loop lets one slow call — or a slow/half-open client — stall
/// every other session's hooks behind it. Each accepted connection gets its own
/// worker thread up to this cap; past it we serve inline as backpressure so the
/// thread count can never run away. Loopback, low volume — a small cap is ample.
const MAX_INFLIGHT: usize = 8;

/// Decrements the in-flight counter when a worker finishes (or panics).
struct InflightGuard(Arc<AtomicUsize>);
impl Drop for InflightGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// How long a recorded touch stays live. A single agent run is minutes; a
/// resumed session (same id, reconnecting hours later) must not re-gate on the
/// prior run's edits, so touches older than this are pruned on every touch and
/// close. Long enough not to forget a genuinely long session, short enough that
/// a stale run's touches age out.
const TOUCH_TTL: Duration = Duration::from_secs(2 * 3600);

/// Drop touches older than [`TOUCH_TTL`].
fn prune_touches(log: &mut Vec<(Instant, Touch)>, now: Instant) {
    log.retain(|(at, _)| now.saturating_duration_since(*at) < TOUCH_TTL);
}

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
/// discovery file, and serve until the returned server is dropped. `on_touch`
/// fires for every recorded touch — the app forwards it to the canvas as a
/// live "session is working here" signal.
pub fn start(
    project: &Path,
    on_touch: impl Fn(&Touch) + Send + Sync + 'static,
) -> Result<HookServer, String> {
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
    let touches: Arc<Mutex<Vec<(Instant, Touch)>>> = Arc::new(Mutex::new(Vec::new()));
    // Shared across the accept loop and every worker thread it spawns.
    let project = Arc::new(project.to_path_buf());
    let token = Arc::new(token);
    let on_touch = Arc::new(on_touch);
    let inflight = Arc::new(AtomicUsize::new(0));

    {
        let shutdown = shutdown.clone();
        let project = project.clone();
        std::thread::spawn(move || {
            while !shutdown.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let _ = stream.set_nodelay(true);
                        // Under the cap, hand the connection to a worker so a slow
                        // request or half-open client can't block the next accept;
                        // at the cap, serve inline as backpressure.
                        if inflight.fetch_add(1, Ordering::SeqCst) < MAX_INFLIGHT {
                            let project = project.clone();
                            let token = token.clone();
                            let touches = touches.clone();
                            let on_touch = on_touch.clone();
                            let guard = InflightGuard(inflight.clone());
                            std::thread::spawn(move || {
                                let _guard = guard;
                                handle_request(stream, &project, &token, &touches, on_touch.as_ref());
                            });
                        } else {
                            inflight.fetch_sub(1, Ordering::SeqCst);
                            handle_request(stream, &project, &token, &touches, on_touch.as_ref());
                        }
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
    touches: &Arc<Mutex<Vec<(Instant, Touch)>>>,
    on_touch: &(impl Fn(&Touch) + Send + 'static),
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
            let now = Instant::now();
            let mut log = touches.lock().unwrap();
            prune_touches(&mut log, now);
            if !log.iter().any(|(_, t)| t == &touch) {
                on_touch(&touch);
                log.push((now, touch));
            }
            respond(&mut stream, 200, &serde_json::json!({ "recorded": log.len() }));
        }
        ("GET", "/close") => {
            let session = param("session").unwrap_or_default();
            let mut log = touches.lock().unwrap();
            prune_touches(&mut log, Instant::now());
            let touched: Vec<Touch> = log
                .iter()
                .filter(|(_, t)| session.is_empty() || t.session == session)
                .map(|(_, t)| t.clone())
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
    let planned = scryer_core::read_planned_at(&r)?;

    // Both altitudes: the element queue (what get_pending hands the agent) and
    // the node/group carriers the canvas draws. One without the other is how
    // this endpoint used to disagree with the agent's own count.
    let pending = scryer_core::diff::pending_element_count(&model, &planned);
    let carriers = scryer_core::diff::plan_carrier_count(&model, &planned);

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

    // Same phrasing as `scryer-mcp statusline` — one sentence for the model's
    // standing state, wherever it is read.
    let plural = if carriers == 1 { "" } else { "s" };
    let work = if pending == 0 {
        "0 pending".to_string()
    } else {
        format!("{pending} pending across {carriers} node{plural}")
    };
    let status_line = format!(
        "scryer: {work} · {drift} drift scope(s) · anchors: {broken} broken, {changed} changed"
    );
    Ok(serde_json::json!({
        "pending": pending,
        "carriers": carriers,
        "driftScopes": drift,
        "anchorsBroken": broken,
        "anchorsChanged": changed,
        "statusLine": status_line,
    }))
}

/// The close-gate view, anchor-informed so it gates only what is genuinely
/// out of sync. Touched files partition three ways:
///
/// - `needsReconcile` — files carrying a claim the check can't vouch for:
///   either a committed anchor whose fingerprint reports changed / broken /
///   missing, OR a plan-added / glob-pattern anchor that has no baseline to
///   fingerprint at all. Both mean "the session touched modeled code that isn't
///   verified clean" — the cases worth blocking on. Plan-layer and glob anchors
///   are exactly the blindness `8ad39fc` closed in completeness.
/// - `cleanModeled` — files whose only claims are committed anchors that hash
///   clean: the session edited around the modeled behaviour and owes nothing.
/// - `unmodeled` — files the model doesn't map at all.
///
/// The fingerprint check compares against the last reconcile baseline, so a
/// long-unreconciled file may surface pre-session changes too — still the
/// right call: the claim needs a look and this session just worked there.
fn close_payload(project: &Path, touched: &[Touch]) -> serde_json::Value {
    let r = scryer_core::ModelRef::ProjectLocal(project.to_path_buf());

    // Out-of-sync anchors across the project (may silently re-anchor moved
    // symbols, exactly like get_health). No baseline yet → no observations →
    // the gate stays silent rather than crying wolf on a fresh model.
    let observations = scryer_extract::anchors::check_anchors(&r)
        .map(|c| c.observations)
        .unwrap_or_default();

    // The working view names hosts and statements for observation keys, and —
    // crucially — carries the PLAN-layer source map, the anchors the committed-
    // only fingerprint check above can't see. `committed` is kept separately to
    // tell fingerprint-checkable anchors from the rest. Both built once.
    let committed = scryer_core::read_model_at(&r).ok();
    let working = match (&committed, scryer_core::read_planned_at(&r)) {
        (Some(c), Ok(p)) => Some(scryer_core::working_view(c, &p)),
        _ => committed.clone(),
    };
    let statement_of = |key: &str| -> Option<String> {
        // A test-anchor observation names the claim its test backs.
        let key = scryer_core::test_resp_id(key).unwrap_or(key);
        let w = working.as_ref()?;
        w.nodes
            .iter()
            .flat_map(|n| n.responsibilities.iter())
            .chain(w.groups.iter().flat_map(|g| g.responsibilities.iter()))
            .find(|resp| resp.id == key)
            .map(|resp| resp.statement.clone())
    };
    let host_name_of = |key: &str| -> Option<String> {
        let w = working.as_ref()?;
        for n in &w.nodes {
            if n.id == key || n.responsibilities.iter().any(|resp| resp.id == key) {
                return Some(n.name.clone());
            }
        }
        w.groups
            .iter()
            .find(|g| g.responsibilities.iter().any(|resp| resp.id == key))
            .map(|g| g.name.clone())
    };
    let loc_matches = |pattern: &str, file: &str| -> bool {
        pattern == file || glob::Pattern::new(pattern).is_ok_and(|p| p.matches(file))
    };
    // The fingerprint baseline covers only committed sourceMap keys with an EXACT
    // location for the file. Anything else on the file — a plan-added anchor (no
    // baseline yet) or a glob-pattern location (never fingerprinted, since the
    // baseline reads the pattern as a literal path) — can't be verified, so a
    // touch surfaces it for a look rather than passing it as clean.
    let committed_exact = |key: &str, file: &str| -> bool {
        committed.as_ref().is_some_and(|c| {
            c.source_map
                .get(key)
                .is_some_and(|locs| locs.iter().any(|l| l.pattern == file))
        })
    };

    let mut files: Vec<&str> = Vec::new();
    for t in touched {
        if !files.contains(&t.file.as_str()) {
            files.push(&t.file);
        }
    }

    let mut needs: Vec<serde_json::Value> = Vec::new();
    let mut clean_modeled: Vec<&str> = Vec::new();
    let mut unmodeled: Vec<&str> = Vec::new();
    for file in files {
        // 1) Committed anchors the fingerprint check flagged changed/broken/missing.
        let mut dirty: Vec<serde_json::Value> = observations
            .iter()
            .filter(|o| o.file == file)
            .map(|o| {
                let mut v = serde_json::json!({
                    "id": o.key,
                    "host": o.host_name,
                    "symbol": o.symbol,
                    "state": o.state,
                    "statement": statement_of(&o.key),
                });
                if let serde_json::Value::Object(map) = &mut v {
                    map.retain(|_, val| !val.is_null());
                }
                v
            })
            .collect();

        // 2) Plan-added and glob anchors on this file — unverifiable, so a touch
        //    surfaces them (mirrors how completeness resolves plan-layer anchors).
        if let Some(w) = working.as_ref() {
            let mut keys: Vec<&String> = w.source_map.keys().collect();
            keys.sort();
            for key in keys {
                if committed_exact(key, file) {
                    continue; // fingerprint-checkable — handled in (1) or genuinely clean
                }
                if let Some(loc) = w.source_map[key].iter().find(|l| loc_matches(&l.pattern, file)) {
                    let mut v = serde_json::json!({
                        "id": key,
                        "host": host_name_of(key),
                        "symbol": loc.symbol,
                        "state": "unreconciled",
                        "statement": statement_of(key),
                    });
                    if let serde_json::Value::Object(map) = &mut v {
                        map.retain(|_, val| !val.is_null());
                    }
                    dirty.push(v);
                }
            }
        }

        if !dirty.is_empty() {
            needs.push(serde_json::json!({ "file": file, "claims": dirty }));
        } else {
            let modeled = scryer_core::locate::locate_at(&r, file, None)
                .map(|rep| !rep.result.claims.is_empty())
                .unwrap_or(false);
            if modeled {
                clean_modeled.push(file);
            } else {
                unmodeled.push(file);
            }
        }
    }

    serde_json::json!({
        "touched": touched,
        "needsReconcile": needs,
        "cleanModeled": clean_modeled,
        "unmodeled": unmodeled,
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

    /// Concurrent clients — more than MAX_INFLIGHT, mixing status reads and
    /// distinct touches — all succeed and the shared touch log stays consistent:
    /// exercises both the worker path and the inline-backpressure path, and the
    /// touches Mutex + dedup under contention.
    #[test]
    fn handles_concurrent_requests() {
        let (dir, _) = temp_project();
        let hits = Arc::new(AtomicUsize::new(0));
        let counter = hits.clone();
        let server = start(dir.path(), move |_| {
            counter.fetch_add(1, Ordering::SeqCst);
        })
        .unwrap();
        let disc: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(dir.path().join(".scryer/hook.json")).unwrap(),
        )
        .unwrap();
        let token = disc["token"].as_str().unwrap().to_string();
        let port = server.port;

        let handles: Vec<_> = (0..16)
            .map(|i| {
                let token = token.clone();
                std::thread::spawn(move || {
                    if i % 2 == 0 {
                        assert_eq!(request(port, &token, "GET", "/status", "").0, 200);
                    } else {
                        let body = format!(r#"{{"session":"s","file":"f{i}.rs"}}"#);
                        assert_eq!(request(port, &token, "POST", "/touch", &body).0, 200);
                    }
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }

        // The eight distinct touches (odd i) all recorded, each notified once.
        let (_, v) = request(port, &token, "GET", "/close?session=s", "");
        assert_eq!(v["touched"].as_array().unwrap().len(), 8);
        assert_eq!(hits.load(Ordering::SeqCst), 8);
    }

    /// Touches older than the TTL are pruned; fresh ones survive — a resumed
    /// session must not re-gate on a prior run's hours-old edits.
    #[test]
    fn prune_drops_only_stale_touches() {
        let now = Instant::now();
        let touch = |f: &str| Touch { session: "s".into(), file: f.into(), symbol: None };
        let mut log = vec![
            (now.checked_sub(TOUCH_TTL + Duration::from_secs(60)).unwrap(), touch("old.rs")),
            (now.checked_sub(Duration::from_secs(60)).unwrap(), touch("fresh.rs")),
        ];
        prune_touches(&mut log, now);
        assert_eq!(log.len(), 1);
        assert_eq!(log[0].1.file, "fresh.rs");
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
        let touched_events = Arc::new(Mutex::new(0usize));
        let counter = touched_events.clone();
        let server = start(dir.path(), move |_| {
            *counter.lock().unwrap() += 1;
        })
        .unwrap();

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

        // Touches record (deduped, notifying the app once) and partition on
        // /close: no anchor baseline exists, so the modeled file reads CLEAN
        // (the gate must not cry wolf on a never-reconciled model).
        let body = r#"{"session":"s1","file":"src/auth.rs","symbol":"verify"}"#;
        request(server.port, &token, "POST", "/touch", body);
        request(server.port, &token, "POST", "/touch", body);
        request(
            server.port,
            &token,
            "POST",
            "/touch",
            r#"{"session":"s1","file":"README.md"}"#,
        );
        assert_eq!(*touched_events.lock().unwrap(), 2, "one notify per distinct touch");
        let (_, v) = request(server.port, &token, "GET", "/close?session=s1", "");
        assert_eq!(v["touched"].as_array().unwrap().len(), 2, "deduped");
        assert!(v["needsReconcile"].as_array().unwrap().is_empty());
        assert_eq!(v["cleanModeled"][0], "src/auth.rs");
        assert_eq!(v["unmodeled"][0], "README.md");
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

    /// With a reconcile baseline in place, /close gates on the anchor
    /// fingerprints: editing the anchored symbol puts the claim in
    /// needsReconcile; editing around it leaves the file cleanModeled.
    #[test]
    fn close_gates_on_anchor_fingerprints() {
        let (dir, _) = temp_project();
        let root = dir.path();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("src/auth.rs"),
            "fn verify() { let ok = true; }\nfn other() {}\n",
        )
        .unwrap();
        let r = ModelRef::ProjectLocal(root.to_path_buf());
        scryer_core::write_sync_state(
            &r,
            &scryer_core::drift::SyncState::anchored_now(None),
        )
        .unwrap();
        scryer_extract::anchors::write_baseline(&r).unwrap();

        let server = start(root, |_| {}).unwrap();
        let disc: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(root.join(".scryer/hook.json")).unwrap(),
        )
        .unwrap();
        let token = disc["token"].as_str().unwrap().to_string();
        let touch = r#"{"session":"s1","file":"src/auth.rs"}"#;
        request(server.port, &token, "POST", "/touch", touch);

        // The anchor check's mtime gate has 1 s granularity — step past it so
        // the edits below are visible (same dance as the drift tests).
        std::thread::sleep(std::time::Duration::from_millis(1100));

        // Edit AROUND the anchored symbol: still clean, no gate.
        std::fs::write(
            root.join("src/auth.rs"),
            "fn verify() { let ok = true; }\nfn other() { println!(); }\n",
        )
        .unwrap();
        let (_, v) = request(server.port, &token, "GET", "/close?session=s1", "");
        assert!(
            v["needsReconcile"].as_array().unwrap().is_empty(),
            "untouched anchor must not gate: {v}"
        );
        assert_eq!(v["cleanModeled"][0], "src/auth.rs");

        // Edit the anchored symbol itself: the claim demands reconciliation.
        std::fs::write(
            root.join("src/auth.rs"),
            "fn verify() { let ok = false; }\nfn other() { println!(); }\n",
        )
        .unwrap();
        let (_, v) = request(server.port, &token, "GET", "/close?session=s1", "");
        let claim = &v["needsReconcile"][0]["claims"][0];
        assert_eq!(v["needsReconcile"][0]["file"], "src/auth.rs", "{v}");
        assert_eq!(claim["id"], "r-1");
        assert_eq!(claim["statement"], "serves requests");
        assert_eq!(claim["state"], "changed");
    }

    /// A claim authored AND anchored during the session — living only in the
    /// plan, with no committed baseline to fingerprint — must still gate the
    /// close when the session touched its file. It can't be verified clean, so
    /// it surfaces as `unreconciled`, not waved through as cleanModeled.
    #[test]
    fn close_gates_on_a_plan_authored_anchor() {
        let (dir, _) = temp_project();
        let root = dir.path();
        let r = ModelRef::ProjectLocal(root.to_path_buf());

        // Add a PLAN-only responsibility on `api`, anchored to a new file, and
        // leave committed untouched — the authored-and-anchored session flow.
        let committed = scryer_core::read_model_at(&r).unwrap();
        let mut plan = committed.clone();
        if let Some(api) = plan.nodes.iter_mut().find(|n| n.id == "api") {
            api.responsibilities.push(
                serde_json::from_value(serde_json::json!({
                    "id": "r-2", "statement": "new plan claim"
                }))
                .unwrap(),
            );
        }
        plan.source_map.insert(
            "r-2".into(),
            vec![serde_json::from_value(
                serde_json::json!({ "pattern": "src/new.rs", "symbol": "foo" }),
            )
            .unwrap()],
        );
        scryer_core::write_planned_at(&r, &plan).unwrap();

        let server = start(root, |_| {}).unwrap();
        let disc: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(root.join(".scryer/hook.json")).unwrap(),
        )
        .unwrap();
        let token = disc["token"].as_str().unwrap().to_string();
        request(server.port, &token, "POST", "/touch", r#"{"session":"s1","file":"src/new.rs"}"#);

        let (_, v) = request(server.port, &token, "GET", "/close?session=s1", "");
        assert_eq!(v["needsReconcile"][0]["file"], "src/new.rs", "{v}");
        let claim = &v["needsReconcile"][0]["claims"][0];
        assert_eq!(claim["id"], "r-2");
        assert_eq!(claim["state"], "unreconciled");
        assert_eq!(claim["statement"], "new plan claim");
        // The committed exact anchor's own file, untouched, must not be dragged in.
        assert!(v["cleanModeled"].as_array().unwrap().is_empty(), "{v}");
    }
}
