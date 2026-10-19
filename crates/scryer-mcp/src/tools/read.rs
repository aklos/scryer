//! The read side of the MCP: the derived C4 tree, its claims, and where a
//! file's new todo stubs go.

use crate::derived;
use crate::helpers::*;
use crate::server::ScryerServer;
use crate::types::*;
use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, Content},
    tool, tool_router, ErrorData as McpError,
};
use scryer_core::{Kind, ModelRef, Node, ScryModel};
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
        // The concern registry — the tree's cross-cutting vocabulary, from
        // the trailing [tags] on test descriptions.
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

    // Attached tests (claim → test locations), scoped like the source map.
    let test_map: serde_json::Map<String, serde_json::Value> = model
        .test_map
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
        "sourceMap": source_map,
        "testMap": test_map,
        "boundaries": boundaries,
    }))
}

/// Normalize a request path to the tree's convention: project-relative,
/// `/`-separated. An absolute path inside the project is accepted.
fn normalize_project_rel(model_ref: &ModelRef, path: &str) -> String {
    let mut file = path.replace('\\', "/");
    let root = model_ref.project_path().to_string_lossy().replace('\\', "/");
    if let Some(rest) = file.strip_prefix(root.as_str()) {
        file = rest.trim_start_matches('/').to_string();
    }
    file.trim_start_matches("./").to_string()
}

/// Load the derived tree, or the error result to answer with.
fn load_or_err(model_ref: &ModelRef) -> Result<derived::Loaded, CallToolResult> {
    derived::load(model_ref).map_err(|e| {
        CallToolResult::error(vec![Content::text(format!(
            "Could not derive the tree for {model_ref}: {e}"
        ))])
    })
}

fn layer_of(tree: &scryer_core::derived_tree::DerivedTree, layer: Layer) -> &ScryModel {
    match layer {
        Layer::Plan => &tree.planned,
        Layer::Committed => &tree.committed,
    }
}

/// Claims listed per file in `orient`; the rest are counted.
const ORIENT_CLAIMS_PER_FILE: usize = 12;
/// Files `orient` looks up; the rest are named for `locate`.
const ORIENT_FILES: usize = 10;
/// The ceiling on `orient`'s whole answer. It is read on every coding task, so
/// it is a pointer into the tree, never the tree itself.
const ORIENT_MAX_CHARS: usize = 4000;

fn clip(s: &str, max: usize) -> String {
    match s.char_indices().nth(max) {
        Some((i, _)) => format!("{}…", &s[..i]),
        None => s.to_string(),
    }
}

/// Serialize `orient`'s payload within [`ORIENT_MAX_CHARS`]: shed the per-file
/// claim lists from the last file backwards, then the matches, until it fits —
/// each shed list is replaced by a count so nothing is silently missing.
fn fit_orient(mut payload: serde_json::Value) -> String {
    let len = |p: &serde_json::Value| serde_json::to_string(p).map(|s| s.len()).unwrap_or(0);
    let nfiles = payload["files"].as_array().map_or(0, Vec::len);
    for i in (0..nfiles).rev() {
        if len(&payload) <= ORIENT_MAX_CHARS {
            break;
        }
        let f = &mut payload["files"][i];
        let n = f["claims"].as_array().map_or(0, Vec::len)
            + f["moreClaims"].as_str().and_then(|m| m.split(' ').next()?.parse::<usize>().ok()).unwrap_or(0);
        if let Some(obj) = f.as_object_mut() {
            if obj.remove("claims").is_some() {
                obj.insert("moreClaims".into(), serde_json::json!(format!("{n} claims — locate {{file}}")));
            }
            obj.remove("description");
        }
    }
    if len(&payload) > ORIENT_MAX_CHARS {
        if let Some(obj) = payload.as_object_mut() {
            if let Some(m) = obj.remove("matches") {
                let ids: Vec<&str> = m.as_array().into_iter().flatten().filter_map(|n| n["id"].as_str()).collect();
                obj.insert("matches".into(), serde_json::json!(ids));
            }
        }
    }
    let mut text = serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_string());
    if text.len() > ORIENT_MAX_CHARS {
        text = clip(&text, ORIENT_MAX_CHARS);
    }
    text
}

