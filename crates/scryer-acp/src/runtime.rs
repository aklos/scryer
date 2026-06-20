use std::path::PathBuf;

use agent_client_protocol::{
    self as acp, Agent as _, CancelNotification, ClientSideConnection,
    InitializeRequest, McpServer, McpServerStdio, NewSessionRequest, PromptRequest,
    ProtocolVersion, StopReason,
};
use tokio::sync::{mpsc, oneshot};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

use crate::client::ScryerClient;
use crate::events::{AgentEvent, Usage};
use crate::AgentKind;

/// How the agent should be launched.
#[derive(Clone)]
pub enum LaunchMode {
    /// CLI print mode. Uses the user's subscription.
    Cli { kind: AgentKind },
    /// ACP subprocess.
    Acp,
}

/// Commands sent to the runtime.
enum RuntimeCommand {
    Start {
        agent_binary: String,
        mode: LaunchMode,
        cwd: String,
        model_name: String,
        effort: String,
        mcp_binary: String,
        prompt: String,
        allowed_tools: Vec<String>,
        event_tx: mpsc::UnboundedSender<AgentEvent>,
        result_tx: oneshot::Sender<Result<String, String>>,
    },
    Cancel {
        result_tx: oneshot::Sender<Result<(), String>>,
    },
    /// Internal: the session with this id finished naturally.
    Done { id: u64 },
}

/// Manages agent sync sessions.
///
/// Supports two launch modes:
/// - CLI: spawns `agent -p` with MCP config flags (Claude Code, Codex)
/// - ACP: spawns an ACP-compatible binary and runs the protocol handshake
#[derive(Clone)]
pub struct AcpRuntime {
    cmd_tx: mpsc::UnboundedSender<RuntimeCommand>,
}

impl AcpRuntime {
    pub fn new() -> Self {
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        let done_tx = cmd_tx.clone();
        std::thread::Builder::new()
            .name("agent-runtime".into())
            .spawn(move || {
                runtime_thread(cmd_rx, done_tx);
            })
            .expect("failed to spawn agent runtime thread");
        Self { cmd_tx }
    }

    /// Start a new sync session.
    pub async fn start_session(
        &self,
        agent_binary: String,
        mode: LaunchMode,
        cwd: String,
        model_name: String,
        effort: String,
        mcp_binary: String,
        prompt: String,
        allowed_tools: Vec<String>,
        event_tx: mpsc::UnboundedSender<AgentEvent>,
    ) -> Result<String, String> {
        let (result_tx, result_rx) = oneshot::channel();
        self.cmd_tx
            .send(RuntimeCommand::Start {
                agent_binary,
                mode,
                cwd,
                model_name,
                effort,
                mcp_binary,
                prompt,
                allowed_tools,
                event_tx,
                result_tx,
            })
            .map_err(|_| "Runtime is gone".to_string())?;
        result_rx.await.map_err(|_| "Runtime dropped".to_string())?
    }

    /// Cancel every active session (used for the user's single "stop" / a build
    /// of many parallel container sessions / orphan cleanup on dev refresh).
    pub async fn cancel(&self) -> Result<(), String> {
        let (result_tx, result_rx) = oneshot::channel();
        self.cmd_tx
            .send(RuntimeCommand::Cancel { result_tx })
            .map_err(|_| "Runtime is gone".to_string())?;
        result_rx.await.map_err(|_| "Runtime dropped".to_string())?
    }
}

