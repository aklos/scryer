//! Atomic write path for codebase-to-model generation.
//!
//! A container modeling agent returns one complete semantic proposal. The
//! server validates it, mints every id, resolves request-local component keys,
//! and performs one locked model write. This avoids the repeated whole-model
//! read/clone/write/baseline cycle of incremental intent calls while preserving
//! the same C4 component and mandatory symbol semantics.

use crate::helpers::*;
use crate::server::ScryerServer;
use crate::types::*;
use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, Content},
    tool, tool_router, ErrorData as McpError,
};
use scryer_core::history::{EventKind, EventRow, HistoryEvent};
use scryer_core::{
    Group, Kind, Link, Node, Responsibility, SchemaProperty, SourceLocation,
};
use std::collections::{HashMap, HashSet};

fn err(msg: impl Into<String>) -> CallToolResult {
    CallToolResult::error(vec![Content::text(msg.into())])
}

struct IdMinter {
    /// Every node, responsibility, and group id this minter must stay clear
    /// of — both layers plus everything it has minted so far.
    taken: HashSet<String>,
}

impl IdMinter {
    fn new(model: &scryer_core::ScryModel) -> Self {
        let mut me = Self { taken: HashSet::new() };
        me.absorb(model);
        me
    }

    /// Reserve every id in `other` too. Used to mint against the UNION of the
    /// committed model and the planned draft: a container commit reads the
    /// committed layer, but a concurrent system-level session may have minted
    /// ids into the planned draft that aren't in committed yet. Seeding from
    /// one layer alone could re-mint those ids and collide when this commit's
    /// subtree is mirrored into planned.
    fn absorb(&mut self, other: &scryer_core::ScryModel) {
        for n in &other.nodes {
            self.taken.insert(n.id.clone());
            for r in &n.responsibilities {
                self.taken.insert(r.id.clone());
            }
        }
        for g in &other.groups {
            self.taken.insert(g.id.clone());
            for r in &g.responsibilities {
                self.taken.insert(r.id.clone());
            }
        }
    }

    fn mint(&mut self, prefix: &str) -> String {
        let id = scryer_core::mint_id_from(prefix, self.taken.iter().map(String::as_str));
        self.taken.insert(id.clone());
        id
    }

    fn node(&mut self) -> String {
        self.mint("node")
    }

    fn resp(&mut self, statement: &str, concern: Option<&str>) -> Responsibility {
        let id = self.mint("resp");
        Responsibility {
            concern: concern.map(Into::into),
            id,
            statement: statement.trim().to_string(),
            vagrant: None,
            stale: None,
            stale_proposal: None,
            directives: Vec::new(),
            last_touched_at: None,
        }
    }

    fn responsibilities(&mut self, statements: &[StatementInput]) -> Vec<Responsibility> {
        statements
            .iter()
            .filter(|s| !s.statement().trim().is_empty())
            .map(|s| self.resp(s.statement(), s.concern()))
            .collect()
    }

    fn group(&mut self) -> String {
        self.mint("group")
    }
}

fn blank_node(id: String, kind: Kind, name: String, parent_id: String) -> Node {
    Node {
        id,
        kind,
        name,
        vagrant: None,
        stale: None,
        parent_id: Some(parent_id),
        external: None,
        technology: None,
        description: None,
        responsibilities: Vec::new(),
        properties: Vec::new(),
        icon: None,
        notes: None,
        position: None,
        directives: Vec::new(),
    }
}

