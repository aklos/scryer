mod helpers;
mod instructions;
mod server;
mod tools;
mod types;
mod validate;

use rmcp::ServiceExt;
use server::ScryerServer;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Handle subcommands
    match std::env::args().nth(1).as_deref() {
        Some("init") => return init_project(),

        _ => {}
    }

    let service = ScryerServer::new()
        .serve(rmcp::transport::io::stdio())
        .await
        .inspect_err(|e| eprintln!("MCP server error: {}", e))?;
    service.waiting().await?;
    Ok(())
}

/// Write project-scoped MCP config files in the current directory so that
/// Claude Code and/or Codex discover scryer-mcp when working in this project.
/// Only writes config for tools that are actually installed.
fn init_project() -> Result<(), Box<dyn std::error::Error>> {
    let binary_path = std::env::current_exe()?
        .canonicalize()?
        .to_string_lossy()
        .to_string();

    let cwd = std::env::current_dir()?;

    let has_claude = which("claude");
    let has_codex = which("codex");

    if !has_claude && !has_codex {
        eprintln!("Neither `claude` nor `codex` found in PATH.");
        eprintln!("Install Claude Code or OpenAI Codex first, then re-run `scryer-mcp init`.");
        std::process::exit(1);
    }

    let mut wrote_any = false;

    if has_claude {
        init_claude_code(&cwd, &binary_path)?;
        wrote_any = true;
    }

    if has_codex {
        init_codex(&cwd, &binary_path)?;
        wrote_any = true;
    }

    if wrote_any {
        let tools: Vec<&str> = [
            if has_claude { Some("Claude Code") } else { None },
            if has_codex { Some("Codex") } else { None },
        ].into_iter().flatten().collect();
        eprintln!("\nDone. {} will use scryer in this project.", tools.join(" and "));
        if has_claude {
            eprintln!("\nTo auto-approve scryer read tools in Claude Code, add to .claude/settings.local.json:");
            eprintln!("  \"permissions\": {{ \"allow\": [\"mcp__scryer__list_models\", \"mcp__scryer__get_model\", \"mcp__scryer__get_node\", \"mcp__scryer__get_rules\", \"mcp__scryer__get_changes\", \"mcp__scryer__get_structure\"] }}");
        }
    }

    Ok(())
}

fn which(name: &str) -> bool {
    // Check PATH for the given binary
    std::env::var_os("PATH")
        .map(|paths| {
            std::env::split_paths(&paths).any(|dir| {
                let candidate = dir.join(name);
                candidate.is_file() || dir.join(format!("{name}.exe")).is_file()
            })
        })
        .unwrap_or(false)
}

/// Write .mcp.json for Claude Code, merging with any existing config.
fn init_claude_code(
    cwd: &std::path::Path,
    binary_path: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let mcp_json_path = cwd.join(".mcp.json");
    let mut root: serde_json::Value = if mcp_json_path.exists() {
        let contents = std::fs::read_to_string(&mcp_json_path)?;
        serde_json::from_str(&contents).unwrap_or_else(|_| serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    if !root.get("mcpServers").is_some_and(|v| v.is_object()) {
        root["mcpServers"] = serde_json::json!({});
    }
    root["mcpServers"]["scryer"] = serde_json::json!({
        "type": "stdio",
        "command": binary_path,
        "args": [],
    });

    std::fs::write(&mcp_json_path, serde_json::to_string_pretty(&root)?)?;
    eprintln!("Wrote {}", mcp_json_path.display());
    Ok(())
}

/// Write .codex/config.toml for OpenAI Codex, merging with any existing config.
fn init_codex(
    cwd: &std::path::Path,
    binary_path: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let codex_dir = cwd.join(".codex");
    let config_toml_path = codex_dir.join("config.toml");

    let mut doc: toml_edit::DocumentMut = if config_toml_path.exists() {
        std::fs::read_to_string(&config_toml_path)?
            .parse()
            .unwrap_or_default()
    } else {
        toml_edit::DocumentMut::new()
    };

    if !doc.contains_table("mcp_servers") {
        doc["mcp_servers"] = toml_edit::Item::Table(toml_edit::Table::new());
    }

    let mut server = toml_edit::Table::new();
    server.insert("command", toml_edit::value(binary_path));
    server.insert("args", toml_edit::value(toml_edit::Array::new()));
    doc["mcp_servers"]["scryer"] = toml_edit::Item::Table(server);

    std::fs::create_dir_all(&codex_dir)?;
    std::fs::write(&config_toml_path, doc.to_string())?;
    eprintln!("Wrote {}", config_toml_path.display());
    Ok(())
}
