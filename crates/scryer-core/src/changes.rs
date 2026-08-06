//! The change ledger — named partitions of the plan.
//!
//! The plan is one draft file; without this module it is a single anonymous
//! transaction ("git's working tree without branches"). A **change** gives a
//! slice of that draft a name and a rationale — the dev's original sentence,
//! the one artifact that otherwise dies in the chat log — so that pending work
//! can be listed, reviewed, folded, and resumed *per task* instead of as one
//! pile.
//!
//! The representation is a side-map, not a field on every element (mirroring
//! `source_map`/`boundaries`): [`ScryModel::change_map`] maps an element key
//! (see [`element_key`]) to a change id, and [`ScryModel::changes`] is the
//! registry of open changes. Both live ONLY in the plan layer — the committed
//! model never carries change state ([`crate::write_model_at`] strips it), and
//! a change's durable record after it closes is a [`crate::history`] event.
//! A side-map also covers what a per-element field cannot: a planned
//! *deletion* has no element left to tag, but its key can still map to the
//! change that ordered it.
//!
//! Lifecycle: a change opens with a rationale ([`open_change`]), writes tag
//! the elements they author ([`tag`]), and the plan↔committed diff remains the
//! single source of what is pending — the map holds no state of its own beyond
//! the grouping. That is enforced by the GC invariant: **every map key must
//! correspond to a current diff entry** ([`gc`] prunes the rest). A change
//! closes when a prune takes its last key — folding an element removes it from
//! the diff (implemented), and so does reverting it (abandoned); either way
//! the change's record is appended to history ([`record_closed`]) and the
//! registry entry is dropped. "If it's committed, it's done" — there is no
//! separate close verb.
//!
//! An open change with NO tagged elements yet is legitimate (just opened, or
//! all its work still unwritten) and is never GC'd: [`gc`] closes only
//! changes whose keys the prune itself removed.

use crate::diff::{diff, ElementChange, ElementKind};
use crate::drift::now_secs;
use crate::history::{append_event, EventKind, EventRow, HistoryEvent};
use crate::{ModelRef, ScryModel};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// One open change in the plan's registry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChangeMeta {
    /// Stable id, minted `chg-N`.
    pub id: String,
    /// The dev's original sentence — why this change exists. Survives the fold
    /// as the history record's text.
    pub rationale: String,
    /// Unix seconds.
    pub created_at: u64,
}

/// Canonical change-map key for an element. Kind-prefixed so a key classifies
/// itself back to a diff element without consulting either layer; properties
/// have no id, so they key by `(owner node, label)` exactly as the diff does.
/// Owner ids must not contain `:` (minted ids never do) — the label may.
pub fn element_key(kind: ElementKind, owner_id: Option<&str>, id: &str) -> String {
    match kind {
        ElementKind::Node => format!("node:{id}"),
        ElementKind::Link => format!("link:{id}"),
        ElementKind::Group => format!("group:{id}"),
        ElementKind::Responsibility => format!("resp:{id}"),
        ElementKind::Property => format!("prop:{}:{id}", owner_id.unwrap_or("")),
    }
}

/// The key of a diff entry — the join point between `change_map` and the plan
/// diff ([`gc`]'s validity test, and how surfaces group pending entries).
pub fn key_for(change: &ElementChange) -> String {
    element_key(change.kind, change.owner_id.as_deref(), &change.id)
}

/// Decompose a map key back into the `(kind, owner, id)` triple
/// `commit_element` consumes — how "fold *this change*" expands into element
/// folds. Returns None for a malformed key.
pub fn parse_key(key: &str) -> Option<(ElementKind, Option<String>, String)> {
    let (kind, rest) = key.split_once(':')?;
    match kind {
        "node" => Some((ElementKind::Node, None, rest.to_string())),
        "link" => Some((ElementKind::Link, None, rest.to_string())),
        "group" => Some((ElementKind::Group, None, rest.to_string())),
        "resp" => Some((ElementKind::Responsibility, None, rest.to_string())),
        "prop" => {
            let (owner, label) = rest.split_once(':')?;
            Some((ElementKind::Property, Some(owner.to_string()), label.to_string()))
        }
        _ => None,
    }
}

