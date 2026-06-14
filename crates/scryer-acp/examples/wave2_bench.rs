//! A1 measurement bench: run ONE Wave 2 container-modeling session headless
//! against a scratch project and report round-trips, wall time, and tokens.
//!
//! Usage:
//!   SCRYER_MCP=/path/to/scryer-mcp \
//!   cargo run -p scryer-acp --example wave2_bench -- <scratch-project> <container-dir> [--no-code]
//!
//! `--no-code` reconstructs the pre-A1 shape (index-only payload, the old
//! procedure wording) so before/after runs differ only in the evidence.
//! The scratch project should be a throwaway copy of a repo — the bench
//! overwrites its `.scryer/model.scry` with a minimal system+container model.

use std::collections::BTreeMap;

fn strip_code(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            map.remove("code");
            for v in map.values_mut() {
                strip_code(v);
            }
        }
        serde_json::Value::Array(arr) => {
            for v in arr.iter_mut() {
                strip_code(v);
            }
        }
        _ => {}
    }
}

/// Revert the A1 prompt edits so a `--no-code` run reproduces the pre-A1
/// session exactly: same payload, same instructions.
fn revert_prompt_to_pre_a1(prompt: &str) -> String {
    prompt
        .replace(
            "every symbol with its line range AND its source excerpt, and the dependency edges are supplied at the end, so you do NOT need to discover structure or read files.",
            "every symbol with its line range, and the dependency edges are supplied at the end, so you do NOT need to discover structure.",
        )
        .replace(
            "The evidence already embeds each symbol's source (`code`) — work from it directly. Open a source file ONLY when a truncated excerpt (`… +N lines`) leaves a symbol's accountability genuinely unclear; never re-read what is already inline.",
            "Read the actual source for each cluster — only enough to state responsibilities accurately.",
        )
        .replace(
            "- each symbol's `code` is its source: doc comment + signature + body. A trailing `… +N lines` marker means the definition continues in the file — everything else is the complete definition.\n",
            "",
        )
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let no_code = args.iter().any(|a| a == "--no-code");
    let pos: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();
    let [project, container_dir] = pos.as_slice() else {
        eprintln!("usage: wave2_bench <scratch-project> <container-dir> [--no-code]");
        std::process::exit(2);
    };
    let project_path = std::path::Path::new(project.as_str());
    let mcp_binary = std::env::var("SCRYER_MCP").unwrap_or_else(|_| "scryer-mcp".into());

    // Deterministic context + the dependency-graph cache the MCP commit reads.
    let ctx = scryer_extract::extract_context(project_path).expect("extraction");
    let edges = scryer_core::build_edges::BuildEdges {
        symbol_edges: ctx
            .symbol_edges
            .iter()
            .map(|e| scryer_core::build_edges::CachedEdge {
                src: e.src.clone(),
                dst: e.dst.clone(),
            })
            .collect(),
    };
    scryer_core::build_edges::write_build_edges(project_path, &edges).expect("edge cache");

    // Minimal model: a system and the one container under test.
    let mut model = scryer_core::ScryModel::new();
    let mut system: scryer_core::Node = serde_json::from_value(serde_json::json!({
        "id": "node-1", "kind": "system", "name": "bench-system"
    }))
    .unwrap();
    system.parent_id = None;
    let container: scryer_core::Node = serde_json::from_value(serde_json::json!({
        "id": "node-2", "kind": "container", "name": container_dir, "parentId": "node-1"
    }))
    .unwrap();
    model.nodes.push(system);
    model.nodes.push(container);
    let glob = if container_dir.is_empty() {
        "**/*".to_string()
    } else {
        format!("{container_dir}/**/*")
    };
    model.boundaries.insert(
        "node-2".into(),
        vec![serde_json::from_value(serde_json::json!({ "pattern": glob })).unwrap()],
    );
    let model_ref = scryer_core::ModelRef::ProjectLocal(project_path.to_path_buf());
    scryer_core::write_model_at(&model_ref, &model).expect("write model");

    // The evidence payload, optionally stripped back to the pre-A1 index.
    let scope = scryer_extract::slice_container(&ctx, container_dir);
    assert!(!scope.files.is_empty(), "container '{container_dir}' has no files");
    let compact = scryer_extract::compact_scope(&scope);
    let mut evidence = serde_json::to_value(&compact).unwrap();
    if no_code {
        strip_code(&mut evidence);
    }
    let evidence_json = serde_json::to_string(&evidence).unwrap();

    let mut prompt = scryer_acp::prompt::build_container_prompt(
        project,
        container_dir,
        "node-2",
        &evidence_json,
    );
    if no_code {
        prompt = revert_prompt_to_pre_a1(&prompt);
    }

    let settings = scryer_core::read_subagent_settings();
    let launch = scryer_acp::detect_available_agent_pref(&settings.agent).expect("agent on PATH");
    let (binary, kind, model_name, effort) = match launch {
        scryer_acp::AgentLaunch::Cli { binary, kind } => {
            let (m, e) = match kind {
                scryer_acp::AgentKind::ClaudeCode => {
                    (settings.claude.model.clone(), settings.claude.effort.clone())
                }
                _ => (settings.codex.model.clone(), settings.codex.effort.clone()),
            };
            (binary, kind, m, e)
        }
        scryer_acp::AgentLaunch::Acp { .. } => panic!("bench needs a CLI agent"),
    };

    eprintln!(
        "[bench] container '{container_dir}': payload {} bytes ({}), agent {binary}",
        evidence_json.len(),
        if no_code { "index only" } else { "evidence embedded" },
    );

    let runtime = scryer_acp::AcpRuntime::new();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let start = std::time::Instant::now();
    runtime
        .start_session(
            binary,
            scryer_acp::runtime::LaunchMode::Cli { kind },
            project.to_string(),
            model_name,
            effort,
            mcp_binary,
            prompt,
            vec!["mcp__scryer__*".into()],
            tx,
        )
        .await
        .expect("session start");

    let mut tool_calls: BTreeMap<String, String> = BTreeMap::new();
    let mut usage = scryer_acp::Usage::default();
    while let Some(ev) = rx.recv().await {
        match ev {
            scryer_acp::AgentEvent::ToolCall { id, name, status } => {
                eprintln!("[bench] {:>7.1}s tool {name} ({status})", start.elapsed().as_secs_f64());
                tool_calls.insert(id, name);
            }
            scryer_acp::AgentEvent::Usage { usage: u } => usage = u,
            scryer_acp::AgentEvent::Completed { .. } => break,
            scryer_acp::AgentEvent::Failed { error } => {
                eprintln!("[bench] FAILED: {error}");
                std::process::exit(1);
            }
            scryer_acp::AgentEvent::Cancelled => break,
            _ => {}
        }
    }
    let elapsed = start.elapsed().as_secs_f64();

    let after = scryer_core::read_model_at(&model_ref).expect("model after");
    let components = after
        .nodes
        .iter()
        .filter(|n| matches!(n.kind, scryer_core::Kind::Component))
        .count();
    let symbols = after
        .nodes
        .iter()
        .filter(|n| matches!(n.kind, scryer_core::Kind::Symbol))
        .count();

    let mut by_tool: BTreeMap<&str, usize> = BTreeMap::new();
    for name in tool_calls.values() {
        *by_tool.entry(name.as_str()).or_default() += 1;
    }
    println!("\n=== wave2_bench result ({}) ===", if no_code { "BEFORE: index only" } else { "AFTER: evidence embedded" });
    println!("container:   {container_dir}");
    println!("payload:     {} bytes", evidence_json.len());
    println!("wall clock:  {elapsed:.1}s");
    println!("tool calls:  {} total — {:?}", tool_calls.len(), by_tool);
    println!(
        "tokens:      in {} / out {} / cache-write {} / cache-read {}",
        usage.input_tokens, usage.output_tokens,
        usage.cache_creation_input_tokens, usage.cache_read_input_tokens,
    );
    println!("model nodes: {components} components, {symbols} symbols");
}