/// The other sessions live in this project, and the import graph to see
/// where their work and the caller's meet. Awareness only: nothing is ever
/// refused on its account.
struct Peers {
    live: Vec<scryer_core::session::LiveSession>,
    edges: Vec<(String, String)>,
}

impl Peers {
    fn of(server: &ScryerServer, model_ref: &ModelRef, tree: &scryer_core::derived_tree::DerivedTree) -> Self {
        let me = server.session_id(model_ref);
        let live = scryer_core::session::live_sessions(model_ref, me.as_deref());
        let edges = if live.is_empty() { Vec::new() } else { derived::import_edges(model_ref, tree).unwrap_or_default() };
        Peers { live, edges }
    }

    /// One line each, only when there is something to say: who else is
    /// editing `file`, the todo tests in it another session owns, and where
    /// a change here reaches files another session is editing.
    fn lines(&self, tree: &scryer_core::derived_tree::DerivedTree, file: &str) -> Vec<(&'static str, String)> {
        let name = |s: &scryer_core::session::LiveSession| {
            let short: String = s.id.chars().take(8).collect();
            match &s.task {
                Some(t) => format!("{short} ({})", clip(t, 40)),
                None => short,
            }
        };
        let mut out = Vec::new();
        let editing: Vec<String> = self.live.iter().filter(|s| s.touched.iter().any(|f| f == file)).map(name).collect();
        if !editing.is_empty() {
            out.push(("alsoEditing", format!("Also edited by live session {}", editing.join("; "))));
        }
        let todos: Vec<String> = self
            .live
            .iter()
            .flat_map(|s| s.stubs.iter().filter(|(f, _)| f == file).map(move |(_, t)| (s, t)))
            .filter(|(_, t)| tree.claims.iter().any(|c| c.file == file && &c.junit_name == *t && c.planned))
            .map(|(s, t)| format!("{t} ({})", s.id.chars().take(8).collect::<String>()))
            .collect();
        if !todos.is_empty() {
            out.push(("othersTodos", format!("Todo tests here owned by another session: {}", todos.join(", "))));
        }
        let files = [file.to_string()];
        let reached = derived::reach(&self.edges, &files);
        let overlaps: Vec<String> = self
            .live
            .iter()
            .filter_map(|s| {
                let hit: Vec<&str> =
                    s.touched.iter().map(String::as_str).filter(|f| *f != file && reached.contains(f)).collect();
                (!hit.is_empty()).then(|| format!("{}: {}", name(s), clip(&hit.join(", "), 120)))
            })
            .collect();
        if !overlaps.is_empty() {
            out.push(("radiusOverlap", format!("A change here reaches files another live session is editing — {}", overlaps.join("; "))));
        }
        out
    }
}

/// What the tree says about one file: its node chain, layer, module
/// description, claims and where new stubs go — and what other live sessions
/// are doing near it.
fn file_report(loaded: &derived::Loaded, peers: &Peers, file: &str, claims_cap: Option<usize>) -> serde_json::Value {
    let tree = &loaded.tree;
    let model = &tree.planned;
    let node = derived::file_node(tree, model, file);
    let claims = derived::file_claims(tree, node, file);
    let shown = claims_cap.unwrap_or(claims.len());
    let more = claims.len().saturating_sub(shown);
    let mut v = serde_json::json!({
        "file": file,
        "node": node.map(|n| n.id.clone()),
        "path": node.map(|n| derived::chain(model, &n.id)),
        "layer": node.and_then(|n| n.layer.clone()),
        "description": node.and_then(|n| n.description.clone()),
        "probe": node.and_then(|n| loaded.node_probe(&n.id)),
        "claims": claims.iter().take(shown).map(|c| loaded.claim_line(c)).collect::<Vec<_>>(),
        "moreClaims": (more > 0).then(|| format!("{more} more — locate {{file}}")),
        "stubs": derived::stub_file(tree, node, file),
    });
    if node.is_none() {
        v["note"] = serde_json::json!("No container or component holds this file — dark code, or not product code.");
    }
    for (key, line) in peers.lines(tree, file) {
        v[key] = serde_json::json!(line);
    }
    strip_fields_compact(&mut v);
    v
}

