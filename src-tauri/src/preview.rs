use std::path::PathBuf;

use tauri::Emitter;

#[cfg(target_os = "macos")]
use crate::ensure_full_path;
use crate::mcp_setup::find_scryer_mcp;
use crate::state::{AcpState, PreviewServer, PreviewState};

/// Resolve (model, effort) for the agent kind we're about to launch from its
/// per-agent settings. An empty model means "use the agent CLI's own default".
pub(crate) fn config_for_launch(
    s: &scryer_core::SubagentSettings,
    launch: &scryer_acp::AgentLaunch,
) -> (String, String) {
    match launch {
        scryer_acp::AgentLaunch::Cli {
            kind: scryer_acp::AgentKind::ClaudeCode,
            ..
        } => (s.claude.model.clone(), s.claude.effort.clone()),
        scryer_acp::AgentLaunch::Cli {
            kind: scryer_acp::AgentKind::Codex,
            ..
        } => (s.codex.model.clone(), s.codex.effort.clone()),
        _ => (String::new(), "medium".to_string()),
    }
}

// --- deterministic preview server (Track B) ----------------------------------

/// The preview sidecar's sources, embedded at compile time and written to
/// `{project}/.scryer/preview/server/` before launch, so the spawned `node`
/// process always runs the version matching this binary. The sidecar has no
/// npm dependencies of its own — it resolves `vite` and `typescript` from the
/// target project's node_modules.
const PREVIEW_SIDECAR: &[(&str, &str)] = &[
    ("server.mjs", include_str!("../../preview/server.mjs")),
    ("plugin.mjs", include_str!("../../preview/plugin.mjs")),
    ("props.mjs", include_str!("../../preview/props.mjs")),
];

/// Start (or reuse) the shared preview dev server for a project and return its
/// base URL. Deterministic rendering: any visual component is then viewable at
/// `{url}/__preview?file=...&export=...` with no agent involvement.
#[tauri::command]
pub(crate) async fn ensure_preview_server(
    cwd: String,
    state: tauri::State<'_, PreviewState>,
) -> Result<String, String> {
    use tokio::io::AsyncBufReadExt;

    #[cfg(target_os = "macos")]
    ensure_full_path();

    let mut guard = state.0.lock().await;

    // Reuse a live server for the same project; replace anything else.
    if let Some(srv) = guard.as_mut() {
        let alive = matches!(srv.child.try_wait(), Ok(None));
        if alive && srv.cwd == cwd {
            return Ok(srv.url.clone());
        }
        let _ = srv.child.kill().await;
        *guard = None;
    }

    let dir = PathBuf::from(&cwd)
        .join(".scryer")
        .join("preview")
        .join("server");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    for (name, source) in PREVIEW_SIDECAR {
        std::fs::write(dir.join(name), source).map_err(|e| e.to_string())?;
    }

    let mut child = tokio::process::Command::new("node")
        .arg(dir.join("server.mjs"))
        .arg(&cwd)
        .arg("--exit-on-stdin-close")
        .current_dir(&cwd)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to launch preview server (is node installed?): {e}"))?;

    // Collect a stderr tail in the background so a startup failure can say
    // WHY the sidecar died, not just that it did. Keeps draining for the
    // server's lifetime so the pipe never fills.
    let stderr = child.stderr.take().ok_or("preview server has no stderr")?;
    let stderr_tail = tokio::spawn(async move {
        let mut lines = tokio::io::BufReader::new(stderr).lines();
        let mut tail = std::collections::VecDeque::with_capacity(20);
        while let Ok(Some(line)) = lines.next_line().await {
            if tail.len() >= 20 {
                tail.pop_front();
            }
            tail.push_back(line);
        }
        tail.into_iter().collect::<Vec<_>>().join("\n")
    });

    let stdout = child.stdout.take().ok_or("preview server has no stdout")?;
    let mut lines = tokio::io::BufReader::new(stdout).lines();
    let url = tokio::time::timeout(std::time::Duration::from_secs(60), async {
        while let Ok(Some(line)) = lines.next_line().await {
            if let Some(url) = line.strip_prefix("SCRYER_PREVIEW_URL=") {
                return Ok(url.trim().to_string());
            }
        }
        Err("preview server exited before reporting its URL".to_string())
    })
    .await
    .unwrap_or_else(|_| Err("preview server startup timed out".to_string()));
    let url = match url {
        Ok(url) => url,
        Err(e) => {
            let _ = child.kill().await;
            let tail = tokio::time::timeout(std::time::Duration::from_secs(2), stderr_tail)
                .await
                .ok()
                .and_then(|r| r.ok())
                .unwrap_or_default();
            return Err(if tail.is_empty() { e } else { format!("{e}\n{tail}") });
        }
    };

    // Keep draining stdout so the sidecar never blocks (or dies on EPIPE)
    // writing logs after we stop caring about them.
    tokio::spawn(async move { while let Ok(Some(_)) = lines.next_line().await {} });

    *guard = Some(PreviewServer {
        cwd,
        url: url.clone(),
        child,
    });
    Ok(url)
}

