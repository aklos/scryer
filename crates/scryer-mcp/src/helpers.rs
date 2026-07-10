use rmcp::model::{CallToolResult, Content};
use rmcp::ErrorData as McpError;
use scryer_core::history::{append_event, EventRow, HistoryEvent};
use scryer_core::{Kind, ModelLock, ModelRef, Node, Responsibility, ScryModel};
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

/// Build a denormalized graph view of a node for MCP responses:
/// adds `childIds`, `incomingLinks`, `outgoingLinks` to the node JSON.
pub(crate) fn denormalize_node(node: &Node, model: &ScryModel) -> serde_json::Value {
    let mut val = serde_json::to_value(node).unwrap_or(serde_json::Value::Null);
    if let serde_json::Value::Object(map) = &mut val {
        let child_ids: Vec<&str> = model
            .nodes
            .iter()
            .filter(|n| n.parent_id.as_deref() == Some(&node.id))
            .map(|n| n.id.as_str())
            .collect();
        let incoming: Vec<&str> = model
            .links
            .iter()
            .filter(|l| l.dst == node.id)
            .map(|l| l.id.as_str())
            .collect();
        let outgoing: Vec<&str> = model
            .links
            .iter()
            .filter(|l| l.src == node.id)
            .map(|l| l.id.as_str())
            .collect();
        map.insert("childIds".to_string(), serde_json::json!(child_ids));
        map.insert("incomingLinks".to_string(), serde_json::json!(incoming));
        map.insert("outgoingLinks".to_string(), serde_json::json!(outgoing));
    }
    val
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

/// Directives are user-authored and read-only to the AI — both a
/// responsibility's `directives` and a node's own node-level `directives`.
/// Before committing any AI write, force each back to whatever the prior
/// on-disk model held for that id; ids with no prior entry get none. This lets
/// the AI create, edit, and move responsibilities and nodes while leaving
/// directives entirely under the user's control. (The interactive patch path
/// can't reach them — they're `schemars(skip)` — but the whole-node generation
/// primitives `set_model`/`set_node` rebuild nodes from JSON and would
/// otherwise drop them.) Not applied to `move_responsibilities`, which
/// preserves directives across a deliberate responsibility-id rename.
pub(crate) fn enforce_readonly_directives(model: &mut ScryModel, prior: &ScryModel) {
    let prior_resps = prior
        .nodes
        .iter()
        .flat_map(|n| n.responsibilities.iter())
        .chain(prior.groups.iter().flat_map(|g| g.responsibilities.iter()));
    let prior_dir: HashMap<&str, &Vec<String>> = prior_resps
        .map(|r| (r.id.as_str(), &r.directives))
        .collect();
    let restore = |r: &mut Responsibility| {
        r.directives = prior_dir
            .get(r.id.as_str())
            .map(|d| (*d).clone())
            .unwrap_or_default();
    };
    // Node-level directives, keyed by node id (same read-only guarantee).
    let prior_node_dir: HashMap<&str, &Vec<String>> = prior
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), &n.directives))
        .collect();
    for n in &mut model.nodes {
        n.responsibilities.iter_mut().for_each(&restore);
        n.directives = prior_node_dir
            .get(n.id.as_str())
            .map(|d| (*d).clone())
            .unwrap_or_default();
    }
    for g in &mut model.groups {
        g.responsibilities.iter_mut().for_each(&restore);
    }
}


/// Project root from request param, active model, or cwd.
/// Build a committed-model event diff row for a responsibility — its statement,
/// anchored to the first source location the model maps it to (if any).
pub(crate) fn resp_event_row(marker: &str, model: &ScryModel, resp: &Responsibility) -> EventRow {
    let row = EventRow::new(marker, resp.statement.clone());
    match model.source_map.get(&resp.id).and_then(|locs| locs.first()) {
        Some(loc) => row.with_source(loc.clone()),
        None => row,
    }
}

