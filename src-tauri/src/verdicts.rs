/// The minted vagrant chain at and above `host_id`: the host plus each ancestor
/// that is itself vagrant, ordered root→leaf. Walks up while nodes are vagrant,
/// stopping at the first committed ancestor. Empty when the host is already a
/// committed node (the finding was routed onto an existing node, not a fresh
/// mint).
fn vagrant_chain(planned: &scryer_core::ScryModel, host_id: &str) -> Vec<String> {
    let mut chain = Vec::new();
    let mut cur = Some(host_id.to_string());
    while let Some(id) = cur {
        match planned.nodes.iter().find(|n| n.id == id) {
            Some(n) if n.vagrant == Some(true) => {
                chain.push(n.id.clone());
                cur = n.parent_id.clone();
            }
            _ => break,
        }
    }
    chain.reverse();
    chain
}

/// What a vagrant fold captured — the host node, the claim's text and source
/// anchor for the timeline, and the minted chain it committed (for reject's plan
/// cleanup).
struct FoldedVagrant {
    host_id: String,
    statement: String,
    source: Option<scryer_core::SourceLocation>,
    chain: Vec<String>,
}

/// Clear the vagrant flags on a code-discovered responsibility and its minted
/// host chain, then FOLD the chain (root→leaf) and the responsibility into the
/// committed model. Shared by adopt (which keeps it) and reject (which then
/// schedules its deletion). The chain must commit before the responsibility, per
/// `commit_element`'s host-must-exist rule — a freshly minted symbol/component
/// has no committed home until its rungs are folded first.
fn fold_vagrant(
    model_ref: &scryer_core::ModelRef,
    resp_id: &str,
) -> Result<FoldedVagrant, String> {
    use scryer_core::diff::ElementKind;

    // Clear the vagrant flag in the plan and capture host + statement, so the
    // copy `commit_element` folds is a clean, adopted claim (it folds verbatim).
    let mut planned = scryer_core::read_planned_seeded_at(model_ref)?;
    let mut host_id = None;
    let mut statement = None;
    for n in &mut planned.nodes {
        if let Some(r) = n.responsibilities.iter_mut().find(|r| r.id == resp_id) {
            r.vagrant = None;
            host_id = Some(n.id.clone());
            statement = Some(r.statement.clone());
            break;
        }
    }
    if host_id.is_none() {
        for g in &mut planned.groups {
            if let Some(r) = g.responsibilities.iter_mut().find(|r| r.id == resp_id) {
                r.vagrant = None;
                host_id = Some(g.id.clone());
                statement = Some(r.statement.clone());
                break;
            }
        }
    }
    let (Some(host_id), Some(statement)) = (host_id, statement) else {
        return Err(format!("Responsibility '{resp_id}' not found in the plan"));
    };
    let source = planned
        .source_map
        .get(resp_id)
        .and_then(|locs| locs.first())
        .cloned();
    // The minted rungs above the responsibility (a new component, the symbol for
    // a new function, …) — clear their vagrant flag so they fold as clean nodes.
    let chain = vagrant_chain(&planned, &host_id);
    for id in &chain {
        if let Some(n) = planned.nodes.iter_mut().find(|n| &n.id == id) {
            n.vagrant = None;
        }
    }
    scryer_core::write_planned_at(model_ref, &planned)?;

    // Fold the chain root→leaf (each parent committed before its child), then the
    // responsibility onto its now-committed host.
    for id in &chain {
        scryer_core::commit_element(model_ref, ElementKind::Node, None, id)?;
    }
    scryer_core::commit_element(model_ref, ElementKind::Responsibility, None, resp_id)?;

    Ok(FoldedVagrant { host_id, statement, source, chain })
}

