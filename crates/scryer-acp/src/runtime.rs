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
use crate::{AcpKind, AgentKind};

/// How the agent should be launched.
#[derive(Clone)]
pub enum LaunchMode {
    /// CLI print mode. Uses the user's subscription.
    Cli { kind: AgentKind },
    /// ACP subprocess.
    Acp { kind: AcpKind },
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
                        LaunchMode::Acp { kind } => start_acp_session(
                            &agent_binary, &kind, &cwd, &model_name, &effort, &mcp_binary,
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

/// The command line an ACP subprocess is spawned with. The protocol carries the
/// session — cwd, MCP servers, the prompt — but not which model or how hard to
/// think, so for a dialect that takes those as process flags they belong here.
fn acp_args(kind: &AcpKind, model_name: &str, effort: &str) -> Vec<String> {
    match kind {
        AcpKind::Copilot => {
            let mut args = vec!["--acp".to_string(), "--stdio".to_string()];
            if !model_name.is_empty() {
                args.push("--model".into());
                args.push(model_name.to_string());
            }
            if !effort.is_empty() {
                args.push("--effort".into());
                args.push(effort.to_string());
            }
            args
        }
        // Nothing is known about an adapter's flags, so it gets none: its own
        // config decides the model, exactly as it does outside scryer.
        AcpKind::Adapter => Vec::new(),
    }
}

/// Does this project's MCP config declare scryer? The question only arises for
/// an agent that won't take a server over the protocol, so the config on disk
/// is the only route in — checking it turns "the agent said it has no scryer
/// tool" into a message that names what to fix. Both files Copilot reads count.
fn project_declares_scryer_mcp(cwd: &str) -> bool {
    let root = std::path::Path::new(cwd);
    [root.join(".mcp.json"), root.join(".github").join("mcp.json")]
        .iter()
        .any(|p| {
            std::fs::read_to_string(p)
                .ok()
                .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
                .is_some_and(|v| v.pointer("/mcpServers/scryer").is_some())
        })
}

#[cfg(test)]
mod acp_args_tests {
    use super::*;

    /// Copilot serves ACP from its own binary, so the mode flags are mandatory
    /// and model/effort ride along as process flags. An empty model or effort
    /// means "whatever the CLI is already set to" and must not be passed as an
    /// empty string, which Copilot rejects as an unknown model.
    #[test]
    fn copilot_gets_its_mode_flags_and_only_the_settings_that_are_set() {
        assert_eq!(
            acp_args(&AcpKind::Copilot, "gpt-5.5", "high"),
            ["--acp", "--stdio", "--model", "gpt-5.5", "--effort", "high"]
        );
        assert_eq!(acp_args(&AcpKind::Copilot, "", ""), ["--acp", "--stdio"]);
        assert_eq!(
            acp_args(&AcpKind::Copilot, "", "medium"),
            ["--acp", "--stdio", "--effort", "medium"]
        );
    }

    /// An adapter binary is spawned bare: nothing is known about its flags, so
    /// inventing any would be a guess that fails at spawn.
    #[test]
    fn an_adapter_is_spawned_with_no_flags() {
        assert!(acp_args(&AcpKind::Adapter, "gpt-5.5", "high").is_empty());
    }
}

#[allow(clippy::too_many_arguments)]
async fn start_acp_session(
    agent_binary: &str,
    kind: &AcpKind,
    cwd: &str,
    model_name: &str,
    effort: &str,
    mcp_binary: &str,
    prompt: &str,
    id: u64,
    event_tx: mpsc::UnboundedSender<AgentEvent>,
    done_tx: mpsc::UnboundedSender<RuntimeCommand>,
) -> Result<oneshot::Sender<()>, String> {
    let mut child = tokio::process::Command::new(agent_binary)
        .args(acp_args(kind, model_name, effort))
        .current_dir(cwd)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("Failed to spawn {agent_binary}: {e}"))?;

    let stdin = child.stdin.take().ok_or("No stdin on child")?;
    let stdout = child.stdout.take().ok_or("No stdout on child")?;

    // ACP carries no channel for an agent's own diagnostics, and stdout is the
    // protocol — so when one of these subprocesses dies, everything it said
    // about why went to stderr. Tee it to the same place CLI sessions log to,
    // or a failure surfaces as nothing more useful than "server shut down".
    if let Some(stderr) = child.stderr.take() {
        let log_path = std::path::Path::new(cwd)
            .join(".scryer")
            .join("build-logs")
            .join(format!("session-{id}.err.log"));
        let _ = std::fs::create_dir_all(log_path.parent().unwrap());
        tokio::task::spawn_local(async move {
            use std::io::Write as _;
            use tokio::io::{AsyncBufReadExt, BufReader};
            let mut log = std::fs::File::create(&log_path).ok();
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if let Some(f) = log.as_mut() {
                    let _ = writeln!(f, "{line}");
                }
            }
        });
    }

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

