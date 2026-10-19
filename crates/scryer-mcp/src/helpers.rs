use rmcp::model::{CallToolResult, Content};
use rmcp::ErrorData as McpError;
use scryer_core::{Kind, ModelLock, ModelRef, Node, ScryModel};

use std::collections::HashMap;

/// Acquire the exclusive model write lock, or return an error result to surface
/// to the agent. Hold the returned guard for the whole read-modify-write of a
/// write tool so concurrent writers (parallel sessions, the canvas) serialize.
pub(crate) fn lock_or_err(model_ref: &ModelRef) -> Result<ModelLock, CallToolResult> {
    scryer_core::lock_model(model_ref)
        .map_err(|e| CallToolResult::error(vec![Content::text(e)]))
}

/// Strip empty values from a JSON tree to keep MCP responses compact.
pub(crate) fn strip_fields_compact(val: &mut serde_json::Value) {
    match val {
        serde_json::Value::Object(map) => {
            map.retain(|_, v| !matches!(v, serde_json::Value::String(s) if s.is_empty()));
            map.retain(|_, v| !v.is_null());
            map.retain(|_, v| !matches!(v, serde_json::Value::Array(a) if a.is_empty()));
            map.retain(|_, v| !matches!(v, serde_json::Value::Object(m) if m.is_empty()));
            for (_, v) in map.iter_mut() {
                strip_fields_compact(v);
            }
        }
        serde_json::Value::Array(arr) => {
            for v in arr.iter_mut() {
                strip_fields_compact(v);
            }
        }
        _ => {}
    }
}

pub(crate) fn parse_kind(s: &str) -> Result<Kind, McpError> {
    match s {
        "person" => Ok(Kind::Person),
        "system" => Ok(Kind::System),
        "container" => Ok(Kind::Container),
        "component" => Ok(Kind::Component),
        // "schema" is the legacy name for a symbol that carries only properties.
        "symbol" | "schema" => Ok(Kind::Symbol),
        _ => Err(McpError::invalid_params(
            format!(
                "Invalid kind '{}'. Must be: person, system, container, component, symbol",
                s
            ),
            None,
        )),
    }
}

pub(crate) fn kind_str(k: &Kind) -> &'static str {
    match k {
        Kind::Person => "person",
        Kind::System => "system",
        Kind::Container => "container",
        Kind::Component => "component",
        Kind::Symbol => "symbol",
    }
}

/// Build a compact outline tree of the model: each node carries its id, name,
/// kind, a one-line description, and responsibility/property COUNTS (not
/// bodies), plus its children nested under it. Lets an agent grasp a model's
/// shape without materializing every responsibility, property, link, and
/// source-map entry. Roots are nodes with no parent. When `include_symbols` is
/// false the code level is omitted — the architecture overview (drill into a
/// component with `read_model {node}` to see its symbols).
pub(crate) fn outline_tree(model: &ScryModel, include_symbols: bool) -> Vec<serde_json::Value> {
    let mut children_of: HashMap<Option<&str>, Vec<&Node>> = HashMap::new();
    for n in &model.nodes {
        if !include_symbols && n.kind == Kind::Symbol {
            continue;
        }
        children_of
            .entry(n.parent_id.as_deref())
            .or_default()
            .push(n);
    }

    fn build(
        node: &Node,
        children_of: &HashMap<Option<&str>, Vec<&Node>>,
    ) -> serde_json::Value {
        let kids: Vec<serde_json::Value> = children_of
            .get(&Some(node.id.as_str()))
            .map(|cs| cs.iter().map(|c| build(c, children_of)).collect())
            .unwrap_or_default();
        let mut v = serde_json::json!({
            "id": node.id,
            "name": node.name,
            "kind": kind_str(&node.kind),
            "description": node.description,
            "nResp": node.responsibilities.len(),
            "nProps": node.properties.len(),
            "children": kids,
        });
        strip_fields_compact(&mut v);
        v
    }

    children_of
        .get(&None)
        .map(|roots| roots.iter().map(|r| build(r, &children_of)).collect())
        .unwrap_or_default()
}

/// Breadcrumb path from a root down to `node_id`, by node name, joined with
/// " / ". Used to give search hits a location without the caller re-walking the
/// tree.
pub(crate) fn breadcrumb(model: &ScryModel, node_id: &str) -> String {
    let by_id: HashMap<&str, &Node> = model.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    let mut names = Vec::new();
    let mut cur = by_id.get(node_id).copied();
    let mut guard = 0;
    while let Some(n) = cur {
        names.push(n.name.as_str());
        cur = n.parent_id.as_deref().and_then(|p| by_id.get(p).copied());
        guard += 1;
        if guard > 64 {
            break; // cycle guard
        }
    }
    names.reverse();
    names.join(" / ")
}

pub(crate) fn resolve_model_ref(req_project: Option<&str>) -> Result<ModelRef, McpError> {
    let path = match req_project {
        Some(p) => std::path::PathBuf::from(p),
        None => std::env::current_dir().map_err(|e| {
            McpError::internal_error(format!("cannot read cwd: {}", e), None)
        })?,
    };
    Ok(ModelRef::ProjectLocal(path))
}

/// Plan-diff element count with vagrants excluded — the same queue
/// `get_pending` reports. Lives in `scryer_core::diff` so the desktop app's
/// hook endpoint counts identically; re-exported here for the call sites that
/// have always spelled it this way.
pub(crate) use scryer_core::diff::pending_elements as pending_changes;

pub(crate) fn pending_change_count(committed: &ScryModel, planned: &ScryModel) -> usize {
    pending_changes(committed, planned).len()
}