/// Adopt a code-discovered (vagrant) responsibility: clear its `vagrant` flag in
/// the plan and FOLD it — together with any minted host chain (a new component,
/// the symbol for a new function) — straight into the committed model. Ordinary
/// plan edits are committed only by the agent (`mark_implemented`), but a vagrant
/// claim is source-anchored to code that ALREADY EXISTS — adopting it IS the
/// commit, there is nothing left to implement. This is the one sanctioned case of
/// the canvas writing the committed model, because it is itself a
/// reconcile-to-existing-code (the same direction as `reconcile_drift`), not the
/// human authoring intent ahead of the code. The file watcher then refreshes both
/// layers in the UI.
#[tauri::command]
pub(crate) fn adopt_responsibility(cwd: String, resp_id: String) -> Result<(), String> {
    let model_ref = scryer_core::ModelRef::ProjectLocal(std::path::PathBuf::from(&cwd));
    // Seeding turned this verdict's read into a writer, so it must hold the model
    // lock across the whole read-modify-write — otherwise the canvas races the
    // agent's MCP process and the two writers clobber each other.
    let _lock = scryer_core::lock_model(&model_ref)?;
    let folded = fold_vagrant(&model_ref, &resp_id)?;

    // Keep the legacy baseline in step and log the fold as a "took code" event,
    // mirroring `mark_implemented`'s Impl event so it lands on the History tab.
    if let Ok(after) = scryer_core::read_model_at(&model_ref) {
        let _ = scryer_core::save_baseline_at(&model_ref, &after);
    }
    let mut row = scryer_core::history::EventRow::new("+", folded.statement);
    if let Some(loc) = folded.source {
        row = row.with_source(loc);
    }
    let _ = scryer_core::history::append_event(
        &model_ref,
        &scryer_core::history::HistoryEvent::new(
            scryer_core::drift::now_secs(),
            scryer_core::history::EventKind::Impl,
            &folded.host_id,
            "took code",
        )
        .with_rows(vec![row]),
    );
    Ok(())
}

/// Reject a code-discovered (vagrant) responsibility: the behaviour should not be
/// in the model. Rather than silently dropping it from the plan — which leaves the
/// code untouched for the next drift check to re-propose — we FOLD it (and any
/// minted host chain) into the committed model, then remove it from the plan.
/// That turns it into an ordinary deletion work item (committed has it, the plan
/// does not → `toDelete`), anchored to the code the agent should remove. Folding
/// it also stops drift re-surfacing it: the committed model now describes the
/// behaviour, so it is no longer "undescribed".
#[tauri::command]
pub(crate) fn reject_responsibility(cwd: String, resp_id: String) -> Result<(), String> {
    let model_ref = scryer_core::ModelRef::ProjectLocal(std::path::PathBuf::from(&cwd));
    // Serialize the whole read-modify-write against the agent's MCP writer.
    let _lock = scryer_core::lock_model(&model_ref)?;
    let folded = fold_vagrant(&model_ref, &resp_id)?;

    // Schedule the deletion: drop the responsibility and the minted chain from the
    // plan, so the committed-vs-plan diff reads as a deletion to carry out.
    let mut planned = scryer_core::read_planned_seeded_at(&model_ref)?;
    for n in &mut planned.nodes {
        n.responsibilities.retain(|r| r.id != resp_id);
    }
    for g in &mut planned.groups {
        g.responsibilities.retain(|r| r.id != resp_id);
    }
    planned.source_map.remove(&resp_id);
    for id in &folded.chain {
        planned.nodes.retain(|n| &n.id != id);
        planned.source_map.remove(id);
    }
    scryer_core::write_planned_at(&model_ref, &planned)?;

    if let Ok(after) = scryer_core::read_model_at(&model_ref) {
        let _ = scryer_core::save_baseline_at(&model_ref, &after);
    }
    let mut row = scryer_core::history::EventRow::new("−", folded.statement);
    if let Some(loc) = folded.source {
        row = row.with_source(loc);
    }
    let _ = scryer_core::history::append_event(
        &model_ref,
        &scryer_core::history::HistoryEvent::new(
            scryer_core::drift::now_secs(),
            scryer_core::history::EventKind::Impl,
            &folded.host_id,
            "rejected — marked for deletion",
        )
        .with_rows(vec![row]),
    );
    Ok(())
}

