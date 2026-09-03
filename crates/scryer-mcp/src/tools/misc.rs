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
        description = "Write the code-side mapping: `entries` (implementation anchors keyed by responsibility \
         id), `test_entries` (attached tests, same shape, `pattern` = test file and `symbol` = \
         the test), `schemas` (schema declarations keyed by node id), `boundaries` (directory \
         globs keyed by node id). An empty `locations`/`sources` array clears an entry. Also the \
         tool to attach a test AFTER a fold.\n\
         Rules: source-map, anchor-completeness, test-attachment"
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
        for entry in req.entries.iter().chain(req.test_entries.iter()) {
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

        // anchor-completeness makes anchoring the BUILD checkpoint, so an entry keyed to a
        // plan-added claim (not yet committed) is usually premature — the code
        // it points at may not exist. Warn, never reject: code-first flows
        // (adopting behaviour the codebase already has) legitimately anchor
        // early, and the author knows which case this is.
        let committed_resp_ids: HashSet<String> = committed
            .as_ref()
            .map(|c| {
                c.nodes
                    .iter()
                    .flat_map(|n| n.responsibilities.iter())
                    .chain(c.groups.iter().flat_map(|g| g.responsibilities.iter()))
                    .map(|r| r.id.clone())
                    .collect()
            })
            .unwrap_or_default();
        let mut premature: Vec<String> = req
            .entries
            .iter()
            .chain(req.test_entries.iter())
            .filter(|e| !e.locations.is_empty() && !committed_resp_ids.contains(&e.responsibility_id))
            .map(|e| e.responsibility_id.clone())
            .collect();
        premature.sort();
        premature.dedup();

        let count = req.entries.len()
            + req.test_entries.len()
            + req.schemas.len()
            + req.boundaries.len();
        let (mut normalized, mut committed_dirty) = apply_resp_anchor_entries(
            model_ref.project_path(),
            &mut model,
            &mut committed,
            std::mem::take(&mut req.entries),
            RespAnchorDim::Source,
        );
        // Attached tests: same routing, the test dimension.
        let (normalized_tests, tests_dirty) = apply_resp_anchor_entries(
            model_ref.project_path(),
            &mut model,
            &mut committed,
            std::mem::take(&mut req.test_entries),
            RespAnchorDim::Test,
        );
        normalized.extend(normalized_tests);
        committed_dirty |= tests_dirty;
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
        if !premature.is_empty() {
            msg.push_str(&format!(
                "\n\nWARNING — anchoring PLAN-ADDED claim(s) not yet committed: {}. An anchor \
                 records BUILT code and a test entry an EXISTING test (see anchor-completeness, test-attachment); the build \
                 checkpoint is `mark_implemented` (fold + `anchors` + `tests` in one call), and \
                 planned claims render WITHOUT their anchors until folded. If the code truly \
                 exists already (adopting behaviour the codebase has), keep the entry and fold \
                 soon; otherwise clear it (same entry, empty `locations`) and anchor at the fold.",
                premature.join(", ")
            ));
        }
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
        description = "GENERATION primitive: create or replace groups in bulk from raw `Group` JSON (one object \
         or an array in `data`). Each group lists `memberIds` (same C4 level) and `parentNodeId` \
         (the node whose children they are). For interactive editing use add_group / update_group \
         / delete_group.\n\
         Rules: groups, generation-fill"
    )]
    fn replace_groups(
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
        let mut reminter = RespIdReminter::for_replacement(&[&model, &committed_floor]);
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
        description = "Patch an existing group by id: name, description, members, or responsibilities. Only \
         fields present change; `memberIds` replaces the membership (2+ children of the group's \
         parent).\n\
         Rules: groups"
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

    #[tool(description = "Delete a group by id. Fold the deletion with mark_implemented `group_ids`.\n\
         Rules: fold-in-layers")]
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
        description = "Open a NEW change from `rationale` (the task in one sentence, as the dev put it), or \
         resume an open one with `change_id` (listed in get_pending's `openChanges`). This \
         session's plan writes tag to it from here; `mark_implemented {change}` folds exactly its \
         entries. Open one before any task beyond a one-line fix: plan writes are refused while no \
         change is open.\n\
         Rules: change-ledger, loop-plan"
    )]
    pub(crate) fn open_change(
        &self,
        Parameters(req): Parameters<OpenChangeRequest>,
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

        match (
            req.rationale.as_deref().map(str::trim).filter(|s| !s.is_empty()),
            req.change_id.as_deref().map(str::trim).filter(|s| !s.is_empty()),
        ) {
            (Some(_), Some(_)) => Ok(CallToolResult::error(vec![Content::text(
                "Pass rationale (open a new change) OR change_id (resume one), not both."
                    .to_string(),
            )])),
            (None, None) => {
                let plan = scryer_core::read_planned_at(&model_ref).unwrap_or_default();
                let current = match self.session_change(&model_ref) {
                    Some(id) => format!("Current change: {id}."),
                    None => "No current change — plan writes are refused until one is open."
                        .to_string(),
                };
                Ok(CallToolResult::error(vec![Content::text(format!(
                    "Pass rationale (open a new change) or change_id (resume one).\n{current}\n{}",
                    open_changes_line(&plan)
                ))]))
            }
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
        }
    }

    #[tool(
        description = "Record the developer's go-ahead on a change (`change_id`, or the session's current one): \
         snapshots its entries as the approved intent, so a claim you reword or add under it \
         afterwards lands as an amendment for the developer's verdict instead of folding.\n\
         Rules: sign-off, loop-sign-off, fold-after-sign-off"
    )]
    pub(crate) fn sign_off(
        &self,
        Parameters(req): Parameters<SignOffRequest>,
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

        let target = match req
            .change_id
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .or_else(|| self.session_change(&model_ref))
        {
            Some(c) => c,
            None => {
                return Ok(CallToolResult::error(vec![Content::text(
                    "Nothing to sign off — pass change_id, or open_change first.".to_string(),
                )]));
            }
        };
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
        let n = match scryer_core::changes::sign_off(
            &mut plan,
            &target,
            scryer_core::drift::now_secs(),
        ) {
            Ok(n) => n,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "{e}\n{}",
                    open_changes_line(&plan)
                ))]));
            }
        };
        if let Err(e) = scryer_core::write_planned_at(&model_ref, &plan) {
            return Ok(CallToolResult::error(vec![Content::text(e)]));
        }
        drop(_lock);
        // The session keeps working on the change it just signed off.
        self.set_session_change(Some((model_ref.project_path().to_path_buf(), target.clone())));
        return Ok(CallToolResult::success(vec![Content::text(format!(
            "Signed off {target} — {n} entr{} snapshotted as the developer's intent. From \
             here, a claim you reword or add under it is an amendment/addition: it lands \
             as vagrant for the developer's verdict at mark_implemented and does not fold. \
             If implementing shows a planned claim is wrong, reword it and fold the rest — \
             the reword waits.",
            if n == 1 { "y" } else { "ies" }
        ))]));
    }

    #[tool(
        description = "Close an EMPTY open change by id, recording it as abandoned with its rationale in history. \
         Refused while it has tagged entries: those close the change when they fold or are \
         reverted. Use it to end a task that filed nothing in the plan.\n\
         Rules: change-ledger"
    )]
    pub(crate) fn close_change(
        &self,
        Parameters(req): Parameters<CloseChangeRequest>,
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

        let cid = req.change_id.trim();
        if cid.is_empty() {
            return Ok(CallToolResult::error(vec![Content::text(
                "Pass change_id — the empty open change to close.".to_string(),
            )]));
        }
        let _lock = match lock_or_err(&model_ref) {
            Ok(l) => l,
            Err(e) => return Ok(e),
        };
        let meta = match scryer_core::changes::close_change(&model_ref, cid) {
            Ok(m) => m,
            Err(e) => {
                let plan = scryer_core::read_planned_at(&model_ref).unwrap_or_default();
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "{e}\n{}",
                    open_changes_line(&plan)
                ))]));
            }
        };
        if self.session_change(&model_ref).as_deref() == Some(cid) {
            self.set_session_change(None);
        }
        return Ok(CallToolResult::success(vec![Content::text(format!(
            "Closed {cid} — \"{}\" (abandoned, no entries). The rationale is kept in \
             the history log.",
            meta.rationale
        ))]));
    }

    #[tool(
        description = "Move pending work between changes without re-writing the spec. `ids` names nodes/groups \
         (carrier plus everything pending under it), responsibilities/links, a change id \
         (everything under it), or \"unfiled\"; `to` is the destination change id or \"unfiled\", \
         default the session's change.\n\
         Rules: change-ledger"
    )]
    pub(crate) fn refile(
        &self,
        Parameters(req): Parameters<RefileRequest>,
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

        let targets: Vec<String> = req
            .ids
            .iter()
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect();
        if targets.is_empty() {
            return Ok(CallToolResult::error(vec![Content::text(
                "refile needs at least one id — a node, group, responsibility, link, \
                 or change id, or \"unfiled\"."
                    .to_string(),
            )]));
        }
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
        let committed = scryer_core::read_model_at(&model_ref).unwrap_or_default();
        // No `to` means "into what I'm working on" — the session's own
        // change. With no selection either, there is nothing to infer.
        let session = self.session_change(&model_ref);
        let dest = match req.to.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            Some("unfiled") => None,
            Some(cid) => Some(cid.to_string()),
            None => match session {
                Some(c) => Some(c),
                None => {
                    return Ok(CallToolResult::error(vec![Content::text(
                        "Pass `to` (a change id, or \"unfiled\") — this session has no \
                         current change to move the work into."
                            .to_string(),
                    )]));
                }
            },
        };
        let outcome =
            match scryer_core::changes::retag(&committed, &mut plan, &targets, dest.as_deref())
            {
                Ok(o) => o,
                Err(e) => {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "{e}\n{}",
                        open_changes_line(&plan)
                    ))]));
                }
            };
        if !outcome.moved.is_empty() {
            if let Err(e) = scryer_core::write_planned_at(&model_ref, &plan) {
                return Ok(CallToolResult::error(vec![Content::text(e)]));
            }
        }
        let where_to = dest.as_deref().unwrap_or("unfiled");
        let mut msg = format!(
            "Moved {} entr{} to {where_to}.",
            outcome.moved.len(),
            if outcome.moved.len() == 1 { "y" } else { "ies" }
        );
        // Name what came from where: a re-file across three changes is
        // exactly when the caller needs to see what it actually touched.
        for (key, from) in &outcome.moved {
            msg.push_str(&format!(
                "\n- {key} (was {})",
                from.as_deref().unwrap_or("unfiled")
            ));
        }
        if !outcome.unmatched.is_empty() {
            msg.push_str(&format!(
                "\nNo pending work under: {} — already folded, or never planned.",
                outcome.unmatched.join(", ")
            ));
        }
        msg.push('\n');
        msg.push_str(&open_changes_line(&plan));
        drop(_lock);
        if let Some(h) = status_header(&model_ref) {
            msg.push_str(&format!("\n{h}"));
        }
        return Ok(CallToolResult::success(vec![Content::text(msg)]));
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
            style: None,
            layer: None,
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
            notes: None,
            position: None,
            directives: Vec::new(),
        }
    }

    /// `test_entries` (claim → attached test) follow the same single-home
    /// routing as `entries`: a committed claim's entry lands in the committed
    /// test_map with no draft shadow; a plan-added claim's stays in the
    /// draft until its claim folds.
    #[test]
    fn test_entries_route_to_the_single_home() {
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
                test_entries: vec![
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
            committed.test_map["r-c"][0].pattern, "tests/c.rs",
            "committed claim's attached test lives in committed"
        );
        assert!(!committed.test_map.contains_key("r-p"));
        let draft = scryer_core::read_planned_at(&model_ref).unwrap();
        assert_eq!(
            draft.test_map["r-p"][0].pattern, "tests/p.rs",
            "plan-added claim's attached test stays in the draft"
        );
        assert!(!draft.test_map.contains_key("r-c"), "no shadow copy in the draft");
    }

    /// An anchor keyed to a plan-added (uncommitted) claim is written — code-
    /// first flows legitimately anchor early — but the response warns loudly:
    /// anchoring is the build checkpoint (anchor-completeness), and the UI hides a planned
    /// claim's anchors until the fold. A committed claim's anchor stays quiet.
    #[test]
    fn anchoring_a_plan_added_claim_warns_but_writes() {
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
                entries: vec![entry("r-p", "src/f.ts")],
                test_entries: vec![],
                schemas: vec![],
                boundaries: vec![],
            }))
            .unwrap();
        assert!(!res.is_error.unwrap_or(false), "warn, never reject");
        let out = serde_json::to_string(&res.content).unwrap();
        assert!(out.contains("PLAN-ADDED") && out.contains("r-p"), "warns: {out}");
        let draft = scryer_core::read_planned_at(&model_ref).unwrap();
        assert_eq!(draft.source_map["r-p"][0].pattern, "src/f.ts", "the write still lands");

        let res = server
            .update_source_map(Parameters(UpdateSourceMapRequest {
                project: Some(dir.path().to_string_lossy().to_string()),
                entries: vec![entry("r-c", "src/f.ts")],
                test_entries: vec![],
                schemas: vec![],
                boundaries: vec![],
            }))
            .unwrap();
        let out = serde_json::to_string(&res.content).unwrap();
        assert!(!out.contains("PLAN-ADDED"), "committed claim's anchor is quiet: {out}");
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
                test_entries: vec![],
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
                test_entries: vec![],
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

        let server = ScryerServer::with_change(dir.path());
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

        let server = ScryerServer::with_change(dir.path());
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
        let minted = planned.groups[0].responsibilities[0].id.clone();
        assert!(scryer_core::is_minted_id(&minted, "resp"), "{minted}");
        assert_ne!(minted, "resp-3", "must not collide with the node-owned resp-3");
        let text = res
            .content
            .iter()
            .find_map(|c| c.as_text().map(|t| t.text.clone()))
            .unwrap();
        assert!(text.contains(&format!("group-1: 'new' → {minted}")), "reports the re-mint: {text}");
    }

    /// replace_groups (raw Group JSON) gets the same guard: invented ids are
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
        let server = ScryerServer::with_change(dir.path());
        let res = server
            .replace_groups(Parameters(SetGroupsRequest {
                project: Some(dir.path().to_string_lossy().to_string()),
                data: data.to_string(),
            }))
            .unwrap();

        let planned = scryer_core::read_planned_at(&model_ref).unwrap();
        let minted = planned.groups[0].responsibilities[0].id.clone();
        assert!(scryer_core::is_minted_id(&minted, "resp"), "{minted}");
        assert_ne!(minted, "resp-1");
        let text = res
            .content
            .iter()
            .find_map(|c| c.as_text().map(|t| t.text.clone()))
            .unwrap();
        assert!(text.contains(&format!("group-1: 'new' → {minted}")), "reports the re-mint: {text}");
    }
}
