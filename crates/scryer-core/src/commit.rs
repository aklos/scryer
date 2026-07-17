use std::collections::HashMap;

use crate::model::{Group, Node, Responsibility, SchemaProperty, ScryModel};
use crate::model_ref::ModelRef;
use crate::storage::{
    read_model_at, read_planned_seeded_at, working_view, write_model_at, write_planned_raw_at,
};
use crate::{changes, diff, drift};

/// Fold a finished from-code build into the committed layer. The built model is
/// the PLANNED draft (container fills and the system-level enrichment both
/// author there), but the draft is seeded clean of committed's single-home
/// anchors — writing it over `model.scry` verbatim would wipe every boundary
/// glob the seed minted and any `source_map` entry routed committed-only during
/// the build. Merge instead: the draft wins (it is the build's output),
/// committed fills the anchor/boundary gaps. The draft is then re-seeded clean,
/// so no pending plan — and no shadow copy of the just-folded anchors — survives
/// the build. Returns the folded model (what `model.scry` now holds) for
/// baselining. The caller must hold the model lock.
pub fn fold_built_model(r: &ModelRef, built: &ScryModel) -> Result<ScryModel, String> {
    // An unreadable committed layer degrades to the plain overwrite (nothing to
    // merge) rather than failing the whole build at its final step.
    let committed = read_model_at(r).unwrap_or_default();
    let mut folded = working_view(&committed, built);
    // The whole draft folds, so every open change folds with it — record each
    // before its registry entry vanishes, then strip the change state: neither
    // the committed layer nor the re-seeded (empty) plan carries any.
    for meta in &folded.changes {
        changes::record_closed(r, meta, "folded");
    }
    folded.changes.clear();
    folded.change_map.clear();
    write_model_at(r, &folded)?;
    let mut seeded = folded.clone();
    seeded.source_map.clear();
    seeded.verify_map.clear();
    seeded.boundaries.clear();
    let json = serde_json::to_string_pretty(&seeded).map_err(|e| e.to_string())?;
    write_planned_raw_at(r, &json)?;
    Ok(folded)
}

/// Locate a responsibility by id anywhere in a model, returning its host id
/// (node or group) and a clone. Responsibility ids are globally unique (the
/// minters seed past every node- and group-owned id), so this is unambiguous.
fn find_responsibility(model: &ScryModel, id: &str) -> Option<(String, Responsibility)> {
    for n in &model.nodes {
        if let Some(r) = n.responsibilities.iter().find(|r| r.id == id) {
            return Some((n.id.clone(), r.clone()));
        }
    }
    for g in &model.groups {
        if let Some(r) = g.responsibilities.iter().find(|r| r.id == id) {
            return Some((g.id.clone(), r.clone()));
        }
    }
    None
}

/// Auto-commit a single planned element into the committed model — the fold that
/// fires when an element's code is implemented (planned → model). Remove-then-
/// insert, so one path handles add, update, move, AND delete:
///
///   - planned still holds the element → upsert it into the model at its planned
///     home (a reparent/move comes along for free: the planned copy carries its
///     new `parent_id` / host).
///   - planned no longer holds it → a committed deletion: drop it from the model.
///
/// On a committed deletion the element is also purged from the planned mirror, so
/// the plan clears. On an upsert, planned already mirrors the element, so it is
/// left as-is (the diff for it goes empty automatically).
///
/// `owner_id` is required only for properties (their `(owner node, label)`
/// identity); for responsibilities the host is derived from planned. Hold the
/// model lock across the call.
///
/// (When the explicit delete tombstone lands, a tombstoned element routes through
/// the same delete branch — one added `.filter(|x| !deleted)` at each lookup.)
/// Strip planned-layer review markers from a responsibility entering the
/// committed model. The committed model is the source of truth and carries
/// neither the `vagrant` adoption marker nor the `stale`/`stale_proposal` drift
/// markers — a fold IS the verdict that resolves them (re-implementation clears
/// stale; an explicit fold adopts). Audit #5.
fn clean_committed_resp(mut resp: Responsibility) -> Responsibility {
    resp.vagrant = None;
    resp.stale = None;
    resp.stale_proposal = None;
    resp
}

/// The committed copy of a planned node folded by `mark_implemented` (whole-node
/// fold). Enforces the "committed never carries review state" invariant: clears
/// the node's own `vagrant`/`stale` markers, DROPS un-adjudicated `vagrant`
/// responsibilities and properties (a bulk fold must not silently commit
/// code-discovered claims that still await an explicit adopt/reject verdict —
/// they stay in the plan), and clears the `stale`/`stale_proposal` drift markers
/// on everything that does fold. Audit #5. Claims tagged to a DIFFERENT change
/// than the node get the same stay-behind treatment as vagrants: they are
/// another task's pending work, and this fold is not their verdict
/// (`change_map` is the plan's ledger — see [`changes::foreign_to_host`]).
fn committed_node_copy(n: &Node, change_map: &HashMap<String, String>) -> Node {
    use diff::ElementKind as EK;
    let host_key = changes::element_key(EK::Node, None, &n.id);
    let mut copy = n.clone();
    copy.vagrant = None;
    copy.stale = None;
    copy.responsibilities = n
        .responsibilities
        .iter()
        .filter(|r| {
            r.vagrant != Some(true)
                && !changes::foreign_to_host(
                    change_map,
                    &host_key,
                    &changes::element_key(EK::Responsibility, None, &r.id),
                )
        })
        .cloned()
        .map(clean_committed_resp)
        .collect();
    copy.properties = n
        .properties
        .iter()
        .filter(|p| {
            p.vagrant != Some(true)
                && !changes::foreign_to_host(
                    change_map,
                    &host_key,
                    &changes::element_key(EK::Property, Some(&n.id), &p.label),
                )
        })
        .cloned()
        .map(|mut p| {
            p.stale = None;
            p
        })
        .collect();
    copy
}

/// The committed copy of a planned group folded into the model. A group has no
/// review markers of its own, but it CAN carry responsibilities (a container
/// group's shared claims — "both surfaces deploy as one Next.js app"), so it
/// gets the same treatment `committed_node_copy` gives a node: drop
/// un-adjudicated `vagrant` claims (they stay in the plan awaiting a verdict),
/// drop claims tagged to a different change (another task's pending work), and
/// clear `stale`/`stale_proposal` on everything that folds. Audit #5 / item A.
fn committed_group_copy(g: &Group, change_map: &HashMap<String, String>) -> Group {
    use diff::ElementKind as EK;
    let host_key = changes::element_key(EK::Group, None, &g.id);
    let mut copy = g.clone();
    copy.responsibilities = g
        .responsibilities
        .iter()
        .filter(|r| {
            r.vagrant != Some(true)
                && !changes::foreign_to_host(
                    change_map,
                    &host_key,
                    &changes::element_key(EK::Responsibility, None, &r.id),
                )
        })
        .cloned()
        .map(clean_committed_resp)
        .collect();
    copy
}

