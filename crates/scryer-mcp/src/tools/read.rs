use crate::helpers::*;
use crate::server::ScryerServer;
use crate::types::*;
use crate::validate;
use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, Content},
    tool, tool_router, ErrorData as McpError,
};
use scryer_core::{Node, ScryModel};
use std::collections::HashSet;

/// Minimum Jaro–Winkler similarity for a query term to count as a fuzzy match
/// on a field word. Short terms are held to a stricter bar so a 3–4 char term
/// can't fan out across half the model on prefix similarity alone.
const FUZZY_THRESHOLD_LONG: f64 = 0.82;
const FUZZY_THRESHOLD_SHORT: f64 = 0.90;

fn fuzzy_threshold(term: &str) -> f64 {
    if term.chars().count() <= 4 {
        FUZZY_THRESHOLD_SHORT
    } else {
        FUZZY_THRESHOLD_LONG
    }
}

/// Best similarity of `term` against one (already lowercased) field. Substring
/// containment is the exact signal preserved from the original search and scores
/// 1.0; otherwise the score is the best Jaro–Winkler similarity of `term`
/// against any alphanumeric word in the field. Returns `(score, exact)`.
fn term_field_score(term: &str, field_lower: &str) -> (f64, bool) {
    if field_lower.contains(term) {
        return (1.0, true);
    }
    let best = field_lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| strsim::jaro_winkler(term, w))
        .fold(0.0_f64, f64::max);
    (best, false)
}

/// Score a node's searchable `fields` against the query `terms`. Returns `None`
/// unless every term clears its threshold somewhere on the node (AND). On a hit,
/// returns the summed best-per-term score (the ranking key — exact matches pull
/// it up) and the per-field match report for the fields that contributed.
fn score_node(
    fields: &[(&str, String)],
    terms: &[String],
) -> Option<(f64, Vec<serde_json::Value>)> {
    let lowered: Vec<String> = fields.iter().map(|(_, v)| v.to_lowercase()).collect();
    // Best (score, exact) seen on each field across all terms, for reporting.
    let mut field_best: Vec<(f64, bool)> = vec![(0.0, false); fields.len()];
    let mut total = 0.0;
    for term in terms {
        let mut term_best = 0.0_f64;
        for (i, fl) in lowered.iter().enumerate() {
            let (s, exact) = term_field_score(term, fl);
            if s > field_best[i].0 {
                field_best[i] = (s, exact);
            }
            term_best = term_best.max(s);
        }
        if term_best < fuzzy_threshold(term) {
            return None; // this term matched nothing — node fails the AND
        }
        total += term_best;
    }
    let matched: Vec<serde_json::Value> = fields
        .iter()
        .enumerate()
        .filter(|(i, _)| field_best[*i].0 >= FUZZY_THRESHOLD_LONG)
        .map(|(i, (where_, v))| {
            let (s, exact) = field_best[i];
            serde_json::json!({
                "in": where_,
                "text": v,
                "match": if exact { "exact" } else { "fuzzy" },
                "score": (s * 100.0).round() / 100.0,
            })
        })
        .collect();
    Some((total, matched))
}

/// The architecture overview: the model tree down to components (symbols
/// excluded) with responsibility/property counts. Always small enough to read
/// whole, so an unqualified `read_model` can never bury the agent's context.
fn overview_payload(model: &ScryModel) -> serde_json::Value {
    serde_json::json!({
        "version": model.version,
        "view": "overview",
        "nodeCount": model.nodes.len(),
        "linkCount": model.links.len(),
        "groupCount": model.groups.len(),
        "overview": outline_tree(model, false),
    })
}

/// Full detail of one node's subtree: the node, its descendants (including
/// symbols), the links among them, external links + the partner nodes for
/// context, the references its children may link to, and the subtree's slice of
/// the source map + boundaries. `Err` if the node id is unknown.
fn subtree_payload(model: &ScryModel, node_id: &str) -> Result<serde_json::Value, String> {
    if !model.nodes.iter().any(|n| n.id == node_id) {
        return Err(format!("Node '{}' not found", node_id));
    }

    let mut subtree_ids: HashSet<String> = HashSet::new();
    subtree_ids.insert(node_id.to_string());
    let mut frontier = vec![node_id.to_string()];
    while let Some(id) = frontier.pop() {
        for child in model.nodes.iter().filter(|n| n.parent_id.as_deref() == Some(&id)) {
            if subtree_ids.insert(child.id.clone()) {
                frontier.push(child.id.clone());
            }
        }
    }

    let subtree_nodes: Vec<&Node> = model
        .nodes
        .iter()
        .filter(|n| subtree_ids.contains(&n.id))
        .collect();

    let internal_links: Vec<_> = model
        .links
        .iter()
        .filter(|l| subtree_ids.contains(&l.src) && subtree_ids.contains(&l.dst))
        .collect();

    let external_links: Vec<_> = model
        .links
        .iter()
        .filter(|l| {
            let s = subtree_ids.contains(&l.src);
            let d = subtree_ids.contains(&l.dst);
            (s && !d) || (!s && d)
        })
        .collect();

    let mut context_ids: HashSet<&str> = HashSet::new();
    for l in &external_links {
        if !subtree_ids.contains(&l.src) {
            context_ids.insert(l.src.as_str());
        }
        if !subtree_ids.contains(&l.dst) {
            context_ids.insert(l.dst.as_str());
        }
    }
    let context_nodes: Vec<serde_json::Value> = model
        .nodes
        .iter()
        .filter(|n| context_ids.contains(n.id.as_str()))
        .map(|n| {
            serde_json::json!({
                "id": n.id,
                "name": n.name,
                "kind": kind_str(&n.kind),
            })
        })
        .collect();

    // Source map is keyed by responsibility id (entries for any responsibility
    // owned by a subtree node) or by a schema node id (entries for any schema
    // node in the subtree).
    let subtree_resp_ids: HashSet<&str> = subtree_nodes
        .iter()
        .flat_map(|n| n.responsibilities.iter())
        .map(|r| r.id.as_str())
        .collect();
    let source_map: serde_json::Map<String, serde_json::Value> = model
        .source_map
        .iter()
        .filter(|(k, _)| subtree_resp_ids.contains(k.as_str()) || subtree_ids.contains(k.as_str()))
        .map(|(k, v)| (k.clone(), serde_json::to_value(v).unwrap_or(serde_json::Value::Null)))
        .collect();

    // Boundaries are keyed by node id.
    let boundaries: serde_json::Map<String, serde_json::Value> = model
        .boundaries
        .iter()
        .filter(|(k, _)| subtree_ids.contains(k.as_str()))
        .map(|(k, v)| (k.clone(), serde_json::to_value(v).unwrap_or(serde_json::Value::Null)))
        .collect();

    // References available to this node's children: the partners of the node's
    // OWN links. Links are same-level, so a child may only link to a node this
    // node already links to.
    let references_for_children: Vec<serde_json::Value> = model
        .links
        .iter()
        .filter_map(|l| {
            let (other, direction) = if l.src == node_id {
                (&l.dst, "outgoing")
            } else if l.dst == node_id {
                (&l.src, "incoming")
            } else {
                return None;
            };
            let n = model.nodes.iter().find(|n| &n.id == other)?;
            Some(serde_json::json!({
                "id": n.id,
                "name": n.name,
                "kind": kind_str(&n.kind),
                "direction": direction,
                "label": l.label,
            }))
        })
        .collect();

    Ok(serde_json::json!({
        "node": subtree_nodes.iter().find(|n| n.id == node_id),
        "descendants": subtree_nodes.iter().filter(|n| n.id != node_id).collect::<Vec<_>>(),
        "internalLinks": internal_links,
        "externalLinks": external_links,
        "contextNodes": context_nodes,
        "referencesForChildren": references_for_children,
        "sourceMap": source_map,
        "boundaries": boundaries,
    }))
}