/// The loop-state counts behind every ambient status line — shared by the MCP
/// response headers and the `status`/`statusline` CLI subcommands.
pub(crate) struct StatusCounts {
    /// The agent's implement queue: one per diverging element (a reworded
    /// claim, an added property…), vagrants excluded — the same list
    /// `get_pending` returns. What the agent-facing header reports.
    pub pending: usize,
    /// Plan-change carriers: element diffs folded under the owning node/group,
    /// the number the Changes page and tree lens show. What the human-facing
    /// status line reports, so it agrees with the canvas.
    pub carriers: usize,
    /// Open changes in the plan's ledger — named workstreams in flight.
    pub open_changes: usize,
    /// None until a reconcile baseline exists — drift and anchor states have
    /// nothing to measure against, and reporting zeros would fake certainty.
    pub baseline: Option<BaselineCounts>,
    /// None until a test report has been ingested — with no recorded verdicts
    /// there is no test state to report, and zeros would fake certainty.
    pub tests: Option<TestCounts>,
    /// Structural violations (imports the layer matrix forbids, banned
    /// packages, misplaced files) from the last build's dependency cache; 0
    /// when no cache exists.
    pub structural_violations: usize,
}

pub(crate) struct BaselineCounts {
    pub drift_scopes: usize,
    pub anchors_changed: usize,
    pub anchors_broken: usize,
}

/// Recorded test-verdict counts. Verified-green is the norm and stays silent
/// in the headers; `failing` (a current verdict that is red — rare, and an
/// alarm precisely because the loop normally fixes red before it lands) and
/// `stale` (verdicts the code has moved past — the standing signal that
/// feeds `get_test_radius`) are what get spoken.
pub(crate) struct TestCounts {
    pub failing: usize,
    pub stale: usize,
    pub recorded: usize,
}

/// Compute [`StatusCounts`] straight from disk. Best-effort: None when there
/// is no committed model to report on. MUST be called with the model lock
/// RELEASED — the anchor check takes the lock itself (it re-anchors
/// moved-but-unchanged symbols in place).
pub(crate) fn status_counts(model_ref: &ModelRef) -> Option<StatusCounts> {
    let committed = scryer_core::read_model_at(model_ref).ok()?;
    let planned = scryer_core::read_planned_at(model_ref).ok()?;
    let pending = pending_change_count(&committed, &planned);
    let carriers = scryer_core::diff::plan_carrier_count(&committed, &planned);
    let open_changes = planned.changes.len();
    // Recorded test verdicts, re-verified against the tree (fingerprint
    // compare with an mtime fast path — cheap enough for every response).
    let verdicts = scryer_extract::test_status::test_statuses(model_ref).unwrap_or_default();
    let tests = (!verdicts.is_empty()).then(|| TestCounts {
        failing: verdicts
            .iter()
            .filter(|s| {
                !s.stale
                    && matches!(
                        s.outcome,
                        scryer_core::test_results::TestOutcome::Failed
                            | scryer_core::test_results::TestOutcome::Errored
                    )
            })
            .count(),
        stale: verdicts.iter().filter(|s| s.stale).count(),
        recorded: verdicts.len(),
    });
    // Cached-graph only (no file walk): cheap enough for every response.
    let structural_violations = scryer_core::build_edges::read_build_edges(&model_ref.build_edges_path())
        .map(|edges| {
            let derived = scryer_core::build_edges::derive_graph(&committed, &edges);
            scryer_core::style_health::check_code(
                &committed,
                &scryer_core::load_styles(model_ref.project_path()),
                &derived,
                &edges,
                None,
            )
            .total()
        })
        .unwrap_or(0);
    if !model_ref.sync_path().exists() {
        return Some(StatusCounts {
            pending,
            carriers,
            open_changes,
            baseline: None,
            tests,
            structural_violations,
        });
    }
    let sync = scryer_core::read_sync_state(model_ref);
    let scopes =
        scryer_core::drift::drifted_scopes(&committed, model_ref.project_path(), &sync).len();
    let check = scryer_extract::anchors::check_anchors(model_ref).unwrap_or_default();
    let broken = check
        .observations
        .iter()
        .filter(|o| !matches!(o.state, scryer_extract::anchors::AnchorState::Changed))
        .count();
    let changed = check.observations.len() - broken;
    Some(StatusCounts {
        pending,
        carriers,
        open_changes,
        baseline: Some(BaselineCounts {
            drift_scopes: scopes,
            anchors_changed: changed,
            anchors_broken: broken,
        }),
        tests,
        structural_violations,
    })
}

/// The header/statusline fragment for structural violations — real imports
/// and files that break a declared style. Empty at zero, since conformance
/// is the norm and only the exceptions are work.
pub(crate) fn structural_phrase(c: &StatusCounts) -> String {
    match c.structural_violations {
        0 => String::new(),
        1 => " · 1 structural violation".to_string(),
        n => format!(" · {n} structural violations"),
    }
}

/// The header/statusline fragment for recorded test verdicts — empty unless
/// something is failing or stale. Verified-green is the norm; the line only
/// speaks when there is work: red left behind, or verdicts the code moved
/// past (run `get_test_radius` for exactly what to re-run).
pub(crate) fn tests_phrase(c: &StatusCounts) -> String {
    match &c.tests {
        Some(t) if t.failing > 0 && t.stale > 0 => {
            format!(" · tests: {} failing, {} stale", t.failing, t.stale)
        }
        Some(t) if t.failing > 0 => format!(" · tests: {} failing", t.failing),
        Some(t) if t.stale > 0 => format!(" · tests: {} stale", t.stale),
        _ => String::new(),
    }
}

