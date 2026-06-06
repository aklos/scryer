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

/// Placeholder for tools that take no parameters.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub(crate) struct EmptyRequest {}

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

#[tool_router(router = tool_router_read, vis = "pub(crate)")]
impl ScryerServer {
    #[tool(
        description = "Read the architecture model. With NO `node`, returns the OVERVIEW: the whole tree down to components (symbols excluded) with responsibility/property counts — small and safe, the right first read. Pass a `node` id to read THAT node's full subtree: its descendants (including symbols), responsibilities, properties, links, `referencesForChildren` (the only nodes its children may link to), and the subtree's source map + boundaries. Drill into a component to see its symbols. If a requested subtree is too large to return whole, you get its direct-child skeleton plus guidance to drill further. The MCP baseline is updated on every call."
    )]
    fn read_model(
        &self,
        Parameters(req): Parameters<ReadModelRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = resolve_model_ref(req.project.as_deref())?;
        *self.active_model.lock().unwrap() = Some(model_ref.clone());

        let model = match scryer_core::read_model_at(&model_ref) {
            Ok(m) => m,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to read model at {}: {}",
                    model_ref, e
                ))]));
            }
        };
        let _ = scryer_core::save_baseline_at(&model_ref, &model);

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
        description = "Search the model for nodes matching free text. Case-insensitive; space-separated terms must ALL match somewhere on the node (name, description, technology, responsibility statements, or property labels). Returns each hit's id, kind, breadcrumb path, and the fields that matched — so you can locate a concept in a large model and then `read_model {node}` into it. Optional `kind` filter. Capped at 50 hits."
    )]
    fn search_model(
        &self,
        Parameters(req): Parameters<SearchModelRequest>,
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
        let _ = scryer_core::save_baseline_at(&model_ref, &model);

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
        let mut hits: Vec<serde_json::Value> = Vec::new();
        let mut truncated = false;
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
            // AND across terms: every term must appear in at least one field.
            let hay: Vec<String> = fields.iter().map(|(_, v)| v.to_lowercase()).collect();
            let all_match = terms
                .iter()
                .all(|t| hay.iter().any(|h| h.contains(t)));
            if !all_match {
                continue;
            }
            if hits.len() >= CAP {
                truncated = true;
                break;
            }
            // Report the specific fields that contained any term.
            let matched: Vec<serde_json::Value> = fields
                .iter()
                .filter(|(_, v)| {
                    let lv = v.to_lowercase();
                    terms.iter().any(|t| lv.contains(t))
                })
                .map(|(where_, v)| serde_json::json!({ "in": where_, "text": v }))
                .collect();
            hits.push(serde_json::json!({
                "id": n.id,
                "kind": kind_str(&n.kind),
                "path": breadcrumb(&model, &n.id),
                "matched": matched,
            }));
        }

        let payload = serde_json::json!({
            "query": req.query,
            "hits": hits.len(),
            "truncated": truncated,
            "results": hits,
        });
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string()),
        )]))
    }

    #[tool(
        description = "What model intent is NOT yet reflected in code — the model→code work outstanding. Returns responsibilities, properties, AND component visuals (the `preview`) at status `proposed` (no code yet) or `changed` (spec/visual edited after implementation, needs re-implementation), plus nodes flagged `deprecated` (delete the code) or `relocated` (move the code), each with its breadcrumb path and source anchors. Call this to find what needs implementing or syncing to the codebase."
    )]
    fn get_unimplemented(
        &self,
        Parameters(req): Parameters<GetUnimplementedRequest>,
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
        let _ = scryer_core::save_baseline_at(&model_ref, &model);

        use scryer_core::Status;
        let outstanding = |s: &Option<Status>| {
            matches!(s, Some(Status::Proposed) | Some(Status::Changed))
        };

        let mut nodes_out: Vec<serde_json::Value> = Vec::new();
        let (mut n_proposed, mut n_changed, mut n_deprecated, mut n_relocated) = (0, 0, 0, 0);

        for n in &model.nodes {
            let resp_items: Vec<serde_json::Value> = n
                .responsibilities
                .iter()
                .filter(|r| outstanding(&r.status))
                .map(|r| {
                    match r.status {
                        Some(Status::Proposed) => n_proposed += 1,
                        Some(Status::Changed) => n_changed += 1,
                        _ => {}
                    }
                    let sources = model.source_map.get(&r.id);
                    serde_json::json!({
                        "id": r.id,
                        "statement": r.statement,
                        "status": r.status,
                        "sources": sources,
                    })
                })
                .collect();

            let prop_items: Vec<serde_json::Value> = n
                .properties
                .iter()
                .filter(|p| outstanding(&p.status))
                .map(|p| {
                    match p.status {
                        Some(Status::Proposed) => n_proposed += 1,
                        Some(Status::Changed) => n_changed += 1,
                        _ => {}
                    }
                    serde_json::json!({ "label": p.label, "status": p.status })
                })
                .collect();

            // A node's appearance is status-bearing too: a `changed` look means
            // the code drifted from the modeled appearance; `proposed` means a
            // planned look not yet built. Both are outstanding model→code work.
            let appearance_status = n.appearance.as_ref().and_then(|a| a.status);
            let appearance_outstanding = outstanding(&appearance_status);
            if appearance_outstanding {
                match appearance_status {
                    Some(Status::Proposed) => n_proposed += 1,
                    Some(Status::Changed) => n_changed += 1,
                    _ => {}
                }
            }

            let deprecated = n.deprecated == Some(true);
            let relocated = n.relocated == Some(true);
            if deprecated {
                n_deprecated += 1;
            }
            if relocated {
                n_relocated += 1;
            }

            if resp_items.is_empty()
                && prop_items.is_empty()
                && !deprecated
                && !relocated
                && !appearance_outstanding
            {
                continue;
            }
            // The declaration source for a data-shape node is keyed by node id.
            let decl_source = if prop_items.is_empty() {
                None
            } else {
                model.source_map.get(&n.id)
            };
            let mut v = serde_json::json!({
                "id": n.id,
                "name": n.name,
                "kind": kind_str(&n.kind),
                "path": breadcrumb(&model, &n.id),
                "deprecated": deprecated,
                "relocated": relocated,
                "relocatedTo": n.relocated_to,
                "responsibilities": resp_items,
                "properties": prop_items,
                "appearanceStatus": if appearance_outstanding {
                    serde_json::to_value(appearance_status).unwrap_or(serde_json::Value::Null)
                } else {
                    serde_json::Value::Null
                },
                "declSource": decl_source,
            });
            strip_fields_compact(&mut v);
            nodes_out.push(v);
        }

        let payload = serde_json::json!({
            "summary": {
                "toImplement": n_proposed,
                "toReimplement": n_changed,
                "nodesToDelete": n_deprecated,
                "nodesToMove": n_relocated,
            },
            "clean": nodes_out.is_empty(),
            "nodes": nodes_out,
        });
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string()),
        )]))
    }

    #[tool(
        description = "Return the scryer modeling rules — the constraints that govern responsibilities, decomposition, groups, and link semantics. Call before building or editing a model."
    )]
    fn get_rules(
        &self,
        Parameters(_): Parameters<EmptyRequest>,
    ) -> Result<CallToolResult, McpError> {
        Ok(CallToolResult::success(vec![Content::text(
            scryer_core::rules::RULES.to_string(),
        )]))
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
        description = "Run the structural validator. Returns a list of warnings: parent-kind mismatches, unknown link endpoints, group members at mixed levels, and source-map entries that reference unknown ids. An empty list means the model is structurally clean (does NOT check responsibility quality)."
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::handler::server::wrapper::Parameters;
    use scryer_core::{Kind, ModelRef, Node, Responsibility, ScryModel, Status};

    fn node(id: &str, kind: Kind, name: &str, parent: Option<&str>) -> Node {
        Node {
            id: id.into(),
            kind,
            name: name.into(),
            parent_id: parent.map(|p| p.into()),
            external: None,
            technology: None,
            description: None,
            responsibilities: Vec::new(),
            properties: Vec::new(),
            icon: None,
            visual: None,
            appearance: None,
            deprecated: None,
            relocated: None,
            locked: None,
            relocated_to: None,
            relocated_from: None,
        }
    }

    fn resp(id: &str, statement: &str) -> Responsibility {
        Responsibility {
            id: id.into(),
            statement: statement.into(),
            status: Some(Status::Implemented),
            vagrant: None,
            locked: None,
            relocated_to: None,
            relocated_from: None,
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
            }))
            .unwrap();
        let v = result_json(&r);
        assert_eq!(v["hits"], 1);
        assert_eq!(v["results"][0]["id"], "node-4");
        assert_eq!(v["results"][0]["path"], "Acme / API / Auth / verify_token");
        assert_eq!(v["results"][0]["matched"][0]["in"], "responsibility");
    }

    #[test]
    fn sync_status_lists_proposed_changed_and_flags() {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());
        let mut m = ScryModel::new();
        m.nodes.push(node("node-1", Kind::System, "Acme", None));
        let mut c = node("node-2", Kind::Component, "Billing", Some("node-1"));
        c.responsibilities = vec![
            resp("resp-impl", "charges the card"),       // implemented → excluded
            resp("resp-prop", "issues refunds"),         // proposed → included
            resp("resp-chg", "emails a receipt"),        // changed → included
        ];
        c.responsibilities[0].status = Some(Status::Implemented);
        c.responsibilities[1].status = Some(Status::Proposed);
        c.responsibilities[2].status = Some(Status::Changed);
        m.nodes.push(c);
        let mut dead = node("node-3", Kind::Component, "Legacy", Some("node-1"));
        dead.deprecated = Some(true);
        m.nodes.push(dead);
        // a node whose ONLY outstanding signal is a changed visual preview
        let mut dash = node("node-4", Kind::Component, "Dashboard", Some("node-1"));
        dash.appearance = Some(scryer_core::Appearance {
            status: Some(Status::Changed),
            dist_path: None,
            built_at: None,
            source_hash: None,
        });
        m.nodes.push(dash);
        // a source anchor for the proposed responsibility
        m.source_map.insert(
            "resp-prop".into(),
            vec![scryer_core::SourceLocation {
                pattern: "src/billing.rs".into(),
                symbol: Some("refund".into()),
                line: None,
                end_line: None,
                command: None,
            }],
        );
        scryer_core::write_model_at(&model_ref, &m).unwrap();
        let server = ScryerServer::new();

        let r = server
            .get_unimplemented(Parameters(GetUnimplementedRequest {
                project: Some(dir.path().to_string_lossy().to_string()),
            }))
            .unwrap();
        let v = result_json(&r);
        assert_eq!(v["clean"], false);
        assert_eq!(v["summary"]["toImplement"], 1);
        // resp-chg (1) + the changed Dashboard preview (1)
        assert_eq!(v["summary"]["toReimplement"], 2);
        assert_eq!(v["summary"]["nodesToDelete"], 1);
        // implemented responsibility is not surfaced
        let dump = serde_json::to_string(&v).unwrap();
        assert!(!dump.contains("charges the card"));
        assert!(dump.contains("issues refunds"));
        assert!(dump.contains("src/billing.rs")); // source anchor carried through
        // the deprecated node appears even with no outstanding responsibilities
        assert!(dump.contains("node-3"));
        // the node whose only outstanding signal is a changed appearance is surfaced
        assert!(dump.contains("node-4"));
        assert!(dump.contains("\"appearanceStatus\":\"changed\""));
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
            }))
            .unwrap();
        assert_eq!(result_json(&r)["hits"], 0);
        // kind filter excludes the matching component ("Auth" is a component)
        let r = server
            .search_model(Parameters(SearchModelRequest {
                project: Some(project),
                query: "Auth".into(),
                kind: Some("symbol".into()),
            }))
            .unwrap();
        assert_eq!(result_json(&r)["hits"], 0);
    }
}
