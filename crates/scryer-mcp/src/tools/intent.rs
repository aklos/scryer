//! Intent write tools — the agent's preferred path for building a model.
//!
//! Each tool takes INTENT (a name, plain responsibility statements, the source
//! location the agent already holds from the codebase context) and builds the
//! node itself: it mints the node id and the `resp-` ids, fixes the kind from
//! the parent level (validating the parent is the right kind), defaults
//! responsibility status to `implemented` (a bootstrap describes existing
//! code), and — for symbols — writes the source map anchored to the file +
//! symbol name. The agent never assembles the JSON shape or hand-mints ids.
//!
//! The raw `set_model` / `set_node` / `add_nodes` tools remain for the legacy
//! flow and canvas-driven edits, but the new modeling prompts use only these.

use crate::helpers::*;
use crate::server::ScryerServer;
use crate::types::*;
use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, Content},
    tool, tool_router, ErrorData as McpError,
};
use scryer_core::{
    Group, Kind, ModelRef, Node, Responsibility, SchemaProperty, ScryModel, Source, SourceLocation,
    Status,
};

/// Mints sequential `resp-N` ids across a single tool call, seeded past every
/// existing responsibility id (on nodes AND groups, so it can't collide with a
/// group-owned id).
struct RespMinter {
    next: u64,
}

impl RespMinter {
    fn new(model: &ScryModel) -> Self {
        let max = model
            .nodes
            .iter()
            .flat_map(|n| n.responsibilities.iter())
            .chain(model.groups.iter().flat_map(|g| g.responsibilities.iter()))
            .filter_map(|r| r.id.strip_prefix("resp-").and_then(|s| s.parse::<u64>().ok()))
            .max()
            .unwrap_or(0);
        Self { next: max + 1 }
    }

    /// Build `implemented` responsibilities from plain statements, skipping blanks.
    fn build(&mut self, statements: &[String]) -> Vec<Responsibility> {
        statements
            .iter()
            .filter(|s| !s.trim().is_empty())
            .map(|s| {
                let id = format!("resp-{}", self.next);
                self.next += 1;
                Responsibility {
                    id,
                    statement: s.trim().to_string(),
                    status: Some(Status::Implemented),
                    vagrant: None,
                    locked: None,
                    relocated_to: None,
                    relocated_from: None,
                    directives: Vec::new(),
                    last_touched_at: None,
                }
            })
            .collect()
    }
}

fn err(msg: impl Into<String>) -> CallToolResult {
    CallToolResult::error(vec![Content::text(msg.into())])
}

/// Read the model or return an error result.
fn read_model(model_ref: &ModelRef) -> Result<ScryModel, CallToolResult> {
    scryer_core::read_model_at(model_ref)
        .map_err(|e| err(format!("Failed to read model at {}: {}", model_ref, e)))
}

/// Verify a parent node exists and is the expected kind. Returns the error
/// result to surface, or `None` when the parent is valid.
fn check_parent(model: &ScryModel, parent_id: &str, want: Kind) -> Option<CallToolResult> {
    match model.nodes.iter().find(|n| n.id == parent_id) {
        None => Some(err(format!("Parent node '{}' not found", parent_id))),
        Some(p) if p.kind != want => Some(err(format!(
            "Parent '{}' must be a {}, but it is a {}",
            parent_id,
            kind_str(&want),
            kind_str(&p.kind)
        ))),
        _ => None,
    }
}

/// A bare node with every optional facet empty — callers set what they need.
fn blank_node(id: String, kind: Kind, name: String, parent_id: Option<String>) -> Node {
    Node {
        id,
        kind,
        name,
        parent_id,
        external: None,
        technology: None,
        description: None,
        responsibilities: Vec::new(),
        properties: Vec::new(),
        cell: None,
        icon: None,
        deprecated: None,
        relocated: None,
        locked: None,
        relocated_to: None,
        relocated_from: None,
    }
}