pub fn commit_element(
    r: &ModelRef,
    kind: diff::ElementKind,
    owner_id: Option<&str>,
    id: &str,
) -> Result<(), String> {
    let mut model = read_model_at(r)?;
    // Seeded read: the fold's plan rewrite below persists the draft, and on a
    // never-seeded project the bare fallback would write committed's anchors
    // back as `planned.scry` — minting the shadow draft seeding prevents.
    let planned = read_planned_seeded_at(r)?;
    let mut purge_from_planned = false;
    // Node ids removed by a DELETE fold — the target plus the subtree/links the
    // plan agrees are gone (item C) — and the responsibility ids they carried.
    // Held so the anchor-lockstep step below can GC their orphaned source-map
    // entries (the elements vanish, but their anchors are keyed separately and
    // would otherwise leak).
    let mut deleted_node_ids: Vec<String> = Vec::new();
    let mut deleted_node_resp_ids: Vec<String> = Vec::new();

    match kind {
        diff::ElementKind::Node => {
            match planned.nodes.iter().find(|n| n.id == id) {
                Some(n) => {
                    // An add/reword fold. The node's parent must already live in
                    // committed, or the folded node dangles off a plan-only id:
                    // outline_tree can't reach it from any root, so it vanishes
                    // from every committed read. Fold top-down (the Responsibility
                    // branch makes the same host-residence check). Item B.
                    if let Some(pid) = &n.parent_id {
                        if !model.nodes.iter().any(|p| &p.id == pid && p.id != *id) {
                            return Err(format!(
                                "cannot commit node '{id}': its parent '{pid}' is not in the \
                                 committed model yet (commit the parent first)"
                            ));
                        }
                    }
                    model.nodes.retain(|n| n.id != id);
                    model.nodes.push(committed_node_copy(n, &planned.change_map));
                }
                None => {
                    // A DELETE fold. delete_nodes removed the node, its whole
                    // subtree, the links touching it, and its group memberships
                    // from the PLAN; the fold must mirror that on committed or the
                    // children reparent to a dead id (silently promoted to health
                    // roots), links dangle, and group refs go stale — the exact
                    // orphaning of item C. Scope removal to the subtree the plan
                    // AGREES is gone (absent from the plan), so a still-present
                    // child isn't clobbered into a phantom re-add.
                    let removed: std::collections::HashSet<String> =
                        drift::subtree_ids(&model, id)
                            .into_iter()
                            .filter(|nid| !planned.nodes.iter().any(|n| &n.id == nid))
                            .collect();
                    deleted_node_resp_ids = model
                        .nodes
                        .iter()
                        .filter(|n| removed.contains(&n.id))
                        .flat_map(|n| n.responsibilities.iter().map(|r| r.id.clone()))
                        .collect();
                    model.nodes.retain(|n| !removed.contains(&n.id));
                    model
                        .links
                        .retain(|l| !removed.contains(&l.src) && !removed.contains(&l.dst));
                    for g in &mut model.groups {
                        g.member_ids.retain(|m| !removed.contains(m));
                    }
                    model.boundaries.retain(|k, _| !removed.contains(k));
                    deleted_node_ids = removed.into_iter().collect();
                    purge_from_planned = true;
                }
            }
        }
        diff::ElementKind::Link => {
            model.links.retain(|l| l.id != id);
            match planned.links.iter().find(|l| l.id == id) {
                Some(l) => {
                    // Both endpoints must live in committed or the folded edge
                    // dangles off a plan-only id — the same residence rule the
                    // Node branch enforces for parents (item B). The ready-
                    // dependent fold pre-filters to committed endpoints; this
                    // guards the direct callers (explicit link_ids folds).
                    for end in [&l.src, &l.dst] {
                        if !model.nodes.iter().any(|n| &n.id == end) {
                            return Err(format!(
                                "cannot commit link '{id}': its endpoint '{end}' is not in \
                                 the committed model yet (commit the node first)"
                            ));
                        }
                    }
                    model.links.push(l.clone());
                }
                None => purge_from_planned = true,
            }
        }
        diff::ElementKind::Group => {
            // A group deletion orphans the anchors of the claims it carried, the
            // same way a node deletion does — hold their ids for the GC below.
            if !planned.groups.iter().any(|g| g.id == id) {
                if let Some(g) = model.groups.iter().find(|g| g.id == id) {
                    deleted_node_resp_ids =
                        g.responsibilities.iter().map(|r| r.id.clone()).collect();
                }
            }
            model.groups.retain(|g| g.id != id);
            match planned.groups.iter().find(|g| g.id == id) {
                Some(g) => {
                    // Every member and the anchoring parent must live in
                    // committed, or the folded group references plan-only ids —
                    // the residence rule of item B, guarding direct group_ids
                    // folds (the ready-dependent fold pre-filters members).
                    if let Some(pid) = &g.parent_node_id {
                        if !model.nodes.iter().any(|n| &n.id == pid) {
                            return Err(format!(
                                "cannot commit group '{id}': its anchor node '{pid}' is not \
                                 in the committed model yet (commit the node first)"
                            ));
                        }
                    }
                    if let Some(pgid) = &g.parent_group_id {
                        if !model.groups.iter().any(|x| &x.id == pgid) {
                            return Err(format!(
                                "cannot commit group '{id}': its parent group '{pgid}' is \
                                 not in the committed model yet (commit that group first)"
                            ));
                        }
                    }
                    for mid in &g.member_ids {
                        if !model.nodes.iter().any(|n| &n.id == mid) {
                            return Err(format!(
                                "cannot commit group '{id}': member '{mid}' is not in the \
                                 committed model yet (commit the member first)"
                            ));
                        }
                    }
                    model.groups.push(committed_group_copy(g, &planned.change_map));
                }
                None => purge_from_planned = true,
            }
        }
        diff::ElementKind::Responsibility => {
            for n in &mut model.nodes {
                n.responsibilities.retain(|x| x.id != id);
            }
            for g in &mut model.groups {
                g.responsibilities.retain(|x| x.id != id);
            }
            match find_responsibility(&planned, id) {
                Some((host, resp)) => {
                    let resp = clean_committed_resp(resp);
                    if let Some(n) = model.nodes.iter_mut().find(|n| n.id == host) {
                        n.responsibilities.push(resp);
                    } else if let Some(g) = model.groups.iter_mut().find(|g| g.id == host) {
                        g.responsibilities.push(resp);
                    } else {
                        return Err(format!(
                            "cannot commit responsibility '{id}': its host '{host}' is not in \
                             the committed model yet (commit the host node/group first)"
                        ));
                    }
                }
                None => purge_from_planned = true,
            }
        }
        diff::ElementKind::Property => {
            let owner = owner_id
                .ok_or_else(|| "committing a property requires its owner node id".to_string())?;
            let node = model
                .nodes
                .iter_mut()
                .find(|n| n.id == owner)
                .ok_or_else(|| {
                    format!("cannot commit property '{id}': owner node '{owner}' not in the model")
                })?;
            node.properties.retain(|p| p.label != id);
            // Upsert from planned if present there; absence is a committed delete,
            // already handled by the retain above.
            if let Some(p) = planned
                .nodes
                .iter()
                .find(|n| n.id == owner)
                .and_then(|n| n.properties.iter().find(|p| p.label == id))
            {
                // Committed carries no review markers — an explicit property fold
                // adopts it and resolves any drift flag. Audit #5.
                node.properties.push(SchemaProperty {
                    vagrant: None,
                    stale: None,
                    ..p.clone()
                });
            }
        }
    }

    // Keep the code-side anchor in lockstep with the element being folded.
    // Anchors have a single home: committed owns committed elements', the draft
    // owns only the elements it adds. So folding MOVES a plan-added element's
    // anchor into committed and strips it from the draft; a committed element
    // already keeps its anchor in committed, so it's left untouched — NOT removed
    // just because the draft doesn't carry it (that would silently unanchor a
    // reworded claim). A deletion drops the anchor from committed outright.
    // Node BOUNDARIES follow the same single-home rule (item D) and ride along in
    // `planned_boundary_strip`. VERIFY entries (claim → backing test) are
    // claim-keyed like anchors and move/GC in lockstep with them.
    let mut planned_anchor_strip: Vec<String> = Vec::new();
    let mut planned_verify_strip: Vec<String> = Vec::new();
    let mut planned_boundary_strip: Vec<String> = Vec::new();
    match kind {
        diff::ElementKind::Responsibility => {
            if purge_from_planned {
                model.source_map.remove(id);
                model.verify_map.remove(id);
            } else {
                if let Some(locs) = planned.source_map.get(id) {
                    model.source_map.insert(id.to_string(), locs.clone());
                    planned_anchor_strip.push(id.to_string());
                }
                if let Some(locs) = planned.verify_map.get(id) {
                    model.verify_map.insert(id.to_string(), locs.clone());
                    planned_verify_strip.push(id.to_string());
                }
            }
        }
        diff::ElementKind::Node => {
            if purge_from_planned {
                // Deletion: drop the declaration anchor of every removed node in
                // the subtree AND the anchors of every responsibility they carried
                // (orphaned otherwise). Item C.
                for nid in &deleted_node_ids {
                    model.source_map.remove(nid);
                }
                for rid in &deleted_node_resp_ids {
                    model.source_map.remove(rid);
                    model.verify_map.remove(rid);
                }
                // Committed boundaries for the removed nodes were already dropped
                // in the deletion branch above (item C); strip any draft copy too
                // so nothing lingers to keep winning ownership contests. Item D.
                planned_boundary_strip.extend(deleted_node_ids.iter().cloned());
            } else if let Some(n) = planned.nodes.iter().find(|n| n.id == id) {
                // The node's own declaration anchor, plus every responsibility it
                // carries — committing the node moves the draft's across. Vagrant
                // claims and claims tagged to a different change don't fold
                // (committed_node_copy drops both), so their anchors stay in the
                // draft alongside them. Audit #5.
                let host_key = changes::element_key(diff::ElementKind::Node, None, id);
                for k in std::iter::once(id.to_string()).chain(
                    n.responsibilities
                        .iter()
                        .filter(|r| {
                            r.vagrant != Some(true)
                                && !changes::foreign_to_host(
                                    &planned.change_map,
                                    &host_key,
                                    &changes::element_key(
                                        diff::ElementKind::Responsibility,
                                        None,
                                        &r.id,
                                    ),
                                )
                        })
                        .map(|r| r.id.clone()),
                ) {
                    if let Some(locs) = planned.source_map.get(&k) {
                        model.source_map.insert(k.clone(), locs.clone());
                        planned_anchor_strip.push(k.clone());
                    }
                    if let Some(locs) = planned.verify_map.get(&k) {
                        model.verify_map.insert(k.clone(), locs.clone());
                        planned_verify_strip.push(k);
                    }
                }
                // A plan-added boundary has a single home too: folding the node
                // moves its glob into committed so drifted_scopes — which runs
                // over committed — can see the new container's region, instead of
                // the boundary staying stranded in the draft forever. Item D.
                if let Some(b) = planned.boundaries.get(id) {
                    model.boundaries.insert(id.to_string(), b.clone());
                    planned_boundary_strip.push(id.to_string());
                }
            }
        }
        diff::ElementKind::Group => {
            // A group carries no declaration anchor of its own, but its
            // responsibilities do — move the non-vagrant ones across (or GC them
            // all on a group deletion), mirroring the Node branch. Item A.
            if purge_from_planned {
                for rid in &deleted_node_resp_ids {
                    model.source_map.remove(rid);
                    model.verify_map.remove(rid);
                }
            } else if let Some(g) = planned.groups.iter().find(|g| g.id == id) {
                let host_key = changes::element_key(diff::ElementKind::Group, None, id);
                for r in g.responsibilities.iter().filter(|r| {
                    r.vagrant != Some(true)
                        && !changes::foreign_to_host(
                            &planned.change_map,
                            &host_key,
                            &changes::element_key(diff::ElementKind::Responsibility, None, &r.id),
                        )
                }) {
                    if let Some(locs) = planned.source_map.get(&r.id) {
                        model.source_map.insert(r.id.clone(), locs.clone());
                        planned_anchor_strip.push(r.id.clone());
                    }
                    if let Some(locs) = planned.verify_map.get(&r.id) {
                        model.verify_map.insert(r.id.clone(), locs.clone());
                        planned_verify_strip.push(r.id.clone());
                    }
                }
            }
        }
        _ => {}
    }

    write_model_at(r, &model)?;

    // Sync the verdict markers the fold just resolved into the DRAFT copy too.
    // Committed entered clean (audit #5), but a lingering plan-side flag keeps
    // the canvas offering a verdict on the already-folded element — and
    // answering "re-implement" on one removes the folded claim from committed,
    // silently undoing the fold. A whole-node/group fold clears only what
    // folded: vagrant claims and properties stayed behind (still awaiting
    // adopt/reject), so they keep their markers; a scoped claim/property fold
    // IS the explicit adopt, so `vagrant` clears along with the drift flags.
    let mut p = planned;
    let mut plan_markers_cleared = false;
    // Same stay-behind set as committed_node_copy: markers clear only on what
    // actually folded, so vagrant claims AND claims tagged to another change
    // keep theirs (this fold was not their verdict).
    let ledger = p.change_map.clone();
    let stays = |host_key: &str, elem_key: String| {
        changes::foreign_to_host(&ledger, host_key, &elem_key)
    };
    if !purge_from_planned {
        match kind {
            diff::ElementKind::Node => {
                let host_key = changes::element_key(diff::ElementKind::Node, None, id);
                if let Some(n) = p.nodes.iter_mut().find(|n| n.id == id) {
                    plan_markers_cleared |= n.vagrant.take().is_some() | n.stale.take().is_some();
                    for x in n.responsibilities.iter_mut().filter(|x| {
                        x.vagrant != Some(true)
                            && !stays(
                                &host_key,
                                changes::element_key(
                                    diff::ElementKind::Responsibility,
                                    None,
                                    &x.id,
                                ),
                            )
                    }) {
                        plan_markers_cleared |=
                            x.stale.take().is_some() | x.stale_proposal.take().is_some();
                    }
                    let owner = id;
                    for x in n.properties.iter_mut().filter(|x| {
                        x.vagrant != Some(true)
                            && !stays(
                                &host_key,
                                changes::element_key(
                                    diff::ElementKind::Property,
                                    Some(owner),
                                    &x.label,
                                ),
                            )
                    }) {
                        plan_markers_cleared |= x.stale.take().is_some();
                    }
                }
            }
            diff::ElementKind::Group => {
                let host_key = changes::element_key(diff::ElementKind::Group, None, id);
                if let Some(g) = p.groups.iter_mut().find(|g| g.id == id) {
                    for x in g.responsibilities.iter_mut().filter(|x| {
                        x.vagrant != Some(true)
                            && !stays(
                                &host_key,
                                changes::element_key(
                                    diff::ElementKind::Responsibility,
                                    None,
                                    &x.id,
                                ),
                            )
                    }) {
                        plan_markers_cleared |=
                            x.stale.take().is_some() | x.stale_proposal.take().is_some();
                    }
                }
            }
            diff::ElementKind::Responsibility => {
                if let Some(x) = p
                    .nodes
                    .iter_mut()
                    .flat_map(|n| n.responsibilities.iter_mut())
                    .chain(p.groups.iter_mut().flat_map(|g| g.responsibilities.iter_mut()))
                    .find(|x| x.id == id)
                {
                    plan_markers_cleared |= x.vagrant.take().is_some()
                        | x.stale.take().is_some()
                        | x.stale_proposal.take().is_some();
                }
            }
            diff::ElementKind::Property => {
                if let Some(x) = owner_id
                    .and_then(|o| p.nodes.iter_mut().find(|n| n.id == o))
                    .and_then(|n| n.properties.iter_mut().find(|x| x.label == id))
                {
                    plan_markers_cleared |= x.vagrant.take().is_some() | x.stale.take().is_some();
                }
            }
            diff::ElementKind::Link => {}
        }
    }

    if purge_from_planned {
        match kind {
            diff::ElementKind::Node => p.nodes.retain(|n| n.id != id),
            diff::ElementKind::Link => p.links.retain(|l| l.id != id),
            diff::ElementKind::Group => p.groups.retain(|g| g.id != id),
            diff::ElementKind::Responsibility => {
                for n in &mut p.nodes {
                    n.responsibilities.retain(|x| x.id != id);
                }
                for g in &mut p.groups {
                    g.responsibilities.retain(|x| x.id != id);
                }
            }
            diff::ElementKind::Property => {}
        }
    }
    for k in &planned_anchor_strip {
        p.source_map.remove(k);
    }
    for k in &planned_verify_strip {
        p.verify_map.remove(k);
    }
    for k in &planned_boundary_strip {
        p.boundaries.remove(k);
    }

    // Ledger GC against the post-fold layers: the element just folded (or was
    // delete-folded) no longer diverges, so any change tag naming it is dead.
    // A change this prune empties is finished — record it as folded, with its
    // rationale, before the registry entry disappears.
    let gc = changes::gc(&model, &mut p);
    for meta in &gc.closed {
        changes::record_closed(r, meta, "folded");
    }

    // Rewrite the draft when the fold removes the element (a committed deletion),
    // when it moved an anchor out of the draft into committed, when it resolved
    // review markers on the draft copy, or when it retired change tags — in each
    // case the draft must stop carrying what committed now owns, so the
    // single-home invariant holds.
    if purge_from_planned
        || plan_markers_cleared
        || !planned_anchor_strip.is_empty()
        || !planned_verify_strip.is_empty()
        || !planned_boundary_strip.is_empty()
        || gc.pruned > 0
    {
        let json = serde_json::to_string_pretty(&p).map_err(|e| e.to_string())?;
        write_planned_raw_at(r, &json)?;
    }

    Ok(())
}