fn runtime_thread(
    mut cmd_rx: mpsc::UnboundedReceiver<RuntimeCommand>,
    done_tx: mpsc::UnboundedSender<RuntimeCommand>,
) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("failed to create tokio runtime");

    let local = tokio::task::LocalSet::new();

    local.block_on(&rt, async move {
        // Cancel sender per in-flight session, keyed by a monotonic id. Many
        // sessions can run at once — they multiplex on this one LocalSet (each
        // is mostly waiting on its agent subprocess), so a parallel Wave 2 just
        // means several entries here. The orchestrator bounds how many start.
        let mut sessions: std::collections::HashMap<u64, oneshot::Sender<()>> =
            std::collections::HashMap::new();
        let mut next_id: u64 = 0;

        while let Some(cmd) = cmd_rx.recv().await {
            match cmd {
                RuntimeCommand::Start {
                    agent_binary,
                    mode,
                    cwd,
                    model_name,
                    effort,
                    mcp_binary,
                    prompt,
                    allowed_tools,
                    event_tx,
                    result_tx,
                } => {
                    let id = next_id;
                    next_id += 1;
                    // External session id stays timestamp-based (unchanged for
                    // callers); the numeric `id` only routes Done/Cancel here.
                    let session_id = format!(
                        "sync-{}",
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_millis()
                    );

                    let tool_refs: Vec<&str> = allowed_tools.iter().map(|s| s.as_str()).collect();
                    let result = match mode {
                        LaunchMode::Cli { kind } => start_cli_session(
                            &agent_binary, &kind, &cwd, &model_name, &effort, &mcp_binary,
                            &prompt, &tool_refs, id, event_tx, done_tx.clone(),
                        ),
                        LaunchMode::Acp => start_acp_session(
                            &agent_binary, &cwd, &model_name, &mcp_binary,
                            &prompt, id, event_tx, done_tx.clone(),
                        ).await,
                    };

                    match result {
                        Ok(tx) => {
                            sessions.insert(id, tx);
                            let _ = result_tx.send(Ok(session_id));
                        }
                        Err(e) => {
                            let _ = result_tx.send(Err(e));
                        }
                    }
                }
                RuntimeCommand::Cancel { result_tx } => {
                    if sessions.is_empty() {
                        let _ = result_tx.send(Err("No active session".into()));
                    } else {
                        // Cancel every in-flight session — a build is many
                        // sessions and the user's "stop" means stop all of them.
                        for (_, tx) in sessions.drain() {
                            let _ = tx.send(());
                        }
                        let _ = result_tx.send(Ok(()));
                    }
                }
                RuntimeCommand::Done { id } => {
                    sessions.remove(&id);
                }
            }
        }
    });
}

// ---------------------------------------------------------------------------
// CLI mode: spawn `agent -p` with MCP config flags
// ---------------------------------------------------------------------------