/// Enforce read-only invariants, write, snapshot the baseline, and return the
/// minted nodes (compact denormalized view) so the agent has their ids.
fn commit(
    model_ref: &ModelRef,
    mut model: ScryModel,
    prior: &ScryModel,
    minted: &[String],
) -> Result<CallToolResult, McpError> {
    enforce_readonly_directives(&mut model, prior);
    enforce_readonly_layout(&mut model, prior);

    if let Err(e) = scryer_core::write_model_at(model_ref, &model) {
        return Ok(err(e));
    }
    let _ = scryer_core::save_baseline_at(model_ref, &model);

    let added: Vec<serde_json::Value> = minted
        .iter()
        .filter_map(|id| model.nodes.iter().find(|n| &n.id == id))
        .map(|n| {
            let mut v = denormalize_node(n, &model);
            strip_fields_compact(&mut v);
            v
        })
        .collect();
    let payload = serde_json::json!({ "added": added });
    Ok(CallToolResult::success(vec![Content::text(
        serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string()),
    )]))
}

#[tool_router(router = tool_router_intent, vis = "pub(crate)")]
impl ScryerServer {
    #[tool(
        description = "Add one or more persons (real users / actors) at the top level. Pass plain responsibility statements — ids and status (implemented) are set for you. Persons link to the SYSTEM, not to its containers."
    )]
    fn add_person(
        &self,
        Parameters(req): Parameters<AddPersonRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = resolve_model_ref(req.project.as_deref())?;
        let _lock = match lock_or_err(&model_ref) {
            Ok(l) => l,
            Err(e) => return Ok(e),
        };
        let mut model = match read_model(&model_ref) {
            Ok(m) => m,
            Err(e) => return Ok(e),
        };
        let prior = model.clone();
        let mut minter = RespMinter::new(&model);
        let mut minted = Vec::new();
        for item in &req.items {
            let id = scryer_core::next_node_id(&model);
            let mut node = blank_node(id.clone(), Kind::Person, item.name.clone(), None);
            node.description = item.description.clone();
            node.responsibilities = minter.build(&item.responsibilities);
            model.nodes.push(node);
            minted.push(id);
        }
        commit(&model_ref, model, &prior, &minted)
    }

    #[tool(
        description = "Add one or more systems at the top level — the system you are modeling, or external third-party systems it depends on (set external=true). Persons and externals link to the system. Pass plain responsibility statements; ids and status are set for you."
    )]
    fn add_system(
        &self,
        Parameters(req): Parameters<AddSystemRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = resolve_model_ref(req.project.as_deref())?;
        let _lock = match lock_or_err(&model_ref) {
            Ok(l) => l,
            Err(e) => return Ok(e),
        };
        let mut model = match read_model(&model_ref) {
            Ok(m) => m,
            Err(e) => return Ok(e),
        };
        let prior = model.clone();
        let mut minter = RespMinter::new(&model);
        let mut minted = Vec::new();
        for item in &req.items {
            let id = scryer_core::next_node_id(&model);
            let mut node = blank_node(id.clone(), Kind::System, item.name.clone(), None);
            node.description = item.description.clone();
            node.technology = item.technology.clone();
            node.external = if item.external { Some(true) } else { None };
            node.responsibilities = minter.build(&item.responsibilities);
            model.nodes.push(node);
            minted.push(id);
        }
        commit(&model_ref, model, &prior, &minted)
    }

    #[tool(
        description = "Add one or more containers under a system. `name` is the role, `technology` is what it IS as software. Pass `boundaryDir` (the container's directory from the codebase context) to set its boundary glob automatically. Give responsibilities at the container's altitude (what the container is accountable for, not what its individual components do). Plain responsibility statements; ids and status set for you."
    )]
    fn add_container(
        &self,
        Parameters(req): Parameters<AddContainerRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = resolve_model_ref(req.project.as_deref())?;
        let _lock = match lock_or_err(&model_ref) {
            Ok(l) => l,
            Err(e) => return Ok(e),
        };
        let mut model = match read_model(&model_ref) {
            Ok(m) => m,
            Err(e) => return Ok(e),
        };
        let prior = model.clone();
        let mut minter = RespMinter::new(&model);
        let mut minted = Vec::new();
        for item in &req.items {
            if let Some(e) = check_parent(&model, &item.parent_id, Kind::System) {
                return Ok(e);
            }
            let id = scryer_core::next_node_id(&model);
            let mut node = blank_node(
                id.clone(),
                Kind::Container,
                item.name.clone(),
                Some(item.parent_id.clone()),
            );
            node.technology = item.technology.clone();
            node.description = item.description.clone();
            node.external = if item.external { Some(true) } else { None };
            node.responsibilities = minter.build(&item.responsibilities);
            model.nodes.push(node);
            if let Some(dir) = item.boundary_dir.as_deref().map(str::trim).filter(|d| !d.is_empty()) {
                model.boundaries.insert(
                    id.clone(),
                    vec![Source {
                        pattern: format!("{}/**/*", dir.trim_end_matches('/')),
                        comment: None,
                    }],
                );
            }
            minted.push(id);
        }
        commit(&model_ref, model, &prior, &minted)
    }

    #[tool(
        description = "Add one or more components under a container. Cluster components from code cohesion + the dependency graph in the provided context — NOT one component per file. Give responsibilities at the component's altitude: one accountability each, NOT what an individual symbol does (that per-handler detail belongs on the symbols below). Plain responsibility statements; ids and status set for you."
    )]
    fn add_component(
        &self,
        Parameters(req): Parameters<AddComponentRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = resolve_model_ref(req.project.as_deref())?;
        let _lock = match lock_or_err(&model_ref) {
            Ok(l) => l,
            Err(e) => return Ok(e),
        };
        let mut model = match read_model(&model_ref) {
            Ok(m) => m,
            Err(e) => return Ok(e),
        };
        let prior = model.clone();
        let mut minter = RespMinter::new(&model);
        let mut minted = Vec::new();
        for item in &req.items {
            if let Some(e) = check_parent(&model, &item.parent_id, Kind::Container) {
                return Ok(e);
            }
            let id = scryer_core::next_node_id(&model);
            let mut node = blank_node(
                id.clone(),
                Kind::Component,
                item.name.clone(),
                Some(item.parent_id.clone()),
            );
            node.description = item.description.clone();
            node.responsibilities = minter.build(&item.responsibilities);
            model.nodes.push(node);
            minted.push(id);
        }
        commit(&model_ref, model, &prior, &minted)
    }

    #[tool(
        description = "Group sibling nodes that ship or package together — a SECONDARY axis, never a substitute for decomposition. `parent_id` = the node whose children you're grouping (the system for a group of containers; a container for a group of components). `member_ids` = the sibling node ids to enclose (2+, all children of parent_id, same level). Optional responsibility statements describe the unit (e.g. 'deploys atomically'). The group id + layout are set for you. Only group genuinely cohesive units; skip when siblings are independent."
    )]
    fn add_group(
        &self,
        Parameters(req): Parameters<AddGroupRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = resolve_model_ref(req.project.as_deref())?;
        let _lock = match lock_or_err(&model_ref) {
            Ok(l) => l,
            Err(e) => return Ok(e),
        };
        let mut model = match read_model(&model_ref) {
            Ok(m) => m,
            Err(e) => return Ok(e),
        };
        let prior = model.clone();
        let mut minter = RespMinter::new(&model);
        let mut minted: Vec<String> = Vec::new();
        for item in &req.items {
            if !model.nodes.iter().any(|n| n.id == item.parent_id) {
                return Ok(err(format!("Parent node '{}' not found", item.parent_id)));
            }
            if item.member_ids.len() < 2 {
                return Ok(err(format!(
                    "Group '{}' needs at least 2 members",
                    item.name
                )));
            }
            // Every member must be an actual child of parent_id (so the group
            // anchors to that node's level and members truly are siblings).
            for mid in &item.member_ids {
                match model.nodes.iter().find(|n| &n.id == mid) {
                    None => return Ok(err(format!("Group member '{}' is not a node", mid))),
                    Some(n) if n.parent_id.as_deref() != Some(item.parent_id.as_str()) => {
                        return Ok(err(format!(
                            "Group member '{}' is not a child of '{}'",
                            mid, item.parent_id
                        )))
                    }
                    _ => {}
                }
            }
            let id = scryer_core::next_group_id(&model);
            model.groups.push(Group {
                id: id.clone(),
                name: item.name.clone(),
                description: item.description.clone(),
                member_ids: item.member_ids.clone(),
                parent_group_id: None,
                parent_node_id: Some(item.parent_id.clone()),
                responsibilities: minter.build(&item.responsibilities),
                cell: None,
                size: None,
                icon: None,
            });
            minted.push(id);
        }
        // Groups aren't nodes, so commit by hand (the node-returning `commit`
        // helper doesn't apply): enforce read-only invariants, write, baseline.
        enforce_readonly_directives(&mut model, &prior);
        enforce_readonly_layout(&mut model, &prior);
        if let Err(e) = scryer_core::write_model_at(&model_ref, &model) {
            return Ok(err(e));
        }
        let _ = scryer_core::save_baseline_at(&model_ref, &model);
        Ok(CallToolResult::success(vec![Content::text(format!(
            "Created {} group(s): {}",
            minted.len(),
            minted.join(", ")
        ))]))
    }

    #[tool(
        description = "Add one or more symbols (one addressable code definition each) under a component. Pass the `sourceFile` (and line/endLine) from the codebase context; the source map is anchored to the file + symbol name for you — no separate update_source_map call. Give `responsibilities` for behavior and/or `properties` for a declared data shape. Plain statements; ids and status set for you."
    )]
    fn add_symbol(
        &self,
        Parameters(req): Parameters<AddSymbolRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = resolve_model_ref(req.project.as_deref())?;
        let _lock = match lock_or_err(&model_ref) {
            Ok(l) => l,
            Err(e) => return Ok(e),
        };
        let mut model = match read_model(&model_ref) {
            Ok(m) => m,
            Err(e) => return Ok(e),
        };
        let prior = model.clone();
        let mut minter = RespMinter::new(&model);
        let mut minted = Vec::new();
        for item in &req.items {
            if let Some(e) = check_parent(&model, &item.parent_id, Kind::Component) {
                return Ok(e);
            }
            let id = scryer_core::next_node_id(&model);
            let mut node = blank_node(
                id.clone(),
                Kind::Symbol,
                item.name.clone(),
                Some(item.parent_id.clone()),
            );
            let resps = minter.build(&item.responsibilities);
            node.properties = item
                .properties
                .iter()
                .map(|p| SchemaProperty {
                    label: p.label.clone(),
                    description: p.description.clone(),
                    status: Some(Status::Implemented),
                    last_touched_at: None,
                })
                .collect();

            // Anchor each responsibility to the file + symbol name (durable over
            // line shifts), and — for a data shape — the declaration block to the
            // symbol node id, using the line range the context provided.
            for r in &resps {
                model.source_map.insert(
                    r.id.clone(),
                    vec![SourceLocation {
                        pattern: item.source_file.clone(),
                        symbol: Some(item.name.clone()),
                        line: None,
                        end_line: None,
                        command: None,
                    }],
                );
            }
            if !node.properties.is_empty() {
                model.source_map.insert(
                    id.clone(),
                    vec![SourceLocation {
                        pattern: item.source_file.clone(),
                        symbol: Some(item.name.clone()),
                        line: item.line,
                        end_line: item.end_line,
                        command: None,
                    }],
                );
            }
            node.responsibilities = resps;
            model.nodes.push(node);
            minted.push(id);
        }
        commit(&model_ref, model, &prior, &minted)
    }

    #[tool(
        description = "Record SEMANTIC drift for a node after comparing its code against its responsibilities. `undescribed`: behaviours the code has that NO responsibility describes — each becomes a vagrant responsibility on the node (implemented, vagrant) for the user to adopt or reject; do NOT report code that changed but still satisfies an existing responsibility (the user doesn't care). `stale`: existing responsibilities whose code no longer discharges them — marked `changed`. Call with empty arrays (or don't call) when the code and the model still agree."
    )]
    fn flag_drift(
        &self,
        Parameters(req): Parameters<FlagDriftRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = resolve_model_ref(req.project.as_deref())?;
        let _lock = match lock_or_err(&model_ref) {
            Ok(l) => l,
            Err(e) => return Ok(e),
        };
        let mut model = match read_model(&model_ref) {
            Ok(m) => m,
            Err(e) => return Ok(e),
        };
        if !model.nodes.iter().any(|n| n.id == req.node_id) {
            return Ok(err(format!("Node '{}' not found", req.node_id)));
        }
        let prior = model.clone();

        // Undescribed behaviour → vagrant responsibilities on the node, each
        // anchored to its source. Pre-filter blanks so items and resps align.
        let items: Vec<&UndescribedItem> = req
            .undescribed
            .iter()
            .filter(|u| !u.statement.trim().is_empty())
            .collect();
        let mut minter = RespMinter::new(&model);
        let statements: Vec<String> = items.iter().map(|u| u.statement.clone()).collect();
        let mut resps = minter.build(&statements);
        for r in resps.iter_mut() {
            r.vagrant = Some(true);
        }
        for (item, r) in items.iter().zip(resps.iter()) {
            model.source_map.insert(
                r.id.clone(),
                vec![SourceLocation {
                    pattern: item.source_file.clone(),
                    symbol: item.symbol.clone(),
                    line: None,
                    end_line: None,
                    command: None,
                }],
            );
        }
        let flagged = resps.len();
        if let Some(node) = model.nodes.iter_mut().find(|n| n.id == req.node_id) {
            node.responsibilities.extend(resps);
        }

        // Stale claims → status `changed` (needs re-confirmation against code).
        let mut staled = 0usize;
        for s in &req.stale {
            let found = model
                .nodes
                .iter_mut()
                .flat_map(|n| n.responsibilities.iter_mut())
                .find(|r| r.id == s.responsibility_id);
            match found {
                Some(r) => {
                    r.status = Some(Status::Changed);
                    staled += 1;
                }
                None => {
                    return Ok(err(format!(
                        "Responsibility '{}' not found",
                        s.responsibility_id
                    )));
                }
            }
        }

        enforce_readonly_directives(&mut model, &prior);
        enforce_readonly_layout(&mut model, &prior);
        if let Err(e) = scryer_core::write_model_at(&model_ref, &model) {
            return Ok(err(e));
        }
        let _ = scryer_core::save_baseline_at(&model_ref, &model);

        let mut msg = format!(
            "Flagged {flagged} undescribed behaviour(s) as vagrant and {staled} stale responsibility(ies) on '{}'.",
            req.node_id
        );
        for s in &req.stale {
            msg.push_str(&format!("\n  changed {}: {}", s.responsibility_id, s.reason));
        }
        Ok(CallToolResult::success(vec![Content::text(msg)]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::handler::server::wrapper::Parameters;

    /// Build a temp project with a single system node and return (server, dir, system_id).
    fn temp_project() -> (ScryerServer, tempfile::TempDir, String) {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());
        let mut model = ScryModel::new();
        model.nodes.push(blank_node(
            "node-1".into(),
            Kind::System,
            "Acme".into(),
            None,
        ));
        scryer_core::write_model_at(&model_ref, &model).unwrap();
        (ScryerServer::new(), dir, "node-1".to_string())
    }

    fn read_back(dir: &tempfile::TempDir) -> ScryModel {
        scryer_core::read_model_at(&ModelRef::ProjectLocal(dir.path().to_path_buf())).unwrap()
    }

    #[test]
    fn intent_tools_build_the_tree_and_source_map() {
        let (server, dir, system_id) = temp_project();
        let project = dir.path().to_string_lossy().to_string();

        // container under the system, with an auto boundary glob
        server
            .add_container(Parameters(AddContainerRequest {
                project: Some(project.clone()),
                items: vec![ContainerItem {
                    parent_id: system_id.clone(),
                    name: "API".into(),
                    technology: Some("Axum".into()),
                    description: None,
                    external: false,
                    responsibilities: vec!["serves the public API".into(), "  ".into()],
                    boundary_dir: Some("crates/api".into()),
                }],
            }))
            .unwrap();
        let m = read_back(&dir);
        let container = m.nodes.iter().find(|n| n.kind == Kind::Container).unwrap();
        let container_id = container.id.clone();
        assert_eq!(container.technology.as_deref(), Some("Axum"));
        // blank statement filtered out; one responsibility, status implemented
        assert_eq!(container.responsibilities.len(), 1);
        assert_eq!(container.responsibilities[0].status, Some(Status::Implemented));
        // auto boundary glob keyed by the container node id
        assert_eq!(
            m.boundaries.get(&container_id).unwrap()[0].pattern,
            "crates/api/**/*"
        );

        // component under the container
        server
            .add_component(Parameters(AddComponentRequest {
                project: Some(project.clone()),
                items: vec![ComponentItem {
                    parent_id: container_id.clone(),
                    name: "Auth".into(),
                    description: None,
                    responsibilities: vec!["authenticates requests".into()],
                }],
            }))
            .unwrap();
        let m = read_back(&dir);
        let component = m.nodes.iter().find(|n| n.kind == Kind::Component).unwrap();
        let component_id = component.id.clone();

        // a data-shape symbol with properties + a responsibility
        server
            .add_symbol(Parameters(AddSymbolRequest {
                project: Some(project.clone()),
                items: vec![SymbolItem {
                    parent_id: component_id.clone(),
                    name: "Session".into(),
                    source_file: "crates/api/src/auth.rs".into(),
                    line: Some(10),
                    end_line: Some(20),
                    responsibilities: vec!["holds the logged-in session".into()],
                    properties: vec![PropertyInput {
                        label: "token".into(),
                        description: "bearer token".into(),
                    }],
                }],
            }))
            .unwrap();
        let m = read_back(&dir);
        let symbol = m.nodes.iter().find(|n| n.kind == Kind::Symbol).unwrap();
        let symbol_id = symbol.id.clone();
        assert_eq!(symbol.properties.len(), 1);
        let resp_id = symbol.responsibilities[0].id.clone();
        // responsibility anchored to file + symbol name, no brittle line numbers
        let resp_loc = &m.source_map.get(&resp_id).unwrap()[0];
        assert_eq!(resp_loc.pattern, "crates/api/src/auth.rs");
        assert_eq!(resp_loc.symbol.as_deref(), Some("Session"));
        assert_eq!(resp_loc.line, None);
        // declaration block keyed by the symbol node id, with the context line range
        let decl = &m.source_map.get(&symbol_id).unwrap()[0];
        assert_eq!(decl.line, Some(10));
        assert_eq!(decl.end_line, Some(20));

        // all ids are unique and the tree is well-formed
        let warnings = crate::validate::validate(&m);
        let hard: Vec<&String> = warnings
            .iter()
            .filter(|w| !(w.contains("disconnected") || w.contains("has no links")))
            .collect();
        assert!(hard.is_empty(), "no hard structural warnings: {hard:?}");
    }

    #[test]
    fn add_group_encloses_sibling_containers() {
        let (server, dir, system_id) = temp_project();
        let project = dir.path().to_string_lossy().to_string();
        server
            .add_container(Parameters(AddContainerRequest {
                project: Some(project.clone()),
                items: vec![
                    ContainerItem {
                        parent_id: system_id.clone(),
                        name: "Web".into(),
                        technology: None,
                        description: None,
                        external: false,
                        responsibilities: vec!["serves the site".into()],
                        boundary_dir: Some("web".into()),
                    },
                    ContainerItem {
                        parent_id: system_id.clone(),
                        name: "Worker".into(),
                        technology: None,
                        description: None,
                        external: false,
                        responsibilities: vec!["runs jobs".into()],
                        boundary_dir: Some("worker".into()),
                    },
                ],
            }))
            .unwrap();
        let m = read_back(&dir);
        let ids: Vec<String> = m
            .nodes
            .iter()
            .filter(|n| n.kind == Kind::Container)
            .map(|n| n.id.clone())
            .collect();
        assert_eq!(ids.len(), 2);

        // group the two containers under the system
        server
            .add_group(Parameters(AddGroupRequest {
                project: Some(project.clone()),
                items: vec![GroupItem {
                    parent_id: system_id.clone(),
                    name: "Backend".into(),
                    description: None,
                    member_ids: ids.clone(),
                    responsibilities: vec!["deploys atomically".into()],
                }],
            }))
            .unwrap();
        let m = read_back(&dir);
        assert_eq!(m.groups.len(), 1);
        let g = &m.groups[0];
        assert_eq!(g.parent_node_id.as_deref(), Some(system_id.as_str()));
        assert_eq!(g.member_ids.len(), 2);
        assert_eq!(g.responsibilities[0].status, Some(Status::Implemented));

        // a member that isn't a child of parent_id is rejected (containers are
        // children of the system, not of another container)
        let res = server
            .add_group(Parameters(AddGroupRequest {
                project: Some(project.clone()),
                items: vec![GroupItem {
                    parent_id: ids[0].clone(),
                    name: "Bad".into(),
                    description: None,
                    member_ids: ids.clone(),
                    responsibilities: vec![],
                }],
            }))
            .unwrap();
        assert!(
            res.is_error.unwrap_or(false),
            "members not children of parent are rejected"
        );
    }

    #[test]
    fn flag_drift_records_vagrant_and_marks_changed() {
        let (server, dir, system_id) = temp_project();
        let project = dir.path().to_string_lossy().to_string();
        server
            .add_container(Parameters(AddContainerRequest {
                project: Some(project.clone()),
                items: vec![ContainerItem {
                    parent_id: system_id,
                    name: "API".into(),
                    technology: None,
                    description: None,
                    external: false,
                    responsibilities: vec!["serves the public API".into()],
                    boundary_dir: Some("api".into()),
                }],
            }))
            .unwrap();
        let m = read_back(&dir);
        let container = m.nodes.iter().find(|n| n.kind == Kind::Container).unwrap();
        let cid = container.id.clone();
        let rid = container.responsibilities[0].id.clone();

        server
            .flag_drift(Parameters(FlagDriftRequest {
                project: Some(project.clone()),
                node_id: cid.clone(),
                undescribed: vec![UndescribedItem {
                    statement: "exposes an undocumented admin endpoint".into(),
                    source_file: "api/admin.rs".into(),
                    symbol: Some("admin_handler".into()),
                }],
                stale: vec![StaleResponsibility {
                    responsibility_id: rid.clone(),
                    reason: "endpoint was removed".into(),
                }],
            }))
            .unwrap();

        let m = read_back(&dir);
        let container = m.nodes.iter().find(|n| n.id == cid).unwrap();
        // original responsibility marked changed
        let orig = container.responsibilities.iter().find(|r| r.id == rid).unwrap();
        assert_eq!(orig.status, Some(Status::Changed));
        // a vagrant responsibility added (implemented + vagrant), source-anchored
        let vagrant = container
            .responsibilities
            .iter()
            .find(|r| r.vagrant == Some(true))
            .expect("a vagrant responsibility was added");
        assert_eq!(vagrant.status, Some(Status::Implemented));
        let anchor = &m.source_map.get(&vagrant.id).unwrap()[0];
        assert_eq!(anchor.pattern, "api/admin.rs");
        assert_eq!(anchor.symbol.as_deref(), Some("admin_handler"));
    }

    #[test]
    fn rejects_wrong_parent_kind() {
        let (server, dir, system_id) = temp_project();
        let project = dir.path().to_string_lossy().to_string();
        // a component's parent must be a container, not the system
        let res = server
            .add_component(Parameters(AddComponentRequest {
                project: Some(project),
                items: vec![ComponentItem {
                    parent_id: system_id,
                    name: "Nope".into(),
                    description: None,
                    responsibilities: vec![],
                }],
            }))
            .unwrap();
        assert!(res.is_error.unwrap_or(false), "wrong parent kind is rejected");
    }
}