/// Clear the vagrant flag on a code-discovered PROPERTY (addressed by its owning
/// node + label, since properties carry no id) and its minted host chain, then
/// FOLD the chain (root→leaf) and the property into the committed model. The
/// property-level twin of [`fold_vagrant`]; shared by adopt and reject. A property
/// has no source anchor of its own (the data node bears it), so `source` is None.
fn fold_vagrant_property(
    model_ref: &scryer_core::ModelRef,
    node_id: &str,
    label: &str,
) -> Result<FoldedVagrant, String> {
    use scryer_core::diff::ElementKind;

    let mut planned = scryer_core::read_planned_seeded_at(model_ref)?;
    let cleared = planned
        .nodes
        .iter_mut()
        .find(|n| n.id == node_id)
        .and_then(|n| n.properties.iter_mut().find(|p| p.label == label))
        .map(|p| p.vagrant = None)
        .is_some();
    if !cleared {
        return Err(format!("Property '{label}' on node '{node_id}' not found in the plan"));
    }
    // The property may have landed on a freshly minted data symbol; fold that
    // chain first so the host exists in committed before the property folds onto it.
    let chain = vagrant_chain(&planned, node_id);
    for id in &chain {
        if let Some(n) = planned.nodes.iter_mut().find(|n| &n.id == id) {
            n.vagrant = None;
        }
    }
    scryer_core::write_planned_at(model_ref, &planned)?;

    for id in &chain {
        scryer_core::commit_element(model_ref, ElementKind::Node, None, id)?;
    }
    scryer_core::commit_element(model_ref, ElementKind::Property, Some(node_id), label)?;

    Ok(FoldedVagrant { host_id: node_id.to_string(), statement: label.to_string(), source: None, chain })
}

/// Adopt a code-discovered (vagrant) property — the property-level twin of
/// [`adopt_responsibility`]. The field already exists in code, so adopting it IS
/// the commit: fold it (and any minted host chain) into the committed model.
#[tauri::command]
pub(crate) fn adopt_property(cwd: String, node_id: String, label: String) -> Result<(), String> {
    let model_ref = scryer_core::ModelRef::ProjectLocal(std::path::PathBuf::from(&cwd));
    // Serialize the whole read-modify-write against the agent's MCP writer.
    let _lock = scryer_core::lock_model(&model_ref)?;
    let folded = fold_vagrant_property(&model_ref, &node_id, &label)?;

    if let Ok(after) = scryer_core::read_model_at(&model_ref) {
        let _ = scryer_core::save_baseline_at(&model_ref, &after);
    }
    let _ = scryer_core::history::append_event(
        &model_ref,
        &scryer_core::history::HistoryEvent::new(
            scryer_core::drift::now_secs(),
            scryer_core::history::EventKind::Impl,
            &folded.host_id,
            "took code",
        )
        .with_rows(vec![scryer_core::history::EventRow::new("+", folded.statement)]),
    );
    Ok(())
}

/// Reject a code-discovered (vagrant) property — the property-level twin of
/// [`reject_responsibility`]. Fold it (and any minted host chain) into committed,
/// then drop it from the plan so the diff reads as a deletion work item anchored
/// to the field the agent should remove; folding also stops drift re-proposing it.
#[tauri::command]
pub(crate) fn reject_property(cwd: String, node_id: String, label: String) -> Result<(), String> {
    let model_ref = scryer_core::ModelRef::ProjectLocal(std::path::PathBuf::from(&cwd));
    // Serialize the whole read-modify-write against the agent's MCP writer.
    let _lock = scryer_core::lock_model(&model_ref)?;
    let folded = fold_vagrant_property(&model_ref, &node_id, &label)?;

    let mut planned = scryer_core::read_planned_seeded_at(&model_ref)?;
    if let Some(n) = planned.nodes.iter_mut().find(|n| n.id == node_id) {
        n.properties.retain(|p| p.label != label);
    }
    for id in &folded.chain {
        planned.nodes.retain(|n| &n.id != id);
        planned.source_map.remove(id);
    }
    scryer_core::write_planned_at(&model_ref, &planned)?;

    if let Ok(after) = scryer_core::read_model_at(&model_ref) {
        let _ = scryer_core::save_baseline_at(&model_ref, &after);
    }
    let _ = scryer_core::history::append_event(
        &model_ref,
        &scryer_core::history::HistoryEvent::new(
            scryer_core::drift::now_secs(),
            scryer_core::history::EventKind::Impl,
            &folded.host_id,
            "rejected — marked for deletion",
        )
        .with_rows(vec![scryer_core::history::EventRow::new("−", folded.statement)]),
    );
    Ok(())
}

// ---- Stale (take-model) verdicts: the mirror of adopt/reject. ----
//
// A stale claim/node means the model still asserts something the code stopped
// doing. The flag rides the PLANNED draft (where the UI reads it). Two verdicts,
// mirroring the take-code pair:
//   • DROP        — the code is right (removed on purpose) → delete the claim /
//                   subtree from BOTH layers. Mirror of adopt: the model gives
//                   way to reality.
//   • RE-IMPLEMENT — the model is right (code regressed) → remove from committed
//                   while the plan keeps it, so the diff reads it as an `Added`
//                   to-do the agent rebuilds (folding back via mark_implemented).
//                   Mirror of reject's toDelete, but in the build direction.

