use crate::helpers::*;
use crate::server::ScryerServer;
use crate::types::*;
use crate::validate;
use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, Content},
    tool, tool_router, ErrorData as McpError,
};
use scryer_core::{Kind, Link, Node, ScryModel};
use scryer_core::history::{EventKind, EventRow, HistoryEvent};
use std::collections::{HashMap, HashSet};

use super::fold_gate;

/// Remove code-side mapping for a set of nodes about to be deleted: their
/// boundary globs (keyed by node id) and the source-map locations of every
/// responsibility they own (keyed by responsibility id). Call before the nodes
/// are retained out of the model, since it reads their responsibilities.
fn prune_code_map(model: &mut ScryModel, removed_node_ids: &HashSet<String>) {
    let removed_resp_ids: HashSet<String> = model
        .nodes
        .iter()
        .filter(|n| removed_node_ids.contains(&n.id))
        .flat_map(|n| n.responsibilities.iter().map(|r| r.id.clone()))
        .collect();
    // source_map keys are responsibility ids or schema node ids — drop both.
    model
        .source_map
        .retain(|k, _| !removed_resp_ids.contains(k) && !removed_node_ids.contains(k));
    model.boundaries.retain(|k, _| !removed_node_ids.contains(k));
}

/// Replace `node_id`'s whole subtree in ONE model layer with `nodes` + `links`:
/// drop every current descendant (and its links and code-side mapping), then
/// insert the payload nodes (skipping `node_id` itself) and any payload link
/// whose endpoints both exist afterward. Returns `(applied, dropped)`: `applied`
/// is `false` — leaving the layer untouched — when `node_id` isn't present, so a
/// caller can apply the same replacement to both layers and skip whichever lacks
/// the target; `dropped` describes every payload link discarded for an endpoint
/// that no node in the resulting layer provides, so the loss is never silent.
/// The ids a subtree replacement legitimately owns: `node_id` itself plus every
/// descendant, i.e. exactly what [`replace_subtree`] swaps out. An id outside
/// this set belongs to some other part of the model and is not the caller's to
/// reuse (see `remint_colliding_node_ids`).
fn subtree_ids(model: &ScryModel, node_id: &str) -> HashSet<String> {
    let mut ids: HashSet<String> = HashSet::from([node_id.to_string()]);
    let mut frontier = vec![node_id.to_string()];
    while let Some(id) = frontier.pop() {
        for child in model.nodes.iter().filter(|n| n.parent_id.as_deref() == Some(&id)) {
            if ids.insert(child.id.clone()) {
                frontier.push(child.id.clone());
            }
        }
    }
    ids
}

fn replace_subtree(
    model: &mut ScryModel,
    node_id: &str,
    nodes: &[Node],
    links: &[Link],
) -> (bool, Vec<String>) {
    if !model.nodes.iter().any(|n| n.id == node_id) {
        return (false, Vec::new());
    }
    let mut to_remove: HashSet<String> = HashSet::new();
    let mut frontier = vec![node_id.to_string()];
    while let Some(id) = frontier.pop() {
        for child in model.nodes.iter().filter(|n| n.parent_id.as_deref() == Some(&id)) {
            if to_remove.insert(child.id.clone()) {
                frontier.push(child.id.clone());
            }
        }
    }
    prune_code_map(model, &to_remove);
    model.nodes.retain(|n| !to_remove.contains(&n.id));
    model
        .links
        .retain(|l| !to_remove.contains(&l.src) && !to_remove.contains(&l.dst));
    for n in nodes {
        if n.id == node_id {
            continue;
        }
        model.nodes.push(n.clone());
    }
    let node_ids: HashSet<&str> = model.nodes.iter().map(|n| n.id.as_str()).collect();
    let mut dropped = Vec::new();
    for l in links {
        let src_ok = node_ids.contains(l.src.as_str());
        let dst_ok = node_ids.contains(l.dst.as_str());
        if src_ok && dst_ok {
            model.links.push(l.clone());
        } else {
            let reason = match (src_ok, dst_ok) {
                (false, false) => "unknown src and dst",
                (false, true) => "unknown src",
                (true, false) => "unknown dst",
                (true, true) => unreachable!(),
            };
            dropped.push(format!("{} -> {} ({reason})", l.src, l.dst));
        }
    }
    (true, dropped)
}

/// Fold a set of target nodes out of ONE model layer: relocate each target's own
/// non-vagrant responsibilities (keeping their ids and source anchors) up to its
/// parent, then remove the target and its descendants along with their links,
/// boundaries, and remaining anchors. Relocating BEFORE pruning is what keeps the
/// parent's coverage of that code intact — `prune_code_map` only drops anchors of
/// responsibilities still owned by a removed node, so a relocated claim survives
/// and the file never goes dark. Code on disk is never touched.
///
/// Returns `(relocated, removed, dropped_descendant_resps)` — the last is the count
/// of responsibilities on removed DESCENDANTS that are lost (a target's own claims
/// are relocated, never dropped), surfaced so the loss is never silent.
fn fold_out_layer(model: &mut ScryModel, target_ids: &[String]) -> (usize, usize, usize) {
    // The full removal set FIRST — every target plus its descendants — so
    // relocation knows what survives and never moves a claim onto a node that is
    // itself about to be removed.
    let mut to_remove: HashSet<String> = target_ids.iter().cloned().collect();
    let mut frontier: Vec<String> = target_ids.to_vec();
    while let Some(id) = frontier.pop() {
        for child in model.nodes.iter().filter(|n| n.parent_id.as_deref() == Some(&id)) {
            if to_remove.insert(child.id.clone()) {
                frontier.push(child.id.clone());
            }
        }
    }

    let mut relocated = 0usize;
    let mut dropped = 0usize;

    // 1) Relocate each target's own non-vagrant responsibilities to its nearest
    //    SURVIVING ancestor. If none survives (a top-level target, or every
    //    ancestor is also being removed) the claims are lost — count them as
    //    dropped so the loss is never silent (both were previously miscounted:
    //    reported "relocated" onto a doomed node, or dropped uncounted).
    for id in target_ids {
        let Some(idx) = model.nodes.iter().position(|n| &n.id == id) else {
            continue;
        };
        let mut moving = Vec::new();
        model.nodes[idx].responsibilities.retain(|r| {
            if r.vagrant == Some(true) {
                true
            } else {
                moving.push(r.clone());
                false
            }
        });
        if moving.is_empty() {
            continue;
        }
        match surviving_ancestor(model, id, &to_remove) {
            Some(anc) => match model.nodes.iter_mut().find(|n| n.id == anc) {
                Some(parent) => {
                    relocated += moving.len();
                    parent.responsibilities.extend(moving);
                }
                None => model.nodes[idx].responsibilities.extend(moving),
            },
            None => dropped += moving.len(),
        }
    }

    // Claims on removed DESCENDANTS (nodes in the set that aren't targets) are
    // lost too — count them alongside the un-relocatable targets' claims above.
    dropped += model
        .nodes
        .iter()
        .filter(|n| to_remove.contains(&n.id) && !target_ids.iter().any(|t| t == &n.id))
        .map(|n| n.responsibilities.len())
        .sum::<usize>();

    let before = model.nodes.len();
    prune_code_map(model, &to_remove);
    model.nodes.retain(|n| !to_remove.contains(&n.id));
    model
        .links
        .retain(|l| !to_remove.contains(&l.src) && !to_remove.contains(&l.dst));
    for g in model.groups.iter_mut() {
        g.member_ids.retain(|m| !to_remove.contains(m));
    }
    (relocated, before - model.nodes.len(), dropped)
}

/// Summary line for an explicit link/group fold — spells out how many of the
/// folded ids were removals (deletion folds) vs. changes landing in committed.
fn fold_summary(noun: &str, total: usize, removals: usize) -> String {
    let s = if total == 1 { "" } else { "s" };
    if removals == 0 {
        format!("Committed {total} {noun}{s} into the model.")
    } else if removals == total {
        format!("Committed the removal of {total} {noun}{s} from the model.")
    } else {
        format!(
            "Committed {total} {noun}{s} into the model ({removals} removal{}).",
            if removals == 1 { "" } else { "s" }
        )
    }
}

/// Surface the parent-residence guard's real recovery at the tool layer: core's
/// "commit the parent first" is correct advice for one missing parent, but in a
/// never-committed (design-first) model it is a ladder to force-committing the
/// whole tree — `commit_ancestors` is the honest exit, so the refusal names it.
fn ancestor_hint(e: String) -> String {
    if e.contains("is not in the committed model yet") {
        format!(
            "{e} — or retry with commit_ancestors: true to fold the plan-only ancestor \
             chain structure-only first (their unbuilt claims stay pending in the plan)"
        )
    } else {
        e
    }
}

/// Fold every plan entry tagged to one change — the `mark_implemented {change}`
/// path ("implement THIS change" instead of element-by-element bookkeeping).
/// Dependency order: resident nodes root-ward (each pulling its ready
/// dependents, exactly like a whole-node fold), then claims and properties
/// whose hosts didn't just fold, then groups, links, and node deletions. The
/// ledger GC inside each fold retires the tags as their entries leave the
/// diff; when the last one goes, the change closes and its rationale lands in
/// the history log (see `scryer_core::changes`). Per-host `impl` events carry
/// the change id. The caller holds the model lock and reports the returned
/// summaries.
fn fold_change_by_id(
    model_ref: &scryer_core::ModelRef,
    planned: &ScryModel,
    before_stmts: &HashMap<String, String>,
    cid: &str,
    commit_ancestors: bool,
    withhold: &HashSet<String>,
) -> Result<Vec<String>, CallToolResult> {
    use scryer_core::changes as ledger;
    use scryer_core::diff::ElementKind as EK;
    let fail = |e: String| CallToolResult::error(vec![Content::text(e)]);

    let Some(meta) = planned.changes.iter().find(|c| c.id == cid) else {
        let open: Vec<&str> = planned.changes.iter().map(|c| c.id.as_str()).collect();
        return Err(fail(format!(
            "No open change '{cid}'. Open changes: {}",
            if open.is_empty() { "none".to_string() } else { open.join(", ") }
        )));
    };
    let keys: Vec<String> = planned
        .change_map
        .iter()
        .filter(|(_, v)| v.as_str() == cid)
        .map(|(k, _)| k.clone())
        .collect();
    if keys.is_empty() {
        return Ok(vec![format!(
            "Change {cid} (\"{}\") has no tagged entries — nothing to fold; it stays open \
             until work is written to it.",
            meta.rationale
        )]);
    }

    let mut resident_nodes: Vec<String> = Vec::new();
    let mut deleted_nodes: Vec<String> = Vec::new();
    let mut resps: Vec<String> = Vec::new();
    let mut props: Vec<(String, String)> = Vec::new();
    let mut groups: Vec<String> = Vec::new();
    let mut links: Vec<String> = Vec::new();
    for k in &keys {
        let Some((kind, owner, id)) = ledger::parse_key(k) else { continue };
        match kind {
            EK::Node => {
                if planned.nodes.iter().any(|n| n.id == id) {
                    resident_nodes.push(id);
                } else {
                    deleted_nodes.push(id);
                }
            }
            EK::Responsibility => resps.push(id),
            EK::Property => props.push((owner.unwrap_or_default(), id)),
            EK::Group => groups.push(id),
            EK::Link => links.push(id),
        }
    }

    // Root-ward, so each node folds under an already-committed parent. Cycle
    // guard like every chain walker.
    let depth = |id: &str| {
        let mut d = 0usize;
        let mut seen: HashSet<String> = std::iter::once(id.to_string()).collect();
        let mut cur = planned.nodes.iter().find(|n| n.id == id).and_then(|n| n.parent_id.clone());
        while let Some(pid) = cur {
            if !seen.insert(pid.clone()) {
                break;
            }
            d += 1;
            cur = planned.nodes.iter().find(|n| n.id == pid).and_then(|n| n.parent_id.clone());
        }
        d
    };
    resident_nodes.sort_by_key(|id| depth(id));

    let host_of_resp = |rid: &str| -> Option<String> {
        planned
            .nodes
            .iter()
            .find(|n| n.responsibilities.iter().any(|r| r.id == rid))
            .map(|n| n.id.clone())
            .or_else(|| {
                planned
                    .groups
                    .iter()
                    .find(|g| g.responsibilities.iter().any(|r| r.id == rid))
                    .map(|g| g.id.clone())
            })
    };

    // Design-first escape, composed exactly as on the single-node path: every
    // node this fold lands on gets its plan-only ancestor chain committed
    // structure-only first; a host that only receives scoped claims rides the
    // cascade itself (include_self).
    if commit_ancestors {
        let mut hosts: Vec<String> = resident_nodes.clone();
        hosts.extend(
            resps
                .iter()
                .filter_map(|rid| host_of_resp(rid))
                .filter(|h| planned.nodes.iter().any(|n| &n.id == h)),
        );
        hosts.extend(props.iter().map(|(o, _)| o.clone()));
        hosts.sort();
        hosts.dedup();
        for h in hosts {
            let include_self = !resident_nodes.contains(&h);
            if let Err(e) = scryer_core::commit_plan_only_ancestors(model_ref, &h, include_self) {
                return Err(fail(ancestor_hint(e)));
            }
        }
    }

    let mut folded_resps = 0usize;
    let mut folded_props = 0usize;
    for id in &resident_nodes {
        if let Err(e) =
            scryer_core::commit_element_withholding(model_ref, EK::Node, None, id, withhold)
        {
            return Err(fail(ancestor_hint(e)));
        }
        if let Err(e) = scryer_core::commit_ready_dependents(model_ref, id) {
            return Err(fail(e));
        }
    }
    let mut withheld_here = 0usize;
    for rid in &resps {
        // A whole-node fold above already carried this claim across.
        if host_of_resp(rid).is_some_and(|h| resident_nodes.contains(&h)) {
            continue;
        }
        // The gates said no — it stays in the plan (and in the change).
        if withhold.contains(rid) {
            withheld_here += 1;
            continue;
        }
        if let Err(e) = scryer_core::commit_element(model_ref, EK::Responsibility, None, rid) {
            return Err(fail(ancestor_hint(e)));
        }
        folded_resps += 1;
    }
    for (owner, label) in &props {
        if resident_nodes.contains(owner) {
            continue;
        }
        if let Err(e) = scryer_core::commit_element(model_ref, EK::Property, Some(owner), label) {
            return Err(fail(ancestor_hint(e)));
        }
        folded_props += 1;
    }
    for gid in &groups {
        if let Err(e) = scryer_core::commit_element(model_ref, EK::Group, None, gid) {
            return Err(fail(e));
        }
    }
    for lid in &links {
        if let Err(e) = scryer_core::commit_element(model_ref, EK::Link, None, lid) {
            return Err(fail(e));
        }
    }
    for id in &deleted_nodes {
        if let Err(e) = scryer_core::commit_element(model_ref, EK::Node, None, id) {
            return Err(fail(e));
        }
    }

    // Per-host `impl` timeline events carrying the change id, mirroring the
    // single-node path: rows are the claims THIS fold added or reworded.
    if let Ok(after) = scryer_core::read_model_at(model_ref) {
        let mut hosts: Vec<String> = resident_nodes.clone();
        hosts.extend(resps.iter().filter_map(|rid| host_of_resp(rid)));
        hosts.sort();
        hosts.dedup();
        let now = scryer_core::drift::now_secs();
        for host in hosts {
            let Some(node) = after.nodes.iter().find(|n| n.id == host) else { continue };
            let rows: Vec<EventRow> = node
                .responsibilities
                .iter()
                .filter(|r| {
                    (resps.contains(&r.id) || resident_nodes.contains(&host))
                        && before_stmts.get(&r.id) != Some(&r.statement)
                })
                .map(|r| {
                    let marker = if before_stmts.contains_key(&r.id) { "~" } else { "+" };
                    resp_event_row(marker, &after, r)
                })
                .collect();
            if !rows.is_empty() {
                record_event(
                    model_ref,
                    HistoryEvent::new(now, EventKind::Impl, &node.id, "fill")
                        .with_change(cid)
                        .with_rows(rows),
                );
            }
        }
    }

    let planned_after = scryer_core::read_planned_at(model_ref).unwrap_or_default();
    let closed = !planned_after.changes.iter().any(|c| c.id == cid);
    let mut parts: Vec<String> = Vec::new();
    if !resident_nodes.is_empty() {
        parts.push(format!("{} node(s)", resident_nodes.len()));
    }
    if folded_resps > 0 {
        parts.push(format!("{folded_resps} claim(s)"));
    }
    if folded_props > 0 {
        parts.push(format!("{folded_props} propert(ies)"));
    }
    if !groups.is_empty() {
        parts.push(format!("{} group(s)", groups.len()));
    }
    if !links.is_empty() {
        parts.push(format!("{} link(s)", links.len()));
    }
    if !deleted_nodes.is_empty() {
        parts.push(format!("{} deletion(s)", deleted_nodes.len()));
    }
    let withheld_total = withheld_here
        + resps
            .iter()
            .filter(|rid| {
                withhold.contains(*rid)
                    && host_of_resp(rid).is_some_and(|h| resident_nodes.contains(&h))
            })
            .count();
    if withheld_total > 0 {
        parts.push(format!("{withheld_total} claim(s) withheld (see below)"));
    }
    let mut summary = format!(
        "Folded change {cid} (\"{}\"): {}.",
        meta.rationale,
        if parts.is_empty() { "nothing".to_string() } else { parts.join(", ") }
    );
    if closed {
        summary.push_str(
            " The change is fully folded and closed — its rationale is recorded in the \
             history log.",
        );
    } else {
        let left = planned_after.change_map.values().filter(|v| v.as_str() == cid).count();
        summary.push_str(&format!(" {left} entr(ies) still pending on it."));
    }
    Ok(vec![summary])
}

/// The nearest ancestor of `id` that is NOT in `removed` — the surviving home for
/// a descoped node's relocated claims. `None` when the whole parent chain up to
/// the root is being removed (or `id` is top-level), so the claims have nowhere to
/// go and are dropped.
fn surviving_ancestor(model: &ScryModel, id: &str, removed: &HashSet<String>) -> Option<String> {
    let mut cur = model.nodes.iter().find(|n| n.id == id)?.parent_id.clone();
    while let Some(pid) = cur {
        if !removed.contains(&pid) {
            return Some(pid);
        }
        cur = model
            .nodes
            .iter()
            .find(|n| n.id == pid)
            .and_then(|n| n.parent_id.clone());
    }
    None
}