/// Find a node's anchored source file: the node-id source-map entry (data
/// shapes) or the first responsibility's source location.
fn node_source_file(model: &scryer_core::ScryModel, node_id: &str) -> String {
    let from_node = model
        .source_map
        .get(node_id)
        .and_then(|locs| locs.first())
        .map(|loc| loc.pattern.clone());
    from_node
        .or_else(|| {
            model
                .nodes
                .iter()
                .find(|n| n.id == node_id)
                .and_then(|node| {
                    node.responsibilities
                        .iter()
                        .find_map(|r| model.source_map.get(&r.id))
                        .and_then(|locs| locs.first())
                        .map(|loc| loc.pattern.clone())
                })
        })
        .unwrap_or_default()
}

/// Repair path for a failed deterministic render (B6). The preview server
/// renders components with synthesized placeholder props; when that comes out
/// empty or crashes, this launches an agent that authors realistic data —
/// primarily a shared, type-keyed fixture set (`.scryer/preview/fixtures/`
/// `shared.tsx` + `manifest.json`) reused across every component touching a
/// type, with a per-node override file as fallback. The preview server picks
/// the files up automatically — no build step.
#[tauri::command]
pub(crate) async fn start_preview_fixture_session(
    cwd: String,
    model_ref: String,
    node_id: String,
    render_status: String,
    render_error: Option<String>,
    app: tauri::AppHandle,
    state: tauri::State<'_, AcpState>,
) -> Result<String, String> {
    let mcp_binary = find_scryer_mcp()
        .ok_or("scryer-mcp binary not found")?;

    let settings = scryer_core::read_subagent_settings();
    let launch = scryer_acp::detect_available_agent_pref(&settings.agent)
        .ok_or("No AI agent found. Install Claude Code or Codex first.")?;

    let parsed_ref = scryer_core::ModelRef::parse(&model_ref)?;
    let model = scryer_core::read_model_at(&parsed_ref)?;
    let node = model
        .nodes
        .iter()
        .find(|n| n.id == node_id)
        .ok_or_else(|| format!("Node '{}' not found in model", node_id))?;
    let node_name = node.name.clone();

    let source_file = node_source_file(&model, &node_id);
    let source_lines = if !source_file.is_empty() {
        let abs = PathBuf::from(&cwd).join(&source_file);
        std::fs::read_to_string(&abs).unwrap_or_default()
    } else {
        String::new()
    };

    let prompt = scryer_acp::prompt::preview_fixture_prompt(
        &cwd,
        &node_id,
        &node_name,
        &source_file,
        &source_lines,
        &render_status,
        render_error.as_deref().unwrap_or(""),
    );
    let (model_name, effort) = config_for_launch(&settings, &launch);

    let runtime = {
        let mut rt = state.0.lock().unwrap();
        if rt.is_none() {
            *rt = Some(scryer_acp::AcpRuntime::new());
        }
        rt.clone().unwrap()
    };

    let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel();

    let handle = app.clone();
    tokio::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            let _ = handle.emit("agent-event", &event);
        }
    });

    let (agent_binary, mode) = match launch {
        scryer_acp::AgentLaunch::Cli { binary, kind } => {
            (binary, scryer_acp::runtime::LaunchMode::Cli { kind })
        }
        scryer_acp::AgentLaunch::Acp { binary } => {
            (binary, scryer_acp::runtime::LaunchMode::Acp)
        }
    };

    let allowed_tools = vec![
        "mcp__scryer__*".into(),
        "Write".into(),
        "Edit".into(),
        "Bash".into(),
    ];

    runtime
        .start_session(agent_binary, mode, cwd, model_name, effort, mcp_binary, prompt, allowed_tools, event_tx)
        .await
}