/// Remove a responsibility wherever it lives (a node or a group), returning
/// (host_id, statement) and GC'ing its source anchor. None if absent.
fn take_responsibility(
    model: &mut scryer_core::ScryModel,
    resp_id: &str,
) -> Option<(String, String)> {
    for n in &mut model.nodes {
        if let Some(pos) = n.responsibilities.iter().position(|r| r.id == resp_id) {
            let r = n.responsibilities.remove(pos);
            model.source_map.remove(resp_id);
            return Some((n.id.clone(), r.statement));
        }
    }
    for g in &mut model.groups {
        if let Some(pos) = g.responsibilities.iter().position(|r| r.id == resp_id) {
            let r = g.responsibilities.remove(pos);
            model.source_map.remove(resp_id);
            return Some((g.id.clone(), r.statement));
        }
    }
    None
}

/// Remove a set of nodes and everything that hangs off them — descendant claims'
/// anchors, the nodes' own declaration anchors and boundaries, links touching
/// them, and dead group memberships. Mirrors the MCP `delete_nodes` cleanup.
fn prune_nodes(model: &mut scryer_core::ScryModel, ids: &std::collections::HashSet<String>) {
    let resp_ids: std::collections::HashSet<String> = model
        .nodes
        .iter()
        .filter(|n| ids.contains(&n.id))
        .flat_map(|n| n.responsibilities.iter().map(|r| r.id.clone()))
        .collect();
    model.source_map.retain(|k, _| !resp_ids.contains(k) && !ids.contains(k));
    model.boundaries.retain(|k, _| !ids.contains(k));
    model.nodes.retain(|n| !ids.contains(&n.id));
    model.links.retain(|l| !ids.contains(&l.src) && !ids.contains(&l.dst));
    for g in &mut model.groups {
        g.member_ids.retain(|m| !ids.contains(m));
    }
}

/// Append a take-model resolution event (the committed model changed). `marker`
/// is the diff glyph for the row (`−` dropped, `+` re-implement to-do).
fn log_take_model(
    model_ref: &scryer_core::ModelRef,
    host_id: &str,
    driver: &str,
    marker: &str,
    text: String,
    source: Option<scryer_core::SourceLocation>,
) {
    let mut row = scryer_core::history::EventRow::new(marker, text);
    if let Some(loc) = source {
        row = row.with_source(loc);
    }
    let _ = scryer_core::history::append_event(
        model_ref,
        &scryer_core::history::HistoryEvent::new(
            scryer_core::drift::now_secs(),
            scryer_core::history::EventKind::Impl,
            host_id,
            driver,
        )
        .with_rows(vec![row]),
    );
}

/// DROP a stale responsibility: the code legitimately no longer does this, so the
/// claim leaves the model entirely (both layers) and its anchor is GC'd. Mirror
/// of `adopt_responsibility`.
#[tauri::command]
pub(crate) fn drop_responsibility(cwd: String, resp_id: String) -> Result<(), String> {
    let model_ref = scryer_core::ModelRef::ProjectLocal(std::path::PathBuf::from(&cwd));
    // Serialize the whole read-modify-write against the agent's MCP writer.
    let _lock = scryer_core::lock_model(&model_ref)?;
    let mut committed = scryer_core::read_model_at(&model_ref)?;
    let mut planned = scryer_core::read_planned_seeded_at(&model_ref)?;

    let source = committed
        .source_map
        .get(&resp_id)
        .or_else(|| planned.source_map.get(&resp_id))
        .and_then(|l| l.first())
        .cloned();

    let from_c = take_responsibility(&mut committed, &resp_id);
    let from_p = take_responsibility(&mut planned, &resp_id);
    let (host_id, statement) =
        from_c.or(from_p).ok_or_else(|| format!("Responsibility '{resp_id}' not found"))?;

    scryer_core::write_model_at(&model_ref, &committed)?;
    scryer_core::write_planned_at(&model_ref, &planned)?;
    let _ = scryer_core::save_baseline_at(&model_ref, &committed);
    log_take_model(&model_ref, &host_id, "dropped — removed from code", "−", statement, source);
    Ok(())
}