/// Record a committed-model history event, best-effort: a logging failure must
/// never abort the model operation that produced it (see [`scryer_core::history`]).
pub(crate) fn record_event(model_ref: &ModelRef, ev: HistoryEvent) {
    let _ = append_event(model_ref, &ev);
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

/// Error text for a failed model/plan read. A missing file means NO MODEL
/// EXISTS yet — steer at the bootstrap path instead of stranding the agent
/// with a raw "os error 2".
pub(crate) fn read_fail(layer: &str, model_ref: &ModelRef, e: &str) -> String {
    if e.contains("os error 2") || e.contains("No such file") {
        format!(
            "No model exists at {model_ref} yet ({layer} file is absent). Start with \
             `read_codebase` to see the codebase, then build top-down: `add_system` / \
             `add_container` / `fill_container`. If you meant a different project, pass \
             its absolute path as `project`."
        )
    } else {
        format!("Failed to read {layer} at {model_ref}: {e}")
    }
}

/// Plan-diff element count with vagrants excluded — the same queue
/// `get_pending` reports (vagrant elements are drift review, never the
/// implement queue).
pub(crate) fn pending_change_count(committed: &ScryModel, planned: &ScryModel) -> usize {
    use scryer_core::diff::ElementKind as EK;
    let plan = scryer_core::diff::diff(committed, planned);
    plan.changes
        .iter()
        .filter(|ch| {
            let vagrant = match ch.kind {
                EK::Node => planned
                    .nodes
                    .iter()
                    .any(|n| n.id == ch.id && n.vagrant == Some(true)),
                EK::Responsibility => planned
                    .nodes
                    .iter()
                    .flat_map(|n| n.responsibilities.iter())
                    .chain(planned.groups.iter().flat_map(|g| g.responsibilities.iter()))
                    .any(|r| r.id == ch.id && r.vagrant == Some(true)),
                EK::Property => ch.owner_id.as_deref().is_some_and(|oid| {
                    planned.nodes.iter().any(|n| {
                        n.id == oid
                            && n.properties
                                .iter()
                                .any(|p| p.label == ch.id && p.vagrant == Some(true))
                    })
                }),
                _ => false,
            };
            !vagrant
        })
        .count()
}

/// The loop-state counts behind every ambient status line — shared by the MCP
/// response headers and the `status`/`statusline` CLI subcommands.
pub(crate) struct StatusCounts {
    pub pending: usize,
    /// None until a reconcile baseline exists — drift and anchor states have
    /// nothing to measure against, and reporting zeros would fake certainty.
    pub baseline: Option<BaselineCounts>,
}

pub(crate) struct BaselineCounts {
    pub drift_scopes: usize,
    pub anchors_changed: usize,
    pub anchors_broken: usize,
}

/// Compute [`StatusCounts`] straight from disk. Best-effort: None when there
/// is no committed model to report on. MUST be called with the model lock
/// RELEASED — the anchor check takes the lock itself (it re-anchors
/// moved-but-unchanged symbols in place).
pub(crate) fn status_counts(model_ref: &ModelRef) -> Option<StatusCounts> {
    let committed = scryer_core::read_model_at(model_ref).ok()?;
    let planned = scryer_core::read_planned_at(model_ref).ok()?;
    let pending = pending_change_count(&committed, &planned);
    if !model_ref.sync_path().exists() {
        return Some(StatusCounts { pending, baseline: None });
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
        baseline: Some(BaselineCounts {
            drift_scopes: scopes,
            anchors_changed: changed,
            anchors_broken: broken,
        }),
    })
}

/// One-line loop-state header for write responses — `plan: N pending · drift:
/// N scope(s) · anchors: N changed, N broken` — so the model's state stays
/// ambient across a coding session without the agent re-polling the
/// orientation tools. Same locking contract as [`status_counts`].
pub(crate) fn status_header(model_ref: &ModelRef) -> Option<String> {
    let c = status_counts(model_ref)?;
    Some(match c.baseline {
        // Never reconciled: drift/anchors have no baseline to report against.
        None => format!("plan: {} pending · drift: no reconcile anchor yet", c.pending),
        Some(b) => format!(
            "plan: {} pending · drift: {} scope(s) · anchors: {} changed, {} broken",
            c.pending, b.drift_scopes, b.anchors_changed, b.anchors_broken
        ),
    })
}

/// Apply responsibility anchor entries (the `entries` shape of
/// `update_source_map`) to their SINGLE home: the committed model owns every
/// committed claim's anchor; the planned draft holds anchors only for claims it
/// ADDS. Whole-symbol line ranges are normalized to symbol-only anchors (the
/// honest encoding for "this whole definition"), reported in the returned
/// notes. Mutates the models in place; the CALLER validates ids beforehand and
/// persists both layers afterwards (writing `committed` only when the returned
/// flag is true). Shared by `update_source_map` and `mark_implemented`'s
/// fold-time `anchors`.
pub(crate) fn apply_resp_anchor_entries(
    project: &std::path::Path,
    planned: &mut ScryModel,
    committed: &mut Option<ScryModel>,
    mut entries: Vec<crate::types::SourceMapEntry>,
) -> (Vec<String>, bool) {
    let mut normalized: Vec<String> = Vec::new();
    {
        let mut resolver = scryer_extract::anchors::ExtentResolver::new(project);
        for entry in &mut entries {
            for loc in &mut entry.locations {
                let (Some(sym), Some(line)) = (loc.symbol.clone(), loc.line) else {
                    continue;
                };
                let end = loc.end_line.unwrap_or(line);
                let Some(extent) = resolver.extent(&loc.pattern, &sym, Some(line)) else {
                    continue;
                };
                if scryer_extract::anchors::covers_extent(line, end, extent) {
                    loc.line = None;
                    loc.end_line = None;
                    normalized.push(format!(
                        "{}: {} L{}-{} covered the whole symbol `{}` (L{}-{})",
                        entry.responsibility_id, loc.pattern, line, end, sym, extent.0, extent.1
                    ));
                }
            }
        }
    }

    let committed_resp_ids: std::collections::HashSet<String> = match committed.as_ref() {
        Some(c) => c
            .nodes
            .iter()
            .flat_map(|n| n.responsibilities.iter())
            .chain(c.groups.iter().flat_map(|g| g.responsibilities.iter()))
            .map(|r| r.id.clone())
            .collect(),
        None => Default::default(),
    };
    let mut committed_dirty = false;
    for entry in entries {
        let key = entry.responsibility_id;
        if entry.locations.is_empty() {
            planned.source_map.remove(&key);
            if committed_resp_ids.contains(&key) {
                if let Some(c) = committed.as_mut() {
                    committed_dirty |= c.source_map.remove(&key).is_some();
                }
            }
        } else if committed_resp_ids.contains(&key) {
            planned.source_map.remove(&key);
            if let Some(c) = committed.as_mut() {
                c.source_map.insert(key, entry.locations);
                committed_dirty = true;
            }
        } else {
            planned.source_map.insert(key, entry.locations);
        }
    }
    (normalized, committed_dirty)
}
