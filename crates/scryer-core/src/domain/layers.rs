//! How the two model layers combine: the working view a reader sees is the
//! planned draft with committed's anchors overlaid.

use crate::ScryModel;
use std::collections::HashSet;

/// The working view the agent operates on: the authored PLAN structure (nodes,
/// links, groups, claims) with committed's single-home anchors overlaid, so
/// nothing that lives only in committed — a committed container's boundary glob,
/// a committed claim's source anchor — vanishes from a plan-based read. Plan
/// entries win on conflict (the draft is the newer authoring). This is what a
/// gate or health read should see: `planned` alone omits committed's anchors
/// (single-home), `model` alone omits the agent's unfolded edits.
pub fn working_view(committed: &ScryModel, planned: &ScryModel) -> ScryModel {
    let mut view = planned.clone();
    // Overlay only entries whose owner is still live in the PLAN: a committed
    // anchor or boundary for a plan-deleted element is pending GC (the deletion
    // fold removes it), and carrying it into the view would make the gate warn
    // about — and health count — an element the plan already removed.
    let node_ids: HashSet<&str> = planned.nodes.iter().map(|n| n.id.as_str()).collect();
    let resp_ids: HashSet<&str> = planned
        .nodes
        .iter()
        .flat_map(|n| n.responsibilities.iter())
        .chain(planned.groups.iter().flat_map(|g| g.responsibilities.iter()))
        .map(|r| r.id.as_str())
        .collect();
    // source_map is keyed by responsibility id or by a property-bearing node id
    // (a schema's declaration site) — the same key universe `validate` checks.
    let property_node_ids: HashSet<&str> = planned
        .nodes
        .iter()
        .filter(|n| !n.properties.is_empty())
        .map(|n| n.id.as_str())
        .collect();
    for (id, sources) in &committed.boundaries {
        if node_ids.contains(id.as_str()) {
            view.boundaries.entry(id.clone()).or_insert_with(|| sources.clone());
        }
    }
    for (id, locs) in &committed.source_map {
        if resp_ids.contains(id.as_str()) || property_node_ids.contains(id.as_str()) {
            view.source_map.entry(id.clone()).or_insert_with(|| locs.clone());
        }
    }
    // test_map is keyed by responsibility id only (a test backs a claim,
    // never a declaration site).
    for (id, locs) in &committed.test_map {
        if resp_ids.contains(id.as_str()) {
            view.test_map.entry(id.clone()).or_insert_with(|| locs.clone());
        }
    }
    view
}