fn start_cli_session(
    agent_binary: &str,
    kind: &AgentKind,
    cwd: &str,
    model_name: &str,
    effort: &str,
    mcp_binary: &str,
    prompt: &str,
    allowed_tools: &[&str],
    id: u64,
    event_tx: mpsc::UnboundedSender<AgentEvent>,
    done_tx: mpsc::UnboundedSender<RuntimeCommand>,
) -> Result<oneshot::Sender<()>, String> {
    let mut cmd = tokio::process::Command::new(agent_binary);

    // The prompt goes over STDIN for the known CLIs, never argv: an
    // evidence-embedded build prompt easily exceeds Linux's ~128 KB per-argument
    // cap (E2BIG), and both `claude -p` and `codex exec -` read stdin.
    let mut prompt_via_stdin = true;
    match kind {
        AgentKind::ClaudeCode => {
            let mcp_config = serde_json::json!({
                "mcpServers": {
                    "scryer": {
                        "type": "stdio",
                        "command": mcp_binary,
                        "args": []
                    }
                }
            });
            cmd.arg("-p")
                .arg("--output-format").arg("stream-json")
                .arg("--verbose")
                .arg("--effort").arg(effort);
            if !model_name.is_empty() {
                cmd.arg("--model").arg(model_name);
            }
            cmd.arg("--mcp-config").arg(mcp_config.to_string());
            for pat in allowed_tools {
                cmd.arg("--allowed-tools").arg(pat);
            }
            cmd.arg("--no-session-persistence");
        }
        AgentKind::Codex => {
            // Codex uses `codex exec` with MCP pre-configured via .codex/config.toml
            cmd.arg("exec")
                .arg("--full-auto")
                .arg("--json")
                .arg("--ephemeral")
                .arg("-c").arg(format!("model_reasoning_effort=\"{}\"", effort));
            if !model_name.is_empty() {
                cmd.arg("-c").arg(format!("model=\"{}\"", model_name));
            }
            // `-` = read the prompt from stdin.
            cmd.arg("-");
        }
        AgentKind::Other => {
            // Best-effort: pass prompt as last arg (unknown CLIs may not read
            // stdin — large prompts are unsupported here).
            cmd.arg(&prompt);
            prompt_via_stdin = false;
        }
    }

    cmd.current_dir(cwd)
        .stdin(if prompt_via_stdin {
            std::process::Stdio::piped()
        } else {
            std::process::Stdio::null()
        })
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);

    // Start in a new process group so we can kill the entire tree
    #[cfg(unix)]
    {
        #[allow(unused_imports)]
        use std::os::unix::process::CommandExt;
        unsafe { cmd.pre_exec(|| { libc::setpgid(0, 0); Ok(()) }); }
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;
        cmd.creation_flags(CREATE_NEW_PROCESS_GROUP);
    }

    let mut child = cmd.spawn()
        .map_err(|e| format!("Failed to spawn {agent_binary}: {e}"))?;

    if prompt_via_stdin {
        if let Some(mut stdin) = child.stdin.take() {
            let prompt_bytes = prompt.as_bytes().to_vec();
            tokio::task::spawn_local(async move {
                use tokio::io::AsyncWriteExt;
                let _ = stdin.write_all(&prompt_bytes).await;
                let _ = stdin.shutdown().await;
                // Dropping stdin closes the pipe — the CLI sees EOF and starts.
            });
        }
    }

    let child_pid = child.id();
    let (cancel_tx, cancel_rx) = oneshot::channel::<()>();

    // Tee the raw agent stream to disk. Sessions run with
    // `--no-session-persistence`, so without this nothing records what the agent
    // actually did — every tool call, every fill_container argument, and
    // every validator rejection is in this stdout stream. One file per session.
    let log_dir = std::path::Path::new(cwd).join(".scryer").join("build-logs");
    let _ = std::fs::create_dir_all(&log_dir);
    let stdout_log_path = log_dir.join(format!("session-{id}.jsonl"));
    let stderr_log_path = log_dir.join(format!("session-{id}.err.log"));

    tokio::task::spawn_local(async move {
        // Stream stdout and stderr to detect activity and tool call events.
        // Claude Code writes JSON events to stdout; some agents use stderr.
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let mut stdout_log = std::fs::File::create(&stdout_log_path).ok();
        let mut stderr_log = std::fs::File::create(&stderr_log_path).ok();

        let event_tx_stdout = event_tx.clone();
        let event_tx_stderr = event_tx.clone();

        let monitor = async {
            let stdout_task = async {
                if let Some(stdout) = stdout {
                    use std::io::Write as _;
                    use tokio::io::{AsyncBufReadExt, BufReader};
                    let reader = BufReader::new(stdout);
                    let mut lines = reader.lines();
                    while let Ok(Some(line)) = lines.next_line().await {
                        if let Some(f) = stdout_log.as_mut() {
                            let _ = writeln!(f, "{line}");
                        }
                        if let Some(usage) = extract_usage(&line) {
                            let _ = event_tx_stdout.send(AgentEvent::Usage { usage });
                        }
                        if let Some(msg) = summarize_event(&line) {
                            let _ = event_tx_stdout.send(AgentEvent::Message { text: msg });
                        }
                    }
                }
            };

            let last_stderr = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
            let last_stderr2 = last_stderr.clone();
            let stderr_task = async move {
                if let Some(stderr) = stderr {
                    use std::io::Write as _;
                    use tokio::io::{AsyncBufReadExt, BufReader};
                    let reader = BufReader::new(stderr);
                    let mut lines = reader.lines();
                    while let Ok(Some(line)) = lines.next_line().await {
                        if let Some(f) = stderr_log.as_mut() {
                            let _ = writeln!(f, "{line}");
                        }
                        *last_stderr2.lock().unwrap() = line.clone();
                        let _ = event_tx_stderr.send(AgentEvent::Message { text: line });
                    }
                }
            };

            // Drive completion off the process exit, not stream EOF. A
            // grandchild (notably the MCP server the agent spawns) can inherit
            // our stdout/stderr pipe and keep it open after the agent itself
            // exits — so waiting for the streams to close can hang forever.
            // Read the streams concurrently, but stop as soon as the child exits.
            let streams = async { tokio::join!(stdout_task, stderr_task); };
            tokio::pin!(streams);
            let waiter = child.wait();
            tokio::pin!(waiter);
            let status = tokio::select! {
                s = &mut waiter => s,
                _ = &mut streams => (&mut waiter).await,
            };
            (status, last_stderr)
        };

        tokio::select! {
            result = monitor => {
                let (status, last_stderr) = result;
                match status {
                    Ok(s) if s.success() => {
                        let _ = event_tx.send(AgentEvent::Completed {
                            stop_reason: "end_turn".into(),
                        });
                    }
                    Ok(s) => {
                        let stderr_line = last_stderr.lock().unwrap().clone();
                        let err_msg = if stderr_line.is_empty() {
                            format!("exit code {}", s.code().unwrap_or(-1))
                        } else {
                            stderr_line
                        };
                        let _ = event_tx.send(AgentEvent::Failed { error: err_msg });
                    }
                    Err(e) => {
                        let stderr_line = last_stderr.lock().unwrap().clone();
                        let err_msg = if stderr_line.is_empty() {
                            format!("{e}")
                        } else {
                            stderr_line
                        };
                        let _ = event_tx.send(AgentEvent::Failed { error: err_msg });
                    }
                }
            }
            _ = cancel_rx => {
                kill_process_tree(&mut child, child_pid).await;
                let _ = event_tx.send(AgentEvent::Cancelled);
            }
        }
        let _ = done_tx.send(RuntimeCommand::Done { id });
    });

    Ok(cancel_tx)
}