/// Open a new change: mint the next `chg-N` id (seeded past every id the plan
/// has seen, registry or map, so a re-open never collides with a tag left by
/// a closed twin) and register it. The caller persists the plan.
pub fn open_change(model: &mut ScryModel, rationale: &str, now: u64) -> String {
    let max = model
        .changes
        .iter()
        .map(|c| c.id.as_str())
        .chain(model.change_map.values().map(|s| s.as_str()))
        .filter_map(|id| id.strip_prefix("chg-")?.parse::<u64>().ok())
        .max()
        .unwrap_or(0);
    let id = format!("chg-{}", max + 1);
    model.changes.push(ChangeMeta {
        id: id.clone(),
        rationale: rationale.trim().to_string(),
        created_at: now,
    });
    id
}

/// Tag plan elements as belonging to `change_id`. Last writer wins — a re-tag
/// replaces the old owner — but each key that was already claimed by a
/// DIFFERENT change is returned as `(key, previous change id)` so the caller
/// can surface the collision: two changes rewording the same claim is exactly
/// the conflict the ledger exists to catch before the code merges.
pub fn tag(model: &mut ScryModel, keys: &[String], change_id: &str) -> Vec<(String, String)> {
    let mut conflicts = Vec::new();
    for k in keys {
        if let Some(prev) = model.change_map.get(k) {
            if prev != change_id {
                conflicts.push((k.clone(), prev.clone()));
            }
        }
        model.change_map.insert(k.clone(), change_id.to_string());
    }
    conflicts
}

/// What a [`retag`] pass moved, for the caller's report.
#[derive(Debug, Default)]
pub struct Retag {
    /// Element keys whose change tag changed, with where they came from —
    /// `(key, previous change id or None for unfiled)`.
    pub moved: Vec<(String, Option<String>)>,
    /// Targets that named nothing pending. Never an error on their own (an id
    /// whose work is already folded is a no-op, not a mistake), but always
    /// reported: silence here is how a caller concludes a move happened when
    /// it didn't.
    pub unmatched: Vec<String>,
}

/// The host a pending element files under — the carrier the Changes page and
/// `get_pending` show it beneath, so "retag this node" means the same set of
/// elements the caller was just looking at. A claim/property files under its
/// owner, a link under the side that performs it, a node/group under itself.
fn host_of(planned: &ScryModel, committed: &ScryModel, ec: &ElementChange) -> Option<String> {
    match ec.kind {
        ElementKind::Node | ElementKind::Group => Some(ec.id.clone()),
        ElementKind::Responsibility | ElementKind::Property => ec.owner_id.clone(),
        ElementKind::Link => planned
            .links
            .iter()
            .chain(committed.links.iter())
            .find(|l| l.id == ec.id)
            .map(|l| l.src.clone()),
    }
}

/// Move pending work between changes — the ledger's only re-filing verb.
///
/// Tags are otherwise a side-effect of writing: whatever a session touches
/// lands in whatever change that session selected. That is right until it
/// isn't (work filed under the wrong change, or one task's plan turning out to
/// be two), and without this the only repair is re-writing the elements
/// themselves under a different selection — editing the spec to fix its
/// bookkeeping.
///
/// `targets` are the BARE ids the caller already holds, resolved against the
/// CURRENT plan diff (the gc invariant: a tag with no pending entry is dead):
///   - a node or group id — that carrier AND every pending element under it,
///     the same unit `get_pending` shows;
///   - a responsibility or link id — that element alone;
///   - a `chg-N` id — everything currently filed under it;
///   - `"unfiled"` — every pending element with no tag.
///
/// `to` is the destination change, or None to detach to unfiled. Idempotent:
/// an element already filed where it is being sent is not reported as moved.
pub fn retag(
    committed: &ScryModel,
    planned: &mut ScryModel,
    targets: &[String],
    to: Option<&str>,
) -> Result<Retag, String> {
    if let Some(dst) = to {
        if !planned.changes.iter().any(|c| c.id == dst) {
            return Err(format!("no open change '{dst}'"));
        }
    }
    let pending = crate::diff::pending_elements(committed, planned);
    let mut out = Retag::default();
    for target in targets {
        let mut hit = false;
        for ec in &pending {
            let key = key_for(ec);
            let current = planned.change_map.get(&key).cloned();
            let matches = if target == "unfiled" {
                current.is_none()
            } else if target.starts_with("chg-") {
                current.as_deref() == Some(target.as_str())
            } else {
                // Direct id, or the carrier this element files under. A
                // property's `id` is its label, never an id — it moves only
                // through its owner.
                (ec.kind != ElementKind::Property && &ec.id == target)
                    || host_of(planned, committed, ec).as_ref() == Some(target)
            };
            if !matches {
                continue;
            }
            hit = true;
            if current.as_deref() == to {
                continue;
            }
            match to {
                Some(dst) => planned.change_map.insert(key.clone(), dst.to_string()),
                None => planned.change_map.remove(&key),
            };
            out.moved.push((key, current));
        }
        if !hit {
            out.unmatched.push(target.clone());
        }
    }
    Ok(out)
}