fn validate_request(req: &CommitContainerModelRequest) -> Result<(), String> {
    if req.components.is_empty() {
        return Err("A code-bearing container proposal needs at least one component".into());
    }

    let mut keys = HashSet::new();
    let mut symbol_anchors = HashSet::new();
    for component in &req.components {
        let key = component.key.trim();
        if key.is_empty() {
            return Err("Every component needs a non-empty local key".into());
        }
        if !keys.insert(key) {
            return Err(format!("Duplicate component key '{key}'"));
        }
        if component.name.trim().is_empty() {
            return Err(format!("Component '{key}' has an empty name"));
        }
        if component.symbols.is_empty() {
            return Err(format!(
                "Component '{}' has no symbols; code-bearing components require code-level coverage",
                component.name
            ));
        }

        for symbol in &component.symbols {
            let symbol_key = symbol.key.trim();
            if symbol_key.is_empty() {
                return Err(format!(
                    "Symbol '{}' needs a non-empty local key",
                    symbol.name
                ));
            }
            if !keys.insert(symbol_key) {
                return Err(format!("Duplicate component/symbol key '{symbol_key}'"));
            }
            if symbol.name.trim().is_empty() {
                return Err(format!(
                    "Component '{}' contains an unnamed symbol",
                    component.name
                ));
            }
            if !symbol_anchors.insert((symbol.source_file.as_str(), symbol.name.as_str())) {
                return Err(format!(
                    "Symbol '{}#{}' is assigned more than once in the container proposal",
                    symbol.source_file, symbol.name
                ));
            }
            if symbol.source_file.trim().is_empty() {
                return Err(format!("Symbol '{}' has no source file", symbol.name));
            }
            if matches!((symbol.line, symbol.end_line), (Some(start), Some(end)) if end < start) {
                return Err(format!(
                    "Symbol '{}' has an invalid line range",
                    symbol.name
                ));
            }
            // A symbol with neither responsibilities nor properties is allowed:
            // a thin definition (a UI leaf like `Logo`, a re-export, an entry
            // point) is real structure with no semantic content to invent.
            // Forcing content here only made the agent fabricate responsibilities
            // and rejected the whole proposal over one such symbol.
        }
    }

    for group in &req.groups {
        if group.member_keys.len() < 2 {
            return Err(format!("Group '{}' needs at least two members", group.name));
        }
        let mut members = HashSet::new();
        for key in &group.member_keys {
            if !keys.contains(key.trim()) {
                return Err(format!(
                    "Group '{}' references unknown component key '{}'",
                    group.name, key
                ));
            }
            if !members.insert(key.trim()) {
                return Err(format!(
                    "Group '{}' repeats component key '{}'",
                    group.name, key
                ));
            }
        }
    }
    Ok(())
}

