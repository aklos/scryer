use tauri::Emitter;

use crate::mcp_setup::find_scryer_mcp;
use crate::preview::config_for_launch;
use crate::state::AcpState;

/// Map a Container node back to the project-relative directory it owns, so its
/// per-container code context can be sliced. Prefers the boundary glob the agent
/// set from `boundaryDir` (validated against the real container dirs), and falls
/// back to matching the node name against the context's container facts (which
/// covers the root container, whose empty dir carries no boundary glob).
fn derive_container_dir(
    node: &scryer_core::Node,
    model: &scryer_core::ScryModel,
    ctx: &scryer_extract::ProjectContext,
) -> Option<String> {
    if let Some(sources) = model.boundaries.get(&node.id) {
        if let Some(s) = sources.first() {
            let dir = s
                .pattern
                .trim_end_matches("/**/*")
                .trim_end_matches("/**")
                .trim_end_matches("/*")
                .to_string();
            if ctx.containers.iter().any(|c| c.dir == dir) {
                return Some(dir);
            }
        }
    }
    ctx.containers
        .iter()
        .find(|c| c.name == node.name)
        .map(|c| c.dir.clone())
}

/// Adapt extracted container facts to the model crate's seeding input.
fn seed_units(ctx: &scryer_extract::ProjectContext) -> Vec<scryer_core::seed::SeedUnit> {
    ctx.containers.iter().map(Into::into).collect()
}

/// Render token usage for a one-line debug log. Leads with the "fresh" tokens
/// (input + output + cache-write) and breaks out cache-read separately — a flat
/// sum is avoided on purpose: cache-read is re-read every turn and bills at 0.1×,
/// so it dwarfs the other buckets in raw count while costing almost nothing.
///
/// The dollar figure is shown ONLY as an "API-equiv" aside. It is `total_cost_usd`
/// straight from the CLI, which is the public pay-as-you-go list price for these
/// tokens — NOT what a subscription (Max/Pro) draws. On a subscription nothing is
/// billed per build; real usage is metered server-side as the session/weekly %
/// the account dashboard shows, and that number is not available here. The
/// list-price figure is still useful: it moves proportionally with the
/// subscription draw, so it's a relative gauge between builds — just not dollars.
fn fmt_usage(u: &scryer_acp::Usage) -> String {
    let fresh = u.input_tokens + u.output_tokens + u.cache_creation_input_tokens;
    let breakdown = format!(
        "in {} / out {} / cache-write {} / cache-read {}",
        u.input_tokens, u.output_tokens, u.cache_creation_input_tokens, u.cache_read_input_tokens,
    );
    if u.cost_usd > 0.0 {
        format!("{fresh} fresh tokens ({breakdown}) · ≈${:.4} API-equiv", u.cost_usd)
    } else {
        // Codex reports no cost — just the token counts.
        format!("{fresh} fresh tokens ({breakdown})")
    }
}

/// Append one timestamped line to `.scryer/build-logs/build.log` — the
/// orchestrator's own trail beside the per-session agent streams. The session
/// files record what each AGENT did; without this, the orchestrator's phase
/// transitions and its final verdict (validation-gate failures included) exist
/// only as transient UI events. Best-effort; epoch-second stamps (the workspace
/// carries no time-formatting dependency) — correlate with session-file mtimes.
fn orch_log(cwd: &str, line: &str) {
    use std::io::Write as _;
    let dir = std::path::Path::new(cwd).join(".scryer").join("build-logs");
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("build.log"))
    else {
        return;
    };
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let _ = writeln!(f, "[{secs}] {line}");
}

/// How a single agent session ended. `Failed` is carried as the `Err` arm of
/// [`run_wave`]'s result, so this only distinguishes the two non-error endings.
enum WaveOutcome {
    /// The session finished its turn — the orchestrator moves to the next wave.
    Completed,
    /// The user cancelled — the orchestrator must abort the whole build.
    Cancelled,
}

struct Wave2Job {
    id: String,
    name: String,
    evidence_json: String,
    work_units: usize,
    payload_bytes: usize,
}

/// One parallel drift-check session: the scope's fully rendered prompt plus its
/// size, which drives pool sizing and permit weighting exactly like a Wave 2 job.
struct DriftJob {
    node_id: String,
    node_name: String,
    prompt: String,
    payload_bytes: usize,
}

/// Derive agent concurrency from the actual per-session payloads rather than a
/// fixed pool. Small scopes are cheap enough to fan out; large prompts get fewer
/// concurrent sessions to avoid memory/subscription pressure. Shared by the
/// Wave 2 build fan-out and the parallel drift check.
///
/// Byte thresholds are calibrated for evidence-embedded payloads (each symbol
/// carries its source excerpt, ~10x the bare index): a typical scope lands at
/// 50–250 KB, and only a genuinely huge scope should sacrifice concurrency.
fn session_pool_size(payload_bytes: &[usize]) -> usize {
    if payload_bytes.is_empty() {
        return 1;
    }
    let average_bytes = payload_bytes.iter().sum::<usize>() / payload_bytes.len();
    let cpu_cap = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(2)
        .clamp(1, 4);
    let payload_cap = match average_bytes {
        0..=150_000 => 4,
        150_001..=450_000 => 3,
        _ => 2,
    };
    payload_bytes.len().min(cpu_cap).min(payload_cap).max(1)
}

fn session_permits(payload_bytes: usize, work_units: usize, pool: usize) -> u32 {
    let desired = match (payload_bytes, work_units) {
        (450_001.., _) | (_, 8_001..) => pool,
        (150_001.., _) | (_, 3_001..) => 2,
        _ => 1,
    };
    desired.min(pool).max(1) as u32
}

#[cfg(test)]
mod build_scheduling_tests {
    use super::{session_permits, session_pool_size};

    fn payloads(count: usize, bytes: usize) -> Vec<usize> {
        vec![bytes; count]
    }