/// Drop the `mcp__<server>__` prefix from MCP tool names for display.
fn short_tool(name: &str) -> &str {
    name.rsplit("__").next().unwrap_or(name)
}

/// Last two path segments, e.g. "/home/alex/p/src/App.tsx" -> "src/App.tsx".
fn short_path(p: &str) -> String {
    let parts: Vec<&str> = p.trim_end_matches('/').split('/').filter(|s| !s.is_empty()).collect();
    match parts.as_slice() {
        [.., a, b] => format!("{}/{}", a, b),
        _ => p.to_string(),
    }
}

/// Pull a short, human-meaningful argument out of a tool_use input so the
/// activity readout shows *what* the tool is acting on (which file, which node).
fn tool_detail(input: &serde_json::Value) -> Option<String> {
    let obj = input.as_object()?;
    if let Some(p) = obj.get("file_path").or_else(|| obj.get("path")).and_then(|v| v.as_str()) {
        if !p.is_empty() {
            return Some(short_path(p));
        }
    }
    for key in ["pattern", "node_id", "nodeId", "query", "command", "url"] {
        if let Some(v) = obj.get(key).and_then(|v| v.as_str()) {
            if !v.is_empty() {
                let v = if v.chars().count() > 60 {
                    format!("{}…", v.chars().take(60).collect::<String>())
                } else {
                    v.to_string()
                };
                return Some(v);
            }
        }
    }
    None
}

/// Pull end-of-turn token usage out of one agent stream-json line. Returns
/// `Some` only for a line that actually reports a turn total — Claude Code's
/// `result` event (top-level `usage` + `total_cost_usd`) or Codex's token-count
/// event (nested under `info.total_token_usage`). Per-chunk `assistant` events
/// carry their partial usage under `message.usage`, which this deliberately does
/// NOT match, so we report the final total once, not every streamed delta.
fn extract_usage(line: &str) -> Option<Usage> {
    let val: serde_json::Value = serde_json::from_str(line).ok()?;
    let usage_obj = val
        .pointer("/usage")
        .or_else(|| val.pointer("/token_usage"))
        .or_else(|| val.pointer("/info/total_token_usage"))
        .or_else(|| val.pointer("/msg/info/total_token_usage"))?;

    let n = |key: &str| usage_obj.get(key).and_then(|v| v.as_u64()).unwrap_or(0);
    Some(Usage {
        input_tokens: n("input_tokens"),
        output_tokens: n("output_tokens"),
        cache_creation_input_tokens: n("cache_creation_input_tokens"),
        // Claude calls the cache-hit bucket `cache_read_input_tokens`; Codex
        // calls it `cached_input_tokens`. Accept whichever is present.
        cache_read_input_tokens: if usage_obj.get("cache_read_input_tokens").is_some() {
            n("cache_read_input_tokens")
        } else {
            n("cached_input_tokens")
        },
        // Cost is Claude-only; Codex omits it (left at 0.0).
        cost_usd: val
            .pointer("/total_cost_usd")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0),
    })
}

