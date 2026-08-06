//! Drive one real ACP session end to end and print what comes back.
//!
//! The ACP path is the fallback for agents scryer doesn't spawn in print mode —
//! Copilot CLI, and any `{name}-acp` adapter — so the things worth checking are
//! that the binary starts in ACP mode at all, that `session/new` carries the
//! scryer MCP server into the session, and that the agent can actually call it.
//! This asks the agent to do exactly that and reports the tool calls it makes.
//!
//! Usage:
//!   SCRYER_MCP=/path/to/scryer-mcp \
//!   cargo run -p scryer-acp --example acp_smoke -- <project> [agent-name]
//!
//! `agent-name` is an MCP client name (e.g. `copilot`); omitted, the configured
//! agent preference decides. The project needs a `.scryer/` model for the MCP
//! call to have anything to answer with.

use scryer_acp::{AcpKind, AgentEvent, AgentLaunch};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(project) = args.first() else {
        eprintln!("usage: acp_smoke <project> [agent-name]");
        std::process::exit(2);
    };
    let mcp_binary = std::env::var("SCRYER_MCP").unwrap_or_else(|_| "scryer-mcp".into());
    let settings = scryer_core::read_subagent_settings();

    let launch = match args.get(1) {
        Some(name) => scryer_acp::resolve_agent_binary(name),
        None => scryer_acp::detect_available_agent_pref(&settings.agent),
    }
    .expect("no agent resolved — is it on PATH?");

    let (binary, kind) = match launch {
        AgentLaunch::Acp { binary, kind } => (binary, kind),
        AgentLaunch::Cli { binary, .. } => {
            eprintln!("{binary} resolves to CLI print mode, not ACP — nothing to smoke here.");
            std::process::exit(2);
        }
    };
    let (model_name, effort) = match kind {
        AcpKind::Copilot => (settings.copilot.model.clone(), settings.copilot.effort.clone()),
        AcpKind::Adapter => (String::new(), String::new()),
    };
    eprintln!("[smoke] {binary} (model {model_name:?}, effort {effort:?}) on {project}");

    let runtime = scryer_acp::AcpRuntime::new();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let session = runtime
        .start_session(
            binary,
            scryer_acp::runtime::LaunchMode::Acp { kind },
            project.clone(),
            model_name,
            effort,
            mcp_binary,
            "Call the scryer MCP tool `get_health` and reply with ONLY the first line of \
             its output. Do not read any files."
                .to_string(),
            vec!["mcp__scryer__*".into()],
            tx,
        )
        .await
        .expect("session start");
    eprintln!("[smoke] session {session}");

    let mut saw_tool_call = false;
    while let Some(event) = rx.recv().await {
        match event {
            AgentEvent::Message { text } => println!("{text}"),
            AgentEvent::Thought { text } => eprintln!("[smoke] thought: {text}"),
            AgentEvent::ToolCall { name, status, .. } => {
                if name.contains("get_health") || name.contains("scryer") {
                    saw_tool_call = true;
                }
                eprintln!("[smoke] tool: {name} ({status})");
            }
            AgentEvent::Plan { content } => eprintln!("[smoke] plan: {content}"),
            AgentEvent::Activity => {}
            AgentEvent::Usage { usage } => eprintln!("[smoke] usage: {usage:?}"),
            AgentEvent::Completed { stop_reason } => {
                eprintln!("[smoke] completed: {stop_reason}");
                break;
            }
            AgentEvent::Failed { error } => {
                eprintln!("[smoke] FAILED: {error}");
                std::process::exit(1);
            }
            AgentEvent::Cancelled => {
                eprintln!("[smoke] cancelled");
                break;
            }
        }
    }
    eprintln!(
        "[smoke] scryer MCP reached from inside the session: {}",
        if saw_tool_call { "yes" } else { "NO" }
    );
}