/// A queryable node field resolved to a comparable typed value.
enum FieldVal {
    Str(Option<String>),
    Bool(bool),
    Num(f64),
}

/// Resolve a `query_model` field name to its typed value on a node. `child_count`
/// is precomputed by the caller. Unknown field names are an error the agent can
/// correct from.
fn resolve_field(n: &Node, field: &str, child_count: usize) -> Result<FieldVal, String> {
    Ok(match field {
        "kind" => FieldVal::Str(Some(kind_str(&n.kind).to_string())),
        "name" => FieldVal::Str(Some(n.name.clone())),
        "description" => FieldVal::Str(n.description.clone()),
        "technology" => FieldVal::Str(n.technology.clone()),
        "external" => FieldVal::Bool(n.external == Some(true)),
        "visual" => FieldVal::Bool(n.visual == Some(true)),
        "empty" => FieldVal::Bool(scryer_core::is_node_empty(n)),
        "vagrant" => FieldVal::Bool(n.responsibilities.iter().any(|r| r.vagrant == Some(true))),
        "responsibilityCount" | "responsibilities" => {
            FieldVal::Num(n.responsibilities.len() as f64)
        }
        "propertyCount" | "properties" => FieldVal::Num(n.properties.len() as f64),
        "childCount" | "children" => FieldVal::Num(child_count as f64),
        other => {
            return Err(format!(
                "Unknown query field '{}'. Valid: kind, name, description, technology, external, \
                 visual, empty, vagrant, responsibilityCount, propertyCount, \
                 childCount.",
                other
            ))
        }
    })
}

/// Evaluate one condition against a node. `Err` on a malformed condition
/// (unknown field/op or a value of the wrong type) so the query fails loud.
fn eval_condition(n: &Node, c: &QueryCondition, child_count: usize) -> Result<bool, String> {
    let fv = resolve_field(n, &c.field, child_count)?;
    let op = c.op.as_str();

    // `exists` / `absent` test presence, not a value.
    if op == "exists" || op == "absent" {
        let present = match &fv {
            FieldVal::Str(o) => o.as_ref().is_some_and(|s| !s.trim().is_empty()),
            FieldVal::Num(x) => *x > 0.0,
            FieldVal::Bool(b) => *b,
        };
        return Ok(if op == "exists" { present } else { !present });
    }

    let value = c
        .value
        .as_ref()
        .ok_or_else(|| format!("Condition on '{}' with op '{}' needs a `value`.", c.field, op))?;

    match &fv {
        FieldVal::Bool(b) => {
            let want = value
                .as_bool()
                .ok_or_else(|| format!("Field '{}' is boolean — `value` must be true/false.", c.field))?;
            match op {
                "eq" => Ok(*b == want),
                "ne" => Ok(*b != want),
                _ => Err(format!("Operator '{}' invalid on boolean field '{}' (use eq/ne).", op, c.field)),
            }
        }
        FieldVal::Num(x) => {
            let want = value
                .as_f64()
                .ok_or_else(|| format!("Field '{}' is numeric — `value` must be a number.", c.field))?;
            match op {
                "eq" => Ok(*x == want),
                "ne" => Ok(*x != want),
                "gt" => Ok(*x > want),
                "gte" => Ok(*x >= want),
                "lt" => Ok(*x < want),
                "lte" => Ok(*x <= want),
                _ => Err(format!("Operator '{}' invalid on numeric field '{}'.", op, c.field)),
            }
        }
        FieldVal::Str(o) => {
            let want = value
                .as_str()
                .ok_or_else(|| format!("Field '{}' is a string — `value` must be a string.", c.field))?;
            let have = o.as_deref().unwrap_or("");
            match op {
                "eq" => Ok(have.eq_ignore_ascii_case(want)),
                "ne" => Ok(!have.eq_ignore_ascii_case(want)),
                "contains" => Ok(have.to_lowercase().contains(&want.to_lowercase())),
                _ => Err(format!(
                    "Operator '{}' invalid on string field '{}' (use eq/ne/contains/exists/absent).",
                    op, c.field
                )),
            }
        }
    }
}

/// Read the model layer a request asked for. The plan is the default everywhere —
/// reads reflect pending edits, matching what the canvas shows — and `read_planned_at`
/// falls back to committed when no plan exists yet, so this is always safe.
fn read_layer(model_ref: &scryer_core::ModelRef, layer: Layer) -> Result<ScryModel, String> {
    match layer {
        Layer::Plan => scryer_core::read_planned_at(model_ref),
        Layer::Committed => scryer_core::read_model_at(model_ref),
    }
}