/// Whether folding the element at `host_key` must NOT carry the element at
/// `elem_key` along: the element belongs to a different change than its host,
/// so it is another task's pending work — a whole-node fold leaves it in the
/// plan exactly as it leaves vagrants. Untagged elements ride any fold (the
/// unfiled serial workflow), and an element always rides its own change.
pub fn foreign_to_host(
    map: &std::collections::HashMap<String, String>,
    host_key: &str,
    elem_key: &str,
) -> bool {
    match map.get(elem_key) {
        None => false,
        Some(c) => map.get(host_key) != Some(c),
    }
}

/// What a [`gc`] pass did: how many dead keys it pruned, and which changes
/// that pruning finished (removed from the registry; the caller records them
/// via [`record_closed`] and persists the plan when `pruned > 0`).
#[derive(Debug, Default)]
pub struct Gc {
    pub pruned: usize,
    pub closed: Vec<ChangeMeta>,
}

/// Enforce the ledger invariant: every `change_map` key corresponds to a
/// current plan-diff entry. A key goes stale two ways — its element folded
/// into committed (implemented) or was edited back to its committed form
/// (abandoned) — and in both cases the pending entry it named no longer
/// exists, so the tag is dead. Prune the dead keys, then close every change
/// the prune emptied. Changes that simply HAVE no keys are left alone (just
/// opened, work not yet written): only a change whose last key died in this
/// pass closes here.
pub fn gc(committed: &ScryModel, planned: &mut ScryModel) -> Gc {
    if planned.change_map.is_empty() && planned.changes.is_empty() {
        return Gc::default();
    }
    let valid: HashSet<String> =
        diff(committed, planned).changes.iter().map(key_for).collect();
    let before = planned.change_map.len();
    let mut candidates: HashSet<String> = HashSet::new();
    planned.change_map.retain(|k, v| {
        let keep = valid.contains(k);
        if !keep {
            candidates.insert(v.clone());
        }
        keep
    });
    let live: HashSet<&String> = planned.change_map.values().collect();
    let closed: Vec<ChangeMeta> = planned
        .changes
        .iter()
        .filter(|c| candidates.contains(&c.id) && !live.contains(&c.id))
        .cloned()
        .collect();
    planned.changes.retain(|c| !closed.iter().any(|x| x.id == c.id));
    Gc { pruned: before - planned.change_map.len(), closed }
}

/// Close an EMPTY open change by hand — the escape hatch for a stranded
/// ledger (opened, but its work ended up tagged or folded elsewhere), which
/// [`gc`] deliberately never touches because "no keys yet" is also what a
/// freshly opened change looks like. Refuses a change that still has tagged
/// entries: those close by folding or reverting the entries themselves, never
/// by discarding the grouping. The close is recorded as "abandoned" so the
/// rationale survives. The caller must hold the model lock.
pub fn close_change(r: &ModelRef, change_id: &str) -> Result<ChangeMeta, String> {
    let mut plan = crate::read_planned_at(r)?;
    let Some(pos) = plan.changes.iter().position(|c| c.id == change_id) else {
        return Err(format!("no open change '{change_id}'"));
    };
    let entries = plan.change_map.values().filter(|v| *v == change_id).count();
    if entries > 0 {
        return Err(format!(
            "{change_id} still has {entries} tagged entr{} — fold or revert them; \
             the change closes itself when its last entry goes",
            if entries == 1 { "y" } else { "ies" }
        ));
    }
    let meta = plan.changes.remove(pos);
    crate::write_planned_at(r, &plan)?;
    record_closed(r, &meta, "abandoned");
    Ok(meta)
}