#[tool_router(router = tool_router_nodes, vis = "pub(crate)")]
impl ScryerServer {
    #[tool(
        description = "GENERATION-PIPELINE primitive — replace the ENTIRE model in one write (used during codebase→model generation to seed the system + container skeleton). Writes both the plan and the committed model. The JSON payload must include `version: \"0.3\"`, `nodes`, `links`, and optional `groups` and `sourceMap`. Validation warnings are returned but the write is committed regardless — fix the warnings in a follow-up call. For interactive editing, use the typed add_*/update_*/move_* tools instead."
    )]
    fn set_model(
        &self,
        Parameters(req): Parameters<SetModelRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = resolve_model_ref(req.project.as_deref())?;
        let _lock = match lock_or_err(&model_ref) {
            Ok(l) => l,
            Err(e) => return Ok(e),
        };

        let mut model: ScryModel = match serde_json::from_str(&req.data) {
            Ok(m) => m,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Invalid model JSON: {}",
                    e
                ))]));
            }
        };
        // Caller-invented responsibility ids are re-minted BEFORE directive
        // enforcement, with the outgoing layers as floors: re-issuing an id the
        // payload dropped would staple the dead claim's user directives onto an
        // unrelated new one (see RespIdReminter).
        let prior_committed = scryer_core::read_model_at(&model_ref).unwrap_or_default();
        let prior_planned = scryer_core::read_planned_at(&model_ref).unwrap_or_default();
        let mut reminter = RespIdReminter::for_replacement(&[&prior_committed, &prior_planned]);
        for n in &model.nodes {
            reminter.absorb(n.responsibilities.iter());
        }
        for g in &model.groups {
            reminter.absorb(g.responsibilities.iter());
        }
        for n in &mut model.nodes {
            reminter.remint(&n.id, n.responsibilities.iter_mut());
        }
        for g in &mut model.groups {
            reminter.remint(&g.id, g.responsibilities.iter_mut());
        }
        enforce_readonly_directives(&mut model, &prior_committed);
        restore_node_positions(&mut model, &[&prior_planned, &prior_committed]);
        if model.version != scryer_core::SCRY_VERSION {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "Model version '{}' does not match expected '{}'",
                model.version,
                scryer_core::SCRY_VERSION
            ))]));
        }

        // A full-state set is code→model generation: write the plan, then commit
        // it (planned and model land equal, so the plan diff is empty afterward).
        if let Err(e) = scryer_core::write_planned_at(&model_ref, &model) {
            return Ok(CallToolResult::error(vec![Content::text(e)]));
        }
        if let Err(e) = scryer_core::write_model_at(&model_ref, &model) {
            return Ok(CallToolResult::error(vec![Content::text(e)]));
        }
        let _ = scryer_core::save_baseline_at(&model_ref, &model);

        let warnings = validate::validate(&model);
        let mut msg = format!(
            "Wrote model to {} — {} nodes, {} links, {} groups",
            model_ref,
            model.nodes.len(),
            model.links.len(),
            model.groups.len()
        );
        reminter.report_into(&mut msg);
        if !warnings.is_empty() {
            msg.push_str(&format!("\n\n{} warning(s):", warnings.len()));
            for w in warnings {
                msg.push_str(&format!("\n- {}", w));
            }
        }
        Ok(CallToolResult::success(vec![Content::text(msg)]))
    }

    #[tool(
        description = "Patch one or more existing nodes by id. Only fields present in each item are changed. Pass `responsibilities` or `properties` to replace the whole array (pass an empty array to clear). Code-side mapping (line-precise locations per responsibility, and boundary globs per node) is written separately via `update_source_map`."
    )]
    fn update_nodes(
        &self,
        Parameters(mut req): Parameters<UpdateNodeRequest>,
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

        // Caller-invented responsibility ids ("new", "") never enter the model:
        // re-mint them up front, before the vagrant-preservation check below
        // compares payload ids against the model's (see RespIdReminter).
        let committed_floor = scryer_core::read_model_at(&model_ref).unwrap_or_default();
        let mut reminter = RespIdReminter::new(&[&model, &committed_floor]);
        for u in &req.nodes {
            if let Some(v) = &u.responsibilities {
                reminter.absorb(v.iter());
            }
        }
        for u in &mut req.nodes {
            if let Some(v) = &mut u.responsibilities {
                reminter.remint(&u.node_id, v.iter_mut());
            }
        }

        let mut updated = 0usize;
        let mut preserved_vagrants = 0usize;
        for u in &req.nodes {
            if !model.nodes.iter().any(|n| n.id == u.node_id) {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Node '{}' not found",
                    u.node_id
                ))]));
            }

            // A reparent runs the SAME gate move_nodes enforces (nodes.rs:635):
            // the new parent must exist, satisfy the kind hierarchy, not be
            // external, and not sit inside the moved node's own subtree. Without
            // these, update_nodes is a backdoor that silently orphans a node onto
            // a nonexistent parent, plants an illegal pairing, hangs children off
            // an external node, or loops the parent chain (which then hangs every
            // ancestor walker in core). Validate against the model as it stands
            // (reparents applied earlier in this batch included) BEFORE mutating.
            if let Some(v) = &u.parent_id {
                // Effective kind — an update may re-kind the node in the same call.
                let cur_kind = model
                    .nodes
                    .iter()
                    .find(|n| n.id == u.node_id)
                    .map(|n| n.kind)
                    .expect("existence checked above");
                let kind = match &u.kind {
                    Some(k) => parse_kind(k)?,
                    None => cur_kind,
                };
                let Some(parent) = model.nodes.iter().find(|n| &n.id == v) else {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "New parent '{}' not found",
                        v
                    ))]));
                };
                let valid = matches!(
                    (parent.kind, kind),
                    (Kind::System, Kind::Container)
                        | (Kind::Container, Kind::Component)
                        | (Kind::Component, Kind::Symbol)
                );
                if !valid {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "A {:?} cannot be parented by a {:?}",
                        kind, parent.kind
                    ))]));
                }
                if parent.external == Some(true) {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "External node '{}' cannot have children",
                        v
                    ))]));
                }
                // The new parent cannot be the node itself or inside its subtree.
                // The `seen` set keeps this walk terminating even if the model
                // already holds a malformed chain.
                let mut cur = Some(v.clone());
                let mut seen: std::collections::HashSet<String> =
                    std::collections::HashSet::new();
                while let Some(id) = cur {
                    if id == u.node_id {
                        return Ok(CallToolResult::error(vec![Content::text(format!(
                            "Cannot set parent of '{}' to '{}': that node is inside its own \
                             subtree, so the move would create a cycle",
                            u.node_id, v
                        ))]));
                    }
                    if !seen.insert(id.clone()) {
                        break;
                    }
                    cur = model
                        .nodes
                        .iter()
                        .find(|n| n.id == id)
                        .and_then(|n| n.parent_id.clone());
                }
            }

            // Remembered before the mutable borrow so group eviction below can
            // tell an actual level change from a no-op reparent.
            let old_parent = model
                .nodes
                .iter()
                .find(|n| n.id == u.node_id)
                .and_then(|n| n.parent_id.clone());

            let n = model
                .nodes
                .iter_mut()
                .find(|n| n.id == u.node_id)
                .expect("existence checked above");

            if let Some(v) = &u.kind {
                n.kind = parse_kind(v)?;
            }
            if let Some(v) = &u.name {
                n.name = v.clone();
            }
            // Empty string = CLEAR — without this, description/technology/
            // external could be set but never removed through this tool.
            if let Some(v) = &u.description {
                n.description = if v.is_empty() { None } else { Some(v.clone()) };
            }
            if let Some(v) = &u.technology {
                n.technology = if v.is_empty() { None } else { Some(v.clone()) };
            }
            if let Some(v) = u.external {
                n.external = if v { Some(true) } else { None };
            }
            if let Some(v) = &u.responsibilities {
                // Replacement never bypasses the review queue: a vagrant
                // (code-discovered) claim awaiting the user's adopt/reject
                // verdict survives a replacement that omits it — deleting it
                // silently would resolve the verdict nobody gave.
                let kept: Vec<_> = n
                    .responsibilities
                    .iter()
                    .filter(|r| r.vagrant == Some(true) && !v.iter().any(|nv| nv.id == r.id))
                    .cloned()
                    .collect();
                preserved_vagrants += kept.len();
                n.responsibilities = v.clone();
                n.responsibilities.extend(kept);
            }
            if let Some(v) = &u.properties {
                let kept: Vec<_> = n
                    .properties
                    .iter()
                    .filter(|p| p.vagrant == Some(true) && !v.iter().any(|nv| nv.label == p.label))
                    .cloned()
                    .collect();
                preserved_vagrants += kept.len();
                n.properties = v.clone();
                n.properties.extend(kept);
            }
            if let Some(v) = &u.parent_id {
                if old_parent.as_deref() != Some(v.as_str()) {
                    // Canvas placements are per-surface coordinates — a
                    // reparent lands on a different surface, so the old spot
                    // is meaningless there. Auto-layout re-homes the node.
                    n.position = None;
                }
                n.parent_id = Some(v.clone());
            }

            // Groups organize siblings at one level — a reparent to a new level
            // leaves the group (mirrors move_nodes). `n`'s borrow ends above.
            if let Some(v) = &u.parent_id {
                if old_parent.as_deref() != Some(v.as_str()) {
                    for g in model.groups.iter_mut() {
                        g.member_ids.retain(|m| m != &u.node_id);
                    }
                }
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

        // Accept + warn (never reject — a rejected write invites a duplicate
        // call): field-shape problems on the nodes just touched ride back on
        // the response.
        let mut msg = format!("Updated {} node(s)", updated);
        reminter.report_into(&mut msg);
        for w in &tag_warnings {
            msg.push_str(&format!("\n{w}"));
        }
        if preserved_vagrants > 0 {
            msg.push_str(&format!(
                "\n{preserved_vagrants} vagrant (code-discovered) item(s) not in your \
                 replacement array were KEPT — they await an adopt/reject verdict and \
                 leave only through one."
            ));
        }
        let touched: std::collections::HashSet<&str> =
            req.nodes.iter().map(|u| u.node_id.as_str()).collect();
        let warnings: Vec<String> = model
            .nodes
            .iter()
            .filter(|n| touched.contains(n.id.as_str()))
            .flat_map(scryer_core::validate::node_field_warnings)
            .collect();
        for w in &warnings {
            msg.push_str(&format!("\nwarning: {}", w));
        }

        drop(_lock);
        if let Some(h) = status_header(&model_ref) {
            msg.push_str(&format!("\n{h}"));
        }
        Ok(CallToolResult::success(vec![Content::text(msg)]))
    }

    #[tool(
        description = "Replace the directives on nodes or responsibilities — the ONE write path to directives, which every other tool leaves read-only. Directives are the USER's prescriptive HOW-constraints: call this ONLY when the user has explicitly asked, in this conversation, for directives to be written, edited, or deleted (e.g. a bulk reword they dictated) — never on your own initiative, and never to relax a constraint you find inconvenient while implementing. Each item names a `node_id` (node-level directives, binding that node's whole subtree) OR a `responsibility_id` (that claim's directives), plus `directives` as the FULL replacement array — an empty array clears. Writes the plan layer like other authoring tools; the change surfaces in the plan diff for the user to see."
    )]
    fn set_directives(
        &self,
        Parameters(req): Parameters<SetDirectivesRequest>,
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

        let mut updated = 0usize;
        for item in &req.items {
            match (&item.node_id, &item.responsibility_id) {
                (Some(node_id), None) => {
                    let Some(n) = model.nodes.iter_mut().find(|n| &n.id == node_id) else {
                        return Ok(CallToolResult::error(vec![Content::text(format!(
                            "Node '{}' not found",
                            node_id
                        ))]));
                    };
                    n.directives = item.directives.clone();
                }
                (None, Some(resp_id)) => {
                    let resp = model
                        .nodes
                        .iter_mut()
                        .flat_map(|n| n.responsibilities.iter_mut())
                        .chain(model.groups.iter_mut().flat_map(|g| g.responsibilities.iter_mut()))
                        .find(|r| &r.id == resp_id);
                    let Some(r) = resp else {
                        return Ok(CallToolResult::error(vec![Content::text(format!(
                            "Responsibility '{}' not found",
                            resp_id
                        ))]));
                    };
                    r.directives = item.directives.clone();
                }
                _ => {
                    return Ok(CallToolResult::error(vec![Content::text(
                        "Each item must set exactly one of `node_id` or `responsibility_id`"
                            .to_string(),
                    )]));
                }
            }
            updated += 1;
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
        let mut msg = format!("Set directives on {} target(s)", updated);
        for w in &tag_warnings {
            msg.push_str(&format!("\n{w}"));
        }
        if let Some(h) = status_header(&model_ref) {
            msg.push_str(&format!("\n{h}"));
        }
        Ok(CallToolResult::success(vec![Content::text(msg)]))
    }

    #[tool(
        description = "Fold a node's outstanding planned work into the committed model after you've written the code — the counterpart to `get_pending`, which closes the loop. This is THE build checkpoint, and it is one atomic statement with three parts: the fold ('I built this'), `anchors` ('here is where it lives'), and `tests` ('here is the test I attached to it') — pass all three in the SAME call rather than folding now and anchoring/attaching later. Folding overwrites the committed claim with the clean planned copy, clearing the `stale` drift flag on anything it folds (re-implementation is the verdict that resolves it). With no `responsibilityIds`, folds every planned responsibility and property on the node, EXCEPT vagrant (code-discovered) claims and properties, which are left in the plan awaiting an explicit adopt/reject verdict and never bypass into the committed model. Pass `responsibilityIds` to fold only those responsibilities, and/or `propertyLabels` to fold only those data fields (properties are identified by label). A whole-node fold also pulls in the plan links touching this node once BOTH their endpoints are committed, and any group this node completes (every member committed). Standalone link/group changes — and EVERY link/group DELETION, which never rides a node fold — fold by their own ids instead: pass `link_ids` / `group_ids`, with or without a `node_id`. In a DESIGN-FIRST model (never committed), folding a built leaf is refused while its ancestors are plan-only — pass `commit_ancestors: true` to fold the ancestor chain structure-only first: the ancestors' identity and boundaries land in committed while their unbuilt claims stay pending in the plan, so partial implementation reads honestly. Call this when you finish implementing, so the plan clears and the model stops reporting the work as outstanding. Pass `anchors` (same shape as update_source_map `entries`) to anchor the folded claims to code IN THE SAME CALL — 'here's what I built and where it lives' as one atomic statement; an unanchored claim reads as scaffolding and carries no drift tripwire. Pass `tests` (same shape) to ATTACH each claim's test alongside — 'and this test exercises it' (`pattern` = test file, `symbol` = the test function). For a claim in a When/While/If form the test is EXPECTED, not opportunistic — and on a symbol host it is MANDATORY (rule 22): the claim names a concrete trigger/state/failure, so write the test that arranges it and asserts the response, and attach it here in the same call. THE FOLD IS GATED ON EVIDENCE: a testable claim on a code-backed host folds only with a test attached AND a current passing verdict (report ingested after the last edit to the implementation and the test); otherwise that claim STAYS IN THE PLAN and the response names the missing fact (no test / no verdict / stale / failing) and the test files to run — the order is write test → attach → run with a JUnit reporter → ingest_test_report → fold. Leaving a claim pending is an honest exit, not a failure; the rest of the fold proceeds. `force: true` folds anyway and records an `unverified` history event (never the default). Ubiquitous claims stay a judgment call and are not gated. SIGN-OFF: claims you reworded or added after the developer signed off their change land as vagrant (origin amendment/addition) for the developer's verdict; they do not fold. If implementing shows a planned claim is wrong, reword it and fold the rest — the reword waits. Every node fold's response ends with a scoped POST-FLIGHT: what's still pending on that node, which of its committed claims lack anchors, and any validation warnings this fold introduced — act on those lines; you do not need a separate validate_model run after every fold. If you DELETED a node in the plan (intending the code to go away) and have now removed that code, call this with the node id to fold the deletion into the committed model. Pass `change` (standalone — a change id from `set_change`/`get_pending`) to fold an ENTIRE change: every plan entry tagged to it, in dependency order; when its last entry folds, the change closes and its rationale is recorded in the history log. NOTE: this is for code you actually changed — to drop something from the model WITHOUT touching code, use `descope` instead."
    )]
    fn mark_implemented(
        &self,
        Parameters(mut req): Parameters<MarkImplementedRequest>,
    ) -> Result<CallToolResult, McpError> {
        use scryer_core::diff::ElementKind;
        let model_ref = resolve_model_ref(req.project.as_deref())?;
        let _lock = match lock_or_err(&model_ref) {
            Ok(l) => l,
            Err(e) => return Ok(e),
        };

        let empty = |v: &Option<Vec<String>>| v.as_ref().map_or(true, |x| x.is_empty());
        if req.change.is_some()
            && (req.node_id.is_some()
                || !empty(&req.link_ids)
                || !empty(&req.group_ids)
                || req.responsibility_ids.is_some()
                || req.property_labels.is_some())
        {
            return Ok(CallToolResult::error(vec![Content::text(
                "`change` folds an entire change and stands alone — don't combine it with \
                 node_id / responsibility_ids / property_labels / link_ids / group_ids."
                    .to_string(),
            )]));
        }
        if req.change.is_none()
            && req.node_id.is_none()
            && empty(&req.link_ids)
            && empty(&req.group_ids)
        {
            return Ok(CallToolResult::error(vec![Content::text(
                "Nothing to fold — provide node_id (optionally with responsibility_ids), \
                 link_ids / group_ids, or a `change` id to fold that whole change."
                    .to_string(),
            )]));
        }
        if (req.responsibility_ids.is_some() || req.property_labels.is_some())
            && req.node_id.is_none()
        {
            return Ok(CallToolResult::error(vec![Content::text(
                "responsibility_ids / property_labels require node_id (the host whose \
                 claims you are folding)."
                    .to_string(),
            )]));
        }

        // The plan (draft) is the source of the work being closed out: marking
        // implemented FOLDS the named elements from `planned` into the committed
        // `model` via the auto-commit fold. The element must exist in the plan.
        // Seeded read: the fold rewrites the draft, so a never-seeded project
        // must not start from the anchor-carrying committed fallback.
        let planned = match scryer_core::read_planned_seeded_at(&model_ref) {
            Ok(p) => p,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(read_fail("plan", &model_ref, &e))]));
            }
        };

        // Snapshot the node's committed responsibilities so the history event can
        // show exactly what THIS fold added or reworded, not claims committed in a
        // prior pass (a whole-node commit re-folds the node's full planned state).
        let committed_before = scryer_core::read_model_at(&model_ref).ok();
        let before_stmts: HashMap<String, String> = committed_before
            .as_ref()
            .map(|m| {
                m.nodes
                    .iter()
                    .flat_map(|n| n.responsibilities.iter())
                    .map(|r| (r.id.clone(), r.statement.clone()))
                    .collect()
            })
            .unwrap_or_default();
        // Pre-fold structural warnings on the working view — the post-flight
        // reports only what THIS call introduced, not the model's standing debt.
        let warnings_before: std::collections::HashSet<String> = {
            let empty = scryer_core::ScryModel::default();
            scryer_core::validate::validate(&scryer_core::working_view(
                committed_before.as_ref().unwrap_or(&empty),
                &planned,
            ))
            .into_iter()
            .collect()
        };

        // Fold-time anchors and attached tests are validated BEFORE any fold
        // runs, so a bad id fails the whole call instead of leaving a fold
        // half-anchored.
        if req.anchors.is_some() || req.tests.is_some() {
            let known: std::collections::HashSet<&str> = planned
                .nodes
                .iter()
                .flat_map(|n| n.responsibilities.iter())
                .chain(planned.groups.iter().flat_map(|g| g.responsibilities.iter()))
                .map(|r| r.id.as_str())
                .chain(before_stmts.keys().map(|k| k.as_str()))
                .collect();
            for (field, entries) in [("anchors", &req.anchors), ("tests", &req.tests)]
            {
                for e in entries.iter().flatten() {
                    if !known.contains(e.responsibility_id.as_str()) {
                        return Ok(CallToolResult::error(vec![Content::text(format!(
                            "{}: responsibility '{}' not found in the plan or the \
                             committed model — nothing was folded. Anchor and test \
                             entries are keyed by responsibility id (not node id, not \
                             statement text); `read_model` on the host node lists its \
                             claims with their ids — retry with the right id",
                            field, e.responsibility_id
                        ))]));
                    }
                }
            }
        }

        // ---- The gates: sign-off (forward vagrancy) and evidence. ------------
        // Decide, BEFORE anything folds, which of the claims this call is about
        // to commit must stay in the plan: post-sign-off amendments/additions
        // (flagged vagrant for the developer's verdict) and testable claims
        // without a current passing verdict (refused with the missing fact).
        // The fold engine honours the withhold set; everything else proceeds.
        let force = req.force == Some(true);
        let now = scryer_core::drift::now_secs();
        let tests_in_call: HashMap<String, Vec<String>> = req
            .tests
            .iter()
            .flatten()
            .map(|e| {
                let mut files: Vec<String> =
                    e.locations.iter().map(|l| l.pattern.clone()).collect();
                files.sort();
                files.dedup();
                (e.responsibility_id.clone(), files)
            })
            .collect();
        let committed_now = committed_before.clone().unwrap_or_default();
        let candidates: Vec<String> = if let Some(cid) = req.change.as_deref() {
            use scryer_core::changes as ledger;
            use scryer_core::diff::ElementKind as EK;
            let mut ids: Vec<String> = Vec::new();
            for (k, v) in &planned.change_map {
                if v != cid {
                    continue;
                }
                match ledger::parse_key(k) {
                    Some((EK::Responsibility, _, id)) => ids.push(id),
                    Some((EK::Node, _, id)) => {
                        ids.extend(fold_gate::pending_claims_on(&committed_now, &planned, &id))
                    }
                    _ => {}
                }
            }
            ids.sort();
            ids.dedup();
            ids
        } else if let Some(node_id) = req.node_id.as_deref() {
            match req.responsibility_ids.as_ref() {
                Some(ids) => ids.clone(),
                None if req.property_labels.is_some() => Vec::new(),
                None => fold_gate::pending_claims_on(&committed_now, &planned, node_id),
            }
        } else {
            Vec::new()
        };
        let mut planned_gated = planned.clone();
        let gate = match fold_gate::gate(
            &model_ref,
            &mut planned_gated,
            &candidates,
            &tests_in_call,
            force,
            now,
        ) {
            Ok(g) => g,
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(e)])),
        };
        let planned = if gate.plan_dirty {
            if let Err(e) = scryer_core::write_planned_at(&model_ref, &planned_gated) {
                return Ok(CallToolResult::error(vec![Content::text(e)]));
            }
            planned_gated
        } else {
            planned
        };
        let folded_ids: Vec<String> = candidates
            .iter()
            .filter(|id| !gate.withhold.contains(*id))
            .cloned()
            .collect();

        let mut summaries: Vec<String> = Vec::new();
        // Set when a node-hosted fold ran — drives the history event below.
        let mut history_node: Option<String> = None;

        if let Some(cid) = req.change.as_deref() {
            // "Implement THIS change": expand the ledger's tags into element
            // folds. History (per-host impl events with the change id) is
            // recorded inside; the change closes via the fold-path GC when its
            // last entry goes.
            match fold_change_by_id(
                &model_ref,
                &planned,
                &before_stmts,
                cid,
                req.commit_ancestors == Some(true),
                &gate.withhold,
            ) {
                Ok(s) => summaries.extend(s),
                Err(e) => return Ok(e),
            }
        }

        if let Some(node_id) = req.node_id.as_deref() {
            if !planned.nodes.iter().any(|n| n.id == node_id) {
                // Gone from the plan but still in committed = a planned DELETION to
                // fold: you removed the code, now remove the claim from the committed
                // model. `commit_element` deletes a committed node whose planned copy
                // is absent.
                let in_committed = scryer_core::read_model_at(&model_ref)
                    .map(|m| m.nodes.iter().any(|n| n.id == node_id))
                    .unwrap_or(false);
                if !in_committed {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "Node '{}' not found in the plan",
                        node_id
                    ))]));
                }
                if let Err(e) =
                    scryer_core::commit_element(&model_ref, ElementKind::Node, None, node_id)
                {
                    return Ok(CallToolResult::error(vec![Content::text(e)]));
                }
                summaries.push(format!(
                    "Committed the removal of '{}' from the model.",
                    node_id
                ));
            } else {
                if req.commit_ancestors == Some(true) {
                    // Design-first escape: fold the plan-only ancestor chain
                    // structure-only FIRST, so the fold below lands under committed
                    // parents without marking the ancestors' unbuilt claims
                    // implemented. A scoped fold needs the HOST itself committed
                    // too, so it rides the structure cascade as well.
                    let include_self =
                        req.responsibility_ids.is_some() || req.property_labels.is_some();
                    match scryer_core::commit_plan_only_ancestors(
                        &model_ref,
                        node_id,
                        include_self,
                    ) {
                        Err(e) => return Ok(CallToolResult::error(vec![Content::text(e)])),
                        Ok(folded) if folded.is_empty() => {}
                        Ok(folded) => {
                            let scoped: std::collections::HashSet<&str> = req
                                .responsibility_ids
                                .iter()
                                .flatten()
                                .map(|s| s.as_str())
                                .collect();
                            let pending: usize = planned
                                .nodes
                                .iter()
                                .filter(|n| folded.contains(&n.id))
                                .map(|n| {
                                    n.responsibilities
                                        .iter()
                                        .filter(|r| !scoped.contains(r.id.as_str()))
                                        .count()
                                        + n.properties.len()
                                })
                                .sum();
                            summaries.push(format!(
                                "Committed {} plan-only node(s) structure-only as \
                                 scaffolding ({}); {} unbuilt claim(s) on them stay \
                                 pending in the plan.",
                                folded.len(),
                                folded.join(", "),
                                pending
                            ));
                        }
                    }
                }
                let scoped =
                    req.responsibility_ids.is_some() || req.property_labels.is_some();
                match scoped {
                    // Scoped: commit exactly the named responsibilities and/or
                    // property labels. Their host node must already be committed
                    // (commit the whole node first otherwise).
                    true => {
                        let ids: Vec<String> = req
                            .responsibility_ids
                            .iter()
                            .flatten()
                            .filter(|id| !gate.withhold.contains(*id))
                            .cloned()
                            .collect();
                        let ids = &ids[..];
                        for id in ids {
                            if let Err(e) = scryer_core::commit_element(
                                &model_ref,
                                ElementKind::Responsibility,
                                None,
                                id,
                            ) {
                                return Ok(CallToolResult::error(vec![Content::text(
                                    ancestor_hint(e),
                                )]));
                            }
                        }
                        // Properties are identified by (host node, label) — the
                        // partial-fold path responsibilities always had.
                        let labels = req.property_labels.as_deref().unwrap_or(&[]);
                        for label in labels {
                            if let Err(e) = scryer_core::commit_element(
                                &model_ref,
                                ElementKind::Property,
                                Some(node_id),
                                label,
                            ) {
                                return Ok(CallToolResult::error(vec![Content::text(
                                    ancestor_hint(e),
                                )]));
                            }
                        }
                        let mut parts: Vec<String> = Vec::new();
                        if !ids.is_empty() {
                            parts.push(format!(
                                "{} responsibilit{}",
                                ids.len(),
                                if ids.len() == 1 { "y" } else { "ies" }
                            ));
                        }
                        if !labels.is_empty() {
                            parts.push(format!(
                                "{} propert{}",
                                labels.len(),
                                if labels.len() == 1 { "y" } else { "ies" }
                            ));
                        }
                        if parts.is_empty() {
                            summaries.push(format!(
                                "Nothing folded on '{}' — every named claim was withheld (see \
                                 below).",
                                node_id
                            ));
                        } else {
                            summaries.push(format!(
                                "Committed {} on '{}' into the model.",
                                parts.join(" and "),
                                node_id
                            ));
                        }
                    }
                    // Whole node: commit the node, folding its whole planned state
                    // (responsibilities, properties) into the model.
                    false => {
                        if let Err(e) = scryer_core::commit_element_withholding(
                            &model_ref,
                            ElementKind::Node,
                            None,
                            node_id,
                            &gate.withhold,
                        ) {
                            return Ok(CallToolResult::error(vec![Content::text(
                                ancestor_hint(e),
                            )]));
                        }
                        // Pull in the ready plan-added links/groups incident to this
                        // node (item A); deletions never ride along — they fold by
                        // their own ids below.
                        if let Err(e) =
                            scryer_core::commit_ready_dependents(&model_ref, node_id)
                        {
                            return Ok(CallToolResult::error(vec![Content::text(e)]));
                        }
                        summaries.push(format!("Committed '{}' into the model.", node_id));
                    }
                }
                history_node = Some(node_id.to_string());
            }
        }

        // Links and groups fold by THEIR ids — the only path for a standalone
        // change and for EVERY link/group deletion (a node fold pulls in only the
        // ready adds incident to it, never removals).
        if !empty(&req.link_ids) || !empty(&req.group_ids) {
            // Fresh reads: the node fold above may have advanced both layers.
            let committed = scryer_core::read_model_at(&model_ref).unwrap_or_default();
            let planned_now = match scryer_core::read_planned_at(&model_ref) {
                Ok(p) => p,
                Err(e) => {
                    return Ok(CallToolResult::error(vec![Content::text(read_fail("plan", &model_ref, &e))]));
                }
            };
            if let Some(ids) = req.link_ids.as_ref().filter(|v| !v.is_empty()) {
                let mut removals = 0usize;
                for id in ids {
                    let in_plan = planned_now.links.iter().any(|l| &l.id == id);
                    if !in_plan && !committed.links.iter().any(|l| &l.id == id) {
                        return Ok(CallToolResult::error(vec![Content::text(format!(
                            "Link '{}' not found in the plan or the committed model",
                            id
                        ))]));
                    }
                    if !in_plan {
                        removals += 1;
                    }
                    if let Err(e) =
                        scryer_core::commit_element(&model_ref, ElementKind::Link, None, id)
                    {
                        return Ok(CallToolResult::error(vec![Content::text(e)]));
                    }
                }
                summaries.push(fold_summary("link", ids.len(), removals));
            }
            if let Some(ids) = req.group_ids.as_ref().filter(|v| !v.is_empty()) {
                let mut removals = 0usize;
                for id in ids {
                    let in_plan = planned_now.groups.iter().any(|g| &g.id == id);
                    if !in_plan && !committed.groups.iter().any(|g| &g.id == id) {
                        return Ok(CallToolResult::error(vec![Content::text(format!(
                            "Group '{}' not found in the plan or the committed model",
                            id
                        ))]));
                    }
                    if !in_plan {
                        removals += 1;
                    }
                    if let Err(e) =
                        scryer_core::commit_element(&model_ref, ElementKind::Group, None, id)
                    {
                        return Ok(CallToolResult::error(vec![Content::text(e)]));
                    }
                }
                summaries.push(fold_summary("group", ids.len(), removals));
            }
        }

        // Fold-time anchors and attached tests, applied AFTER the folds so each
        // claim's entry lands in its single home (a just-committed claim's
        // anchor belongs to the committed layer, not a shadow copy in the
        // draft). One read-modify-write covers both dimensions.
        let mut anchor_notes: Vec<String> = Vec::new();
        let anchors = req.anchors.take().filter(|v| !v.is_empty());
        let tests = req.tests.take().filter(|v| !v.is_empty());
        if anchors.is_some() || tests.is_some() {
            let mut planned_now = match scryer_core::read_planned_seeded_at(&model_ref) {
                Ok(p) => p,
                Err(e) => return Ok(CallToolResult::error(vec![Content::text(e)])),
            };
            let mut committed_now = scryer_core::read_model_at(&model_ref).ok();
            let mut committed_dirty = false;
            if let Some(entries) = anchors {
                let n = entries.len();
                let (normalized, dirty) = apply_resp_anchor_entries(
                    model_ref.project_path(),
                    &mut planned_now,
                    &mut committed_now,
                    entries,
                    RespAnchorDim::Source,
                );
                committed_dirty |= dirty;
                summaries.push(format!("Anchored {} claim(s).", n));
                anchor_notes = normalized;
            }
            if let Some(entries) = tests {
                let n = entries.len();
                let (normalized, dirty) = apply_resp_anchor_entries(
                    model_ref.project_path(),
                    &mut planned_now,
                    &mut committed_now,
                    entries,
                    RespAnchorDim::Test,
                );
                committed_dirty |= dirty;
                summaries.push(format!("Recorded attached test(s) for {} claim(s).", n));
                anchor_notes.extend(normalized);
            }
            if let Err(e) = scryer_core::write_planned_at(&model_ref, &planned_now) {
                return Ok(CallToolResult::error(vec![Content::text(e)]));
            }
            if committed_dirty {
                if let Some(c) = committed_now {
                    if let Err(e) = scryer_core::write_model_at(&model_ref, &c) {
                        return Ok(CallToolResult::error(vec![Content::text(e)]));
                    }
                }
            }
        }

        // Fingerprint baseline for what this fold landed: the folded claims'
        // implementation and test keys, plus whatever the call anchored or
        // attached. Scoped to those keys — a full rewrite would silently
        // re-baseline (and swallow) unreconciled drift elsewhere. A freshly
        // folded claim must never be silent (`silentAnchors`).
        {
            let mut keys = fold_gate::baseline_keys(&folded_ids);
            for e in req.anchors.iter().flatten() {
                keys.insert(e.responsibility_id.clone());
            }
            for e in req.tests.iter().flatten() {
                keys.insert(scryer_core::test_key(&e.responsibility_id));
            }
            if let Err(e) = scryer_extract::anchors::write_baseline_for(&model_ref, &keys) {
                summaries.push(format!("(baseline not refreshed: {e})"));
            }
        }
        // The refusal ledger: record what stayed behind and why, clear what
        // folded, and drop entries for claims no longer in the plan.
        {
            let _ = scryer_core::refusals::update_refusals(
                &model_ref,
                &gate.refusals,
                &folded_ids,
            );
            if let Ok(p) = scryer_core::read_planned_at(&model_ref) {
                let live: HashSet<String> = p
                    .nodes
                    .iter()
                    .flat_map(|n| n.responsibilities.iter())
                    .chain(p.groups.iter().flat_map(|g| g.responsibilities.iter()))
                    .map(|r| r.id.clone())
                    .collect();
                scryer_core::refusals::prune_refusals(&model_ref, &live);
            }
        }
        // A forced bypass of the evidence gate is visible in the log: one
        // `unverified` event per host naming the claims that folded unproven.
        if !gate.forced.is_empty() {
            if let Ok(after) = scryer_core::read_model_at(&model_ref) {
                let mut by_host: std::collections::BTreeMap<String, Vec<EventRow>> =
                    Default::default();
                for id in &gate.forced {
                    let Some((host, r)) = after
                        .nodes
                        .iter()
                        .flat_map(|n| n.responsibilities.iter().map(move |r| (n.id.as_str(), r)))
                        .find(|(_, r)| &r.id == id)
                    else {
                        continue;
                    };
                    by_host
                        .entry(host.to_string())
                        .or_default()
                        .push(resp_event_row("!", &after, r));
                }
                for (host, rows) in by_host {
                    record_event(
                        &model_ref,
                        HistoryEvent::new(now, EventKind::Impl, &host, "unverified")
                            .with_rows(rows),
                    );
                }
            }
        }

        let summary = summaries.join(" ");

        // Keep the legacy baseline snapshot in step with the committed model, and
        // record the fold as an `impl` event listing the claims it discharged.
        if let Ok(after) = scryer_core::read_model_at(&model_ref) {
            let _ = scryer_core::save_baseline_at(&model_ref, &after);
            if let Some(node) = history_node
                .as_deref()
                .and_then(|id| after.nodes.iter().find(|n| n.id == id))
            {
                let target: Vec<&scryer_core::Responsibility> = match &req.responsibility_ids {
                    Some(ids) => node.responsibilities.iter().filter(|r| ids.contains(&r.id)).collect(),
                    // Whole-node: only the responsibilities this fold newly added or
                    // reworded relative to the committed snapshot above.
                    None => node
                        .responsibilities
                        .iter()
                        .filter(|r| before_stmts.get(&r.id) != Some(&r.statement))
                        .collect(),
                };
                let rows: Vec<EventRow> = target
                    .iter()
                    .map(|r| {
                        let marker = if before_stmts.contains_key(&r.id) { "~" } else { "+" };
                        resp_event_row(marker, &after, r)
                    })
                    .collect();
                if !rows.is_empty() {
                    // Ledger attribution: if the folded work was tagged, the
                    // impl event carries its change id — "which change
                    // introduced this claim?" stays answerable after the fold.
                    let cid = {
                        use scryer_core::changes as ledger;
                        use scryer_core::diff::ElementKind as EK;
                        planned
                            .change_map
                            .get(&ledger::element_key(EK::Node, None, &node.id))
                            .or_else(|| {
                                target.iter().find_map(|r| {
                                    planned.change_map.get(&ledger::element_key(
                                        EK::Responsibility,
                                        None,
                                        &r.id,
                                    ))
                                })
                            })
                            .cloned()
                    };
                    let mut ev = HistoryEvent::new(
                        scryer_core::drift::now_secs(),
                        EventKind::Impl,
                        &node.id,
                        "fill",
                    )
                    .with_rows(rows);
                    if let Some(c) = cid {
                        ev = ev.with_change(c);
                    }
                    record_event(&model_ref, ev);
                }
            }
        }

        // Scoped post-flight: what this fold left behind on the node — the
        // consistency burden lives here, not in the agent's memory of which
        // follow-up tools it was supposed to call.
        let mut msg = summary;
        if !gate.lines.is_empty() {
            msg.push_str("\ngate:");
            for l in &gate.lines {
                msg.push_str(&format!("\n- {l}"));
            }
        }
        for n in &anchor_notes {
            msg.push_str(&format!(
                "\nnormalized: {} — the range covered the whole symbol, so the \
                 symbol-only anchor was kept (a range must be a proper subset)",
                n
            ));
        }
        if let Some(node_id) = req.node_id.as_deref() {
            let committed = scryer_core::read_model_at(&model_ref).unwrap_or_default();
            let planned_after =
                scryer_core::read_planned_at(&model_ref).unwrap_or_default();
            let mut lines: Vec<String> = Vec::new();

            // Tests-attached callout — the FIRST post-flight line, because it is
            // the primary concern (rule 22): testable committed claims on this
            // node with no test attached. Same gate as health's `testable`
            // counter: person/external hosts never expect tests. Guidance, not a
            // gate — the fold has already succeeded.
            if let Some(node) = committed.nodes.iter().find(|n| n.id == node_id) {
                if node.external != Some(true) && node.kind != scryer_core::Kind::Person {
                    let has_test = |key: &str| {
                        committed.test_map.get(key).is_some_and(|l| !l.is_empty())
                            || planned_after.test_map.get(key).is_some_and(|l| !l.is_empty())
                    };
                    let untested: Vec<&str> = node
                        .responsibilities
                        .iter()
                        .filter(|r| {
                            scryer_core::ears::classify(&r.statement).testable()
                                && !has_test(&r.id)
                        })
                        .map(|r| r.id.as_str())
                        .collect();
                    if !untested.is_empty() {
                        let strength = if node.kind == scryer_core::Kind::Symbol {
                            "MANDATORY on a symbol host (rule 22)"
                        } else {
                            "expected (rule 22)"
                        };
                        lines.push(format!(
                            "NO TEST ATTACHED to {} testable claim(s) on it ({}) — a test is {}. \
                             Each statement already names the trigger/state/failure to arrange \
                             and the response to assert: write that test in the project's suite, \
                             then attach it via update_source_map `test_entries` (or `tests` on \
                             your next mark_implemented) with `pattern` = test file, `symbol` = \
                             the test function",
                            untested.len(),
                            untested.join(", "),
                            strength
                        ));
                    }
                }
            }

            // Still pending on this node (vagrants excluded — they are drift
            // review, not the implement queue; matches get_pending).
            use scryer_core::diff::{Change, ElementKind as EK};
            let plan = scryer_core::diff::diff(&committed, &planned_after);
            let is_vagrant = |ch: &scryer_core::diff::ElementChange| match ch.kind {
                EK::Node => planned_after
                    .nodes
                    .iter()
                    .any(|n| n.id == ch.id && n.vagrant == Some(true)),
                EK::Responsibility => planned_after
                    .nodes
                    .iter()
                    .flat_map(|n| n.responsibilities.iter())
                    .chain(
                        planned_after
                            .groups
                            .iter()
                            .flat_map(|g| g.responsibilities.iter()),
                    )
                    .any(|r| r.id == ch.id && r.vagrant == Some(true)),
                EK::Property => ch.owner_id.as_deref().is_some_and(|oid| {
                    planned_after.nodes.iter().any(|n| {
                        n.id == oid
                            && n.properties
                                .iter()
                                .any(|p| p.label == ch.id && p.vagrant == Some(true))
                    })
                }),
                _ => false,
            };
            let link_touches = |id: &str| {
                planned_after
                    .links
                    .iter()
                    .chain(committed.links.iter())
                    .any(|l| l.id == id && (l.src == node_id || l.dst == node_id))
            };
            let pending = plan
                .changes
                .iter()
                .filter(|ch| {
                    let scoped = match ch.kind {
                        EK::Node => ch.id == node_id,
                        EK::Responsibility | EK::Property => {
                            ch.owner_id.as_deref() == Some(node_id)
                        }
                        EK::Link => link_touches(&ch.id),
                        EK::Group => false,
                    };
                    scoped && !is_vagrant(ch)
                })
                .count();
            if pending > 0 {
                let deletions = plan
                    .changes
                    .iter()
                    .filter(|ch| {
                        (ch.id == node_id
                            || ch.owner_id.as_deref() == Some(node_id)
                            || (ch.kind == EK::Link && link_touches(&ch.id)))
                            && ch.changes.contains(&Change::Deleted)
                    })
                    .count();
                let hint = if deletions > 0 {
                    " (deletions fold only by explicit ids — pass link_ids / the node id)"
                } else {
                    ""
                };
                lines.push(format!(
                    "{pending} change(s) touching this node still pending in the plan{hint} — get_pending lists them"
                ));
            }

            // Unanchored committed claims — LEAF nodes only: a structural node's
            // claims discharge through its subtree and never anchor directly.
            if let Some(node) = committed.nodes.iter().find(|n| n.id == node_id) {
                let has_children = committed
                    .nodes
                    .iter()
                    .chain(planned_after.nodes.iter())
                    .any(|n| n.parent_id.as_deref() == Some(node_id));
                if !has_children {
                    let anchored = |key: &str| {
                        committed.source_map.contains_key(key)
                            || planned_after.source_map.contains_key(key)
                    };
                    let mut unanchored: Vec<&str> = node
                        .responsibilities
                        .iter()
                        .filter(|r| !anchored(&r.id))
                        .map(|r| r.id.as_str())
                        .collect();
                    if !node.properties.is_empty() && !anchored(&node.id) {
                        unanchored.push("the schema declaration");
                    }
                    if !unanchored.is_empty() {
                        lines.push(format!(
                            "{} committed claim(s) on it have NO code anchor ({}) — they read \
                             as scaffolding and carry no drift tripwire; anchor them via the \
                             `anchors` param or update_source_map",
                            unanchored.len(),
                            unanchored.join(", ")
                        ));
                    }
                }
            }

            // Validation warnings this call introduced (standing debt stays out).
            let new_warnings: Vec<String> = scryer_core::validate::validate(
                &scryer_core::working_view(&committed, &planned_after),
            )
            .into_iter()
            .filter(|w| !warnings_before.contains(w))
            .collect();
            for w in new_warnings.iter().take(3) {
                lines.push(format!("new warning: {}", w));
            }
            if new_warnings.len() > 3 {
                lines.push(format!(
                    "…and {} more new warning(s) — validate_model lists them",
                    new_warnings.len() - 3
                ));
            }

            if lines.is_empty() {
                msg.push_str(&format!(
                    "\npost-flight '{node_id}': plan clear on this node, no new warnings."
                ));
            } else {
                msg.push_str(&format!("\npost-flight '{node_id}':"));
                for l in &lines {
                    msg.push_str(&format!("\n- {}", l));
                }
            }
        }

        drop(_lock);
        if let Some(h) = status_header(&model_ref) {
            msg.push_str(&format!("\n{h}"));
        }
        Ok(CallToolResult::success(vec![Content::text(msg)]))
    }

    #[tool(
        description = "Re-parent nodes — move a node (and its whole subtree) to a different parent, e.g. a component to another container. Validated: the new parent must satisfy the kind hierarchy (system→container→component→symbol; omit newParentId only for top-level systems/persons), must not be external, and must not sit inside the moved node's own subtree. The node leaves any group at its old level (groups organize siblings). Links to former siblings may become invalid — run `validate_model` after structural moves."
    )]
    fn move_nodes(
        &self,
        Parameters(req): Parameters<MoveNodesRequest>,
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

        let mut moved = 0usize;
        for mv in &req.moves {
            let Some(node) = model.nodes.iter().find(|n| n.id == mv.node_id) else {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Node '{}' not found",
                    mv.node_id
                ))]));
            };
            let kind = node.kind;

            match mv.new_parent_id.as_deref() {
                None => {
                    if !matches!(kind, Kind::System | Kind::Person) {
                        return Ok(CallToolResult::error(vec![Content::text(format!(
                            "Node '{}' ({:?}) cannot be top-level — only person/system are",
                            mv.node_id, kind
                        ))]));
                    }
                }
                Some(pid) => {
                    let Some(parent) = model.nodes.iter().find(|n| n.id == pid) else {
                        return Ok(CallToolResult::error(vec![Content::text(format!(
                            "New parent '{}' not found",
                            pid
                        ))]));
                    };
                    let valid = matches!(
                        (parent.kind, kind),
                        (Kind::System, Kind::Container)
                            | (Kind::Container, Kind::Component)
                            | (Kind::Component, Kind::Symbol)
                    );
                    if !valid {
                        return Ok(CallToolResult::error(vec![Content::text(format!(
                            "A {:?} cannot be parented by a {:?}",
                            kind, parent.kind
                        ))]));
                    }
                    if parent.external == Some(true) {
                        return Ok(CallToolResult::error(vec![Content::text(format!(
                            "External node '{}' cannot have children",
                            pid
                        ))]));
                    }
                    // The new parent must not be the node itself or inside its
                    // own subtree (that would orphan the chain into a cycle).
                    let mut cur = Some(pid.to_string());
                    let mut seen: std::collections::HashSet<String> =
                        std::collections::HashSet::new();
                    while let Some(id) = cur {
                        if id == mv.node_id {
                            return Ok(CallToolResult::error(vec![Content::text(format!(
                                "Cannot move '{}' under its own subtree",
                                mv.node_id
                            ))]));
                        }
                        if !seen.insert(id.clone()) {
                            break; // cycle guard — a pre-existing parent loop never hangs the walk
                        }
                        cur = model
                            .nodes
                            .iter()
                            .find(|n| n.id == id)
                            .and_then(|n| n.parent_id.clone());
                    }
                }
            }

            let node = model.nodes.iter_mut().find(|n| n.id == mv.node_id).unwrap();
            if node.parent_id != mv.new_parent_id {
                // Placements are per-surface — the old coordinates mean
                // nothing on the new parent's map. Auto-layout re-homes it.
                node.position = None;
            }
            node.parent_id = mv.new_parent_id.clone();
            // Groups organize siblings at one level — leaving the level leaves
            // the group.
            for g in model.groups.iter_mut() {
                g.member_ids.retain(|m| m != &mv.node_id);
            }
            moved += 1;
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

        // Timeline: a `move` event per node that actually changed parent.
        let name_of = |m: &ScryModel, id: &str| {
            m.nodes.iter().find(|n| n.id == id).map(|n| n.name.clone()).unwrap_or_else(|| id.to_string())
        };
        let now = scryer_core::drift::now_secs();
        for mv in &req.moves {
            let old_parent = prior.nodes.iter().find(|n| n.id == mv.node_id).and_then(|n| n.parent_id.clone());
            if old_parent == mv.new_parent_id {
                continue;
            }
            let from = old_parent.as_deref().map(|p| name_of(&prior, p)).unwrap_or_else(|| "top level".into());
            let to = mv.new_parent_id.as_deref().map(|p| name_of(&model, p)).unwrap_or_else(|| "top level".into());
            record_event(
                &model_ref,
                HistoryEvent::new(now, EventKind::Move, &mv.node_id, "reorganize")
                    .with_rows(vec![EventRow::new("→", format!("reparented {from} → {to}"))]),
            );
        }

        let warnings = validate::validate(&model);
        let mut msg = format!("Moved {moved} node(s).");
        for w in &tag_warnings {
            msg.push_str(&format!("\n{w}"));
        }
        if !warnings.is_empty() {
            msg.push_str(&format!(
                " {} validation warning(s) — run validate_model:",
                warnings.len()
            ));
            for w in warnings.iter().take(5) {
                msg.push_str(&format!("\n- {}", w));
            }
        }
        drop(_lock);
        if let Some(h) = status_header(&model_ref) {
            msg.push_str(&format!("\n{h}"));
        }
        Ok(CallToolResult::success(vec![Content::text(msg)]))
    }

    #[tool(
        description = "GENERATION-PIPELINE primitive — replace a node's whole subtree in one write (used during codebase→model generation to attach a container's structure to the seeded skeleton). Writes BOTH the plan and the committed model: the subtree describes code that already exists, so it lands as built, not as a pending \"implement this whole subtree\" queue in the plan diff. The data payload is JSON `{ \"nodes\": [...], \"links\": [...] }` where every node in `nodes` has a parent chain rooted at `node_id`. All existing descendants of `node_id` are removed before the new subtree is inserted. Links replace any link whose endpoints are inside the subtree; links connecting to nodes outside the subtree are also accepted. For interactive editing, use the typed add_*/update_*/move_* tools instead."
    )]
    fn set_node(
        &self,
        Parameters(req): Parameters<SetNodeRequest>,
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

        if !model.nodes.iter().any(|n| n.id == req.node_id) {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "Node '{}' not found",
                req.node_id
            ))]));
        }
        let prior = model.clone();

        #[derive(serde::Deserialize)]
        struct SubtreePayload {
            nodes: Vec<Node>,
            #[serde(default)]
            links: Vec<Link>,
        }
        let mut payload: SubtreePayload = match serde_json::from_str(&req.data) {
            Ok(p) => p,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Invalid subtree JSON: {}",
                    e
                ))]));
            }
        };

        // Caller-invented responsibility ids are re-minted ONCE, before the
        // subtree is applied — both layers then receive identical ids (see
        // RespIdReminter).
        let committed_floor = scryer_core::read_model_at(&model_ref).unwrap_or_default();
        let mut reminter = RespIdReminter::for_replacement(&[&model, &committed_floor]);
        for n in &payload.nodes {
            reminter.absorb(n.responsibilities.iter());
        }
        for n in &mut payload.nodes {
            reminter.remint(&n.id, n.responsibilities.iter_mut());
        }

        // Node ids get the same guard. This write owns exactly the subtree it
        // replaces; a payload id naming a node ANYWHERE else is a stale
        // snapshot's collision, and pushing it in would leave two nodes sharing
        // an id (see remint_colliding_node_ids). Runs against both layers, and
        // before the replacement, so the ids that land are already unique.
        let replaced: std::collections::HashSet<String> =
            subtree_ids(&model, &req.node_id);
        let node_remints = remint_colliding_node_ids(
            &mut payload.nodes,
            &mut payload.links,
            &replaced,
            &[&model, &committed_floor],
        );

        // Apply the subtree replacement to the plan. The dropped-link report
        // comes from the plan layer — always applied (node existence checked
        // above) and the authoritative surface the caller edits.
        let (_, dropped_links) = replace_subtree(&mut model, &req.node_id, &payload.nodes, &payload.links);
        enforce_readonly_directives(&mut model, &prior);
        restore_node_positions(&mut model, &[&prior]);

        // Generation reverse-engineers code that ALREADY EXISTS, so the same
        // subtree must land in the committed model too (mirroring set_model /
        // fill_container) — otherwise the plan diff reports the whole built
        // subtree as `added` work forever. Only when `node_id` is committed (the
        // generation skeleton); if it lives only in the plan this stays a
        // plan-only edit, and there is nothing to commit. Prepared before the
        // plan write so a `None` here just means "plan-only".
        let committed = scryer_core::read_model_at(&model_ref).ok().and_then(|mut c| {
            let cprior = c.clone();
            let (applied, _) = replace_subtree(&mut c, &req.node_id, &payload.nodes, &payload.links);
            applied.then(|| {
                enforce_readonly_directives(&mut c, &cprior);
                // Plan-first prior order: the plan layer is where the canvas
                // writes placements, so the committed copy inherits the same
                // positions the plan write above just restored.
                restore_node_positions(&mut c, &[&prior, &cprior]);
                c
            })
        });

        // Write the plan first: if the committed write then fails, committed lags
        // the plan (recoverable pending work), never leads it (a phantom deletion).
        if let Err(e) = scryer_core::write_planned_at(&model_ref, &model) {
            return Ok(CallToolResult::error(vec![Content::text(e)]));
        }
        if let Some(committed) = committed {
            if let Err(e) = scryer_core::write_model_at(&model_ref, &committed) {
                return Ok(CallToolResult::error(vec![Content::text(e)]));
            }
            let _ = scryer_core::save_baseline_at(&model_ref, &committed);
        }

        let warnings = validate::validate(&model);
        let mut msg = format!("Replaced subtree under {}", req.node_id);
        reminter.report_into(&mut msg);
        if !node_remints.is_empty() {
            msg.push_str(&format!(
                "\n{} caller-supplied node id(s) re-minted (ids are server-assigned; \
                 use the new ids from here on):",
                node_remints.len()
            ));
            for line in &node_remints {
                msg.push_str(&format!("\n- {line}"));
            }
        }
        if !dropped_links.is_empty() {
            msg.push_str(&format!(
                "\n\nDropped {} link(s) with endpoints absent from the subtree:",
                dropped_links.len()
            ));
            for d in &dropped_links {
                msg.push_str(&format!("\n- {d}"));
            }
        }
        if !warnings.is_empty() {
            msg.push_str(&format!("\n\n{} warning(s):", warnings.len()));
            for w in warnings {
                msg.push_str(&format!("\n- {}", w));
            }
        }
        Ok(CallToolResult::success(vec![Content::text(msg)]))
    }

    #[tool(
        description = "Delete one or more nodes because the CODE they model should go away — a forward modeling intent. Each node's descendants, connected links, and source-map entries are also removed. This stages real removal work in the plan: it shows up as pending until you delete the code and call `mark_implemented`. If instead the code is fine and you just shouldn't be MODELING it (an entry-point `main`, boilerplate), use `descope` — that's a code-untouched, model-only correction."
    )]
    fn delete_nodes(
        &self,
        Parameters(req): Parameters<DeleteNodeRequest>,
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

        // Expand to include all descendants
        let mut to_remove: HashSet<String> = req.node_ids.iter().cloned().collect();
        let mut frontier: Vec<String> = req.node_ids.clone();
        while let Some(id) = frontier.pop() {
            for child in model.nodes.iter().filter(|n| n.parent_id.as_deref() == Some(&id)) {
                if to_remove.insert(child.id.clone()) {
                    frontier.push(child.id.clone());
                }
            }
        }

        let before = model.nodes.len();
        prune_code_map(&mut model, &to_remove);
        model.nodes.retain(|n| !to_remove.contains(&n.id));
        model
            .links
            .retain(|l| !to_remove.contains(&l.src) && !to_remove.contains(&l.dst));
        // Prune dead group memberships
        for g in model.groups.iter_mut() {
            g.member_ids.retain(|m| !to_remove.contains(m));
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
        let mut msg = format!(
            "Deleted {} node(s) (including descendants) — staged in the plan; the code removal \
             is outstanding work until you delete it and mark_implemented each node id",
            before - model.nodes.len()
        );
        for w in &tag_warnings {
            msg.push_str(&format!("\n{w}"));
        }
        if let Some(h) = status_header(&model_ref) {
            msg.push_str(&format!("\n{h}"));
        }
        Ok(CallToolResult::success(vec![Content::text(msg)]))
    }

    #[tool(
        description = "Descope nodes: remove them from the MODEL because they shouldn't be modeled — the CODE is fine and stays untouched (e.g. an entry-point `main`, a trivial wrapper, generated boilerplate). Each target's own responsibilities relocate up to its parent component, keeping their source anchors, so the parent still covers that code and no darkness appears; the node and its descendants are then removed. This is a model-only correction — it writes BOTH the plan and the committed model at once, so there is NO code work to do and it never shows up in the pending work queue. Reach for this when the model over-claims relative to code reality. To instead remove the CODE itself, use `delete_nodes`, which stages real removal work."
    )]
    fn descope(
        &self,
        Parameters(req): Parameters<DescopeRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = resolve_model_ref(req.project.as_deref())?;
        let _lock = match lock_or_err(&model_ref) {
            Ok(l) => l,
            Err(e) => return Ok(e),
        };

        let mut planned = match scryer_core::read_planned_seeded_at(&model_ref) {
            Ok(m) => m,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(read_fail("plan", &model_ref, &e))]));
            }
        };
        let mut committed = match scryer_core::read_model_at(&model_ref) {
            Ok(m) => m,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(read_fail("model", &model_ref, &e))]));
            }
        };

        // Every target must exist in at least one layer.
        for id in &req.node_ids {
            let present = planned.nodes.iter().any(|n| &n.id == id)
                || committed.nodes.iter().any(|n| &n.id == id);
            if !present {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Node '{}' not found",
                    id
                ))]));
            }
        }

        // Fold the targets out of both layers identically — descope is true in both.
        // Report whichever layer actually changed: when the plan was already folded
        // (e.g. the canvas removed them first), committed is where the work lands, so
        // take the max rather than letting a clean plan report a misleading 0.
        let (rp, remp, dp) = fold_out_layer(&mut planned, &req.node_ids);
        let (rc, remc, dc) = fold_out_layer(&mut committed, &req.node_ids);
        let (relocated, removed, dropped) = (rp.max(rc), remp.max(remc), dp.max(dc));

        if let Err(e) = scryer_core::write_planned_at(&model_ref, &planned) {
            return Ok(CallToolResult::error(vec![Content::text(e)]));
        }
        if let Err(e) = scryer_core::write_model_at(&model_ref, &committed) {
            return Ok(CallToolResult::error(vec![Content::text(e)]));
        }
        let _ = scryer_core::save_baseline_at(&model_ref, &committed);

        let mut msg = format!(
            "Descoped {} node(s) from the model — code untouched. Relocated {} responsibilit{} to parent component(s).",
            removed,
            relocated,
            if relocated == 1 { "y" } else { "ies" }
        );
        if dropped > 0 {
            msg.push_str(&format!(
                " Note: {} responsibilit{} on removed descendants were dropped.",
                dropped,
                if dropped == 1 { "y" } else { "ies" }
            ));
        }
        drop(_lock);
        if let Some(h) = status_header(&model_ref) {
            msg.push_str(&format!("\n{h}"));
        }
        Ok(CallToolResult::success(vec![Content::text(msg)]))
    }

    #[tool(
        description = "Move responsibilities between nodes. A responsibility keeps its id and is reparented onto the destination node; the plan diff records the move (shown as `moved`). Vagrant responsibilities cannot be moved."
    )]
    fn move_responsibilities(
        &self,
        Parameters(req): Parameters<MoveResponsibilitiesRequest>,
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

        let mut moved = 0usize;
        // (destination node id, row text) for the timeline `move` events below.
        let mut reloc_rows: Vec<(String, String)> = Vec::new();
        for mv in &req.moves {
            let resp = {
                let from_node = model.nodes.iter().find(|n| n.id == mv.from_node_id);
                let Some(from_node) = from_node else {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "Source node '{}' not found", mv.from_node_id
                    ))]));
                };
                let Some(r) = from_node.responsibilities.iter().find(|r| r.id == mv.responsibility_id) else {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "Responsibility '{}' not found on node '{}'", mv.responsibility_id, mv.from_node_id
                    ))]));
                };
                if r.vagrant == Some(true) {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "Vagrant responsibility '{}' cannot be moved", mv.responsibility_id
                    ))]));
                }
                r.clone()
            };

            if !model.nodes.iter().any(|n| n.id == mv.to_node_id) {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Destination node '{}' not found", mv.to_node_id
                ))]));
            }

            // Plain reparent: keep the same id so the plan diff matches the
            // claim by id and renders the move as `moved` (R). No ghost/locked
            // copy at the source — the diff is the record of the relocation.
            let statement = resp.statement.clone();
            let from_name = model
                .nodes
                .iter()
                .find(|n| n.id == mv.from_node_id)
                .map(|n| n.name.clone())
                .unwrap_or_else(|| mv.from_node_id.clone());
            let from = model.nodes.iter_mut().find(|n| n.id == mv.from_node_id).unwrap();
            from.responsibilities.retain(|r| r.id != mv.responsibility_id);
            let to = model.nodes.iter_mut().find(|n| n.id == mv.to_node_id).unwrap();
            to.responsibilities.push(resp);
            reloc_rows.push((
                mv.to_node_id.clone(),
                format!("relocated “{}” from {}", statement, from_name),
            ));
            moved += 1;
        }

        let tag_warnings = match write_planned_tagged(
            &model_ref,
            &mut model,
            self.session_change(&model_ref).as_deref(),
        ) {
            Ok(w) => w,
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(e)])),
        };

        // Timeline: a `move` event on each destination node.
        let now = scryer_core::drift::now_secs();
        for (to_node_id, text) in reloc_rows {
            record_event(
                &model_ref,
                HistoryEvent::new(now, EventKind::Move, &to_node_id, "reorganize")
                    .with_rows(vec![EventRow::new("→", text)]),
            );
        }

        drop(_lock);
        let mut msg = format!("Moved {} responsibility(ies)", moved);
        for w in &tag_warnings {
            msg.push_str(&format!("\n{w}"));
        }
        if let Some(h) = status_header(&model_ref) {
            msg.push_str(&format!("\n{h}"));
        }
        Ok(CallToolResult::success(vec![Content::text(msg)]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scryer_core::{ModelRef, Responsibility};

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
            notes: None,
            position: None,
            directives: Vec::new(),
        }
    }

    fn resp(id: &str) -> Responsibility {
        Responsibility {
            concern: None,
            id: id.into(),
            statement: format!("does {id}"),
            vagrant: None,
            stale: None,
            stale_proposal: None,
            directives: Vec::new(),
            last_touched_at: None,
        }
    }

    /// A descoped node's own claim relocates to the nearest SURVIVING ancestor,
    /// not onto a parent that is itself being removed (which would report a
    /// "relocation" that is really a deletion). Here the container and its
    /// component are descoped together; the component's claim bubbles past the
    /// doomed container to the surviving system.
    #[test]
    fn fold_out_layer_relocates_past_a_removed_ancestor() {
        let mut m = ScryModel::new();
        m.nodes.push(node("sys", Kind::System, "S", None));
        m.nodes.push(node("con", Kind::Container, "C", Some("sys")));
        let mut comp = node("comp", Kind::Component, "Cmp", Some("con"));
        comp.responsibilities = vec![resp("r-comp")];
        m.nodes.push(comp);

        let (relocated, removed, dropped) =
            fold_out_layer(&mut m, &["con".into(), "comp".into()]);
        assert_eq!(relocated, 1, "the claim relocates to the surviving system");
        assert_eq!(dropped, 0, "nothing is lost");
        assert_eq!(removed, 2);
        let sys = m.nodes.iter().find(|n| n.id == "sys").unwrap();
        assert!(
            sys.responsibilities.iter().any(|r| r.id == "r-comp"),
            "the claim actually landed on the survivor"
        );
    }

    /// A top-level target has no ancestor to catch its claims, so they are DROPPED
    /// — and the count must reflect that, never report the loss as zero.
    #[test]
    fn fold_out_layer_counts_a_top_level_targets_dropped_claims() {
        let mut m = ScryModel::new();
        let mut sys = node("sys", Kind::System, "S", None);
        sys.responsibilities = vec![resp("r-sys")];
        m.nodes.push(sys);

        let (relocated, removed, dropped) = fold_out_layer(&mut m, &["sys".into()]);
        assert_eq!(relocated, 0);
        assert_eq!(dropped, 1, "the top-level claim is dropped AND counted");
        assert_eq!(removed, 1);
    }

    /// Descope: a symbol that shouldn't be modeled is removed from BOTH layers, its
    /// responsibility relocates to the parent with its source anchor intact (so the
    /// file stays lit and no darkness appears), and the code is never consulted.
    #[test]
    fn descope_relocates_responsibility_and_writes_both_layers() {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());

        let mut m = ScryModel::new();
        m.nodes.push(node("node-1", Kind::Component, "Harness", None));
        let mut main = node("node-2", Kind::Symbol, "main", Some("node-1"));
        main.responsibilities = vec![resp("r-main")];
        m.nodes.push(main);
        m.source_map.insert(
            "r-main".into(),
            vec![scryer_core::SourceLocation {
                pattern: "examples/bench.rs".into(),
                symbol: Some("main".into()),
                line: None,
                end_line: None,
            }],
        );
        scryer_core::write_model_at(&model_ref, &m).unwrap();
        scryer_core::write_planned_at(&model_ref, &m).unwrap(); // plan mirrors committed

        let server = ScryerServer::new();
        server
            .descope(Parameters(DescopeRequest {
                project: Some(dir.path().to_string_lossy().to_string()),
                node_ids: vec!["node-2".into()],
            }))
            .unwrap();

        // Both layers must show the same model-only correction.
        let committed = scryer_core::read_model_at(&model_ref).unwrap();
        let planned = scryer_core::read_planned_at(&model_ref).unwrap();
        for layer in [&committed, &planned] {
            assert!(layer.nodes.iter().all(|n| n.id != "node-2"), "main removed");
            let parent = layer.nodes.iter().find(|n| n.id == "node-1").unwrap();
            assert!(
                parent.responsibilities.iter().any(|r| r.id == "r-main"),
                "responsibility relocated to parent"
            );
        }
        assert_eq!(
            committed.source_map.get("r-main").unwrap()[0].pattern,
            "examples/bench.rs",
            "source anchor preserved — file stays lit"
        );
        // Single home: the draft this test wrote mirrored committed's anchor
        // (the pre-seeding shadow state), and the seeded read heals it — the
        // anchor lives in committed only, and the working view still surfaces it.
        assert!(!planned.source_map.contains_key("r-main"), "no shadow copy in the draft");
        assert!(
            scryer_core::working_view(&committed, &planned).source_map.contains_key("r-main"),
            "the working view still lights the file"
        );
    }

    /// set_directives is the ONE deliberate write path to directives (every
    /// other tool restores them via enforce_readonly_directives). It replaces
    /// the full array on a node (node-level), a node-hosted claim, and a
    /// group-hosted claim — writing the PLAN layer only, like the other
    /// authoring tools, so the edit surfaces in the plan diff.
    #[test]
    fn set_directives_replaces_on_node_and_responsibility_hosts() {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());

        let mut m = ScryModel::new();
        let mut sys = node("sys", Kind::System, "S", None);
        sys.directives = vec!["old node rule".into()];
        let mut con = node("con", Kind::Container, "C", Some("sys"));
        con.responsibilities = vec![resp("r-con")];
        m.nodes.push(sys);
        m.nodes.push(con);
        m.groups.push(scryer_core::Group {
            id: "grp".into(),
            name: "G".into(),
            description: None,
            member_ids: Vec::new(),
            parent_group_id: None,
            parent_node_id: Some("sys".into()),
            responsibilities: vec![resp("r-grp")],
            icon: None,
        });
        scryer_core::write_model_at(&model_ref, &m).unwrap();
        scryer_core::write_planned_at(&model_ref, &m).unwrap();

        let server = ScryerServer::new();
        let item = |n: Option<&str>, r: Option<&str>, d: &[&str]| SetDirectivesItem {
            node_id: n.map(Into::into),
            responsibility_id: r.map(Into::into),
            directives: d.iter().map(|s| s.to_string()).collect(),
        };
        let res = server
            .set_directives(Parameters(SetDirectivesRequest {
                project: Some(dir.path().to_string_lossy().to_string()),
                items: vec![
                    item(Some("sys"), None, &["must stay stateless"]),
                    item(None, Some("r-con"), &["never trust client input"]),
                    item(None, Some("r-grp"), &["must audit-log"]),
                ],
            }))
            .unwrap();
        assert_ne!(res.is_error, Some(true), "{:?}", res.content);

        let planned = scryer_core::read_planned_at(&model_ref).unwrap();
        let sys = planned.nodes.iter().find(|n| n.id == "sys").unwrap();
        assert_eq!(sys.directives, vec!["must stay stateless"], "node-level replaced");
        let con = planned.nodes.iter().find(|n| n.id == "con").unwrap();
        assert_eq!(con.responsibilities[0].directives, vec!["never trust client input"]);
        assert_eq!(planned.groups[0].responsibilities[0].directives, vec!["must audit-log"]);

        // The committed model is untouched — the edit is plan work like any
        // other authoring write, visible in the plan diff until folded.
        let committed = scryer_core::read_model_at(&model_ref).unwrap();
        let sys_c = committed.nodes.iter().find(|n| n.id == "sys").unwrap();
        assert_eq!(sys_c.directives, vec!["old node rule"], "committed layer untouched");
    }

    /// An empty replacement array CLEARS — without it directives could be set
    /// but never removed through this tool.
    #[test]
    fn set_directives_clears_with_an_empty_array() {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());

        let mut m = ScryModel::new();
        let mut sys = node("sys", Kind::System, "S", None);
        let mut r = resp("r-sys");
        r.directives = vec!["must do things".into()];
        sys.responsibilities = vec![r];
        sys.directives = vec!["a node rule".into()];
        m.nodes.push(sys);
        scryer_core::write_model_at(&model_ref, &m).unwrap();
        scryer_core::write_planned_at(&model_ref, &m).unwrap();

        let server = ScryerServer::new();
        server
            .set_directives(Parameters(SetDirectivesRequest {
                project: Some(dir.path().to_string_lossy().to_string()),
                items: vec![
                    SetDirectivesItem {
                        node_id: Some("sys".into()),
                        responsibility_id: None,
                        directives: Vec::new(),
                    },
                    SetDirectivesItem {
                        node_id: None,
                        responsibility_id: Some("r-sys".into()),
                        directives: Vec::new(),
                    },
                ],
            }))
            .unwrap();

        let planned = scryer_core::read_planned_at(&model_ref).unwrap();
        let sys = planned.nodes.iter().find(|n| n.id == "sys").unwrap();
        assert!(sys.directives.is_empty(), "node-level cleared");
        assert!(sys.responsibilities[0].directives.is_empty(), "claim-level cleared");
    }

    /// An unknown id — or an item that names both / neither target — is
    /// rejected before anything lands, leaving the model untouched.
    #[test]
    fn set_directives_rejects_bad_items_without_writing() {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());

        let mut m = ScryModel::new();
        let mut sys = node("sys", Kind::System, "S", None);
        sys.directives = vec!["keep me".into()];
        m.nodes.push(sys);
        scryer_core::write_model_at(&model_ref, &m).unwrap();
        scryer_core::write_planned_at(&model_ref, &m).unwrap();

        let server = ScryerServer::new();
        let bad_items = [
            SetDirectivesItem {
                node_id: Some("nope".into()),
                responsibility_id: None,
                directives: vec!["x".into()],
            },
            SetDirectivesItem {
                node_id: None,
                responsibility_id: Some("nope".into()),
                directives: vec!["x".into()],
            },
            SetDirectivesItem {
                node_id: Some("sys".into()),
                responsibility_id: Some("r-x".into()),
                directives: vec!["x".into()],
            },
            SetDirectivesItem { node_id: None, responsibility_id: None, directives: vec!["x".into()] },
        ];
        for bad in bad_items {
            // A valid edit batched BEHIND the bad item must not land either.
            let res = server
                .set_directives(Parameters(SetDirectivesRequest {
                    project: Some(dir.path().to_string_lossy().to_string()),
                    items: vec![
                        bad,
                        SetDirectivesItem {
                            node_id: Some("sys".into()),
                            responsibility_id: None,
                            directives: vec!["should not land".into()],
                        },
                    ],
                }))
                .unwrap();
            assert_eq!(res.is_error, Some(true));
            let planned = scryer_core::read_planned_at(&model_ref).unwrap();
            assert_eq!(
                planned.nodes[0].directives,
                vec!["keep me"],
                "model untouched after a rejected batch"
            );
        }
    }

    /// Canvas placements are user-authored, like directives: a whole-model
    /// regeneration must carry each surviving node's position over from the
    /// prior model (plan layer first — that's where the canvas writes), and an
    /// agent-authored position on a NEW node never enters the model (auto-layout
    /// places new nodes).
    #[test]
    fn set_model_restores_user_positions_and_strips_agent_ones() {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());

        let mut m = ScryModel::new();
        let mut placed = node("sys", Kind::System, "S", None);
        placed.position = Some(scryer_core::Position { x: 120.0, y: -40.0 });
        m.nodes.push(placed);
        scryer_core::write_model_at(&model_ref, &ScryModel::new()).unwrap();
        scryer_core::write_planned_at(&model_ref, &m).unwrap(); // placement lives in the plan only

        let server = ScryerServer::new();
        let data = r#"{"version":"0.3","nodes":[
            {"id":"sys","kind":"system","name":"S"},
            {"id":"sys2","kind":"system","name":"S2","position":{"x":1.0,"y":2.0}}
        ],"links":[]}"#;
        server
            .set_model(Parameters(SetModelRequest {
                project: Some(dir.path().to_string_lossy().to_string()),
                data: data.into(),
            }))
            .unwrap();

        let planned = scryer_core::read_planned_at(&model_ref).unwrap();
        let sys = planned.nodes.iter().find(|n| n.id == "sys").unwrap();
        assert_eq!(
            sys.position,
            Some(scryer_core::Position { x: 120.0, y: -40.0 }),
            "the user's plan-layer placement survives the regeneration"
        );
        let sys2 = planned.nodes.iter().find(|n| n.id == "sys2").unwrap();
        assert_eq!(sys2.position, None, "an agent-invented position never lands");
    }

    /// A reparent drops the node's canvas placement: coordinates are relative to
    /// the parent's surface, so the old spot is meaningless on the new one —
    /// auto-layout re-homes the node. A same-parent "move" keeps it.
    #[test]
    fn move_nodes_clears_position_only_on_a_real_reparent() {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());

        let mut m = ScryModel::new();
        m.nodes.push(node("sys-a", Kind::System, "A", None));
        m.nodes.push(node("sys-b", Kind::System, "B", None));
        let mut placed = node("con", Kind::Container, "C", Some("sys-a"));
        placed.position = Some(scryer_core::Position { x: 300.0, y: 180.0 });
        let mut kept = node("con2", Kind::Container, "C2", Some("sys-a"));
        kept.position = Some(scryer_core::Position { x: 0.0, y: 0.0 });
        m.nodes.push(placed);
        m.nodes.push(kept);
        scryer_core::write_model_at(&model_ref, &m).unwrap();
        scryer_core::write_planned_at(&model_ref, &m).unwrap();

        let server = ScryerServer::new();
        server
            .move_nodes(Parameters(MoveNodesRequest {
                project: Some(dir.path().to_string_lossy().to_string()),
                moves: vec![
                    NodeMove { node_id: "con".into(), new_parent_id: Some("sys-b".into()) },
                    NodeMove { node_id: "con2".into(), new_parent_id: Some("sys-a".into()) },
                ],
            }))
            .unwrap();

        let planned = scryer_core::read_planned_at(&model_ref).unwrap();
        let moved = planned.nodes.iter().find(|n| n.id == "con").unwrap();
        assert_eq!(moved.parent_id.as_deref(), Some("sys-b"));
        assert_eq!(moved.position, None, "reparent lands on a new surface — placement cleared");
        let stayed = planned.nodes.iter().find(|n| n.id == "con2").unwrap();
        assert!(stayed.position.is_some(), "same-parent move keeps the placement");
    }

    /// set_node is a generation primitive describing code that ALREADY exists, so
    /// the subtree it attaches must land in BOTH layers — otherwise `get_pending`
    /// reports the whole built subtree as `added` work forever (the phantom
    /// queue). After the write, committed == planned, so the plan diff is empty.
    #[test]
    fn set_node_commits_the_generated_subtree_to_both_layers() {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());

        // Generation skeleton: a system, mirrored into both layers (as set_model
        // leaves them).
        let mut m = ScryModel::new();
        m.nodes.push(node("node-1", Kind::System, "Acme", None));
        scryer_core::write_model_at(&model_ref, &m).unwrap();
        scryer_core::write_planned_at(&model_ref, &m).unwrap();

        // Attach a container under the system.
        let payload = serde_json::json!({
            "nodes": [
                { "id": "node-2", "kind": "container", "name": "API", "parentId": "node-1",
                  "responsibilities": [{ "id": "resp-1", "statement": "serves requests" }] }
            ],
            "links": []
        });
        let server = ScryerServer::new();
        server
            .set_node(Parameters(SetNodeRequest {
                project: Some(dir.path().to_string_lossy().to_string()),
                node_id: "node-1".into(),
                data: payload.to_string(),
            }))
            .unwrap();

        let committed = scryer_core::read_model_at(&model_ref).unwrap();
        let planned = scryer_core::read_planned_at(&model_ref).unwrap();
        assert!(
            committed.nodes.iter().any(|n| n.id == "node-2"),
            "the generated container lands in the committed model, not just the plan"
        );
        assert!(planned.nodes.iter().any(|n| n.id == "node-2"), "and in the plan");
        assert!(
            scryer_core::diff::diff(&committed, &planned).is_empty(),
            "committed == planned, so the plan diff is empty — no phantom subtree queue"
        );
    }

    /// A payload link whose endpoint is not in the resulting subtree is dropped
    /// (it would dangle) — but never silently: set_node reports each drop with a
    /// reason so a mis-keyed link is visible, not lost.
    #[test]
    fn set_node_reports_dropped_links_with_unknown_endpoints() {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());

        let mut m = ScryModel::new();
        m.nodes.push(node("node-1", Kind::System, "Acme", None));
        scryer_core::write_model_at(&model_ref, &m).unwrap();
        scryer_core::write_planned_at(&model_ref, &m).unwrap();

        // Two components; one link joins them (kept), one points at a node that
        // does not exist (dropped and reported).
        let payload = serde_json::json!({
            "nodes": [
                { "id": "node-2", "kind": "container", "name": "API", "parentId": "node-1" },
                { "id": "node-3", "kind": "container", "name": "DB", "parentId": "node-1" }
            ],
            "links": [
                { "id": "l-ok", "src": "node-2", "dst": "node-3", "label": "queries" },
                { "id": "l-bad", "src": "node-2", "dst": "ghost", "label": "calls" }
            ]
        });
        let server = ScryerServer::new();
        let result = server
            .set_node(Parameters(SetNodeRequest {
                project: Some(dir.path().to_string_lossy().to_string()),
                node_id: "node-1".into(),
                data: payload.to_string(),
            }))
            .unwrap();

        let text = result.content.iter().find_map(|c| c.as_text().map(|t| t.text.clone())).unwrap();
        assert!(text.contains("Dropped 1 link"), "reports the drop: {text}");
        assert!(text.contains("node-2 -> ghost (unknown dst)"), "names the bad link: {text}");

        let planned = scryer_core::read_planned_at(&model_ref).unwrap();
        assert!(planned.links.iter().any(|l| l.id == "l-ok"), "the valid link is kept");
        assert!(!planned.links.iter().any(|l| l.id == "l-bad"), "the dangling link is dropped");
    }

    /// mark_implemented folds a planned DELETION: a node removed from the plan (its
    /// code now gone) is dropped from the committed model.
    #[test]
    fn mark_implemented_folds_a_planned_deletion() {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());

        let mut m = ScryModel::new();
        m.nodes.push(node("node-1", Kind::Component, "Root", None));
        m.nodes.push(node("node-2", Kind::Symbol, "gone", Some("node-1")));
        scryer_core::write_model_at(&model_ref, &m).unwrap();
        // Plan no longer has node-2 (the agent deleted it, then removed the code).
        let mut planned = m.clone();
        planned.nodes.retain(|n| n.id != "node-2");
        scryer_core::write_planned_at(&model_ref, &planned).unwrap();

        let server = ScryerServer::new();
        let r = server
            .mark_implemented(Parameters(MarkImplementedRequest {
                project: Some(dir.path().to_string_lossy().to_string()),
                node_id: Some("node-2".into()),
                link_ids: None,
                group_ids: None,
                responsibility_ids: None,
                property_labels: None,
                commit_ancestors: None,
                force: None,
                anchors: None,
                tests: None,
                change: None,
            }))
            .unwrap();
        assert!(serde_json::to_string(&r.content).unwrap().contains("removal"));

        let committed = scryer_core::read_model_at(&model_ref).unwrap();
        assert!(
            committed.nodes.iter().all(|n| n.id != "node-2"),
            "deletion folded into the committed model"
        );
    }

    /// Whole-node: marking implemented folds the node's entire planned state
    /// (responsibilities + properties) into the committed model, and the plan
    /// for that node clears.
    #[test]
    fn mark_implemented_commits_whole_node_from_plan() {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());

        // Committed model: the node exists but is empty.
        let mut m = ScryModel::new();
        m.nodes.push(node("node-1", Kind::Component, "ModelTree", None));
        scryer_core::write_model_at(&model_ref, &m).unwrap();

        // Plan (draft): the node gains responsibilities.
        let mut planned = m.clone();
        planned.nodes[0].responsibilities =
            vec![resp("r-a"), resp("r-b")];
        scryer_core::write_planned_at(&model_ref, &planned).unwrap();

        let server = ScryerServer::new();
        server
            .mark_implemented(Parameters(MarkImplementedRequest {
                project: Some(dir.path().to_string_lossy().to_string()),
                node_id: Some("node-1".into()),
                link_ids: None,
                group_ids: None,
                responsibility_ids: None,
                property_labels: None,
                commit_ancestors: None,
                force: None,
                anchors: None,
                tests: None,
                change: None,
            }))
            .unwrap();

        let m = scryer_core::read_model_at(&model_ref).unwrap();
        let n = &m.nodes[0];
        assert_eq!(n.responsibilities.len(), 2, "responsibilities committed");
        assert!(
            scryer_core::plan_diff_at(&model_ref).unwrap().is_empty(),
            "plan clears after commit"
        );

        // The fold is recorded as an `impl` event listing both folded claims.
        let log = scryer_core::history::read_history(&model_ref);
        assert_eq!(log.len(), 1, "one impl event");
        assert_eq!(log[0].kind, scryer_core::history::EventKind::Impl);
        assert_eq!(log[0].node_id, "node-1");
        assert_eq!(log[0].rows.len(), 2, "both newly-folded claims listed");
    }

    /// Design-first close: without `commit_ancestors` a built leaf's fold is
    /// refused (and the refusal names the flag); with it, the plan-only chain
    /// lands in committed structure-only — ancestors carry no claims, the leaf
    /// folds whole, and the ancestors' unbuilt claims stay pending in the plan.
    #[test]
    fn mark_implemented_commit_ancestors_folds_scaffolding() {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());
        scryer_core::write_model_at(&model_ref, &ScryModel::new()).unwrap();

        let mut planned = ScryModel::new();
        let mut sys = node("sys", Kind::System, "System", None);
        sys.responsibilities = vec![resp("r-sys")];
        let mut app = node("app", Kind::Container, "App", Some("sys"));
        app.responsibilities = vec![resp("r-app")];
        let mut leaf = node("leaf", Kind::Component, "Feature", Some("app"));
        leaf.responsibilities = vec![resp("r-leaf")];
        planned.nodes.extend([sys, app, leaf]);
        scryer_core::write_planned_at(&model_ref, &planned).unwrap();

        let server = ScryerServer::new();
        let call = |commit_ancestors: Option<bool>| {
            server
                .mark_implemented(Parameters(MarkImplementedRequest {
                    project: Some(dir.path().to_string_lossy().to_string()),
                    node_id: Some("leaf".into()),
                    responsibility_ids: None,
                    property_labels: None,
                    commit_ancestors,
                    force: None,
                    link_ids: None,
                    group_ids: None,
                    anchors: None,
                    tests: None,
                    change: None,
                }))
                .unwrap()
        };

        // Without the flag: refused, and the refusal steers at the flag.
        let refused = call(None);
        assert!(refused.is_error.unwrap_or(false), "plan-only parent refused");
        let text = refused
            .content
            .iter()
            .find_map(|c| c.as_text().map(|t| t.text.clone()))
            .unwrap();
        assert!(text.contains("commit_ancestors"), "refusal names the recovery: {text}");

        // With the flag: scaffolding chain + leaf all land, honestly.
        let ok = call(Some(true));
        assert!(!ok.is_error.unwrap_or(false), "{ok:?}");
        let m = scryer_core::read_model_at(&model_ref).unwrap();
        let by_id = |id: &str| m.nodes.iter().find(|n| n.id == id).unwrap();
        assert!(by_id("sys").responsibilities.is_empty(), "scaffolding carries no claims");
        assert!(by_id("app").responsibilities.is_empty(), "scaffolding carries no claims");
        assert_eq!(by_id("leaf").responsibilities.len(), 1, "built claim folded");
        assert_eq!(
            scryer_core::plan_diff_at(&model_ref).unwrap().changes.len(),
            2,
            "exactly the ancestors' unbuilt claims stay pending"
        );
    }

    /// Partial implementation on a plan-only node: `responsibility_ids` +
    /// `commit_ancestors` commits the chain AND the host structure-only, folds
    /// exactly the named claims, and leaves the rest pending on the node.
    #[test]
    fn mark_implemented_scoped_fold_with_ancestors_commits_host_structure_only() {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());
        scryer_core::write_model_at(&model_ref, &ScryModel::new()).unwrap();

        let mut planned = ScryModel::new();
        planned.nodes.push(node("app", Kind::Container, "App", None));
        let mut c = node("c", Kind::Component, "Feature", Some("app"));
        c.responsibilities = vec![resp("r-1"), resp("r-2")];
        planned.nodes.push(c);
        scryer_core::write_planned_at(&model_ref, &planned).unwrap();

        let server = ScryerServer::new();
        let r = server
            .mark_implemented(Parameters(MarkImplementedRequest {
                project: Some(dir.path().to_string_lossy().to_string()),
                node_id: Some("c".into()),
                responsibility_ids: Some(vec!["r-1".into()]),
                property_labels: None,
                commit_ancestors: Some(true),
                force: None,
                anchors: None,
                tests: None,
                change: None,
                link_ids: None,
                group_ids: None,
            }))
            .unwrap();
        assert!(!r.is_error.unwrap_or(false), "{r:?}");

        let m = scryer_core::read_model_at(&model_ref).unwrap();
        let c = m.nodes.iter().find(|n| n.id == "c").expect("host structure-committed");
        assert_eq!(c.responsibilities.len(), 1, "only the named claim folded");
        assert_eq!(c.responsibilities[0].id, "r-1");
        assert!(m.nodes.iter().any(|n| n.id == "app"), "ancestor structure-committed");
        assert!(
            !scryer_core::plan_diff_at(&model_ref).unwrap().is_empty(),
            "the unbuilt claim r-2 stays pending"
        );
    }

    /// End-to-end: `delete_nodes` stages a subtree deletion in the plan, then
    /// `mark_implemented` folds it — and committed loses the whole subtree and its
    /// dangling link, not just the target node. Item C.
    #[test]
    fn delete_nodes_then_mark_implemented_cascades_committed() {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());
        let project = Some(dir.path().to_string_lossy().to_string());

        // Committed: parent → child, plus an untouched sibling and a link into it.
        let mut m = ScryModel::new();
        m.nodes.push(node("parent-1", Kind::Container, "Parent", None));
        m.nodes.push(node("child-1", Kind::Component, "Child", Some("parent-1")));
        m.nodes.push(node("keep-1", Kind::Component, "Keep", None));
        m.links.push(Link {
            id: "l1".into(),
            src: "child-1".into(),
            dst: "keep-1".into(),
            label: "calls".into(),
            method: None,
        });
        scryer_core::write_model_at(&model_ref, &m).unwrap();
        scryer_core::write_planned_at(&model_ref, &m).unwrap();

        let server = ScryerServer::new();
        // Stage the deletion of the whole `parent-1` subtree in the plan.
        server
            .delete_nodes(Parameters(DeleteNodeRequest {
                project: project.clone(),
                node_ids: vec!["parent-1".into()],
            }))
            .unwrap();
        // Code removed → fold the deletion.
        server
            .mark_implemented(Parameters(MarkImplementedRequest {
                project: project.clone(),
                node_id: Some("parent-1".into()),
                link_ids: None,
                group_ids: None,
                responsibility_ids: None,
                property_labels: None,
                commit_ancestors: None,
                force: None,
                anchors: None,
                tests: None,
                change: None,
            }))
            .unwrap();

        let m = scryer_core::read_model_at(&model_ref).unwrap();
        assert!(
            !m.nodes.iter().any(|n| n.id == "parent-1" || n.id == "child-1"),
            "whole subtree removed from committed, not just the target"
        );
        assert!(m.nodes.iter().any(|n| n.id == "keep-1"), "sibling untouched");
        assert!(m.links.is_empty(), "link into the deleted subtree dropped");
        assert!(
            scryer_core::plan_diff_at(&model_ref).unwrap().is_empty(),
            "no pending deletions left stranded"
        );
    }

    /// A plan carrying `add_links` output closes fully through `mark_implemented`:
    /// the link has no node id to fold by, so it rides in on the fold of its
    /// second endpoint. After both nodes are folded, nothing is left pending —
    /// the CLOSE loop terminates. Item A.
    #[test]
    fn mark_implemented_folds_a_nodes_incident_links() {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());
        let project = Some(dir.path().to_string_lossy().to_string());

        // Committed: empty. Plan: two new nodes and a link between them.
        scryer_core::write_model_at(&model_ref, &ScryModel::new()).unwrap();
        let mut planned = ScryModel::new();
        planned.nodes.push(node("node-1", Kind::Component, "A", None));
        planned.nodes.push(node("node-2", Kind::Component, "B", None));
        planned.links.push(Link {
            id: "l1".into(),
            src: "node-1".into(),
            dst: "node-2".into(),
            label: "calls".into(),
            method: None,
        });
        scryer_core::write_planned_at(&model_ref, &planned).unwrap();

        let server = ScryerServer::new();
        let mark = |id: &str| {
            server
                .mark_implemented(Parameters(MarkImplementedRequest {
                    project: project.clone(),
                    node_id: Some(id.into()),
                    link_ids: None,
                    group_ids: None,
                    responsibility_ids: None,
                    property_labels: None,
                    commit_ancestors: None,
                    force: None,
                anchors: None,
                tests: None,
                change: None,
                }))
                .unwrap();
        };

        // Folding the first node leaves the link pending — its far end isn't built.
        mark("node-1");
        assert!(
            !scryer_core::read_model_at(&model_ref).unwrap().links.iter().any(|l| l.id == "l1"),
            "link waits for its second endpoint"
        );

        // Folding the second node pulls the link in; the plan diff reaches empty.
        mark("node-2");
        assert!(
            scryer_core::read_model_at(&model_ref).unwrap().links.iter().any(|l| l.id == "l1"),
            "link folded with its second endpoint"
        );
        assert!(
            scryer_core::plan_diff_at(&model_ref).unwrap().is_empty(),
            "no pending work remains — CLOSE terminates"
        );
    }

    /// A link deleted between two SURVIVING nodes never rides a node fold — it
    /// folds by its own id, as does a deleted group. Without this the plan's
    /// deletion stays pending forever and the CLOSE loop can't terminate
    /// (audit theme 1 / item A residue).
    #[test]
    fn mark_implemented_folds_link_and_group_deletions() {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());

        let mut m = ScryModel::new();
        m.nodes.push(node("node-1", Kind::System, "A", None));
        m.nodes.push(node("node-2", Kind::System, "B", None));
        m.links.push(scryer_core::Link {
            id: "link-node-1-node-2".into(),
            src: "node-1".into(),
            dst: "node-2".into(),
            label: "calls".into(),
            method: None,
        });
        m.groups.push(scryer_core::Group {
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

        // Plan: the link and the group are deleted; both endpoints survive.
        scryer_core::ensure_planned_at(&model_ref).unwrap();
        let mut planned = scryer_core::read_planned_at(&model_ref).unwrap();
        planned.links.clear();
        planned.groups.clear();
        scryer_core::write_planned_at(&model_ref, &planned).unwrap();

        let server = ScryerServer::new();
        server
            .mark_implemented(Parameters(MarkImplementedRequest {
                project: Some(dir.path().to_string_lossy().to_string()),
                node_id: None,
                link_ids: Some(vec!["link-node-1-node-2".into()]),
                group_ids: Some(vec!["group-1".into()]),
                responsibility_ids: None,
                property_labels: None,
                commit_ancestors: None,
                force: None,
                anchors: None,
                tests: None,
                change: None,
            }))
            .unwrap();

        let committed = scryer_core::read_model_at(&model_ref).unwrap();
        assert!(committed.links.is_empty(), "link removal folded");
        assert!(committed.groups.is_empty(), "group removal folded");
        assert_eq!(committed.nodes.len(), 2, "endpoints untouched");
        assert!(
            scryer_core::plan_diff_at(&model_ref).unwrap().is_empty(),
            "the deletion-only plan clears — CLOSE terminates"
        );
    }

    /// Scoped: marking a single newly-planned responsibility folds just it onto
    /// its (already-committed) host node, leaving the rest of the plan untouched.
    #[test]
    fn mark_implemented_commits_named_responsibility_from_plan() {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());

        // Committed model: node-1 already owns r-a.
        let mut m = ScryModel::new();
        let mut c = node("node-1", Kind::Component, "Billing", None);
        c.responsibilities = vec![resp("r-a")];
        m.nodes.push(c);
        scryer_core::write_model_at(&model_ref, &m).unwrap();

        // Plan: a new r-b is proposed on the same node; r-c is proposed too but
        // not named in the call, so it must stay in the plan.
        let mut planned = m.clone();
        planned.nodes[0]
            .responsibilities
            .push(resp("r-b"));
        planned.nodes[0]
            .responsibilities
            .push(resp("r-c"));
        scryer_core::write_planned_at(&model_ref, &planned).unwrap();

        let server = ScryerServer::new();
        server
            .mark_implemented(Parameters(MarkImplementedRequest {
                project: Some(dir.path().to_string_lossy().to_string()),
                node_id: Some("node-1".into()),
                link_ids: None,
                group_ids: None,
                responsibility_ids: Some(vec!["r-b".into()]),
                property_labels: None,
                commit_ancestors: None,
                force: None,
                anchors: None,
                tests: None,
                change: None,
            }))
            .unwrap();

        let m = scryer_core::read_model_at(&model_ref).unwrap();
        let n = &m.nodes[0];
        assert!(n.responsibilities.iter().any(|r| r.id == "r-b"), "r-b committed");
        assert!(!n.responsibilities.iter().any(|r| r.id == "r-c"), "r-c left uncommitted");

        // Only r-c remains as a pending plan entry (Added).
        let plan = scryer_core::plan_diff_at(&model_ref).unwrap();
        assert_eq!(plan.changes.len(), 1);
        assert_eq!(plan.changes[0].id, "r-c");
    }

    /// move_nodes re-parents with kind/cycle validation and pulls the node out
    /// of its old-level group.
    #[test]
    fn move_nodes_validates_and_leaves_group() {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());
        let mut m = ScryModel::new();
        m.nodes.push(node("sys", Kind::System, "Sys", None));
        m.nodes.push(node("ca", Kind::Container, "A", Some("sys")));
        m.nodes.push(node("cb", Kind::Container, "B", Some("sys")));
        m.nodes.push(node("comp", Kind::Component, "Comp", Some("ca")));
        m.nodes.push(node("sym", Kind::Symbol, "sym", Some("comp")));
        m.groups.push(scryer_core::Group {
            id: "g1".into(),
            name: "Edge".into(),
            description: None,
            member_ids: vec!["comp".into()],
            parent_group_id: None,
            parent_node_id: Some("ca".into()),
            responsibilities: Vec::new(),
            icon: None,
        });
        scryer_core::write_model_at(&model_ref, &m).unwrap();
        let server = ScryerServer::new();
        let project = dir.path().to_string_lossy().to_string();

        // Valid: component A→B. The subtree (sym) follows; the group lets go.
        let r = server
            .move_nodes(Parameters(MoveNodesRequest {
                project: Some(project.clone()),
                moves: vec![NodeMove { node_id: "comp".into(), new_parent_id: Some("cb".into()) }],
            }))
            .unwrap();
        assert!(!r.is_error.unwrap_or(false), "{r:?}");
        // move_nodes authors into the plan; assert on the planned (draft) model.
        let m = scryer_core::read_planned_at(&model_ref).unwrap();
        let comp = m.nodes.iter().find(|n| n.id == "comp").unwrap();
        assert_eq!(comp.parent_id.as_deref(), Some("cb"));
        let sym = m.nodes.iter().find(|n| n.id == "sym").unwrap();
        assert_eq!(sym.parent_id.as_deref(), Some("comp"), "subtree intact");
        assert!(m.groups[0].member_ids.is_empty(), "left the old-level group");

        // Invalid kind pair: component under system.
        let r = server
            .move_nodes(Parameters(MoveNodesRequest {
                project: Some(project.clone()),
                moves: vec![NodeMove { node_id: "comp".into(), new_parent_id: Some("sys".into()) }],
            }))
            .unwrap();
        assert!(r.is_error.unwrap_or(false), "kind pair rejected");

        // Cycle: container under a symbol inside its own subtree.
        let r = server
            .move_nodes(Parameters(MoveNodesRequest {
                project: Some(project),
                moves: vec![NodeMove { node_id: "cb".into(), new_parent_id: Some("sym".into()) }],
            }))
            .unwrap();
        assert!(r.is_error.unwrap_or(false), "cycle rejected");
    }

    /// update_nodes reparents too, and must reject a cycle-creating parent the
    /// same way move_nodes does (audit #6) — otherwise it's a backdoor that
    /// plants a parent-chain loop and hangs every ancestor walker in core. The
    /// rejected write must leave the plan untouched.
    #[test]
    fn update_nodes_rejects_a_cycle_creating_reparent() {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());
        let mut m = ScryModel::new();
        m.nodes.push(node("sys", Kind::System, "Sys", None));
        m.nodes.push(node("ca", Kind::Container, "A", Some("sys")));
        m.nodes.push(node("comp", Kind::Component, "Comp", Some("ca")));
        scryer_core::write_planned_at(&model_ref, &m).unwrap();
        let server = ScryerServer::new();
        let project = dir.path().to_string_lossy().to_string();

        let reparent = |node_id: &str, parent: &str| UpdateNodeItem {
            node_id: node_id.into(),
            parent_id: Some(parent.into()),
            kind: None,
            name: None,
            description: None,
            technology: None,
            external: None,
            responsibilities: None,
            properties: None,
        };

        // Re-parent the System under its own grandchild: a cycle.
        let r = server
            .update_nodes(Parameters(UpdateNodeRequest {
                project: Some(project.clone()),
                nodes: vec![reparent("sys", "comp")],
            }))
            .unwrap();
        assert!(r.is_error.unwrap_or(false), "cycle reparent rejected");

        // Rejected wholesale: the plan is untouched, no loop persisted.
        let after = scryer_core::read_planned_at(&model_ref).unwrap();
        let sys = after.nodes.iter().find(|n| n.id == "sys").unwrap();
        assert_eq!(sys.parent_id, None, "parent unchanged after rejection");

        // A node set as its own parent is likewise rejected.
        let r = server
            .update_nodes(Parameters(UpdateNodeRequest {
                project: Some(project),
                nodes: vec![reparent("ca", "ca")],
            }))
            .unwrap();
        assert!(r.is_error.unwrap_or(false), "self-parent rejected");
    }

    /// update_nodes clear semantics + review-queue protection: an empty string
    /// clears description/technology (they could be set but never removed),
    /// and a responsibilities replacement that omits a VAGRANT claim keeps it —
    /// a code-discovered claim leaves only through an explicit verdict.
    #[test]
    fn update_nodes_clears_fields_and_keeps_vagrants() {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());
        let mut m = ScryModel::new();
        let mut c = node("comp", Kind::Component, "Comp", None);
        c.description = Some("old words".into());
        c.technology = Some("React".into());
        c.responsibilities.push(resp("r-keep"));
        let mut vagrant = resp("r-vagrant");
        vagrant.vagrant = Some(true);
        c.responsibilities.push(vagrant);
        m.nodes.push(c);
        scryer_core::write_planned_at(&model_ref, &m).unwrap();
        let server = ScryerServer::new();
        let project = dir.path().to_string_lossy().to_string();

        let r = server
            .update_nodes(Parameters(UpdateNodeRequest {
                project: Some(project),
                nodes: vec![UpdateNodeItem {
                    node_id: "comp".into(),
                    description: Some(String::new()),
                    technology: Some(String::new()),
                    responsibilities: Some(vec![resp("r-keep")]),
                    kind: None,
                    name: None,
                    external: None,
                    properties: None,
                    parent_id: None,
                }],
            }))
            .unwrap();
        assert!(!r.is_error.unwrap_or(false));

        let after = scryer_core::read_planned_at(&model_ref).unwrap();
        let comp = after.nodes.iter().find(|n| n.id == "comp").unwrap();
        assert_eq!(comp.description, None, "empty string cleared it");
        assert_eq!(comp.technology, None);
        assert!(
            comp.responsibilities.iter().any(|r| r.id == "r-vagrant"),
            "the vagrant claim survived the replacement"
        );
        assert!(comp.responsibilities.iter().any(|r| r.id == "r-keep"));
        assert_eq!(comp.responsibilities.len(), 2);
    }

    /// property_labels partial-folds data fields the way responsibility_ids
    /// always could for claims: the named property lands in committed while
    /// the rest of the node's plan stays pending.
    #[test]
    fn mark_implemented_folds_named_properties() {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());
        let prop = |label: &str, desc: &str| scryer_core::SchemaProperty {
            label: label.into(),
            description: desc.into(),
            vagrant: None,
            stale: None,
            last_touched_at: None,
        };
        let mut committed = ScryModel::new();
        let mut shape = node("shape", Kind::Symbol, "Lead", None);
        shape.properties.push(prop("email", "v1"));
        committed.nodes.push(shape);
        scryer_core::write_model_at(&model_ref, &committed).unwrap();
        let mut planned = committed.clone();
        planned.nodes[0].properties = vec![prop("email", "v2"), prop("age", "new field")];
        scryer_core::write_planned_at(&model_ref, &planned).unwrap();

        let server = ScryerServer::new();
        let r = server
            .mark_implemented(Parameters(MarkImplementedRequest {
                project: Some(dir.path().to_string_lossy().to_string()),
                node_id: Some("shape".into()),
                responsibility_ids: None,
                property_labels: Some(vec!["email".into()]),
                link_ids: None,
                group_ids: None,
                commit_ancestors: None,
                force: None,
                anchors: None,
                tests: None,
                change: None,
            }))
            .unwrap();
        assert!(!r.is_error.unwrap_or(false), "{r:?}");

        let after = scryer_core::read_model_at(&model_ref).unwrap();
        let shape = after.nodes.iter().find(|n| n.id == "shape").unwrap();
        let email = shape.properties.iter().find(|p| p.label == "email").unwrap();
        assert_eq!(email.description, "v2", "named fold landed");
        assert!(
            !shape.properties.iter().any(|p| p.label == "age"),
            "the unnamed property stays pending in the plan"
        );
    }

    /// update_nodes reparenting must enforce the rest of move_nodes' gate too
    /// (audit #6): a nonexistent parent, an illegal kind pairing, and an external
    /// parent are all rejected, and a valid reparent evicts the node from its
    /// old-level group.
    #[test]
    fn update_nodes_reparent_enforces_the_full_move_gate() {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());
        let mut m = ScryModel::new();
        m.nodes.push(node("sys", Kind::System, "Sys", None));
        m.nodes.push(node("ca", Kind::Container, "A", Some("sys")));
        m.nodes.push(node("cb", Kind::Container, "B", Some("sys")));
        m.nodes.push(node("comp", Kind::Component, "Comp", Some("ca")));
        let mut ext = node("ext", Kind::Container, "Ext", Some("sys"));
        ext.external = Some(true);
        m.nodes.push(ext);
        m.groups.push(scryer_core::Group {
            id: "g1".into(),
            name: "Edge".into(),
            description: None,
            member_ids: vec!["comp".into()],
            parent_group_id: None,
            parent_node_id: Some("ca".into()),
            responsibilities: Vec::new(),
            icon: None,
        });
        scryer_core::write_planned_at(&model_ref, &m).unwrap();
        let server = ScryerServer::new();
        let project = dir.path().to_string_lossy().to_string();

        let reparent = |node_id: &str, parent: &str| UpdateNodeItem {
            node_id: node_id.into(),
            parent_id: Some(parent.into()),
            kind: None,
            name: None,
            description: None,
            technology: None,
            external: None,
            responsibilities: None,
            properties: None,
        };
        let attempt = |item: UpdateNodeItem| {
            server
                .update_nodes(Parameters(UpdateNodeRequest {
                    project: Some(project.clone()),
                    nodes: vec![item],
                }))
                .unwrap()
        };

        // Nonexistent parent: rejected, not silently orphaned.
        assert!(attempt(reparent("comp", "ghost")).is_error.unwrap_or(false), "missing parent");
        // Illegal pairing: a component cannot be parented by a system.
        assert!(attempt(reparent("comp", "sys")).is_error.unwrap_or(false), "kind pair");
        // External node cannot take children.
        assert!(attempt(reparent("comp", "ext")).is_error.unwrap_or(false), "external parent");

        // The rejections left the plan untouched.
        let after = scryer_core::read_planned_at(&model_ref).unwrap();
        assert_eq!(
            after.nodes.iter().find(|n| n.id == "comp").unwrap().parent_id.as_deref(),
            Some("ca"),
            "parent unchanged after the rejected reparents"
        );

        // Valid reparent A→B: applied, and the node leaves its old-level group.
        assert!(!attempt(reparent("comp", "cb")).is_error.unwrap_or(false), "valid reparent");
        let after = scryer_core::read_planned_at(&model_ref).unwrap();
        assert_eq!(
            after.nodes.iter().find(|n| n.id == "comp").unwrap().parent_id.as_deref(),
            Some("cb")
        );
        assert!(after.groups[0].member_ids.is_empty(), "left the old-level group");
    }

    /// Fold + anchor is one atomic statement: `anchors` on mark_implemented
    /// writes the just-committed claim's anchor into its single home (the
    /// committed layer, no shadow copy in the draft), and an unknown
    /// responsibility id fails the call BEFORE anything folds.
    #[test]
    fn mark_implemented_anchors_atomically() {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());

        let mut committed = ScryModel::new();
        committed.nodes.push(node("sys", Kind::System, "Acme", None));
        committed.nodes.push(node("cont", Kind::Container, "API", Some("sys")));
        committed.nodes.push(node("comp", Kind::Component, "Auth", Some("cont")));
        let mut planned = committed.clone();
        planned
            .nodes
            .iter_mut()
            .find(|n| n.id == "comp")
            .unwrap()
            .responsibilities
            .push(resp("resp-1"));
        scryer_core::write_model_at(&model_ref, &committed).unwrap();
        scryer_core::write_planned_at(&model_ref, &planned).unwrap();

        let server = ScryerServer::new();
        let project = dir.path().to_string_lossy().to_string();
        let anchor = |rid: &str| SourceMapEntry {
            responsibility_id: rid.into(),
            locations: vec![serde_json::from_value(
                serde_json::json!({ "pattern": "src/auth.rs" }),
            )
            .unwrap()],
        };

        // Unknown responsibility id: rejected up front, nothing folded.
        let r = server
            .mark_implemented(Parameters(MarkImplementedRequest {
                project: Some(project.clone()),
                node_id: Some("comp".into()),
                responsibility_ids: None,
                property_labels: None,
                link_ids: None,
                group_ids: None,
                commit_ancestors: None,
                force: None,
                anchors: Some(vec![anchor("resp-ghost")]),
                tests: None,
                change: None,
            }))
            .unwrap();
        assert!(r.is_error.unwrap_or(false), "unknown anchor id rejected");
        let m = scryer_core::read_model_at(&model_ref).unwrap();
        assert!(
            m.nodes.iter().find(|n| n.id == "comp").unwrap().responsibilities.is_empty(),
            "nothing folded on the failed call"
        );

        // Valid: fold + anchor in one call.
        let r = server
            .mark_implemented(Parameters(MarkImplementedRequest {
                project: Some(project),
                node_id: Some("comp".into()),
                responsibility_ids: None,
                property_labels: None,
                link_ids: None,
                group_ids: None,
                commit_ancestors: None,
                force: None,
                anchors: Some(vec![anchor("resp-1")]),
                tests: None,
                change: None,
            }))
            .unwrap();
        let text = r
            .content
            .iter()
            .find_map(|c| c.as_text().map(|t| t.text.clone()))
            .unwrap();
        assert!(text.contains("Anchored 1 claim(s)"), "{text}");
        assert!(
            text.contains("post-flight 'comp': plan clear on this node"),
            "anchored + folded reads clean: {text}"
        );

        let committed = scryer_core::read_model_at(&model_ref).unwrap();
        assert_eq!(
            committed.source_map["resp-1"][0].pattern, "src/auth.rs",
            "anchor lives in the committed layer"
        );
        let planned = scryer_core::read_planned_at(&model_ref).unwrap();
        assert!(
            !planned.source_map.contains_key("resp-1"),
            "no shadow copy in the draft"
        );
    }

    /// `tests` records a claim's attached test in the same fold call: the
    /// entry lands in the committed test_map (single home, like fold-time
    /// anchors), an unknown id is rejected up front, and the response says
    /// what was recorded.
    #[test]
    fn mark_implemented_records_attached_tests() {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());
        let mut committed = ScryModel::new();
        committed.nodes.push(node("comp", Kind::Component, "Auth", None));
        scryer_core::write_model_at(&model_ref, &committed).unwrap();
        scryer_core::ensure_planned_at(&model_ref).unwrap();
        let mut planned = scryer_core::read_planned_at(&model_ref).unwrap();
        planned.nodes[0].responsibilities.push(resp("resp-1"));
        scryer_core::write_planned_at(&model_ref, &planned).unwrap();

        let test_entry = |rid: &str| SourceMapEntry {
            responsibility_id: rid.into(),
            locations: vec![serde_json::from_value(
                serde_json::json!({ "pattern": "tests/auth.rs", "symbol": "forged_rejected" }),
            )
            .unwrap()],
        };
        let server = ScryerServer::new();
        let project = dir.path().to_string_lossy().to_string();

        // Unknown id: rejected before anything folds.
        let r = server
            .mark_implemented(Parameters(MarkImplementedRequest {
                project: Some(project.clone()),
                node_id: Some("comp".into()),
                responsibility_ids: None,
                property_labels: None,
                link_ids: None,
                group_ids: None,
                commit_ancestors: None,
                force: None,
                anchors: None,
                tests: Some(vec![test_entry("resp-ghost")]),
                change: None,
            }))
            .unwrap();
        assert!(r.is_error.unwrap_or(false), "unknown tests id rejected");

        let r = server
            .mark_implemented(Parameters(MarkImplementedRequest {
                project: Some(project),
                node_id: Some("comp".into()),
                responsibility_ids: None,
                property_labels: None,
                link_ids: None,
                group_ids: None,
                commit_ancestors: None,
                force: None,
                anchors: None,
                tests: Some(vec![test_entry("resp-1")]),
                change: None,
            }))
            .unwrap();
        let text = r
            .content
            .iter()
            .find_map(|c| c.as_text().map(|t| t.text.clone()))
            .unwrap();
        assert!(text.contains("Recorded attached test(s) for 1 claim(s)"), "{text}");

        let committed = scryer_core::read_model_at(&model_ref).unwrap();
        let loc = &committed.test_map["resp-1"][0];
        assert_eq!(loc.pattern, "tests/auth.rs", "attached test lives in the committed layer");
        assert_eq!(loc.symbol.as_deref(), Some("forged_rejected"));
        let planned = scryer_core::read_planned_at(&model_ref).unwrap();
        assert!(!planned.test_map.contains_key("resp-1"), "no shadow copy in the draft");
    }


    /// A symbol host with one claim in the plan, the committed model empty.
    fn plan_with_claim(model_ref: &ModelRef, statement: &str) {
        scryer_core::write_model_at(model_ref, &ScryModel::new()).unwrap();
        scryer_core::ensure_planned_at(model_ref).unwrap();
        let mut planned = scryer_core::read_planned_at(model_ref).unwrap();
        let mut sym = node("vt", Kind::Symbol, "verify_token", None);
        let mut r1 = resp("resp-1");
        r1.statement = statement.into();
        sym.responsibilities.push(r1);
        planned.nodes.push(sym);
        scryer_core::write_planned_at(model_ref, &planned).unwrap();
    }

    fn fold_node(server: &ScryerServer, dir: &std::path::Path, node_id: &str, force: bool) -> String {
        let r = server
            .mark_implemented(Parameters(MarkImplementedRequest {
                project: Some(dir.to_string_lossy().to_string()),
                node_id: Some(node_id.into()),
                responsibility_ids: None,
                property_labels: None,
                link_ids: None,
                group_ids: None,
                commit_ancestors: None,
                force: force.then_some(true),
                anchors: None,
                tests: None,
                change: None,
            }))
            .unwrap();
        assert!(!r.is_error.unwrap_or(false), "a refusal is never a tool error");
        tool_text(&r)
    }

    /// Fold a whole change — how tagged work folds in practice (a whole-node
    /// fold of an untagged host leaves tagged claims behind by design).
    fn fold_change(server: &ScryerServer, dir: &std::path::Path, cid: &str) -> String {
        let r = server
            .mark_implemented(Parameters(MarkImplementedRequest {
                project: Some(dir.to_string_lossy().to_string()),
                node_id: None,
                responsibility_ids: None,
                property_labels: None,
                link_ids: None,
                group_ids: None,
                commit_ancestors: None,
                force: None,
                anchors: None,
                tests: None,
                change: Some(cid.into()),
            }))
            .unwrap();
        assert!(!r.is_error.unwrap_or(false), "a refusal is never a tool error: {}", tool_text(&r));
        tool_text(&r)
    }

    fn committed_has(model_ref: &ModelRef, resp_id: &str) -> bool {
        scryer_core::read_model_at(model_ref)
            .unwrap()
            .nodes
            .iter()
            .flat_map(|n| n.responsibilities.iter())
            .any(|r| r.id == resp_id)
    }

    fn planned_resp(model_ref: &ModelRef, resp_id: &str) -> Option<Responsibility> {
        scryer_core::read_planned_at(model_ref)
            .unwrap()
            .nodes
            .iter()
            .flat_map(|n| n.responsibilities.iter())
            .find(|r| r.id == resp_id)
            .cloned()
    }

    /// The evidence gate: a testable (When/While/If) claim on a code-backed
    /// host with NO test attached does not fold. It stays in the plan, the
    /// response names the claim and the missing fact, and the refusal ledger
    /// records it for the inbox.
    #[test]
    fn fold_refuses_a_testable_claim_without_a_test() {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());
        plan_with_claim(&model_ref, "**If** the token is forged, **then** reject the request");

        let text = fold_node(&ScryerServer::new(), dir.path(), "vt", false);
        assert!(text.contains("REFUSED resp-1"), "{text}");
        assert!(text.contains("no test attached"), "{text}");
        assert!(!committed_has(&model_ref, "resp-1"), "the claim did not fold");
        assert!(planned_resp(&model_ref, "resp-1").is_some(), "the claim stays in the plan");
        let refusals = scryer_core::refusals::read_refusals(&model_ref);
        assert_eq!(refusals.len(), 1);
        assert_eq!(refusals[0].kind, "no-test");
        // The host itself folded structure-wise — the rest of the fold proceeds.
        assert!(scryer_core::read_model_at(&model_ref).unwrap().nodes.iter().any(|n| n.id == "vt"));
    }

    /// A test attached in the SAME call but never run is still refused: the
    /// verdict comes from a run + ingest, and the refusal names the file.
    #[test]
    fn fold_refuses_a_claim_whose_attached_test_has_no_verdict() {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());
        plan_with_claim(&model_ref, "**When** a token arrives, **verify** its signature");
        let server = ScryerServer::new();
        let r = server
            .mark_implemented(Parameters(MarkImplementedRequest {
                project: Some(dir.path().to_string_lossy().to_string()),
                node_id: Some("vt".into()),
                responsibility_ids: None,
                property_labels: None,
                link_ids: None,
                group_ids: None,
                commit_ancestors: None,
                force: None,
                anchors: None,
                tests: Some(vec![SourceMapEntry {
                    responsibility_id: "resp-1".into(),
                    locations: vec![serde_json::from_value(
                        serde_json::json!({ "pattern": "tests/auth.rs", "symbol": "verifies" }),
                    )
                    .unwrap()],
                }]),
                change: None,
            }))
            .unwrap();
        let text = tool_text(&r);
        assert!(text.contains("REFUSED resp-1"), "{text}");
        assert!(text.contains("no verdict recorded: run tests/auth.rs"), "{text}");
        assert!(!committed_has(&model_ref, "resp-1"));
        // The attachment still landed — on the plan copy, where the claim lives.
        let planned = scryer_core::read_planned_at(&model_ref).unwrap();
        assert!(planned.test_map.contains_key("resp-1"), "test attached to the plan copy");
        assert_eq!(scryer_core::refusals::read_refusals(&model_ref)[0].kind, "no-verdict");
    }

    /// Ubiquitous claims are not gated: a test is a judgment call there, and
    /// the fold stays advisory exactly as before.
    #[test]
    fn fold_leaves_ubiquitous_claims_ungated() {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());
        plan_with_claim(&model_ref, "Verifies request signatures");
        let text = fold_node(&ScryerServer::new(), dir.path(), "vt", false);
        assert!(!text.contains("REFUSED"), "{text}");
        assert!(committed_has(&model_ref, "resp-1"));
        assert!(scryer_core::refusals::read_refusals(&model_ref).is_empty());
    }

    /// Seed a project whose claim is anchored, test-attached, and whose test
    /// report has been ingested — the fully verified state.
    fn verified_project(dir: &std::path::Path) -> ModelRef {
        let model_ref = ModelRef::ProjectLocal(dir.to_path_buf());
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::create_dir_all(dir.join("tests")).unwrap();
        std::fs::write(dir.join("src/auth.rs"), "fn verify_token() {\n    let ok = true;\n}\n").unwrap();
        std::fs::write(dir.join("tests/auth.rs"), "#[test]\nfn forged_rejected() {\n    assert!(true);\n}\n").unwrap();
        plan_with_claim(&model_ref, "**If** the token is forged, **then** reject the request");
        let mut planned = scryer_core::read_planned_at(&model_ref).unwrap();
        planned.source_map.insert(
            "resp-1".into(),
            vec![serde_json::from_value(
                serde_json::json!({ "pattern": "src/auth.rs", "symbol": "verify_token" }),
            )
            .unwrap()],
        );
        planned.test_map.insert(
            "resp-1".into(),
            vec![serde_json::from_value(
                serde_json::json!({ "pattern": "tests/auth.rs", "symbol": "forged_rejected" }),
            )
            .unwrap()],
        );
        scryer_core::write_planned_at(&model_ref, &planned).unwrap();
        let junit = r#"<testsuites><testsuite name="auth"><testcase classname="tests/auth.rs" name="forged_rejected" time="0.001"/></testsuite></testsuites>"#;
        let summary = scryer_extract::test_status::ingest_report(&model_ref, junit).unwrap();
        assert_eq!(summary.recorded, 1, "the plan-layer attachment matched: {:?}", summary.report);
        model_ref
    }

    /// With a current passing verdict the claim folds, and the fold writes the
    /// fingerprint baseline for exactly what it landed — the implementation
    /// anchor and the test anchor — so the fresh claim is never silent.
    #[test]
    fn fold_accepts_a_verified_claim_and_writes_its_baseline() {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = verified_project(dir.path());
        let text = fold_node(&ScryerServer::new(), dir.path(), "vt", false);
        assert!(!text.contains("REFUSED"), "{text}");
        assert!(committed_has(&model_ref, "resp-1"));
        let baseline = std::fs::read_to_string(model_ref.anchors_path()).unwrap();
        assert!(baseline.contains("\"key\":\"resp-1\""), "impl anchor fingerprinted: {baseline}");
        assert!(baseline.contains("\"key\":\"test:resp-1\""), "test anchor fingerprinted: {baseline}");
        // The verdict still reads current after the fold moved the anchors to committed.
        let statuses = scryer_extract::test_status::test_statuses(&model_ref).unwrap();
        assert_eq!(statuses.len(), 1);
        assert!(!statuses[0].stale, "fold must not age the verdict");
    }

    /// Editing the implementation after the report was ingested makes the
    /// verdict stale, and a stale verdict refuses like a missing one — naming
    /// the file to re-run.
    #[test]
    fn fold_refuses_a_claim_with_a_stale_verdict() {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = verified_project(dir.path());
        std::fs::write(
            dir.path().join("src/auth.rs"),
            "fn verify_token() {\n    let ok = false;\n}\n",
        )
        .unwrap();
        let text = fold_node(&ScryerServer::new(), dir.path(), "vt", false);
        assert!(text.contains("REFUSED resp-1"), "{text}");
        assert!(text.contains("verdict stale: run tests/auth.rs"), "{text}");
        assert!(!committed_has(&model_ref, "resp-1"));
        assert_eq!(scryer_core::refusals::read_refusals(&model_ref)[0].kind, "stale");
    }

    /// `force: true` folds an unverified claim anyway — and leaves an
    /// `unverified` history event naming it, so the bypass is on the record.
    #[test]
    fn force_folds_unverified_and_records_it() {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());
        plan_with_claim(&model_ref, "**If** the token is forged, **then** reject the request");
        let text = fold_node(&ScryerServer::new(), dir.path(), "vt", true);
        assert!(text.contains("UNVERIFIED resp-1"), "{text}");
        assert!(committed_has(&model_ref, "resp-1"));
        let events = scryer_core::history::read_history(&model_ref);
        assert!(
            events.iter().any(|e| e.driver == "unverified" && e.node_id == "vt"),
            "{events:?}"
        );
        assert!(scryer_core::refusals::read_refusals(&model_ref).is_empty());
    }

    /// A signed-off plan with one change, `resp-1` tagged to it. Ubiquitous
    /// statement so the evidence gate stays out of the picture.
    fn signed_off_plan(model_ref: &ModelRef, committed_statement: Option<&str>) -> String {
        let mut committed = ScryModel::new();
        let mut sym = node("vt", Kind::Symbol, "verify_token", None);
        if let Some(stmt) = committed_statement {
            let mut r1 = resp("resp-1");
            r1.statement = stmt.into();
            sym.responsibilities.push(r1);
        }
        committed.nodes.push(sym);
        scryer_core::write_model_at(model_ref, &committed).unwrap();
        scryer_core::ensure_planned_at(model_ref).unwrap();
        let mut planned = scryer_core::read_planned_at(model_ref).unwrap();
        let host = planned.nodes.iter_mut().find(|n| n.id == "vt").unwrap();
        host.responsibilities.retain(|r| r.id != "resp-1");
        let mut r1 = resp("resp-1");
        r1.statement = "Verifies the approved thing".into();
        host.responsibilities.push(r1);
        let cid = scryer_core::changes::open_change(&mut planned, "verify tokens", 1);
        scryer_core::changes::tag(&mut planned, &["resp:resp-1".to_string()], &cid);
        scryer_core::changes::sign_off(&mut planned, &cid, 2).unwrap();
        scryer_core::write_planned_at(model_ref, &planned).unwrap();
        cid
    }

    /// Rewording a signed-off claim after sign-off: the fold does NOT commit
    /// it. It becomes vagrant/amendment carrying the approved text, stays in
    /// the plan, and — when it was already committed — committed keeps the
    /// original instead of losing the claim.
    #[test]
    fn fold_withholds_a_post_signoff_amendment_and_keeps_the_committed_original() {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());
        let cid = signed_off_plan(&model_ref, Some("Verifies the old thing"));
        let mut planned = scryer_core::read_planned_at(&model_ref).unwrap();
        planned.nodes[0].responsibilities[0].statement = "Verifies something else entirely".into();
        scryer_core::write_planned_at(&model_ref, &planned).unwrap();

        let text = fold_change(&ScryerServer::new(), dir.path(), &cid);
        assert!(text.contains("AWAITING VERDICT resp-1"), "{text}");
        assert!(text.contains("reworded after sign-off"), "{text}");
        let r = planned_resp(&model_ref, "resp-1").unwrap();
        assert_eq!(r.vagrant, Some(true));
        assert_eq!(r.vagrant_origin.as_deref(), Some("amendment"));
        assert_eq!(r.approved_statement.as_deref(), Some("Verifies the approved thing"));
        assert_eq!(r.statement, "Verifies something else entirely");
        let committed = scryer_core::read_model_at(&model_ref).unwrap();
        let c = committed.nodes[0].responsibilities.iter().find(|r| r.id == "resp-1").unwrap();
        assert_eq!(c.statement, "Verifies the old thing", "committed keeps the original");
        assert_eq!(scryer_core::refusals::read_refusals(&model_ref)[0].kind, "amendment");
    }

    /// A claim the agent adds after sign-off is scope it invented: withheld as
    /// vagrant/addition, while the signed-off claim beside it folds.
    #[test]
    fn fold_withholds_a_post_signoff_addition_but_folds_the_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());
        let cid = signed_off_plan(&model_ref, None);
        let mut planned = scryer_core::read_planned_at(&model_ref).unwrap();
        let mut extra = resp("resp-2");
        extra.statement = "Also logs every token".into();
        planned.nodes[0].responsibilities.push(extra);
        scryer_core::changes::tag(&mut planned, &["resp:resp-2".to_string()], &cid);
        scryer_core::write_planned_at(&model_ref, &planned).unwrap();

        let text = fold_change(&ScryerServer::new(), dir.path(), &cid);
        assert!(text.contains("AWAITING VERDICT resp-2"), "{text}");
        assert!(text.contains("added after sign-off"), "{text}");
        assert!(committed_has(&model_ref, "resp-1"), "the untouched intent folded");
        assert!(!committed_has(&model_ref, "resp-2"), "the addition did not");
        let r2 = planned_resp(&model_ref, "resp-2").unwrap();
        assert_eq!(r2.vagrant_origin.as_deref(), Some("addition"));
        assert!(r2.approved_statement.is_none());
    }

    /// Retagging the concern is metadata, not intent — it never reads as an
    /// amendment, so the claim folds as signed off.
    #[test]
    fn a_cosmetic_edit_after_signoff_is_not_an_amendment() {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());
        let cid = signed_off_plan(&model_ref, None);
        let mut planned = scryer_core::read_planned_at(&model_ref).unwrap();
        planned.nodes[0].responsibilities[0].concern = Some("auth".into());
        scryer_core::write_planned_at(&model_ref, &planned).unwrap();
        let text = fold_change(&ScryerServer::new(), dir.path(), &cid);
        assert!(!text.contains("AWAITING VERDICT"), "{text}");
        assert!(committed_has(&model_ref, "resp-1"));
    }

    /// A signed-off claim the agent dropped from the plan comes back as
    /// pending intent at the fold, and the response says the agent proposed
    /// dropping it.
    #[test]
    fn fold_restores_a_signed_off_claim_the_agent_dropped() {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());
        let cid = signed_off_plan(&model_ref, None);
        let mut planned = scryer_core::read_planned_at(&model_ref).unwrap();
        let mut keep = resp("resp-3");
        keep.statement = "Keeps this one".into();
        planned.nodes[0].responsibilities.push(keep);
        scryer_core::changes::tag(&mut planned, &["resp:resp-3".to_string()], &cid);
        scryer_core::changes::sign_off(&mut planned, &cid, 3).unwrap();
        // The agent drops resp-1 (its tag is GC'd by the write) and folds.
        planned.nodes[0].responsibilities.retain(|r| r.id != "resp-1");
        scryer_core::write_planned_at(&model_ref, &planned).unwrap();
        assert!(planned_resp(&model_ref, "resp-1").is_none());

        let text = fold_change(&ScryerServer::new(), dir.path(), &cid);
        assert!(text.contains("RESTORED resp-1"), "{text}");
        let r1 = planned_resp(&model_ref, "resp-1").expect("restored into the plan");
        assert_eq!(r1.statement, "Verifies the approved thing");
        assert!(!committed_has(&model_ref, "resp-1"), "restored as PENDING intent, not folded");
        assert!(committed_has(&model_ref, "resp-3"), "the untouched claim folded");
        let planned = scryer_core::read_planned_at(&model_ref).unwrap();
        assert_eq!(planned.change_map.get("resp:resp-1").map(String::as_str), Some(cid.as_str()));
        assert!(planned.changes.iter().any(|c| c.id == cid), "the change stays open on it");
    }

    /// The fold response carries a scoped post-flight: what's still pending on
    /// the node (with the deletions-need-explicit-ids hint) and which committed
    /// claims have no code anchor — the consistency burden lives in the tool,
    /// not in the agent remembering four follow-up calls.
    #[test]
    fn mark_implemented_postflight_reports_leftovers() {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());

        let mut committed = ScryModel::new();
        committed.nodes.push(node("sys", Kind::System, "Acme", None));
        committed.nodes.push(node("cont", Kind::Container, "API", Some("sys")));
        committed.nodes.push(node("comp", Kind::Component, "Auth", Some("cont")));
        committed.nodes.push(node("peer", Kind::Component, "Billing", Some("cont")));
        // A committed link the plan REMOVES — deletions never ride a node fold.
        committed.links.push(Link {
            id: "l-old".into(),
            src: "comp".into(),
            dst: "peer".into(),
            label: "notifies".into(),
            method: None,
        });
        let mut planned = committed.clone();
        planned.links.clear();
        planned
            .nodes
            .iter_mut()
            .find(|n| n.id == "comp")
            .unwrap()
            .responsibilities
            .push(resp("resp-1"));
        // A plan-added link to a plan-only node: not ready, stays pending.
        planned.nodes.push(node("newco", Kind::Component, "Tokens", Some("cont")));
        planned.links.push(Link {
            id: "l-new".into(),
            src: "comp".into(),
            dst: "newco".into(),
            label: "mints via".into(),
            method: None,
        });
        scryer_core::write_model_at(&model_ref, &committed).unwrap();
        scryer_core::write_planned_at(&model_ref, &planned).unwrap();

        let server = ScryerServer::new();
        let r = server
            .mark_implemented(Parameters(MarkImplementedRequest {
                project: Some(dir.path().to_string_lossy().to_string()),
                node_id: Some("comp".into()),
                responsibility_ids: None,
                property_labels: None,
                link_ids: None,
                group_ids: None,
                commit_ancestors: None,
                force: None,
                anchors: None,
                tests: None,
                change: None,
            }))
            .unwrap();
        let text = r
            .content
            .iter()
            .find_map(|c| c.as_text().map(|t| t.text.clone()))
            .unwrap();

        assert!(text.contains("post-flight 'comp':"), "{text}");
        assert!(
            text.contains("2 change(s) touching this node still pending"),
            "the unready link add and the link deletion: {text}"
        );
        assert!(
            text.contains("deletions fold only by explicit ids"),
            "steers at the unterminable-queue trap: {text}"
        );
        assert!(
            text.contains("1 committed claim(s) on it have NO code anchor (resp-1)"),
            "the unanchored fold is named: {text}"
        );
    }

    /// The change id a `set_change` response opened — "Opened chg-…".
    fn opened(r: &CallToolResult) -> String {
        let text = tool_text(r);
        let rest = text.split("Opened ").nth(1).unwrap_or_else(|| panic!("no 'Opened' in: {text}"));
        rest.split(|c: char| c.is_whitespace() || c == '(' || c == ',' || c == '.').next().unwrap().to_string()
    }

    /// The minted id (and first claim id) of the planned node called `name`.
    fn planned_named(model_ref: &ModelRef, name: &str) -> (String, Option<String>) {
        let planned = scryer_core::read_planned_at(model_ref).unwrap();
        let n = planned
            .nodes
            .iter()
            .find(|n| n.name == name)
            .unwrap_or_else(|| panic!("no planned node named {name}"));
        (n.id.clone(), n.responsibilities.first().map(|r| r.id.clone()))
    }

    fn tool_text(r: &CallToolResult) -> String {
        r.content.iter().find_map(|c| c.as_text().map(|t| t.text.clone())).unwrap()
    }

    /// The ledger loop end to end: `set_change` opens a named change, an
    /// authoring write tags to it automatically, `get_pending` groups and
    /// filters by it, a second session resumes it by id, and
    /// `mark_implemented {change}` folds exactly its entries — closing the
    /// change and recording its rationale in history.
    #[test]
    fn change_ledger_tags_writes_filters_pending_and_folds_by_change() {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());
        let project = dir.path().to_string_lossy().to_string();
        let mut m = ScryModel::new();
        m.nodes.push(node("node-1", Kind::System, "Acme", None));
        m.nodes.push(node("node-2", Kind::Container, "API", Some("node-1")));
        scryer_core::write_model_at(&model_ref, &m).unwrap();

        let server = ScryerServer::new();
        let r = server
            .set_change(Parameters(SetChangeRequest {
                project: Some(project.clone()),
                rationale: Some("give the API rate limiting".into()),
                change_id: None,
                clear: None,
                close: None,
                retag: None,
                to: None,
            }))
            .unwrap();
        assert!(tool_text(&r).contains("Opened chg-"), "{}", tool_text(&r));
        let chg = opened(&r);

        // An authoring write in this session tags what it changed.
        server
            .add_component(Parameters(AddComponentRequest {
                project: Some(project.clone()),
                items: vec![ComponentItem {
                    parent_id: "node-2".into(),
                    name: "RateLimiter".into(),
                    description: None,
                    responsibilities: vec!["throttles requests per client".into()],
                }],
            }))
            .unwrap();
        let (rl, rl_resp) = planned_named(&model_ref, "RateLimiter");
        let rl_resp = rl_resp.unwrap();
        let planned = scryer_core::read_planned_at(&model_ref).unwrap();
        assert_eq!(planned.change_map.get(&format!("node:{rl}")).map(String::as_str), Some(chg.as_str()));
        assert_eq!(planned.change_map.get(&format!("resp:{rl_resp}")).map(String::as_str), Some(chg.as_str()));

        // get_pending groups by change and filters to one.
        let r = server
            .get_pending(Parameters(GetPendingRequest {
                project: Some(project.clone()),
                change: None,
            }))
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&tool_text(&r)).unwrap();
        assert_eq!(v["currentChange"], chg.as_str());
        assert_eq!(v["openChanges"][0]["id"], chg.as_str());
        assert_eq!(v["openChanges"][0]["rationale"], "give the API rate limiting");
        assert!(v["changes"].as_array().unwrap().iter().all(|c| c["change"] == chg.as_str()));
        let r = server
            .get_pending(Parameters(GetPendingRequest {
                project: Some(project.clone()),
                change: Some("unfiled".into()),
            }))
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&tool_text(&r)).unwrap();
        assert!(v["changes"].as_array().unwrap().is_empty(), "everything is tagged");

        // A FRESH session (new server) resumes the change by id…
        let session2 = ScryerServer::new();
        let r = session2
            .set_change(Parameters(SetChangeRequest {
                project: Some(project.clone()),
                rationale: None,
                change_id: Some(chg.clone()),
                clear: None,
                close: None,
                retag: None,
                to: None,
            }))
            .unwrap();
        assert!(tool_text(&r).contains(&format!("Resumed {chg}")), "{}", tool_text(&r));

        // …and folds the whole change in one call.
        let r = session2
            .mark_implemented(Parameters(MarkImplementedRequest {
                project: Some(project.clone()),
                node_id: None,
                responsibility_ids: None,
                property_labels: None,
                link_ids: None,
                group_ids: None,
                commit_ancestors: None,
                force: None,
                anchors: None,
                tests: None,
                change: Some(chg.clone()),
            }))
            .unwrap();
        let text = tool_text(&r);
        assert!(text.contains(&format!("Folded change {chg}")), "{text}");
        assert!(text.contains("fully folded and closed"), "{text}");

        let committed = scryer_core::read_model_at(&model_ref).unwrap();
        assert!(committed.nodes.iter().any(|n| n.id == rl));
        let planned = scryer_core::read_planned_at(&model_ref).unwrap();
        assert!(planned.changes.is_empty() && planned.change_map.is_empty());

        // The rationale survived the fold, and the impl event knows its change.
        let history = scryer_core::history::read_history(&model_ref);
        let close = history
            .iter()
            .find(|e| e.kind == scryer_core::history::EventKind::Change)
            .expect("a change-closed event");
        assert_eq!(close.change_id.as_deref(), Some(chg.as_str()));
        assert_eq!(close.rows[0].text, "give the API rate limiting");
        let impl_ev = history
            .iter()
            .find(|e| e.kind == EventKind::Impl)
            .expect("an impl event");
        assert_eq!(impl_ev.change_id.as_deref(), Some(chg.as_str()));
    }

    /// `set_change {retag}` re-files work that already exists: a node id moves
    /// the carrier and everything pending under it, so an agent that filed a
    /// task under the wrong change repairs the ledger instead of re-writing
    /// the spec. The response names what moved and what matched nothing.
    #[test]
    fn set_change_retag_moves_pending_work_between_changes() {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());
        let project = dir.path().to_string_lossy().to_string();
        let mut m = ScryModel::new();
        m.nodes.push(node("node-1", Kind::System, "Acme", None));
        m.nodes.push(node("node-2", Kind::Container, "API", Some("node-1")));
        scryer_core::write_model_at(&model_ref, &m).unwrap();

        let server = ScryerServer::new();
        let open = |rationale: &str| {
            server
                .set_change(Parameters(SetChangeRequest {
                    project: Some(project.clone()),
                    rationale: Some(rationale.into()),
                    change_id: None,
                    clear: None,
                    close: None,
                    retag: None,
                    to: None,
                }))
                .unwrap()
        };
        let chg1 = opened(&open("give the API rate limiting"));
        // Written while chg-1 is selected — this is the mis-filing.
        server
            .add_component(Parameters(AddComponentRequest {
                project: Some(project.clone()),
                items: vec![ComponentItem {
                    parent_id: "node-2".into(),
                    name: "RateLimiter".into(),
                    description: None,
                    responsibilities: vec!["throttles requests per client".into()],
                }],
            }))
            .unwrap();
        let chg2 = opened(&open("the change it actually belongs to"));
        let (rl, rl_resp) = planned_named(&model_ref, "RateLimiter");
        let rl_resp = rl_resp.unwrap();

        let r = server
            .set_change(Parameters(SetChangeRequest {
                project: Some(project.clone()),
                rationale: None,
                change_id: None,
                clear: None,
                close: None,
                retag: Some(vec![rl.clone(), "node-99".into()]),
                to: Some(chg2.clone()),
            }))
            .unwrap();
        let text = tool_text(&r);
        assert!(text.contains(&format!("Moved 2 entries to {chg2}")), "{text}");
        assert!(text.contains(&format!("node:{rl} (was {chg1})")), "{text}");
        assert!(text.contains(&format!("resp:{rl_resp} (was {chg1})")), "{text}");
        assert!(text.contains("No pending work under: node-99"), "{text}");

        // The carrier AND its claim moved together — the unit get_pending shows.
        let planned = scryer_core::read_planned_at(&model_ref).unwrap();
        assert_eq!(planned.change_map.get(&format!("node:{rl}")).map(String::as_str), Some(chg2.as_str()));
        assert_eq!(planned.change_map.get(&format!("resp:{rl_resp}")).map(String::as_str), Some(chg2.as_str()));

        // Detaching sends them back to the unfiled bucket.
        let r = server
            .set_change(Parameters(SetChangeRequest {
                project: Some(project.clone()),
                rationale: None,
                change_id: None,
                clear: None,
                close: None,
                retag: Some(vec![chg2.clone()]),
                to: Some("unfiled".into()),
            }))
            .unwrap();
        assert!(tool_text(&r).contains("Moved 2 entries to unfiled"), "{}", tool_text(&r));
        let planned = scryer_core::read_planned_at(&model_ref).unwrap();
        assert!(planned.change_map.is_empty(), "{:?}", planned.change_map);
    }

    /// `set_change {close}` is the escape hatch for a stranded empty ledger:
    /// it refuses while the change has tagged entries, closes it once empty,
    /// and detaches a session selection pointing at the closed id.
    #[test]
    fn set_change_close_discards_a_stranded_empty_ledger() {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());
        let project = dir.path().to_string_lossy().to_string();
        let mut m = ScryModel::new();
        m.nodes.push(node("node-1", Kind::System, "Acme", None));
        m.nodes.push(node("node-2", Kind::Container, "API", Some("node-1")));
        scryer_core::write_model_at(&model_ref, &m).unwrap();

        // chg-1 gets real work; chg-2 is opened and never written to.
        let server = ScryerServer::new();
        let chg1 = opened(
            &server
                .set_change(Parameters(SetChangeRequest {
                    project: Some(project.clone()),
                    rationale: Some("rate limiting".into()),
                    change_id: None,
                    clear: None,
                    close: None,
                    retag: None,
                    to: None,
                }))
                .unwrap(),
        );
        server
            .add_component(Parameters(AddComponentRequest {
                project: Some(project.clone()),
                items: vec![ComponentItem {
                    parent_id: "node-2".into(),
                    name: "RateLimiter".into(),
                    description: None,
                    responsibilities: vec![],
                }],
            }))
            .unwrap();
        let chg2 = opened(
            &server
                .set_change(Parameters(SetChangeRequest {
                    project: Some(project.clone()),
                    rationale: Some("opened then orphaned".into()),
                    change_id: None,
                    clear: None,
                    close: None,
                    retag: None,
                    to: None,
                }))
                .unwrap(),
        );
        let close = |id: &str| {
            server.set_change(Parameters(SetChangeRequest {
                project: Some(project.clone()),
                rationale: None,
                change_id: None,
                clear: None,
                close: Some(id.into()),
                retag: None,
                to: None,
            }))
        };

        // A change with tagged entries refuses to close by hand.
        let r = close(&chg1).unwrap();
        assert_eq!(r.is_error, Some(true));
        assert!(tool_text(&r).contains("still has 1 tagged entry"), "{}", tool_text(&r));

        // The stranded one closes, and the session (which selected it on
        // open) detaches.
        let r = close(&chg2).unwrap();
        assert!(tool_text(&r).contains(&format!("Closed {chg2}")), "{}", tool_text(&r));
        let planned = scryer_core::read_planned_at(&model_ref).unwrap();
        assert_eq!(planned.changes.len(), 1);
        assert_eq!(planned.changes[0].id, chg1);
        assert!(server.session_change(&model_ref).is_none(), "selection detached");

        let history = scryer_core::history::read_history(&model_ref);
        let ev = history
            .iter()
            .find(|e| e.kind == scryer_core::history::EventKind::Change)
            .expect("a change-closed event");
        assert_eq!(ev.change_id.as_deref(), Some(chg2.as_str()));
        assert_eq!(ev.driver, "abandoned");
        assert_eq!(ev.rows[0].text, "opened then orphaned");

        let r = close("chg-9").unwrap();
        assert_eq!(r.is_error, Some(true));
        assert!(tool_text(&r).contains("no open change 'chg-9'"), "{}", tool_text(&r));
    }

    /// Two changes touching the same element is the collision the ledger
    /// exists to catch: the second session's write wins the tag, but the
    /// response says so out loud.
    #[test]
    fn cross_change_retag_warns_about_the_collision() {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());
        let project = dir.path().to_string_lossy().to_string();
        let mut m = ScryModel::new();
        m.nodes.push(node("node-1", Kind::System, "Acme", None));
        m.nodes.push(node("node-2", Kind::Container, "API", Some("node-1")));
        scryer_core::write_model_at(&model_ref, &m).unwrap();

        let session1 = ScryerServer::new();
        let chg1 = opened(
            &session1
                .set_change(Parameters(SetChangeRequest {
                    project: Some(project.clone()),
                    rationale: Some("rate limiting".into()),
                    change_id: None,
                    clear: None,
                    close: None,
                    retag: None,
                    to: None,
                }))
                .unwrap(),
        );
        session1
            .add_component(Parameters(AddComponentRequest {
                project: Some(project.clone()),
                items: vec![ComponentItem {
                    parent_id: "node-2".into(),
                    name: "RateLimiter".into(),
                    description: None,
                    responsibilities: vec![],
                }],
            }))
            .unwrap();

        let (rl, _) = planned_named(&model_ref, "RateLimiter");
        let session2 = ScryerServer::new();
        let chg2 = opened(
            &session2
                .set_change(Parameters(SetChangeRequest {
                    project: Some(project.clone()),
                    rationale: Some("rename things".into()),
                    change_id: None,
                    clear: None,
                    close: None,
                    retag: None,
                    to: None,
                }))
                .unwrap(),
        );
        let r = session2
            .update_nodes(Parameters(UpdateNodeRequest {
                project: Some(project.clone()),
                nodes: vec![UpdateNodeItem {
                    node_id: rl.clone(),
                    name: Some("Throttler".into()),
                    kind: None,
                    description: None,
                    technology: None,
                    external: None,
                    responsibilities: None,
                    properties: None,
                    parent_id: None,
                }],
            }))
            .unwrap();
        let text = tool_text(&r);
        assert!(
            text.contains(&format!("conflict: node:{rl} was tagged by {chg1} (\"rate limiting\")")),
            "{text}"
        );
        let planned = scryer_core::read_planned_at(&model_ref).unwrap();
        assert_eq!(
            planned.change_map.get(&format!("node:{rl}")).map(String::as_str),
            Some(chg2.as_str()),
            "last writer wins the tag"
        );
    }

    /// A caller-invented responsibility id ("new") never enters the model:
    /// update_nodes re-mints it past BOTH layers (a plan-deleted claim's id in
    /// committed must not be re-issued) and past any hand-written `resp-N` in
    /// the same payload, keeps minted-format ids untouched, and reports the
    /// re-mints so the caller learns the real ids.
    #[test]
    fn update_nodes_remints_caller_invented_responsibility_ids() {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());

        // Committed holds resp-2 (deleted from the plan) — the union floor.
        let mut committed = ScryModel::new();
        let mut c = node("node-1", Kind::Component, "Comp", None);
        c.responsibilities = vec![resp("resp-1"), resp("resp-2")];
        committed.nodes.push(c);
        scryer_core::write_model_at(&model_ref, &committed).unwrap();
        let mut planned = committed.clone();
        planned.nodes[0].responsibilities.retain(|r| r.id != "resp-2");
        scryer_core::write_planned_at(&model_ref, &planned).unwrap();

        let server = ScryerServer::new();
        let r = server
            .update_nodes(Parameters(UpdateNodeRequest {
                project: Some(dir.path().to_string_lossy().to_string()),
                nodes: vec![UpdateNodeItem {
                    node_id: "node-1".into(),
                    kind: None,
                    name: None,
                    description: None,
                    technology: None,
                    external: None,
                    responsibilities: Some(vec![
                        resp("resp-1"),
                        resp("new"),
                        resp("resp-9"),
                        resp("new"),
                    ]),
                    properties: None,
                    parent_id: None,
                }],
            }))
            .unwrap();

        let planned = scryer_core::read_planned_at(&model_ref).unwrap();
        let ids: Vec<&str> = planned.nodes[0]
            .responsibilities
            .iter()
            .map(|r| r.id.as_str())
            .collect();
        // resp-1 and the hand-written resp-9 keep their identity; both 'new's
        // mint PAST resp-9 (payload floor) — not past resp-2 alone.
        assert_eq!(ids.len(), 4, "{ids:?}");
        assert_eq!((ids[0], ids[2]), ("resp-1", "resp-9"), "real ids keep their identity: {ids:?}");
        for fresh in [ids[1], ids[3]] {
            assert!(scryer_core::is_minted_id(fresh, "resp"), "{fresh}");
            assert!(!["resp-1", "resp-2", "resp-9"].contains(&fresh), "{ids:?}");
        }
        assert_ne!(ids[1], ids[3], "two 'new's take two ids: {ids:?}");
        let text = tool_text(&r);
        assert!(text.contains(&format!("node-1: 'new' → {}", ids[1])), "reports the re-mint: {text}");
    }

    /// The stale-snapshot collision: an agent working from an old read picks a
    /// `resp-N` for a NEW claim that has since been taken by a claim on ANOTHER
    /// node. That id is minted-format and known, so it used to sail through —
    /// hijacking the real claim's identity (directives, anchors, attached
    /// tests, change tag) and leaving two claims sharing one id. It must be
    /// re-minted, and the caller must be told why.
    #[test]
    fn update_nodes_remints_an_id_that_belongs_to_another_nodes_claim() {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());

        let mut committed = ScryModel::new();
        let mut a = node("node-1", Kind::Component, "A", None);
        a.responsibilities = vec![resp("resp-1")];
        let mut b = node("node-2", Kind::Component, "B", None);
        b.responsibilities = vec![resp("resp-2")];
        committed.nodes.push(a);
        committed.nodes.push(b);
        scryer_core::write_model_at(&model_ref, &committed).unwrap();
        scryer_core::write_planned_at(&model_ref, &committed).unwrap();

        // node-1 is written with resp-2 — which lives on node-2.
        let server = ScryerServer::new();
        let r = server
            .update_nodes(Parameters(UpdateNodeRequest {
                project: Some(dir.path().to_string_lossy().to_string()),
                nodes: vec![UpdateNodeItem {
                    node_id: "node-1".into(),
                    kind: None,
                    name: None,
                    description: None,
                    technology: None,
                    external: None,
                    responsibilities: Some(vec![resp("resp-1"), resp("resp-2")]),
                    properties: None,
                    parent_id: None,
                }],
            }))
            .unwrap();

        let planned = scryer_core::read_planned_at(&model_ref).unwrap();
        let on = |id: &str| -> Vec<String> {
            planned
                .nodes
                .iter()
                .find(|n| n.id == id)
                .unwrap()
                .responsibilities
                .iter()
                .map(|r| r.id.clone())
                .collect()
        };
        let n1 = on("node-1");
        assert_eq!(n1[0], "resp-1");
        let fresh = n1[1].clone();
        assert!(scryer_core::is_minted_id(&fresh, "resp") && fresh != "resp-2", "the colliding id was re-minted: {n1:?}");
        assert_eq!(on("node-2"), vec!["resp-2"], "the real resp-2 is untouched");
        let text = tool_text(&r);
        assert!(
            text.contains(&format!("node-1: 'resp-2' → {fresh} (that id belongs to a claim on another node)")),
            "the report names the collision and the new id: {text}"
        );
    }

    /// The node-level twin, on the tool that mints subtrees: a payload node id
    /// naming a node OUTSIDE the replaced subtree is re-minted rather than
    /// pushed in beside the real one, and the payload's own references follow
    /// the rename.
    #[test]
    fn set_node_remints_a_node_id_that_is_taken_outside_the_subtree() {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());
        let mut m = ScryModel::new();
        m.nodes.push(node("node-1", Kind::System, "Acme", None));
        m.nodes.push(node("node-2", Kind::Container, "API", Some("node-1")));
        // Lives elsewhere in the tree — not the subtree being replaced.
        m.nodes.push(node("node-3", Kind::Container, "Worker", Some("node-1")));
        scryer_core::write_model_at(&model_ref, &m).unwrap();
        scryer_core::write_planned_at(&model_ref, &m).unwrap();

        // A stale snapshot: the agent thinks node-3 is free and mints two
        // components with node-3 / node-4, linked to each other.
        let payload = serde_json::json!({
            "nodes": [
                { "id": "node-2", "kind": "container", "name": "API", "parentId": "node-1" },
                { "id": "node-3", "kind": "component", "name": "Router", "parentId": "node-2" },
                { "id": "node-4", "kind": "component", "name": "Auth", "parentId": "node-3" },
            ],
            "links": [{ "id": "link-1", "src": "node-4", "dst": "node-3", "label": "routes via" }],
        });
        let server = ScryerServer::new();
        let r = server
            .set_node(Parameters(SetNodeRequest {
                project: Some(dir.path().to_string_lossy().to_string()),
                node_id: "node-2".into(),
                data: payload.to_string(),
            }))
            .unwrap();

        let planned = scryer_core::read_planned_at(&model_ref).unwrap();
        let worker = planned.nodes.iter().find(|n| n.id == "node-3").unwrap();
        assert_eq!(worker.name, "Worker", "the real node-3 still stands");
        assert_eq!(
            planned.nodes.iter().filter(|n| n.id == "node-3").count(),
            1,
            "no duplicate id landed"
        );
        let router = planned.nodes.iter().find(|n| n.name == "Router").unwrap();
        let fresh = router.id.clone();
        assert!(scryer_core::is_minted_id(&fresh, "node") && fresh != "node-3", "the collision took a fresh id: {fresh}");
        let auth = planned.nodes.iter().find(|n| n.name == "Auth").unwrap();
        assert_eq!(
            auth.parent_id.as_deref(),
            Some(fresh.as_str()),
            "the child's parent followed the rename"
        );
        let link = planned.links.iter().find(|l| l.id == "link-1").unwrap();
        assert_eq!(link.dst, fresh, "the link endpoint followed the rename");
        let text = tool_text(&r);
        assert!(
            text.contains(&format!("'node-3' → {fresh} (that id belongs to a node outside this subtree)")),
            "the report names the collision: {text}"
        );
    }

    /// set_node writes the same subtree into BOTH layers, so a re-minted id
    /// must land identically in each — otherwise the plan diff reports a
    /// phantom reword.
    #[test]
    fn set_node_remints_ids_identically_in_both_layers() {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());

        let mut m = ScryModel::new();
        m.nodes.push(node("node-1", Kind::System, "Acme", None));
        scryer_core::write_model_at(&model_ref, &m).unwrap();
        scryer_core::write_planned_at(&model_ref, &m).unwrap();

        let payload = serde_json::json!({
            "nodes": [
                { "id": "node-2", "kind": "container", "name": "API", "parentId": "node-1",
                  "responsibilities": [{ "id": "new", "statement": "serves requests" }] }
            ],
            "links": []
        });
        let server = ScryerServer::new();
        let r = server
            .set_node(Parameters(SetNodeRequest {
                project: Some(dir.path().to_string_lossy().to_string()),
                node_id: "node-1".into(),
                data: payload.to_string(),
            }))
            .unwrap();

        let committed = scryer_core::read_model_at(&model_ref).unwrap();
        let planned = scryer_core::read_planned_at(&model_ref).unwrap();
        let minted = committed.nodes.iter().find(|n| n.id == "node-2").unwrap().responsibilities[0].id.clone();
        assert!(scryer_core::is_minted_id(&minted, "resp"), "{minted}");
        for layer in [&committed, &planned] {
            let api = layer.nodes.iter().find(|n| n.id == "node-2").unwrap();
            assert_eq!(api.responsibilities[0].id, minted, "minted, and the same in both layers");
        }
        assert!(
            scryer_core::diff::diff(&committed, &planned).is_empty(),
            "no phantom reword between layers"
        );
        assert!(tool_text(&r).contains(&format!("node-2: 'new' → {minted}")));
    }

    /// set_model replaces the whole model, but its re-mint floor still includes
    /// the OUTGOING layers: re-issuing an id the payload dropped would let
    /// enforce_readonly_directives staple the dead claim's user directives onto
    /// an unrelated new one.
    #[test]
    fn set_model_remints_past_the_outgoing_layers() {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());

        let mut prior = ScryModel::new();
        let mut c = node("node-1", Kind::System, "Acme", None);
        c.responsibilities = vec![resp("resp-5")];
        prior.nodes.push(c);
        scryer_core::write_model_at(&model_ref, &prior).unwrap();
        scryer_core::write_planned_at(&model_ref, &prior).unwrap();

        // The payload drops resp-5 and carries one caller-invented id.
        let payload = serde_json::json!({
            "version": scryer_core::SCRY_VERSION,
            "nodes": [
                { "id": "node-1", "kind": "system", "name": "Acme",
                  "responsibilities": [{ "id": "new", "statement": "does things" }] }
            ],
            "links": []
        });
        let server = ScryerServer::new();
        server
            .set_model(Parameters(SetModelRequest {
                project: Some(dir.path().to_string_lossy().to_string()),
                data: payload.to_string(),
            }))
            .unwrap();

        let committed = scryer_core::read_model_at(&model_ref).unwrap();
        let minted = &committed.nodes[0].responsibilities[0].id;
        assert!(scryer_core::is_minted_id(minted, "resp"), "{minted}");
        assert_ne!(minted, "resp-5", "must not reuse the dropped resp-5 still live in the outgoing layers");
    }
}
