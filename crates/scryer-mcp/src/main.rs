mod cli;
mod helpers;
mod hook_client;
mod instructions;
mod server;
mod tools;
mod types;

// Validation lives in scryer-core so the deterministic extractor and any
// orchestrator share one definition of "valid". Re-exported here as
// `crate::validate` so the tool handlers' `use crate::validate;` stays put.
pub use scryer_core::validate;

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
        // Opt-in CI gate: exit 0 clean, 1 findings, 2 unusable.
        Some("check") => {
            let args: Vec<String> = std::env::args().skip(2).collect();
            return cli::run_check(&args);
        }
        _ => {}
    }

    let service = ScryerServer::new()
        .serve(rmcp::transport::io::stdio())
        .await
        .inspect_err(|e| eprintln!("MCP server error: {}", e))?;
    service.waiting().await?;
    Ok(())
}