/// Generate visual variations of a component (B6). Launches an agent that
/// writes N self-contained variant modules under
/// `.scryer/preview/variations/{nodeId}/{i}.tsx`; the always-running preview
/// server serves each as a virtual entry instantly — no build step.
/// Ephemeral — does NOT update the model on completion.
#[tauri::command]
pub(crate) async fn start_visual_variation_session(
    cwd: String,
    model_ref: String,
    node_id: String,
    prompt: String,
    variation_count: Option<usize>,
    base_variation_idx: Option<u32>,
    app: tauri::AppHandle,
    state: tauri::State<'_, AcpState>,
) -> Result<String, String> {
    let variation_count = variation_count.unwrap_or(3).clamp(1, 5);

    let mcp_binary = find_scryer_mcp()
        .ok_or("scryer-mcp binary not found")?;

    let settings = scryer_core::read_subagent_settings();
    let launch = scryer_acp::detect_available_agent_pref(&settings.agent)
        .ok_or("No AI agent found. Install Claude Code or Codex first.")?;

    let parsed_ref = scryer_core::ModelRef::parse(&model_ref)?;
    let model = scryer_core::read_model_at(&parsed_ref)?;
    let node = model
        .nodes
        .iter()
        .find(|n| n.id == node_id)
        .ok_or_else(|| format!("Node '{}' not found in model", node_id))?;
    let node_name = node.name.clone();

    let source_file = node_source_file(&model, &node_id);
    let source_lines = if !source_file.is_empty() {
        let abs = PathBuf::from(&cwd).join(&source_file);
        std::fs::read_to_string(&abs).unwrap_or_default()
    } else {
        String::new()
    };

    // If iterating on a previous variation, that variant is the base; otherwise
    // a previously accepted variant (if any) is.
    let base_variant = if let Some(idx) = base_variation_idx {
        let path = PathBuf::from(&cwd)
            .join(".scryer/preview/variations")
            .join(&node_id)
            .join(format!("{idx}.tsx"));
        std::fs::read_to_string(&path).unwrap_or_default()
    } else {
        let path = PathBuf::from(&cwd)
            .join(".scryer/preview/accepted")
            .join(format!("{node_id}.tsx"));
        std::fs::read_to_string(&path).unwrap_or_default()
    };

    let agent_prompt = scryer_acp::prompt::visual_variation_prompt(
        &cwd, &node_id, &node_name, &source_file, &source_lines,
        &prompt, &base_variant, variation_count,
    );
    let (model_name, effort) = config_for_launch(&settings, &launch);

    let runtime = {
        let mut rt = state.0.lock().unwrap();
        if rt.is_none() {
            *rt = Some(scryer_acp::AcpRuntime::new());
        }
        rt.clone().unwrap()
    };

    let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel();

    let handle = app.clone();
    tokio::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            let _ = handle.emit("agent-event", &event);
        }
    });

    let (agent_binary, mode) = match launch {
        scryer_acp::AgentLaunch::Cli { binary, kind } => {
            (binary, scryer_acp::runtime::LaunchMode::Cli { kind })
        }
        scryer_acp::AgentLaunch::Acp { binary } => {
            (binary, scryer_acp::runtime::LaunchMode::Acp)
        }
    };

    let allowed_tools = vec![
        "mcp__scryer__*".into(),
        "Write".into(),
        "Edit".into(),
        "Bash".into(),
    ];

    runtime
        .start_session(agent_binary, mode, cwd, model_name, effort, mcp_binary, agent_prompt, allowed_tools, event_tx)
        .await
}

