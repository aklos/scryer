//! A2 measurement: run the FULL build headlessly the way the app orchestrates
//! it — mint the skeleton mechanically, then the system semantic session and
//! every container session concurrently in an adaptive pool. Reports
//! per-session wall clock + tokens and the build's total wall clock.
//!
//! Usage:
//!   SCRYER_MCP=/path/to/scryer-mcp \
//!   cargo run -p scryer-acp --example full_build_bench -- <scratch-project>
//!
//! The scratch project should be a throwaway copy of a repo — the bench
//! overwrites its `.scryer/model.scry`.

use std::sync::Arc;

struct Job {
    id: String,
    name: String,
    evidence_json: String,
    work_units: usize,
    payload_bytes: usize,
}

// Mirror of the app's adaptive pool sizing (src-tauri wave2_pool_size/permits).
fn pool_size(jobs: &[Job]) -> usize {
    if jobs.is_empty() {
        return 1;
    }
    let average = jobs.iter().map(|j| j.payload_bytes).sum::<usize>() / jobs.len();
    let cpu_cap = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2).clamp(1, 4);
    let payload_cap = match average {
        0..=150_000 => 4,
        150_001..=450_000 => 3,
        _ => 2,
    };
    jobs.len().min(cpu_cap).min(payload_cap).max(1)
}

fn job_permits(job: &Job, pool: usize) -> u32 {
    let desired = match (job.payload_bytes, job.work_units) {
        (450_001.., _) | (_, 8_001..) => pool,
        (150_001.., _) | (_, 3_001..) => 2,
        _ => 1,
    };
    desired.min(pool).max(1) as u32
}