/// RE-IMPLEMENT a stale responsibility: the model is right and the code must be
/// rebuilt. Remove it from the committed model (which should only hold claims the
/// code satisfies) while the plan keeps a clean, anchored copy — so the diff
/// reads it as an `Added` to-do the agent implements, folding it back in via
/// `mark_implemented`. Mirror of `reject_responsibility`, in the build direction.
#[tauri::command]
pub(crate) fn reimplement_responsibility(cwd: String, resp_id: String) -> Result<(), String> {
    let model_ref = scryer_core::ModelRef::ProjectLocal(std::path::PathBuf::from(&cwd));
    // Serialize the whole read-modify-write against the agent's MCP writer.
    let _lock = scryer_core::lock_model(&model_ref)?;
    let mut committed = scryer_core::read_model_at(&model_ref)?;
    let mut planned = scryer_core::read_planned_seeded_at(&model_ref)?;

    let source = committed
        .source_map
        .get(&resp_id)
        .or_else(|| planned.source_map.get(&resp_id))
        .and_then(|l| l.first())
        .cloned();
    let committed_anchor = committed.source_map.get(&resp_id).cloned();

    // Remove from committed — the code regressed, so it no longer holds.
    let removed = take_responsibility(&mut committed, &resp_id);

    // Keep a clean to-do in the plan: clear stale, ensure it's present + anchored.
    let mut host_id = None;
    let mut statement = None;
    let in_plan = planned
        .nodes
        .iter_mut()
        .flat_map(|n| {
            let nid = n.id.clone();
            n.responsibilities.iter_mut().map(move |r| (nid.clone(), r))
        })
        .chain(planned.groups.iter_mut().flat_map(|g| {
            let gid = g.id.clone();
            g.responsibilities.iter_mut().map(move |r| (gid.clone(), r))
        }))
        .find(|(_, r)| r.id == resp_id);
    if let Some((hid, r)) = in_plan {
        r.stale = None;
        r.stale_proposal = None;
        host_id = Some(hid);
        statement = Some(r.statement.clone());
    } else if let Some((chost, cstmt)) = &removed {
        // The plan had dropped it — reconstruct from committed so the to-do exists.
        if let Some(n) = planned.nodes.iter_mut().find(|n| &n.id == chost) {
            n.responsibilities.push(scryer_core::Responsibility {
                concern: None,
                id: resp_id.clone(),
                statement: cstmt.clone(),
                vagrant: None,
                stale: None,
                stale_proposal: None,
                directives: Vec::new(),
                last_touched_at: None,
            });
            host_id = Some(chost.clone());
            statement = Some(cstmt.clone());
        }
    }
    if let Some(anchor) = committed_anchor {
        planned.source_map.entry(resp_id.clone()).or_insert(anchor);
    }

    let (Some(host_id), Some(statement)) = (host_id, statement) else {
        return Err(format!("Responsibility '{resp_id}' not found"));
    };

    scryer_core::write_model_at(&model_ref, &committed)?;
    scryer_core::write_planned_at(&model_ref, &planned)?;
    let _ = scryer_core::save_baseline_at(&model_ref, &committed);
    log_take_model(&model_ref, &host_id, "re-implement — code regressed", "+", statement, source);
    Ok(())
}

/// Remove a property by (node, label), returning it. None if absent. Properties
/// have no source anchor of their own, so nothing else to GC. Mirror of
/// [`take_responsibility`] for the data-shape layer.
fn take_property(
    model: &mut scryer_core::ScryModel,
    node_id: &str,
    label: &str,
) -> Option<scryer_core::SchemaProperty> {
    let n = model.nodes.iter_mut().find(|n| n.id == node_id)?;
    let pos = n.properties.iter().position(|p| p.label == label)?;
    Some(n.properties.remove(pos))
}