#[tool_router(router = tool_router_read, vis = "pub(crate)")]
impl ScryerServer {
    #[tool(
        description = "Read the C4 tree derived from the code: containers from packaging, components from \
         modules, claims from behaviour-test descriptions nested by test level. With no `node`: \
         the overview down to components with counts. With a `node` id: its subtree with \
         claims, links, test map and boundaries. `layer: \"committed\"` leaves out todo stubs."
    )]
    fn read_model(
        &self,
        Parameters(req): Parameters<ReadModelRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = resolve_model_ref(req.project.as_deref())?;
        let loaded = match load_or_err(&model_ref) {
            Ok(t) => t,
            Err(e) => return Ok(e),
        };
        let model = layer_of(&loaded.tree, req.layer);

        // Above this serialized size a subtree risks blowing the agent's
        // context, so it degrades to a child skeleton instead of dumping.
        const DETAIL_LIMIT: usize = 50_000;

        let Some(node_id) = req.node.as_deref() else {
            let payload = overview_payload(model);
            return Ok(CallToolResult::success(vec![Content::text(
                serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_string()),
            )]));
        };

        let mut payload = match subtree_payload(model, node_id) {
            Ok(p) => p,
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(e)])),
        };
        strip_fields_compact(&mut payload);
        let detail = serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_string());
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
            serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_string()),
        )]))
    }

    #[tool(
        description = "Search the derived tree by free text: space-separated terms must ALL match somewhere on \
         the node (name, module description, technology, claims, property labels), as substring \
         or close edit-distance. Ranked, each hit with id, kind, breadcrumb, `score` and matched \
         fields. Optional `kind` filter; top 50. Then read_model `{node}` into a hit."
    )]
    fn search_model(
        &self,
        Parameters(req): Parameters<SearchModelRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = resolve_model_ref(req.project.as_deref())?;
        let loaded = match load_or_err(&model_ref) {
            Ok(t) => t,
            Err(e) => return Ok(e),
        };
        let model = layer_of(&loaded.tree, req.layer);

        let kind_filter = match req.kind.as_deref() {
            Some(k) => Some(parse_kind(k)?),
            None => None,
        };
        let terms: Vec<String> = req.query.split_whitespace().map(|t| t.to_lowercase()).collect();
        if terms.is_empty() {
            return Ok(CallToolResult::error(vec![Content::text("Empty query.")]));
        }

        const CAP: usize = 50;
        // Collect every matching node with its rank score, then sort and cap —
        // fuzzy matches mean node order is no longer a good proxy for relevance.
        let mut scored: Vec<(f64, serde_json::Value)> = Vec::new();
        for n in &model.nodes {
            if kind_filter.as_ref().is_some_and(|k| &n.kind != k) {
                continue;
            }
            let Some((score, matched)) = score_node(&node_fields(n), &terms) else {
                continue;
            };
            scored.push((
                score,
                serde_json::json!({
                    "id": n.id,
                    "kind": kind_str(&n.kind),
                    "path": breadcrumb(model, &n.id),
                    "score": (score * 100.0).round() / 100.0,
                    "matched": matched,
                }),
            ));
        }

        let truncated = scored.len() > CAP;
        // Stable sort by descending score keeps tree order for ties.
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        let results: Vec<serde_json::Value> = scored.into_iter().take(CAP).map(|(_, v)| v).collect();

        let payload = serde_json::json!({
            "query": req.query,
            "hits": results.len(),
            "truncated": truncated,
            "results": results,
        });
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_string()),
        )]))
    }

    #[tool(
        description = "Look up one project-relative `file`: its node chain, its layer when the container \
         declares a style in .scryer/structure.toml, the module description, every claim that \
         speaks for it (the tests in it, then its component's) with status, test level and JUnit \
         name, and the test file where new todo stubs go. A structural violation is fixed by \
         moving code, never by changing the style. The single-file form of orient."
    )]
    fn locate(
        &self,
        Parameters(req): Parameters<LocateRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = resolve_model_ref(req.project.as_deref())?;
        let loaded = match load_or_err(&model_ref) {
            Ok(t) => t,
            Err(e) => return Ok(e),
        };
        let tree = &loaded.tree;
        let file = normalize_project_rel(&model_ref, &req.file);
        let peers = Peers::of(self, &model_ref, tree);
        let mut payload = file_report(&loaded, &peers, &file, Some(0));
        let node = derived::file_node(tree, &tree.planned, &file);
        let claims: Vec<serde_json::Value> = derived::file_claims(tree, node, &file)
            .into_iter()
            .map(|c| {
                serde_json::json!({
                    "claim": c.description,
                    "status": loaded.claim_status(c),
                    "level": derived::test_level(c.level),
                    "test": c.junit_name,
                    "at": format!("{}:{}", c.file, c.line),
                    "concerns": c.concerns,
                })
            })
            .collect();
        if let Some(obj) = payload.as_object_mut() {
            obj.remove("moreClaims");
            obj.insert("claims".into(), serde_json::json!(claims));
        }
        strip_fields_compact(&mut payload);
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_string()),
        )]))
    }

    #[tool(
        description = "The front door for a coding task: pass the `task` in a few words and/or the \
         project-relative `files` it touches. Returns, capped at ~4k chars: per file its node \
         chain, layer, module description, one line per claim with status, `stubs` — the test \
         file where each claim you will satisfy goes as a todo test before you build — and what \
         other live sessions are doing near it; per task the best-matching nodes. Name claims in \
         the domain's plain words; a cross-cutting concern is a trailing [tag]."
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
                "Give `task` (a few words) and/or `files` — orient scopes the tree to what you are \
                 about to do. For the whole tree use read_model.",
            )]));
        }
        // The first task this session orients on titles it.
        if let (Some(task), Some(sid)) = (task, self.session_id(&model_ref)) {
            let _ = scryer_core::session::record_task(&model_ref, &sid, task);
        }
        let loaded = match load_or_err(&model_ref) {
            Ok(t) => t,
            Err(e) => return Ok(e),
        };
        let tree = &loaded.tree;
        let peers = Peers::of(self, &model_ref, tree);

        let mut files_out: Vec<serde_json::Value> = files
            .iter()
            .take(ORIENT_FILES)
            .map(|f| file_report(&loaded, &peers, &normalize_project_rel(&model_ref, f), Some(ORIENT_CLAIMS_PER_FILE)))
            .collect();
        if files.len() > ORIENT_FILES {
            files_out.push(serde_json::json!({
                "note": format!("{} more file(s) not looked up — locate them individually", files.len() - ORIENT_FILES),
            }));
        }

        // Task → best-matching nodes. Lenient scoring (unlike search_model's
        // strict AND): a task sentence carries filler words, so a node ranks by
        // the terms it DOES clear — at least one required — plus a coverage
        // bonus, so multi-term matches outrank one-word coincidences.
        let mut matches_out: Vec<serde_json::Value> = Vec::new();
        if let Some(t) = task {
            let model = &tree.planned;
            let terms: Vec<String> = t.split_whitespace().map(|w| w.to_lowercase()).collect();
            let mut scored: Vec<(f64, &Node)> = Vec::new();
            for n in model.nodes.iter().filter(|n| n.kind != Kind::Symbol) {
                let fields: Vec<String> = node_fields(n).into_iter().map(|(_, v)| v.to_lowercase()).collect();
                let (mut total, mut cleared) = (0.0_f64, 0usize);
                for term in &terms {
                    let best = fields.iter().map(|fl| term_field_score(term, fl).0).fold(0.0_f64, f64::max);
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
            for (_, n) in scored.iter().take(3) {
                let planned = tree.claims.iter().filter(|c| c.node_id == n.id && c.planned).count();
                let mut v = serde_json::json!({
                    "id": n.id,
                    "kind": kind_str(&n.kind),
                    "path": breadcrumb(model, &n.id),
                    "claims": n.responsibilities.len(),
                    "planned": (planned > 0).then_some(planned),
                });
                strip_fields_compact(&mut v);
                matches_out.push(v);
            }
        }

        let mut payload = serde_json::json!({ "files": files_out, "matches": matches_out });
        strip_fields_compact(&mut payload);
        Ok(CallToolResult::success(vec![Content::text(fit_orient(payload))]))
    }
}