async fn run_session(
    runtime: &scryer_acp::AcpRuntime,
    binary: &str,
    kind: &scryer_acp::AgentKind,
    cwd: &str,
    model_name: &str,
    effort: &str,
    mcp_binary: &str,
    prompt: String,
) -> Result<(scryer_acp::Usage, f64), String> {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let start = std::time::Instant::now();
    runtime
        .start_session(
            binary.to_string(),
            scryer_acp::runtime::LaunchMode::Cli { kind: kind.clone() },
            cwd.to_string(),
            model_name.to_string(),
            effort.to_string(),
            mcp_binary.to_string(),
            prompt,
            vec!["mcp__scryer__*".into()],
            tx,
        )
        .await?;
    let mut usage = scryer_acp::Usage::default();
    while let Some(ev) = rx.recv().await {
        match ev {
            scryer_acp::AgentEvent::Usage { usage: u } => usage = u,
            scryer_acp::AgentEvent::Completed { .. } => break,
            scryer_acp::AgentEvent::Cancelled => break,
            scryer_acp::AgentEvent::Failed { error } => return Err(error),
            _ => {}
        }
    }
    Ok((usage, start.elapsed().as_secs_f64()))
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let project = std::env::args().nth(1).expect("usage: full_build_bench <scratch-project>");
    let project_path = std::path::Path::new(&project);
    let mcp_binary = std::env::var("SCRYER_MCP").unwrap_or_else(|_| "scryer-mcp".into());

    let build_start = std::time::Instant::now();

    // Deterministic context + dependency-graph cache (as the app does).
    let ctx = scryer_extract::extract_context(project_path).expect("extraction");
    let edges = scryer_core::build_edges::BuildEdges {
        symbol_edges: ctx
            .symbol_edges
            .iter()
            .map(|e| scryer_core::build_edges::CachedEdge { src: e.src.clone(), dst: e.dst.clone() })
            .collect(),
    };
    scryer_core::build_edges::write_build_edges(project_path, &edges).expect("edge cache");

    // Seed the skeleton mechanically (A2) into a fresh model.
    let units: Vec<scryer_core::seed::SeedUnit> = ctx
        .containers
        .iter()
        .map(|c| scryer_core::seed::SeedUnit {
            dir: c.dir.clone(),
            name: c.name.clone(),
            technology: c.technology.clone(),
            dep_dirs: c.dep_dirs.clone(),
        })
        .collect();
    let mut model = scryer_core::ScryModel::new();
    let (system_id, triples) =
        scryer_core::seed::mint_initial_structure(&mut model, &ctx.project_name, &units);
    let model_ref = scryer_core::ModelRef::ProjectLocal(project_path.to_path_buf());
    scryer_core::write_model_at(&model_ref, &model).expect("write model");
    eprintln!(
        "[bench] seeded {} containers under {} in {:.2}s",
        triples.len(),
        system_id,
        build_start.elapsed().as_secs_f64(),
    );

    let structure_json = serde_json::to_string_pretty(
        &ctx.containers
            .iter()
            .zip(&triples)
            .map(|(c, (id, name, dir))| {
                serde_json::json!({
                    "id": id, "name": name, "dir": dir,
                    "technology": c.technology, "depDirs": c.dep_dirs,
                })
            })
            .collect::<Vec<_>>(),
    )
    .unwrap();

    // Container jobs.
    let mut jobs: Vec<Job> = Vec::new();
    for (id, name, dir) in &triples {
        let scope = scryer_extract::slice_container(&ctx, dir);
        if scope.files.is_empty() {
            continue;
        }
        let evidence = scryer_extract::compact_scope(&scope);
        let evidence_json = serde_json::to_string(&evidence).unwrap();
        jobs.push(Job {
            id: id.clone(),
            name: if name.is_empty() { format!("'{dir}'") } else { name.clone() },
            payload_bytes: evidence_json.len(),
            work_units: evidence.work_units(),
            evidence_json,
        });
    }
    jobs.sort_by(|a, b| b.work_units.cmp(&a.work_units));
    let pool = pool_size(&jobs);
    eprintln!("[bench] {} container job(s), pool {} (+1 system)", jobs.len(), pool);

    let settings = scryer_core::read_subagent_settings();
    let launch = scryer_acp::detect_available_agent_pref(&settings.agent).expect("agent on PATH");
    let (binary, kind, model_name, effort) = match launch {
        scryer_acp::AgentLaunch::Cli { binary, kind } => {
            let (m, e) = match kind {
                scryer_acp::AgentKind::ClaudeCode => {
                    (settings.claude.model.clone(), settings.claude.effort.clone())
                }
                scryer_acp::AgentKind::Codex => {
                    (settings.codex.model.clone(), settings.codex.effort.clone())
                }
                _ => (settings.cursor.model.clone(), settings.cursor.effort.clone()),
            };
            (binary, kind, m, e)
        }
        scryer_acp::AgentLaunch::Acp { .. } => panic!("bench needs a CLI agent"),
    };

    let runtime = scryer_acp::AcpRuntime::new();
    let sem = Arc::new(tokio::sync::Semaphore::new(pool + 1));
    let results: Arc<tokio::sync::Mutex<Vec<(String, f64, scryer_acp::Usage)>>> =
        Arc::new(tokio::sync::Mutex::new(Vec::new()));
    let mut handles = Vec::new();

    // System semantic session, concurrent with the container pool.
    {
        let sem = sem.clone();
        let results = results.clone();
        let runtime = runtime.clone();
        let (binary, kind, project, model_name, effort, mcp_binary) = (
            binary.clone(), kind.clone(), project.clone(),
            model_name.clone(), effort.clone(), mcp_binary.clone(),
        );
        let prompt = scryer_acp::prompt::enrich_system_prompt(&project, &system_id, &structure_json);
        handles.push(tokio::spawn(async move {
            let _p = sem.acquire().await.unwrap();
            eprintln!("[bench] +{:>5.1}s start system pass", 0.0);
            match run_session(&runtime, &binary, &kind, &project, &model_name, &effort, &mcp_binary, prompt).await {
                Ok((usage, secs)) => results.lock().await.push(("(system pass)".into(), secs, usage)),
                Err(e) => eprintln!("[bench] system pass FAILED: {e}"),
            }
        }));
    }

    for job in jobs {
        let permits = job_permits(&job, pool);
        let sem = sem.clone();
        let results = results.clone();
        let runtime = runtime.clone();
        let (binary, kind, project, model_name, effort, mcp_binary) = (
            binary.clone(), kind.clone(), project.clone(),
            model_name.clone(), effort.clone(), mcp_binary.clone(),
        );
        let build_start = build_start;
        handles.push(tokio::spawn(async move {
            let _p = sem.acquire_many(permits as u32).await.unwrap();
            eprintln!(
                "[bench] +{:>5.1}s start '{}' ({} bytes, {} permits)",
                build_start.elapsed().as_secs_f64(), job.name, job.payload_bytes, permits,
            );
            let prompt = scryer_acp::prompt::build_container_prompt(
                &project, &job.name, &job.id, &job.evidence_json,
            );
            match run_session(&runtime, &binary, &kind, &project, &model_name, &effort, &mcp_binary, prompt).await {
                Ok((usage, secs)) => results.lock().await.push((job.name.clone(), secs, usage)),
                Err(e) => eprintln!("[bench] '{}' FAILED: {e}", job.name),
            }
        }));
    }

    for h in handles {
        let _ = h.await;
    }

    let total = build_start.elapsed().as_secs_f64();
    let after = scryer_core::read_model_at(&model_ref).expect("model after");
    let count = |k: scryer_core::Kind| after.nodes.iter().filter(|n| n.kind == k).count();

    println!("\n=== full_build_bench ===");
    let mut grand = scryer_acp::Usage::default();
    let mut rows = results.lock().await.clone();
    rows.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    for (name, secs, usage) in &rows {
        grand.add(usage);
        println!(
            "{:<28} {:>6.1}s   out {:>6}  cache-w {:>7}  cache-r {:>7}",
            name, secs, usage.output_tokens,
            usage.cache_creation_input_tokens, usage.cache_read_input_tokens,
        );
    }
    println!(
        "TOTAL wall {total:.1}s — {} containers, {} components, {} symbols — out {} / cache-w {} / cache-r {} (≈${:.2} API-equiv)",
        count(scryer_core::Kind::Container),
        count(scryer_core::Kind::Component),
        count(scryer_core::Kind::Symbol),
        grand.output_tokens, grand.cache_creation_input_tokens, grand.cache_read_input_tokens,
        grand.cost_usd,
    );
}