/// DROP a stale property: the code legitimately removed this field, so the property
/// leaves the model entirely (both layers). Property-level twin of
/// [`drop_responsibility`].
#[tauri::command]
pub(crate) fn drop_property(cwd: String, node_id: String, label: String) -> Result<(), String> {
    let model_ref = scryer_core::ModelRef::ProjectLocal(std::path::PathBuf::from(&cwd));
    // Serialize the whole read-modify-write against the agent's MCP writer.
    let _lock = scryer_core::lock_model(&model_ref)?;
    let mut committed = scryer_core::read_model_at(&model_ref)?;
    let mut planned = scryer_core::read_planned_seeded_at(&model_ref)?;

    let removed = take_property(&mut committed, &node_id, &label)
        .or_else(|| take_property(&mut planned, &node_id, &label))
        .ok_or_else(|| format!("Property '{label}' on node '{node_id}' not found"))?;
    // Make sure it's gone from BOTH layers regardless of which one matched first.
    take_property(&mut committed, &node_id, &label);
    take_property(&mut planned, &node_id, &label);

    scryer_core::write_model_at(&model_ref, &committed)?;
    scryer_core::write_planned_at(&model_ref, &planned)?;
    let _ = scryer_core::save_baseline_at(&model_ref, &committed);
    log_take_model(&model_ref, &node_id, "dropped — removed from code", "−", removed.label, None);
    Ok(())
}

/// RE-IMPLEMENT a stale property: the model is right and the field must be rebuilt.
/// Remove it from committed while the plan keeps a clean copy (stale cleared), so
/// the diff reads it as an `Added` to-do. Property-level twin of
/// [`reimplement_responsibility`].
#[tauri::command]
pub(crate) fn reimplement_property(cwd: String, node_id: String, label: String) -> Result<(), String> {
    let model_ref = scryer_core::ModelRef::ProjectLocal(std::path::PathBuf::from(&cwd));
    // Serialize the whole read-modify-write against the agent's MCP writer.
    let _lock = scryer_core::lock_model(&model_ref)?;
    let mut committed = scryer_core::read_model_at(&model_ref)?;
    let mut planned = scryer_core::read_planned_seeded_at(&model_ref)?;

    let removed = take_property(&mut committed, &node_id, &label);

    // Keep a clean to-do in the plan: clear stale, or reconstruct from committed.
    let in_plan = planned
        .nodes
        .iter_mut()
        .find(|n| n.id == node_id)
        .and_then(|n| n.properties.iter_mut().find(|p| p.label == label));
    if let Some(p) = in_plan {
        p.stale = None;
    } else if let Some(prop) = &removed {
        if let Some(n) = planned.nodes.iter_mut().find(|n| n.id == node_id) {
            n.properties.push(scryer_core::SchemaProperty {
                label: prop.label.clone(),
                description: prop.description.clone(),
                vagrant: None,
                stale: None,
                last_touched_at: None,
            });
        }
    } else {
        return Err(format!("Property '{label}' on node '{node_id}' not found"));
    }

    scryer_core::write_model_at(&model_ref, &committed)?;
    scryer_core::write_planned_at(&model_ref, &planned)?;
    let _ = scryer_core::save_baseline_at(&model_ref, &committed);
    log_take_model(&model_ref, &node_id, "re-implement — code regressed", "+", label, None);
    Ok(())
}

/// Set a responsibility's statement wherever it lives (nodes or groups), clearing
/// the drift flags and stamping the edit. Returns the host node/group id.
fn reword_in_model(
    model: &mut scryer_core::ScryModel,
    resp_id: &str,
    statement: &str,
    now: u64,
) -> Option<String> {
    let host = model
        .nodes
        .iter_mut()
        .map(|n| (n.id.clone(), &mut n.responsibilities))
        .chain(model.groups.iter_mut().map(|g| (g.id.clone(), &mut g.responsibilities)))
        .find_map(|(hid, resps)| resps.iter_mut().find(|r| r.id == resp_id).map(|r| (hid, r)));
    let (host_id, r) = host?;
    r.statement = statement.to_string();
    r.stale = None;
    r.stale_proposal = None;
    r.last_touched_at = Some(now);
    Some(host_id)
}

