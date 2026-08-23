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
        scryer_acp::AgentLaunch::Acp {
            kind: scryer_acp::AcpKind::Copilot,
            ..
        } => (s.copilot.model.clone(), s.copilot.effort.clone()),
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
        .ok_or("No AI agent found. Install Claude Code, Codex or Copilot CLI first.")?;

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
        scryer_acp::AgentLaunch::Acp { binary, kind } => {
            (binary, scryer_acp::runtime::LaunchMode::Acp { kind })
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
        .ok_or("No AI agent found. Install Claude Code, Codex or Copilot CLI first.")?;

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
        scryer_acp::AgentLaunch::Acp { binary, kind } => {
            (binary, scryer_acp::runtime::LaunchMode::Acp { kind })
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The launch config comes from the launched agent's OWN settings slot,
    /// with the CLI's default (empty model, medium effort) for unknown agents.
    #[test]
    fn launch_config_resolves_the_agents_own_settings() {
        let mut s = scryer_core::SubagentSettings::default();
        s.claude.model = "opus".into();
        s.claude.effort = "high".into();
        s.codex.model = "gpt-x".into();
        s.copilot.model = "cp".into();

        let cli = |kind| scryer_acp::AgentLaunch::Cli { binary: "b".into(), kind };
        assert_eq!(
            config_for_launch(&s, &cli(scryer_acp::AgentKind::ClaudeCode)),
            ("opus".to_string(), "high".to_string())
        );
        assert_eq!(config_for_launch(&s, &cli(scryer_acp::AgentKind::Codex)).0, "gpt-x");
        assert_eq!(
            config_for_launch(
                &s,
                &scryer_acp::AgentLaunch::Acp { binary: "b".into(), kind: scryer_acp::AcpKind::Copilot }
            )
            .0,
            "cp"
        );
        assert_eq!(
            config_for_launch(
                &s,
                &scryer_acp::AgentLaunch::Acp { binary: "b".into(), kind: scryer_acp::AcpKind::Adapter }
            ),
            (String::new(), "medium".to_string()),
            "unknown agents fall back to the CLI's own default"
        );
    }

    /// A node's source file comes from its own source-map entry first, then
    /// its first anchored responsibility; empty when nothing anchors.
    #[test]
    fn node_source_resolves_own_anchor_then_first_responsibility() {
        let mut m = scryer_core::ScryModel::new();
        let mut node: scryer_core::Node = serde_json::from_value(
            serde_json::json!({ "id": "node-1", "kind": "symbol", "name": "Card" }),
        )
        .unwrap();
        node.responsibilities = vec![serde_json::from_value(
            serde_json::json!({ "id": "resp-1", "statement": "renders the card" }),
        )
        .unwrap()];
        m.nodes.push(node);

        assert_eq!(node_source_file(&m, "node-1"), "", "nothing anchored yet");
        m.source_map.insert(
            "resp-1".into(),
            vec![serde_json::from_value(serde_json::json!({ "pattern": "src/Card.tsx" })).unwrap()],
        );
        assert_eq!(node_source_file(&m, "node-1"), "src/Card.tsx", "claim anchor found");
        m.source_map.insert(
            "node-1".into(),
            vec![serde_json::from_value(serde_json::json!({ "pattern": "src/CardDecl.tsx" })).unwrap()],
        );
        assert_eq!(node_source_file(&m, "node-1"), "src/CardDecl.tsx", "own anchor wins");
    }

    /// Accepting a variation persists it as the node's appearance, stamps the
    /// plan with pending design intent, and clears the variation files.
    #[test]
    fn accepting_a_variation_persists_it_and_stamps_the_plan() {
        let dir = tempfile::tempdir().unwrap();
        let cwd = dir.path().to_string_lossy().to_string();
        let r = scryer_core::ModelRef::ProjectLocal(dir.path().to_path_buf());
        let mut m = scryer_core::ScryModel::new();
        m.nodes.push(
            serde_json::from_value(
                serde_json::json!({ "id": "node-1", "kind": "symbol", "name": "Card", "visual": true }),
            )
            .unwrap(),
        );
        scryer_core::write_model_at(&r, &m).unwrap();
        let vars = dir.path().join(".scryer/preview/variations/node-1");
        std::fs::create_dir_all(&vars).unwrap();
        std::fs::write(vars.join("2.tsx"), "export const V2 = () => null;").unwrap();

        tauri::async_runtime::block_on(accept_visual_variation(
            cwd,
            r.to_ref_string(),
            "node-1".into(),
            2,
        ))
        .unwrap();

        let accepted = dir.path().join(".scryer/preview/accepted/node-1.tsx");
        assert!(accepted.exists(), "the chosen variant is persisted");
        assert!(!vars.exists(), "variation files are cleared");
        let plan = scryer_core::read_planned_at(&r).unwrap();
        let appearance = plan.nodes[0].appearance.as_ref().expect("plan stamped");
        assert_eq!(appearance.status, Some(scryer_core::RenderState::Changed));
        assert_eq!(appearance.dist_path.as_deref(), Some(".scryer/preview/accepted/node-1.tsx"));

        // A missing variant is an error, not a silent accept.
        let err = tauri::async_runtime::block_on(accept_visual_variation(
            dir.path().to_string_lossy().to_string(),
            r.to_ref_string(),
            "node-1".into(),
            9,
        ))
        .unwrap_err();
        assert!(err.contains("not found"), "{err}");
    }

    /// Discarding removes the node's variation files — and is a quiet no-op
    /// when there are none.
    #[test]
    fn discarding_variations_removes_the_nodes_files() {
        let dir = tempfile::tempdir().unwrap();
        let cwd = dir.path().to_string_lossy().to_string();
        let vars = dir.path().join(".scryer/preview/variations/node-1");
        std::fs::create_dir_all(&vars).unwrap();
        std::fs::write(vars.join("0.tsx"), "x").unwrap();

        tauri::async_runtime::block_on(discard_visual_variations(cwd.clone(), "node-1".into()))
            .unwrap();
        assert!(!vars.exists());
        tauri::async_runtime::block_on(discard_visual_variations(cwd, "node-1".into()))
            .expect("no files is not an error");
    }
}
