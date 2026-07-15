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
        description = "Write the code-side mapping (agent-produced, regenerable). `entries` set source locations keyed by responsibility id — the conformance numerator (where reality discharges a responsibility). Each location is the SPECIFIC line range that does the work: `pattern` = file, `line`/`endLine` = the range, `symbol` = the enclosing definition (anchor + context). A line range must be a PROPER subset of its symbol — when one responsibility is the whole definition's work, omit `line`/`endLine` (a symbol-only anchor means the whole definition). Ranges that cover the whole symbol are normalized to symbol-only anchors and reported back. `verify_entries` record each claim's BACKING TESTS — keyed by responsibility id like `entries`, but pointing at the test that demonstrates the claim (`pattern` = test file, `symbol` = the test function; symbol-only means the whole test; optional `command` records how to run it, never executed). A separate dimension: where a claim is implemented vs. where it is verified. `schemas` set the declaration location of a schema-kind node (which has properties, not responsibilities) — keyed by node id, normally one location: `pattern` = file, `symbol` = the type name, `line`/`endLine` = the declaration range. `boundaries` set directory globs keyed by node id — the coverage denominator (the code region a node owns); use for containers/components, keeping a child's boundary within its parent's. Pass an empty `locations`/`sources` array to clear an entry."
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
        let mut model = match scryer_core::read_planned_seeded_at(&model_ref) {
            Ok(m) => m,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(read_fail("model", &model_ref, &e))]));
            }
        };

        let resp_ids: HashSet<&str> = model
            .nodes
            .iter()
            .flat_map(|n| n.responsibilities.iter())
            .chain(model.groups.iter().flat_map(|g| g.responsibilities.iter()))
            .map(|r| r.id.as_str())
            .collect();
        for entry in req.entries.iter().chain(req.verify_entries.iter()) {
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
        // Warn (don't reject) on boundary globs with no directory prefix: a
        // whole-repo glob owns every otherwise-unowned file, so drift and
        // coverage attribute unrelated changes to this node. Collected here,
        // reported in the write response so the author is steered immediately.
        let mut broad_boundaries: Vec<String> = Vec::new();
        for b in &req.boundaries {
            if !model.nodes.iter().any(|n| n.id == b.node_id) {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Node '{}' not found",
                    b.node_id
                ))]));
            }
            for s in &b.sources {
                if scryer_core::ownership::pattern_specificity(&s.pattern) == 0 {
                    broad_boundaries.push(format!("'{}' on {}", s.pattern, b.node_id));
                }
            }
        }

        // Code-side mapping has a SINGLE home, keyed by element: the committed
        // model owns every committed element's anchor; the planned draft holds
        // anchors only for elements it ADDS (not yet committed). So route by
        // element residence — a committed element's anchor is written to
        // committed and kept OUT of the draft (no shadow copy to drift); a
        // plan-added element's anchor stays in the draft and folds into committed
        // later (auto_commit carries it across). The working view merges the two
        // layers for display (see `effectiveSourceMap`), so a committed-side
        // write surfaces immediately without the draft mirroring it. Whole-symbol
        // line ranges are normalized to symbol-only anchors and reported back so
        // the agent learns. Routing + normalization shared with mark_implemented's
        // fold-time `anchors` (see apply_resp_anchor_entries).
        let mut committed = scryer_core::read_model_at(&model_ref).ok();
        let committed_node_ids: HashSet<String> = match committed.as_ref() {
            Some(c) => c.nodes.iter().map(|n| n.id.clone()).collect(),
            None => HashSet::new(),
        };

        let count = req.entries.len()
            + req.verify_entries.len()
            + req.schemas.len()
            + req.boundaries.len();
        let (mut normalized, mut committed_dirty) = apply_resp_anchor_entries(
            model_ref.project_path(),
            &mut model,
            &mut committed,
            std::mem::take(&mut req.entries),
            RespAnchorDim::Source,
        );
        // Backing tests: same routing, the verify dimension.
        let (normalized_verify, verify_dirty) = apply_resp_anchor_entries(
            model_ref.project_path(),
            &mut model,
            &mut committed,
            std::mem::take(&mut req.verify_entries),
            RespAnchorDim::Verify,
        );
        normalized.extend(normalized_verify);
        committed_dirty |= verify_dirty;
        for s in req.schemas {
            let key = s.node_id;
            if s.locations.is_empty() {
                model.source_map.remove(&key);
                if committed_node_ids.contains(&key) {
                    if let Some(c) = committed.as_mut() {
                        committed_dirty |= c.source_map.remove(&key).is_some();
                    }
                }
            } else if committed_node_ids.contains(&key) {
                model.source_map.remove(&key);
                if let Some(c) = committed.as_mut() {
                    c.source_map.insert(key, s.locations);
                    committed_dirty = true;
                }
            } else {
                model.source_map.insert(key, s.locations);
            }
        }
        for b in req.boundaries {
            let key = b.node_id;
            if b.sources.is_empty() {
                model.boundaries.remove(&key);
                if committed_node_ids.contains(&key) {
                    if let Some(c) = committed.as_mut() {
                        committed_dirty |= c.boundaries.remove(&key).is_some();
                    }
                }
            } else if committed_node_ids.contains(&key) {
                model.boundaries.remove(&key);
                if let Some(c) = committed.as_mut() {
                    c.boundaries.insert(key, b.sources);
                    committed_dirty = true;
                }
            } else {
                model.boundaries.insert(key, b.sources);
            }
        }

        if let Err(e) = scryer_core::write_planned_at(&model_ref, &model) {
            return Ok(CallToolResult::error(vec![Content::text(e)]));
        }
        // Persist the committed-side writes in the same lock so the single home
        // is updated atomically with the draft.
        if committed_dirty {
            if let Some(c) = committed {
                if let Err(e) = scryer_core::write_model_at(&model_ref, &c) {
                    return Ok(CallToolResult::error(vec![Content::text(e)]));
                }
            }
        }
        let mut msg = format!("Updated code-side mapping ({} entr(ies))", count);
        if !broad_boundaries.is_empty() {
            msg.push_str(&format!(
                "\n\nWARNING — {} boundary glob(s) with no directory prefix: {}. Such a glob \
                 owns every otherwise-unowned file in the repository, so drift and coverage \
                 attribute unrelated changes to this node. Scope it to the node's real code \
                 region (e.g. 'src/**/*').",
                broad_boundaries.len(),
                broad_boundaries.join(", ")
            ));
        }
        if !normalized.is_empty() {
            msg.push_str(&format!(
                "\n\nNormalized {} location(s) — the line range covered the whole enclosing symbol, so it was dropped and the symbol-only anchor kept. A range must be a PROPER subset of its symbol: map the specific lines that do each responsibility's work, or omit line/endLine to mean the whole definition:",
                normalized.len()
            ));
            for n in &normalized {
                msg.push_str(&format!("\n- {}", n));
            }
        }
        drop(_lock);
        if let Some(h) = status_header(&model_ref) {
            msg.push_str(&format!("\n{h}"));
        }
        Ok(CallToolResult::success(vec![Content::text(msg)]))
    }

    #[tool(
        description = "GENERATION-PIPELINE primitive — create or replace groups in bulk from raw `Group` JSON (used during codebase→model generation). Pass a single group object or an array of groups in `data`. Groups are organizational: at container level they represent deployment units, at component level they represent modules. Each group MUST list its `memberIds` and set `parentNodeId` to the node whose children those members are — parentNodeId anchors the group to that node's level so it renders inside that node's diagram (a memberless or parentNodeId-less group renders empty at the top level). Members must all be at the same C4 level. Groups can carry their own responsibilities. For interactive editing, use the typed `add_group` / `update_group` / `delete_group` instead."
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
        let mut model = match scryer_core::read_planned_seeded_at(&model_ref) {
            Ok(m) => m,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(read_fail("model", &model_ref, &e))]));
            }
        };

        let prior = model.clone();
        let mut groups: Vec<Group> = match serde_json::from_str::<Vec<Group>>(&req.data) {
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

        // Caller-invented responsibility ids never enter the model — re-mint
        // them against both layers (see RespIdReminter).
        let committed_floor = scryer_core::read_model_at(&model_ref).unwrap_or_default();
        let mut reminter = RespIdReminter::new(&[&model, &committed_floor]);
        for g in &groups {
            reminter.absorb(g.responsibilities.iter());
        }
        for g in &mut groups {
            reminter.remint(&g.id, g.responsibilities.iter_mut());
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

        let tag_warnings = match write_planned_tagged(
            &model_ref,
            &mut model,
            self.session_change(&model_ref).as_deref(),
        ) {
            Ok(w) => w,
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(e)])),
        };
        let mut msg = format!("Wrote {} group(s)", count);
        reminter.report_into(&mut msg);
        for w in &tag_warnings {
            msg.push_str(&format!("\n{w}"));
        }
        Ok(CallToolResult::success(vec![Content::text(msg)]))
    }

    #[tool(
        description = "Patch an existing group by id — change its name, description, members, or responsibilities. Only fields present in each item are changed; omit a field to leave it. `memberIds`, if given, replaces the membership (2+ nodes, all children of the group's parent node — same C4 level). This is the typed counterpart to `add_group`/`delete_group` for EDITING a group without reassembling raw JSON."
    )]
    fn update_group(
        &self,
        Parameters(mut req): Parameters<UpdateGroupRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = resolve_model_ref(req.project.as_deref())?;
        let _lock = match lock_or_err(&model_ref) {
            Ok(l) => l,
            Err(e) => return Ok(e),
        };
        let mut model = match scryer_core::read_planned_seeded_at(&model_ref) {
            Ok(m) => m,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(read_fail("model", &model_ref, &e))]));
            }
        };

        let prior = model.clone();

        // Caller-invented responsibility ids never enter the model — re-mint
        // them against both layers (see RespIdReminter).
        let committed_floor = scryer_core::read_model_at(&model_ref).unwrap_or_default();
        let mut reminter = RespIdReminter::new(&[&model, &committed_floor]);
        for item in &req.items {
            if let Some(v) = &item.responsibilities {
                reminter.absorb(v.iter());
            }
        }
        for item in &mut req.items {
            if let Some(v) = &mut item.responsibilities {
                reminter.remint(&item.group_id, v.iter_mut());
            }
        }

        let mut updated = 0usize;
        for item in &req.items {
            // Validate a replacement membership against the group's own level
            // before mutating, so a bad member leaves the model untouched.
            if let Some(members) = &item.member_ids {
                if members.len() < 2 {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "Group '{}' needs at least 2 members",
                        item.group_id
                    ))]));
                }
                let parent_node = match model.groups.iter().find(|g| g.id == item.group_id) {
                    Some(g) => g.parent_node_id.clone(),
                    None => {
                        return Ok(CallToolResult::error(vec![Content::text(format!(
                            "Group '{}' not found",
                            item.group_id
                        ))]))
                    }
                };
                for mid in members {
                    match model.nodes.iter().find(|n| &n.id == mid) {
                        None => {
                            return Ok(CallToolResult::error(vec![Content::text(format!(
                                "Group member '{}' is not a node",
                                mid
                            ))]))
                        }
                        Some(n) if parent_node.is_some() && n.parent_id != parent_node => {
                            return Ok(CallToolResult::error(vec![Content::text(format!(
                                "Group member '{}' is not a child of the group's parent node",
                                mid
                            ))]))
                        }
                        _ => {}
                    }
                }
            }

            let Some(g) = model.groups.iter_mut().find(|g| g.id == item.group_id) else {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Group '{}' not found",
                    item.group_id
                ))]));
            };
            if let Some(v) = &item.name {
                g.name = v.clone();
            }
            if let Some(v) = &item.description {
                g.description = Some(v.clone());
            }
            if let Some(v) = &item.member_ids {
                g.member_ids = v.clone();
            }
            if let Some(v) = &item.responsibilities {
                g.responsibilities = v.clone();
            }
            updated += 1;
        }
        enforce_readonly_directives(&mut model, &prior);

        let tag_warnings = match write_planned_tagged(
            &model_ref,
            &mut model,
            self.session_change(&model_ref).as_deref(),
        ) {
            Ok(w) => w,
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(e)])),
        };
        drop(_lock);
        let mut msg = format!("Updated {} group(s)", updated);
        reminter.report_into(&mut msg);
        for w in &tag_warnings {
            msg.push_str(&format!("\n{w}"));
        }
        if let Some(h) = status_header(&model_ref) {
            msg.push_str(&format!("\n{h}"));
        }
        Ok(CallToolResult::success(vec![Content::text(msg)]))
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
        let mut model = match scryer_core::read_planned_seeded_at(&model_ref) {
            Ok(m) => m,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(read_fail("model", &model_ref, &e))]));
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

        let tag_warnings = match write_planned_tagged(
            &model_ref,
            &mut model,
            self.session_change(&model_ref).as_deref(),
        ) {
            Ok(w) => w,
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(e)])),
        };
        drop(_lock);
        let mut msg = format!("Deleted group '{}'", req.group_id);
        for w in &tag_warnings {
            msg.push_str(&format!("\n{w}"));
        }
        if let Some(h) = status_header(&model_ref) {
            msg.push_str(&format!("\n{h}"));
        }
        Ok(CallToolResult::success(vec![Content::text(msg)]))
    }

    #[tool(
        description = "Select which CHANGE this session's plan writes belong to — a named partition of the plan carrying the dev's rationale, so parallel workstreams stay separable and review/fold can work per task. Pass `rationale` (the task in one sentence, as the dev put it) to OPEN a new change, or `change_id` to RESUME an open one from a prior session (list them via get_pending's openChanges). After this, every plan write in this session is tagged to the change automatically; `mark_implemented {change}` folds exactly its entries, and the change closes when its last entry folds — the rationale survives in the history log. Pass `clear: true` to detach (writes go unfiled, today's serial behavior). With no arguments, reports the current selection and the open changes. Use this at the start of a task when other work may share the plan; skip it for quick serial edits."
    )]
    pub(crate) fn set_change(
        &self,
        Parameters(req): Parameters<SetChangeRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = resolve_model_ref(req.project.as_deref())?;

        let open_changes_line = |m: &scryer_core::ScryModel| -> String {
            if m.changes.is_empty() {
                return "No open changes.".to_string();
            }
            let mut s = String::from("Open changes:");
            for c in &m.changes {
                let entries = m.change_map.values().filter(|v| *v == &c.id).count();
                s.push_str(&format!(
                    "\n  {} — \"{}\" ({} tagged entr{})",
                    c.id,
                    c.rationale,
                    entries,
                    if entries == 1 { "y" } else { "ies" }
                ));
            }
            s
        };

        if req.clear == Some(true) {
            self.set_session_change(None);
            return Ok(CallToolResult::success(vec![Content::text(
                "Detached — writes in this session now go unfiled.".to_string(),
            )]));
        }

        match (
            req.rationale.as_deref().map(str::trim).filter(|s| !s.is_empty()),
            req.change_id.as_deref().map(str::trim).filter(|s| !s.is_empty()),
        ) {
            (Some(_), Some(_)) => Ok(CallToolResult::error(vec![Content::text(
                "Pass rationale (open a new change) OR change_id (resume one), not both."
                    .to_string(),
            )])),
            // Open: register the change in the plan under the lock, then point
            // the session at it.
            (Some(rationale), None) => {
                let _lock = match lock_or_err(&model_ref) {
                    Ok(l) => l,
                    Err(e) => return Ok(e),
                };
                let mut plan = match scryer_core::read_planned_seeded_at(&model_ref) {
                    Ok(p) => p,
                    Err(e) => {
                        return Ok(CallToolResult::error(vec![Content::text(read_fail(
                            "plan", &model_ref, &e,
                        ))]));
                    }
                };
                let id = scryer_core::changes::open_change(
                    &mut plan,
                    rationale,
                    scryer_core::drift::now_secs(),
                );
                if let Err(e) = scryer_core::write_planned_at(&model_ref, &plan) {
                    return Ok(CallToolResult::error(vec![Content::text(e)]));
                }
                drop(_lock);
                self.set_session_change(Some((
                    model_ref.project_path().to_path_buf(),
                    id.clone(),
                )));
                Ok(CallToolResult::success(vec![Content::text(format!(
                    "Opened {id} — \"{rationale}\". Plan writes in this session are now \
                     tagged to it; fold it with mark_implemented {{change: \"{id}\"}} when \
                     the code is done."
                ))]))
            }
            // Resume: the change object persists in the plan; the session just
            // points at it again.
            (None, Some(cid)) => {
                let plan = match scryer_core::read_planned_at(&model_ref) {
                    Ok(p) => p,
                    Err(e) => {
                        return Ok(CallToolResult::error(vec![Content::text(read_fail(
                            "plan", &model_ref, &e,
                        ))]));
                    }
                };
                let Some(meta) = plan.changes.iter().find(|c| c.id == cid) else {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "No open change '{cid}'.\n{}",
                        open_changes_line(&plan)
                    ))]));
                };
                let entries = plan.change_map.values().filter(|v| *v == cid).count();
                self.set_session_change(Some((
                    model_ref.project_path().to_path_buf(),
                    cid.to_string(),
                )));
                Ok(CallToolResult::success(vec![Content::text(format!(
                    "Resumed {cid} — \"{}\" ({} tagged entr{}). Plan writes in this \
                     session are now tagged to it.",
                    meta.rationale,
                    entries,
                    if entries == 1 { "y" } else { "ies" }
                ))]))
            }
            // Report.
            (None, None) => {
                let plan = scryer_core::read_planned_at(&model_ref).unwrap_or_default();
                let current = match self.session_change(&model_ref) {
                    Some(id) => format!("Current change: {id}."),
                    None => "No current change — writes go unfiled.".to_string(),
                };
                Ok(CallToolResult::success(vec![Content::text(format!(
                    "{current}\n{}",
                    open_changes_line(&plan)
                ))]))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::handler::server::wrapper::Parameters;
    use scryer_core::{Group, Kind, ModelRef, Node, ScryModel};

    fn resp(id: &str) -> scryer_core::Responsibility {
        serde_json::from_value(serde_json::json!({ "id": id, "statement": format!("does {id}") }))
            .unwrap()
    }

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
            directives: Vec::new(),
        }
    }

    /// `verify_entries` (claim → backing test) follow the same single-home
    /// routing as `entries`: a committed claim's entry lands in the committed
    /// verify_map with no draft shadow; a plan-added claim's stays in the
    /// draft until its claim folds.
    #[test]
    fn verify_entries_route_to_the_single_home() {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());
        let resp = |id: &str| -> scryer_core::Responsibility {
            serde_json::from_value(serde_json::json!({ "id": id, "statement": "does" })).unwrap()
        };
        let mut committed = ScryModel::new();
        let mut comp = node("comp", Kind::Component, "Comp", None);
        comp.responsibilities.push(resp("r-c"));
        committed.nodes.push(comp);
        scryer_core::write_model_at(&model_ref, &committed).unwrap();
        scryer_core::ensure_planned_at(&model_ref).unwrap();
        let mut planned = scryer_core::read_planned_at(&model_ref).unwrap();
        planned
            .nodes
            .iter_mut()
            .find(|n| n.id == "comp")
            .unwrap()
            .responsibilities
            .push(resp("r-p"));
        scryer_core::write_planned_at(&model_ref, &planned).unwrap();

        let entry = |rid: &str, file: &str| SourceMapEntry {
            responsibility_id: rid.into(),
            locations: vec![
                serde_json::from_value(serde_json::json!({ "pattern": file })).unwrap(),
            ],
        };
        let server = ScryerServer::new();
        let res = server
            .update_source_map(Parameters(UpdateSourceMapRequest {
                project: Some(dir.path().to_string_lossy().to_string()),
                entries: vec![],
                verify_entries: vec![
                    entry("r-c", "tests/c.rs"),
                    entry("r-p", "tests/p.rs"),
                ],
                schemas: vec![],
                boundaries: vec![],
            }))
            .unwrap();
        assert!(!res.is_error.unwrap_or(false));

        let committed = scryer_core::read_model_at(&model_ref).unwrap();
        assert_eq!(
            committed.verify_map["r-c"][0].pattern, "tests/c.rs",
            "committed claim's backing test lives in committed"
        );
        assert!(!committed.verify_map.contains_key("r-p"));
        let draft = scryer_core::read_planned_at(&model_ref).unwrap();
        assert_eq!(
            draft.verify_map["r-p"][0].pattern, "tests/p.rs",
            "plan-added claim's backing test stays in the draft"
        );
        assert!(!draft.verify_map.contains_key("r-c"), "no shadow copy in the draft");
    }

    /// A boundary glob with no directory prefix is written (the user may mean
    /// it) but the response warns, steering the author to a scoped region.
    #[test]
    fn boundary_write_warns_on_whole_repo_glob() {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());
        let mut m = ScryModel::new();
        m.nodes.push(node("node-1", Kind::System, "Acme", None));
        m.nodes.push(node("node-2", Kind::Container, "Web", Some("node-1")));
        scryer_core::write_planned_at(&model_ref, &m).unwrap();

        let server = ScryerServer::new();
        let project = dir.path().to_string_lossy().to_string();
        let sources = |p: &str| -> Vec<scryer_core::Source> {
            vec![serde_json::from_value(serde_json::json!({ "pattern": p })).unwrap()]
        };

        let res = server
            .update_source_map(Parameters(UpdateSourceMapRequest {
                project: Some(project.clone()),
                entries: vec![],
                verify_entries: vec![],
                schemas: vec![],
                boundaries: vec![BoundaryEntry {
                    node_id: "node-2".into(),
                    sources: sources("**/*"),
                }],
            }))
            .unwrap();
        let out = serde_json::to_string(&res.content).unwrap();
        assert!(out.contains("no directory prefix"), "warns on **/*: {out}");
        // The write itself still lands — warn, don't reject.
        let written = scryer_core::read_planned_at(&model_ref).unwrap();
        assert_eq!(written.boundaries["node-2"][0].pattern, "**/*");

        let res = server
            .update_source_map(Parameters(UpdateSourceMapRequest {
                project: Some(project),
                entries: vec![],
                verify_entries: vec![],
                schemas: vec![],
                boundaries: vec![BoundaryEntry {
                    node_id: "node-2".into(),
                    sources: sources("web/**/*"),
                }],
            }))
            .unwrap();
        let out = serde_json::to_string(&res.content).unwrap();
        assert!(!out.contains("no directory prefix"), "scoped glob is quiet: {out}");
    }

    /// update_group patches an existing group by id: rename + clear
    /// responsibilities, leaving membership intact when memberIds is omitted;
    /// an unknown id is rejected.
    #[test]
    fn update_group_patches_fields_by_id() {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());
        let mut m = ScryModel::new();
        m.nodes.push(node("node-1", Kind::System, "Acme", None));
        m.nodes.push(node("node-2", Kind::Container, "Web", Some("node-1")));
        m.nodes.push(node("node-3", Kind::Container, "Worker", Some("node-1")));
        m.groups.push(Group {
            id: "group-1".into(),
            name: "Backend".into(),
            description: None,
            member_ids: vec!["node-2".into(), "node-3".into()],
            parent_group_id: None,
            parent_node_id: Some("node-1".into()),
            responsibilities: vec![],
            icon: None,
        });
        scryer_core::write_planned_at(&model_ref, &m).unwrap();

        let server = ScryerServer::new();
        let project = dir.path().to_string_lossy().to_string();

        server
            .update_group(Parameters(UpdateGroupRequest {
                project: Some(project.clone()),
                items: vec![UpdateGroupItem {
                    group_id: "group-1".into(),
                    name: Some("Platform".into()),
                    description: Some("deployable backend".into()),
                    member_ids: None,
                    responsibilities: Some(vec![]),
                }],
            }))
            .unwrap();

        let g = scryer_core::read_planned_at(&model_ref).unwrap().groups[0].clone();
        assert_eq!(g.name, "Platform");
        assert_eq!(g.description.as_deref(), Some("deployable backend"));
        assert_eq!(g.member_ids.len(), 2, "membership unchanged when memberIds omitted");
        assert!(g.responsibilities.is_empty(), "responsibilities cleared");

        let res = server
            .update_group(Parameters(UpdateGroupRequest {
                project: Some(project),
                items: vec![UpdateGroupItem {
                    group_id: "group-999".into(),
                    name: Some("X".into()),
                    description: None,
                    member_ids: None,
                    responsibilities: None,
                }],
            }))
            .unwrap();
        assert!(res.is_error.unwrap_or(false), "unknown group id rejected");
    }

    /// A caller-invented responsibility id in an update_group replacement is
    /// re-minted past both layers, and the response reports the new id.
    #[test]
    fn update_group_remints_caller_invented_responsibility_ids() {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());

        let mut m = ScryModel::new();
        let mut comp = node("node-1", Kind::Component, "A", None);
        comp.responsibilities = vec![resp("resp-3")];
        m.nodes.push(comp);
        m.nodes.push(node("node-2", Kind::Component, "B", None));
        m.groups.push(Group {
            id: "group-1".into(),
            name: "Pair".into(),
            description: None,
            member_ids: vec!["node-1".into(), "node-2".into()],
            parent_group_id: None,
            parent_node_id: None,
            responsibilities: Vec::new(),
            icon: None,
        });
        scryer_core::write_model_at(&model_ref, &m).unwrap();
        scryer_core::write_planned_at(&model_ref, &m).unwrap();

        let server = ScryerServer::new();
        let res = server
            .update_group(Parameters(UpdateGroupRequest {
                project: Some(dir.path().to_string_lossy().to_string()),
                items: vec![UpdateGroupItem {
                    group_id: "group-1".into(),
                    name: None,
                    description: None,
                    member_ids: None,
                    responsibilities: Some(vec![resp("new")]),
                }],
            }))
            .unwrap();

        let planned = scryer_core::read_planned_at(&model_ref).unwrap();
        assert_eq!(
            planned.groups[0].responsibilities[0].id, "resp-4",
            "minted past the node-owned resp-3"
        );
        let text = res
            .content
            .iter()
            .find_map(|c| c.as_text().map(|t| t.text.clone()))
            .unwrap();
        assert!(text.contains("group-1: 'new' → resp-4"), "reports the re-mint: {text}");
    }

    /// set_groups (raw Group JSON) gets the same guard: invented ids are
    /// re-minted before the groups land in the plan.
    #[test]
    fn set_groups_remints_caller_invented_responsibility_ids() {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());

        let mut m = ScryModel::new();
        let mut comp = node("node-1", Kind::Component, "A", None);
        comp.responsibilities = vec![resp("resp-1")];
        m.nodes.push(comp);
        m.nodes.push(node("node-2", Kind::Component, "B", None));
        scryer_core::write_model_at(&model_ref, &m).unwrap();
        scryer_core::write_planned_at(&model_ref, &m).unwrap();

        let data = serde_json::json!([{
            "id": "group-1",
            "name": "Pair",
            "memberIds": ["node-1", "node-2"],
            "responsibilities": [{ "id": "new", "statement": "coordinates the pair" }]
        }]);
        let server = ScryerServer::new();
        let res = server
            .set_groups(Parameters(SetGroupsRequest {
                project: Some(dir.path().to_string_lossy().to_string()),
                data: data.to_string(),
            }))
            .unwrap();

        let planned = scryer_core::read_planned_at(&model_ref).unwrap();
        assert_eq!(planned.groups[0].responsibilities[0].id, "resp-2");
        let text = res
            .content
            .iter()
            .find_map(|c| c.as_text().map(|t| t.text.clone()))
            .unwrap();
        assert!(text.contains("group-1: 'new' → resp-2"), "reports the re-mint: {text}");
    }
}