/// A node's searchable fields, tagged by where they live.
fn node_fields(n: &Node) -> Vec<(&'static str, String)> {
    let mut fields: Vec<(&str, String)> = vec![("name", n.name.clone())];
    if let Some(d) = &n.description {
        fields.push(("description", d.clone()));
    }
    if let Some(t) = &n.technology {
        fields.push(("technology", t.clone()));
    }
    for r in &n.responsibilities {
        fields.push(("claim", r.statement.clone()));
    }
    for p in &n.properties {
        fields.push(("property", p.label.clone()));
    }
    fields
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::fixture::*;

    fn orient(dir: &tempfile::TempDir, task: Option<&str>, files: &[&str]) -> CallToolResult {
        orient_as(&ScryerServer::new(), dir, task, files)
    }

    fn orient_as(server: &ScryerServer, dir: &tempfile::TempDir, task: Option<&str>, files: &[&str]) -> CallToolResult {
        server
            .orient(Parameters(OrientRequest {
                project: project_arg(dir),
                task: task.map(str::to_string),
                files: Some(files.iter().map(|f| f.to_string()).collect()),
            }))
            .unwrap()
    }

    /// For a file, orient gives its node chain, module description, each
    /// claim with its status, and the test file where new stubs go; for a
    /// task, the nodes it matches.
    #[test]
    fn orient_reports_each_file_and_the_task_matches() {
        let dir = project();
        let out = json_of(&orient(&dir, Some("persist text"), &["crates/notes/src/draft.rs", "crates/notes/src/sync.rs"]));
        let draft = &out["files"][0];
        assert_eq!(draft["path"], "notes / notes / draft", "{out}");
        assert_eq!(draft["description"], "Holds the user's draft.");
        let claims: Vec<&str> = draft["claims"].as_array().unwrap().iter().filter_map(|c| c.as_str()).collect();
        assert!(claims.iter().any(|c| c.starts_with("[stale] When a save fails, keeps the draft [persistence]")), "{claims:?}");
        assert!(claims.iter().any(|c| c.starts_with("[planned] Merges two drafts")), "{claims:?}");
        assert_eq!(draft["stubs"], "crates/notes/src/draft.rs", "Rust unit tests go in the file's own test module");
        assert_eq!(out["files"][1]["stubs"], "crates/notes/src/sync.rs");
        assert!(out["matches"].as_array().unwrap().iter().any(|m| m["path"].as_str().unwrap().ends_with("store")), "{out}");
        for gone in ["pending", "drift", "rules", "phase", "state"] {
            assert!(out.get(gone).is_none(), "orient no longer carries {gone}");
        }
    }

    /// With another session live in the project, orient says per file who
    /// else is editing it, which of its todo tests that session owns, and
    /// where a change here reaches files it is editing — and says nothing of
    /// the caller's own work.
    #[test]
    fn orient_names_what_other_live_sessions_are_doing_near_each_file() {
        let dir = project();
        let r = ModelRef::ProjectLocal(dir.path().to_path_buf());
        let draft = "crates/notes/src/draft.rs";
        let store = "crates/notes/src/store/mod.rs";
        scryer_core::session::record_task(&r, "peer-session-1", "split drafts").unwrap();
        scryer_core::session::record_touch(&r, "peer-session-1", draft).unwrap();
        scryer_core::session::stop(&r, "peer-session-1", Some(&[(draft.to_string(), "merges".to_string())]), Vec::new())
            .unwrap();
        scryer_core::session::record_touch(&r, "me", store).unwrap();

        let me = ScryerServer::for_session("me");
        let out = json_of(&orient_as(&me, &dir, None, &[draft, store]));
        let (d, s) = (&out["files"][0], &out["files"][1]);
        assert_eq!(d["alsoEditing"], "Also edited by live session peer-ses (split drafts)", "{out}");
        assert_eq!(d["othersTodos"], "Todo tests here owned by another session: merges (peer-ses)");
        assert!(s["radiusOverlap"].as_str().unwrap().contains(draft), "store's change reaches draft.rs: {out}");
        assert!(s.get("alsoEditing").is_none() && d.get("radiusOverlap").is_none(), "{out}");

        let alone = json_of(&orient_as(&ScryerServer::for_session("peer-session-1"), &dir, None, &[draft]));
        assert!(alone["files"][0].get("othersTodos").is_none(), "a session's own todos are not news to it: {alone}");
    }

    /// However many files and claims are asked about, orient's answer stays
    /// under its cap, with what it shed counted rather than dropped silently.
    #[test]
    fn orient_stays_under_its_cap() {
        let dir = project();
        let tests: String = (0..60)
            .map(|i| format!("    /// When request number {i} arrives with a long enough description to take room, answers it\n    #[test]\n    fn t{i}() {{}}\n"))
            .collect();
        write(dir.path(), "crates/notes/src/big.rs", &format!("pub fn big() {{}}\n#[cfg(test)]\nmod tests {{\n{tests}}}\n"));
        let files: Vec<String> = (0..10).map(|_| "crates/notes/src/big.rs".to_string()).collect();
        let files: Vec<&str> = files.iter().map(String::as_str).collect();
        let text = text_of(&orient(&dir, Some("answer requests"), &files));
        assert!(text.len() <= ORIENT_MAX_CHARS, "{} chars", text.len());
        assert!(text.contains("claims — locate {file}"), "{text}");
    }

    /// locate on a test file lists the claims written in it with their
    /// level, status and JUnit name; on a file no node holds it says so.
    #[test]
    fn locate_lists_a_files_claims_with_their_tests() {
        let dir = project();
        let server = ScryerServer::new();
        let at = |file: &str| {
            json_of(
                &server
                    .locate(Parameters(LocateRequest { project: project_arg(&dir), file: file.into() }))
                    .unwrap(),
            )
        };
        let it = at("crates/notes/tests/roundtrip.rs");
        let claim = &it["claims"][0];
        assert_eq!(claim["claim"], "Saves and reloads a draft unchanged", "{it}");
        assert_eq!(claim["level"], "integration");
        assert_eq!(claim["test"], "roundtrip");
        assert_eq!(claim["status"], "stale", "no report has covered it yet");
        let abs = format!("{}/crates/notes/src/draft.rs", dir.path().display());
        assert_eq!(at(&abs)["file"], "crates/notes/src/draft.rs", "an absolute path is accepted");
        assert!(at("README.md")["note"].as_str().unwrap().contains("No container or component"));
    }

    /// read_model reads the derived tree: the overview has the crate and its
    /// components, and the committed layer leaves the todo stub out.
    #[test]
    fn read_model_reads_the_derived_tree_by_layer() {
        let dir = project();
        let server = ScryerServer::new();
        let read = |node: Option<String>, layer: Layer| {
            json_of(&server.read_model(Parameters(ReadModelRequest { project: project_arg(&dir), node, layer })).unwrap())
        };
        let text = read(None, Layer::Plan).to_string();
        assert!(text.contains("\"draft\"") && text.contains("\"store\""), "{text}");
        let tree = derived::load(&ModelRef::ProjectLocal(dir.path().to_path_buf())).unwrap().tree;
        let draft = tree.planned.nodes.iter().find(|n| n.name == "draft").unwrap().id.clone();
        let count = |layer| read(Some(draft.clone()), layer)["node"]["responsibilities"].as_array().unwrap().len();
        assert_eq!(count(Layer::Plan), 2);
        assert_eq!(count(Layer::Committed), 1);
    }

    /// search_model finds a node by its claims' words.
    #[test]
    fn search_model_finds_nodes_by_claim_text() {
        let dir = project();
        let out = json_of(
            &ScryerServer::new()
                .search_model(Parameters(SearchModelRequest {
                    project: project_arg(&dir),
                    query: "merges drafts".into(),
                    kind: Some("component".into()),
                    layer: Layer::Plan,
                }))
                .unwrap(),
        );
        assert_eq!(out["results"][0]["path"], "notes / notes / draft", "{out}");
    }
}