#[tool_router(router = tool_router_generation, vis = "pub(crate)")]
impl ScryerServer {
    #[tool(
        description = "GENERATION-PIPELINE primitive — atomically fill in the complete component + symbol model for ONE existing container in a single write (the agent never assembles ids by hand). Used during codebase→model generation instead of separate add_component/add_symbol/add_group calls. Components and nested mandatory symbols use unique request-local `key` values; group memberKeys are local component keys. You do NOT author code-level links: the server wires component→component and symbol→symbol links automatically from the deterministic dependency graph. The optional `links` field is only for cross-boundary relationships (to an external/other-container node id) that the dependency graph can't infer; any link it can't place legally is dropped and reported, never fatal. The server validates the proposal, mints ids, resolves references, derives links, and performs one model write. Structural problems (missing symbols, duplicate keys) reject the whole proposal with a specific reason; fix exactly that and retry. Filling records structure and anchors only — it attaches no tests, so testable (When/While/If) claims land as `untested`. Immediately after a fill, attach the tests the codebase ALREADY has for those claims via update_source_map `test_entries` (`pattern` = test file, `symbol` = the test function); claims with no existing test stay honestly untested until one is written."
    )]
    fn fill_container(
        &self,
        Parameters(req): Parameters<CommitContainerModelRequest>,
    ) -> Result<CallToolResult, McpError> {
        if let Err(e) = validate_request(&req) {
            return Ok(err(e));
        }

        let model_ref = resolve_model_ref(req.project.as_deref())?;
        let _lock = match lock_or_err(&model_ref) {
            Ok(lock) => lock,
            Err(e) => return Ok(e),
        };
        let mut model = match scryer_core::read_model_at(&model_ref) {
            Ok(model) => model,
            Err(e) => return Ok(err(read_fail("model", &model_ref, &e))),
        };
        let Some(container) = model.nodes.iter().find(|n| n.id == req.container_id) else {
            // fill_container generates against COMMITTED (it writes both layers).
            // A container the agent just authored lives only in the plan, so name
            // the layer and the recovery instead of a bare "not found".
            let in_plan = scryer_core::read_planned_at(&model_ref)
                .map(|p| p.nodes.iter().any(|n| n.id == req.container_id))
                .unwrap_or(false);
            if in_plan {
                return Ok(err(format!(
                    "Container '{}' exists in the plan but not the committed model — fill_container generates against committed (it describes code that exists). Commit it first (mark_implemented) or seed the skeleton with set_node/set_model, which write both layers.",
                    req.container_id
                )));
            }
            return Ok(err(format!("Container '{}' not found", req.container_id)));
        };
        if container.kind != Kind::Container {
            return Ok(err(format!(
                "Node '{}' is not a container",
                req.container_id
            )));
        }
        if model.nodes.iter().any(|n| {
            n.parent_id.as_deref() == Some(req.container_id.as_str()) && n.kind == Kind::Component
        }) {
            return Ok(err(format!(
                "Container '{}' already has components; atomic generation only fills an empty subtree",
                req.container_id
            )));
        }

        let existing_ids: HashSet<String> = model.nodes.iter().map(|n| n.id.clone()).collect();
        if let Some(key) = req
            .components
            .iter()
            .flat_map(|component| {
                std::iter::once(component.key.trim())
                    .chain(component.symbols.iter().map(|symbol| symbol.key.trim()))
            })
            .find(|key| existing_ids.contains(*key))
        {
            return Ok(err(format!(
                "Local key '{key}' conflicts with an existing model node id"
            )));
        }
        // The planned draft is read under the same lock. A concurrent
        // system-level session writes its enrichment (persons, externals, system
        // responsibilities, group/link labels) to the planned layer ONLY, so we
        // must (a) mint ids that don't collide with it and (b) preserve it by
        // appending this subtree to the existing draft rather than overwriting.
        // Seeded read only — a plain committed fallback here would mint a shadow
        // draft (committed's anchors/boundaries copied into the plan, the very
        // single-home violation seeding exists to avoid). A failed read is
        // exceptional; fail the fill rather than corrupt the draft.
        let planned_before = match scryer_core::read_planned_seeded_at(&model_ref) {
            Ok(p) => p,
            Err(e) => return Ok(err(read_fail("plan", &model_ref, &e))),
        };

        let mut minter = IdMinter::new(&model);
        minter.absorb(&planned_before);
        let mut local_ids: HashMap<&str, String> = HashMap::new();
        let mut component_ids: HashMap<&str, String> = HashMap::new();
        let mut minted_components = Vec::new();
        let mut minted_symbols = 0usize;
        // (source_file, name) → symbol node id, for joining the deterministic
        // dependency graph back to freshly minted symbols. A location that
        // resolves to more than one symbol is ambiguous and skipped when wiring.
        let mut symbol_node_by_loc: HashMap<(String, String), String> = HashMap::new();
        let mut ambiguous_locs: HashSet<(String, String)> = HashSet::new();
        // symbol node id → its owning component node id.
        let mut symbol_component: HashMap<String, String> = HashMap::new();
        // Mark where this commit's additions begin so the same subtree can be
        // mirrored into the planned draft after all wiring is done.
        let node_start = model.nodes.len();
        let mut new_sm_keys: Vec<String> = Vec::new();

        for component in &req.components {
            let component_id = minter.node();
            component_ids.insert(component.key.trim(), component_id.clone());
            local_ids.insert(component.key.trim(), component_id.clone());
            minted_components.push(component_id.clone());

            let mut node = blank_node(
                component_id.clone(),
                Kind::Component,
                component.name.trim().to_string(),
                req.container_id.clone(),
            );
            node.description = component.description.clone();
            node.responsibilities = minter.responsibilities(&component.responsibilities);
            model.nodes.push(node);

            for symbol in &component.symbols {
                let symbol_id = minter.node();
                local_ids.insert(symbol.key.trim(), symbol_id.clone());
                let loc = (
                    symbol.source_file.trim().to_string(),
                    symbol.name.trim().to_string(),
                );
                if symbol_node_by_loc.insert(loc.clone(), symbol_id.clone()).is_some() {
                    ambiguous_locs.insert(loc);
                }
                symbol_component.insert(symbol_id.clone(), component_id.clone());
                let mut node = blank_node(
                    symbol_id.clone(),
                    Kind::Symbol,
                    symbol.name.trim().to_string(),
                    component_id.clone(),
                );
                node.properties = symbol
                    .properties
                    .iter()
                    .map(|property| SchemaProperty {
                        label: property.label.clone(),
                        description: property.description.clone(),
                        vagrant: None,
                        stale: None,
                        last_touched_at: None,
                    })
                    .collect();

                for input in &symbol.responsibilities {
                    if input.statement().trim().is_empty() {
                        continue;
                    }
                    let responsibility = minter.resp(input.statement(), input.concern());
                    new_sm_keys.push(responsibility.id.clone());
                    model.source_map.insert(
                        responsibility.id.clone(),
                        vec![SourceLocation {
                            pattern: symbol.source_file.clone(),
                            symbol: Some(symbol.name.clone()),
                            line: input.line(),
                            end_line: input.end_line(),
                        }],
                    );
                    node.responsibilities.push(responsibility);
                }
                if !node.properties.is_empty() {
                    new_sm_keys.push(symbol_id.clone());
                    model.source_map.insert(
                        symbol_id.clone(),
                        vec![SourceLocation {
                            pattern: symbol.source_file.clone(),
                            symbol: Some(symbol.name.clone()),
                            line: symbol.line,
                            end_line: symbol.end_line,
                        }],
                    );
                }
                model.nodes.push(node);
                minted_symbols += 1;
            }
        }

        let resolve = |endpoint: &str| -> Option<String> {
            local_ids.get(endpoint.trim()).cloned().or_else(|| {
                existing_ids
                    .contains(endpoint)
                    .then(|| endpoint.to_string())
            })
        };
        // A link is never fatal to the commit: the components and symbols are the
        // valuable, hard-won output, and links are derivable. Anything that can't
        // be wired legally is dropped and reported, not allowed to reject the
        // whole proposal (which forced the agent into expensive regeneration).
        let link_start = model.links.len();
        let mut seen_pairs: HashSet<(String, String)> =
            model.links.iter().map(|l| (l.src.clone(), l.dst.clone())).collect();
        let mut agent_pairs: HashSet<(String, String)> = HashSet::new();
        let mut dropped_links: Vec<String> = Vec::new();
        for proposed in &req.links {
            let Some(src) = resolve(&proposed.src) else {
                dropped_links.push(format!("unknown src '{}'", proposed.src));
                continue;
            };
            let Some(dst) = resolve(&proposed.dst) else {
                dropped_links.push(format!("unknown dst '{}'", proposed.dst));
                continue;
            };
            if src == dst {
                dropped_links.push(format!("self-link {src}"));
                continue;
            }
            if !seen_pairs.insert((src.clone(), dst.clone())) {
                dropped_links.push(format!("duplicate {src} -> {dst}"));
                continue;
            }
            agent_pairs.insert((src.clone(), dst.clone()));
            model.links.push(Link {
                id: scryer_core::make_link_id(&src, &dst),
                src,
                dst,
                label: proposed.label.clone(),
                method: proposed.method.clone(),
            });
        }

        let group_start = model.groups.len();
        for proposed in &req.groups {
            let member_ids: Vec<String> = proposed
                .member_keys
                .iter()
                .filter_map(|key| component_ids.get(key.trim()).cloned())
                .collect();
            model.groups.push(Group {
                id: minter.group(),
                name: proposed.name.clone(),
                description: proposed.description.clone(),
                member_ids,
                parent_group_id: None,
                parent_node_id: Some(req.container_id.clone()),
                responsibilities: minter.responsibilities(&proposed.responsibilities),
                icon: None,
            });
        }

        // Wire code-level links from the deterministic dependency graph the build
        // cached, joining each edge endpoint back to a minted symbol by source
        // location. An intra-component edge becomes a symbol→symbol link; a
        // cross-component edge is lifted to the component→component link that
        // authorizes it. Both are legal by construction (same-parent siblings),
        // so the agent never has to author — or mis-author — them.
        let mut derived_links = 0usize;
        if let Some(cache) = scryer_core::build_edges::read_build_edges(&model_ref.build_edges_path())
        {
            let lookup = |key: &str| -> Option<&String> {
                let (path, name) = scryer_core::build_edges::BuildEdges::split_symbol_key(key)?;
                let loc = (path.to_string(), name.to_string());
                if ambiguous_locs.contains(&loc) {
                    return None;
                }
                symbol_node_by_loc.get(&loc)
            };
            for edge in &cache.symbol_edges {
                let (Some(src_sym), Some(dst_sym)) = (lookup(&edge.src), lookup(&edge.dst)) else {
                    continue;
                };
                if src_sym == dst_sym {
                    continue;
                }
                let (Some(src_comp), Some(dst_comp)) =
                    (symbol_component.get(src_sym), symbol_component.get(dst_sym))
                else {
                    continue;
                };
                let (src, dst) = if src_comp == dst_comp {
                    (src_sym.clone(), dst_sym.clone())
                } else {
                    (src_comp.clone(), dst_comp.clone())
                };
                if src == dst || !seen_pairs.insert((src.clone(), dst.clone())) {
                    continue;
                }
                model.links.push(Link {
                    id: scryer_core::make_link_id(&src, &dst),
                    src,
                    dst,
                    label: String::new(),
                    method: None,
                });
                derived_links += 1;
            }
        }

        // Non-fatal legality sweep over AGENT-authored links only (derived links
        // are legal by construction). Illegal ones — e.g. a symbol reaching a
        // node its component isn't connected to — are removed and reported, never
        // allowed to fail the write.
        let illegal: Vec<(String, String, String)> = agent_pairs
            .iter()
            .filter_map(|(src, dst)| {
                scryer_core::validate::link_violation(&model, src, dst).map(|v| {
                    (
                        src.clone(),
                        dst.clone(),
                        scryer_core::validate::describe_violation(&model, src, dst, &v),
                    )
                })
            })
            .collect();
        for (src, dst, msg) in &illegal {
            model.links.retain(|l| !(l.src == *src && l.dst == *dst));
            dropped_links.push(msg.clone());
        }

        // Codebase→model generation lands in BOTH layers so an extracted
        // container carries no pending plan. The committed model takes the full
        // rebuilt value; the planned draft is APPENDED to (not overwritten) so a
        // concurrent system-level session's planned-only enrichment survives.
        // Ids were minted against the union of both layers, so the mirrored
        // subtree can't collide with anything already in the draft.
        let mut planned = planned_before;
        planned.nodes.extend(model.nodes[node_start..].iter().cloned());
        let planned_pairs: HashSet<(String, String)> =
            planned.links.iter().map(|l| (l.src.clone(), l.dst.clone())).collect();
        for link in &model.links[link_start..] {
            if !planned_pairs.contains(&(link.src.clone(), link.dst.clone())) {
                planned.links.push(link.clone());
            }
        }
        planned.groups.extend(model.groups[group_start..].iter().cloned());
        for key in &new_sm_keys {
            if let Some(locs) = model.source_map.get(key) {
                planned.source_map.insert(key.clone(), locs.clone());
            }
        }
        if let Err(e) = scryer_core::write_planned_at(&model_ref, &planned) {
            return Ok(err(e));
        }
        if let Err(e) = scryer_core::write_model_at(&model_ref, &model) {
            return Ok(err(e));
        }

        // Timeline: a `born` event per component that just entered the model.
        let now = scryer_core::drift::now_secs();
        for comp_id in &minted_components {
            let resp_count = model
                .nodes
                .iter()
                .find(|n| n.id == *comp_id)
                .map(|n| n.responsibilities.len())
                .unwrap_or(0);
            let sym_count = model
                .nodes
                .iter()
                .filter(|n| n.parent_id.as_deref() == Some(comp_id.as_str()) && n.kind == Kind::Symbol)
                .count();
            let text = format!(
                "{resp_count} responsibilit{} · {sym_count} symbol{} · component",
                if resp_count == 1 { "y" } else { "ies" },
                if sym_count == 1 { "" } else { "s" },
            );
            record_event(
                &model_ref,
                HistoryEvent::new(now, EventKind::Born, comp_id, "build")
                    .with_rows(vec![EventRow::new("+", text)]),
            );
        }

        let payload = serde_json::json!({
            "containerId": req.container_id,
            "componentIds": minted_components,
            "components": req.components.len(),
            "symbols": minted_symbols,
            "links": req.links.len().saturating_sub(dropped_links.len()),
            "derivedLinks": derived_links,
            "droppedLinks": dropped_links,
            "groups": req.groups.len(),
        });
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string(&payload).unwrap_or_else(|_| "{}".into()),
        )]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::handler::server::wrapper::Parameters;
    use scryer_core::{ModelRef, ScryModel};

    fn temp_project() -> (ScryerServer, tempfile::TempDir, String) {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());
        let mut model = ScryModel::new();
        let mut system = blank_node("node-1".into(), Kind::System, "Acme".into(), String::new());
        system.parent_id = None;
        model.nodes.push(system);
        model.nodes.push(blank_node(
            "node-2".into(),
            Kind::Container,
            "API".into(),
            "node-1".into(),
        ));
        scryer_core::write_model_at(&model_ref, &model).unwrap();
        (ScryerServer::new(), dir, "node-2".into())
    }

    fn request(project: String, container_id: String) -> CommitContainerModelRequest {
        CommitContainerModelRequest {
            project: Some(project),
            container_id,
            components: vec![
                ProposedComponent {
                    key: "auth".into(),
                    name: "Authentication".into(),
                    description: None,
                    responsibilities: vec!["controls account access".into()],
                    symbols: vec![ProposedSymbol {
                        key: "auth.login".into(),
                        name: "login".into(),
                        source_file: "src/auth.rs".into(),
                        line: Some(10),
                        end_line: Some(30),
                        responsibilities: vec![ResponsibilityInput::Plain(
                            "authenticates credentials".into(),
                        )],
                        properties: Vec::new(),
                    }],
                },
                ProposedComponent {
                    key: "sessions".into(),
                    name: "Sessions".into(),
                    description: None,
                    responsibilities: vec!["maintains active sessions".into()],
                    symbols: vec![ProposedSymbol {
                        key: "sessions.session".into(),
                        name: "Session".into(),
                        source_file: "src/session.rs".into(),
                        line: Some(1),
                        end_line: Some(8),
                        responsibilities: Vec::new(),
                        properties: vec![PropertyInput {
                            label: "user_id".into(),
                            description: "identifies the account".into(),
                        }],
                    }],
                },
            ],
            links: vec![
                ProposedLink {
                    src: "auth".into(),
                    dst: "sessions".into(),
                    label: "creates".into(),
                    method: None,
                },
                ProposedLink {
                    src: "auth.login".into(),
                    dst: "sessions".into(),
                    label: "creates".into(),
                    method: None,
                },
                ProposedLink {
                    src: "sessions.session".into(),
                    dst: "auth".into(),
                    label: "is created by".into(),
                    method: None,
                },
            ],
            groups: vec![ProposedGroup {
                name: "Identity".into(),
                description: None,
                member_keys: vec!["auth".into(), "sessions".into()],
                responsibilities: Vec::new(),
            }],
        }
    }

    #[test]
    fn commits_complete_container_in_one_write() {
        let (server, dir, container_id) = temp_project();
        let project = dir.path().to_string_lossy().to_string();
        server
            .fill_container(Parameters(request(project, container_id.clone())))
            .unwrap();

        let model =
            scryer_core::read_model_at(&ModelRef::ProjectLocal(dir.path().to_path_buf())).unwrap();
        let components: Vec<&Node> = model
            .nodes
            .iter()
            .filter(|n| n.parent_id.as_deref() == Some(container_id.as_str()))
            .collect();
        assert_eq!(components.len(), 2);
        assert_eq!(
            model
                .nodes
                .iter()
                .filter(|n| n.kind == Kind::Symbol)
                .count(),
            2
        );
        assert_eq!(model.links.len(), 3);
        assert_eq!(model.groups.len(), 1);
        assert_eq!(model.source_map.len(), 2);
        assert!(
            scryer_core::validate::validate(&model).is_empty(),
            "atomic proposal should produce a clean C4 subtree"
        );
        assert!(
            !ModelRef::ProjectLocal(dir.path().to_path_buf())
                .baseline_path()
                .exists(),
            "atomic generation defers the baseline snapshot"
        );
    }

    #[test]
    fn commit_preserves_concurrent_planned_only_enrichment() {
        // Reproduces the lost-system-nodes bug: a system-level session writes a
        // person (and an extra id) to the planned draft ONLY; a container commit
        // running beside it must not clobber that draft.
        let (server, dir, container_id) = temp_project();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());

        // System pass: append a person to the planned layer, minting an id beyond
        // the committed max (node-2) exactly as the live authoring tools do.
        let mut planned = scryer_core::read_planned_at(&model_ref).unwrap();
        let person = blank_node("node-3".into(), Kind::Person, "Developer".into(), String::new());
        planned.nodes.push(person);
        scryer_core::write_planned_at(&model_ref, &planned).unwrap();

        let project = dir.path().to_string_lossy().to_string();
        server
            .fill_container(Parameters(request(project, container_id.clone())))
            .unwrap();

        let planned = scryer_core::read_planned_at(&model_ref).unwrap();
        assert!(
            planned.nodes.iter().any(|n| n.id == "node-3" && n.kind == Kind::Person),
            "the concurrent planned-only person must survive the container commit"
        );
        assert_eq!(
            planned
                .nodes
                .iter()
                .filter(|n| n.parent_id.as_deref() == Some(container_id.as_str()))
                .count(),
            2,
            "the commit's components must also be appended to the planned draft"
        );
        // No two nodes share an id — the union minter skipped node-3.
        let ids: HashSet<&str> = planned.nodes.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(ids.len(), planned.nodes.len(), "ids must stay unique across layers");

        // The committed layer carries the container subtree but not the
        // planned-only person (that lands at the build-end fold).
        let model = scryer_core::read_model_at(&model_ref).unwrap();
        assert!(!model.nodes.iter().any(|n| n.id == "node-3"));
    }

    #[test]
    fn rejects_missing_symbol_coverage_without_writing() {
        let (server, dir, container_id) = temp_project();
        let project = dir.path().to_string_lossy().to_string();
        let mut req = request(project, container_id);
        req.components[0].symbols.clear();
        server.fill_container(Parameters(req)).unwrap();

        let model =
            scryer_core::read_model_at(&ModelRef::ProjectLocal(dir.path().to_path_buf())).unwrap();
        assert_eq!(model.nodes.len(), 2);
    }

    #[test]
    fn derives_code_links_from_cached_dependency_graph() {
        let (server, dir, container_id) = temp_project();
        let project = dir.path().to_string_lossy().to_string();

        // login (in component 'auth') calls Session (in component 'sessions').
        // The join is on (file, name); the @line only disambiguates, so a
        // different reported line still matches.
        let edges = scryer_core::build_edges::BuildEdges {
            symbol_edges: vec![scryer_core::build_edges::CachedEdge {
                src: "src/auth.rs#login@10".into(),
                dst: "src/session.rs#Session@99".into(),
            }],
        };
        scryer_core::build_edges::write_build_edges(dir.path(), &edges).unwrap();

        let mut req = request(project, container_id);
        req.links.clear(); // the agent authors nothing; the server must wire it

        server.fill_container(Parameters(req)).unwrap();

        let model =
            scryer_core::read_model_at(&ModelRef::ProjectLocal(dir.path().to_path_buf())).unwrap();
        // The cross-component symbol edge lifts to a component→component link.
        assert_eq!(model.links.len(), 1, "one derived link expected");
        let kind = |id: &str| {
            model
                .nodes
                .iter()
                .find(|n| n.id == id)
                .map(|n| n.kind)
                .unwrap()
        };
        assert_eq!(kind(&model.links[0].src), Kind::Component);
        assert_eq!(kind(&model.links[0].dst), Kind::Component);
        assert!(
            scryer_core::validate::link_violation(&model, &model.links[0].src, &model.links[0].dst)
                .is_none(),
            "derived links must be legal by construction"
        );
    }

    #[test]
    fn drops_illegal_agent_link_but_still_commits() {
        let (server, dir, container_id) = temp_project();
        let project = dir.path().to_string_lossy().to_string();
        let mut req = request(project, container_id);
        // A symbol linking to a sibling symbol under a DIFFERENT component, with
        // no authorizing component link — structurally illegal.
        req.links = vec![ProposedLink {
            src: "auth.login".into(),
            dst: "sessions.session".into(),
            label: "calls".into(),
            method: None,
        }];
        server.fill_container(Parameters(req)).unwrap();

        let model =
            scryer_core::read_model_at(&ModelRef::ProjectLocal(dir.path().to_path_buf())).unwrap();
        // Components + symbols survive; only the illegal link is dropped.
        assert_eq!(
            model.nodes.iter().filter(|n| n.kind == Kind::Symbol).count(),
            2
        );
        assert_eq!(
            model
                .nodes
                .iter()
                .filter(|n| n.kind == Kind::Component)
                .count(),
            2
        );
        assert!(
            model.links.is_empty(),
            "illegal cross-component symbol link must be dropped, not committed"
        );
    }

    #[test]
    fn rejects_duplicate_local_keys_without_writing() {
        let (server, dir, container_id) = temp_project();
        let project = dir.path().to_string_lossy().to_string();
        let mut req = request(project, container_id);
        req.components[0].symbols[0].key = "sessions".into();
        server.fill_container(Parameters(req)).unwrap();

        let model =
            scryer_core::read_model_at(&ModelRef::ProjectLocal(dir.path().to_path_buf())).unwrap();
        assert_eq!(model.nodes.len(), 2);
        assert!(model.links.is_empty());
    }
}