    #[test]
    fn large_prompts_never_get_more_concurrency_than_small_prompts() {
        let small = payloads(8, 60_000);
        let medium = payloads(8, 250_000);
        let large = payloads(8, 900_000);
        assert!(session_pool_size(&large) <= session_pool_size(&small));
        assert_eq!(session_pool_size(&[]), 1);
        assert_eq!(session_pool_size(&payloads(1, 60_000)), 1);
        assert_eq!(session_permits(small[0], 0, 4), 1);
        assert_eq!(session_permits(medium[0], 0, 4), 2);
        assert_eq!(session_permits(large[0], 0, 4), 4);
    }

    /// Permits weigh BOTH dimensions: a small payload with many work units is
    /// throttled just like a big payload, and permits never exceed the pool.
    #[test]
    fn permits_weigh_work_units_alongside_payload_size() {
        assert_eq!(session_permits(60_000, 9_000, 4), 4, "heavy work units take the pool");
        assert_eq!(session_permits(60_000, 4_000, 4), 2, "medium work units take two");
        assert_eq!(session_permits(60_000, 100, 4), 1, "light stays at one");
        assert_eq!(session_permits(900_000, 9_000, 2), 2, "capped at the pool");
        assert_eq!(session_permits(0, 0, 1), 1, "never below one");
    }
}

#[cfg(test)]
mod build_helper_tests {
    use super::*;