/// Append a closed change's durable record to the history log — the rationale
/// finally survives the fold ("which change introduced this claim?" has an
/// answer). `driver` says how it closed: "folded" (its entries reached
/// committed) or "abandoned" (they were reverted). Best-effort like every
/// history append: a log failure must never abort the model operation.
pub fn record_closed(r: &ModelRef, meta: &ChangeMeta, driver: &str) {
    let ev = HistoryEvent::new(now_secs(), EventKind::Change, "", driver)
        .with_change(&meta.id)
        .with_rows(vec![EventRow::new("✓", meta.rationale.clone())]);
    let _ = append_event(r, &ev);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::read_history;
    use crate::{
        commit_element, fold_built_model, read_model_at, read_planned_at, write_model_at,
        write_planned_at,
    };
    use tempfile::tempdir;

    /// A model whose single component `n1` carries the given responsibilities.
    fn model_with_resps(resps: &[(&str, &str)]) -> ScryModel {
        let resps: Vec<_> = resps
            .iter()
            .map(|(id, s)| serde_json::json!({ "id": id, "statement": s }))
            .collect();
        serde_json::from_value(serde_json::json!({
            "version": crate::SCRY_VERSION,
            "nodes": [{ "id": "n1", "kind": "component", "name": "C", "responsibilities": resps }],
            "links": [],
        }))
        .unwrap()
    }

    /// The full lifecycle: two changes tag pending claims; folding one claim
    /// closes its change (recorded "folded", rationale intact) while the other
    /// stays open; the committed layer never carries change state.
    #[test]
    fn fold_closes_the_emptied_change_and_records_its_rationale() {
        let tmp = tempdir().unwrap();
        let r = ModelRef::ProjectLocal(tmp.path().to_path_buf());
        write_model_at(&r, &model_with_resps(&[("r1", "exists")])).unwrap();

        let mut plan = model_with_resps(&[("r1", "exists"), ("r2", "new A"), ("r3", "new B")]);
        let a = open_change(&mut plan, "track vagrant properties too", 100);
        let b = open_change(&mut plan, "second workstream", 200);
        tag(&mut plan, &[element_key(ElementKind::Responsibility, None, "r2")], &a);
        tag(&mut plan, &[element_key(ElementKind::Responsibility, None, "r3")], &b);
        write_planned_at(&r, &plan).unwrap();

        commit_element(&r, ElementKind::Responsibility, None, "r2").unwrap();

        let planned = read_planned_at(&r).unwrap();
        assert_eq!(planned.changes.len(), 1, "the emptied change left the registry");
        assert_eq!(planned.changes[0].id, b);
        assert_eq!(
            planned.change_map.keys().collect::<Vec<_>>(),
            vec![&element_key(ElementKind::Responsibility, None, "r3")]
        );
        let committed = read_model_at(&r).unwrap();
        assert!(committed.changes.is_empty() && committed.change_map.is_empty());

        let closes: Vec<_> =
            read_history(&r).into_iter().filter(|e| e.kind == EventKind::Change).collect();
        assert_eq!(closes.len(), 1);
        assert_eq!(closes[0].change_id.as_deref(), Some(a.as_str()));
        assert_eq!(closes[0].driver, "folded");
        assert_eq!(closes[0].rows[0].text, "track vagrant properties too");
    }

    /// Reverting a tagged element on the authoring path kills its pending
    /// entry, so the change closes as "abandoned" — while a freshly opened
    /// change with no tags yet survives every write untouched.
    #[test]
    fn revert_abandons_the_change_but_an_untagged_open_change_survives() {
        let tmp = tempdir().unwrap();
        let r = ModelRef::ProjectLocal(tmp.path().to_path_buf());
        write_model_at(&r, &model_with_resps(&[("r1", "exists")])).unwrap();

        let mut plan = model_with_resps(&[("r1", "exists"), ("r2", "new A")]);
        let tagged = open_change(&mut plan, "doomed", 100);
        tag(&mut plan, &[element_key(ElementKind::Responsibility, None, "r2")], &tagged);
        let fresh = open_change(&mut plan, "not yet written", 150);
        write_planned_at(&r, &plan).unwrap();

        // Revert r2: the next write carries the map entry but no divergence.
        let mut reverted = read_planned_at(&r).unwrap();
        for n in &mut reverted.nodes {
            n.responsibilities.retain(|x| x.id != "r2");
        }
        write_planned_at(&r, &reverted).unwrap();

        let planned = read_planned_at(&r).unwrap();
        assert_eq!(planned.changes.len(), 1);
        assert_eq!(planned.changes[0].id, fresh, "the never-tagged change is not GC bait");
        assert!(planned.change_map.is_empty());
        let closes: Vec<_> =
            read_history(&r).into_iter().filter(|e| e.kind == EventKind::Change).collect();
        assert_eq!(closes.len(), 1);
        assert_eq!(closes[0].change_id.as_deref(), Some(tagged.as_str()));
        assert_eq!(closes[0].driver, "abandoned");
    }

    /// A whole-node fold carries only its own change's claims: a claim tagged
    /// to a DIFFERENT change stays pending in the plan (it is another task's
    /// work), so folding change A can never silently complete change B.
    #[test]
    fn whole_node_fold_leaves_foreign_tagged_claims_in_the_plan() {
        let tmp = tempdir().unwrap();
        let r = ModelRef::ProjectLocal(tmp.path().to_path_buf());
        let committed: ScryModel = serde_json::from_value(serde_json::json!({
            "version": crate::SCRY_VERSION,
            "nodes": [{ "id": "n1", "kind": "system", "name": "S" }],
            "links": [],
        }))
        .unwrap();
        write_model_at(&r, &committed).unwrap();

        let mut plan: ScryModel = serde_json::from_value(serde_json::json!({
            "version": crate::SCRY_VERSION,
            "nodes": [
                { "id": "n1", "kind": "system", "name": "S" },
                { "id": "n2", "kind": "container", "name": "C", "parentId": "n1",
                  "responsibilities": [
                      { "id": "r2", "statement": "task A's claim" },
                      { "id": "r3", "statement": "task B's claim" },
                  ] },
            ],
            "links": [],
        }))
        .unwrap();
        let a = open_change(&mut plan, "task A", 100);
        let b = open_change(&mut plan, "task B", 200);
        tag(
            &mut plan,
            &[
                element_key(ElementKind::Node, None, "n2"),
                element_key(ElementKind::Responsibility, None, "r2"),
            ],
            &a,
        );
        tag(&mut plan, &[element_key(ElementKind::Responsibility, None, "r3")], &b);
        write_planned_at(&r, &plan).unwrap();

        commit_element(&r, ElementKind::Node, None, "n2").unwrap();

        let committed = read_model_at(&r).unwrap();
        let n2 = committed.nodes.iter().find(|n| n.id == "n2").unwrap();
        assert!(n2.responsibilities.iter().any(|x| x.id == "r2"));
        assert!(
            !n2.responsibilities.iter().any(|x| x.id == "r3"),
            "task B's claim must not ride task A's fold"
        );
        let planned = read_planned_at(&r).unwrap();
        assert_eq!(planned.changes.len(), 1, "task B stays open");
        assert_eq!(planned.changes[0].id, b);
        assert_eq!(
            planned.change_map.keys().collect::<Vec<_>>(),
            vec![&element_key(ElementKind::Responsibility, None, "r3")]
        );
    }

    /// A whole-build fold closes every open change and re-seeds both layers
    /// clean of change state.
    #[test]
    fn build_fold_closes_all_changes_and_strips_both_layers() {
        let tmp = tempdir().unwrap();
        let r = ModelRef::ProjectLocal(tmp.path().to_path_buf());
        write_model_at(&r, &model_with_resps(&[("r1", "exists")])).unwrap();

        let mut built = model_with_resps(&[("r1", "exists"), ("r2", "built")]);
        let id = open_change(&mut built, "the build task", 100);
        tag(&mut built, &[element_key(ElementKind::Responsibility, None, "r2")], &id);

        fold_built_model(&r, &built).unwrap();

        let committed = read_model_at(&r).unwrap();
        assert!(committed.changes.is_empty() && committed.change_map.is_empty());
        let planned = read_planned_at(&r).unwrap();
        assert!(planned.changes.is_empty() && planned.change_map.is_empty());
        let closes: Vec<_> =
            read_history(&r).into_iter().filter(|e| e.kind == EventKind::Change).collect();
        assert_eq!(closes.len(), 1);
        assert_eq!(closes[0].change_id.as_deref(), Some(id.as_str()));
    }

    /// The hand-close escape hatch: an empty (stranded) change closes and is
    /// recorded "abandoned"; a change with tagged entries refuses (it closes
    /// through its entries); an unknown id is an error.
    #[test]
    fn close_change_discards_only_empty_ledgers() {
        let tmp = tempdir().unwrap();
        let r = ModelRef::ProjectLocal(tmp.path().to_path_buf());
        write_model_at(&r, &model_with_resps(&[("r1", "exists")])).unwrap();

        let mut plan = model_with_resps(&[("r1", "exists"), ("r2", "new A")]);
        let tagged = open_change(&mut plan, "real work", 100);
        tag(&mut plan, &[element_key(ElementKind::Responsibility, None, "r2")], &tagged);
        let stranded = open_change(&mut plan, "opened then orphaned", 200);
        write_planned_at(&r, &plan).unwrap();

        assert!(close_change(&r, &tagged).unwrap_err().contains("1 tagged entry"));
        assert!(close_change(&r, "chg-99").unwrap_err().contains("no open change"));

        let meta = close_change(&r, &stranded).unwrap();
        assert_eq!(meta.rationale, "opened then orphaned");
        let planned = read_planned_at(&r).unwrap();
        assert_eq!(planned.changes.len(), 1, "only the tagged change remains open");
        assert_eq!(planned.changes[0].id, tagged);

        let closes: Vec<_> =
            read_history(&r).into_iter().filter(|e| e.kind == EventKind::Change).collect();
        assert_eq!(closes.len(), 1);
        assert_eq!(closes[0].change_id.as_deref(), Some(stranded.as_str()));
        assert_eq!(closes[0].driver, "abandoned");
        assert_eq!(closes[0].rows[0].text, "opened then orphaned");
    }

    #[test]
    fn element_keys_round_trip_and_self_classify() {
        for (kind, owner, id) in [
            (ElementKind::Node, None, "node-3"),
            (ElementKind::Link, None, "link-1"),
            (ElementKind::Group, None, "grp-2"),
            (ElementKind::Responsibility, None, "resp-9"),
            (ElementKind::Property, Some("node-3"), "odooMapping"),
        ] {
            let key = element_key(kind, owner, id);
            let (k, o, i) = parse_key(&key).unwrap();
            assert_eq!(k, kind);
            assert_eq!(o.as_deref(), owner);
            assert_eq!(i, id);
        }
        // A label may itself contain the separator; the owner side never does.
        let key = element_key(ElementKind::Property, Some("node-3"), "std::vec::Vec");
        let (_, o, i) = parse_key(&key).unwrap();
        assert_eq!(o.as_deref(), Some("node-3"));
        assert_eq!(i, "std::vec::Vec");
        assert!(parse_key("bogus").is_none());
        assert!(parse_key("widget:x").is_none());
    }

    #[test]
    fn open_change_mints_past_every_seen_id() {
        let mut m = ScryModel::new();
        let a = open_change(&mut m, "  first task  ", 100);
        assert_eq!(a, "chg-1");
        assert_eq!(m.changes[0].rationale, "first task");
        // A tag left by a closed change still advances the mint.
        m.change_map.insert("node:n1".into(), "chg-7".into());
        let b = open_change(&mut m, "second", 200);
        assert_eq!(b, "chg-8");
        assert_eq!(m.changes.len(), 2);
    }

    #[test]
    fn tag_reports_cross_change_collisions_and_last_writer_wins() {
        let mut m = ScryModel::new();
        let keys = vec!["resp:r1".to_string(), "resp:r2".to_string()];
        assert!(tag(&mut m, &keys, "chg-1").is_empty());
        // Same change re-tagging is not a conflict.
        assert!(tag(&mut m, &keys[..1].to_vec(), "chg-1").is_empty());
        let conflicts = tag(&mut m, &keys, "chg-2");
        assert_eq!(
            conflicts,
            vec![("resp:r1".into(), "chg-1".into()), ("resp:r2".into(), "chg-1".into())]
        );
        assert_eq!(m.change_map["resp:r1"], "chg-2");
    }

    /// Retag by the CARRIER: a node id takes the node's own pending change and
    /// every pending element under it — the unit `get_pending` shows — so
    /// re-filing a mis-filed task is one id, not a hand-assembled key list.
    #[test]
    fn retag_moves_a_carrier_with_everything_pending_under_it() {
        let committed = model_with_resps(&[("r1", "exists")]);
        let mut plan = model_with_resps(&[("r1", "exists"), ("r2", "new A"), ("r3", "new B")]);
        let wrong = open_change(&mut plan, "the wrong home", 100);
        let right = open_change(&mut plan, "where it belongs", 200);
        tag(
            &mut plan,
            &[
                element_key(ElementKind::Responsibility, None, "r2"),
                element_key(ElementKind::Responsibility, None, "r3"),
            ],
            &wrong,
        );

        let out = retag(&committed, &mut plan, &["n1".into()], Some(&right)).unwrap();

        assert_eq!(out.moved.len(), 2, "both claims under the node moved: {:?}", out.moved);
        assert!(out.unmatched.is_empty());
        assert_eq!(plan.change_map["resp:r2"], right);
        assert_eq!(plan.change_map["resp:r3"], right);
        assert!(out.moved.iter().all(|(_, from)| from.as_deref() == Some(wrong.as_str())));
    }

    /// The other three target forms: one element by its own id, a whole change
    /// by `chg-N`, and `unfiled` for everything untagged. Detaching (`to:
    /// None`) drops the key rather than pointing it somewhere.
    #[test]
    fn retag_targets_elements_whole_changes_and_the_unfiled_bucket() {
        let committed = model_with_resps(&[("r1", "exists")]);
        let mut plan = model_with_resps(&[("r1", "exists"), ("r2", "new A"), ("r3", "new B")]);
        let a = open_change(&mut plan, "change A", 100);
        let b = open_change(&mut plan, "change B", 200);
        tag(&mut plan, &[element_key(ElementKind::Responsibility, None, "r2")], &a);
        // r3 stays unfiled.

        // One element by id, into B.
        let out = retag(&committed, &mut plan, &["r3".into()], Some(&b)).unwrap();
        assert_eq!(out.moved, vec![("resp:r3".to_string(), None)]);

        // A whole change: everything filed under A joins B.
        let out = retag(&committed, &mut plan, &[a.clone()], Some(&b)).unwrap();
        assert_eq!(out.moved, vec![("resp:r2".to_string(), Some(a.clone()))]);
        assert_eq!(plan.change_map["resp:r2"], b);

        // Detach everything under B back to unfiled.
        let out = retag(&committed, &mut plan, &[b.clone()], None).unwrap();
        assert_eq!(out.moved.len(), 2);
        assert!(plan.change_map.is_empty(), "detached keys leave the map: {:?}", plan.change_map);

        // And now "unfiled" is what names them.
        let out = retag(&committed, &mut plan, &["unfiled".into()], Some(&a)).unwrap();
        assert_eq!(out.moved.len(), 2);
    }

    /// An id with no pending work is reported, never silently swallowed — and
    /// an already-correct filing is not counted as a move. A caller that reads
    /// "moved 0" must be able to tell "nothing to do" from "wrong id".
    #[test]
    fn retag_reports_unmatched_ids_and_stays_idempotent() {
        let committed = model_with_resps(&[("r1", "exists")]);
        let mut plan = model_with_resps(&[("r1", "exists"), ("r2", "new A")]);
        let a = open_change(&mut plan, "change A", 100);
        tag(&mut plan, &[element_key(ElementKind::Responsibility, None, "r2")], &a);

        // r1 is committed and unchanged — it carries no pending entry.
        let out = retag(&committed, &mut plan, &["r1".into(), "r2".into()], Some(&a)).unwrap();
        assert_eq!(out.unmatched, vec!["r1".to_string()]);
        assert!(out.moved.is_empty(), "r2 was already filed under A");

        // A destination that does not exist is refused outright.
        assert!(retag(&committed, &mut plan, &["r2".into()], Some("chg-99")).is_err());
    }
}