/// The committed copy of a plan-only ANCESTOR folded as scaffolding by
/// `commit_plan_only_ancestors`: the node's identity and structure — kind,
/// parent, name, description, technology, external, directives, appearance —
/// WITHOUT its responsibilities or properties. Those stay in the plan as
/// pending build work on a now-committed node (the ordinary incremental-add
/// diff shape), so the committed layer keeps reflecting only what the code
/// actually contains.
fn structure_only_copy(n: &Node) -> Node {
    // The claim/property filtering inside committed_node_copy is moot here —
    // everything it kept is cleared — so no change map is consulted.
    let mut copy = committed_node_copy(n, &HashMap::new());
    copy.responsibilities.clear();
    copy.properties.clear();
    copy
}

/// Commit the plan-only ancestors of `node_id` STRUCTURE-ONLY, so a built leaf
/// can fold in a design-first model without marking the whole tree's unbuilt
/// claims as implemented.
///
/// `commit_element`'s parent-residence guard (item B) refuses to fold a node
/// whose parent is plan-only — correct against accidental orphans, but in a
/// model that has never been committed the recovery ("commit the parent
/// first") is a ladder to force-committing the entire tree, and a whole-node
/// fold of an ancestor would commit every unbuilt claim it carries. This
/// cascade is the honest middle: walk the parent chain root-ward, fold each
/// plan-only ancestor as scaffolding (`structure_only_copy` — no claims, no
/// properties), and leave its pending work in the plan. The caller then folds
/// the target itself with the claims it actually built.
///
/// The walk stops at the first committed ancestor (the chain is anchored
/// there). A parent id present in NEITHER layer still errors — the cascade
/// must not paper over a genuinely dangling reference, which is the case the
/// residence guard exists for. Returns the folded ids, root-ward.
///
/// `include_self` appends the target itself to the structure fold when it is
/// plan-only — for a SCOPED claim fold ("I built these 2 of its 5 claims"),
/// where the host must reach committed but its unfolded claims must not.
pub fn commit_plan_only_ancestors(
    r: &ModelRef,
    node_id: &str,
    include_self: bool,
) -> Result<Vec<String>, String> {
    let mut model = read_model_at(r)?;
    // Seeded read: the plan rewrite below persists the draft (same rationale
    // as commit_element).
    let planned = read_planned_seeded_at(r)?;

    let target = planned
        .nodes
        .iter()
        .find(|n| n.id == node_id)
        .ok_or_else(|| format!("node '{node_id}' not found in the plan"))?;

    // Collect the plan-only stretch of the parent chain, leaf-ward order.
    // Seen-set guards against parent cycles like every chain walker (audit #6).
    let mut chain: Vec<String> = Vec::new();
    let mut seen: std::collections::HashSet<String> =
        std::iter::once(node_id.to_string()).collect();
    let mut cur = target.parent_id.clone();
    while let Some(pid) = cur {
        if model.nodes.iter().any(|n| n.id == pid) {
            break;
        }
        if !seen.insert(pid.clone()) {
            return Err(format!("parent chain of '{node_id}' contains a cycle at '{pid}'"));
        }
        match planned.nodes.iter().find(|n| n.id == pid) {
            Some(p) => {
                chain.push(pid.clone());
                cur = p.parent_id.clone();
            }
            None => {
                return Err(format!(
                    "cannot commit ancestors of '{node_id}': parent '{pid}' exists in neither \
                     layer — fix the parent id before folding"
                ));
            }
        }
    }
    chain.reverse(); // fold root-ward so each lands under a committed parent
    if include_self && !model.nodes.iter().any(|n| n.id == node_id) {
        chain.push(node_id.to_string());
    }
    if chain.is_empty() {
        return Ok(chain);
    }

    // Structure folds root-ward. The node's own declaration anchor and boundary
    // are structural and follow the single-home rule (item D): they move into
    // committed — so drifted_scopes and ownership see the region — and leave
    // the draft. Claim anchors (and claim-keyed verify entries) stay in the
    // draft with their pending claims.
    let mut planned_anchor_strip: Vec<String> = Vec::new();
    let mut planned_boundary_strip: Vec<String> = Vec::new();
    for aid in &chain {
        let n = planned.nodes.iter().find(|n| &n.id == aid).expect("collected from planned");
        model.nodes.retain(|x| &x.id != aid);
        model.nodes.push(structure_only_copy(n));
        if let Some(locs) = planned.source_map.get(aid) {
            model.source_map.insert(aid.clone(), locs.clone());
            planned_anchor_strip.push(aid.clone());
        }
        if let Some(b) = planned.boundaries.get(aid) {
            model.boundaries.insert(aid.clone(), b.clone());
            planned_boundary_strip.push(aid.clone());
        }
    }

    write_model_at(r, &model)?;

    // Draft sync, mirroring commit_element: clear the node-level markers the
    // fold just resolved (folding a vagrant ancestor IS its adopt — the built
    // leaf demonstrably lives inside it), and strip what committed now owns.
    // Claim/property markers are untouched: those claims didn't fold and their
    // verdicts are still pending.
    let mut p = planned;
    let mut plan_markers_cleared = false;
    for aid in &chain {
        if let Some(n) = p.nodes.iter_mut().find(|n| &n.id == aid) {
            plan_markers_cleared |= n.vagrant.take().is_some() | n.stale.take().is_some();
        }
    }
    for k in &planned_anchor_strip {
        p.source_map.remove(k);
    }
    for k in &planned_boundary_strip {
        p.boundaries.remove(k);
    }

    // Ledger GC, mirroring commit_element: a folded ancestor's own node tag is
    // dead (its structure no longer diverges), while the tags on its still-
    // pending claims survive with them.
    let gc = changes::gc(&model, &mut p);
    for meta in &gc.closed {
        changes::record_closed(r, meta, "folded");
    }

    if plan_markers_cleared
        || !planned_anchor_strip.is_empty()
        || !planned_boundary_strip.is_empty()
        || gc.pruned > 0
    {
        let json = serde_json::to_string_pretty(&p).map_err(|e| e.to_string())?;
        write_planned_raw_at(r, &json)?;
    }

    Ok(chain)
}

