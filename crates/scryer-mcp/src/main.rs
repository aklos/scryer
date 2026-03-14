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
        Some("check-drift") => return check_drift(),
        _ => {}
    }

    let service = ScryerServer::new()
        .serve(rmcp::transport::io::stdio())
        .await
        .inspect_err(|e| eprintln!("MCP server error: {}", e))?;
    service.waiting().await?;
    Ok(())
}

/// Claude Code PostToolUse hook handler. Reads hook input JSON from stdin,
/// checks if the edited file matches any source map pattern across all models,
/// and outputs a nudge for the AI if so.
fn check_drift() -> Result<(), Box<dyn std::error::Error>> {
    let input: serde_json::Value = serde_json::from_reader(std::io::stdin().lock())?;
    let file_path = input
        .pointer("/tool_input/file_path")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if file_path.is_empty() {
        return Ok(());
    }

    // Normalize to relative path from cwd
    let cwd = std::env::current_dir()?;
    let abs_path = std::path::Path::new(file_path);
    let rel_path = abs_path.strip_prefix(&cwd).unwrap_or(abs_path);
    let rel_str = rel_path.to_string_lossy();

    let models = scryer_core::list_models().unwrap_or_default();
    let mut matches: Vec<(String, String, String)> = Vec::new(); // (model, node_id, node_name)

    for model_name in &models {
        let model = match scryer_core::read_model(model_name) {
            Ok(m) => m,
            Err(_) => continue,
        };
        for (node_id, locations) in &model.source_map {
            for loc in locations {
                let pat = &loc.pattern;
                // Check glob match or prefix match
                let is_match = if pat.contains('*') || pat.contains('?') || pat.contains('[') {
                    glob::Pattern::new(pat)
                        .map(|g| g.matches(&rel_str))
                        .unwrap_or(false)
                } else {
                    rel_str.starts_with(pat) || rel_str == pat.as_str()
                };
                if is_match {
                    let node_name = model
                        .nodes
                        .iter()
                        .find(|n| n.id == *node_id)
                        .map(|n| n.data.name.clone())
                        .unwrap_or_default();
                    matches.push((model_name.clone(), node_id.clone(), node_name));
                    break; // one match per node is enough
                }
            }
        }
    }

    if matches.is_empty() {
        return Ok(());
    }

    // Group by model
    let mut by_model: std::collections::BTreeMap<&str, Vec<String>> = std::collections::BTreeMap::new();
    for (model, id, name) in &matches {
        by_model.entry(model).or_default().push(format!("{} [{}]", name, id));
    }
    let msg = by_model.iter()
        .map(|(model, nodes)| format!("scryer \u{2014} update if changed ({}): {}", model, nodes.join(", ")))
        .collect::<Vec<_>>()
        .join("\n");

    // Exit 0 with additionalContext — non-blocking nudge to the AI
    let output = serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "PostToolUse",
            "additionalContext": msg
        }
    });
    println!("{}", output);
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
            eprintln!("\nTo auto-approve scryer tools in Claude Code, add to .claude/settings.json:");
            eprintln!("  \"permissions\": {{ \"allow\": [\"mcp__scryer__*\"] }}");
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
