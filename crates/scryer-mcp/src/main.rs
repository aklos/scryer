//! Provide command-line entry points for project setup, session hooks, status, and CI checks.
//!
//! Ingest test verdicts to gate claims as tested.
//!
//! Enforce the plan, sign-off, build, and verify change loop.
//!
//! Tracks each session's currently open change so plan writes auto-tag to the right task, scoped per project
//!
//! Expose model reading, editing, and rule tools to AI coding agents.
//!
//! Exposes an opt-in CI gate (`check`) that fails loud on validator warnings, broken anchors, and — when asked — drift or pending work
//!
//! Writes or merges .mcp.json, the shared config format Claude Code and Copilot CLI both read
//!
//! Bootstraps per-project agent config so installed CLIs (Claude Code, Copilot, Codex) discover the scryer-mcp server and, optionally, its statusline
//!
//! Carries the connect-time instructions that steer the model-first workflow loop and point at rules by slug
//!
//! Reports the model's loop-state (pending work, drift, test verdicts) as a human status line and as JSON
//!
//! Writes project-scoped MCP config for every installed agent CLI so it discovers the scryer-mcp binary
//!
//! Writes or merges .codex/config.toml to register the server for OpenAI Codex
//!
//! Assembles every tool category's router into the one advertised tool list, with schemas slimmed for a model reading them

mod cli;
mod derived;
mod helpers;
mod hook_client;
mod instructions;
mod server;
pub(crate) mod tools;
mod types;

use rmcp::ServiceExt;
use server::ScryerServer;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Handle subcommands
    match std::env::args().nth(1).as_deref() {
        // One switch for agent integration in this project: `on`, `off`, or
        // no argument to print what is installed, per tool, per file.
        Some("setup") => {
            let args: Vec<String> = std::env::args().skip(2).collect();
            return cli::run_setup(&args);
        }
        // Agent session hook: event JSON on stdin, hook JSON on stdout. Takes
        // `--copilot` because Copilot's tool names and reply shape differ and
        // the event can't be sniffed for them — the install writes the flag.
        // Silent no-op unless the Scryer app has this project open.
        Some("hook") => {
            let args: Vec<String> = std::env::args().skip(2).collect();
            return hook_client::run_hook_client(&args);
        }
        // Loop-state one-liner for humans, straight from disk (no app needed).
        Some("status") => {
            let args: Vec<String> = std::env::args().skip(2).collect();
            return cli::run_status(&args);
        }
        // Claude Code statusline command: session JSON on stdin, one line out.
        // Prints nothing when no model is found.
        Some("statusline") => return cli::run_statusline(),
        // EARS lint of every test description: exit 0 clean, 1 findings.
        Some("lint") => {
            let args: Vec<String> = std::env::args().skip(2).collect();
            return cli::run_lint(&args);
        }
        // One-shot move of an authored model into the code.
        Some("migrate") => {
            let args: Vec<String> = std::env::args().skip(2).collect();
            return cli::run_migrate(&args);
        }
        // Opt-in CI gate: exit 0 clean, 1 findings, 2 unusable.
        Some("check") => {
            let args: Vec<String> = std::env::args().skip(2).collect();
            return cli::run_check(&args);
        }
        _ => {}
    }

    // The server reads the derived tree, never the authored model, and the
    // verdict and probe store resolves claim ids against it only under this
    // flag. SEAM: step 6 deletes the authored model and the flag with it;
    // until then the server switches it on for its own process.
    std::env::set_var(scryer_core::derived_tree::DERIVED_TREE_ENV, "1");
    let service = ScryerServer::new()
        .serve(rmcp::transport::io::stdio())
        .await
        .inspect_err(|e| eprintln!("MCP server error: {}", e))?;
    service.waiting().await?;
    Ok(())
}
