use std::path::PathBuf;

use agent_client_protocol::{
    self as acp, Agent as _, CancelNotification, ClientSideConnection,
    InitializeRequest, McpServer, McpServerStdio, NewSessionRequest, PromptRequest,
    ProtocolVersion, StopReason,
};
use tokio::sync::{mpsc, oneshot};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

use scryer_core::drift::DriftedNode;

use crate::client::ScryerClient;
use crate::events::AgentEvent;
use crate::prompt::sync_prompt;
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
        mcp_binary: String,
        drifted: Vec<DriftedNode>,
        structure_changed: bool,
        model_json: String,
        event_tx: mpsc::UnboundedSender<AgentEvent>,
        result_tx: oneshot::Sender<Result<String, String>>,
    },
    Cancel {
        result_tx: oneshot::Sender<Result<(), String>>,
    },
    /// Internal: session finished naturally.
    Done,
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
        mcp_binary: String,
        drifted: Vec<DriftedNode>,
        structure_changed: bool,
        model_json: String,
        event_tx: mpsc::UnboundedSender<AgentEvent>,
    ) -> Result<String, String> {
        let (result_tx, result_rx) = oneshot::channel();
        self.cmd_tx
            .send(RuntimeCommand::Start {
                agent_binary,
                mode,
                cwd,
                model_name,
                mcp_binary,
                drifted,
                structure_changed,
                model_json,
                event_tx,
                result_tx,
            })
            .map_err(|_| "Runtime is gone".to_string())?;
        result_rx.await.map_err(|_| "Runtime dropped".to_string())?
    }

    /// Cancel the active session.
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
        // Cancel sender for the active session (works for both modes)
        let mut cancel_tx: Option<oneshot::Sender<()>> = None;

        while let Some(cmd) = cmd_rx.recv().await {
            match cmd {
                RuntimeCommand::Start {
                    agent_binary,
                    mode,
                    cwd,
                    model_name,
                    mcp_binary,
                    drifted,
                    structure_changed,
                    model_json,
                    event_tx,
                    result_tx,
                } => {
                    if cancel_tx.is_some() {
                        let _ = result_tx.send(Err(
                            "A sync session is already running.".into(),
                        ));
                        continue;
                    }

                    let session_id = format!(
                        "sync-{}",
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_millis()
                    );

                    let result = match mode {
                        LaunchMode::Cli { kind } => start_cli_session(
                            &agent_binary, &kind, &cwd, &model_name, &mcp_binary,
                            &drifted, structure_changed, &model_json, event_tx, done_tx.clone(),
                        ),
                        LaunchMode::Acp => start_acp_session(
                            &agent_binary, &cwd, &model_name, &mcp_binary,
                            &drifted, structure_changed, &model_json, event_tx, done_tx.clone(),
                        ).await,
                    };

                    match result {
                        Ok(tx) => {
                            cancel_tx = Some(tx);
                            let _ = result_tx.send(Ok(session_id));
                        }
                        Err(e) => {
                            let _ = result_tx.send(Err(e));
                        }
                    }
                }
                RuntimeCommand::Cancel { result_tx } => {
                    if let Some(tx) = cancel_tx.take() {
                        let _ = tx.send(());
                        let _ = result_tx.send(Ok(()));
                    } else {
                        let _ = result_tx.send(Err("No active session".into()));
                    }
                }
                RuntimeCommand::Done => {
                    cancel_tx = None;
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
    mcp_binary: &str,
    drifted: &[DriftedNode],
    structure_changed: bool,
    model_json: &str,
    event_tx: mpsc::UnboundedSender<AgentEvent>,
    done_tx: mpsc::UnboundedSender<RuntimeCommand>,
) -> Result<oneshot::Sender<()>, String> {
    let prompt = sync_prompt(model_name, cwd, drifted, structure_changed, model_json);

    let mut cmd = tokio::process::Command::new(agent_binary);

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
                .arg("--output-format").arg("json")
                .arg("--mcp-config").arg(mcp_config.to_string())
                .arg("--allowed-tools").arg("mcp__scryer__*")
                .arg("--no-session-persistence")
                .arg(&prompt);
        }
        AgentKind::Codex => {
            // Codex uses `codex exec` with MCP pre-configured via .codex/config.toml
            cmd.arg("exec")
                .arg("--full-auto")
                .arg("--json")
                .arg("--ephemeral")
                .arg(&prompt);
        }
        AgentKind::Other => {
            // Best-effort: pass prompt as last arg
            cmd.arg(&prompt);
        }
    }

    cmd.current_dir(cwd)
        .stdin(std::process::Stdio::null())
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

    let child_pid = child.id();
    let (cancel_tx, cancel_rx) = oneshot::channel::<()>();

    tokio::task::spawn_local(async move {
        // Stream stdout line-by-line to detect tool call events
        let stdout = child.stdout.take();

        let monitor = async {
            if let Some(stdout) = stdout {
                use tokio::io::{AsyncBufReadExt, BufReader};
                let reader = BufReader::new(stdout);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    if let Some(tool) = extract_scryer_tool(&line) {
                        let _ = event_tx.send(AgentEvent::ToolCall {
                            id: String::new(),
                            name: tool,
                            status: "running".into(),
                        });
                    }
                }
            }
            // stdout closed — process has exited or is about to
            child.wait().await
        };

        tokio::select! {
            result = monitor => {
                match result {
                    Ok(status) if status.success() => {
                        let _ = event_tx.send(AgentEvent::Completed {
                            stop_reason: "end_turn".into(),
                        });
                    }
                    Ok(status) => {
                        let mut err_msg = format!("exit code {}", status.code().unwrap_or(-1));
                        if let Some(mut stderr) = child.stderr.take() {
                            let mut buf = String::new();
                            use tokio::io::AsyncReadExt;
                            if stderr.read_to_string(&mut buf).await.is_ok() && !buf.is_empty() {
                                err_msg = buf.lines().last().unwrap_or(&err_msg).to_string();
                            }
                        }
                        let _ = event_tx.send(AgentEvent::Failed { error: err_msg });
                    }
                    Err(e) => {
                        let _ = event_tx.send(AgentEvent::Failed {
                            error: format!("{e}"),
                        });
                    }
                }
            }
            _ = cancel_rx => {
                kill_process_tree(&mut child, child_pid).await;
                let _ = event_tx.send(AgentEvent::Cancelled);
            }
        }
        let _ = done_tx.send(RuntimeCommand::Done);
    });

    Ok(cancel_tx)
}

/// Extract a scryer MCP tool name from a JSON output line.
/// Looks for `"mcp__scryer__<tool>"` anywhere in the line.
fn extract_scryer_tool(line: &str) -> Option<String> {
    let marker = "\"mcp__scryer__";
    let idx = line.find(marker)?;
    let after = &line[idx + marker.len()..];
    let end = after.find('"').unwrap_or(after.len());
    let tool = &after[..end];
    if tool.is_empty() || tool.contains('*') {
        return None;
    }
    Some(tool.to_string())
}

// ---------------------------------------------------------------------------
// ACP mode: full protocol handshake
// ---------------------------------------------------------------------------

async fn start_acp_session(
    agent_binary: &str,
    cwd: &str,
    model_name: &str,
    mcp_binary: &str,
    drifted: &[DriftedNode],
    structure_changed: bool,
    model_json: &str,
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
    let prompt_text = sync_prompt(model_name, cwd, drifted, structure_changed, model_json);
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
        let _ = done_tx.send(RuntimeCommand::Done);
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