/// REWORD a stale responsibility: the code didn't lose the behaviour, it DIVERGED,
/// and drift proposed a corrected statement. Accepting it brings the model in line
/// with code that already exists — so the new wording lands in BOTH layers and the
/// stale/proposal flags clear, leaving the layers identical and thus no plan work
/// item. The reconcile mirror of `drop`/`adopt`: a model edit catching up to
/// reality, not a build to-do. `statement` is the accepted text (drift's proposal,
/// possibly edited by the user).
#[tauri::command]
pub(crate) fn reword_responsibility(cwd: String, resp_id: String, statement: String) -> Result<(), String> {
    let statement = statement.trim().to_string();
    if statement.is_empty() {
        return Err("Reworded statement is empty".into());
    }
    let model_ref = scryer_core::ModelRef::ProjectLocal(std::path::PathBuf::from(&cwd));
    // Serialize the whole read-modify-write against the agent's MCP writer.
    let _lock = scryer_core::lock_model(&model_ref)?;
    let mut committed = scryer_core::read_model_at(&model_ref)?;
    let mut planned = scryer_core::read_planned_seeded_at(&model_ref)?;

    let source = committed
        .source_map
        .get(&resp_id)
        .or_else(|| planned.source_map.get(&resp_id))
        .and_then(|l| l.first())
        .cloned();

    let now = scryer_core::drift::now_secs();
    let in_c = reword_in_model(&mut committed, &resp_id, &statement, now);
    let in_p = reword_in_model(&mut planned, &resp_id, &statement, now);
    let host_id = in_c.or(in_p).ok_or_else(|| format!("Responsibility '{resp_id}' not found"))?;

    scryer_core::write_model_at(&model_ref, &committed)?;
    scryer_core::write_planned_at(&model_ref, &planned)?;
    let _ = scryer_core::save_baseline_at(&model_ref, &committed);
    log_take_model(&model_ref, &host_id, "reworded — code diverged", "~", statement, source);
    Ok(())
}

/// DROP a stale node: the whole subtree's backing code is gone on purpose, so the
/// node and every descendant (claims, links, group memberships, anchors) leaves
/// both layers. The node-level mirror of `drop_responsibility`.
#[tauri::command]
pub(crate) fn drop_node(cwd: String, node_id: String) -> Result<(), String> {
    let model_ref = scryer_core::ModelRef::ProjectLocal(std::path::PathBuf::from(&cwd));
    // Serialize the whole read-modify-write against the agent's MCP writer.
    let _lock = scryer_core::lock_model(&model_ref)?;
    let mut committed = scryer_core::read_model_at(&model_ref)?;
    let mut planned = scryer_core::read_planned_seeded_at(&model_ref)?;

    let in_c = committed.nodes.iter().any(|n| n.id == node_id);
    let in_p = planned.nodes.iter().any(|n| n.id == node_id);
    if !in_c && !in_p {
        return Err(format!("Node '{node_id}' not found"));
    }

    // Name + parent for the timeline (the node itself is about to disappear).
    let (name, parent_id) = committed
        .nodes
        .iter()
        .chain(planned.nodes.iter())
        .find(|n| n.id == node_id)
        .map(|n| (n.name.clone(), n.parent_id.clone()))
        .unwrap_or_else(|| (node_id.clone(), None));

    // Subtree from each layer it lives in (a node may exist in only one).
    let mut ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    if in_c {
        ids.extend(scryer_core::drift::subtree_ids(&committed, &node_id));
    }
    if in_p {
        ids.extend(scryer_core::drift::subtree_ids(&planned, &node_id));
    }

    prune_nodes(&mut committed, &ids);
    prune_nodes(&mut planned, &ids);

    scryer_core::write_model_at(&model_ref, &committed)?;
    scryer_core::write_planned_at(&model_ref, &planned)?;
    let _ = scryer_core::save_baseline_at(&model_ref, &committed);
    // Attach to the parent — the node id is gone.
    let host = parent_id.unwrap_or_else(|| node_id.clone());
    log_take_model(
        &model_ref,
        &host,
        "dropped — removed from code",
        "−",
        format!("{name} (subtree)"),
        None,
    );
    Ok(())
}