    /// A monorepo with a root package and one `api/` sub-package, extracted for
    /// real so the context carries genuine container facts.
    fn ctx() -> (tempfile::TempDir, scryer_extract::ProjectContext) {
        let dir = tempfile::tempdir().unwrap();
        let write = |rel: &str, text: &str| {
            let path = dir.path().join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        write("package.json", r#"{"name":"app"}"#);
        write("src/main.ts", "export function main() { return 1; }");
        write("api/package.json", r#"{"name":"api-svc"}"#);
        write("api/src/server.ts", "export function serve() { return 2; }");
        let ctx = scryer_extract::extract_context(dir.path()).unwrap();
        (dir, ctx)
    }

    fn node(v: serde_json::Value) -> scryer_core::Node {
        serde_json::from_value(v).unwrap()
    }

    /// A container maps back to its owned directory through its boundary glob;
    /// without one, by matching its name against the extracted facts — which
    /// also covers the root container's empty dir.
    #[test]
    fn a_container_maps_back_to_the_directory_it_owns() {
        let (_dir, ctx) = ctx();
        let mut model = scryer_core::ScryModel::new();
        model.boundaries.insert(
            "node-2".into(),
            vec![serde_json::from_value(serde_json::json!({ "pattern": "api/**/*" })).unwrap()],
        );

        let via_boundary = node(
            serde_json::json!({ "id": "node-2", "kind": "container", "name": "Renamed API" }),
        );
        assert_eq!(derive_container_dir(&via_boundary, &model, &ctx).as_deref(), Some("api"));

        let via_name =
            node(serde_json::json!({ "id": "node-3", "kind": "container", "name": "api-svc" }));
        assert_eq!(derive_container_dir(&via_name, &model, &ctx).as_deref(), Some("api"));

        let root = node(serde_json::json!({ "id": "node-4", "kind": "container", "name": "app" }));
        assert_eq!(derive_container_dir(&root, &model, &ctx).as_deref(), Some(""));

        let unknown =
            node(serde_json::json!({ "id": "node-5", "kind": "container", "name": "ghost" }));
        assert_eq!(derive_container_dir(&unknown, &model, &ctx), None);
    }

    /// Extracted container facts adapt one-to-one into the seeding input.
    #[test]
    fn seed_units_mirror_the_extracted_container_facts() {
        let (_dir, ctx) = ctx();
        let units = seed_units(&ctx);
        assert_eq!(units.len(), ctx.containers.len());
        for (unit, fact) in units.iter().zip(&ctx.containers) {
            assert_eq!(unit.dir, fact.dir);
            assert_eq!(unit.name, fact.name);
            assert_eq!(unit.dep_dirs, fact.dep_dirs);
        }
        assert!(units.iter().any(|u| u.name == "api-svc" && u.dir == "api"));
    }

    /// Usage renders as one line: fresh tokens lead, cache-read is broken out,
    /// and the API-equivalent cost appears only when the agent reported one.
    #[test]
    fn usage_renders_a_one_line_summary_with_cost_only_when_reported() {
        let with_cost = scryer_acp::Usage {
            input_tokens: 100,
            output_tokens: 50,
            cache_creation_input_tokens: 10,
            cache_read_input_tokens: 9_000,
            cost_usd: 1.23,
        };
        let line = fmt_usage(&with_cost);
        assert!(line.starts_with("160 fresh tokens"), "{line}");
        assert!(line.contains("cache-read 9000"), "{line}");
        assert!(line.contains("≈$1.2300 API-equiv"), "{line}");

        let no_cost = scryer_acp::Usage { cost_usd: 0.0, ..with_cost };
        assert!(!fmt_usage(&no_cost).contains('$'), "Codex reports no cost");
    }

    /// Each orchestrator line lands timestamped in the build log, appended
    /// across calls.
    #[test]
    fn orchestrator_lines_append_timestamped_to_the_build_log() {
        let dir = tempfile::tempdir().unwrap();
        let cwd = dir.path().to_string_lossy().to_string();
        orch_log(&cwd, "build start");
        orch_log(&cwd, "✓ wave done");

        let log =
            std::fs::read_to_string(dir.path().join(".scryer/build-logs/build.log")).unwrap();
        let lines: Vec<&str> = log.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].starts_with('[') && lines[0].ends_with("build start"), "{}", lines[0]);
        assert!(lines[1].ends_with("✓ wave done"));
    }
}

/// Run one modeling agent session to completion. Forwards only the session's
/// *progress* events (messages, tool calls, activity) to the frontend and
/// translates its terminal event into a [`WaveOutcome`]/`Err`.
///
/// Crucially, per-session terminal events (`Completed`/`Cancelled`/`Failed`) are
/// NOT emitted to the frontend: a build is many sessions, and the frontend tears
/// the canvas down — re-enabling write-back — on the first terminal it sees. The
/// orchestrator owns the single terminal event for the whole build; until then
/// write-back must stay suppressed or later sessions' writes get clobbered.
///
/// Sessions can run concurrently (the runtime tracks one cancel handle per
/// session); Wave 2 starts several of these at once, bounded by the orchestrator.
///
/// Returns the session's outcome alongside the token usage it reported (CLI
/// agents report a turn total; ACP mode reports none, so usage stays zero). The
/// caller sums usage across the build's many sessions to log a grand total.
#[allow(clippy::too_many_arguments)]
async fn run_wave(
    runtime: &scryer_acp::AcpRuntime,
    agent_binary: &str,
    mode: &scryer_acp::runtime::LaunchMode,
    cwd: &str,
    model_name: &str,
    effort: &str,
    mcp_binary: &str,
    prompt: String,
    app: &tauri::AppHandle,
) -> Result<(WaveOutcome, scryer_acp::Usage), String> {
    let (tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel();
    runtime
        .start_session(
            agent_binary.to_string(),
            mode.clone(),
            cwd.to_string(),
            model_name.to_string(),
            effort.to_string(),
            mcp_binary.to_string(),
            prompt,
            vec!["mcp__scryer__*".into()],
            tx,
        )
        .await?;

    // The agent reports its turn total once (Claude's `result` event); keep the
    // last value seen rather than summing, so a cumulative reporter (Codex) can't
    // be double-counted within a single session.
    let mut usage = scryer_acp::Usage::default();
    while let Some(ev) = event_rx.recv().await {
        match &ev {
            scryer_acp::AgentEvent::Completed { .. } => return Ok((WaveOutcome::Completed, usage)),
            scryer_acp::AgentEvent::Cancelled => return Ok((WaveOutcome::Cancelled, usage)),
            scryer_acp::AgentEvent::Failed { error } => return Err(error.clone()),
            // Token totals are for the orchestrator's log, not the canvas — keep
            // them out of the forwarded stream.
            scryer_acp::AgentEvent::Usage { usage: u } => usage = *u,
            // Progress only — forward so the activity readout stays live.
            _ => {
                let _ = app.emit("agent-event", &ev);
            }
        }
    }
    // Stream closed without an explicit terminal — treat as a clean finish.
    Ok((WaveOutcome::Completed, usage))
}

/// Orchestrate a full auto-context model build with zero per-level clicking:
/// extract the deterministic codebase context, mint the system + container
/// skeleton mechanically from manifest facts (instant — no agent), then run
/// EVERY semantic session concurrently: the system-level pass (persons,
/// externals, responsibilities, names) beside one adaptive-pool session per
/// code-bearing container (each fed its compact sliced code context with
/// embedded source evidence). Wall clock approaches max(single session)
/// instead of wave1 + slowest round. Returns immediately; progress streams via
/// "agent-event" and nodes stream onto the canvas as the agent writes them.
#[tauri::command]
pub(crate) async fn start_model_build(
    cwd: String,
    app: tauri::AppHandle,
    state: tauri::State<'_, AcpState>,
) -> Result<String, String> {
    let project = std::path::Path::new(&cwd);

    // 1. Deterministic context — instant, in-memory, never persisted as a model.
    let (ctx, extraction) = scryer_extract::extract_context_with_stats(project)?;
    eprintln!(
        "[build] extraction: {} source files, {} parsed, {} cache hits",
        extraction.source_files, extraction.parsed_files, extraction.cache_hits,
    );
    // Cache the deterministic symbol dependency graph so the MCP
    // `fill_container` tool (a separate process) wires code-level links
    // from the same edges the agent saw, instead of having the agent author
    // them by hand and fight the same-level link validator. Best-effort.
    let build_edges = scryer_core::build_edges::BuildEdges {
        symbol_edges: ctx
            .symbol_edges
            .iter()
            .map(|e| scryer_core::build_edges::CachedEdge {
                src: e.src.clone(),
                dst: e.dst.clone(),
            })
            .collect(),
    };
    if let Err(e) = scryer_core::build_edges::write_build_edges(project, &build_edges) {
        eprintln!("[build] could not cache dependency graph: {e}");
    }

    // 2. Ensure a model exists so the intent tools have something to read; a
    //    blank one if absent (the agent's writes become the first real content).
    let model_ref = scryer_core::ModelRef::ProjectLocal(project.to_path_buf());
    {
        let _lock = scryer_core::lock_model(&model_ref)?;
        if scryer_core::read_model_at(&model_ref).is_err() {
            scryer_core::write_model_at(&model_ref, &scryer_core::ScryModel::new())?;
        }
    }

    // 3. Resolve the agent + its launch config.
    let mcp_binary = find_scryer_mcp().ok_or("scryer-mcp binary not found")?;
    let settings = scryer_core::read_subagent_settings();
    let launch = scryer_acp::detect_available_agent_pref(&settings.agent)
        .ok_or("No AI agent found. Install Claude Code, Codex or Copilot CLI first.")?;
    let (model_name, effort) = config_for_launch(&settings, &launch);
    let (agent_binary, mode) = match launch {
        scryer_acp::AgentLaunch::Cli { binary, kind } => {
            (binary, scryer_acp::runtime::LaunchMode::Cli { kind })
        }
        scryer_acp::AgentLaunch::Acp { binary, kind } => {
            (binary, scryer_acp::runtime::LaunchMode::Acp { kind })
        }
    };

    let runtime = {
        let mut rt = state.0.lock().unwrap();
        if rt.is_none() {
            *rt = Some(scryer_acp::AcpRuntime::new());
        }
        rt.clone().unwrap()
    };

    // Build-scoped cancel flag: clear it for this fresh build. The orchestrator
    // checks it at the wave boundary and every parallel task re-checks it after
    // acquiring its slot, so a "stop" set by `cancel_agent_session` stops new
    // sessions even in a no-session gap or just before a queued session starts.
    let cancel_flag = state.1.clone();
    cancel_flag.store(false, std::sync::atomic::Ordering::SeqCst);

    // 4. Background orchestrator: Wave 1, then a bounded-parallel Wave 2.
    tokio::spawn(async move {
        let emit_msg = |text: String| {
            orch_log(&cwd, &text);
            let _ = app.emit("agent-event", &scryer_acp::AgentEvent::Message { text });
        };
        let emit_fail = |error: String| {
            orch_log(&cwd, &format!("✗ build failed: {error}"));
            let _ = app.emit("agent-event", &scryer_acp::AgentEvent::Failed { error });
        };

        // Debug instrumentation: wall-clock for the whole build and a running
        // token total summed across every session (Wave 1 + each Wave 2
        // container). Token counts come from CLI agents (Claude Code / Codex);
        // ACP-mode agents report none, so they stay zero.
        let build_start = std::time::Instant::now();
        let mut total_usage = scryer_acp::Usage::default();
        eprintln!("[build] start: {cwd}");
        orch_log(&cwd, &format!("build start: {cwd}"));

        // No Wave 1 (A2): mint the structural skeleton mechanically — the
        // system + containers land on the canvas instantly, container coverage
        // holds by construction, and no session blocks any other.
        let minted = {
            let mint = || -> Result<(String, Vec<(String, String, String)>), String> {
                let _lock = scryer_core::lock_model(&model_ref)?;
                let mut model = scryer_core::read_model_at(&model_ref)?;
                let minted = scryer_core::seed::mint_initial_structure(
                    &mut model,
                    &ctx.project_name,
                    &seed_units(&ctx),
                );
                scryer_core::write_model_at(&model_ref, &model)?;
                Ok(minted)
            };
            match mint() {
                Ok(v) => v,
                Err(e) => {
                    emit_fail(format!("Could not seed the model structure: {e}"));
                    return;
                }
            }
        };
        let (system_id, containers) = minted;

        // What the system-level semantic session works against: the minted
        // skeleton with authoritative ids.
        let structure_json = serde_json::to_string_pretty(
            &ctx.containers
                .iter()
                .zip(&containers)
                .map(|(c, (id, name, dir))| {
                    serde_json::json!({
                        "id": id, "name": name, "dir": dir,
                        "technology": c.technology, "depDirs": c.dep_dirs,
                    })
                })
                .collect::<Vec<_>>(),
        )
        .unwrap_or_default();

        // Slice each codeful container's scope up front; skip source-less units
        // (e.g. a database image). Convert to the compact indexed wire format so
        // paths and synthetic symbol keys are not repeated throughout the prompt.
        let mut jobs: Vec<Wave2Job> = Vec::new();
        for (id, name, dir) in containers {
            let scope = scryer_extract::slice_container(&ctx, &dir);
            if scope.files.is_empty() {
                continue;
            }
            let evidence = scryer_extract::compact_scope(&scope);
            if let Ok(evidence_json) = serde_json::to_string(&evidence) {
                jobs.push(Wave2Job {
                    id,
                    name,
                    payload_bytes: evidence_json.len(),
                    work_units: evidence.work_units(),
                    evidence_json,
                });
            }
        }
        // Longest-processing-time-first reduces the tail when containers vary
        // substantially in size.
        jobs.sort_by(|a, b| b.work_units.cmp(&a.work_units));
        let wave2_pool =
            session_pool_size(&jobs.iter().map(|job| job.payload_bytes).collect::<Vec<_>>());
        eprintln!(
            "[build] Wave 2: {} job(s), adaptive pool {}, {} evidence bytes",
            jobs.len(),
            wave2_pool,
            jobs.iter().map(|job| job.payload_bytes).sum::<usize>(),
        );

        // Honor a stop pressed during setup (mint + slice every container): no
        // session is live here, so the runtime cancel was a no-op — this flag
        // is the only signal that survives the gap.
        if cancel_flag.load(std::sync::atomic::Ordering::SeqCst) {
            orch_log(&cwd, "build cancelled during setup");
            let _ = app.emit("agent-event", &scryer_acp::AgentEvent::Cancelled);
            return;
        }

        emit_msg("▶ Modeling the system and every container in parallel…".into());

        // Shared across the parallel tasks: the live active-node set (drives
        // the canvas rings — several can be amber at once), a cancel flag, and
        // a failure log we surface after the pool drains.
        let active: std::sync::Arc<tokio::sync::Mutex<std::collections::BTreeSet<String>>> =
            std::sync::Arc::new(tokio::sync::Mutex::new(std::collections::BTreeSet::new()));
        let failures: std::sync::Arc<tokio::sync::Mutex<Vec<String>>> =
            std::sync::Arc::new(tokio::sync::Mutex::new(Vec::new()));
        // Tokens summed across the parallel sessions (debug log).
        let wave2_usage: std::sync::Arc<tokio::sync::Mutex<scryer_acp::Usage>> =
            std::sync::Arc::new(tokio::sync::Mutex::new(scryer_acp::Usage::default()));
        // +1 permit: the system-level semantic session runs beside the
        // container pool without stealing a container slot.
        let sem = std::sync::Arc::new(tokio::sync::Semaphore::new(wave2_pool + 1));

        let mut handles = Vec::with_capacity(jobs.len() + 1);

        // The system-level semantic session — persons, externals, system &
        // container responsibilities, refined names, link labels — runs
        // concurrently with every container session.
        {
            let sem = sem.clone();
            let active = active.clone();
            let cancelled = cancel_flag.clone();
            let failures = failures.clone();
            let wave2_usage = wave2_usage.clone();
            let runtime = runtime.clone();
            let agent_binary = agent_binary.clone();
            let mode = mode.clone();
            let cwd = cwd.clone();
            let model_name = model_name.clone();
            let effort = effort.clone();
            let mcp_binary = mcp_binary.clone();
            let app = app.clone();
            let system_id = system_id.clone();
            handles.push(tokio::spawn(async move {
                let _permit = match sem.acquire().await {
                    Ok(p) => p,
                    Err(_) => return,
                };
                if cancelled.load(std::sync::atomic::Ordering::SeqCst) {
                    return;
                }
                {
                    let mut a = active.lock().await;
                    a.insert(system_id.clone());
                    let _ = app.emit(
                        "build-active-node",
                        a.iter().cloned().collect::<Vec<String>>(),
                    );
                }
                let s_start = std::time::Instant::now();
                let prompt = scryer_acp::prompt::enrich_system_prompt(
                    &cwd,
                    &system_id,
                    &structure_json,
                );
                let outcome = run_wave(
                    &runtime, &agent_binary, &mode, &cwd, &model_name, &effort, &mcp_binary,
                    prompt, &app,
                )
                .await;
                {
                    let mut a = active.lock().await;
                    a.remove(&system_id);
                    let _ = app.emit(
                        "build-active-node",
                        a.iter().cloned().collect::<Vec<String>>(),
                    );
                }
                match outcome {
                    Ok((WaveOutcome::Completed, usage)) => {
                        wave2_usage.lock().await.add(&usage);
                        let line = format!(
                            "system semantic pass: {:.1}s, {}",
                            s_start.elapsed().as_secs_f64(),
                            fmt_usage(&usage),
                        );
                        eprintln!("[build] {line}");
                        orch_log(&cwd, &line);
                    }
                    Ok((WaveOutcome::Cancelled, _)) => {
                        cancelled.store(true, std::sync::atomic::Ordering::SeqCst);
                    }
                    Err(e) => failures
                        .lock()
                        .await
                        .push(format!("System-level semantic pass failed: {e}")),
                }
            }));
        }
        for job in jobs {
            let permit_count = session_permits(job.payload_bytes, job.work_units, wave2_pool);
            let Wave2Job {
                id,
                name,
                evidence_json,
                work_units,
                payload_bytes,
            } = job;
            let sem = sem.clone();
            let active = active.clone();
            let cancelled = cancel_flag.clone();
            let failures = failures.clone();
            let wave2_usage = wave2_usage.clone();
            let runtime = runtime.clone();
            let agent_binary = agent_binary.clone();
            let mode = mode.clone();
            let cwd = cwd.clone();
            let model_name = model_name.clone();
            let effort = effort.clone();
            let mcp_binary = mcp_binary.clone();
            let app = app.clone();
            handles.push(tokio::spawn(async move {
                // Large prompts consume more pool capacity so one oversized
                // container cannot run beside several other expensive sessions.
                let _permit = match sem.acquire_many(permit_count).await {
                    Ok(p) => p,
                    Err(_) => return,
                };
                if cancelled.load(std::sync::atomic::Ordering::SeqCst) {
                    return;
                }
                {
                    let mut a = active.lock().await;
                    a.insert(id.clone());
                    let _ = app.emit(
                        "build-active-node",
                        a.iter().cloned().collect::<Vec<String>>(),
                    );
                }
                // Re-check right before launching: closes the window between the
                // post-acquire check and the session actually starting, so a
                // cancel during the brief setup above doesn't spawn a new agent.
                let c_start = std::time::Instant::now();
                let outcome = if cancelled.load(std::sync::atomic::Ordering::SeqCst) {
                    Ok((WaveOutcome::Cancelled, scryer_acp::Usage::default()))
                } else {
                    orch_log(&cwd, &format!("Modeling container: {name}…"));
                    let _ = app.emit(
                        "agent-event",
                        &scryer_acp::AgentEvent::Message {
                            text: format!("Modeling container: {name}…"),
                        },
                    );
                    let w2 = scryer_acp::prompt::build_container_prompt(
                        &cwd,
                        &name,
                        &id,
                        &evidence_json,
                    );
                    run_wave(
                        &runtime, &agent_binary, &mode, &cwd, &model_name, &effort, &mcp_binary,
                        w2, &app,
                    )
                    .await
                };
                {
                    let mut a = active.lock().await;
                    a.remove(&id);
                    let _ = app.emit(
                        "build-active-node",
                        a.iter().cloned().collect::<Vec<String>>(),
                    );
                }
                match outcome {
                    Ok((WaveOutcome::Completed, usage)) => {
                        wave2_usage.lock().await.add(&usage);
                        let line = format!(
                            "Wave 2 container '{name}': {:.1}s, {} (work {}, payload {} bytes)",
                            c_start.elapsed().as_secs_f64(),
                            fmt_usage(&usage),
                            work_units,
                            payload_bytes,
                        );
                        eprintln!("[build] {line}");
                        orch_log(&cwd, &line);
                    }
                    Ok((WaveOutcome::Cancelled, _)) => {
                        cancelled.store(true, std::sync::atomic::Ordering::SeqCst);
                    }
                    // One container failing shouldn't abort the rest of the build.
                    Err(e) => failures
                        .lock()
                        .await
                        .push(format!("Container '{name}' modeling failed: {e}")),
                }
            }));
        }

        for h in handles {
            let _ = h.await;
        }

        let failed_jobs = failures.lock().await.clone();
        if !failed_jobs.is_empty() {
            emit_fail(format!(
                "{} container modeling job(s) failed: {}",
                failed_jobs.len(),
                failed_jobs.join(" | ")
            ));
            return;
        }

        total_usage.add(&*wave2_usage.lock().await);

        if cancel_flag.load(std::sync::atomic::Ordering::SeqCst) {
            let line = format!(
                "cancelled after {:.1}s, {} (partial)",
                build_start.elapsed().as_secs_f64(),
                fmt_usage(&total_usage),
            );
            eprintln!("[build] {line}");
            orch_log(&cwd, &line);
            let _ = app.emit("build-active-node", Vec::<String>::new());
            let _ = app.emit("agent-event", &scryer_acp::AgentEvent::Cancelled);
            return;
        }

        // The assembled model lives in the PLANNED draft: container commits land
        // their subtrees there and the system-level session writes its enrichment
        // (persons, externals, system responsibilities, labels) there ONLY. The
        // committed layer has every container's components but not that
        // enrichment, so the planned draft is the full picture to validate,
        // repair, and ultimately fold back into the committed model below.
        let mut completed_model = match scryer_core::read_planned_at(&model_ref) {
            Ok(model) => model,
            Err(e) => {
                emit_fail(format!("Could not read the completed model: {e}"));
                return;
            }
        };
        // Validate the WORKING VIEW (committed's code anchors overlaid on the
        // draft), never the raw draft. Anchors route committed-only: the seed's
        // boundary globs and the fills' source maps live in `model.scry` and the
        // draft is seeded clean of them, so `validate_coverage` on the raw draft
        // flags every manifest directory as uncovered — unconditionally — and no
        // repair session can clear it (the agent's `validate_model` tool checks
        // the working view and rightly reports it clean). The fold below merges
        // this same view into committed, so the view is exactly what a passing
        // gate lets land.
        let validate_completed = |planned: &scryer_core::ScryModel| {
            let committed = scryer_core::read_model_at(&model_ref).unwrap_or_default();
            let view = scryer_core::working_view(&committed, planned);
            let mut warnings = scryer_core::validate::validate(&view);
            warnings.extend(scryer_core::validate::validate_coverage(
                &view,
                std::path::Path::new(&cwd),
            ));
            warnings
        };
        // A code-level "appears disconnected" warning is NOT worth a repair
        // session: the deterministic dependency graph is legitimately sparse
        // (data types, UI leaves, entry points connect to nothing), so firing an
        // agent to invent links between them is pure cost for no signal. The
        // validator still reports them for the canvas/SyncBar; the BUILD just
        // doesn't repair them. Everything else (bad parents, real cross-level
        // link violations, coverage gaps) still gates.
        let is_sparse_code_disconnect = |w: &str| {
            w.contains("disconnected") && (w.contains("(symbol)") || w.contains("(component)"))
        };
        let all_warnings = validate_completed(&completed_model);
        let deferred = all_warnings.iter().filter(|w| is_sparse_code_disconnect(w)).count();
        if deferred > 0 {
            emit_msg(format!(
                "ℹ {deferred} code-level node(s) have no modeled relationship — left as-is (not a build error)."
            ));
        }
        let mut warnings: Vec<String> = all_warnings
            .into_iter()
            .filter(|w| !is_sparse_code_disconnect(w))
            .collect();
        if !warnings.is_empty() {
            emit_msg(format!(
                "▶ Repairing {} model validation issue(s)…",
                warnings.len()
            ));
            orch_log(&cwd, &format!("validation warnings: {}", warnings.join(" | ")));
            let warnings_json =
                serde_json::to_string(&warnings).unwrap_or_else(|_| "[]".to_string());
            let repair_prompt = scryer_acp::prompt::repair_model_prompt(&cwd, &warnings_json);
            match run_wave(
                &runtime,
                &agent_binary,
                &mode,
                &cwd,
                &model_name,
                &effort,
                &mcp_binary,
                repair_prompt,
                &app,
            )
            .await
            {
                Ok((WaveOutcome::Completed, usage)) => total_usage.add(&usage),
                Ok((WaveOutcome::Cancelled, _)) => {
                    orch_log(&cwd, "build cancelled during validation repair");
                    let _ = app.emit("agent-event", &scryer_acp::AgentEvent::Cancelled);
                    return;
                }
                Err(e) => {
                    emit_fail(format!("Model validation repair failed: {e}"));
                    return;
                }
            }

            // The repair session also authors into the planned draft (its
            // add_links/update_nodes write planned only), so re-read it there.
            completed_model = match scryer_core::read_planned_at(&model_ref) {
                Ok(model) => model,
                Err(e) => {
                    emit_fail(format!("Could not read the repaired model: {e}"));
                    return;
                }
            };
            warnings = validate_completed(&completed_model)
                .into_iter()
                .filter(|w| !is_sparse_code_disconnect(w))
                .collect();
            if !warnings.is_empty() {
                emit_fail(format!(
                    "Model remains invalid after repair: {}",
                    warnings.join(" | ")
                ));
                return;
            }
        }
        // Fold the assembled draft into the committed model: a from-code build is
        // extracted truth, so model and planned end equal and no spurious pending
        // plan remains. This is what lands the system-level enrichment (and any
        // repair-session edits, which were authored into the planned draft) into
        // the committed model the wiki reads. The fold MERGES rather than
        // overwrites: the draft is seeded clean of committed's single-home
        // anchors, so a verbatim write would wipe the seed's boundary globs.
        let completed_model = match scryer_core::fold_built_model(&model_ref, &completed_model) {
            Ok(folded) => folded,
            Err(e) => {
                emit_fail(format!(
                    "Could not fold the completed model into the committed layer: {e}"
                ));
                return;
            }
        };
        if let Err(e) = scryer_core::save_baseline_at(&model_ref, &completed_model) {
            emit_msg(format!("⚠ Could not save the final model baseline: {e}"));
        }

        // Anchor the reconcile point so the first drift check only examines
        // changes made AFTER the build, not the whole repo — and fingerprint
        // every anchor so the check is content-addressed, not git-dependent.
        let _ = scryer_core::write_sync_state(
            &model_ref,
            &scryer_core::drift::SyncState::anchored_now(
                scryer_core::drift::head_commit(std::path::Path::new(&cwd)),
            ),
        );
        if let Err(e) = scryer_extract::anchors::write_baseline(&model_ref) {
            emit_msg(format!("⚠ Could not fingerprint anchors: {e}"));
        }

        let elapsed = build_start.elapsed().as_secs_f64();
        let line = format!("complete: {:.1}s total, {}", elapsed, fmt_usage(&total_usage));
        eprintln!("[build] {line}");
        orch_log(&cwd, &line);

        let _ = app.emit("build-active-node", Vec::<String>::new());
        let fresh = total_usage.input_tokens
            + total_usage.output_tokens
            + total_usage.cache_creation_input_tokens;
        emit_msg(format!(
            "✓ Model build complete — {elapsed:.0}s, {fresh} tokens.",
        ));
        let _ = app.emit(
            "agent-event",
            &scryer_acp::AgentEvent::Completed {
                stop_reason: "build_complete".into(),
            },
        );
    });

    Ok("started".to_string())
}

/// Run a semantic drift check: find the boundary-owning nodes whose code changed
/// since the last reconcile, then for each, an agent compares what the code DOES
/// against the node's responsibilities and records findings via `flag_drift`
/// (undescribed behaviour → vagrant responsibilities; stale claims/nodes → the
/// `stale` flag on the working draft).
/// Returns immediately; progress + findings stream via "agent-event". The
/// reconcile anchor advances when the check finishes so the next run sees only
/// newer changes.
#[tauri::command]
pub(crate) async fn start_drift_check(
    cwd: String,
    app: tauri::AppHandle,
    state: tauri::State<'_, AcpState>,
) -> Result<String, String> {
    let project = std::path::Path::new(&cwd);
    let model_ref = scryer_core::ModelRef::ProjectLocal(project.to_path_buf());
    let model = scryer_core::read_model_at(&model_ref)
        .map_err(|e| format!("No model to check for drift: {e}"))?;
    let sync = scryer_core::read_sync_state(&model_ref);
    let scopes = scryer_core::drift::drifted_scopes(&model, project, &sync);

    let mcp_binary = find_scryer_mcp().ok_or("scryer-mcp binary not found")?;
    let settings = scryer_core::read_subagent_settings();
    let launch = scryer_acp::detect_available_agent_pref(&settings.agent)
        .ok_or("No AI agent found. Install Claude Code, Codex or Copilot CLI first.")?;
    let (model_name, effort) = config_for_launch(&settings, &launch);
    let (agent_binary, mode) = match launch {
        scryer_acp::AgentLaunch::Cli { binary, kind } => {
            (binary, scryer_acp::runtime::LaunchMode::Cli { kind })
        }
        scryer_acp::AgentLaunch::Acp { binary, kind } => {
            (binary, scryer_acp::runtime::LaunchMode::Acp { kind })
        }
    };
    let runtime = {
        let mut rt = state.0.lock().unwrap();
        if rt.is_none() {
            *rt = Some(scryer_acp::AcpRuntime::new());
        }
        rt.clone().unwrap()
    };

    let (ctx, extraction) = scryer_extract::extract_context_with_stats(project)?;
    eprintln!(
        "[drift] extraction: {} source files, {} parsed, {} cache hits",
        extraction.source_files, extraction.parsed_files, extraction.cache_hits,
    );

    let cancel_flag = state.1.clone();
    cancel_flag.store(false, std::sync::atomic::Ordering::SeqCst);

    tokio::spawn(async move {
        let emit_msg = |text: String| {
            orch_log(&cwd, &text);
            let _ = app.emit("agent-event", &scryer_acp::AgentEvent::Message { text });
        };

        let write_anchor = || {
            let _ = scryer_core::write_sync_state(
                &model_ref,
                &scryer_core::drift::SyncState::anchored_now(
                scryer_core::drift::head_commit(std::path::Path::new(&cwd)),
            ),
            );
            let _ = scryer_extract::anchors::write_baseline(&model_ref);
        };

        if scopes.is_empty() {
            emit_msg("✓ Model is in sync with the code — nothing changed.".into());
            write_anchor();
            let _ = app.emit(
                "agent-event",
                &scryer_acp::AgentEvent::Completed {
                    stop_reason: "in_sync".into(),
                },
            );
            return;
        }

        orch_log(&cwd, &format!("drift check start: {cwd}"));
        emit_msg(format!(
            "▶ Checking {} changed scope(s) for drift…",
            scopes.len()
        ));

        // Render every scope's prompt up front (in-memory, instant), then run
        // the checks through the same bounded-parallel pool the build fan-out
        // uses. Scopes are independent, and `flag_drift` does its
        // read-modify-write under the cross-process model file lock — the same
        // serialization parallel `fill_container` commits already rely on — so
        // sessions can overlap freely.
        let drift_start = std::time::Instant::now();
        let mut jobs: Vec<DriftJob> = Vec::new();
        for scope in &scopes {
            let dir = model
                .nodes
                .iter()
                .find(|n| n.id == scope.node_id)
                .and_then(|n| derive_container_dir(n, &model, &ctx))
                .unwrap_or_default();
            let slice = scryer_extract::slice_container(&ctx, &dir);
            // Compact index for the whole container, with source evidence
            // embedded ONLY for the changed files — the agent judges those
            // inline instead of spending round-trips re-reading them.
            let changed_set: std::collections::BTreeSet<String> =
                scope.changed_files.iter().cloned().collect();
            let evidence = scryer_extract::compact_scope_with_evidence(&slice, &changed_set);
            let scope_json = serde_json::to_string(&evidence).unwrap_or_default();
            let changed_json = serde_json::to_string(&scope.changed_files).unwrap_or_default();
            // Feed only this node's subtree (its claims), not the whole model.
            let subtree_json =
                scryer_acp::prompt::serialize_subtree_for_prompt(&model, &scope.node_id);
            let prompt = scryer_acp::prompt::drift_check_prompt(
                &cwd,
                &scope.node_name,
                &scope.node_id,
                &subtree_json,
                &scope_json,
                &changed_json,
            );
            jobs.push(DriftJob {
                node_id: scope.node_id.clone(),
                node_name: scope.node_name.clone(),
                payload_bytes: prompt.len(),
                prompt,
            });
        }
        // Longest-processing-time-first reduces the tail when scopes vary in size.
        jobs.sort_by(|a, b| b.payload_bytes.cmp(&a.payload_bytes));
        let pool =
            session_pool_size(&jobs.iter().map(|job| job.payload_bytes).collect::<Vec<_>>());
        eprintln!(
            "[drift] {} scope(s), adaptive pool {}, {} payload bytes",
            jobs.len(),
            pool,
            jobs.iter().map(|job| job.payload_bytes).sum::<usize>(),
        );

        let active: std::sync::Arc<tokio::sync::Mutex<std::collections::BTreeSet<String>>> =
            std::sync::Arc::new(tokio::sync::Mutex::new(std::collections::BTreeSet::new()));
        let failures: std::sync::Arc<tokio::sync::Mutex<Vec<String>>> =
            std::sync::Arc::new(tokio::sync::Mutex::new(Vec::new()));
        // The first failure stops LAUNCHING further sessions — the cause is
        // usually global (auth, network), so more sessions would just repeat the
        // same error — while sessions already in flight run to completion and
        // keep the findings they flag.
        let failed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let sem = std::sync::Arc::new(tokio::sync::Semaphore::new(pool));

        let mut handles = Vec::with_capacity(jobs.len());
        for job in jobs {
            let permit_count = session_permits(job.payload_bytes, 0, pool);
            let DriftJob {
                node_id,
                node_name,
                prompt,
                ..
            } = job;
            let sem = sem.clone();
            let active = active.clone();
            let cancelled = cancel_flag.clone();
            let failures = failures.clone();
            let failed = failed.clone();
            let runtime = runtime.clone();
            let agent_binary = agent_binary.clone();
            let mode = mode.clone();
            let cwd = cwd.clone();
            let model_name = model_name.clone();
            let effort = effort.clone();
            let mcp_binary = mcp_binary.clone();
            let app = app.clone();
            handles.push(tokio::spawn(async move {
                let _permit = match sem.acquire_many(permit_count).await {
                    Ok(p) => p,
                    Err(_) => return,
                };
                // Honor a stop (or an earlier failure) that landed while this
                // scope was queued — the gap where no session of ours is live,
                // so the runtime cancel alone can't reach it.
                if cancelled.load(std::sync::atomic::Ordering::SeqCst)
                    || failed.load(std::sync::atomic::Ordering::SeqCst)
                {
                    return;
                }
                {
                    let mut a = active.lock().await;
                    a.insert(node_id.clone());
                    let _ = app.emit(
                        "build-active-node",
                        a.iter().cloned().collect::<Vec<String>>(),
                    );
                }
                orch_log(&cwd, &format!("▶ Drift check: {node_name}…"));
                let _ = app.emit(
                    "agent-event",
                    &scryer_acp::AgentEvent::Message {
                        text: format!("▶ Drift check: {node_name}…"),
                    },
                );
                let d_start = std::time::Instant::now();
                let outcome = run_wave(
                    &runtime, &agent_binary, &mode, &cwd, &model_name, &effort, &mcp_binary,
                    prompt, &app,
                )
                .await;
                {
                    let mut a = active.lock().await;
                    a.remove(&node_id);
                    let _ = app.emit(
                        "build-active-node",
                        a.iter().cloned().collect::<Vec<String>>(),
                    );
                }
                match outcome {
                    Ok((WaveOutcome::Completed, usage)) => {
                        let line = format!(
                            "drift scope '{node_name}': {:.1}s, {}",
                            d_start.elapsed().as_secs_f64(),
                            fmt_usage(&usage),
                        );
                        eprintln!("[drift] {line}");
                        orch_log(&cwd, &line);
                    }
                    Ok((WaveOutcome::Cancelled, _)) => {
                        cancelled.store(true, std::sync::atomic::Ordering::SeqCst);
                    }
                    Err(e) => {
                        failed.store(true, std::sync::atomic::Ordering::SeqCst);
                        failures
                            .lock()
                            .await
                            .push(format!("Drift check for '{node_name}' failed: {e}"));
                    }
                }
            }));
        }
        for h in handles {
            let _ = h.await;
        }

        if cancel_flag.load(std::sync::atomic::Ordering::SeqCst) {
            orch_log(&cwd, "drift check cancelled");
            let _ = app.emit("build-active-node", Vec::<String>::new());
            let _ = app.emit("agent-event", &scryer_acp::AgentEvent::Cancelled);
            return;
        }
        // A failed session means its scope's drift was never examined. Surface
        // the failure(s) and bail WITHOUT advancing the anchor, so the model's
        // drift state is left untouched and a re-run re-checks every changed
        // scope (findings already flagged by the scopes that completed persist).
        let failed_scopes = failures.lock().await.clone();
        if !failed_scopes.is_empty() {
            orch_log(&cwd, &format!("✗ drift check failed: {}", failed_scopes.join(" | ")));
            let _ = app.emit("build-active-node", Vec::<String>::new());
            let _ = app.emit(
                "agent-event",
                &scryer_acp::AgentEvent::Failed {
                    error: failed_scopes.join(" | "),
                },
            );
            return;
        }
        let line = format!("drift complete: {:.1}s total", drift_start.elapsed().as_secs_f64());
        eprintln!("[drift] {line}");
        orch_log(&cwd, &line);

        write_anchor();
        let _ = app.emit("build-active-node", Vec::<String>::new());
        emit_msg("✓ Drift check complete — review any flagged items.".into());
        let _ = app.emit(
            "agent-event",
            &scryer_acp::AgentEvent::Completed {
                stop_reason: "drift_check_complete".into(),
            },
        );
    });

    Ok("started".to_string())
}
