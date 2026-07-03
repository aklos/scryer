//! Intent write tools — the agent's preferred path for building a model.
//!
//! Each tool takes INTENT (a name, plain responsibility statements, the source
//! location the agent already holds from the codebase context) and builds the
//! node itself: it mints the node id and the `resp-` ids, fixes the kind from
//! the parent level (validating the parent is the right kind), and — for
//! symbols — writes the source map anchored to the file + symbol name. The
//! agent never assembles the JSON shape or hand-mints ids.
//!
//! These tools author into the PLANNED draft (`.scryer/planned.scry`); the
//! committed model changes only when the work is implemented and folds in
//! (`mark_implemented`). The bulk `set_model` / `set_node` tools are
//! generation-pipeline primitives (whole-model and whole-subtree writes used
//! during codebase→model generation); interactive editing uses the tools above.

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
    Group, Kind, ModelRef, Node, Responsibility, SchemaProperty, ScryModel, Source, SourceLocation,
};
use std::collections::HashMap;

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

    /// Raise the counter past every responsibility id in `other` (the committed
    /// model) too, so minting against the plan can't re-issue an id the plan
    /// deleted but committed still holds (audit #3).
    fn absorb(&mut self, other: &ScryModel) {
        let max = other
            .nodes
            .iter()
            .flat_map(|n| n.responsibilities.iter())
            .chain(other.groups.iter().flat_map(|g| g.responsibilities.iter()))
            .filter_map(|r| r.id.strip_prefix("resp-").and_then(|s| s.parse::<u64>().ok()))
            .max()
            .unwrap_or(0);
        self.next = self.next.max(max + 1);
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
                    vagrant: None,
                    stale: None,
                    stale_proposal: None,
                    directives: Vec::new(),
                    last_touched_at: None,
                }
            })
            .collect()
    }

    /// Build responsibilities from rich inputs, returning per-responsibility
    /// line ranges alongside.
    fn build_rich(
        &mut self,
        inputs: &[ResponsibilityInput],
    ) -> Vec<(Responsibility, Option<u32>, Option<u32>)> {
        inputs
            .iter()
            .filter(|i| !i.statement().trim().is_empty())
            .map(|i| {
                let id = format!("resp-{}", self.next);
                self.next += 1;
                let resp = Responsibility {
                    id,
                    statement: i.statement().trim().to_string(),
                    vagrant: None,
                    stale: None,
                    stale_proposal: None,
                    directives: Vec::new(),
                    last_touched_at: None,
                };
                (resp, i.line(), i.end_line())
            })
            .collect()
    }
}

fn err(msg: impl Into<String>) -> CallToolResult {
    CallToolResult::error(vec![Content::text(msg.into())])
}

/// Read the planned (draft) model — the authoring base. Falls back to the
/// committed model when no plan has diverged yet. Agent authoring proposes into
/// the plan; the committed model only changes when the work is implemented and
/// folded (planned → model).
fn read_planned(model_ref: &ModelRef) -> Result<ScryModel, CallToolResult> {
    scryer_core::read_planned_at(model_ref)
        .map_err(|e| err(format!("Failed to read plan at {}: {}", model_ref, e)))
}

/// The committed model, read only to raise id-minting floors. Plan-first tools
/// author into the PLANNED draft, but a node/responsibility/group the plan
/// DELETED still lives in committed under its id. Minting from the plan alone
/// can re-issue that id: the pending deletion then reads as a reword and
/// `mark_implemented` overwrites the committed node (audit #3). `fill_container`
/// guards the same way via `IdMinter::absorb`. A missing or unreadable committed
/// model (design-first, before any fold) is simply empty.
fn read_committed(model_ref: &ModelRef) -> ScryModel {
    scryer_core::read_model_at(model_ref).unwrap_or_default()
}

/// Highest numeric suffix among ids carrying `prefix` (e.g. `node-`); 0 if none.
fn max_id_suffix<'a>(ids: impl Iterator<Item = &'a str>, prefix: &str) -> u64 {
    ids.filter_map(|id| id.strip_prefix(prefix).and_then(|s| s.parse::<u64>().ok()))
        .max()
        .unwrap_or(0)
}

/// Next `node-N` id past BOTH the planned draft and the committed model, so a
/// node the plan deleted (still live in committed) can't have its id re-issued.
fn next_node_id_union(planned: &ScryModel, committed: &ScryModel) -> String {
    let max = max_id_suffix(
        planned.nodes.iter().chain(committed.nodes.iter()).map(|n| n.id.as_str()),
        "node-",
    );
    format!("node-{}", max + 1)
}