    // Hand the session the scryer MCP server — except where the agent has told
    // us it can't take one this way. Copilot's ACP advertises `http` and `sse`
    // MCP transports and no stdio, and true to that it accepts a stdio entry
    // here without complaint and then ignores it, leaving the session with no
    // scryer tools at all. It does read the project's own `.mcp.json`, which is
    // the file scryer already writes, so there the config on disk is the route
    // in — and folder trust becomes a prerequisite for the launch path too, not
    // just for hooks.
    let mcp_servers = match kind {
        AcpKind::Copilot => {
            if !project_declares_scryer_mcp(cwd) {
                return Err(format!(
                    "Copilot can only reach scryer through this project's own MCP config, and \
                     {cwd}/.mcp.json doesn't declare it. Enable AI tool integration for this \
                     project (or run `scryer-mcp init`), and make sure you've trusted the folder \
                     in Copilot — it skips project MCP servers in untrusted folders."
                ));
            }
            Vec::new()
        }
        AcpKind::Adapter => vec![McpServer::Stdio(McpServerStdio::new("scryer", mcp_binary))],
    };
    let session = connection
        .new_session(NewSessionRequest::new(PathBuf::from(cwd)).mcp_servers(mcp_servers))
        .await
        .map_err(|e| format!("ACP new_session failed: {e}"))?;

    let (cancel_tx, cancel_rx) = oneshot::channel::<()>();
    let prompt_text = prompt.to_string();
    let sid = session.session_id.clone();
    let child_pid = child.id();

    tokio::task::spawn_local(async move {
        // `child` is owned by this task for the life of the session. It was
        // spawned `kill_on_drop`, so leaving it in the starting frame killed the
        // agent the moment the session was handed back — the whole ACP path
        // died between `session/new` and the first prompt.
        let mut child = child;
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
                // Ask politely over the protocol first, then make sure: an
                // agent that ignores the notification would otherwise outlive
                // the session it was cancelled out of.
                let _ = connection.cancel(CancelNotification::new(sid)).await;
                kill_process_tree(&mut child, child_pid).await;
                let _ = event_tx.send(AgentEvent::Cancelled);
            }
        }
        drop(child); // ends the agent process for a completed session too
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

#[cfg(test)]
mod session_tests {
    use super::*;

    fn feed() -> (mpsc::UnboundedSender<AgentEvent>, mpsc::UnboundedReceiver<AgentEvent>) {
        mpsc::unbounded_channel()
    }

    async fn start(
        rt: &AcpRuntime,
        binary: &str,
        prompt: &str,
        cwd: &str,
        tx: mpsc::UnboundedSender<AgentEvent>,
    ) -> Result<String, String> {
        rt.start_session(
            binary.into(),
            LaunchMode::Cli { kind: AgentKind::Other },
            cwd.into(),
            String::new(),
            String::new(),
            String::new(),
            prompt.into(),
            Vec::new(),
            tx,
        )
        .await
    }

