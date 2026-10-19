//! scryer-mcp: the stdio MCP server through which an agent reads the derived
//! C4 tree and runs the test loop, and — by subcommand — the CLI: `setup`,
//! `hook`, `status`, `statusline`, `lint`, `migrate` and `check`.

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

    let service = ScryerServer::new()
        .serve(rmcp::transport::io::stdio())
        .await
        .inspect_err(|e| eprintln!("MCP server error: {}", e))?;
    service.waiting().await?;
    Ok(())
}