/// After a node is folded into the committed model, pull in the plan-added links
/// and groups that THIS node's commit has just made "ready". Links and groups
/// have no node id of their own, so `mark_implemented` (keyed by node) can never
/// fold them directly — without this a plan carrying `add_links` output or a
/// group keeps `toImplement` above zero forever and the CLOSE loop never
/// terminates (audit Theme 1 / item A).
///
/// "Ready" is deliberately scoped to dependents INCIDENT to the just-committed
/// node, and only when their other endpoints/members are already committed:
/// - a link is folded when it touches `node_id` (as src or dst) and both of its
///   endpoints live in committed — the edge's code rides with its endpoints, so
///   folding one of them is the natural moment to fold the edge;
/// - a group is folded when it contains `node_id` and every member is committed.
///
/// Deletions are intentionally excluded: folding a link/group removal on an
/// unrelated node fold would commit a removal whose code may not be gone yet.
/// A node-scoped delete cascade owns that path.
pub fn commit_ready_dependents(r: &ModelRef, node_id: &str) -> Result<(), String> {
    let committed = read_model_at(r)?;
    let committed_ids: std::collections::HashSet<&str> =
        committed.nodes.iter().map(|n| n.id.as_str()).collect();
    let committed_group_ids: std::collections::HashSet<&str> =
        committed.groups.iter().map(|g| g.id.as_str()).collect();
    // Nothing became reachable if the node itself isn't committed (e.g. this was
    // a deletion fold, which removes rather than adds).
    if !committed_ids.contains(node_id) {
        return Ok(());
    }
    let planned = read_planned_seeded_at(r)?;
    let plan = diff::diff(&committed, &planned);

    let is_deletion =
        |c: &diff::ElementChange| c.changes.iter().any(|ch| matches!(ch, diff::Change::Deleted));

    let ready_links: Vec<String> = plan
        .changes
        .iter()
        .filter(|c| c.kind == diff::ElementKind::Link && !is_deletion(c))
        .filter_map(|c| planned.links.iter().find(|l| l.id == c.id))
        .filter(|l| l.src == node_id || l.dst == node_id)
        .filter(|l| {
            committed_ids.contains(l.src.as_str()) && committed_ids.contains(l.dst.as_str())
        })
        .map(|l| l.id.clone())
        .collect();
    for id in ready_links {
        commit_element(r, diff::ElementKind::Link, None, &id)?;
    }

    let ready_groups: Vec<String> = plan
        .changes
        .iter()
        .filter(|c| c.kind == diff::ElementKind::Group && !is_deletion(c))
        .filter_map(|c| planned.groups.iter().find(|g| g.id == c.id))
        .filter(|g| g.member_ids.iter().any(|m| m == node_id))
        // A group folds only once EVERY commit_element precondition holds — its
        // members, its anchor node, AND its parent group. Checking members alone
        // let a group with a plan-only parent slip through, so commit_element then
        // errored AFTER this node's own fold already committed: a partial success
        // reported as failure. Match all three residence checks it enforces.
        .filter(|g| {
            g.member_ids.iter().all(|m| committed_ids.contains(m.as_str()))
                && g.parent_node_id.as_deref().is_none_or(|p| committed_ids.contains(p))
                && g.parent_group_id.as_deref().is_none_or(|p| committed_group_ids.contains(p))
        })
        .map(|g| g.id.clone())
        .collect();
    for id in ready_groups {
        commit_element(r, diff::ElementKind::Group, None, &id)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ensure_planned_at, plan_diff_at, read_planned_at, write_planned_at, Kind, Link,
        Source, SourceLocation,
    };

    fn temp_ref() -> (tempfile::TempDir, ModelRef) {
        let dir = tempfile::tempdir().unwrap();
        let r = ModelRef::ProjectLocal(dir.path().to_path_buf());
        (dir, r)
    }

    /// The end-of-build fold must MERGE the draft over committed, not replace
    /// it: the draft is seeded clean of committed's single-home anchors, so a
    /// verbatim write wipes every boundary glob the seed minted (and any
    /// committed-only `source_map` entry). After the fold the draft is re-seeded
    /// clean — no pending plan and no shadow anchors survive the build.
    #[test]
    fn fold_built_model_keeps_committed_anchors_and_reseeds_the_draft() {
        let (_dir, r) = temp_ref();
        let mut committed = ScryModel::new();
        let mut app = mk_node("app", "App", None);
        app.responsibilities.push(mk_resp("resp-1", "serve the API"));
        committed.nodes.push(app);
        committed
            .boundaries
            .insert("app".into(), vec![Source { pattern: "src/**".into(), comment: None }]);
        committed.source_map.insert(
            "resp-1".into(),
            vec![serde_json::from_value(serde_json::json!({ "pattern": "src/api.rs" })).unwrap()],
        );
        committed.verify_map.insert(
            "resp-1".into(),
            vec![serde_json::from_value(serde_json::json!({ "pattern": "tests/api.rs" })).unwrap()],
        );
        write_model_at(&r, &committed).unwrap();

        // The build authors into a clean-seeded draft: a new component plus its
        // own plan-side anchor (fill_container mirrors new keys into the draft).
        ensure_planned_at(&r).unwrap();
        let mut built = read_planned_at(&r).unwrap();
        let mut core = mk_node("core", "Core", Some("app"));
        core.responsibilities.push(mk_resp("resp-2", "parse the input"));
        built.nodes.push(core);
        built.source_map.insert(
            "resp-2".into(),
            vec![serde_json::from_value(serde_json::json!({ "pattern": "src/core.rs" })).unwrap()],
        );

        let folded = fold_built_model(&r, &built).unwrap();

        assert!(folded.boundaries.contains_key("app"), "committed boundary glob survives the fold");
        assert!(folded.source_map.contains_key("resp-1"), "committed-only anchor survives the fold");
        assert!(folded.verify_map.contains_key("resp-1"), "committed-only verify entry survives");
        assert!(folded.source_map.contains_key("resp-2"), "the build's own anchor lands");
        let on_disk = read_model_at(&r).unwrap();
        assert!(on_disk.boundaries.contains_key("app"));
        assert!(on_disk.source_map.contains_key("resp-1"));

        let planned = read_planned_at(&r).unwrap();
        assert!(
            planned.source_map.is_empty()
                && planned.verify_map.is_empty()
                && planned.boundaries.is_empty(),
            "draft re-seeded clean — no shadow anchors"
        );
        assert!(plan_diff_at(&r).unwrap().is_empty(), "no pending plan survives the build");
    }

    /// Verify entries (claim → backing test) ride the fold in lockstep with
    /// claim anchors: committing a plan-added claim MOVES its verify entry into
    /// committed and strips the draft copy; a node-deletion fold GCs the verify
    /// entries of every claim the removed subtree carried.
    #[test]
    fn commit_element_moves_and_gcs_verify_entries() {
        let (_dir, r) = temp_ref();
        let loc = |p: &str| -> Vec<SourceLocation> {
            vec![serde_json::from_value(serde_json::json!({ "pattern": p })).unwrap()]
        };
        let mut m = ScryModel::new();
        m.nodes.push(mk_node("host", "Host", None));
        write_model_at(&r, &m).unwrap();

        // Plan: a new claim on the committed host, anchored and test-backed.
        ensure_planned_at(&r).unwrap();
        let mut planned = read_planned_at(&r).unwrap();
        planned
            .nodes
            .iter_mut()
            .find(|n| n.id == "host")
            .unwrap()
            .responsibilities
            .push(mk_resp("resp-1", "parse the input"));
        planned.source_map.insert("resp-1".into(), loc("src/parse.rs"));
        planned.verify_map.insert("resp-1".into(), loc("tests/parse.rs"));
        write_planned_at(&r, &planned).unwrap();

        commit_element(&r, diff::ElementKind::Responsibility, None, "resp-1").unwrap();

        let committed = read_model_at(&r).unwrap();
        assert_eq!(
            committed.verify_map.get("resp-1"),
            Some(&loc("tests/parse.rs")),
            "the verify entry folds into committed with its claim"
        );
        let draft = read_planned_at(&r).unwrap();
        assert!(
            draft.source_map.is_empty() && draft.verify_map.is_empty(),
            "the draft stops carrying what committed now owns"
        );

        // Plan: delete the host outright; the deletion fold GCs the verify
        // entry along with the claim's anchor (nothing leaks keyed to a dead id).
        let mut planned = read_planned_at(&r).unwrap();
        planned.nodes.retain(|n| n.id != "host");
        write_planned_at(&r, &planned).unwrap();

        commit_element(&r, diff::ElementKind::Node, None, "host").unwrap();

        let committed = read_model_at(&r).unwrap();
        assert!(committed.nodes.is_empty(), "the deletion folded");
        assert!(
            committed.source_map.is_empty() && committed.verify_map.is_empty(),
            "anchor and verify entries of the deleted subtree's claims are GC'd"
        );
    }

    /// A fold on a never-seeded project must not mint a shadow draft: the plan
    /// rewrite persists whatever it read, so it must read the seeded form, not
    /// the anchor-carrying committed fallback.
    #[test]
    fn commit_element_on_an_unseeded_project_seeds_a_clean_draft() {
        let (_dir, r) = temp_ref();
        let mut m = ScryModel::new();
        let mut a = mk_node("a", "A", None);
        let mut resp = mk_resp("resp-1", "do the thing");
        resp.stale = Some(true); // makes the fold's plan rewrite fire
        a.responsibilities.push(resp);
        m.nodes.push(a);
        m.source_map.insert(
            "resp-1".into(),
            vec![serde_json::from_value(serde_json::json!({ "pattern": "a.rs" })).unwrap()],
        );
        write_model_at(&r, &m).unwrap();
        assert!(!r.planned_path().exists());

        commit_element(&r, diff::ElementKind::Node, None, "a").unwrap();

        assert!(r.planned_path().exists(), "the fold seeded the draft");
        let planned = read_planned_at(&r).unwrap();
        assert!(
            planned.source_map.is_empty() && planned.boundaries.is_empty(),
            "the persisted draft carries no shadow of committed's anchors"
        );
        assert!(
            read_model_at(&r).unwrap().source_map.contains_key("resp-1"),
            "committed keeps its anchor"
        );
    }

    /// A direct link fold must not plant an edge whose endpoint is plan-only —
    /// the residence rule of item B, applied to the explicit `link_ids` path
    /// (the ready-dependent fold pre-filters, this guards everyone else).
    #[test]
    fn commit_link_requires_endpoints_in_committed() {
        let (_dir, r) = temp_ref();
        let mut m = ScryModel::new();
        m.nodes.push(mk_node("a", "A", None));
        write_model_at(&r, &m).unwrap();

        ensure_planned_at(&r).unwrap();
        let mut planned = read_planned_at(&r).unwrap();
        planned.nodes.push(mk_node("b", "B", None));
        planned.links.push(Link {
            id: "link-a-b".into(),
            src: "a".into(),
            dst: "b".into(),
            label: "calls".into(),
            method: None,
        });
        write_planned_at(&r, &planned).unwrap();

        let err = commit_element(&r, diff::ElementKind::Link, None, "link-a-b").unwrap_err();
        assert!(err.contains("'b'"), "error names the plan-only endpoint: {err}");
        assert!(read_model_at(&r).unwrap().links.is_empty(), "nothing folded");

        // Endpoint first, then the link folds cleanly.
        commit_element(&r, diff::ElementKind::Node, None, "b").unwrap();
        commit_element(&r, diff::ElementKind::Link, None, "link-a-b").unwrap();
        assert!(read_model_at(&r).unwrap().links.iter().any(|l| l.id == "link-a-b"));
    }

    /// A direct group fold must not reference plan-only members or parents —
    /// same residence rule, for the explicit `group_ids` path.
    #[test]
    fn commit_group_requires_members_in_committed() {
        let (_dir, r) = temp_ref();
        let mut m = ScryModel::new();
        m.nodes.push(mk_node("a", "A", None));
        write_model_at(&r, &m).unwrap();

        ensure_planned_at(&r).unwrap();
        let mut planned = read_planned_at(&r).unwrap();
        planned.nodes.push(mk_node("b", "B", None));
        planned.groups.push(Group {
            id: "grp".into(),
            name: "Pair".into(),
            description: None,
            member_ids: vec!["a".into(), "b".into()],
            parent_group_id: None,
            parent_node_id: None,
            responsibilities: Vec::new(),
            icon: None,
        });
        write_planned_at(&r, &planned).unwrap();

        let err = commit_element(&r, diff::ElementKind::Group, None, "grp").unwrap_err();
        assert!(err.contains("'b'"), "error names the plan-only member: {err}");
        assert!(read_model_at(&r).unwrap().groups.is_empty(), "nothing folded");

        commit_element(&r, diff::ElementKind::Node, None, "b").unwrap();
        commit_element(&r, diff::ElementKind::Group, None, "grp").unwrap();
        assert!(read_model_at(&r).unwrap().groups.iter().any(|g| g.id == "grp"));
    }

    /// commit_ready_dependents must not pick a group whose members are all
    /// committed but whose anchor node is still plan-only — commit_element would
    /// then error AFTER the node fold already committed (a partial success
    /// reported as failure). It skips such a group and returns Ok; once the
    /// anchor commits, the group folds.
    #[test]
    fn ready_dependents_skips_a_group_with_a_plan_only_anchor() {
        let (_dir, r) = temp_ref();
        let mut m = ScryModel::new();
        m.nodes.push(mk_node("root", "Root", None));
        write_model_at(&r, &m).unwrap();

        ensure_planned_at(&r).unwrap();
        let mut planned = read_planned_at(&r).unwrap();
        planned.nodes.push(mk_node("m", "Member", Some("root"))); // foldable: parent committed
        planned.nodes.push(mk_node("anchor", "Anchor", Some("root"))); // stays plan-only
        planned.groups.push(Group {
            id: "grp".into(),
            name: "G".into(),
            description: None,
            member_ids: vec!["m".into()],
            parent_group_id: None,
            parent_node_id: Some("anchor".into()),
            responsibilities: Vec::new(),
            icon: None,
        });
        write_planned_at(&r, &planned).unwrap();

        // Fold the member, then run the ready-dependent sweep: it must NOT error
        // on the group whose anchor is still uncommitted.
        commit_element(&r, diff::ElementKind::Node, None, "m").unwrap();
        commit_ready_dependents(&r, "m").expect("must not fail on a not-yet-ready group");
        assert!(
            !read_model_at(&r).unwrap().groups.iter().any(|g| g.id == "grp"),
            "the group is skipped while its anchor is plan-only"
        );

        // Commit the anchor, sweep again: now every precondition holds, it folds.
        commit_element(&r, diff::ElementKind::Node, None, "anchor").unwrap();
        commit_ready_dependents(&r, "m").unwrap();
        assert!(read_model_at(&r).unwrap().groups.iter().any(|g| g.id == "grp"));
    }

    /// A scoped responsibility fold is the explicit verdict: the PLAN copy's
    /// review markers must clear along with the committed one's, or the canvas
    /// keeps offering a verdict on the already-folded claim — and answering
    /// "re-implement" removes it from committed, silently undoing the fold.
    #[test]
    fn commit_responsibility_clears_the_plan_copies_markers() {
        let (_dir, r) = temp_ref();
        let mut m = ScryModel::new();
        let mut a = mk_node("a", "A", None);
        a.responsibilities.push(mk_resp("resp-1", "do the thing"));
        m.nodes.push(a);
        write_model_at(&r, &m).unwrap();

        ensure_planned_at(&r).unwrap();
        let mut planned = read_planned_at(&r).unwrap();
        {
            let x = &mut planned.nodes[0].responsibilities[0];
            x.stale = Some(true);
            x.stale_proposal = Some("new wording".into());
        }
        write_planned_at(&r, &planned).unwrap();

        commit_element(&r, diff::ElementKind::Responsibility, None, "resp-1").unwrap();

        let committed = read_model_at(&r).unwrap();
        let c = &committed.nodes[0].responsibilities[0];
        assert!(c.stale.is_none() && c.stale_proposal.is_none(), "committed entered clean");
        let planned = read_planned_at(&r).unwrap();
        let p = &planned.nodes[0].responsibilities[0];
        assert!(
            p.stale.is_none() && p.stale_proposal.is_none(),
            "the plan copy no longer awaits a verdict"
        );
    }

    /// A whole-node fold resolves the markers of what it folded — the node's own
    /// flags and its claims' drift flags clear in the plan too — while a vagrant
    /// claim, which the fold deliberately leaves behind, keeps its marker and
    /// stays in the adopt/reject queue.
    #[test]
    fn commit_node_syncs_cleared_markers_into_the_plan() {
        let (_dir, r) = temp_ref();
        let mut m = ScryModel::new();
        let mut a = mk_node("a", "A", None);
        a.responsibilities.push(mk_resp("resp-1", "kept behaviour"));
        m.nodes.push(a);
        write_model_at(&r, &m).unwrap();

        ensure_planned_at(&r).unwrap();
        let mut planned = read_planned_at(&r).unwrap();
        {
            let n = &mut planned.nodes[0];
            n.stale = Some(true);
            n.responsibilities[0].stale = Some(true);
            let mut vag = mk_resp("resp-2", "code-discovered");
            vag.vagrant = Some(true);
            n.responsibilities.push(vag);
        }
        write_planned_at(&r, &planned).unwrap();

        commit_element(&r, diff::ElementKind::Node, None, "a").unwrap();

        let planned = read_planned_at(&r).unwrap();
        let n = &planned.nodes[0];
        assert!(n.stale.is_none(), "the folded node's own drift flag clears in the plan");
        let kept = n.responsibilities.iter().find(|x| x.id == "resp-1").unwrap();
        assert!(kept.stale.is_none(), "the folded claim no longer awaits a verdict");
        let vag = n.responsibilities.iter().find(|x| x.id == "resp-2").unwrap();
        assert_eq!(vag.vagrant, Some(true), "the un-adjudicated vagrant claim stays pending");
        assert!(
            !read_model_at(&r)
                .unwrap()
                .nodes[0]
                .responsibilities
                .iter()
                .any(|x| x.id == "resp-2"),
            "the vagrant claim did not fold"
        );
    }

    fn mk_node(id: &str, name: &str, parent: Option<&str>) -> Node {
        Node {
            id: id.into(),
            kind: Kind::Component,
            name: name.into(),
            vagrant: None,
            stale: None,
            parent_id: parent.map(|s| s.into()),
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

    fn mk_resp(id: &str, statement: &str) -> Responsibility {
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

    /// Committing an added/renamed node folds the draft into the model; once
    /// committed the plan diff for it goes empty.
    #[test]
    fn commit_node_add_and_update() {
        let (_dir, r) = temp_ref();
        let mut m = ScryModel::new();
        m.nodes.push(mk_node("n1", "Old", None));
        write_model_at(&r, &m).unwrap();

        ensure_planned_at(&r).unwrap();
        let mut planned = read_planned_at(&r).unwrap();
        planned.nodes[0].name = "New".into(); // rename n1
        planned.nodes.push(mk_node("n2", "Billing", None)); // add n2
        write_planned_at(&r, &planned).unwrap();

        commit_element(&r, diff::ElementKind::Node, None, "n1").unwrap();
        commit_element(&r, diff::ElementKind::Node, None, "n2").unwrap();

        let model = read_model_at(&r).unwrap();
        assert_eq!(model.nodes.iter().find(|n| n.id == "n1").unwrap().name, "New");
        assert!(model.nodes.iter().any(|n| n.id == "n2"));
        assert!(plan_diff_at(&r).unwrap().is_empty(), "plan clears after commit");
    }

    /// Committing a node that the draft dropped removes it from the model and
    /// purges it from the plan.
    #[test]
    fn commit_node_delete() {
        let (_dir, r) = temp_ref();
        let mut m = ScryModel::new();
        m.nodes.push(mk_node("n1", "Keep", None));
        m.nodes.push(mk_node("n2", "Drop", None));
        write_model_at(&r, &m).unwrap();

        ensure_planned_at(&r).unwrap();
        let mut planned = read_planned_at(&r).unwrap();
        planned.nodes.retain(|n| n.id != "n2"); // delete n2 in the draft
        write_planned_at(&r, &planned).unwrap();

        commit_element(&r, diff::ElementKind::Node, None, "n2").unwrap();

        let model = read_model_at(&r).unwrap();
        assert!(model.nodes.iter().any(|n| n.id == "n1"));
        assert!(!model.nodes.iter().any(|n| n.id == "n2"), "n2 removed from model");
        assert!(plan_diff_at(&r).unwrap().is_empty());
    }

    /// Folding a node DELETION cascades to committed the same way `delete_nodes`
    /// cascaded to the plan: the whole subtree, the links touching it, its group
    /// memberships, boundaries and anchors all go — no orphaned children left to
    /// reparent onto a dead id (promoted to phantom health roots), no dangling
    /// links. Item C.
    #[test]
    fn commit_node_delete_cascades_subtree_links_and_group_refs() {
        let (_dir, r) = temp_ref();
        let mut m = ScryModel::new();
        m.nodes.push(mk_node("p", "Parent", None));
        let mut c = mk_node("c", "Child", Some("p"));
        c.responsibilities.push(mk_resp("r-c", "does a thing"));
        m.nodes.push(c);
        m.nodes.push(mk_node("keep", "Keep", None));
        m.links.push(Link {
            id: "l1".into(),
            src: "c".into(),
            dst: "keep".into(),
            label: "calls".into(),
            method: None,
        });
        m.groups.push(Group {
            id: "grp".into(),
            name: "G".into(),
            description: None,
            member_ids: vec!["c".into(), "keep".into()],
            parent_group_id: None,
            parent_node_id: None,
            responsibilities: Vec::new(),
            icon: None,
        });
        m.source_map.insert(
            "r-c".into(),
            vec![serde_json::from_value(serde_json::json!({ "pattern": "c.rs" })).unwrap()],
        );
        m.boundaries.insert("c".into(), vec![Source { pattern: "c/**".into(), comment: None }]);
        write_model_at(&r, &m).unwrap();

        // Plan: the whole `p` subtree deleted (mirrors delete_nodes on the plan).
        ensure_planned_at(&r).unwrap();
        let mut planned = read_planned_at(&r).unwrap();
        planned.nodes.retain(|n| n.id != "p" && n.id != "c");
        planned.links.clear();
        planned.groups[0].member_ids.retain(|x| x == "keep");
        planned.source_map.remove("r-c");
        planned.boundaries.remove("c");
        write_planned_at(&r, &planned).unwrap();

        // Fold the deletion of the subtree root.
        commit_element(&r, diff::ElementKind::Node, None, "p").unwrap();

        let model = read_model_at(&r).unwrap();
        assert!(!model.nodes.iter().any(|n| n.id == "p" || n.id == "c"), "subtree removed");
        assert!(model.nodes.iter().any(|n| n.id == "keep"), "untouched sibling kept");
        assert!(model.links.is_empty(), "dangling link dropped");
        assert_eq!(model.groups[0].member_ids, vec!["keep"], "dead group ref pruned");
        assert!(!model.source_map.contains_key("r-c"), "orphaned anchor GC'd");
        assert!(!model.boundaries.contains_key("c"), "deleted node's boundary GC'd");
        assert!(plan_diff_at(&r).unwrap().is_empty(), "committed reconciled to the plan");
    }

    /// The delete cascade only removes what the plan AGREES is gone: a child kept
    /// in the plan (e.g. reparented out before the delete) survives the fold of
    /// its old parent rather than being clobbered into a phantom re-add. Item C.
    #[test]
    fn commit_node_delete_spares_a_child_still_in_the_plan() {
        let (_dir, r) = temp_ref();
        let mut m = ScryModel::new();
        m.nodes.push(mk_node("p", "Parent", None));
        m.nodes.push(mk_node("c", "Child", Some("p")));
        write_model_at(&r, &m).unwrap();

        // Plan: `p` deleted, but `c` reparented to root and kept.
        ensure_planned_at(&r).unwrap();
        let mut planned = read_planned_at(&r).unwrap();
        planned.nodes.retain(|n| n.id != "p");
        planned.nodes.iter_mut().find(|n| n.id == "c").unwrap().parent_id = None;
        write_planned_at(&r, &planned).unwrap();

        commit_element(&r, diff::ElementKind::Node, None, "p").unwrap();

        let model = read_model_at(&r).unwrap();
        assert!(!model.nodes.iter().any(|n| n.id == "p"), "parent deleted");
        assert!(model.nodes.iter().any(|n| n.id == "c"), "kept child not clobbered");
    }

    /// A plan-added container's boundary has a single home: folding the node
    /// moves its glob into committed (so drifted_scopes, which runs over
    /// committed, can see the region) and strips it from the draft. Item D.
    #[test]
    fn commit_node_folds_its_plan_added_boundary_into_committed() {
        let (_dir, r) = temp_ref();
        write_model_at(&r, &ScryModel::new()).unwrap();

        // Plan: a new container carrying a boundary glob (as fill_container writes).
        ensure_planned_at(&r).unwrap();
        let mut planned = read_planned_at(&r).unwrap();
        planned.nodes.push(mk_node("box", "API", None));
        planned
            .boundaries
            .insert("box".into(), vec![Source { pattern: "api/**".into(), comment: None }]);
        write_planned_at(&r, &planned).unwrap();

        commit_element(&r, diff::ElementKind::Node, None, "box").unwrap();

        // Committed now owns the boundary…
        let model = read_model_at(&r).unwrap();
        assert_eq!(
            model.boundaries.get("box").expect("boundary folded into committed")[0].pattern,
            "api/**"
        );
        // …and the draft no longer carries it (single home).
        let plan = read_planned_at(&r).unwrap();
        assert!(!plan.boundaries.contains_key("box"), "boundary left the draft");
        assert!(plan_diff_at(&r).unwrap().is_empty());
    }

    /// A node can't fold before its parent: committing a child whose parent is
    /// still plan-only would dangle the child off a non-existent committed id
    /// (invisible to outline_tree). The fold errors; folding parent-then-child
    /// succeeds. Item B.
    #[test]
    fn commit_node_requires_parent_in_committed() {
        let (_dir, r) = temp_ref();
        write_model_at(&r, &ScryModel::new()).unwrap();

        ensure_planned_at(&r).unwrap();
        let mut planned = read_planned_at(&r).unwrap();
        planned.nodes.push(mk_node("p", "Parent", None));
        planned.nodes.push(mk_node("c", "Child", Some("p")));
        write_planned_at(&r, &planned).unwrap();

        // Child first: rejected — its parent isn't committed yet.
        let err = commit_element(&r, diff::ElementKind::Node, None, "c").unwrap_err();
        assert!(err.contains("parent 'p'"), "error names the missing parent: {err}");
        assert!(
            !read_model_at(&r).unwrap().nodes.iter().any(|n| n.id == "c"),
            "child not committed while its parent is plan-only"
        );

        // Parent then child: both land, and the plan clears.
        commit_element(&r, diff::ElementKind::Node, None, "p").unwrap();
        commit_element(&r, diff::ElementKind::Node, None, "c").unwrap();
        let model = read_model_at(&r).unwrap();
        assert!(model.nodes.iter().any(|n| n.id == "p"));
        assert!(model.nodes.iter().any(|n| n.id == "c"));
        assert!(plan_diff_at(&r).unwrap().is_empty());
    }

    /// The scaffolding cascade behind an opt-in ancestor commit: in a
    /// design-first model, a built leaf's plan-only ancestor chain folds
    /// STRUCTURE-ONLY — ancestors land in committed without their unbuilt
    /// claims (those stay pending in the plan), structural anchors
    /// (declaration + boundary) move to their single committed home, and the
    /// leaf itself then folds normally.
    #[test]
    fn commit_plan_only_ancestors_folds_structure_only() {
        let (_dir, r) = temp_ref();
        write_model_at(&r, &ScryModel::new()).unwrap();

        ensure_planned_at(&r).unwrap();
        let mut planned = read_planned_at(&r).unwrap();
        let mut sys = mk_node("sys", "System", None);
        sys.responsibilities.push(mk_resp("resp-s", "own the domain"));
        let mut app = mk_node("app", "App", Some("sys"));
        app.responsibilities.push(mk_resp("resp-a", "serve the api"));
        let mut leaf = mk_node("leaf", "Feature", Some("app"));
        leaf.responsibilities.push(mk_resp("resp-l", "do the built thing"));
        planned.nodes.extend([sys, app, leaf]);
        planned
            .boundaries
            .insert("app".into(), vec![Source { pattern: "api/**".into(), comment: None }]);
        planned.source_map.insert(
            "app".into(),
            vec![SourceLocation {
                pattern: "api/mod.rs".into(),
                symbol: None,
                line: None,
                end_line: None,
                command: None,
            }],
        );
        write_planned_at(&r, &planned).unwrap();

        let folded = commit_plan_only_ancestors(&r, "leaf", false).unwrap();
        assert_eq!(folded, vec!["sys".to_string(), "app".to_string()], "root-ward order");

        // Ancestors are committed as scaffolding: structure without claims.
        let model = read_model_at(&r).unwrap();
        let sys = model.nodes.iter().find(|n| n.id == "sys").expect("sys committed");
        let app = model.nodes.iter().find(|n| n.id == "app").expect("app committed");
        assert!(sys.responsibilities.is_empty(), "unbuilt claims did not fold");
        assert!(app.responsibilities.is_empty(), "unbuilt claims did not fold");
        assert!(!model.nodes.iter().any(|n| n.id == "leaf"), "the target itself is not folded");

        // Structural anchors moved to their single committed home…
        assert_eq!(model.boundaries.get("app").expect("boundary folded")[0].pattern, "api/**");
        assert!(model.source_map.contains_key("app"), "declaration anchor folded");
        let plan = read_planned_at(&r).unwrap();
        assert!(!plan.boundaries.contains_key("app"), "boundary left the draft");
        assert!(!plan.source_map.contains_key("app"), "anchor left the draft");

        // …and the leaf now folds normally, carrying only its own built claim,
        // while the ancestors' unbuilt claims remain the pending plan work.
        commit_element(&r, diff::ElementKind::Node, None, "leaf").unwrap();
        let model = read_model_at(&r).unwrap();
        let leaf = model.nodes.iter().find(|n| n.id == "leaf").unwrap();
        assert_eq!(leaf.responsibilities.len(), 1, "built claim folded with the leaf");
        assert_eq!(
            plan_diff_at(&r).unwrap().changes.len(),
            2,
            "exactly the ancestors' unbuilt claims stay pending"
        );
    }

    /// No plan-only ancestors → no-op: the cascade returns empty and commits
    /// nothing.
    #[test]
    fn commit_plan_only_ancestors_noop_when_parent_committed() {
        let (_dir, r) = temp_ref();
        let mut m = ScryModel::new();
        m.nodes.push(mk_node("p", "Parent", None));
        write_model_at(&r, &m).unwrap();

        ensure_planned_at(&r).unwrap();
        let mut planned = read_planned_at(&r).unwrap();
        planned.nodes.push(mk_node("c", "Child", Some("p")));
        write_planned_at(&r, &planned).unwrap();

        assert!(commit_plan_only_ancestors(&r, "c", false).unwrap().is_empty());
        assert_eq!(read_model_at(&r).unwrap().nodes.len(), 1, "committed untouched");
    }

    /// A parent id in NEITHER layer is a dangling reference, not scaffolding —
    /// the cascade refuses instead of papering it over (the protective case
    /// the residence guard exists for).
    #[test]
    fn commit_plan_only_ancestors_rejects_a_dangling_parent() {
        let (_dir, r) = temp_ref();
        write_model_at(&r, &ScryModel::new()).unwrap();

        ensure_planned_at(&r).unwrap();
        let mut planned = read_planned_at(&r).unwrap();
        planned.nodes.push(mk_node("c", "Child", Some("ghost")));
        write_planned_at(&r, &planned).unwrap();

        let err = commit_plan_only_ancestors(&r, "c", false).unwrap_err();
        assert!(err.contains("'ghost'"), "error names the dangling parent: {err}");
        assert!(read_model_at(&r).unwrap().nodes.is_empty(), "nothing committed");
    }

    /// A parent cycle in the plan terminates with an error instead of hanging —
    /// the same seen-set rule every chain walker carries (audit #6).
    #[test]
    fn commit_plan_only_ancestors_survives_a_parent_cycle() {
        let (_dir, r) = temp_ref();
        write_model_at(&r, &ScryModel::new()).unwrap();

        ensure_planned_at(&r).unwrap();
        let mut planned = read_planned_at(&r).unwrap();
        planned.nodes.push(mk_node("a", "A", Some("b")));
        planned.nodes.push(mk_node("b", "B", Some("a")));
        planned.nodes.push(mk_node("c", "C", Some("a")));
        write_planned_at(&r, &planned).unwrap();

        let err = commit_plan_only_ancestors(&r, "c", false).unwrap_err();
        assert!(err.contains("cycle"), "cycle detected: {err}");
    }

    /// Committing a responsibility that the draft moved to another host lands it
    /// under the new host in the model and removes it from the old one.
    #[test]
    fn commit_responsibility_move() {
        let (_dir, r) = temp_ref();
        let mut m = ScryModel::new();
        let mut a = mk_node("a", "A", None);
        a.responsibilities.push(mk_resp("resp-1", "do the thing"));
        m.nodes.push(a);
        m.nodes.push(mk_node("b", "B", None));
        write_model_at(&r, &m).unwrap();

        ensure_planned_at(&r).unwrap();
        let mut planned = read_planned_at(&r).unwrap();
        // move resp-1 from a to b in the draft
        let resp = planned.nodes[0].responsibilities.remove(0);
        planned.nodes[1].responsibilities.push(resp);
        write_planned_at(&r, &planned).unwrap();

        commit_element(&r, diff::ElementKind::Responsibility, None, "resp-1").unwrap();

        let model = read_model_at(&r).unwrap();
        let a = model.nodes.iter().find(|n| n.id == "a").unwrap();
        let b = model.nodes.iter().find(|n| n.id == "b").unwrap();
        assert!(a.responsibilities.is_empty(), "resp left the old host");
        assert_eq!(b.responsibilities.len(), 1, "resp landed on the new host");
        assert!(plan_diff_at(&r).unwrap().is_empty());
    }

    /// Committing a property upserts it by `(owner, label)`.
    #[test]
    fn commit_property_update() {
        let (_dir, r) = temp_ref();
        let mut m = ScryModel::new();
        let mut a = mk_node("a", "A", None);
        a.properties.push(SchemaProperty {
            label: "email".into(),
            description: "old".into(),
            vagrant: None,
            stale: None,
            last_touched_at: None,
        });
        m.nodes.push(a);
        write_model_at(&r, &m).unwrap();

        ensure_planned_at(&r).unwrap();
        let mut planned = read_planned_at(&r).unwrap();
        planned.nodes[0].properties[0].description = "new".into();
        write_planned_at(&r, &planned).unwrap();

        commit_element(&r, diff::ElementKind::Property, Some("a"), "email").unwrap();

        let model = read_model_at(&r).unwrap();
        assert_eq!(model.nodes[0].properties[0].description, "new");
        assert!(plan_diff_at(&r).unwrap().is_empty());
    }

    /// A whole-node fold must not carry un-adjudicated review state into the
    /// source of truth (audit #5): `stale` drift flags clear on the claims that
    /// fold, and `vagrant` code-discovered claims/properties are LEFT in the plan
    /// awaiting an explicit adopt/reject verdict — not silently committed.
    #[test]
    fn commit_node_clears_stale_and_leaves_vagrant_pending() {
        let (_dir, r) = temp_ref();
        let mut m = ScryModel::new();
        let mut n = mk_node("n", "Svc", None);
        n.responsibilities.push(mk_resp("resp-1", "serves requests"));
        m.nodes.push(n);
        write_model_at(&r, &m).unwrap();

        // Plan: resp-1 went stale (code regressed, then re-implemented), a vagrant
        // claim resp-2 was drift-discovered, and a vagrant property was too.
        ensure_planned_at(&r).unwrap();
        let mut planned = read_planned_at(&r).unwrap();
        {
            let pn = &mut planned.nodes[0];
            pn.responsibilities[0].stale = Some(true);
            pn.responsibilities[0].stale_proposal = Some("serves v2 requests".into());
            let mut vagrant = mk_resp("resp-2", "also logs metrics");
            vagrant.vagrant = Some(true);
            pn.responsibilities.push(vagrant);
            pn.properties.push(SchemaProperty {
                label: "region".into(),
                description: String::new(),
                vagrant: Some(true),
                stale: None,
                last_touched_at: None,
            });
        }
        write_planned_at(&r, &planned).unwrap();

        commit_element(&r, diff::ElementKind::Node, None, "n").unwrap();

        let model = read_model_at(&r).unwrap();
        let cn = model.nodes.iter().find(|x| x.id == "n").unwrap();
        // The stale claim folded, with its drift markers cleared.
        let r1 = cn.responsibilities.iter().find(|x| x.id == "resp-1").unwrap();
        assert_eq!(r1.stale, None, "stale flag cleared on fold");
        assert_eq!(r1.stale_proposal, None, "stale proposal cleared on fold");
        // The vagrant claim and property did NOT bypass review into committed.
        assert!(
            !cn.responsibilities.iter().any(|x| x.id == "resp-2"),
            "vagrant claim not silently committed"
        );
        assert!(cn.properties.is_empty(), "vagrant property not silently committed");

        // They stay in the plan, still pending an adopt/reject verdict.
        let plan = read_planned_at(&r).unwrap();
        let pn = plan.nodes.iter().find(|x| x.id == "n").unwrap();
        assert!(
            pn.responsibilities.iter().any(|x| x.id == "resp-2" && x.vagrant == Some(true)),
            "vagrant claim still pending in the plan"
        );
        assert!(
            pn.properties.iter().any(|p| p.label == "region" && p.vagrant == Some(true)),
            "vagrant property still pending in the plan"
        );
    }

    /// A plan-added link folds only once BOTH its endpoints are committed, and it
    /// folds as a side effect of the node fold — no separate id to fold by. This
    /// is what makes the CLOSE loop terminable for a plan carrying `add_links`
    /// output: after folding both nodes, the plan diff reaches empty. Item A.
    #[test]
    fn ready_link_folds_when_its_second_endpoint_commits() {
        let (_dir, r) = temp_ref();
        write_model_at(&r, &ScryModel::new()).unwrap();

        ensure_planned_at(&r).unwrap();
        let mut planned = read_planned_at(&r).unwrap();
        planned.nodes.push(mk_node("a", "A", None));
        planned.nodes.push(mk_node("b", "B", None));
        planned.links.push(Link {
            id: "l1".into(),
            src: "a".into(),
            dst: "b".into(),
            label: "calls".into(),
            method: None,
        });
        write_planned_at(&r, &planned).unwrap();

        // Fold node `a`. The link is incident but `b` isn't committed yet, so it
        // stays pending — folding an edge whose far end has no code would be wrong.
        commit_element(&r, diff::ElementKind::Node, None, "a").unwrap();
        commit_ready_dependents(&r, "a").unwrap();
        assert!(
            !read_model_at(&r).unwrap().links.iter().any(|l| l.id == "l1"),
            "link waits until both endpoints are committed"
        );

        // Fold node `b` — now both endpoints live in committed, so the link rides
        // in on this fold and the plan diff clears.
        commit_element(&r, diff::ElementKind::Node, None, "b").unwrap();
        commit_ready_dependents(&r, "b").unwrap();
        assert!(
            read_model_at(&r).unwrap().links.iter().any(|l| l.id == "l1"),
            "link folded once its second endpoint committed"
        );
        assert!(plan_diff_at(&r).unwrap().is_empty(), "CLOSE loop terminates");
    }

    /// Folding a group (once its members are committed) carries the group's own
    /// responsibilities into committed the same way a node fold does: it drops
    /// un-adjudicated vagrant claims, clears stale markers, and moves the anchor
    /// of the folded claim across. Item A + audit #5.
    #[test]
    fn ready_group_folds_and_cleans_its_responsibilities() {
        let (_dir, r) = temp_ref();
        let mut m = ScryModel::new();
        m.nodes.push(mk_node("a", "A", None));
        write_model_at(&r, &m).unwrap();

        ensure_planned_at(&r).unwrap();
        let mut planned = read_planned_at(&r).unwrap();
        let mut claim = mk_resp("g-resp", "both surfaces deploy as one app");
        claim.stale = Some(true);
        let mut vagrant = mk_resp("g-vagrant", "drift-discovered claim");
        vagrant.vagrant = Some(true);
        planned.groups.push(Group {
            id: "grp".into(),
            name: "Payload".into(),
            description: None,
            member_ids: vec!["a".into()],
            parent_group_id: None,
            parent_node_id: None,
            responsibilities: vec![claim, vagrant],
            icon: None,
        });
        planned.source_map.insert(
            "g-resp".into(),
            vec![serde_json::from_value(serde_json::json!({
                "pattern": "app/deploy.ts", "symbol": "deploy"
            }))
            .unwrap()],
        );
        write_planned_at(&r, &planned).unwrap();

        // Fold the member node; the group rides in on that fold.
        commit_element(&r, diff::ElementKind::Node, None, "a").unwrap();
        commit_ready_dependents(&r, "a").unwrap();

        let model = read_model_at(&r).unwrap();
        let g = model.groups.iter().find(|g| g.id == "grp").expect("group folded in");
        let folded = g.responsibilities.iter().find(|x| x.id == "g-resp").unwrap();
        assert_eq!(folded.stale, None, "stale cleared on the folded claim");
        assert!(
            !g.responsibilities.iter().any(|x| x.id == "g-vagrant"),
            "vagrant claim did not bypass review into committed"
        );
        assert_eq!(
            model.source_map.get("g-resp").expect("anchor carried across")[0].pattern,
            "app/deploy.ts"
        );

        // The vagrant claim stays in the plan awaiting a verdict.
        let plan = read_planned_at(&r).unwrap();
        let pg = plan.groups.iter().find(|g| g.id == "grp").unwrap();
        assert!(
            pg.responsibilities.iter().any(|x| x.id == "g-vagrant" && x.vagrant == Some(true)),
            "vagrant group claim still pending in the plan"
        );
    }

    /// Folding a minted chain (component → symbol) then its responsibility — the
    /// adopt path — lands every rung in the committed model AND carries the code
    /// anchor across, so the adopted claim is mapped (and a later deletion work
    /// item could point at the code).
    #[test]
    fn commit_folds_chain_and_carries_source_anchor() {
        let (_dir, r) = temp_ref();
        let node = |v: serde_json::Value| serde_json::from_value::<Node>(v).unwrap();

        // Committed: just a container.
        let mut m = ScryModel::new();
        m.nodes.push(node(serde_json::json!({ "id": "c", "kind": "container", "name": "API" })));
        write_model_at(&r, &m).unwrap();

        // Plan: container + a new component + a new symbol carrying a claim,
        // anchored to code in the plan's source map.
        let mut planned = m.clone();
        planned.nodes.push(node(serde_json::json!({
            "id": "comp", "kind": "component", "name": "Admin", "parentId": "c"
        })));
        planned.nodes.push(node(serde_json::json!({
            "id": "sym", "kind": "symbol", "name": "admin_handler", "parentId": "comp",
            "responsibilities": [{ "id": "r1", "statement": "exposes admin endpoint" }],
        })));
        planned.source_map.insert(
            "r1".into(),
            vec![serde_json::from_value(serde_json::json!({
                "pattern": "api/admin.rs", "symbol": "admin_handler"
            }))
            .unwrap()],
        );
        write_planned_at(&r, &planned).unwrap();

        // Fold root→leaf, then the responsibility — the host node must exist first.
        commit_element(&r, diff::ElementKind::Node, None, "comp").unwrap();
        commit_element(&r, diff::ElementKind::Node, None, "sym").unwrap();
        commit_element(&r, diff::ElementKind::Responsibility, None, "r1").unwrap();

        let model = read_model_at(&r).unwrap();
        assert!(model.nodes.iter().any(|n| n.id == "comp"), "component folded in");
        let sym = model.nodes.iter().find(|n| n.id == "sym").expect("symbol folded in");
        assert!(sym.responsibilities.iter().any(|x| x.id == "r1"), "claim on the symbol");
        assert_eq!(
            model.source_map.get("r1").expect("anchor carried into committed")[0].pattern,
            "api/admin.rs"
        );
        assert!(plan_diff_at(&r).unwrap().is_empty(), "plan and model agree after the fold");
    }

    /// Dedup invariant: a committed claim's anchor lives only in committed, so
    /// the draft does not carry it. Folding a reworded version of that claim must
    /// KEEP the committed anchor — not drop it just because the draft has no copy
    /// (pre-dedup the draft mirrored every anchor, which masked this path).
    #[test]
    fn fold_keeps_committed_anchor_when_draft_does_not_carry_it() {
        let (_dir, r) = temp_ref();
        let node = |v: serde_json::Value| serde_json::from_value::<Node>(v).unwrap();

        // Committed: a leaf symbol with an anchored claim.
        let mut m = ScryModel::new();
        m.nodes.push(node(serde_json::json!({
            "id": "sym", "kind": "symbol", "name": "h",
            "responsibilities": [{ "id": "r1", "statement": "old wording" }],
        })));
        m.source_map.insert(
            "r1".into(),
            vec![serde_json::from_value(serde_json::json!({
                "pattern": "src/h.rs", "symbol": "h"
            }))
            .unwrap()],
        );
        write_model_at(&r, &m).unwrap();

        // Draft: the SAME claim reworded (an authored change) but with NO anchor
        // of its own — committed owns it; the draft overlays only what it adds.
        let mut planned = m.clone();
        planned.source_map.clear();
        for n in &mut planned.nodes {
            for resp in &mut n.responsibilities {
                if resp.id == "r1" {
                    resp.statement = "new wording".into();
                }
            }
        }
        write_planned_at(&r, &planned).unwrap();

        commit_element(&r, diff::ElementKind::Responsibility, None, "r1").unwrap();

        let model = read_model_at(&r).unwrap();
        let resp = model
            .nodes
            .iter()
            .flat_map(|n| &n.responsibilities)
            .find(|x| x.id == "r1")
            .expect("claim still committed");
        assert_eq!(resp.statement, "new wording", "the reword folded in");
        assert_eq!(
            model.source_map.get("r1").expect("committed anchor preserved")[0].pattern,
            "src/h.rs",
            "folding the reword must not unanchor the committed claim"
        );
    }

    /// Deleting a node folds out its own anchor AND the anchors of the
    /// responsibilities it carried — none are left orphaned in the committed
    /// source map.
    #[test]
    fn commit_node_deletion_gcs_responsibility_anchors() {
        let (_dir, r) = temp_ref();
        let node = |v: serde_json::Value| serde_json::from_value::<Node>(v).unwrap();

        // Committed: a symbol carrying a claim, both anchored to code.
        let mut m = ScryModel::new();
        m.nodes.push(node(serde_json::json!({ "id": "c", "kind": "container", "name": "API" })));
        m.nodes.push(node(serde_json::json!({
            "id": "sym", "kind": "symbol", "name": "admin_handler", "parentId": "c",
            "responsibilities": [{ "id": "r1", "statement": "exposes admin endpoint" }],
        })));
        let loc = |p: &str| vec![serde_json::from_value::<SourceLocation>(
            serde_json::json!({ "pattern": p }),
        )
        .unwrap()];
        m.source_map.insert("sym".into(), loc("api/admin.rs")); // the node's decl anchor
        m.source_map.insert("r1".into(), loc("api/admin.rs")); // the claim's anchor
        write_model_at(&r, &m).unwrap();

        // Plan drops the symbol → committing the deletion must GC both anchors.
        let mut planned = m.clone();
        planned.nodes.retain(|n| n.id != "sym");
        write_planned_at(&r, &planned).unwrap();

        commit_element(&r, diff::ElementKind::Node, None, "sym").unwrap();

        let model = read_model_at(&r).unwrap();
        assert!(!model.nodes.iter().any(|n| n.id == "sym"), "symbol deleted");
        assert!(model.source_map.get("sym").is_none(), "node anchor GC'd");
        assert!(
            model.source_map.get("r1").is_none(),
            "the deleted node's responsibility anchor must not be left orphaned"
        );
    }
}
