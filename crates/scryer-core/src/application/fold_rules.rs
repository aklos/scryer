//! The rules a fold applies: which copy of a node, group or claim lands in the
//! committed model, and what a committed claim keeps. Pure — reading and
//! writing the two layers is `composition::fold`.

use crate::domain::model::{Group, Node, Responsibility, SchemaProperty, ScryModel};
use crate::domain::{changes, diff};
use std::collections::{BTreeMap, HashSet};

/// Locate a responsibility by id anywhere in a model, returning its host id
/// (node or group) and a clone. Responsibility ids are globally unique (the
/// minters seed past every node- and group-owned id), so this is unambiguous.
pub(crate) fn find_responsibility(model: &ScryModel, id: &str) -> Option<(String, Responsibility)> {
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
pub(crate) fn clean_committed_resp(mut resp: Responsibility) -> Responsibility {
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
///
/// `withhold` names claims this fold must NOT carry across even though they
/// are neither vagrant nor foreign — the evidence gate's refusals (a testable
/// claim without a current passing verdict). `prior` is the node's committed
/// copy before the fold: every claim or property that stays behind in the
/// plan (vagrant, foreign, withheld) keeps its COMMITTED ORIGINAL in place
/// rather than vanishing from committed — a reworded-then-withheld claim is
/// still what the code was last verified to do, and dropping it would turn a
/// refusal into a silent deletion.
pub(crate) fn committed_node_copy(
    n: &Node,
    change_map: &BTreeMap<String, String>,
    prior: Option<&Node>,
    withhold: &HashSet<String>,
) -> Node {
    use diff::ElementKind as EK;
    let host_key = changes::element_key(EK::Node, None, &n.id);
    let mut copy = n.clone();
    copy.vagrant = None;
    copy.stale = None;
    let folds_resp = |r: &Responsibility| {
        r.vagrant != Some(true)
            && !withhold.contains(&r.id)
            && !changes::foreign_to_host(
                change_map,
                &host_key,
                &changes::element_key(EK::Responsibility, None, &r.id),
            )
    };
    let folds_prop = |p: &SchemaProperty| {
        p.vagrant != Some(true)
            && !changes::foreign_to_host(
                change_map,
                &host_key,
                &changes::element_key(EK::Property, Some(&n.id), &p.label),
            )
    };
    copy.responsibilities = n
        .responsibilities
        .iter()
        .filter(|r| folds_resp(r))
        .cloned()
        .map(clean_committed_resp)
        .collect();
    copy.properties = n
        .properties
        .iter()
        .filter(|p| folds_prop(p))
        .cloned()
        .map(|mut p| {
            p.stale = None;
            p
        })
        .collect();
    if let Some(prior) = prior {
        // Stay-behind elements that were already committed keep their
        // committed original (the plan still carries the new version, so the
        // entry stays pending). Elements the plan no longer has at all are a
        // planned deletion and DO leave committed here.
        for r in &prior.responsibilities {
            if n.responsibilities.iter().any(|x| x.id == r.id && !folds_resp(x)) {
                copy.responsibilities.push(r.clone());
            }
        }
        for p in &prior.properties {
            if n.properties.iter().any(|x| x.label == p.label && !folds_prop(x)) {
                copy.properties.push(p.clone());
            }
        }
    }
    copy
}

/// The committed copy of a planned group folded into the model. A group has no
/// review markers of its own, but it CAN carry responsibilities (a container
/// group's shared claims — "both surfaces deploy as one Next.js app"), so it
/// gets the same treatment `committed_node_copy` gives a node: drop
/// un-adjudicated `vagrant` claims (they stay in the plan awaiting a verdict),
/// drop claims tagged to a different change (another task's pending work), and
/// clear `stale`/`stale_proposal` on everything that folds. Audit #5 / item A.
pub(crate) fn committed_group_copy(
    g: &Group,
    change_map: &BTreeMap<String, String>,
    prior: Option<&Group>,
    withhold: &HashSet<String>,
) -> Group {
    use diff::ElementKind as EK;
    let host_key = changes::element_key(EK::Group, None, &g.id);
    let mut copy = g.clone();
    let folds = |r: &Responsibility| {
        r.vagrant != Some(true)
            && !withhold.contains(&r.id)
            && !changes::foreign_to_host(
                change_map,
                &host_key,
                &changes::element_key(EK::Responsibility, None, &r.id),
            )
    };
    copy.responsibilities = g
        .responsibilities
        .iter()
        .filter(|r| folds(r))
        .cloned()
        .map(clean_committed_resp)
        .collect();
    if let Some(prior) = prior {
        for r in &prior.responsibilities {
            if g.responsibilities.iter().any(|x| x.id == r.id && !folds(x)) {
                copy.responsibilities.push(r.clone());
            }
        }
    }
    copy
}

/// The committed copy of a plan-only ANCESTOR folded as scaffolding by
/// `commit_plan_only_ancestors`: the node's identity and structure — kind,
/// parent, name, description, technology, external, directives —
/// WITHOUT its responsibilities or properties. Those stay in the plan as
/// pending build work on a now-committed node (the ordinary incremental-add
/// diff shape), so the committed layer keeps reflecting only what the code
/// actually contains.
pub(crate) fn structure_only_copy(n: &Node) -> Node {
    // The claim/property filtering inside committed_node_copy is moot here —
    // everything it kept is cleared — so no change map is consulted.
    let mut copy = committed_node_copy(n, &BTreeMap::new(), None, &HashSet::new());
    copy.responsibilities.clear();
    copy.properties.clear();
    copy
}