/// RE-IMPLEMENT a stale node: the model is right and the whole subtree must be
/// rebuilt. Remove the subtree from the committed model while the plan keeps it
/// (stale cleared), so each node/claim reads as an `Added` to-do. The node-level
/// mirror of `reimplement_responsibility`.
#[tauri::command]
pub(crate) fn reimplement_node(cwd: String, node_id: String) -> Result<(), String> {
    let model_ref = scryer_core::ModelRef::ProjectLocal(std::path::PathBuf::from(&cwd));
    // Serialize the whole read-modify-write against the agent's MCP writer.
    let _lock = scryer_core::lock_model(&model_ref)?;
    let mut committed = scryer_core::read_model_at(&model_ref)?;
    let mut planned = scryer_core::read_planned_seeded_at(&model_ref)?;

    let in_c = committed.nodes.iter().any(|n| n.id == node_id);
    let in_p = planned.nodes.iter().any(|n| n.id == node_id);
    if !in_c && !in_p {
        return Err(format!("Node '{node_id}' not found"));
    }

    let name = planned
        .nodes
        .iter()
        .chain(committed.nodes.iter())
        .find(|n| n.id == node_id)
        .map(|n| n.name.clone())
        .unwrap_or_else(|| node_id.clone());

    let mut ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    if in_c {
        ids.extend(scryer_core::drift::subtree_ids(&committed, &node_id));
    }
    if in_p {
        ids.extend(scryer_core::drift::subtree_ids(&planned, &node_id));
    }

    // Remove the subtree from committed → it becomes `Added` in the plan diff.
    prune_nodes(&mut committed, &ids);
    // Clear stale on the surviving plan subtree (nodes AND their claims) so it
    // reads as clean pending work, not drift.
    for n in &mut planned.nodes {
        if ids.contains(&n.id) {
            n.stale = None;
            for r in &mut n.responsibilities {
                r.stale = None;
            }
        }
    }

    scryer_core::write_model_at(&model_ref, &committed)?;
    scryer_core::write_planned_at(&model_ref, &planned)?;
    let _ = scryer_core::save_baseline_at(&model_ref, &committed);
    log_take_model(
        &model_ref,
        &node_id,
        "re-implement — code regressed",
        "+",
        format!("{name} (subtree)"),
        None,
    );
    Ok(())
}

#[cfg(test)]
mod plan_seed_tests {
    use scryer_core::{ModelRef, ScryModel};

    fn resp(id: &str, statement: &str) -> scryer_core::Responsibility {
        scryer_core::Responsibility {
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

    /// Regression for the plan-seed seam (audit theme 1): a canvas verdict on a
    /// project with a COMMITTED model but no `planned.scry` yet must seed a CLEAN
    /// draft, not persist the committed fallback (anchors and all) as the plan.
    /// Without the seed, `read_planned_at` returns committed and writing it back
    /// mints the draft as a full shadow of committed's source_map/boundaries.
    #[test]
    fn a_verdict_seeds_a_clean_plan_without_shadowing_committed_anchors() {
        let dir = tempfile::tempdir().unwrap();
        let r = ModelRef::ProjectLocal(dir.path().to_path_buf());

        // Committed: a container whose responsibility carries a source anchor and
        // whose box carries a boundary glob. NO planned.scry is written.
        let node = |v: serde_json::Value| serde_json::from_value::<scryer_core::Node>(v).unwrap();
        let mut model = ScryModel::new();
        model
            .nodes
            .push(node(serde_json::json!({ "id": "node-1", "kind": "system", "name": "Acme" })));
        let mut cont = node(serde_json::json!({
            "id": "node-2", "kind": "container", "name": "API", "parentId": "node-1"
        }));
        cont.responsibilities = vec![resp("resp-1", "serves the API")];
        model.nodes.push(cont);
        model.source_map.insert(
            "resp-1".into(),
            vec![serde_json::from_value(serde_json::json!({ "pattern": "api/mod.rs" })).unwrap()],
        );
        model.boundaries.insert(
            "node-2".into(),
            vec![serde_json::from_value(serde_json::json!({ "pattern": "api/**/*" })).unwrap()],
        );
        scryer_core::write_model_at(&r, &model).unwrap();
        assert!(!r.planned_path().exists(), "precondition: no draft exists yet");

        // A canvas verdict with no prior draft (reword lands in both layers).
        let cwd = dir.path().to_string_lossy().to_string();
        super::reword_responsibility(cwd, "resp-1".into(), "serves the public API".into()).unwrap();

        // The draft now exists and carries the reworded claim, but owns NO shadow
        // of committed's anchors: a committed element's mapping lives in committed
        // alone.
        let plan = scryer_core::read_planned_at(&r).unwrap();
        let r1 = plan
            .nodes
            .iter()
            .flat_map(|n| &n.responsibilities)
            .find(|x| x.id == "resp-1")
            .unwrap();
        assert_eq!(r1.statement, "serves the public API", "the verdict landed in the plan");
        assert!(plan.source_map.is_empty(), "draft must not shadow committed's source_map");
        assert!(plan.boundaries.is_empty(), "draft must not shadow committed's boundaries");

        // Committed keeps its anchors — nothing was moved or lost.
        let committed = scryer_core::read_model_at(&r).unwrap();
        assert!(committed.source_map.contains_key("resp-1"));
        assert!(committed.boundaries.contains_key("node-2"));
    }
}
