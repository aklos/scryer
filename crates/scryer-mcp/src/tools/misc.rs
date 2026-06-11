use crate::helpers::*;
use crate::server::ScryerServer;
use crate::types::*;
use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, Content},
    tool, tool_router, ErrorData as McpError,
};
use scryer_core::Group;
use std::collections::HashSet;

#[tool_router(router = tool_router_misc, vis = "pub(crate)")]
impl ScryerServer {
    #[tool(
        description = "Write the code-side mapping (agent-produced, regenerable). `entries` set source locations keyed by responsibility id — the conformance numerator (where reality discharges a responsibility). Each location is the SPECIFIC line range that does the work: `pattern` = file, `line`/`endLine` = the range, `symbol` = the enclosing definition (anchor + context). A line range must be a PROPER subset of its symbol — when one responsibility is the whole definition's work, omit `line`/`endLine` (a symbol-only anchor means the whole definition). Ranges that cover the whole symbol are normalized to symbol-only anchors and reported back. `schemas` set the declaration location of a schema-kind node (which has properties, not responsibilities) — keyed by node id, normally one location: `pattern` = file, `symbol` = the type name, `line`/`endLine` = the declaration range. `boundaries` set directory globs keyed by node id — the coverage denominator (the code region a node owns); use for containers/components, keeping a child's boundary within its parent's. Pass an empty `locations`/`sources` array to clear an entry."
    )]
    fn update_source_map(
        &self,
        Parameters(mut req): Parameters<UpdateSourceMapRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = resolve_model_ref(req.project.as_deref())?;
        let _lock = match lock_or_err(&model_ref) {
            Ok(l) => l,
            Err(e) => return Ok(e),
        };
        let mut model = match scryer_core::read_model_at(&model_ref) {
            Ok(m) => m,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to read model at {}: {}",
                    model_ref, e
                ))]));
            }
        };

        let resp_ids: HashSet<&str> = model
            .nodes
            .iter()
            .flat_map(|n| n.responsibilities.iter())
            .chain(model.groups.iter().flat_map(|g| g.responsibilities.iter()))
            .map(|r| r.id.as_str())
            .collect();
        for entry in &req.entries {
            if !resp_ids.contains(entry.responsibility_id.as_str()) {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Responsibility '{}' not found",
                    entry.responsibility_id
                ))]));
            }
        }
        for s in &req.schemas {
            match model.nodes.iter().find(|n| n.id == s.node_id) {
                None => {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "Node '{}' not found",
                        s.node_id
                    ))]));
                }
                Some(n) if n.properties.is_empty() => {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "Node '{}' declares no properties — a `schemas` entry maps a data shape's declaration location; use `entries` (responsibilities) for behavior",
                        s.node_id
                    ))]));
                }
                _ => {}
            }
        }
        for b in &req.boundaries {
            if !model.nodes.iter().any(|n| n.id == b.node_id) {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Node '{}' not found",
                    b.node_id
                ))]));
            }
        }

        // Normalize whole-symbol mappings: an explicit line range must be a
        // PROPER subset of its enclosing symbol. A range covering the whole
        // extent is stripped to the symbol-only anchor (the honest encoding
        // for "this whole definition") and reported back so the agent learns.
        let mut normalized: Vec<String> = Vec::new();
        {
            let mut resolver =
                scryer_extract::anchors::ExtentResolver::new(model_ref.project_path());
            for entry in &mut req.entries {
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

        let count = req.entries.len() + req.schemas.len() + req.boundaries.len();
        for entry in req.entries {
            if entry.locations.is_empty() {
                model.source_map.remove(&entry.responsibility_id);
            } else {
                model.source_map.insert(entry.responsibility_id, entry.locations);
            }
        }
        for s in req.schemas {
            if s.locations.is_empty() {
                model.source_map.remove(&s.node_id);
            } else {
                model.source_map.insert(s.node_id, s.locations);
            }
        }
        for b in req.boundaries {
            if b.sources.is_empty() {
                model.boundaries.remove(&b.node_id);
            } else {
                model.boundaries.insert(b.node_id, b.sources);
            }
        }

        if let Err(e) = scryer_core::write_model_at(&model_ref, &model) {
            return Ok(CallToolResult::error(vec![Content::text(e)]));
        }
        let _ = scryer_core::save_baseline_at(&model_ref, &model);
        let mut msg = format!("Updated code-side mapping ({} entr(ies))", count);
        if !normalized.is_empty() {
            msg.push_str(&format!(
                "\n\nNormalized {} location(s) — the line range covered the whole enclosing symbol, so it was dropped and the symbol-only anchor kept. A range must be a PROPER subset of its symbol: map the specific lines that do each responsibility's work, or omit line/endLine to mean the whole definition:",
                normalized.len()
            ));
            for n in &normalized {
                msg.push_str(&format!("\n- {}", n));
            }
        }
        Ok(CallToolResult::success(vec![Content::text(msg)]))
    }

    #[tool(
        description = "Create or replace one or more groups. Pass a single group object or an array of groups in `data`. Groups are organizational: at container level they represent deployment units, at component level they represent modules. Each group MUST list its `memberIds` and set `parentNodeId` to the node whose children those members are — parentNodeId anchors the group to that node's level so it renders inside that node's diagram (a memberless or parentNodeId-less group renders empty at the top level). Members must all be at the same C4 level. Groups can carry their own responsibilities."
    )]
    fn set_groups(
        &self,
        Parameters(req): Parameters<SetGroupsRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = resolve_model_ref(req.project.as_deref())?;
        let _lock = match lock_or_err(&model_ref) {
            Ok(l) => l,
            Err(e) => return Ok(e),
        };
        let mut model = match scryer_core::read_model_at(&model_ref) {
            Ok(m) => m,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to read model at {}: {}",
                    model_ref, e
                ))]));
            }
        };

        let prior = model.clone();
        let groups: Vec<Group> = match serde_json::from_str::<Vec<Group>>(&req.data) {
            Ok(arr) => arr,
            Err(_) => match serde_json::from_str::<Group>(&req.data) {
                Ok(g) => vec![g],
                Err(e) => {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "Invalid group JSON: {}",
                        e
                    ))]));
                }
            },
        };
        if groups.is_empty() {
            return Ok(CallToolResult::error(vec![Content::text(
                "Empty group array",
            )]));
        }

        // Validate members exist + share a level
        let node_kinds: std::collections::HashMap<&str, scryer_core::Kind> =
            model.nodes.iter().map(|n| (n.id.as_str(), n.kind)).collect();
        for g in &groups {
            let mut kinds: HashSet<scryer_core::Kind> = HashSet::new();
            for mid in &g.member_ids {
                match node_kinds.get(mid.as_str()) {
                    Some(k) => {
                        kinds.insert(*k);
                    }
                    None => {
                        return Ok(CallToolResult::error(vec![Content::text(format!(
                            "Group '{}' member '{}' is not a node",
                            g.id, mid
                        ))]))
                    }
                }
            }
            if kinds.len() > 1 {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Group '{}' mixes member kinds — all members must be at the same level",
                    g.id
                ))]));
            }
        }

        let count = groups.len();
        for g in groups {
            if let Some(existing) = model.groups.iter_mut().find(|x| x.id == g.id) {
                *existing = g;
            } else {
                model.groups.push(g);
            }
        }
        enforce_readonly_directives(&mut model, &prior);

        if let Err(e) = scryer_core::write_model_at(&model_ref, &model) {
            return Ok(CallToolResult::error(vec![Content::text(e)]));
        }
        let _ = scryer_core::save_baseline_at(&model_ref, &model);
        Ok(CallToolResult::success(vec![Content::text(format!(
            "Wrote {} group(s)",
            count
        ))]))
    }

    #[tool(description = "Delete a group by id.")]
    fn delete_group(
        &self,
        Parameters(req): Parameters<DeleteGroupRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = resolve_model_ref(req.project.as_deref())?;
        let _lock = match lock_or_err(&model_ref) {
            Ok(l) => l,
            Err(e) => return Ok(e),
        };
        let mut model = match scryer_core::read_model_at(&model_ref) {
            Ok(m) => m,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Failed to read model at {}: {}",
                    model_ref, e
                ))]));
            }
        };

        let before = model.groups.len();
        model.groups.retain(|g| g.id != req.group_id);
        // Detach any child groups from the deleted parent
        for g in model.groups.iter_mut() {
            if g.parent_group_id.as_deref() == Some(&req.group_id) {
                g.parent_group_id = None;
            }
        }
        if model.groups.len() == before {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "Group '{}' not found",
                req.group_id
            ))]));
        }

        if let Err(e) = scryer_core::write_model_at(&model_ref, &model) {
            return Ok(CallToolResult::error(vec![Content::text(e)]));
        }
        let _ = scryer_core::save_baseline_at(&model_ref, &model);
        Ok(CallToolResult::success(vec![Content::text(format!(
            "Deleted group '{}'",
            req.group_id
        ))]))
    }

    #[tool(
        description = "Pause or resume drift detection for this project's model. Call with active=true before implementing code; active=false after."
    )]
    fn set_implementing(
        &self,
        Parameters(req): Parameters<SetImplementingRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = resolve_model_ref(req.project.as_deref())?;
        if let Err(e) = scryer_core::set_implementing_at(&model_ref, req.active) {
            return Ok(CallToolResult::error(vec![Content::text(e)]));
        }
        Ok(CallToolResult::success(vec![Content::text(format!(
            "Implementing flag = {}",
            req.active
        ))]))
    }
}