/// Accept a visual variation: persist the chosen variant module as the node's
/// appearance (`.scryer/preview/accepted/{nodeId}.tsx`), point the model's
/// preview metadata at it, and clean up the variation files. The accepted
/// variant is design intent — the node renders it (status `changed`) until the
/// real component code catches up.
#[tauri::command]
pub(crate) async fn accept_visual_variation(
    cwd: String,
    model_ref: String,
    node_id: String,
    variation_idx: u32,
) -> Result<(), String> {
    let vars_dir = PathBuf::from(&cwd).join(".scryer/preview/variations").join(&node_id);
    let variant = vars_dir.join(format!("{variation_idx}.tsx"));
    if !variant.exists() {
        return Err(format!("Variation {} not found", variation_idx));
    }

    let accepted_rel = format!(".scryer/preview/accepted/{node_id}.tsx");
    let accepted = PathBuf::from(&cwd).join(&accepted_rel);
    std::fs::create_dir_all(accepted.parent().unwrap()).map_err(|e| e.to_string())?;
    std::fs::copy(&variant, &accepted).map_err(|e| e.to_string())?;

    // Accepting a new look is a PLANNED change (the model wants the visual to
    // change): stamp it onto the planned model so it surfaces as pending work in
    // the plan diff, and the agent reconciles the code + `mark_implemented`s it
    // like any other plan item — the accepted fixture is the basis.
    let parsed_ref = scryer_core::ModelRef::parse(&model_ref)?;
    let _lock = scryer_core::lock_model(&parsed_ref).ok();
    scryer_core::ensure_planned_at(&parsed_ref)?;
    if let Ok(mut m) = scryer_core::read_planned_at(&parsed_ref) {
        if let Some(n) = m.nodes.iter_mut().find(|n| n.id == node_id) {
            n.appearance = Some(scryer_core::Appearance {
                status: Some(scryer_core::RenderState::Changed),
                dist_path: Some(accepted_rel),
                built_at: Some(scryer_core::drift::now_secs()),
                source_hash: None,
            });
        }
        let _ = scryer_core::write_planned_at(&parsed_ref, &m);
    }

    let _ = std::fs::remove_dir_all(&vars_dir);
    Ok(())
}

/// Discard visual variations: remove the node's variation files.
#[tauri::command]
pub(crate) async fn discard_visual_variations(
    cwd: String,
    node_id: String,
) -> Result<(), String> {
    let vars_dir = PathBuf::from(&cwd)
        .join(".scryer/preview/variations")
        .join(&node_id);
    if vars_dir.exists() {
        std::fs::remove_dir_all(&vars_dir).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub(crate) async fn cancel_agent_session(
    state: tauri::State<'_, AcpState>,
) -> Result<(), String> {
    // Set the durable cancel flag FIRST so orchestrators stop launching new
    // sessions even if the runtime currently has none (a wave gap) or a queued
    // session is about to start. Then best-effort kill any live sessions.
    state.1.store(true, std::sync::atomic::Ordering::SeqCst);
    let runtime = {
        let rt = state.0.lock().unwrap();
        rt.clone()
    };
    if let Some(runtime) = runtime {
        let _ = runtime.cancel().await;
    }
    Ok(())
}
