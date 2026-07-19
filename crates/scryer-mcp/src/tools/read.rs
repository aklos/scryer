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
use std::collections::{BTreeMap, HashSet};

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
        // The concern registry — the model's cross-cutting vocabulary. Reuse
        // these slugs when tagging responsibilities (rule 20).
        "concerns": model.concerns,
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

    // Backing tests (claim → verify locations), scoped like the source map.
    let verify_map: serde_json::Map<String, serde_json::Value> = model
        .verify_map
        .iter()
        .filter(|(k, _)| subtree_resp_ids.contains(k.as_str()))
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

    // Registry entries for the concerns tagged within this subtree, so a
    // reader can resolve each responsibility's `concern` slug in place.
    let used_concerns: HashSet<&str> = subtree_nodes
        .iter()
        .flat_map(|n| n.responsibilities.iter())
        .filter_map(|r| r.concern.as_deref())
        .collect();
    let concerns: Vec<_> =
        model.concerns.iter().filter(|c| used_concerns.contains(c.slug.as_str())).collect();

    Ok(serde_json::json!({
        "node": subtree_nodes.iter().find(|n| n.id == node_id),
        "descendants": subtree_nodes.iter().filter(|n| n.id != node_id).collect::<Vec<_>>(),
        "concerns": concerns,
        "internalLinks": internal_links,
        "externalLinks": external_links,
        "contextNodes": context_nodes,
        "referencesForChildren": references_for_children,
        // HOW-constraints carried down from ancestors above this subtree. The
        // node's OWN directives are on `node.directives`; together they are the
        // full binding set the implementation must satisfy. Each descendant
        // additionally inherits this node's own directives (visible above).
        "inheritedDirectives": scryer_core::inherited_directives(model, node_id),
        "sourceMap": source_map,
        "verifyMap": verify_map,
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

/// Normalize a request path to the model's convention: project-relative,
/// `/`-separated. An absolute path inside the project is accepted.
fn normalize_project_rel(model_ref: &scryer_core::ModelRef, path: &str) -> String {
    let mut file = path.replace('\\', "/");
    let root = model_ref.project_path().to_string_lossy().replace('\\', "/");
    if let Some(rest) = file.strip_prefix(root.as_str()) {
        file = rest.trim_start_matches('/').to_string();
    }
    file.trim_start_matches("./").to_string()
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

        let model = match read_layer(&model_ref, req.layer) {
            Ok(m) => m,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(read_fail("model", &model_ref, &e))]));
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
                return Ok(CallToolResult::error(vec![Content::text(read_fail("model", &model_ref, &e))]));
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
        description = "Reverse lookup from code into the model — the tool to reach for when a task starts from a FILE ('fix the save race in useModelStorage.ts') rather than from the model. Given a project-relative `file` (and optional `symbol` to narrow to one definition), returns the intent governing that location in ONE call: every claim anchored there (with stale/vagrant flags, and `verifiedBy` listing its backing tests when linked; locating a TEST file returns the claims that test backs, marked `viaTest`), the owning node chain finest-first with its breadcrumb, the boundary owner of the code region, the BINDING directives (the claim's own, the finest node's, and everything inherited from its ancestors), any pending plan entries touching the located elements, and `scopeHealth` — the owning node's own + subtree coverage counts and completeness, so you see how well-modeled the surrounding scope is, not just what it intends. Reads the working view, so claims you just authored are visible. A file with no anchored claims still reports its boundary owner — the node whose intent governs the region. When you already know the file you're working in, one `locate` call replaces the search_model → read_model orientation dance."
    )]
    fn locate(
        &self,
        Parameters(req): Parameters<LocateRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = resolve_model_ref(req.project.as_deref())?;

        let file = normalize_project_rel(&model_ref, &req.file);

        // The shared payload generator: working-view locate (plan-authored
        // claims visible, committed anchors overlaid), breadcrumb, and pending
        // plan entries scoped to the located elements — so a code-first agent
        // sees this file's outstanding intent without a get_pending sweep.
        let report = match scryer_core::locate::locate_at(&model_ref, &file, req.symbol.as_deref())
        {
            Ok(r) => r,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(read_fail("model", &model_ref, &e))]));
            }
        };
        let res = report.result;

        // Steer the next action instead of returning a bare empty result.
        let note = if res.claims.is_empty() {
            Some(match &res.boundary_owner {
                Some(b) => format!(
                    "No claim is anchored to {file} — dark code under '{}' ({}). Read that node \
                     with read_model {{node: \"{}\"}} for the intent governing this region; when \
                     you implement modeled behaviour here, anchor its claim with update_source_map.",
                    b.name, b.id, b.id
                ),
                None => format!(
                    "No model intent maps {file}: no claim anchors to it and no boundary owns it. \
                     If work here changes behaviour, find the governing node with search_model and \
                     plan the change there first; use update_source_map (or add_symbol) to bring \
                     the file under the model."
                ),
            })
        } else if req.symbol.is_some() && !res.symbol_matched {
            Some(format!(
                "Symbol '{}' matched no anchor in {file}; showing the whole file's claims.",
                req.symbol.as_deref().unwrap_or_default()
            ))
        } else {
            None
        };

        // Scope health: the owning node's own + subtree coverage counts and its
        // completeness, so a `locate` shows not just the intent governing this
        // file but how well-modeled the surrounding scope is. Scoped to the finest
        // governing node (owner_chain.first()). Side-effect-free — no anchor
        // fingerprint check (that writes/re-anchors and is get_health's job);
        // completeness resolves anchors against the file inventory, so a symbol
        // whose file exists but whose content broke still reads as covered here.
        let scope_health = res.owner_chain.first().and_then(|owner| {
            let committed = scryer_core::read_model_at(&model_ref).ok()?;
            let planned =
                scryer_core::read_planned_at(&model_ref).unwrap_or_else(|_| committed.clone());
            let health = scryer_core::health::compute_health(&committed, Some(&planned), None);
            let nh = health.nodes.get(&owner.id)?;
            let files = scryer_extract::list_project_files(model_ref.project_path());
            let completeness = scryer_core::health::resolve_completeness(
                &committed,
                &planned,
                &files,
                &HashSet::new(),
            )
            .get(&owner.id)
            .cloned();
            Some(serde_json::json!({
                "nodeId": owner.id,
                "name": owner.name,
                "own": serde_json::to_value(&nh.own).unwrap_or_default(),
                "subtree": serde_json::to_value(&nh.subtree).unwrap_or_default(),
                "completeness": completeness,
            }))
        });

        let mut payload = serde_json::json!({
            "file": file,
            "symbol": req.symbol,
            "symbolMatched": req.symbol.as_deref().map(|_| res.symbol_matched),
            "path": report.path,
            "ownerChain": res.owner_chain,
            "boundaryOwner": res.boundary_owner,
            "claims": res.claims,
            "ownDirectives": res.own_directives,
            "inheritedDirectives": res.inherited_directives,
            "pending": report.pending,
            "scopeHealth": scope_health,
            "note": note,
        });
        strip_fields_compact(&mut payload);
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string()),
        )]))
    }

    #[tool(
        description = "One-call task-scoped orientation — start here when you have a TASK ('fix the save race in useModelStorage') instead of a model question; it replaces the get_health / get_rules / search_model / read_model dance. Pass the task in a few words and/or the project-relative files it touches. Returns, scoped to what you're about to do: per file the governing node chain, anchored claims, and BINDING directives (own + inherited, same as `locate`); per task the best-matching model nodes with their responsibilities and inherited directives; the pending plan entries touching that scope (the work queue you may be executing); the drift scopes inside it (code changed since the last reconcile); up to 3 matching modeling rules IN FULL; a `phase` verdict — plan-execution (pending intent exists: implement it), reconcile (code changed outside the plan: compare and flag_drift), or free — and the whole-loop `state` line. The whole-model tools (get_health, read_model, get_pending) remain for model-building sessions; orient is the front door for coding sessions."
    )]
    fn orient(
        &self,
        Parameters(req): Parameters<OrientRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = resolve_model_ref(req.project.as_deref())?;
        let task = req.task.as_deref().map(str::trim).filter(|t| !t.is_empty());
        let files: Vec<String> = req.files.unwrap_or_default();
        if task.is_none() && files.is_empty() {
            return Ok(CallToolResult::error(vec![Content::text(
                "Give `task` (a few words) and/or `files` — orient scopes the model to what \
                 you are about to do. For the whole model use read_model / get_health.",
            )]));
        }

        let committed = match scryer_core::read_model_at(&model_ref) {
            Ok(m) => m,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(read_fail(
                    "model", &model_ref, &e,
                ))]));
            }
        };
        let planned =
            scryer_core::read_planned_at(&model_ref).unwrap_or_else(|_| committed.clone());
        let working = scryer_core::working_view(&committed, &planned);

        // The governing set, in two grains: `finest` nodes grow to their
        // descendants (their subtree IS the task's scope); `chain` ancestors
        // join by id only — growing descendants from a chain that reaches the
        // root system would make every scope the whole model.
        let mut finest: HashSet<String> = HashSet::new();
        let mut chain: HashSet<String> = HashSet::new();

        // Files → reverse lookup, same payload core as `locate`.
        let mut files_out: Vec<serde_json::Value> = Vec::new();
        for f in files.iter().take(10) {
            let file = normalize_project_rel(&model_ref, f);
            match scryer_core::locate::locate_at(&model_ref, &file, None) {
                Ok(report) => {
                    if let Some(first) = report.result.owner_chain.first() {
                        finest.insert(first.id.clone());
                    }
                    for o in &report.result.owner_chain {
                        chain.insert(o.id.clone());
                    }
                    if let Some(b) = &report.result.boundary_owner {
                        // The only handle on an unmapped file is its boundary
                        // owner — then that whole region is the scope.
                        if report.result.owner_chain.is_empty() {
                            finest.insert(b.id.clone());
                        }
                        chain.insert(b.id.clone());
                    }
                    let mut v = serde_json::json!({
                        "file": file,
                        "path": report.path,
                        "ownerChain": report.result.owner_chain,
                        "claims": report.result.claims,
                        "ownDirectives": report.result.own_directives,
                        "inheritedDirectives": report.result.inherited_directives,
                    });
                    strip_fields_compact(&mut v);
                    files_out.push(v);
                }
                Err(e) => files_out.push(serde_json::json!({ "file": file, "error": e })),
            }
        }
        if files.len() > 10 {
            files_out.push(serde_json::json!({
                "note": format!("{} more file(s) not looked up — locate them individually", files.len() - 10),
            }));
        }

        // Task → best-matching nodes. Lenient scoring (unlike search_model's
        // strict AND): a task sentence carries filler words, so a node ranks by
        // the terms it DOES clear — at least one required — plus a coverage
        // bonus, so multi-term matches outrank one-word coincidences.
        let mut matches_out: Vec<serde_json::Value> = Vec::new();
        if let Some(t) = task {
            let terms: Vec<String> = t.split_whitespace().map(|w| w.to_lowercase()).collect();
            let mut scored: Vec<(f64, &scryer_core::Node)> = Vec::new();
            for n in &working.nodes {
                let mut fields: Vec<String> = vec![n.name.to_lowercase()];
                if let Some(d) = &n.description {
                    fields.push(d.to_lowercase());
                }
                if let Some(tech) = &n.technology {
                    fields.push(tech.to_lowercase());
                }
                for r in &n.responsibilities {
                    fields.push(r.statement.to_lowercase());
                }
                for p in &n.properties {
                    fields.push(p.label.to_lowercase());
                }
                let (mut total, mut cleared) = (0.0_f64, 0usize);
                for term in &terms {
                    let mut best = 0.0_f64;
                    for fl in &fields {
                        let (sc, _) = term_field_score(term, fl);
                        best = best.max(sc);
                    }
                    if best >= fuzzy_threshold(term) {
                        total += best;
                        cleared += 1;
                    }
                }
                if cleared > 0 {
                    scored.push((total + cleared as f64, n));
                }
            }
            scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
            for (score, n) in scored.iter().take(5) {
                finest.insert(n.id.clone());
                let resps: Vec<&str> =
                    n.responsibilities.iter().map(|r| r.statement.as_str()).collect();
                let mut v = serde_json::json!({
                    "id": n.id,
                    "kind": kind_str(&n.kind),
                    "name": n.name,
                    "path": breadcrumb(&working, &n.id),
                    "score": (score * 100.0).round() / 100.0,
                    "responsibilities": resps,
                    "inheritedDirectives": scryer_core::inherited_directives(&working, &n.id),
                });
                strip_fields_compact(&mut v);
                matches_out.push(v);
            }
        }

        // Scope = finest ∪ their descendants ∪ the chain ancestors (by id):
        // pending work under a matched node is this task's work, and a drift
        // scope attributes to a boundary owner that may sit above the finest
        // governing node.
        let mut scope: HashSet<String> = finest.clone();
        let mut frontier: Vec<String> = finest.iter().cloned().collect();
        while let Some(id) = frontier.pop() {
            for child in working.nodes.iter().filter(|n| n.parent_id.as_deref() == Some(&id)) {
                if scope.insert(child.id.clone()) {
                    frontier.push(child.id.clone());
                }
            }
        }
        for id in finest.iter().chain(chain.iter()) {
            let mut cur = id.clone();
            while let Some(pid) = working
                .nodes
                .iter()
                .find(|n| n.id == cur)
                .and_then(|n| n.parent_id.clone())
            {
                scope.insert(pid.clone());
                cur = pid;
            }
        }
        scope.extend(chain.iter().cloned());

        // Pending plan entries inside the scope, vagrants excluded (they are
        // drift review, not the implement queue).
        use scryer_core::diff::ElementKind as EK;
        let plan = scryer_core::diff::diff(&committed, &planned);
        let link_touches = |id: &str| {
            planned
                .links
                .iter()
                .chain(committed.links.iter())
                .any(|l| l.id == id && (scope.contains(&l.src) || scope.contains(&l.dst)))
        };
        let is_vagrant = |ch: &scryer_core::diff::ElementChange| match ch.kind {
            EK::Node => planned.nodes.iter().any(|n| n.id == ch.id && n.vagrant == Some(true)),
            EK::Responsibility => planned
                .nodes
                .iter()
                .flat_map(|n| n.responsibilities.iter())
                .chain(planned.groups.iter().flat_map(|g| g.responsibilities.iter()))
                .any(|r| r.id == ch.id && r.vagrant == Some(true)),
            EK::Property => ch.owner_id.as_deref().is_some_and(|oid| {
                planned.nodes.iter().any(|n| {
                    n.id == oid
                        && n.properties.iter().any(|p| p.label == ch.id && p.vagrant == Some(true))
                })
            }),
            _ => false,
        };
        let scoped_pending: Vec<&scryer_core::diff::ElementChange> = plan
            .changes
            .iter()
            .filter(|ch| {
                let in_scope = match ch.kind {
                    EK::Node => scope.contains(&ch.id),
                    EK::Responsibility | EK::Property => {
                        ch.owner_id.as_deref().is_some_and(|o| scope.contains(o))
                    }
                    EK::Link => link_touches(&ch.id),
                    EK::Group => false,
                };
                in_scope && !is_vagrant(ch)
            })
            .collect();
        let pending_total = scoped_pending.len();
        let pending_out: Vec<serde_json::Value> = scoped_pending
            .iter()
            .take(20)
            .map(|ch| serde_json::to_value(ch).unwrap_or(serde_json::Value::Null))
            .collect();

        // Drift scopes inside the scope (only meaningful once a reconcile
        // anchor exists).
        let drift_out: Vec<serde_json::Value> = if model_ref.sync_path().exists() {
            let sync = scryer_core::read_sync_state(&model_ref);
            scryer_core::drift::drifted_scopes(&committed, model_ref.project_path(), &sync)
                .iter()
                .filter(|sc| scope.contains(&sc.node_id))
                .map(|sc| {
                    serde_json::json!({
                        "nodeId": sc.node_id,
                        "nodeName": sc.node_name,
                        "changedFiles": sc.changed_files,
                    })
                })
                .collect()
        } else {
            Vec::new()
        };
        let drift_total = drift_out.len();

        // The 2-3 rules the task is about, in full — saves the get_rules trip.
        let rules_out: Vec<serde_json::Value> = match task {
            Some(t) => scryer_core::rules::lookup(t)
                .iter()
                .take(3)
                .map(|r| serde_json::json!({ "id": r.id, "title": r.title, "body": r.body }))
                .collect(),
            None => Vec::new(),
        };

        let phase = match (pending_total > 0, drift_total > 0) {
            (true, true) => "plan-execution + reconcile: this scope has pending intent to implement AND code that changed outside the plan — get_drift the scope before building on it",
            (true, false) => "plan-execution: pending intent exists in this scope — implement it, then mark_implemented (anchors param folds + anchors in one call)",
            (false, true) => "reconcile: code in this scope changed since the last reconcile — compare against the claims, flag_drift findings, reconcile_drift when done",
            (false, false) => "free: model and code agree in this scope — plan model deltas first if your change alters what the model claims (see Proportionality)",
        };

        let mut payload = serde_json::json!({
            "task": task,
            "files": files_out,
            "matches": matches_out,
            "pending": pending_out,
            "pendingTotal": pending_total,
            "drift": drift_out,
            "rules": rules_out,
            "phase": phase,
            "state": status_header(&model_ref),
        });
        strip_fields_compact(&mut payload);
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
                return Ok(CallToolResult::error(vec![Content::text(read_fail("model", &model_ref, &e))]));
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
                return Ok(CallToolResult::error(vec![Content::text(read_fail("model", &model_ref, &e))]));
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
                &scryer_core::drift::SyncState::anchored_now(scryer_core::drift::head_commit(project)),
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
        description = "What model intent is NOT yet reflected in code — the model→code work outstanding. This is the PLAN diff: how the draft (`planned`) diverges from the committed `model`. Each entry names an element (node / responsibility / property / link / group) and what to do: `added` (implement new code), `reworded` (re-implement to the new spec), `moved` (move the code), `repointed` (re-point the relationship), `deleted` (remove the code) — with a breadcrumb path and, for responsibilities, source anchors. Implementing an entry and calling `mark_implemented` folds it from the plan into the committed model. A `reworded` claim on the `appearance` field is a planned VISUAL change — reconcile the component's code to the accepted fixture named in the entry's `appearanceInstruction`, not to a text spec. Entries tagged to a CHANGE (a named plan partition, see `set_change`) carry its id in `change`; `openChanges` lists every open change with its rationale, and passing `change` (an id, or \"unfiled\") filters the queue to one task — an agent told to implement one change need not wade through the rest. Call this to find what needs implementing or syncing to the codebase."
    )]
    pub(crate) fn get_pending(
        &self,
        Parameters(req): Parameters<GetPendingRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = resolve_model_ref(req.project.as_deref())?;
        let model = match scryer_core::read_model_at(&model_ref) {
            Ok(m) => m,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(read_fail("model", &model_ref, &e))]));
            }
        };
        let planned = match scryer_core::read_planned_at(&model_ref) {
            Ok(p) => p,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(read_fail("plan", &model_ref, &e))]));
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
        // A property's identity is (owner node, label); a vagrant one is a
        // code-discovered field awaiting adopt/reject, so it too is kept out of the
        // implement queue.
        let is_vagrant_prop = |owner: Option<&str>, label: &str| {
            owner.is_some_and(|oid| {
                planned.nodes.iter().any(|n| {
                    n.id == oid
                        && n.properties
                            .iter()
                            .any(|p| p.label == label && p.vagrant == Some(true))
                })
            })
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
                ElementKind::Property => is_vagrant_prop(ch.owner_id.as_deref(), &ch.id),
                _ => false,
            };
            if vagrant {
                continue;
            }
            // Ledger: which change (named plan partition) this entry belongs to
            // — untagged entries are the unfiled bucket. `change` filters the
            // queue to one change ("implement THIS change"), and the summary
            // counts follow the filter.
            let tagged = planned.change_map.get(&scryer_core::changes::key_for(ch));
            if let Some(want) = req.change.as_deref() {
                let keep = if want == "unfiled" {
                    tagged.is_none()
                } else {
                    tagged.map(String::as_str) == Some(want)
                };
                if !keep {
                    continue;
                }
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
            // A reworded "appearance" claim is a planned VISUAL change, not a text
            // spec: the agent can't re-implement it from the wording, so point it
            // at the accepted fixture to reconcile the component's code against.
            if ch.kind == ElementKind::Node {
                if let Some(Change::Reworded { to, .. }) = ch.changes.iter().find(
                    |c| matches!(c, Change::Reworded { field, .. } if field == "appearance"),
                ) {
                    v["appearanceInstruction"] = serde_json::Value::String(format!(
                        "This node's appearance has a planned change — the model wants a new look. \
                         Reconcile the component's code to the accepted fixture at {to} \
                         (the fixture is the basis; do not diff its contents), then mark_implemented."
                    ));
                }
            }
            if let Some(cid) = tagged {
                v["change"] = serde_json::Value::String(cid.clone());
            }
            changes_out.push(v);
        }

        // The open-change registry rides every pending read: a fresh session
        // resumes a change from here (set_change {change_id}) instead of doing
        // archaeology on the flat queue.
        let open_changes: Vec<serde_json::Value> = planned
            .changes
            .iter()
            .map(|c| {
                serde_json::json!({
                    "id": c.id,
                    "rationale": c.rationale,
                    "entries": planned.change_map.values().filter(|v| *v == &c.id).count(),
                })
            })
            .collect();

        let mut payload = serde_json::json!({
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
        if !open_changes.is_empty() {
            payload["openChanges"] = serde_json::Value::Array(open_changes);
        }
        if let Some(current) = self.session_change(&model_ref) {
            payload["currentChange"] = serde_json::Value::String(current);
        }
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
        description = "Annotated project directory tree — the codebase itself, not just its manifests: source files render (capped per directory so generated trees cannot drown the shape), with manifests ([manifest]), infrastructure configs ([infrastructure]), and environment templates ([environment]) called out. Use before modeling to identify deployable units, data stores, external integrations, and frameworks. Respects .gitignore and skips build output / dependency directories."
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
        description = "Run the structural validator over your WORKING model (the plan, with committed's code anchors overlaid) — so it sees the edits you just authored, which is what makes it a post-CLOSE gate. Returns a list of warnings: parent-kind mismatches, unknown link endpoints, group members at mixed levels, empty symbols (carrying no responsibility/property/appearance), source-map entries that reference unknown ids, and responsibility mappings whose line range covers the whole enclosing symbol (a range must be a proper subset — drop it to mean the whole definition). A clean run is a post-edit gate, not a lookup — to FIND nodes by shape on demand (e.g. every empty symbol) use `query_model`. Does NOT judge responsibility wording quality."
    )]
    fn validate_model(
        &self,
        Parameters(req): Parameters<ValidateModelRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = resolve_model_ref(req.project.as_deref())?;
        // Validate the working view: authoring lands in the plan, so a
        // committed-only read would miss every edit the agent is about to close
        // out. `read_planned_at` falls back to committed when no plan diverges, so
        // this reduces to the committed model on a clean project.
        let committed = match scryer_core::read_model_at(&model_ref) {
            Ok(m) => m,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(read_fail("model", &model_ref, &e))]));
            }
        };
        let planned = match scryer_core::read_planned_at(&model_ref) {
            Ok(m) => m,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(read_fail("plan", &model_ref, &e))]));
            }
        };
        let model = scryer_core::working_view(&committed, &planned);
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
        description = "The model's observability report — deterministic, no semantic judgment. Per node: own + subtree rollups of responsibility/property counts, vagrant/stale flags, and anchor coverage (anchorable = any committed claim on LEAF nodes; claims on structural nodes are discharged through their subtree and are never 'unmapped'). `verified` counts claims carrying a BACKING TEST (a verify entry) — a separate dimension from anchoring, not gated on leafness (a structural claim backed by an integration test counts); anchor observations keyed `verify:{id}` are that claim's test changing/breaking, not its implementation. `testable` counts claims in a When/While/If form on code-backed hosts — a concrete trigger/state/failure a test can demonstrate, classified deterministically from the leading keyword — and `untested` is the testable claims with no verify entry: the demonstrable claims nothing demonstrates (rule 22's work queue). Plus anchor state from the git-free fingerprint check — `changed` (the anchored span's content differs from what the model last saw), `broken` (the symbol is gone), `fileMissing` — with moved-but-unchanged symbols silently re-anchored. The whole-model summary AGGREGATES anchors per container scope (`anchorSummary.byScope`: 'API: 31 changed, 5 broken'); the flat per-anchor list appears only on the node-scoped call. Also a declared-link audit against the extracted import graph (edge_count 0 = asserted-only; 'unmodeled' = sibling pairs the code connects but no link declares). Also per node: `completeness` — how much of the node's AUTHORED subtree (committed + planned) reads through to real code, so it is defined from greenfield onward. `pct` (0–100) is anchored primitives over authored ones, where a primitive is a node's boundary box (counted only when its glob owns a real file), a leaf responsibility, or a data shape (counted when its anchor resolves and is not broken/missing); a scaffolded container reads low but non-zero, greenfield reads 0. `pct` is ABSENT ('—', unmeasured) when the subtree has no leaf primitives (a bare box), so an undecomposed shell never reads 100%. Only anchor what you have implemented — that discipline is what makes the figure trustworthy. Pass node_id to scope to one subtree with per-child summaries; omit it for the whole-model summary. Use this to decide WHERE work is needed (unmapped claims, vagrant flags, dark links) before reading full subtrees. `broadBoundaries` flags node boundary globs with no directory prefix (e.g. `**/*`), which silently own every otherwise-unowned file. The whole-model summary also carries `coverage` — calibration of the deterministic layer itself: which languages' imports the link audit resolves FULLY vs by name-heuristic (a declared link between name-heuristic files can read asserted-only even when real), and `silentAnchors`, sourceMap anchors holding no fingerprint tripwire — drift can never fire for those, so treat their green as silence, not health."
    )]
    fn get_health(
        &self,
        Parameters(req): Parameters<GetHealthRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = resolve_model_ref(req.project.as_deref())?;
        let model = match scryer_core::read_model_at(&model_ref) {
            Ok(m) => m,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(read_fail("model", &model_ref, &e))]));
            }
        };
        let project = model_ref.project_path();

        // Design-first guard. get_health reports the COMMITTED model as a lens
        // over code; before anything is implemented the whole architecture lives
        // in the plan and committed is empty, so a coverage report here is all
        // zeros — which reads as "nothing authored" when in fact a full plan is
        // waiting. Detect that case and redirect to the plan-layer reads instead
        // of emitting a misleading empty report.
        if model.nodes.is_empty() {
            if let Ok(planned) = scryer_core::read_planned_at(&model_ref) {
                let nodes = planned.nodes.len();
                if nodes > 0 {
                    let resps: usize = planned
                        .nodes
                        .iter()
                        .map(|n| n.responsibilities.len())
                        .chain(planned.groups.iter().map(|g| g.responsibilities.len()))
                        .sum();
                    // Completeness IS meaningful from greenfield — its
                    // denominator is authored intent (committed + planned), so
                    // the design-first answer is real numbers with a real
                    // denominator (0% of 50 primitives), not a refusal.
                    let files = scryer_extract::list_project_files(project);
                    let completeness = scryer_core::health::resolve_completeness(
                        &model,
                        &planned,
                        &files,
                        &HashSet::new(),
                    );
                    let roots: Vec<serde_json::Value> = planned
                        .nodes
                        .iter()
                        .filter(|n| n.parent_id.is_none())
                        .map(|n| {
                            serde_json::json!({
                                "id": n.id,
                                "name": n.name,
                                "kind": kind_str(&n.kind),
                                "completeness": completeness
                                    .get(&n.id)
                                    .map(|c| serde_json::to_value(c).unwrap_or_default()),
                            })
                        })
                        .collect();
                    let payload = serde_json::json!({
                        "designFirst": true,
                        "planNodes": nodes,
                        "planResponsibilities": resps,
                        "completeness": roots,
                        "guidance": "Committed model is empty — nothing has been implemented \
                             yet — but the plan holds the authored architecture. Coverage and \
                             anchor reporting start once work folds in (mark_implemented); \
                             `completeness` above is already real: anchored primitives over \
                             AUTHORED ones, so it grows from 0 as you build. Read the plan \
                             with get_pending (the model→code work queue) and read_model. Do \
                             NOT conclude the model is empty.",
                    });
                    return Ok(CallToolResult::success(vec![Content::text(
                        serde_json::to_string_pretty(&payload)
                            .unwrap_or_else(|_| "{}".to_string()),
                    )]));
                }
            }
        }

        // Same no-anchor bootstrap as get_drift: a model never reconciled would
        // read as "everything drifted" against no baseline. Seeding also writes
        // the anchor fingerprint baseline (git-free content snapshot).
        let anchor_check = if model_ref.sync_path().exists() {
            // May silently re-anchor moved symbols — re-read the model after.
            scryer_extract::anchors::check_anchors(&model_ref).unwrap_or_default()
        } else {
            let _ = scryer_core::write_sync_state(
                &model_ref,
                &scryer_core::drift::SyncState::anchored_now(scryer_core::drift::head_commit(project)),
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
        // covers everything model-derivable (counts, discharge, coverage). The
        // plan layer widens the leaf verdict: a design-ahead child makes its
        // parent structural, matching completeness's union view.
        let planned_for_health =
            scryer_core::read_planned_at(&model_ref).unwrap_or_else(|_| model.clone());
        let health =
            scryer_core::health::compute_health(&model, Some(&planned_for_health), None);

        // Completeness — how much of each node's AUTHORED subtree reads through to
        // real code. Spans committed + planned (so it is defined from greenfield),
        // and resolves anchors against the filesystem: a boundary box counts only
        // when its glob owns a real file; a leaf claim only when its anchor is
        // present and not broken/missing.
        let files = scryer_extract::list_project_files(project);
        let completeness = {
            let planned = scryer_core::read_planned_at(&model_ref).unwrap_or_else(|_| model.clone());
            // Anchors reported broken/missing are dead; `changed` still exists.
            let dead: HashSet<&str> = anchor_check
                .observations
                .iter()
                .filter(|o| {
                    matches!(
                        o.state,
                        scryer_extract::anchors::AnchorState::Broken
                            | scryer_extract::anchors::AnchorState::FileMissing
                    )
                })
                .map(|o| o.key.as_str())
                .collect();
            scryer_core::health::resolve_completeness(&model, &planned, &files, &dead)
        };
        let comp_json = |id: &str| {
            completeness
                .get(id)
                .map(|c| serde_json::to_value(c).unwrap_or(serde_json::Value::Null))
        };

        // The import graph is cached by builds / the app's health refresh; when
        // absent the link audit is simply omitted rather than guessed.
        let derived = scryer_core::build_edges::read_build_edges(&model_ref.build_edges_path())
            .map(|edges| scryer_core::build_edges::derive_graph(&model, &edges));

        let counts_json = |c: &scryer_core::health::HealthCounts| {
            serde_json::to_value(c).unwrap_or_default()
        };

        // Boundary globs with no directory prefix (`**/*`, specificity 0) own
        // every otherwise-unowned file, so drift and coverage attribute unrelated
        // changes to that node. update_source_map warns at write time; surface the
        // standing state here too. `scope` limits it to a subtree (None = whole
        // model); a boundary keyed to a dead node is skipped.
        let broad_boundaries = |scope: Option<&HashSet<&str>>| -> Vec<serde_json::Value> {
            let mut keys: Vec<&String> = model.boundaries.keys().collect();
            keys.sort();
            let mut out = Vec::new();
            for nid in keys {
                let Some(node) = model.nodes.iter().find(|n| &n.id == nid) else {
                    continue;
                };
                if scope.is_some_and(|s| !s.contains(nid.as_str())) {
                    continue;
                }
                for src in &model.boundaries[nid] {
                    if scryer_core::ownership::pattern_specificity(&src.pattern) == 0 {
                        out.push(serde_json::json!({
                            "node": nid, "name": node.name, "pattern": src.pattern,
                        }));
                    }
                }
            }
            out
        };

        // Calibration: what the deterministic layer can and cannot see. The
        // link audit resolves real imports for some languages and falls back
        // to bare-name coincidence for others — a declared link between
        // name-heuristic files can audit as asserted-only even when real.
        // Silent anchors have no fingerprint, so no drift will EVER fire for
        // them; without this, they read as green.
        let link_audit_coverage = {
            let mut tiers: BTreeMap<&str, std::collections::BTreeSet<&str>> = BTreeMap::new();
            for file in &files {
                let Some(ext) = std::path::Path::new(file.as_str())
                    .extension()
                    .and_then(|e| e.to_str())
                else {
                    continue;
                };
                if let Some(tier) = scryer_extract::lang::import_resolution_tier(ext) {
                    tiers.entry(tier).or_default().insert(ext);
                }
            }
            serde_json::json!({
                "full": tiers.get("full").map(|s| s.iter().collect::<Vec<_>>()).unwrap_or_default(),
                "nameHeuristic": tiers.get("nameHeuristic").map(|s| s.iter().collect::<Vec<_>>()).unwrap_or_default(),
            })
        };
        let silent_anchors =
            scryer_extract::anchors::untracked_anchors(&model_ref).unwrap_or_default();

        let payload = match req.node_id.as_deref() {
            Some(node_id) => {
                let Some(node) = model.nodes.iter().find(|n| n.id == node_id) else {
                    // get_health is a lens over COMMITTED code. A node the agent
                    // just authored lives only in the plan and has no code to
                    // report on yet — say so and point at the plan reads, instead
                    // of a bare "not found" that reads as a typo.
                    let in_plan = scryer_core::read_planned_at(&model_ref)
                        .map(|p| p.nodes.iter().any(|n| n.id == node_id))
                        .unwrap_or(false);
                    let msg = if in_plan {
                        format!(
                            "Node '{node_id}' exists in the plan but not the committed model — get_health reports committed code, so there is nothing to measure until you implement and fold it. Read it with read_model {{layer: plan}}, or get_pending to see its outstanding work."
                        )
                    } else {
                        format!("Node '{node_id}' not found")
                    };
                    return Ok(CallToolResult::error(vec![Content::text(msg)]));
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
                            "completeness": comp_json(&n.id),
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
                    "completeness": comp_json(&node.id),
                    "children": children,
                    "anchors": drift_here,
                    "links": links_here,
                    "unmodeled": unmodeled_here,
                    "broadBoundaries": broad_boundaries(Some(&subtree_ids)),
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
                            "completeness": comp_json(&n.id),
                        })
                    })
                    .collect();
                let asserted_only = derived
                    .as_ref()
                    .map(|g| g.link_audit.iter().filter(|a| a.edge_count == 0).count());

                // Anchor observations aggregated per owning CONTAINER scope —
                // "Desktop UI: 31 changed, 5 broken" — so a busy repo costs a
                // handful of lines, not a wall. The flat per-anchor list stays
                // on the node-scoped call, where it is small and actionable.
                let scope_of = |host_id: &str| -> (String, String) {
                    let mut cur = host_id;
                    let mut root = host_id;
                    while let Some(n) = model.nodes.iter().find(|n| n.id == cur) {
                        if n.kind == scryer_core::Kind::Container {
                            return (n.id.clone(), n.name.clone());
                        }
                        root = cur;
                        match n.parent_id.as_deref() {
                            Some(p) => cur = p,
                            None => break,
                        }
                    }
                    let name = model
                        .nodes
                        .iter()
                        .find(|n| n.id == root)
                        .map(|n| n.name.clone())
                        .unwrap_or_else(|| root.to_string());
                    (root.to_string(), name)
                };
                use scryer_extract::anchors::AnchorState;
                let mut per_scope: BTreeMap<(String, String), (usize, usize, usize)> =
                    BTreeMap::new();
                let (mut n_changed, mut n_broken, mut n_missing) = (0usize, 0usize, 0usize);
                for o in &anchor_check.observations {
                    let e = per_scope.entry(scope_of(&o.host_id)).or_default();
                    match o.state {
                        AnchorState::Changed => {
                            e.0 += 1;
                            n_changed += 1;
                        }
                        AnchorState::Broken => {
                            e.1 += 1;
                            n_broken += 1;
                        }
                        AnchorState::FileMissing => {
                            e.2 += 1;
                            n_missing += 1;
                        }
                    }
                }
                let mut by_scope: Vec<((String, String), (usize, usize, usize))> =
                    per_scope.into_iter().collect();
                by_scope.sort_by_key(|(_, (c, b, m))| std::cmp::Reverse(c + b + m));
                let by_scope: Vec<serde_json::Value> = by_scope
                    .into_iter()
                    .map(|((id, name), (c, b, m))| {
                        serde_json::json!({
                            "nodeId": id, "name": name,
                            "changed": c, "broken": b, "fileMissing": m,
                        })
                    })
                    .collect();

                serde_json::json!({
                    "totals": counts_json(&health.totals),
                    "totalsNote": "totals.stale counts claims semantically FLAGGED stale by a \
                                   drift review (flag_drift) — it is NOT the anchor tripwire \
                                   count. 0 stale next to changed/broken anchors means those \
                                   anchors AWAIT review, not that all is clean.",
                    "roots": roots,
                    "anchorSummary": {
                        "changed": n_changed,
                        "broken": n_broken,
                        "fileMissing": n_missing,
                        "byScope": by_scope,
                        "note": "per-anchor detail is node-scoped — get_health {nodeId} for a scope's exact anchors",
                    },
                    "reanchored": anchor_check.reanchored,
                    "assertedOnlyLinks": asserted_only,
                    "unmodeled": derived.as_ref().map(|g| &g.unmodeled),
                    "broadBoundaries": broad_boundaries(None),
                    "disconnected": health.disconnected.iter().map(|id| {
                        let name = model.nodes.iter().find(|n| &n.id == id).map(|n| n.name.clone()).unwrap_or_default();
                        serde_json::json!({ "nodeId": id, "name": name, "path": breadcrumb(&model, id) })
                    }).collect::<Vec<_>>(),
                    "disconnectedNote": "architecture nodes no relationship link names as source or target — they read as edgeless/disconnected on every diagram and are easy to miss. Wire each into the relationship it actually performs, or confirm it belongs. Symbols are exempt.",
                    "edgeGraph": if derived.is_some() { "from last build's dependency cache" } else { "absent — run a model build (or the app's health refresh) to derive the link audit" },
                    "coverage": {
                        "linkAudit": link_audit_coverage,
                        "linkAuditNote": "declared links between nameHeuristic-language files can read asserted-only even when real — calibrate the audit's verdict accordingly",
                        "silentAnchors": silent_anchors.len(),
                        "silentAnchorSample": silent_anchors.iter().take(5).collect::<Vec<_>>(),
                        "silentAnchorNote": if silent_anchors.is_empty() { "every anchor carries a fingerprint tripwire" } else { "these anchors have NO fingerprint (file absent, symbol unresolvable, or glob matching nothing) — drift can never fire for them, so their green is silence, not health" },
                    },
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
            position: None,
            directives: Vec::new(),
        }
    }

    fn resp(id: &str, statement: &str) -> Responsibility {
        Responsibility {
            concern: None,
            id: id.into(),
            statement: statement.into(),
            vagrant: None,
            stale: None,
            stale_proposal: None,
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

    /// validate_model is the CLOSE gate, but authoring lands in the PLAN — so it
    /// must validate the working view, not committed alone. Committed here is
    /// clean; the plan adds a structurally-invalid node (a component parented to a
    /// system). A committed-only gate would report clean and miss it.
    #[test]
    fn validate_model_sees_plan_only_edits() {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());
        let project = dir.path().to_string_lossy().to_string();

        // Committed: a lone, clean system.
        let mut committed = ScryModel::new();
        committed.nodes.push(node("node-1", Kind::System, "Acme", None));
        scryer_core::write_model_at(&model_ref, &committed).unwrap();

        // Plan authors a component directly under the system — a parent-kind
        // violation that lives only in the draft.
        let mut planned = committed.clone();
        planned
            .nodes
            .push(node("node-2", Kind::Component, "Orphan", Some("node-1")));
        scryer_core::write_planned_at(&model_ref, &planned).unwrap();

        let server = ScryerServer::new();
        let r = server
            .validate_model(Parameters(ValidateModelRequest {
                project: Some(project),
            }))
            .unwrap();
        let out = serde_json::to_string(&r.content).unwrap();
        assert!(
            out.contains("node-2") && out.contains("cannot have parent"),
            "the gate must surface the plan-authored violation: {out}"
        );
    }

    /// A pending deletion must not trip the gate: the deleted element's
    /// committed boundary and anchor are pending GC (the deletion fold removes
    /// them), not dangling references the agent must somehow clear.
    #[test]
    fn validate_model_is_quiet_on_a_pending_deletion() {
        use scryer_core::Source;
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());
        let project = dir.path().to_string_lossy().to_string();

        // Committed: an anchored, boundary-owning system.
        let mut committed = ScryModel::new();
        let mut sys = node("node-1", Kind::System, "Acme", None);
        sys.responsibilities.push(resp("resp-1", "serve the API"));
        committed.nodes.push(sys);
        committed
            .boundaries
            .insert("node-1".into(), vec![Source { pattern: "src/**".into(), comment: None }]);
        committed.source_map.insert(
            "resp-1".into(),
            vec![serde_json::from_value(serde_json::json!({ "pattern": "src/api.rs" })).unwrap()],
        );
        scryer_core::write_model_at(&model_ref, &committed).unwrap();

        // Plan deletes the system; its anchors stay single-homed in committed.
        let mut planned = committed.clone();
        planned.nodes.clear();
        planned.source_map.clear();
        planned.boundaries.clear();
        scryer_core::write_planned_at(&model_ref, &planned).unwrap();

        let server = ScryerServer::new();
        let r = server
            .validate_model(Parameters(ValidateModelRequest {
                project: Some(project),
            }))
            .unwrap();
        let out = serde_json::to_string(&r.content).unwrap();
        assert!(
            !out.contains("unknown"),
            "no unknown-reference warnings for a pending deletion: {out}"
        );
    }

    /// get_health on a plan-only node must name the layer, not return a bare "not
    /// found": the node is authored but uncommitted, so there is no code to
    /// measure yet — the message points at the plan reads instead.
    #[test]
    fn get_health_on_a_plan_only_node_names_the_layer() {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());
        let project = dir.path().to_string_lossy().to_string();

        let mut committed = ScryModel::new();
        committed.nodes.push(node("node-1", Kind::System, "Acme", None));
        scryer_core::write_model_at(&model_ref, &committed).unwrap();
        let mut planned = committed.clone();
        planned
            .nodes
            .push(node("node-2", Kind::Container, "API", Some("node-1")));
        scryer_core::write_planned_at(&model_ref, &planned).unwrap();

        let server = ScryerServer::new();
        let r = server
            .get_health(Parameters(GetHealthRequest {
                project: Some(project),
                node_id: Some("node-2".into()),
            }))
            .unwrap();
        let out = serde_json::to_string(&r.content).unwrap();
        assert!(
            out.contains("plan but not the committed"),
            "the message must name the layer: {out}"
        );
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
                change: None,
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
    fn get_pending_gives_appearance_change_a_fixture_instruction() {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());

        let mut m = ScryModel::new();
        m.nodes.push(node("node-1", Kind::Component, "Chart", None));
        scryer_core::write_model_at(&model_ref, &m).unwrap();

        // The model wants a new look: the planned node gains an accepted fixture.
        let mut planned = m.clone();
        planned.nodes[0].appearance = Some(scryer_core::Appearance {
            status: Some(scryer_core::RenderState::Changed),
            dist_path: Some(".scryer/preview/accepted/node-1.tsx".into()),
            built_at: Some(1),
            source_hash: None,
        });
        scryer_core::write_planned_at(&model_ref, &planned).unwrap();

        let server = ScryerServer::new();
        let r = server
            .get_pending(Parameters(GetPendingRequest {
                change: None,
                project: Some(dir.path().to_string_lossy().to_string()),
            }))
            .unwrap();
        let dump = serde_json::to_string(&result_json(&r)).unwrap();

        // The visual change surfaces with purpose-built guidance, not a bare
        // reworded field — naming the fixture and the fold that closes the loop.
        assert!(dump.contains("appearanceInstruction"));
        assert!(dump.contains(".scryer/preview/accepted/node-1.tsx"));
        assert!(dump.contains("mark_implemented"));
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

    /// Whole-model get_health aggregates anchor observations per container
    /// scope — a busy repo costs a handful of lines, not a flat wall — and the
    /// exact anchors stay on the node-scoped call. totals carries the
    /// stale-is-not-anchor-state note.
    #[test]
    fn get_health_aggregates_anchors_per_scope() {
        use scryer_core::Source;

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let model_ref = ModelRef::ProjectLocal(root.to_path_buf());
        std::fs::create_dir_all(root.join("api/src")).unwrap();
        std::fs::write(root.join("api/src/server.rs"), "fn v1() {}\n").unwrap();

        let mut m = ScryModel::new();
        m.nodes.push(node("sys", Kind::System, "Acme", None));
        m.nodes.push(node("api", Kind::Container, "API", Some("sys")));
        let mut sym = node("h", Kind::Symbol, "handler", Some("api"));
        sym.responsibilities = vec![resp("r-h", "serves requests")];
        m.nodes.push(sym);
        m.source_map.insert(
            "r-h".into(),
            vec![serde_json::from_value(serde_json::json!({ "pattern": "api/src/server.rs" }))
                .unwrap()],
        );
        m.boundaries
            .insert("api".into(), vec![Source { pattern: "api/**/*".into(), comment: None }]);
        scryer_core::write_model_at(&model_ref, &m).unwrap();
        let project = root.to_string_lossy().to_string();
        let server = ScryerServer::new();

        // First call seeds the baseline; then the anchored file changes.
        let _ = server
            .get_health(Parameters(GetHealthRequest { project: Some(project.clone()), node_id: None }))
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        std::fs::write(root.join("api/src/server.rs"), "fn v2() {}\n").unwrap();

        let v = result_json(
            &server
                .get_health(Parameters(GetHealthRequest { project: Some(project.clone()), node_id: None }))
                .unwrap(),
        );
        assert!(v.get("anchors").is_none(), "no flat list on the whole-model call");
        assert_eq!(v["anchorSummary"]["changed"], 1);
        assert_eq!(v["anchorSummary"]["byScope"][0]["nodeId"], "api");
        assert_eq!(v["anchorSummary"]["byScope"][0]["name"], "API");
        assert_eq!(v["anchorSummary"]["byScope"][0]["changed"], 1);
        assert!(
            v["totalsNote"].as_str().unwrap().contains("NOT the anchor tripwire count"),
            "stale is annotated"
        );

        // Node-scoped call still carries the exact anchors.
        let v = result_json(
            &server
                .get_health(Parameters(GetHealthRequest {
                    project: Some(project),
                    node_id: Some("api".into()),
                }))
                .unwrap(),
        );
        let anchors = v["anchors"].as_array().unwrap();
        assert_eq!(anchors.len(), 1, "{anchors:?}");
        assert_eq!(anchors[0]["file"], "api/src/server.rs");
    }

    /// Design-first: with an empty committed model and an authored plan,
    /// get_health redirects — but WITH real numbers: completeness is defined
    /// from greenfield (denominator = authored intent), so the caller sees
    /// 0%-of-N, not a refusal to compute.
    #[test]
    fn get_health_design_first_reports_completeness() {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());
        scryer_core::write_model_at(&model_ref, &ScryModel::new()).unwrap();
        let mut planned = ScryModel::new();
        planned.nodes.push(node("sys", Kind::System, "Acme", None));
        let mut c = node("api", Kind::Container, "API", Some("sys"));
        c.responsibilities = vec![resp("r-1", "serves requests"), resp("r-2", "persists data")];
        planned.nodes.push(c);
        scryer_core::write_planned_at(&model_ref, &planned).unwrap();

        let server = ScryerServer::new();
        let v = result_json(
            &server
                .get_health(Parameters(GetHealthRequest {
                    project: Some(dir.path().to_string_lossy().to_string()),
                    node_id: None,
                }))
                .unwrap(),
        );
        assert_eq!(v["designFirst"], true);
        assert_eq!(v["planNodes"], 2);
        assert_eq!(v["planResponsibilities"], 2);
        let comp = &v["completeness"][0];
        assert_eq!(comp["id"], "sys");
        assert_eq!(
            comp["completeness"]["pct"], 0,
            "greenfield reads 0 WITH a denominator: {v}"
        );
        assert!(
            v["guidance"].as_str().unwrap().contains("Do NOT conclude the model is empty"),
            "{v}"
        );
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
            concern: None,
            id: "r-sys".into(),
            statement: "orchestrates everything".into(),
            vagrant: None,
            stale: None,
            stale_proposal: None,
            directives: Vec::new(),
            last_touched_at: None,
        });
        m.nodes.push(sys);
        let mut leaf = node("leaf", Kind::Symbol, "leafFn", Some("sys"));
        leaf.responsibilities.push(Responsibility {
            concern: None,
            id: "r-leaf".into(),
            statement: "does the thing".into(),
            vagrant: None,
            stale: None,
            stale_proposal: None,
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

    /// get_health surfaces boundary globs with no directory prefix (`**/*`) as
    /// `broadBoundaries` — they silently own every otherwise-unowned file — while
    /// a properly-scoped glob is left off the list.
    #[test]
    fn get_health_flags_broad_boundaries() {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());
        let mut m = ScryModel::new();
        m.nodes.push(node("sys", Kind::System, "Sys", None));
        m.nodes.push(node("c", Kind::Container, "Core", Some("sys")));
        m.boundaries.insert(
            "c".into(),
            vec![scryer_core::Source { pattern: "**/*".into(), comment: None }],
        );
        m.boundaries.insert(
            "sys".into(),
            vec![scryer_core::Source { pattern: "src/**/*".into(), comment: None }],
        );
        scryer_core::write_model_at(&model_ref, &m).unwrap();

        let server = ScryerServer::new();
        let v = result_json(
            &server
                .get_health(Parameters(GetHealthRequest {
                    project: Some(dir.path().to_string_lossy().to_string()),
                    node_id: None,
                }))
                .unwrap(),
        );
        let broad = v["broadBoundaries"].as_array().unwrap();
        assert_eq!(broad.len(), 1, "only the prefixless glob is flagged: {broad:?}");
        assert_eq!(broad[0]["node"], "c");
        assert_eq!(broad[0]["pattern"], "**/*");
    }

    /// System > Container (boundary src/**) > Component > symbol, with the
    /// symbol's claim anchored in `src/auth.rs` — committed. Directives on the
    /// component and container prove the binding set rides along.
    fn locate_project() -> (ScryerServer, tempfile::TempDir, String, ModelRef) {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());
        let mut m = ScryModel::new();
        m.nodes.push(node("sys", Kind::System, "Acme", None));
        let mut api = node("api", Kind::Container, "API", Some("sys"));
        api.directives = vec!["must stay stateless".into()];
        m.nodes.push(api);
        let mut auth = node("auth", Kind::Component, "Auth", Some("api"));
        auth.directives = vec!["must never log tokens".into()];
        m.nodes.push(auth);
        let mut sym = node("vt", Kind::Symbol, "verify_token", Some("auth"));
        sym.responsibilities = vec![resp("r-vt", "rejects forged credentials")];
        m.nodes.push(sym);
        m.source_map.insert(
            "r-vt".into(),
            vec![serde_json::from_value(
                serde_json::json!({ "pattern": "src/auth.rs", "symbol": "verify_token" }),
            )
            .unwrap()],
        );
        m.boundaries.insert(
            "api".into(),
            vec![serde_json::from_value(serde_json::json!({ "pattern": "src/**/*" })).unwrap()],
        );
        scryer_core::write_model_at(&model_ref, &m).unwrap();
        let project = dir.path().to_string_lossy().to_string();
        (ScryerServer::new(), dir, project, model_ref)
    }

    #[test]
    fn locate_returns_claims_chain_and_directives() {
        let (server, _dir, project, _mr) = locate_project();
        let v = result_json(
            &server
                .locate(Parameters(LocateRequest {
                    project: Some(project),
                    file: "src/auth.rs".into(),
                    symbol: None,
                }))
                .unwrap(),
        );
        assert_eq!(v["claims"][0]["id"], "r-vt");
        assert_eq!(v["claims"][0]["hostName"], "verify_token");
        assert_eq!(v["ownerChain"][0]["id"], "vt");
        assert_eq!(v["path"], "Acme / API / Auth / verify_token");
        assert_eq!(v["boundaryOwner"]["id"], "api");
        assert_eq!(v["ownDirectives"], serde_json::Value::Null, "symbol carries none");
        let inh = serde_json::to_string(&v["inheritedDirectives"]).unwrap();
        assert!(inh.contains("must never log tokens") && inh.contains("must stay stateless"));
    }

    /// locate reports the owning scope's health — the finest node's own +
    /// subtree counts and its completeness — so you see how well-modeled the
    /// region is, not just its intent.
    #[test]
    fn locate_reports_owning_scope_health() {
        let (server, dir, project, _mr) = locate_project();
        // Make the anchored file real so the leaf claim resolves to code.
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/auth.rs"), "fn verify_token() {}\n").unwrap();

        let v = result_json(
            &server
                .locate(Parameters(LocateRequest {
                    project: Some(project),
                    file: "src/auth.rs".into(),
                    symbol: None,
                }))
                .unwrap(),
        );
        let sh = &v["scopeHealth"];
        assert_eq!(sh["nodeId"], "vt", "scoped to the finest owning node");
        assert_eq!(sh["name"], "verify_token");
        assert_eq!(sh["own"]["responsibilities"], 1);
        // The leaf's one claim resolves to a real file → fully covered.
        assert_eq!(sh["completeness"]["pct"], 100);
    }

    /// orient bundles the five-call dance: per-file governing chain +
    /// directives, task-matched nodes, the pending entries scoped to that
    /// region (an unrelated sibling's work stays out), the matching rules in
    /// full, and a phase verdict.
    #[test]
    fn orient_bundles_scope_pending_rules_and_phase() {
        let (server, _dir, project, model_ref) = locate_project();
        let committed = scryer_core::read_model_at(&model_ref).unwrap();
        let mut planned = committed.clone();
        planned
            .nodes
            .iter_mut()
            .find(|n| n.id == "vt")
            .unwrap()
            .responsibilities
            .push(resp("r-new", "refuses expired tokens"));
        // Unrelated pending work elsewhere: a sibling container.
        planned.nodes.push(node("web", Kind::Container, "Web", Some("sys")));
        scryer_core::write_planned_at(&model_ref, &planned).unwrap();

        let v = result_json(
            &server
                .orient(Parameters(OrientRequest {
                    project: Some(project),
                    task: Some("token symbol".into()),
                    files: Some(vec!["src/auth.rs".into()]),
                }))
                .unwrap(),
        );

        // File side: the chain and the binding directives ride along.
        assert_eq!(v["files"][0]["ownerChain"][0]["id"], "vt");
        let inh = serde_json::to_string(&v["files"][0]["inheritedDirectives"]).unwrap();
        assert!(inh.contains("must never log tokens"), "{inh}");

        // Task side: the symbol matches on its name/claim.
        let matches = serde_json::to_string(&v["matches"]).unwrap();
        assert!(matches.contains("\"vt\""), "task terms reach the symbol: {matches}");

        // Pending is scoped: the new claim on vt shows; the unrelated sibling
        // container's work does not.
        let pending = serde_json::to_string(&v["pending"]).unwrap();
        assert!(pending.contains("r-new"), "{pending}");
        assert!(!pending.contains("\"web\""), "sibling work stays out: {pending}");
        assert_eq!(v["pendingTotal"], 1);

        // Rules inline: "symbol" pulls rule 8 in full, capped at 3.
        let rules = v["rules"].as_array().unwrap();
        assert!(rules.iter().any(|r| r["id"] == 8), "rule 8 rides along: {rules:?}");
        assert!(rules.len() <= 3);

        // Phase: pending intent exists, no drift baseline → plan-execution.
        let phase = v["phase"].as_str().unwrap();
        assert!(phase.starts_with("plan-execution:"), "{phase}");
    }

    /// orient with neither task nor files is a usage error that steers, and a
    /// task-only call still works against the model.
    #[test]
    fn orient_requires_a_scope_and_works_task_only() {
        let (server, _dir, project, _mr) = locate_project();
        let r = server
            .orient(Parameters(OrientRequest {
                project: Some(project.clone()),
                task: None,
                files: None,
            }))
            .unwrap();
        assert!(r.is_error.unwrap_or(false));

        let v = result_json(
            &server
                .orient(Parameters(OrientRequest {
                    project: Some(project),
                    task: Some("forged credentials".into()),
                    files: None,
                }))
                .unwrap(),
        );
        let matches = serde_json::to_string(&v["matches"]).unwrap();
        assert!(matches.contains("\"vt\""), "claim text reaches the node: {matches}");
        let phase = v["phase"].as_str().unwrap();
        assert!(phase.starts_with("free:"), "clean scope reads free: {phase}");
    }

    #[test]
    fn locate_sees_plan_claims_and_scopes_pending() {
        let (server, _dir, project, model_ref) = locate_project();
        // The plan authors a second claim on the symbol, anchored to the same
        // file, plus an unrelated node elsewhere.
        let committed = scryer_core::read_model_at(&model_ref).unwrap();
        let mut planned = committed.clone();
        planned
            .nodes
            .iter_mut()
            .find(|n| n.id == "vt")
            .unwrap()
            .responsibilities
            .push(resp("r-new", "refuses expired tokens"));
        planned.source_map.insert(
            "r-new".into(),
            vec![serde_json::from_value(
                serde_json::json!({ "pattern": "src/auth.rs", "symbol": "verify_token" }),
            )
            .unwrap()],
        );
        planned.nodes.push(node("web", Kind::Container, "Web", Some("sys")));
        scryer_core::write_planned_at(&model_ref, &planned).unwrap();

        let v = result_json(
            &server
                .locate(Parameters(LocateRequest {
                    project: Some(project),
                    file: "src/auth.rs".into(),
                    symbol: Some("verify_token".into()),
                }))
                .unwrap(),
        );
        assert_eq!(v["symbolMatched"], true);
        let dump = serde_json::to_string(&v["claims"]).unwrap();
        assert!(dump.contains("r-new"), "plan-authored claim visible: {dump}");
        // Pending is scoped: the new claim shows, the unrelated container doesn't.
        let pending = serde_json::to_string(&v["pending"]).unwrap();
        assert!(pending.contains("r-new"), "scoped pending: {pending}");
        assert!(!pending.contains("\"web\""), "unrelated pending excluded: {pending}");
    }

    #[test]
    fn locate_dark_and_unowned_files_steer() {
        let (server, _dir, project, _mr) = locate_project();
        // Under the boundary, no anchors: dark code, steered to the owner.
        let v = result_json(
            &server
                .locate(Parameters(LocateRequest {
                    project: Some(project.clone()),
                    file: "src/dark.rs".into(),
                    symbol: None,
                }))
                .unwrap(),
        );
        assert!(v["claims"][0].is_null());
        assert_eq!(v["boundaryOwner"]["id"], "api");
        assert!(v["note"].as_str().unwrap().contains("dark code under 'API'"));

        // Outside every boundary and anchor: steered to model-first authoring.
        let v = result_json(
            &server
                .locate(Parameters(LocateRequest {
                    project: Some(project),
                    file: "docs/readme.md".into(),
                    symbol: None,
                }))
                .unwrap(),
        );
        assert!(v["note"].as_str().unwrap().contains("No model intent maps"));
    }
}