/// Extract a readable one-liner from a Claude Code stream-json event.
fn summarize_event(line: &str) -> Option<String> {
    let val: serde_json::Value = serde_json::from_str(line).ok()?;
    let kind = val.get("type")?.as_str()?;
    match kind {
        "assistant" => {
            // Extract text content from assistant message
            let content = val.pointer("/message/content")?.as_array()?;
            for block in content {
                if block.get("type")?.as_str()? == "tool_use" {
                    let name = short_tool(block.get("name")?.as_str()?);
                    return Some(match block.get("input").and_then(tool_detail) {
                        Some(d) => format!("-> {} {}", name, d),
                        None => format!("-> {}", name),
                    });
                }
                if block.get("type")?.as_str()? == "text" {
                    let text = block.get("text")?.as_str()?;
                    let first = text.trim().lines().next().unwrap_or("").trim();
                    if !first.is_empty() {
                        let truncated = if first.len() > 120 { format!("{}…", &first[..120]) } else { first.to_string() };
                        return Some(truncated);
                    }
                }
            }
            None
        }
        "tool_result" | "tool_use" => {
            let name = short_tool(val.get("name").and_then(|v| v.as_str()).unwrap_or("tool"));
            Some(match val.get("input").and_then(tool_detail) {
                Some(d) => format!("-> {} {}", name, d),
                None => format!("-> {}", name),
            })
        }
        "result" => {
            let subtype = val.get("subtype").and_then(|v| v.as_str()).unwrap_or("");
            Some(format!("Done ({})", subtype))
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// ACP mode: full protocol handshake
// ---------------------------------------------------------------------------

async fn start_acp_session(
    agent_binary: &str,
    cwd: &str,
    _model_name: &str,
    mcp_binary: &str,
    prompt: &str,
    id: u64,
    event_tx: mpsc::UnboundedSender<AgentEvent>,
    done_tx: mpsc::UnboundedSender<RuntimeCommand>,
) -> Result<oneshot::Sender<()>, String> {
    let mut child = tokio::process::Command::new(agent_binary)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("Failed to spawn {agent_binary}: {e}"))?;

    let stdin = child.stdin.take().ok_or("No stdin on child")?;
    let stdout = child.stdout.take().ok_or("No stdout on child")?;

    let client = ScryerClient::new(event_tx.clone());

    let (connection, io_future) =
        ClientSideConnection::new(client, stdin.compat_write(), stdout.compat(), |fut| {
            tokio::task::spawn_local(fut);
        });

    tokio::task::spawn_local(async move {
        if let Err(e) = io_future.await {
            eprintln!("ACP I/O error: {e}");
        }
    });

    let _init = connection
        .initialize(
            InitializeRequest::new(ProtocolVersion::V1).client_info(
                acp::Implementation::new("scryer", env!("CARGO_PKG_VERSION")).title("Scryer"),
            ),
        )
        .await
        .map_err(|e| format!("ACP initialize failed: {e}"))?;

    let mcp_server = McpServer::Stdio(McpServerStdio::new("scryer", mcp_binary));
    let session = connection
        .new_session(
            NewSessionRequest::new(PathBuf::from(cwd)).mcp_servers(vec![mcp_server]),
        )
        .await
        .map_err(|e| format!("ACP new_session failed: {e}"))?;

    let (cancel_tx, cancel_rx) = oneshot::channel::<()>();
    let prompt_text = prompt.to_string();
    let sid = session.session_id.clone();

    tokio::task::spawn_local(async move {
        let prompt_fut = connection.prompt(PromptRequest::new(
            sid.clone(),
            vec![prompt_text.into()],
        ));

        tokio::select! {
            result = prompt_fut => {
                match result {
                    Ok(resp) => {
                        let reason = match resp.stop_reason {
                            StopReason::EndTurn => "end_turn",
                            StopReason::MaxTokens => "max_tokens",
                            StopReason::Cancelled => "cancelled",
                            _ => "other",
                        };
                        let _ = event_tx.send(AgentEvent::Completed {
                            stop_reason: reason.to_string(),
                        });
                    }
                    Err(e) => {
                        let _ = event_tx.send(AgentEvent::Failed {
                            error: format!("{e}"),
                        });
                    }
                }
            }
            _ = cancel_rx => {
                let _ = connection.cancel(CancelNotification::new(sid)).await;
                let _ = event_tx.send(AgentEvent::Cancelled);
            }
        }
        let _ = done_tx.send(RuntimeCommand::Done { id });
    });

    Ok(cancel_tx)
}

// ---------------------------------------------------------------------------
// Process cleanup
// ---------------------------------------------------------------------------

/// Kill the agent subprocess and its entire process tree.
///
/// On Unix, the child was placed in its own process group via `setpgid(0, 0)`,
/// so `killpg` sends the signal to the whole group (child + grandchildren like
/// the MCP server subprocess).
///
/// On Windows, uses `taskkill /F /T /PID` to recursively kill the process tree.
///
/// Falls back to `child.kill()` if the PID is gone or platform-specific methods fail.
async fn kill_process_tree(child: &mut tokio::process::Child, pid: Option<u32>) {
    #[cfg(unix)]
    if let Some(pid) = pid {
        // SIGTERM the process group for graceful shutdown
        unsafe { libc::killpg(pid as libc::pid_t, libc::SIGTERM); }
        // Brief grace period, then force-kill
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        unsafe { libc::killpg(pid as libc::pid_t, libc::SIGKILL); }
        let _ = child.wait().await;
        return;
    }

    #[cfg(windows)]
    if let Some(pid) = pid {
        // taskkill /F /T kills the process and all its children
        let _ = tokio::process::Command::new("taskkill")
            .args(["/F", "/T", "/PID", &pid.to_string()])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .await;
        let _ = child.wait().await;
        return;
    }

    let _ = child.kill().await;
    let _ = child.wait().await;
}
