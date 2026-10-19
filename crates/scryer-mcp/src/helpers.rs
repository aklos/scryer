//! Builds a node's breadcrumb path by name from a root down to it
//!
//! Re-mints an invented, repeated, or wrong-host responsibility id in a raw-write payload so no two claims ever share an id
//!
//! Enforces read-only invariants — user directives and canvas positions — against the AI's whole-node generation writes
//!
//! Computes the loop-state counts straight from disk, best-effort, with the model write lock released
//!
//! Re-mints caller-supplied node and responsibility ids that would otherwise collide with a claim living on another host or in another layer
//!
//! Re-mints payload node ids that name a node living outside the region a write is replacing, repointing every reference to the rename
//!
//! Routes a claim's implementation or test anchor entries to its single home layer, normalizing whole-symbol ranges
//!
//! Restores each surviving node's user-placed canvas position after a whole-node regeneration, leaving new nodes for auto-layout
//!
//! Writes the plan after tagging what this write changed to the session's current change, warning on collisions with another change's tag
//!
//! Assembles the one-line loop-state header for write responses: pending, untested, drift, structural violations, tests, and probe survivors
//!
//! Applies claim anchor entries to their single home layer and tags plan writes to the session's open change
//!
//! Builds a compact outline of the model tree with responsibility/property counts instead of full bodies
//!
//! Forces every directive back to the prior on-disk model's value before committing an AI write, keeping directives entirely under user control
//!
//! Computes and phrases the ambient loop-state counts (pending work, drift, tests, structural violations) shared by every write response and the CLI

use rmcp::model::{CallToolResult, Content};
use rmcp::ErrorData as McpError;
use scryer_core::infrastructure::verdict_store::ProjectLock;
use scryer_core::{Kind, ModelRef, Node, ScryModel};

use std::collections::HashMap;

/// Acquire the project's exclusive lock, or return an error result to surface
/// to the agent. Hold the guard for a tool's whole read-modify-write of the
/// evidence caches so concurrent sessions serialize.
pub(crate) fn lock_or_err(model_ref: &ModelRef) -> Result<ProjectLock, CallToolResult> {
    scryer_core::derived_tree::lock_project(model_ref)
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