/// Next `group-N` id past both layers — same union guard as {@link next_node_id_union}.
fn next_group_id_union(planned: &ScryModel, committed: &ScryModel) -> String {
    let max = max_id_suffix(
        planned.groups.iter().chain(committed.groups.iter()).map(|g| g.id.as_str()),
        "group-",
    );
    format!("group-{}", max + 1)
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
        vagrant: None,
        stale: None,
        parent_id,
        external: None,
        technology: None,
        description: None,
        responsibilities: Vec::new(),
        properties: Vec::new(),
        icon: None,
        visual: None,
        appearance: None,
        notes: None,
        directives: Vec::new(),
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

    if let Err(e) = scryer_core::write_planned_at(model_ref, &model) {
        return Ok(err(e));
    }

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
        let mut model = match read_planned(&model_ref) {
            Ok(m) => m,
            Err(e) => return Ok(e),
        };
        let prior = model.clone();
        let committed = read_committed(&model_ref);
        let mut minter = RespMinter::new(&model);
        minter.absorb(&committed);
        let mut minted = Vec::new();
        for item in &req.items {
            let id = next_node_id_union(&model, &committed);
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
        let mut model = match read_planned(&model_ref) {
            Ok(m) => m,
            Err(e) => return Ok(e),
        };
        let prior = model.clone();
        let committed = read_committed(&model_ref);
        let mut minter = RespMinter::new(&model);
        minter.absorb(&committed);
        let mut minted = Vec::new();
        for item in &req.items {
            let id = next_node_id_union(&model, &committed);
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
        description = "Add one or more containers under a system. `name` is the role; `technology` is what it IS as software. Pass `boundaryDir` (the container's directory from the codebase context) to set its boundary glob automatically. Responsibilities go at the container's own altitude — what it is accountable for, not what its components do. Plain responsibility statements; ids and status set for you. On altitude and runtime boundaries: get_rules{topic:'container altitude'}."
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
        let mut model = match read_planned(&model_ref) {
            Ok(m) => m,
            Err(e) => return Ok(e),
        };
        let prior = model.clone();
        let committed = read_committed(&model_ref);
        let mut minter = RespMinter::new(&model);
        minter.absorb(&committed);
        let mut minted = Vec::new();
        for item in &req.items {
            if let Some(e) = check_parent(&model, &item.parent_id, Kind::System) {
                return Ok(e);
            }
            let id = next_node_id_union(&model, &committed);
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
        description = "Add one or more components under a container. Give responsibilities at the component's own altitude — one accountability each, not what an individual symbol does. Plain responsibility statements; ids and status set for you. How to cluster components (cohesion + dependency graph, not one-per-file) and pitch altitude: get_rules{topic:'component'}."
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
        let mut model = match read_planned(&model_ref) {
            Ok(m) => m,
            Err(e) => return Ok(e),
        };
        let prior = model.clone();
        let committed = read_committed(&model_ref);
        let mut minter = RespMinter::new(&model);
        minter.absorb(&committed);
        let mut minted = Vec::new();
        for item in &req.items {
            if let Some(e) = check_parent(&model, &item.parent_id, Kind::Container) {
                return Ok(e);
            }
            let id = next_node_id_union(&model, &committed);
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
        description = "Group sibling nodes that ship or package together — a SECONDARY axis, never a substitute for decomposition. `parent_id` = the node whose children you're grouping (the system for a group of containers; a container for a group of components). `member_ids` = the sibling node ids to enclose (2+, all children of parent_id, same level). Optional responsibility statements describe the unit (e.g. 'deploys atomically'). The group id + layout are set for you. When a group is right vs. a missing parent node, and logical vs. architectural groups: get_rules{topic:'group'}."
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
        let mut model = match read_planned(&model_ref) {
            Ok(m) => m,
            Err(e) => return Ok(e),
        };
        let prior = model.clone();
        let committed = read_committed(&model_ref);
        let mut minter = RespMinter::new(&model);
        minter.absorb(&committed);
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
            let id = next_group_id_union(&model, &committed);
            model.groups.push(Group {
                id: id.clone(),
                name: item.name.clone(),
                description: item.description.clone(),
                member_ids: item.member_ids.clone(),
                parent_group_id: None,
                parent_node_id: Some(item.parent_id.clone()),
                responsibilities: minter.build(&item.responsibilities),
                icon: None,
            });
            minted.push(id);
        }
        // Groups aren't nodes, so commit by hand (the node-returning `commit`
        // helper doesn't apply): enforce read-only invariants, write, baseline.
        enforce_readonly_directives(&mut model, &prior);
        if let Err(e) = scryer_core::write_planned_at(&model_ref, &model) {
            return Ok(err(e));
        }
        Ok(CallToolResult::success(vec![Content::text(format!(
            "Created {} group(s): {}",
            minted.len(),
            minted.join(", ")
        ))]))
    }

    #[tool(
        description = "Add one or more symbols (one addressable code definition each) under a component. Pass the `sourceFile` (and line/endLine for the full definition) from the codebase context; the source map is anchored to the file + symbol name for you — no separate update_source_map call. Each responsibility is a plain string or `{statement, line, endLine}` for the sub-range that does the work; give `properties` for a declared data shape. Ids and status set for you. Not every definition earns a symbol, and a data shape goes in `properties`, never in prose — read get_rules{topic:'symbol'} before adding."
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
        let mut model = match read_planned(&model_ref) {
            Ok(m) => m,
            Err(e) => return Ok(e),
        };
        let prior = model.clone();
        let committed = read_committed(&model_ref);
        let mut minter = RespMinter::new(&model);
        minter.absorb(&committed);
        let mut minted = Vec::new();
        for item in &req.items {
            if let Some(e) = check_parent(&model, &item.parent_id, Kind::Component) {
                return Ok(e);
            }
            let id = next_node_id_union(&model, &committed);
            let mut node = blank_node(
                id.clone(),
                Kind::Symbol,
                item.name.clone(),
                Some(item.parent_id.clone()),
            );
            let rich = minter.build_rich(&item.responsibilities);
            node.properties = item
                .properties
                .iter()
                .map(|p| SchemaProperty {
                    label: p.label.clone(),
                    description: p.description.clone(),
                    vagrant: None,
                    stale: None,
                    last_touched_at: None,
                })
                .collect();

            // Anchor each responsibility to the file + symbol name (durable over
            // line shifts) with the specific sub-range when provided. For a data
            // shape, the declaration block is anchored to the symbol node id.
            let resps: Vec<Responsibility> = rich
                .into_iter()
                .map(|(r, line, end_line)| {
                    model.source_map.insert(
                        r.id.clone(),
                        vec![SourceLocation {
                            pattern: item.source_file.clone(),
                            symbol: Some(item.name.clone()),
                            line,
                            end_line,
                            command: None,
                        }],
                    );
                    r
                })
                .collect();
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
            node.visual = item.visual;
            model.nodes.push(node);
            minted.push(id);
        }
        commit(&model_ref, model, &prior, &minted)
    }

    #[tool(
        description = "Record SEMANTIC drift for a node after comparing its code against its responsibilities — the model↔code reconcile, where each finding gets a DIRECTION. `undescribed` is the *take-code* direction: behaviours the code has that NO responsibility describes — each is proposed into the PLAN as a vagrant adoption (a code-discovered `added` claim), which the user adopts (commit — the code already exists) or rejects (mark the code for deletion); do NOT report code that changed but still satisfies an existing responsibility. Each undescribed finding is HOMED on a node: it routes automatically to the node that already owns its `symbol`/file (or set `nodeId` to force an existing node). When the model has NO node for the code, MINT the missing rungs in `newNodes` (a `key`, `kind`, `name`, and a parent via `parentId` on an existing node or `parentKey` on a shallower mint — list ancestors first) and point the finding at the leaf with `nodeKey`, so it lands at its true altitude instead of bubbling up to the reviewed container. `stale` is the *take-model* direction: existing responsibilities the model still asserts but whose code regressed — flagged `stale` so the user can give a verdict: re-implement (the model is right, the code is rebuilt) or drop (the behaviour was removed on purpose, so the claim leaves the model). When the behaviour did NOT vanish but DIVERGED — the code still does a related thing, just differently than the claim says — also set `proposedStatement` to the corrected wording that matches what the code now does; the user can then accept it (folding the new wording into the model with no rebuild, since the code already does it) instead of choosing re-implement/drop. Omit `proposedStatement` when the behaviour is truly gone. `staleNodes` is the node-level version of the same direction: when a deleted file or folder wipes out a whole modeled node — a symbol, a component, an entire container subtree — flag the NODE (by `nodeId`) instead of listing each of its claims; the verdict then applies to the whole subtree. Use `staleNodes` when the node's backing code is gone entirely, `stale` when only one of a still-present node's claims regressed. Properties have the SAME two directions as a data type's fields drift: a newly-declared struct field / interface member that no property describes is DATA, not behaviour — report it under `undescribedProperties` (its `label`, `sourceFile`, enclosing `symbol`, homed like `undescribed`) so it lands as a vagrant property, NEVER as a responsibility; and a property whose backing field was removed or materially changed goes under `staleProperties` (`nodeId` + `label`, since properties have no id). Call with empty arrays (or don't call) when the code and the model still agree."
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

        // TAKE CODE — undescribed behaviour → adoptions proposed into the PLAN.
        // Each becomes a vagrant `Added` responsibility in the draft, anchored to
        // its source; the user adopts it (commit — code already exists) or rejects
        // it (drop from the plan). The `vagrant` marker distinguishes "code already
        // has this, adopt?" from "intent ahead of code, implement!".
        let mut planned = match scryer_core::read_planned_at(&model_ref) {
            Ok(p) => p,
            Err(e) => return Ok(err(format!("Failed to read plan: {e}"))),
        };
        if !planned.nodes.iter().any(|n| n.id == req.node_id) {
            return Ok(err(format!("Node '{}' not found", req.node_id)));
        }
        let prior_plan = planned.clone();
        let committed = read_committed(&model_ref);

        // 1. MINT vagrant nodes for code the model has no node for — the agent's
        //    declared missing rungs (a new component, a symbol for a new
        //    definition, or a whole chain). Resolve each parent (an existing node
        //    id, or an earlier-minted node's key), assign an id, and push it
        //    vagrant. `key_to_id` lets later items reference a mint before its
        //    real id exists.
        let mut key_to_id: HashMap<String, String> = HashMap::new();
        let mut minted_node_ids: Vec<String> = Vec::new();
        for nn in &req.new_nodes {
            let kind = match parse_kind(&nn.kind) {
                Ok(k) => k,
                Err(_) => {
                    return Ok(err(format!(
                        "newNode '{}' has invalid kind '{}'",
                        nn.key, nn.kind
                    )))
                }
            };
            if key_to_id.contains_key(&nn.key) {
                return Ok(err(format!("duplicate newNode key '{}'", nn.key)));
            }
            let parent_id = match (&nn.parent_id, &nn.parent_key) {
                (Some(_), Some(_)) => {
                    return Ok(err(format!(
                        "newNode '{}' sets both parentId and parentKey — set exactly one",
                        nn.key
                    )))
                }
                (None, None) => {
                    return Ok(err(format!(
                        "newNode '{}' needs a parentId or parentKey",
                        nn.key
                    )))
                }
                (Some(pid), None) => {
                    if !planned.nodes.iter().any(|n| &n.id == pid) {
                        return Ok(err(format!(
                            "newNode '{}' parentId '{}' not found",
                            nn.key, pid
                        )));
                    }
                    pid.clone()
                }
                (None, Some(pk)) => match key_to_id.get(pk) {
                    Some(id) => id.clone(),
                    None => {
                        return Ok(err(format!(
                            "newNode '{}' parentKey '{}' is not a node minted earlier in this call \
                             (list ancestors before descendants)",
                            nn.key, pk
                        )))
                    }
                },
            };
            let id = next_node_id_union(&planned, &committed);
            let mut node = blank_node(id.clone(), kind, nn.name.clone(), Some(parent_id));
            node.vagrant = Some(true);
            node.description = nn.description.clone();
            node.technology = nn.technology.clone();
            planned.nodes.push(node);
            key_to_id.insert(nn.key.clone(), id.clone());
            minted_node_ids.push(id);
        }

        // 2. A vagrant responsibility per undescribed behaviour, anchored to its
        //    source.
        let items: Vec<&UndescribedItem> = req
            .undescribed
            .iter()
            .filter(|u| !u.statement.trim().is_empty())
            .collect();
        let mut minter = RespMinter::new(&planned);
        minter.absorb(&committed);
        let statements: Vec<String> = items.iter().map(|u| u.statement.clone()).collect();
        let mut resps = minter.build(&statements);
        for r in resps.iter_mut() {
            r.vagrant = Some(true);
        }
        for (item, r) in items.iter().zip(resps.iter()) {
            planned.source_map.insert(
                r.id.clone(),
                vec![SourceLocation {
                    pattern: item.source_file.clone(),
                    symbol: item.symbol.clone(),
                    line: item.line,
                    end_line: item.end_line,
                    command: None,
                }],
            );
        }
        let flagged = resps.len();
        // 3. Route each finding to its host node. An explicit `nodeKey` (a mint)
        //    or `nodeId` (an existing node) wins; otherwise fall back to the
        //    FINEST node the source map already ties the file to, so the finding
        //    lands at its true altitude instead of bubbling up to the reviewed
        //    container.
        let mut targets: Vec<String> = Vec::with_capacity(items.len());
        for item in &items {
            let target = if let Some(k) = &item.node_key {
                match key_to_id.get(k) {
                    Some(id) => id.clone(),
                    None => {
                        return Ok(err(format!(
                            "undescribed item references nodeKey '{}' with no matching newNode",
                            k
                        )))
                    }
                }
            } else if let Some(nid) = &item.node_id {
                if !planned.nodes.iter().any(|n| &n.id == nid) {
                    return Ok(err(format!(
                        "undescribed item references nodeId '{}' not in the model",
                        nid
                    )));
                }
                nid.clone()
            } else {
                scryer_core::ownership::owning_node_for_location(
                    &planned,
                    &req.node_id,
                    &item.source_file,
                    item.symbol.as_deref(),
                )
            };
            targets.push(target);
        }
        // Build the timeline rows now (while the adoptions and their source map are
        // still in hand), grouped by the node each lands on.
        let mut rows_by_node: HashMap<String, Vec<EventRow>> = HashMap::new();
        for (target, r) in targets.iter().zip(resps.iter()) {
            rows_by_node
                .entry(target.clone())
                .or_default()
                .push(resp_event_row("+", &planned, r));
        }
        for (target, r) in targets.into_iter().zip(resps.into_iter()) {
            if let Some(node) = planned.nodes.iter_mut().find(|n| n.id == target) {
                node.responsibilities.push(r);
            }
        }

        // TAKE CODE (properties) — undescribed data fields → vagrant properties on
        // the data-shape node that owns them. Routed exactly like responsibilities
        // (explicit nodeKey/nodeId, else the finest node the source map ties the
        // file/symbol to), but keyed by (node, label) since properties carry no id.
        let mut flagged_props = 0usize;
        let mut prop_rows_by_node: HashMap<String, Vec<EventRow>> = HashMap::new();
        for p in &req.undescribed_properties {
            if p.label.trim().is_empty() {
                continue;
            }
            let target = if let Some(k) = &p.node_key {
                match key_to_id.get(k) {
                    Some(id) => id.clone(),
                    None => {
                        return Ok(err(format!(
                            "undescribed property references nodeKey '{}' with no matching newNode",
                            k
                        )))
                    }
                }
            } else if let Some(nid) = &p.node_id {
                if !planned.nodes.iter().any(|n| &n.id == nid) {
                    return Ok(err(format!(
                        "undescribed property references nodeId '{}' not in the model",
                        nid
                    )));
                }
                nid.clone()
            } else {
                scryer_core::ownership::owning_node_for_location(
                    &planned,
                    &req.node_id,
                    &p.source_file,
                    p.symbol.as_deref(),
                )
            };
            if let Some(node) = planned.nodes.iter_mut().find(|n| n.id == target) {
                // A property the node already declares is not undescribed — skip it
                // rather than mint a duplicate label.
                if node.properties.iter().any(|x| x.label == p.label) {
                    continue;
                }
                node.properties.push(SchemaProperty {
                    label: p.label.clone(),
                    description: p.description.clone(),
                    vagrant: Some(true),
                    stale: None,
                    last_touched_at: None,
                });
                prop_rows_by_node
                    .entry(target.clone())
                    .or_default()
                    .push(EventRow::new("+", p.label.clone()));
                flagged_props += 1;
            }
        }

        // TAKE MODEL — stale claims → the `stale` observation flag written to the
        // PLANNED draft (the working layer the UI reads), exactly where `vagrant`
        // lives. The committed model is untouched; the flag rides the working
        // claim until the user gives a verdict (re-implement / drop). `diff`
        // compares only statement/directives, so a stale claim awaiting a verdict
        // is NOT itself a plan work item. Applied here so it rides the SAME write
        // as the take-code findings below.
        let mut staled = 0usize;
        for s in &req.stale {
            let r = planned
                .nodes
                .iter_mut()
                .flat_map(|n| n.responsibilities.iter_mut())
                .chain(planned.groups.iter_mut().flat_map(|g| g.responsibilities.iter_mut()))
                .find(|r| r.id == s.responsibility_id);
            match r {
                Some(r) => {
                    r.stale = Some(true);
                    r.stale_proposal = s.proposed_statement.clone().filter(|t| !t.trim().is_empty());
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

        // TAKE MODEL (properties) — an existing property whose backing field is
        // gone or materially changed. Flagged stale on the planned node, addressed
        // by (node, label) since properties have no id. Mirror of stale claims.
        let mut staled_props = 0usize;
        for sp in &req.stale_properties {
            match planned
                .nodes
                .iter_mut()
                .find(|n| n.id == sp.node_id)
                .and_then(|n| n.properties.iter_mut().find(|p| p.label == sp.label))
            {
                Some(p) => {
                    p.stale = Some(true);
                    staled_props += 1;
                }
                None => {
                    return Ok(err(format!(
                        "Property '{}' on node '{}' not found",
                        sp.label, sp.node_id
                    )));
                }
            }
        }

        // Node-level stale — a whole subtree's backing code is gone. Flag the
        // named node itself; the verdict (re-implement / drop) cascades to its
        // descendants, so only the top of the gone subtree needs flagging.
        let mut staled_nodes = 0usize;
        for sn in &req.stale_nodes {
            match planned.nodes.iter_mut().find(|n| n.id == sn.node_id) {
                Some(n) => {
                    n.stale = Some(true);
                    staled_nodes += 1;
                }
                None => {
                    return Ok(err(format!("Node '{}' not found", sn.node_id)));
                }
            }
        }

        if flagged > 0
            || flagged_props > 0
            || !minted_node_ids.is_empty()
            || staled > 0
            || staled_props > 0
            || staled_nodes > 0
        {
            enforce_readonly_directives(&mut planned, &prior_plan);
            if let Err(e) = scryer_core::write_planned_at(&model_ref, &planned) {
                return Ok(err(e));
            }
        }

        // Timeline: each stale finding reads as a "took model" drift event on its
        // host node (the model is right, the code regressed).
        if staled > 0 {
            let now = scryer_core::drift::now_secs();
            let mut stale_by_node: HashMap<String, Vec<EventRow>> = HashMap::new();
            for s in &req.stale {
                if let Some(node) = planned
                    .nodes
                    .iter()
                    .find(|n| n.responsibilities.iter().any(|r| r.id == s.responsibility_id))
                {
                    if let Some(r) =
                        node.responsibilities.iter().find(|r| r.id == s.responsibility_id)
                    {
                        stale_by_node
                            .entry(node.id.clone())
                            .or_default()
                            .push(resp_event_row("!", &planned, r));
                    }
                }
            }
            for (node_id, rows) in stale_by_node {
                record_event(
                    &model_ref,
                    HistoryEvent::new(now, EventKind::Drift, &node_id, "took model").with_rows(rows),
                );
            }
        }

        // Timeline: each stale NODE reads as a "took model" drift event on itself
        // (its whole backing code is gone).
        if staled_nodes > 0 {
            let now = scryer_core::drift::now_secs();
            for sn in &req.stale_nodes {
                if let Some(node) = planned.nodes.iter().find(|n| n.id == sn.node_id) {
                    record_event(
                        &model_ref,
                        HistoryEvent::new(now, EventKind::Drift, &node.id, "took model")
                            .with_rows(vec![EventRow::new("!", node.name.clone())]),
                    );
                }
            }
        }

        // Timeline: undescribed behaviours adopted from code read as a "took code"
        // drift event on EACH node they were routed onto (not just the reviewed
        // container).
        if !rows_by_node.is_empty() {
            let now_code = scryer_core::drift::now_secs();
            for (node_id, rows) in rows_by_node {
                record_event(
                    &model_ref,
                    HistoryEvent::new(now_code, EventKind::Drift, &node_id, "took code")
                        .with_rows(rows),
                );
            }
        }

        // Timeline: undescribed data fields adopted from code read as a "took code"
        // drift event on each data-shape node they landed on.
        if !prop_rows_by_node.is_empty() {
            let now_code = scryer_core::drift::now_secs();
            for (node_id, rows) in prop_rows_by_node {
                record_event(
                    &model_ref,
                    HistoryEvent::new(now_code, EventKind::Drift, &node_id, "took code")
                        .with_rows(rows),
                );
            }
        }

        // Timeline: each stale property reads as a "took model" drift event on its
        // owning node (the model is right, the field regressed).
        if staled_props > 0 {
            let now = scryer_core::drift::now_secs();
            let mut by_node: HashMap<String, Vec<EventRow>> = HashMap::new();
            for sp in &req.stale_properties {
                by_node
                    .entry(sp.node_id.clone())
                    .or_default()
                    .push(EventRow::new("!", sp.label.clone()));
            }
            for (node_id, rows) in by_node {
                record_event(
                    &model_ref,
                    HistoryEvent::new(now, EventKind::Drift, &node_id, "took model").with_rows(rows),
                );
            }
        }

        let mut msg = format!(
            "Proposed {flagged} undescribed behaviour(s) into the plan as adoptions (routed to the nodes that own their code{}) and flagged {staled} stale responsibility(ies) under '{}'.",
            if minted_node_ids.is_empty() {
                String::new()
            } else {
                format!(", minting {} vagrant node(s): {}", minted_node_ids.len(), minted_node_ids.join(", "))
            },
            req.node_id
        );
        for s in &req.stale {
            msg.push_str(&format!("\n  stale {}: {}", s.responsibility_id, s.reason));
        }
        if flagged_props > 0 || staled_props > 0 {
            msg.push_str(&format!(
                "\nProposed {flagged_props} undescribed data field(s) as vagrant properties and flagged {staled_props} stale property(ies)."
            ));
            for sp in &req.stale_properties {
                msg.push_str(&format!("\n  stale property {}.{}: {}", sp.node_id, sp.label, sp.reason));
            }
        }
        if staled_nodes > 0 {
            msg.push_str(&format!("\nFlagged {staled_nodes} stale node subtree(s):"));
            for sn in &req.stale_nodes {
                msg.push_str(&format!("\n  stale node {}: {}", sn.node_id, sn.reason));
            }
        }
        Ok(CallToolResult::success(vec![Content::text(msg)]))
    }

    #[tool(
        description = "Advance the drift reconcile anchor to NOW after you have examined every scope `get_drift` reported (and recorded any findings with `flag_drift`). This stamps the model as reconciled against the current code state — file changes up to this point stop surfacing in `get_drift`, so only NEWER changes count next time. Call it to close a drift pass, or when `get_drift` is already clean and you simply want to re-baseline. Records the current git commit when the project is a repo. Caution: this asserts you have reviewed everything that changed — anything you skipped will not resurface."
    )]
    fn reconcile_drift(
        &self,
        Parameters(req): Parameters<ReconcileDriftRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = resolve_model_ref(req.project.as_deref())?;
        let project = model_ref.project_path();
        let state = scryer_core::drift::SyncState {
            reconciled_at: scryer_core::drift::now_secs(),
            commit: scryer_core::drift::head_commit(project), ..Default::default() };
        if let Err(e) = scryer_core::write_sync_state(&model_ref, &state) {
            return Ok(err(format!("Failed to write reconcile anchor: {e}")));
        }
        // "Reconciled" means the anchors as they stand are the truth — refresh
        // the content fingerprints so the next check compares against them.
        if let Err(e) = scryer_extract::anchors::write_baseline(&model_ref) {
            return Ok(err(format!("Failed to fingerprint anchors: {e}")));
        }
        let commit_note = match &state.commit {
            Some(c) => format!(" at commit {}", &c[..c.len().min(12)]),
            None => String::new(),
        };
        Ok(CallToolResult::success(vec![Content::text(format!(
            "Reconciled — drift anchor advanced to now{commit_note}. Only code changes after this point will surface in get_drift."
        ))]))
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

    /// The planned (draft) layer — where the authoring tools now write.
    fn read_plan(dir: &tempfile::TempDir) -> ScryModel {
        scryer_core::read_planned_at(&ModelRef::ProjectLocal(dir.path().to_path_buf())).unwrap()
    }

    /// Commit the whole current plan into the committed model — the test stand-in
    /// for "this authored intent got implemented" (planned → model fold).
    fn commit_plan(dir: &tempfile::TempDir) {
        let r = ModelRef::ProjectLocal(dir.path().to_path_buf());
        let plan = scryer_core::read_planned_at(&r).unwrap();
        scryer_core::write_model_at(&r, &plan).unwrap();
    }

    fn resp(id: &str, statement: &str) -> Responsibility {
        Responsibility {
            id: id.into(),
            statement: statement.into(),
            vagrant: None,
            stale: None,
            stale_proposal: None,
            directives: Vec::new(),
            last_touched_at: None,
        }
    }

    /// Regression for audit #3: a plan-deleted node/responsibility still lives in
    /// committed under its id, so minting must clear BOTH layers — otherwise a new
    /// node re-uses the deleted id, the pending deletion silently becomes a reword,
    /// and the fold overwrites the committed node.
    #[test]
    fn intent_mints_ids_past_the_committed_layer() {
        // Committed: system node-1 (resp-1) + container node-2 (resp-2) — node-2 /
        // resp-2 are the max ids in committed.
        let dir = tempfile::tempdir().unwrap();
        let r = ModelRef::ProjectLocal(dir.path().to_path_buf());
        let mut model = ScryModel::new();
        let mut system = blank_node("node-1".into(), Kind::System, "Acme".into(), None);
        system.responsibilities = vec![resp("resp-1", "is the system")];
        let mut old = blank_node("node-2".into(), Kind::Container, "Old".into(), Some("node-1".into()));
        old.responsibilities = vec![resp("resp-2", "does the old thing")];
        model.nodes.push(system);
        model.nodes.push(old);
        scryer_core::write_model_at(&r, &model).unwrap();

        // Plan deletes node-2 — the draft holds only node-1, so minting from the
        // plan alone would re-issue node-2 / resp-2 (both still live in committed).
        let mut plan = model.clone();
        plan.nodes.retain(|n| n.id != "node-2");
        scryer_core::write_planned_at(&r, &plan).unwrap();

        let server = ScryerServer::new();
        let project = dir.path().to_string_lossy().to_string();
        server
            .add_container(Parameters(AddContainerRequest {
                project: Some(project),
                items: vec![ContainerItem {
                    parent_id: "node-1".into(),
                    name: "New".into(),
                    technology: None,
                    description: None,
                    external: false,
                    responsibilities: vec!["does the new thing".into()],
                    boundary_dir: None,
                }],
            }))
            .unwrap();

        let plan = read_plan(&dir);
        let new = plan.nodes.iter().find(|n| n.name == "New").unwrap();
        assert_eq!(new.id, "node-3", "node id must clear the committed max, not reuse plan-deleted node-2");
        assert_eq!(
            new.responsibilities[0].id, "resp-3",
            "responsibility id must clear the committed max, not reuse plan-deleted resp-2"
        );
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
        let m = read_plan(&dir);
        let container = m.nodes.iter().find(|n| n.kind == Kind::Container).unwrap();
        let container_id = container.id.clone();
        assert_eq!(container.technology.as_deref(), Some("Axum"));
        // blank statement filtered out; one responsibility
        assert_eq!(container.responsibilities.len(), 1);
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
        let m = read_plan(&dir);
        let component = m.nodes.iter().find(|n| n.kind == Kind::Component).unwrap();
        let component_id = component.id.clone();

        // a data-shape symbol with properties + a responsibility with sub-range
        server
            .add_symbol(Parameters(AddSymbolRequest {
                project: Some(project.clone()),
                items: vec![SymbolItem {
                    parent_id: component_id.clone(),
                    name: "Session".into(),
                    source_file: "crates/api/src/auth.rs".into(),
                    line: Some(10),
                    end_line: Some(20),
                    responsibilities: vec![ResponsibilityInput::Rich {
                        statement: "holds the logged-in session".into(),
                        line: Some(12),
                        end_line: Some(15),
                    }],
                    properties: vec![PropertyInput {
                        label: "token".into(),
                        description: "bearer token".into(),
                    }],
                    visual: None,
                }],
            }))
            .unwrap();
        let m = read_plan(&dir);
        let symbol = m.nodes.iter().find(|n| n.kind == Kind::Symbol).unwrap();
        let symbol_id = symbol.id.clone();
        assert_eq!(symbol.properties.len(), 1);
        let resp_id = symbol.responsibilities[0].id.clone();
        // responsibility anchored to file + symbol name + specific sub-range
        let resp_loc = &m.source_map.get(&resp_id).unwrap()[0];
        assert_eq!(resp_loc.pattern, "crates/api/src/auth.rs");
        assert_eq!(resp_loc.symbol.as_deref(), Some("Session"));
        assert_eq!(resp_loc.line, Some(12));
        assert_eq!(resp_loc.end_line, Some(15));
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
        let m = read_plan(&dir);
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
        let m = read_plan(&dir);
        assert_eq!(m.groups.len(), 1);
        let g = &m.groups[0];
        assert_eq!(g.parent_node_id.as_deref(), Some(system_id.as_str()));
        assert_eq!(g.member_ids.len(), 2);

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
    fn flag_drift_records_vagrant_and_flags_stale() {
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
        // The container was authored into the plan; commit it so there is a
        // COMMITTED, implemented claim for the stale path to flag.
        commit_plan(&dir);
        let m = read_back(&dir);
        let container = m.nodes.iter().find(|n| n.kind == Kind::Container).unwrap();
        let cid = container.id.clone();
        let rid = container.responsibilities[0].id.clone();

        server
            .flag_drift(Parameters(FlagDriftRequest {
                project: Some(project.clone()),
                node_id: cid.clone(),
                new_nodes: vec![],
                undescribed: vec![UndescribedItem {
                    statement: "exposes an undocumented admin endpoint".into(),
                    source_file: "api/admin.rs".into(),
                    symbol: Some("admin_handler".into()),
                    line: Some(42),
                    end_line: Some(58),
                    node_id: None,
                    node_key: None,
                }],
                stale: vec![StaleResponsibility {
                    responsibility_id: rid.clone(),
                    reason: "endpoint was removed".into(),
                    proposed_statement: None,
                }],
                stale_nodes: vec![],
                undescribed_properties: vec![],
                stale_properties: vec![],
            }))
            .unwrap();

        // TAKE MODEL — the committed model is UNTOUCHED: stale (like vagrant)
        // rides the working draft, never the source of truth.
        let m = read_back(&dir);
        let container = m.nodes.iter().find(|n| n.id == cid).unwrap();
        let orig = container.responsibilities.iter().find(|r| r.id == rid).unwrap();
        assert_ne!(orig.stale, Some(true), "committed claim is not flagged stale");
        assert!(
            container.responsibilities.iter().all(|r| r.vagrant != Some(true)),
            "undescribed behaviour must not land in the committed model"
        );

        // The plan (draft) carries BOTH findings: the original claim is flagged
        // stale, and the undescribed behaviour is a vagrant adoption awaiting a
        // verdict, source-anchored.
        let plan = scryer_core::read_planned_at(&scryer_core::ModelRef::ProjectLocal(
            dir.path().to_path_buf(),
        ))
        .unwrap();
        let pc = plan.nodes.iter().find(|n| n.id == cid).unwrap();
        let staled = pc.responsibilities.iter().find(|r| r.id == rid).unwrap();
        assert_eq!(staled.stale, Some(true), "stale flag rides the working draft");
        let vagrant = pc
            .responsibilities
            .iter()
            .find(|r| r.vagrant == Some(true))
            .expect("a vagrant adoption was proposed into the plan");
        assert_eq!(vagrant.statement, "exposes an undocumented admin endpoint");
        let anchor = &plan.source_map.get(&vagrant.id).unwrap()[0];
        assert_eq!(anchor.pattern, "api/admin.rs");
        assert_eq!(anchor.symbol.as_deref(), Some("admin_handler"));
        assert_eq!(anchor.line, Some(42));
        assert_eq!(anchor.end_line, Some(58));
    }

    #[test]
    fn flag_drift_records_a_reword_proposal_on_a_diverged_claim() {
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
                    responsibilities: vec!["charges the card in USD".into()],
                    boundary_dir: Some("api".into()),
                }],
            }))
            .unwrap();
        commit_plan(&dir);
        let m = read_back(&dir);
        let container = m.nodes.iter().find(|n| n.kind == Kind::Container).unwrap();
        let cid = container.id.clone();
        let rid = container.responsibilities[0].id.clone();

        // Drift judged the behaviour DIVERGED, not vanished: it proposes corrected
        // wording alongside the stale flag.
        let flag = |proposed: Option<&str>| {
            server
                .flag_drift(Parameters(FlagDriftRequest {
                    project: Some(project.clone()),
                    node_id: cid.clone(),
                    new_nodes: vec![],
                    undescribed: vec![],
                    stale: vec![StaleResponsibility {
                        responsibility_id: rid.clone(),
                        reason: "now charges in the account's currency, not USD".into(),
                        proposed_statement: proposed.map(|s| s.to_string()),
                    }],
                    stale_nodes: vec![],
                    undescribed_properties: vec![],
                    stale_properties: vec![],
                }))
                .unwrap();
        };
        let plan = || {
            scryer_core::read_planned_at(&scryer_core::ModelRef::ProjectLocal(
                dir.path().to_path_buf(),
            ))
            .unwrap()
        };

        flag(Some("charges the card in the account's currency"));

        // The proposal rides the planned claim next to `stale`; the live statement
        // is untouched until the user accepts (the source of truth waits for a
        // verdict).
        let p = plan();
        let staled = p
            .nodes
            .iter()
            .find(|n| n.id == cid)
            .unwrap()
            .responsibilities
            .iter()
            .find(|r| r.id == rid)
            .unwrap();
        assert_eq!(staled.stale, Some(true));
        assert_eq!(
            staled.stale_proposal.as_deref(),
            Some("charges the card in the account's currency"),
        );
        assert_eq!(
            staled.statement, "charges the card in USD",
            "the live statement is unchanged until the reword is accepted"
        );

        // The committed claim carries neither the flag nor the proposal.
        let committed = read_back(&dir);
        let cr = committed
            .nodes
            .iter()
            .find(|n| n.id == cid)
            .unwrap()
            .responsibilities
            .iter()
            .find(|r| r.id == rid)
            .unwrap();
        assert_eq!(cr.stale_proposal, None, "no proposal on the committed claim");

        // A blank proposal is not a proposal — re-flagging with whitespace clears it
        // while keeping the stale flag, so the user falls back to re-implement/drop.
        flag(Some("   "));
        let p = plan();
        let staled = p
            .nodes
            .iter()
            .find(|n| n.id == cid)
            .unwrap()
            .responsibilities
            .iter()
            .find(|r| r.id == rid)
            .unwrap();
        assert_eq!(staled.stale, Some(true));
        assert_eq!(staled.stale_proposal, None, "whitespace is filtered out");
    }

    #[test]
    fn flag_drift_routes_undescribed_to_the_owning_symbol() {
        let (server, dir, _sys) = temp_project();
        let project = dir.path().to_string_lossy().to_string();
        let mref = scryer_core::ModelRef::ProjectLocal(dir.path().to_path_buf());

        // Plan: container `c` → component `comp` → symbol `admin_handler`, the
        // symbol mapped to api/admin.rs via its responsibility.
        let mut m = scryer_core::ScryModel::new();
        let node = |v: serde_json::Value| serde_json::from_value::<scryer_core::Node>(v).unwrap();
        m.nodes.push(node(serde_json::json!({ "id": "c", "kind": "container", "name": "API" })));
        m.nodes
            .push(node(serde_json::json!({ "id": "comp", "kind": "component", "name": "Admin", "parentId": "c" })));
        m.nodes.push(node(serde_json::json!({
            "id": "sym", "kind": "symbol", "name": "admin_handler", "parentId": "comp",
            "responsibilities": [{ "id": "r-sym", "statement": "handle admin requests" }],
        })));
        m.source_map.insert(
            "r-sym".into(),
            vec![serde_json::from_value(serde_json::json!({ "pattern": "api/admin.rs" })).unwrap()],
        );
        m.boundaries.insert(
            "c".into(),
            vec![serde_json::from_value(serde_json::json!({ "pattern": "api/**/*" })).unwrap()],
        );
        scryer_core::write_planned_at(&mref, &m).unwrap();

        server
            .flag_drift(Parameters(FlagDriftRequest {
                project: Some(project),
                node_id: "c".into(), // reviewed at the container, as always
                new_nodes: vec![],
                undescribed: vec![UndescribedItem {
                    statement: "exposes an undocumented admin endpoint".into(),
                    source_file: "api/admin.rs".into(),
                    symbol: Some("admin_handler".into()),
                    line: Some(42),
                    end_line: Some(58),
                    node_id: None,
                    node_key: None,
                }],
                stale: vec![],
                stale_nodes: vec![],
                undescribed_properties: vec![],
                stale_properties: vec![],
            }))
            .unwrap();

        let plan = scryer_core::read_planned_at(&mref).unwrap();
        let sym = plan.nodes.iter().find(|n| n.id == "sym").unwrap();
        assert!(
            sym.responsibilities.iter().any(|r| r.vagrant == Some(true)),
            "the finding is routed to the symbol that owns api/admin.rs"
        );
        let cont = plan.nodes.iter().find(|n| n.id == "c").unwrap();
        assert!(
            cont.responsibilities.iter().all(|r| r.vagrant != Some(true)),
            "the finding must NOT land on the reviewed container"
        );
    }

    #[test]
    fn flag_drift_routes_undescribed_and_stale_properties_to_the_data_node() {
        let (server, dir, _sys) = temp_project();
        let project = dir.path().to_string_lossy().to_string();
        let mref = scryer_core::ModelRef::ProjectLocal(dir.path().to_path_buf());

        // Plan: container `c` → component `comp` → data-shape symbol `Settings`
        // mapped to api/settings.rs, already carrying one property `agent`.
        let mut m = scryer_core::ScryModel::new();
        let node = |v: serde_json::Value| serde_json::from_value::<scryer_core::Node>(v).unwrap();
        m.nodes.push(node(serde_json::json!({ "id": "c", "kind": "container", "name": "API" })));
        m.nodes
            .push(node(serde_json::json!({ "id": "comp", "kind": "component", "name": "Config", "parentId": "c" })));
        m.nodes.push(node(serde_json::json!({
            "id": "sym", "kind": "symbol", "name": "Settings", "parentId": "comp",
            "properties": [{ "label": "agent" }],
        })));
        m.source_map.insert(
            "sym".into(),
            vec![serde_json::from_value(serde_json::json!({ "pattern": "api/settings.rs", "symbol": "Settings" })).unwrap()],
        );
        m.boundaries.insert(
            "c".into(),
            vec![serde_json::from_value(serde_json::json!({ "pattern": "api/**/*" })).unwrap()],
        );
        scryer_core::write_planned_at(&mref, &m).unwrap();

        server
            .flag_drift(Parameters(FlagDriftRequest {
                project: Some(project),
                node_id: "c".into(), // reviewed at the container
                new_nodes: vec![],
                undescribed: vec![],
                stale: vec![],
                stale_nodes: vec![],
                undescribed_properties: vec![UndescribedProperty {
                    label: "confirm_launch".into(),
                    description: "gate before a billable launch".into(),
                    source_file: "api/settings.rs".into(),
                    symbol: Some("Settings".into()),
                    node_id: None,
                    node_key: None,
                }],
                stale_properties: vec![StaleProperty {
                    node_id: "sym".into(),
                    label: "agent".into(),
                    reason: "field was removed".into(),
                }],
            }))
            .unwrap();

        let plan = scryer_core::read_planned_at(&mref).unwrap();
        let sym = plan.nodes.iter().find(|n| n.id == "sym").unwrap();
        let new_prop = sym
            .properties
            .iter()
            .find(|p| p.label == "confirm_launch")
            .expect("undescribed field is routed to the data node as a property");
        assert_eq!(new_prop.vagrant, Some(true), "the new field lands vagrant, not as a responsibility");
        assert!(
            sym.responsibilities.iter().all(|r| r.vagrant != Some(true)),
            "a data field must NOT become a vagrant responsibility"
        );
        let stale = sym.properties.iter().find(|p| p.label == "agent").unwrap();
        assert_eq!(stale.stale, Some(true), "the removed field's property is flagged stale");
    }

    #[test]
    fn flag_drift_mints_a_vagrant_chain_for_unmodeled_code() {
        let (server, dir, _sys) = temp_project();
        let project = dir.path().to_string_lossy().to_string();
        let mref = scryer_core::ModelRef::ProjectLocal(dir.path().to_path_buf());

        // Plan: a container `c` with NO component/symbol for the new file — the
        // case that used to dump findings on the container.
        let mut m = scryer_core::ScryModel::new();
        let node = |v: serde_json::Value| serde_json::from_value::<scryer_core::Node>(v).unwrap();
        m.nodes.push(node(serde_json::json!({ "id": "c", "kind": "container", "name": "API" })));
        m.boundaries.insert(
            "c".into(),
            vec![serde_json::from_value(serde_json::json!({ "pattern": "api/**/*" })).unwrap()],
        );
        scryer_core::write_planned_at(&mref, &m).unwrap();

        // The agent mints the missing rungs: a component under the container, and
        // a symbol under it, then hangs the finding on the symbol via nodeKey.
        server
            .flag_drift(Parameters(FlagDriftRequest {
                project: Some(project),
                node_id: "c".into(),
                new_nodes: vec![
                    NewNode {
                        key: "k-comp".into(),
                        kind: "component".into(),
                        name: "Admin".into(),
                        parent_id: Some("c".into()),
                        parent_key: None,
                        description: None,
                        technology: None,
                    },
                    NewNode {
                        key: "k-sym".into(),
                        kind: "symbol".into(),
                        name: "admin_handler".into(),
                        parent_id: None,
                        parent_key: Some("k-comp".into()),
                        description: None,
                        technology: None,
                    },
                ],
                undescribed: vec![UndescribedItem {
                    statement: "exposes an undocumented admin endpoint".into(),
                    source_file: "api/admin.rs".into(),
                    symbol: Some("admin_handler".into()),
                    line: Some(42),
                    end_line: Some(58),
                    node_id: None,
                    node_key: Some("k-sym".into()),
                }],
                stale: vec![],
                stale_nodes: vec![],
                undescribed_properties: vec![],
                stale_properties: vec![],
            }))
            .unwrap();

        let plan = scryer_core::read_planned_at(&mref).unwrap();
        // Both minted nodes exist, are vagrant, and form a chain under `c`.
        let comp = plan.nodes.iter().find(|n| n.name == "Admin").expect("component minted");
        let sym = plan.nodes.iter().find(|n| n.name == "admin_handler").expect("symbol minted");
        assert_eq!(comp.vagrant, Some(true), "minted component is vagrant");
        assert_eq!(sym.vagrant, Some(true), "minted symbol is vagrant");
        assert_eq!(comp.parent_id.as_deref(), Some("c"));
        assert_eq!(sym.parent_id.as_deref(), Some(comp.id.as_str()));
        // The finding lands on the minted symbol, vagrant + anchored, NOT on `c`.
        assert!(sym.responsibilities.iter().any(|r| r.vagrant == Some(true)));
        let rid = &sym.responsibilities[0].id;
        assert_eq!(plan.source_map.get(rid).unwrap()[0].pattern, "api/admin.rs");
        let cont = plan.nodes.iter().find(|n| n.id == "c").unwrap();
        assert!(cont.responsibilities.is_empty(), "nothing parks on the container");
    }

    #[test]
    fn flag_drift_flags_a_whole_stale_node() {
        let (server, dir, _sys) = temp_project();
        let project = dir.path().to_string_lossy().to_string();
        let mref = scryer_core::ModelRef::ProjectLocal(dir.path().to_path_buf());

        // Plan: a container with a component whose backing folder was deleted.
        let mut m = scryer_core::ScryModel::new();
        let node = |v: serde_json::Value| serde_json::from_value::<scryer_core::Node>(v).unwrap();
        m.nodes.push(node(serde_json::json!({ "id": "c", "kind": "container", "name": "API" })));
        m.nodes.push(node(serde_json::json!({
            "id": "comp", "kind": "component", "name": "Admin", "parentId": "c"
        })));
        scryer_core::write_planned_at(&mref, &m).unwrap();

        server
            .flag_drift(Parameters(FlagDriftRequest {
                project: Some(project),
                node_id: "c".into(),
                new_nodes: vec![],
                undescribed: vec![],
                stale: vec![],
                stale_nodes: vec![StaleNode {
                    node_id: "comp".into(),
                    reason: "the admin/ folder was deleted".into(),
                }],
                undescribed_properties: vec![],
                stale_properties: vec![],
            }))
            .unwrap();

        let plan = scryer_core::read_planned_at(&mref).unwrap();
        let comp = plan.nodes.iter().find(|n| n.id == "comp").unwrap();
        assert_eq!(comp.stale, Some(true), "the node itself is flagged stale");
        let cont = plan.nodes.iter().find(|n| n.id == "c").unwrap();
        assert_ne!(cont.stale, Some(true), "the reviewed container is not flagged");
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

    #[test]
    fn reconcile_drift_advances_anchor_and_clears_scope() {
        use scryer_core::{drift, Source};

        let (server, dir, _system_id) = temp_project();
        let root = dir.path();
        let model_ref = ModelRef::ProjectLocal(root.to_path_buf());
        std::fs::create_dir_all(root.join("api/src")).unwrap();
        std::fs::write(root.join("api/src/server.rs"), "fn v1() {}").unwrap();

        let mut model = read_back(&dir);
        model
            .boundaries
            .insert("node-1".into(), vec![Source { pattern: "api/**/*".into(), comment: None }]);
        scryer_core::write_model_at(&model_ref, &model).unwrap();

        // Anchor in the past + a file touched after it → the scope is drifted.
        let old = drift::SyncState { reconciled_at: drift::now_secs(), commit: None, ..Default::default() };
        scryer_core::write_sync_state(&model_ref, &old).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        std::fs::write(root.join("api/src/server.rs"), "fn v2() {}").unwrap();
        assert!(
            !drift::drifted_scopes(&model, root, &old).is_empty(),
            "the touched file should drift against the old anchor"
        );

        // Reconcile advances the anchor to now → the same change stops surfacing.
        let project = root.to_string_lossy().to_string();
        let res = server
            .reconcile_drift(Parameters(ReconcileDriftRequest { project: Some(project) }))
            .unwrap();
        assert!(!res.is_error.unwrap_or(false));
        let fresh = scryer_core::read_sync_state(&model_ref);
        assert!(fresh.reconciled_at > old.reconciled_at, "anchor moved forward");
        assert!(
            drift::drifted_scopes(&model, root, &fresh).is_empty(),
            "post-reconcile, the prior change no longer reads as drift"
        );
    }
}