#[tool_router(router = tool_router_read, vis = "pub(crate)")]
impl ScryerServer {
    #[tool(
        description = "Read the architecture model. With NO `node`, returns the OVERVIEW: the whole tree down to components (symbols excluded) with responsibility/property counts — small and safe, the right first read. Pass a `node` id to read THAT node's full subtree: its descendants (including symbols), responsibilities, properties, links, `referencesForChildren` (the only nodes its children may link to), and the subtree's source map + boundaries. Drill into a component to see its symbols. If a requested subtree is too large to return whole, you get its direct-child skeleton plus guidance to drill further. Reads the PLAN by default — your editable draft, the same state the canvas shows, including your pending edits — so what you read back reflects what you just authored. Pass `layer: \"committed\"` only to inspect the source of truth the code currently satisfies."
    )]
    fn read_model(
        &self,
        Parameters(req): Parameters<ReadModelRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = resolve_model_ref(req.project.as_deref())?;
        *self.active_model.lock().unwrap() = Some(model_ref.clone());

        let model = match read_layer(&model_ref, req.layer) {
            Ok(m) => m,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to read model at {}: {}",
                    model_ref, e
                ))]));
            }
        };
        // Keep the legacy committed-model baseline fresh only when committed was actually
        // read; reading the plan (the default) must never overwrite the committed snapshot.
        if matches!(req.layer, Layer::Committed) {
            let _ = scryer_core::save_baseline_at(&model_ref, &model);
        }

        // Above this serialized size a subtree risks blowing the agent's
        // context, so it degrades to a child skeleton instead of dumping.
        const DETAIL_LIMIT: usize = 50_000;

        // No node: the architecture overview (always small — symbols excluded).
        let Some(node_id) = req.node.as_deref() else {
            let payload = overview_payload(&model);
            return Ok(CallToolResult::success(vec![Content::text(
                serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string()),
            )]));
        };

        let mut payload = match subtree_payload(&model, node_id) {
            Ok(p) => p,
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(e)])),
        };
        strip_fields_compact(&mut payload);
        let detail = serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string());
        if detail.len() <= DETAIL_LIMIT {
            return Ok(CallToolResult::success(vec![Content::text(detail)]));
        }

        // Subtree too big: return its direct-child skeleton so the agent can
        // drill into a specific child rather than swallowing the whole thing.
        let children: Vec<serde_json::Value> = model
            .nodes
            .iter()
            .filter(|n| n.parent_id.as_deref() == Some(node_id))
            .map(|n| {
                let mut v = serde_json::json!({
                    "id": n.id,
                    "name": n.name,
                    "kind": kind_str(&n.kind),
                    "nResp": n.responsibilities.len(),
                    "nProps": n.properties.len(),
                });
                strip_fields_compact(&mut v);
                v
            })
            .collect();
        let note = format!(
            "Subtree '{}' is ~{} KB — too large to return whole. Listed its direct children; \
             call read_model with one of their ids to drill in, or search_model to find a node.",
            node_id,
            detail.len() / 1024
        );
        let payload = serde_json::json!({
            "view": "overview",
            "node": node_id,
            "note": note,
            "children": children,
        });
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string()),
        )]))
    }

    #[tool(
        description = "Search the model for nodes matching free text. Case-insensitive and fuzzy: space-separated terms must ALL match somewhere on the node (name, description, technology, responsibility statements, or property labels), where a term matches either as a substring or by close edit-distance similarity — so `authentication` finds `authenticate` and typos still hit. Results are ranked by match quality (exact substrings rank above fuzzy), each carrying a `score` and a per-field `match` of `exact` or `fuzzy`. Returns each hit's id, kind, breadcrumb path, score, and matched fields — so you can locate a concept in a large model and then `read_model {node}` into it. Optional `kind` filter. Top 50 hits by score."
    )]
    fn search_model(
        &self,
        Parameters(req): Parameters<SearchModelRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = resolve_model_ref(req.project.as_deref())?;
        let model = match read_layer(&model_ref, req.layer) {
            Ok(m) => m,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to read model at {}: {}",
                    model_ref, e
                ))]));
            }
        };
        // Keep the legacy committed-model baseline fresh only when committed was actually
        // read; reading the plan (the default) must never overwrite the committed snapshot.
        if matches!(req.layer, Layer::Committed) {
            let _ = scryer_core::save_baseline_at(&model_ref, &model);
        }

        let kind_filter = match req.kind.as_deref() {
            Some(k) => Some(parse_kind(k)?),
            None => None,
        };
        let terms: Vec<String> = req
            .query
            .split_whitespace()
            .map(|t| t.to_lowercase())
            .collect();
        if terms.is_empty() {
            return Ok(CallToolResult::error(vec![Content::text(
                "Empty query.",
            )]));
        }

        const CAP: usize = 50;
        // Collect every matching node with its rank score, then sort and cap —
        // fuzzy matches mean node order is no longer a good proxy for relevance.
        let mut scored: Vec<(f64, serde_json::Value)> = Vec::new();
        for n in &model.nodes {
            if kind_filter.as_ref().is_some_and(|k| &n.kind != k) {
                continue;
            }
            // Collect this node's searchable fields, tagged by where they live.
            let mut fields: Vec<(&str, String)> = vec![("name", n.name.clone())];
            if let Some(d) = &n.description {
                fields.push(("description", d.clone()));
            }
            if let Some(t) = &n.technology {
                fields.push(("technology", t.clone()));
            }
            for r in &n.responsibilities {
                fields.push(("responsibility", r.statement.clone()));
            }
            for p in &n.properties {
                fields.push(("property", p.label.clone()));
            }
            // AND across terms: every term must match (exactly or fuzzily) somewhere.
            let Some((score, matched)) = score_node(&fields, &terms) else {
                continue;
            };
            scored.push((
                score,
                serde_json::json!({
                    "id": n.id,
                    "kind": kind_str(&n.kind),
                    "path": breadcrumb(&model, &n.id),
                    "score": (score * 100.0).round() / 100.0,
                    "matched": matched,
                }),
            ));
        }

        let truncated = scored.len() > CAP;
        // Stable sort by descending score keeps model order for ties.
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        let results: Vec<serde_json::Value> =
            scored.into_iter().take(CAP).map(|(_, v)| v).collect();

        let payload = serde_json::json!({
            "query": req.query,
            "hits": results.len(),
            "truncated": truncated,
            "results": results,
        });
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string()),
        )]))
    }

    #[tool(
        description = "Query the model for nodes matching field predicates — the on-demand, structural complement to the text-based `search_model`. Supply `where`: a list of `{field, op, value}` conditions that must ALL hold (AND). Fields and operators compose freely, so any node-shape question is expressible without a bespoke flag: empty symbols = `[{field:'kind',op:'eq',value:'symbol'},{field:'empty',op:'eq',value:true}]`; under-decomposed components = `[{field:'kind',op:'eq',value:'component'},{field:'childCount',op:'eq',value:0}]`; external systems = `[{field:'kind',op:'eq',value:'system'},{field:'external',op:'eq',value:true}]`. Scope to a subtree with `under`. Returns each node's id, kind, name, breadcrumb path, and responsibility/property counts. Use this to find nodes by SHAPE instead of reading the raw `.scry` file. Capped at 200 hits."
    )]
    fn query_model(
        &self,
        Parameters(req): Parameters<QueryModelRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = resolve_model_ref(req.project.as_deref())?;
        let model = match read_layer(&model_ref, req.layer) {
            Ok(m) => m,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to read model at {}: {}",
                    model_ref, e
                ))]));
            }
        };
        // Keep the legacy committed-model baseline fresh only when committed was actually
        // read; reading the plan (the default) must never overwrite the committed snapshot.
        if matches!(req.layer, Layer::Committed) {
            let _ = scryer_core::save_baseline_at(&model_ref, &model);
        }

        if req.conditions.is_empty() {
            return Ok(CallToolResult::error(vec![Content::text(
                "query_model needs at least one condition in `where`. For the full tree use read_model.",
            )]));
        }

        // `under`: restrict to the subtree rooted at the given node id.
        let scope: Option<HashSet<String>> = match req.under.as_deref() {
            Some(root) => {
                if !model.nodes.iter().any(|n| n.id == root) {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "Node '{}' not found",
                        root
                    ))]));
                }
                let mut ids: HashSet<String> = HashSet::new();
                ids.insert(root.to_string());
                let mut frontier = vec![root.to_string()];
                while let Some(id) = frontier.pop() {
                    for child in model.nodes.iter().filter(|n| n.parent_id.as_deref() == Some(&id)) {
                        if ids.insert(child.id.clone()) {
                            frontier.push(child.id.clone());
                        }
                    }
                }
                Some(ids)
            }
            None => None,
        };

        // Child counts, computed once (childCount is a queryable field).
        let mut child_count: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
        for n in &model.nodes {
            if let Some(p) = n.parent_id.as_deref() {
                *child_count.entry(p).or_insert(0) += 1;
            }
        }

        const CAP: usize = 200;
        let mut hits: Vec<serde_json::Value> = Vec::new();
        let mut truncated = false;
        for n in &model.nodes {
            if scope.as_ref().is_some_and(|s| !s.contains(&n.id)) {
                continue;
            }
            // Every condition must hold. A malformed condition (unknown field /
            // op, or a type mismatch) aborts the whole query with guidance.
            let mut matches = true;
            for c in &req.conditions {
                match eval_condition(n, c, child_count.get(n.id.as_str()).copied().unwrap_or(0)) {
                    Ok(true) => {}
                    Ok(false) => {
                        matches = false;
                        break;
                    }
                    Err(e) => {
                        return Ok(CallToolResult::error(vec![Content::text(e)]));
                    }
                }
            }
            if !matches {
                continue;
            }
            if hits.len() >= CAP {
                truncated = true;
                break;
            }
            let mut v = serde_json::json!({
                "id": n.id,
                "kind": kind_str(&n.kind),
                "name": n.name,
                "path": breadcrumb(&model, &n.id),
                "nResp": n.responsibilities.len(),
                "nProps": n.properties.len(),
                // surfaced only when true (strip_fields_compact drops the null)
                "empty": if scryer_core::is_node_empty(n) { serde_json::json!(true) } else { serde_json::Value::Null },
            });
            strip_fields_compact(&mut v);
            hits.push(v);
        }

        let payload = serde_json::json!({
            "hits": hits.len(),
            "truncated": truncated,
            "results": hits,
        });
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string()),
        )]))
    }

    #[tool(
        description = "What code has CHANGED since the model was last reconciled — the code→model drift scope. Cheap and deterministic (file mtimes + git diff, no semantic judgment): returns the boundary-owning nodes whose code changed and the exact `changedFiles` under each, so you know where to re-examine. A changed file is NOT a verdict that the model drifted — it only means \"re-check this scope.\" The loop: for each scope, `read_model {node}` to load its claims, compare them against what the changed code now does, then call `flag_drift` to record undescribed behaviour (→ vagrant) and stale claims (→ `changed`). When you have examined every scope, call `reconcile_drift` to advance the anchor so the same changes don't resurface. A model with no reconcile anchor yet (e.g. just built through these tools) is seeded as in-sync as of now and reports clean — real drift surfaces once code changes after that."
    )]
    fn get_drift(
        &self,
        Parameters(req): Parameters<GetDriftRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = resolve_model_ref(req.project.as_deref())?;
        let model = match scryer_core::read_model_at(&model_ref) {
            Ok(m) => m,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to read model at {}: {}",
                    model_ref, e
                ))]));
            }
        };
        let project = model_ref.project_path();

        // A model that has never been reconciled has no `.sync` anchor, so the
        // baseline defaults to epoch 0 and EVERY file reads as "changed since
        // reconcile" — flagging every boundary-owning node as drift, forever,
        // against no real baseline. Seed the anchor to now (treat the model as
        // in-sync as of this moment) and report clean; real drift then surfaces
        // once code changes after this point. Mirrors the in-app `get_drift_status`
        // bootstrap — models built through these MCP tools land here, since only
        // the in-app build and `reconcile_drift` write the anchor.
        if !model_ref.sync_path().exists() {
            let _ = scryer_core::write_sync_state(
                &model_ref,
                &scryer_core::drift::SyncState {
                    reconciled_at: scryer_core::drift::now_secs(),
                    commit: scryer_core::drift::head_commit(project), ..Default::default() },
            );
            let _ = scryer_extract::anchors::write_baseline(&model_ref);
            let payload = serde_json::json!({
                "clean": true,
                "seeded": true,
                "scopes": [],
                "guidance": "No reconcile anchor existed; seeded the model as in-sync as of now. \
                             Drift will surface here once code changes after this point.",
            });
            return Ok(CallToolResult::success(vec![Content::text(
                serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string()),
            )]));
        }

        let sync = scryer_core::read_sync_state(&model_ref);
        let scopes = scryer_core::drift::drifted_scopes(&model, project, &sync);
        let scopes_out: Vec<serde_json::Value> = scopes
            .iter()
            .map(|s| {
                serde_json::json!({
                    "nodeId": s.node_id,
                    "nodeName": s.node_name,
                    "path": breadcrumb(&model, &s.node_id),
                    "changedFiles": s.changed_files,
                })
            })
            .collect();

        let guidance = if scopes_out.is_empty() {
            "No code changed since the last reconcile — the model is in sync. \
             Nothing to flag; no need to reconcile."
                .to_string()
        } else {
            "For each scope: read_model {node} to load its claims, compare them against the \
             changed code, then flag_drift to record undescribed behaviour and stale claims. \
             After examining every scope, call reconcile_drift to advance the anchor."
                .to_string()
        };
        let payload = serde_json::json!({
            "clean": scopes_out.is_empty(),
            "scopes": scopes_out,
            "guidance": guidance,
        });
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string()),
        )]))
    }

    #[tool(
        description = "What model intent is NOT yet reflected in code — the model→code work outstanding. This is the PLAN diff: how the draft (`planned`) diverges from the committed `model`. Each entry names an element (node / responsibility / property / link / group) and what to do: `added` (implement new code), `reworded` (re-implement to the new spec), `moved` (move the code), `repointed` (re-point the relationship), `deleted` (remove the code) — with a breadcrumb path and, for responsibilities, source anchors. Implementing an entry and calling `mark_implemented` folds it from the plan into the committed model. Call this to find what needs implementing or syncing to the codebase."
    )]
    fn get_pending(
        &self,
        Parameters(req): Parameters<GetPendingRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = resolve_model_ref(req.project.as_deref())?;
        let model = match scryer_core::read_model_at(&model_ref) {
            Ok(m) => m,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to read model at {}: {}",
                    model_ref, e
                ))]));
            }
        };
        let planned = match scryer_core::read_planned_at(&model_ref) {
            Ok(p) => p,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to read plan at {}: {}",
                    model_ref, e
                ))]));
            }
        };

        // The work queue IS the plan diff: how the draft diverges from the
        // committed model. Each change is a thing the code must catch up to.
        use scryer_core::diff::{Change, ElementKind};
        let plan = scryer_core::diff::diff(&model, &planned);

        // Vagrant elements are code-discovered drift ("adopt?"), not planned
        // intent ahead of code ("implement!") — they belong in the drift review
        // queue, never the implement queue. A vagrant node and any responsibility
        // it (or an existing node) carries are filtered out here.
        let is_vagrant_node = |id: &str| {
            planned
                .nodes
                .iter()
                .any(|n| n.id == id && n.vagrant == Some(true))
        };
        let is_vagrant_resp = |id: &str| {
            planned
                .nodes
                .iter()
                .flat_map(|n| n.responsibilities.iter())
                .chain(planned.groups.iter().flat_map(|g| g.responsibilities.iter()))
                .any(|r| r.id == id && r.vagrant == Some(true))
        };

        // Resolve a node's breadcrumb against whichever side actually holds it:
        // the draft for added/existing nodes, the committed model for a deletion.
        let breadcrumb_of = |id: &str| -> String {
            if planned.nodes.iter().any(|n| n.id == id) {
                breadcrumb(&planned, id)
            } else {
                breadcrumb(&model, id)
            }
        };

        let (mut to_implement, mut to_reimplement, mut to_move, mut to_delete, mut to_repoint) =
            (0u32, 0u32, 0u32, 0u32, 0u32);
        let mut changes_out: Vec<serde_json::Value> = Vec::new();

        for ch in &plan.changes {
            let vagrant = match ch.kind {
                ElementKind::Node => is_vagrant_node(&ch.id),
                ElementKind::Responsibility => is_vagrant_resp(&ch.id),
                _ => false,
            };
            if vagrant {
                continue;
            }
            for c in &ch.changes {
                match c {
                    Change::Added => to_implement += 1,
                    Change::Reworded { .. } => to_reimplement += 1,
                    Change::Moved { .. } | Change::MembersChanged { .. } => to_move += 1,
                    Change::Deleted => to_delete += 1,
                    Change::Repointed { .. } => to_repoint += 1,
                }
            }

            let mut v = serde_json::to_value(ch).unwrap_or(serde_json::Value::Null);
            match ch.kind {
                ElementKind::Node => {
                    v["path"] = serde_json::Value::String(breadcrumb_of(&ch.id));
                }
                ElementKind::Responsibility => {
                    if let Some(owner) = &ch.owner_id {
                        v["path"] = serde_json::Value::String(breadcrumb_of(owner));
                    }
                    // Source anchors live in the draft for added claims, the
                    // committed model for existing/deleted ones.
                    if let Some(src) = planned
                        .source_map
                        .get(&ch.id)
                        .or_else(|| model.source_map.get(&ch.id))
                    {
                        v["sources"] = serde_json::to_value(src).unwrap_or(serde_json::Value::Null);
                    }
                }
                ElementKind::Property => {
                    if let Some(owner) = &ch.owner_id {
                        v["path"] = serde_json::Value::String(breadcrumb_of(owner));
                    }
                }
                ElementKind::Link | ElementKind::Group => {}
            }
            changes_out.push(v);
        }

        let payload = serde_json::json!({
            "summary": {
                "toImplement": to_implement,
                "toReimplement": to_reimplement,
                "toMove": to_move,
                "toDelete": to_delete,
                "toRepoint": to_repoint,
            },
            "clean": changes_out.is_empty(),
            "changes": changes_out,
        });
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string()),
        )]))
    }

    #[tool(
        description = "The authoritative, BINDING scryer modeling rules — the knowledge base that governs responsibilities, decomposition, symbols, groups, status, and link semantics. These rules decide every modeling judgment: consult them, never infer the conventions from existing nodes. With NO `topic`: the compact index (every rule's id, title, tags) — read it to see what's available. With a `topic` (e.g. \"symbol\", \"group\", \"responsibility altitude\"): the matching rules in full. Pull the relevant rule whenever you're deciding how to model something — what earns a symbol, how to pitch a responsibility, when a group is right."
    )]
    fn get_rules(
        &self,
        Parameters(req): Parameters<GetRulesRequest>,
    ) -> Result<CallToolResult, McpError> {
        use scryer_core::rules;
        let body = match req.topic.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
            None => format!(
                "Modeling rules (index). These are authoritative and binding — pull the full text of \
                 any rule with get_rules{{topic}} before making the related modeling decision.\n\n{}",
                rules::rules_index()
            ),
            Some(topic) => {
                let hits = rules::lookup(topic);
                if hits.is_empty() {
                    format!(
                        "No rule matched '{}'. Pick a topic from the index:\n\n{}",
                        topic,
                        rules::rules_index()
                    )
                } else {
                    rules::render(&hits)
                }
            }
        };
        Ok(CallToolResult::success(vec![Content::text(body)]))
    }

    #[tool(
        description = "Annotated project directory tree. Surfaces manifests ([manifest]), infrastructure configs ([infrastructure]), and environment templates ([environment]). Use before modeling to identify deployable units, data stores, external integrations, and frameworks. Respects .gitignore and skips build output / dependency directories."
    )]
    fn read_codebase(
        &self,
        Parameters(req): Parameters<ReadCodebaseRequest>,
    ) -> Result<CallToolResult, McpError> {
        let path = std::path::Path::new(&req.path);
        match scryer_core::scan::project_structure(path) {
            Ok(tree) => Ok(CallToolResult::success(vec![Content::text(tree)])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e)])),
        }
    }

    #[tool(
        description = "Run the structural validator. Returns a list of warnings: parent-kind mismatches, unknown link endpoints, group members at mixed levels, empty symbols (carrying no responsibility/property/appearance), source-map entries that reference unknown ids, and responsibility mappings whose line range covers the whole enclosing symbol (a range must be a proper subset — drop it to mean the whole definition). A clean run is a post-edit gate, not a lookup — to FIND nodes by shape on demand (e.g. every empty symbol) use `query_model`. Does NOT judge responsibility wording quality."
    )]
    fn validate_model(
        &self,
        Parameters(req): Parameters<ValidateModelRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = resolve_model_ref(req.project.as_deref())?;
        let model = match scryer_core::read_model_at(&model_ref) {
            Ok(m) => m,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to read model at {}: {}",
                    model_ref, e
                ))]));
            }
        };
        let mut warnings = validate::validate(&model);
        warnings.extend(validate::validate_coverage(&model, model_ref.project_path()));
        warnings.extend(scryer_extract::anchors::whole_symbol_warnings(
            &model,
            model_ref.project_path(),
        ));
        if warnings.is_empty() {
            Ok(CallToolResult::success(vec![Content::text(
                "Model is structurally clean.",
            )]))
        } else {
            let mut msg = format!("Model '{}' — {} warning(s):", model_ref, warnings.len());
            for w in &warnings {
                msg.push_str(&format!("\n- {}", w));
            }
            Ok(CallToolResult::success(vec![Content::text(msg)]))
        }
    }

    #[tool(
        description = "The model's observability report — deterministic, no semantic judgment. Per node: own + subtree rollups of responsibility/property counts, vagrant/stale flags, and anchor coverage (anchorable = any committed claim on LEAF nodes; claims on structural nodes are discharged through their subtree and are never 'unmapped'). Plus: anchor observations from the git-free fingerprint check — `changed` (the anchored span's content differs from what the model last saw), `broken` (the symbol is gone), `fileMissing` — with moved-but-unchanged symbols silently re-anchored, and a declared-link audit against the extracted import graph (edge_count 0 = asserted-only; 'unmodeled' = sibling pairs the code connects but no link declares). Pass node_id to scope to one subtree with per-child summaries; omit it for the whole-model summary. Use this to decide WHERE work is needed (unmapped claims, vagrant flags, dark links) before reading full subtrees."
    )]
    fn get_health(
        &self,
        Parameters(req): Parameters<GetHealthRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = resolve_model_ref(req.project.as_deref())?;
        let model = match scryer_core::read_model_at(&model_ref) {
            Ok(m) => m,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to read model at {}: {}",
                    model_ref, e
                ))]));
            }
        };
        let project = model_ref.project_path();

        // Same no-anchor bootstrap as get_drift: a model never reconciled would
        // read as "everything drifted" against no baseline. Seeding also writes
        // the anchor fingerprint baseline (git-free content snapshot).
        let anchor_check = if model_ref.sync_path().exists() {
            // May silently re-anchor moved symbols — re-read the model after.
            scryer_extract::anchors::check_anchors(&model_ref).unwrap_or_default()
        } else {
            let _ = scryer_core::write_sync_state(
                &model_ref,
                &scryer_core::drift::SyncState {
                    reconciled_at: scryer_core::drift::now_secs(),
                    commit: scryer_core::drift::head_commit(project), ..Default::default() },
            );
            let _ = scryer_extract::anchors::write_baseline(&model_ref);
            scryer_extract::anchors::AnchorCheck::default()
        };
        let model = if anchor_check.reanchored > 0 {
            scryer_core::read_model_at(&model_ref).unwrap_or(model)
        } else {
            model
        };

        // Boundary darkness needs the extractor's file inventory; health here
        // covers everything model-derivable (counts, discharge, coverage).
        let health = scryer_core::health::compute_health(&model, None);

        // The import graph is cached by builds / the app's health refresh; when
        // absent the link audit is simply omitted rather than guessed.
        let derived = scryer_core::build_edges::read_build_edges(&model_ref.build_edges_path())
            .map(|edges| scryer_core::build_edges::derive_graph(&model, &edges));

        let counts_json = |c: &scryer_core::health::HealthCounts| {
            serde_json::to_value(c).unwrap_or_default()
        };

        let payload = match req.node_id.as_deref() {
            Some(node_id) => {
                let Some(node) = model.nodes.iter().find(|n| n.id == node_id) else {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "Node '{}' not found",
                        node_id
                    ))]));
                };
                let nh = health.nodes.get(node_id);

                // One level down: each child's subtree summary, so altitude
                // decisions don't require walking the whole tree.
                let children: Vec<serde_json::Value> = model
                    .nodes
                    .iter()
                    .filter(|n| n.parent_id.as_deref() == Some(node_id))
                    .map(|n| {
                        serde_json::json!({
                            "id": n.id,
                            "name": n.name,
                            "kind": kind_str(&n.kind),
                            "subtree": health.nodes.get(&n.id).map(|h| counts_json(&h.subtree)),
                        })
                    })
                    .collect();

                // Subtree membership for filtering drift to this scope.
                let mut subtree_ids: HashSet<&str> = HashSet::new();
                let mut frontier = vec![node_id];
                while let Some(id) = frontier.pop() {
                    if !subtree_ids.insert(id) {
                        continue;
                    }
                    frontier.extend(
                        model
                            .nodes
                            .iter()
                            .filter(|n| n.parent_id.as_deref() == Some(id))
                            .map(|n| n.id.as_str()),
                    );
                }
                let drift_here: Vec<&scryer_extract::anchors::AnchorObservation> = anchor_check
                    .observations
                    .iter()
                    .filter(|d| subtree_ids.contains(d.host_id.as_str()))
                    .collect();

                let links_here = derived.as_ref().map(|g| {
                    let audited: Vec<serde_json::Value> = model
                        .links
                        .iter()
                        .filter(|l| l.src == node_id || l.dst == node_id)
                        .map(|l| {
                            let backed = g
                                .link_audit
                                .iter()
                                .find(|a| a.link_id == l.id)
                                .map(|a| a.edge_count)
                                .unwrap_or(0);
                            serde_json::json!({
                                "id": l.id, "src": l.src, "dst": l.dst,
                                "label": l.label, "edgeCount": backed,
                            })
                        })
                        .collect();
                    audited
                });
                let unmodeled_here = derived.as_ref().map(|g| {
                    g.unmodeled
                        .iter()
                        .filter(|e| {
                            subtree_ids.contains(e.src.as_str())
                                || subtree_ids.contains(e.dst.as_str())
                        })
                        .collect::<Vec<_>>()
                });

                serde_json::json!({
                    "nodeId": node.id,
                    "name": node.name,
                    "kind": kind_str(&node.kind),
                    "own": nh.map(|h| counts_json(&h.own)),
                    "subtree": nh.map(|h| counts_json(&h.subtree)),
                    "children": children,
                    "anchors": drift_here,
                    "links": links_here,
                    "unmodeled": unmodeled_here,
                })
            }
            None => {
                let roots: Vec<serde_json::Value> = model
                    .nodes
                    .iter()
                    .filter(|n| n.parent_id.is_none())
                    .map(|n| {
                        serde_json::json!({
                            "id": n.id,
                            "name": n.name,
                            "kind": kind_str(&n.kind),
                            "subtree": health.nodes.get(&n.id).map(|h| counts_json(&h.subtree)),
                        })
                    })
                    .collect();
                let asserted_only = derived
                    .as_ref()
                    .map(|g| g.link_audit.iter().filter(|a| a.edge_count == 0).count());
                serde_json::json!({
                    "totals": counts_json(&health.totals),
                    "roots": roots,
                    "anchors": anchor_check.observations,
                    "reanchored": anchor_check.reanchored,
                    "assertedOnlyLinks": asserted_only,
                    "unmodeled": derived.as_ref().map(|g| &g.unmodeled),
                    "edgeGraph": if derived.is_some() { "from last build's dependency cache" } else { "absent — run a model build (or the app's health refresh) to derive the link audit" },
                })
            }
        };

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string()),
        )]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::handler::server::wrapper::Parameters;
    use scryer_core::{Kind, ModelRef, Node, Responsibility, ScryModel};

    fn node(id: &str, kind: Kind, name: &str, parent: Option<&str>) -> Node {
        Node {
            id: id.into(),
            kind,
            name: name.into(),
            vagrant: None,
            stale: None,
            parent_id: parent.map(|p| p.into()),
            external: None,
            technology: None,
            description: None,
            responsibilities: Vec::new(),
            properties: Vec::new(),
            icon: None,
            visual: None,
            appearance: None,
            notes: None,
        }
    }

    fn resp(id: &str, statement: &str) -> Responsibility {
        Responsibility {
            id: id.into(),
            statement: statement.into(),
            vagrant: None,
            stale: None,
            directives: Vec::new(),
            last_touched_at: None,
        }
    }

    /// Build a tiny on-disk model: System > Container > Component > two symbols.
    fn temp_project() -> (ScryerServer, tempfile::TempDir, String) {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());
        let mut m = ScryModel::new();
        m.nodes.push(node("node-1", Kind::System, "Acme", None));
        m.nodes.push(node("node-2", Kind::Container, "API", Some("node-1")));
        m.nodes.push(node("node-3", Kind::Component, "Auth", Some("node-2")));
        let mut sym = node("node-4", Kind::Symbol, "verify_token", Some("node-3"));
        sym.responsibilities = vec![resp("resp-1", "rejects forged credentials")];
        m.nodes.push(sym);
        m.nodes
            .push(node("node-5", Kind::Symbol, "hash_password", Some("node-3")));
        scryer_core::write_model_at(&model_ref, &m).unwrap();
        let project = dir.path().to_string_lossy().to_string();
        (ScryerServer::new(), dir, project)
    }

    /// Pull the single text block out of a tool result and parse it as JSON.
    fn result_json(r: &CallToolResult) -> serde_json::Value {
        let content = serde_json::to_value(&r.content).unwrap();
        let text = content[0]["text"].as_str().expect("text content");
        serde_json::from_str(text).expect("text is JSON")
    }

    #[test]
    fn read_model_overview_excludes_symbols_then_drills_in() {
        let (server, _dir, project) = temp_project();
        // No node => architecture overview: tree down to components, NO symbols.
        let r = server
            .read_model(Parameters(ReadModelRequest {
                project: Some(project.clone()),
                node: None,
                layer: Layer::Plan,
            }))
            .unwrap();
        let v = result_json(&r);
        assert_eq!(v["view"], "overview");
        // System > Container > Component is present; the symbol is not.
        let comp = &v["overview"][0]["children"][0]["children"][0];
        assert_eq!(comp["id"], "node-3");
        let dump = serde_json::to_string(&v).unwrap();
        assert!(!dump.contains("node-4")); // symbol excluded from overview
        assert!(!dump.contains("rejects forged credentials")); // no bodies

        // Scope to the component => full subtree detail incl. the symbol + body.
        let r = server
            .read_model(Parameters(ReadModelRequest {
                project: Some(project),
                node: Some("node-3".into()),
                layer: Layer::Plan,
            }))
            .unwrap();
        let v = result_json(&r);
        let dump = serde_json::to_string(&v).unwrap();
        assert!(dump.contains("node-4"));
        assert!(dump.contains("rejects forged credentials"));
    }

    #[test]
    fn read_model_unknown_node_errors() {
        let (server, _dir, project) = temp_project();
        let r = server
            .read_model(Parameters(ReadModelRequest {
                project: Some(project),
                node: Some("node-999".into()),
                layer: Layer::Plan,
            }))
            .unwrap();
        assert!(serde_json::to_string(&r.content).unwrap().contains("not found"));
    }

    #[test]
    fn read_model_subtree_too_large_returns_child_skeleton() {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());
        let mut m = ScryModel::new();
        m.nodes.push(node("node-1", Kind::System, "Acme", None));
        m.nodes.push(node("node-2", Kind::Component, "Fat", Some("node-1")));
        // Many fat symbols under the component to push its subtree past the guard.
        for i in 0..400 {
            let mut s = node(
                &format!("node-{}", i + 3),
                Kind::Symbol,
                &format!("symbol_{i}_with_a_deliberately_long_identifier"),
                Some("node-2"),
            );
            s.description = Some("a".repeat(200));
            m.nodes.push(s);
        }
        scryer_core::write_model_at(&model_ref, &m).unwrap();
        let server = ScryerServer::new();
        let project = dir.path().to_string_lossy().to_string();

        // Drilling into the oversize component degrades to a child skeleton.
        let r = server
            .read_model(Parameters(ReadModelRequest {
                project: Some(project),
                node: Some("node-2".into()),
                layer: Layer::Plan,
            }))
            .unwrap();
        let v = result_json(&r);
        assert_eq!(v["view"], "overview");
        assert!(v["note"].as_str().unwrap().contains("too large"));
        assert!(v["children"].as_array().unwrap().len() == 400);
        // skeleton only — no responsibility/source bodies
        assert!(v["children"][0].get("responsibilities").is_none());
    }

    #[test]
    fn search_matches_responsibility_and_reports_path() {
        let (server, _dir, project) = temp_project();
        let r = server
            .search_model(Parameters(SearchModelRequest {
                project: Some(project),
                query: "forged".into(),
                kind: None,
                layer: Layer::Plan,
            }))
            .unwrap();
        let v = result_json(&r);
        assert_eq!(v["hits"], 1);
        assert_eq!(v["results"][0]["id"], "node-4");
        assert_eq!(v["results"][0]["path"], "Acme / API / Auth / verify_token");
        assert_eq!(v["results"][0]["matched"][0]["in"], "responsibility");
    }

    #[test]
    fn get_pending_reports_the_plan_diff() {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());

        // Committed model: a system with a Billing component (two claims) and a
        // Legacy component.
        let mut m = ScryModel::new();
        m.nodes.push(node("node-1", Kind::System, "Acme", None));
        let mut c = node("node-2", Kind::Component, "Billing", Some("node-1"));
        c.responsibilities = vec![
            resp("resp-impl", "charges the card"),
            resp("resp-keep", "settles nightly"),
        ];
        m.nodes.push(c);
        m.nodes.push(node("node-3", Kind::Component, "Legacy", Some("node-1")));
        scryer_core::write_model_at(&model_ref, &m).unwrap();

        // Plan (draft): add a claim, reword another, leave one untouched, and
        // delete the Legacy node.
        let mut planned = m.clone();
        planned.nodes[1]
            .responsibilities
            .push(resp("resp-prop", "issues refunds")); // Added
        planned.nodes[1].responsibilities[0].statement = "charges the card and logs it".into(); // Reworded
        planned.nodes.retain(|n| n.id != "node-3"); // Deleted
        // a source anchor for the new claim (lives in the draft's source map)
        planned.source_map.insert(
            "resp-prop".into(),
            vec![scryer_core::SourceLocation {
                pattern: "src/billing.rs".into(),
                symbol: Some("refund".into()),
                line: None,
                end_line: None,
                command: None,
            }],
        );
        scryer_core::write_planned_at(&model_ref, &planned).unwrap();

        let server = ScryerServer::new();
        let r = server
            .get_pending(Parameters(GetPendingRequest {
                project: Some(dir.path().to_string_lossy().to_string()),
            }))
            .unwrap();
        let v = result_json(&r);

        assert_eq!(v["clean"], false);
        assert_eq!(v["summary"]["toImplement"], 1); // resp-prop added
        assert_eq!(v["summary"]["toReimplement"], 1); // resp-impl reworded
        assert_eq!(v["summary"]["toDelete"], 1); // node-3 deleted

        let dump = serde_json::to_string(&v).unwrap();
        assert!(dump.contains("issues refunds")); // added claim surfaced
        assert!(dump.contains("charges the card and logs it")); // reworded (new text)
        assert!(dump.contains("src/billing.rs")); // source anchor carried through
        assert!(dump.contains("node-3")); // deletion surfaced
        // the untouched claim is not part of the plan
        assert!(!dump.contains("settles nightly"));
    }

    #[test]
    fn query_finds_empty_symbols_and_composes_predicates() {
        let (server, _dir, project) = temp_project();
        let run = |conds: serde_json::Value, under: Option<&str>| {
            let req: QueryModelRequest = serde_json::from_value(serde_json::json!({
                "project": project,
                "where": conds,
                "under": under,
            }))
            .unwrap();
            result_json(&server.query_model(Parameters(req)).unwrap())
        };

        // node-5 (hash_password) is an empty symbol; node-4 (verify_token) is not.
        let v = run(
            serde_json::json!([
                {"field": "kind", "op": "eq", "value": "symbol"},
                {"field": "empty", "op": "eq", "value": true},
            ]),
            None,
        );
        assert_eq!(v["hits"], 1);
        assert_eq!(v["results"][0]["id"], "node-5");
        assert_eq!(v["results"][0]["empty"], true);

        // Numeric op: components with zero children (under-decomposed). None here —
        // the only component (node-3) has two symbols.
        let v = run(
            serde_json::json!([
                {"field": "kind", "op": "eq", "value": "component"},
                {"field": "childCount", "op": "eq", "value": 0},
            ]),
            None,
        );
        assert_eq!(v["hits"], 0);

        // `under` scopes to a subtree: both symbols live under the component.
        let v = run(
            serde_json::json!([{"field": "kind", "op": "eq", "value": "symbol"}]),
            Some("node-3"),
        );
        assert_eq!(v["hits"], 2);

        // Empty `where` is rejected.
        let req: QueryModelRequest = serde_json::from_value(serde_json::json!({
            "project": project, "where": [],
        }))
        .unwrap();
        let r = server.query_model(Parameters(req)).unwrap();
        assert!(r.is_error.unwrap_or(false));

        // A bad operator on a string field fails loudly.
        let req: QueryModelRequest = serde_json::from_value(serde_json::json!({
            "project": project,
            "where": [{"field": "name", "op": "gt", "value": "x"}],
        }))
        .unwrap();
        let r = server.query_model(Parameters(req)).unwrap();
        assert!(r.is_error.unwrap_or(false));
    }

    #[test]
    fn search_ands_terms_and_filters_by_kind() {
        let (server, _dir, project) = temp_project();
        // both terms present, but on different nodes => no single-node match
        let r = server
            .search_model(Parameters(SearchModelRequest {
                project: Some(project.clone()),
                query: "verify hash".into(),
                kind: None,
                layer: Layer::Plan,
            }))
            .unwrap();
        assert_eq!(result_json(&r)["hits"], 0);
        // kind filter excludes the matching component ("Auth" is a component)
        let r = server
            .search_model(Parameters(SearchModelRequest {
                project: Some(project),
                query: "Auth".into(),
                kind: Some("symbol".into()),
                layer: Layer::Plan,
            }))
            .unwrap();
        assert_eq!(result_json(&r)["hits"], 0);
    }

    #[test]
    fn search_fuzzy_matches_typo_and_tags_match_kind() {
        let (server, _dir, project) = temp_project();
        // "verfy" is a typo of the symbol name "verify_token" — no substring hit,
        // so it only lands via edit-distance similarity.
        let r = server
            .search_model(Parameters(SearchModelRequest {
                project: Some(project),
                query: "verfy".into(),
                kind: None,
                layer: Layer::Plan,
            }))
            .unwrap();
        let v = result_json(&r);
        assert_eq!(v["hits"], 1);
        assert_eq!(v["results"][0]["id"], "node-4");
        assert_eq!(v["results"][0]["matched"][0]["in"], "name");
        assert_eq!(v["results"][0]["matched"][0]["match"], "fuzzy");
    }

    #[test]
    fn search_unrelated_term_does_not_fuzzy_match() {
        let (server, _dir, project) = temp_project();
        let r = server
            .search_model(Parameters(SearchModelRequest {
                project: Some(project),
                query: "elephant".into(),
                kind: None,
                layer: Layer::Plan,
            }))
            .unwrap();
        assert_eq!(result_json(&r)["hits"], 0);
    }

    #[test]
    fn search_ranks_exact_above_fuzzy() {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());
        let mut m = ScryModel::new();
        m.nodes.push(node("node-1", Kind::System, "Acme", None));
        // Exact substring hit on a responsibility statement.
        let mut exact = node("node-2", Kind::Component, "Billing", Some("node-1"));
        exact.responsibilities = vec![resp("r1", "charges the card")];
        m.nodes.push(exact);
        // Fuzzy-only hit: the name "charge" is one edit from the query "charges".
        m.nodes
            .push(node("node-3", Kind::Component, "charge", Some("node-1")));
        scryer_core::write_model_at(&model_ref, &m).unwrap();
        let project = dir.path().to_string_lossy().to_string();
        let server = ScryerServer::new();

        let r = server
            .search_model(Parameters(SearchModelRequest {
                project: Some(project),
                query: "charges".into(),
                kind: None,
                layer: Layer::Plan,
            }))
            .unwrap();
        let v = result_json(&r);
        assert_eq!(v["hits"], 2);
        assert_eq!(v["results"][0]["id"], "node-2");
        assert_eq!(v["results"][0]["matched"][0]["match"], "exact");
        assert_eq!(v["results"][1]["id"], "node-3");
        assert_eq!(v["results"][1]["matched"][0]["match"], "fuzzy");
    }

    #[test]
    fn get_drift_seeds_then_surfaces() {
        use scryer_core::Source;

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let model_ref = ModelRef::ProjectLocal(root.to_path_buf());
        std::fs::create_dir_all(root.join("api/src")).unwrap();
        std::fs::write(root.join("api/src/server.rs"), "fn v1() {}").unwrap();

        let mut m = ScryModel::new();
        m.nodes.push(node("node-1", Kind::Container, "API", None));
        m.boundaries
            .insert("node-1".into(), vec![Source { pattern: "api/**/*".into(), comment: None }]);
        scryer_core::write_model_at(&model_ref, &m).unwrap();
        let project = root.to_string_lossy().to_string();
        let server = ScryerServer::new();

        // First call: no anchor exists → seed in-sync, report clean (not noise).
        let v = result_json(
            &server
                .get_drift(Parameters(GetDriftRequest { project: Some(project.clone()) }))
                .unwrap(),
        );
        assert_eq!(v["clean"], true);
        assert_eq!(v["seeded"], true);
        assert!(model_ref.sync_path().exists());

        // Touch a boundary file AFTER the seed → its scope surfaces.
        std::thread::sleep(std::time::Duration::from_millis(1100));
        std::fs::write(root.join("api/src/server.rs"), "fn v2() {}").unwrap();
        let v = result_json(
            &server
                .get_drift(Parameters(GetDriftRequest { project: Some(project.clone()) }))
                .unwrap(),
        );
        assert_eq!(v["clean"], false);
        assert_eq!(v["scopes"][0]["nodeId"], "node-1");
        assert!(v["scopes"][0]["changedFiles"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f == "api/src/server.rs"));
    }

    /// get_health: structural claims are discharged (never unmapped), leaf
    /// blind spots roll up, and the node-scoped report carries child summaries.
    #[test]
    fn get_health_reports_discharge_and_rollup() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let model_ref = ModelRef::ProjectLocal(root.to_path_buf());

        let mut m = ScryModel::new();
        let mut sys = node("sys", Kind::System, "Sys", None);
        sys.responsibilities.push(Responsibility {
            id: "r-sys".into(),
            statement: "orchestrates everything".into(),
            vagrant: None,
            stale: None,
            directives: Vec::new(),
            last_touched_at: None,
        });
        m.nodes.push(sys);
        let mut leaf = node("leaf", Kind::Symbol, "leafFn", Some("sys"));
        leaf.responsibilities.push(Responsibility {
            id: "r-leaf".into(),
            statement: "does the thing".into(),
            vagrant: None,
            stale: None,
            directives: Vec::new(),
            last_touched_at: None,
        });
        m.nodes.push(leaf);
        scryer_core::write_model_at(&model_ref, &m).unwrap();

        let server = ScryerServer::new();
        let project = root.to_string_lossy().to_string();

        // Whole-model summary: the system's claim is discharged structurally;
        // the leaf's unanchored claim is the only blind spot.
        let v = result_json(
            &server
                .get_health(Parameters(GetHealthRequest {
                    project: Some(project.clone()),
                    node_id: None,
                }))
                .unwrap(),
        );
        assert_eq!(v["totals"]["responsibilities"], 2);
        assert_eq!(v["totals"]["anchorable"], 1);
        assert_eq!(v["totals"]["unmapped"], 1);

        // Node scope: child summaries surface the leaf's gap at the parent.
        let v = result_json(
            &server
                .get_health(Parameters(GetHealthRequest {
                    project: Some(project),
                    node_id: Some("sys".into()),
                }))
                .unwrap(),
        );
        assert_eq!(v["own"]["unmapped"], 0, "structural claim never unmapped");
        assert_eq!(v["subtree"]["unmapped"], 1);
        assert_eq!(v["children"][0]["id"], "leaf");
        assert_eq!(v["children"][0]["subtree"]["unmapped"], 1);
    }
}