    /// The caller's start_session resolves over the session's dedicated
    /// channel: a spawn failure comes back as the error, a live session as its
    /// id — and the session's own end arrives on the event feed.
    #[tokio::test]
    async fn a_caller_awaits_the_sessions_result_over_its_channel() {
        let tmp = tempfile::tempdir().unwrap();
        let cwd = tmp.path().to_string_lossy().into_owned();
        let rt = AcpRuntime::new();

        let (tx, _rx) = feed();
        let err = start(&rt, "/nonexistent/agent-binary", "", &cwd, tx).await.unwrap_err();
        assert!(err.contains("Failed to spawn"), "{err}");

        let (tx, mut rx) = feed();
        let id = start(&rt, "true", "", &cwd, tx).await.unwrap();
        assert!(id.starts_with("sync-"), "{id}");
        loop {
            match rx.recv().await.expect("event feed closed before the session ended") {
                AgentEvent::Completed { stop_reason } => {
                    assert_eq!(stop_reason, "end_turn");
                    break;
                }
                _ => continue,
            }
        }
    }

    /// One cancel stops EVERY active session — both report Cancelled, and a
    /// follow-up cancel finds nothing left to stop.
    #[tokio::test]
    async fn cancel_stops_every_active_session_at_once() {
        let tmp = tempfile::tempdir().unwrap();
        let cwd = tmp.path().to_string_lossy().into_owned();
        let rt = AcpRuntime::new();
        assert!(rt.cancel().await.is_err(), "nothing to stop yet");

        let (tx1, mut rx1) = feed();
        let (tx2, mut rx2) = feed();
        start(&rt, "sleep", "30", &cwd, tx1).await.unwrap();
        start(&rt, "sleep", "30", &cwd, tx2).await.unwrap();

        rt.cancel().await.unwrap();
        for rx in [&mut rx1, &mut rx2] {
            loop {
                match rx.recv().await.expect("event feed closed without a Cancelled") {
                    AgentEvent::Cancelled => break,
                    _ => continue,
                }
            }
        }
        assert!(rt.cancel().await.is_err(), "no session may survive the cancel");
    }

    /// Killing a session takes down the agent's whole process group — the
    /// grandchild dies with it — and a groupless child still dies via the
    /// plain-kill fallback.
    #[cfg(unix)]
    #[tokio::test]
    async fn kill_process_tree_takes_the_grandchild_and_falls_back_to_plain_kill() {
        let tmp = tempfile::tempdir().unwrap();
        let pidfile = tmp.path().join("grandchild.pid");
        let mut cmd = tokio::process::Command::new("sh");
        cmd.arg("-c")
            .arg(format!("sleep 30 & echo $! > '{}'; wait", pidfile.display()))
            .stdin(std::process::Stdio::null())
            .kill_on_drop(true);
        unsafe {
            cmd.pre_exec(|| {
                libc::setpgid(0, 0);
                Ok(())
            });
        }
        let mut child = cmd.spawn().unwrap();
        let pid = child.id();
        let grandchild: libc::pid_t = loop {
            if let Ok(s) = std::fs::read_to_string(&pidfile) {
                if let Ok(p) = s.trim().parse() {
                    break p;
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        };

        kill_process_tree(&mut child, pid).await;
        assert!(child.try_wait().unwrap().is_some(), "the agent process must be gone");
        let mut gone = false;
        for _ in 0..100 {
            if unsafe { libc::kill(grandchild, 0) } == -1 {
                gone = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        assert!(gone, "the grandchild must die with the group");

        // No known pid: the plain-kill fallback still ends the child.
        let mut child = tokio::process::Command::new("sleep")
            .arg("30")
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        kill_process_tree(&mut child, None).await;
        assert!(child.try_wait().unwrap().is_some(), "the fallback must kill the child");
    }
}
