//! Model diff — the PLANNING substrate.
//!
//! Scryer holds three layers: `planned` (the intent the canvas and agent edit),
//! `model` (the committed source of truth, `model.scry`), and `code` (the
//! implementation). The two diffs between them drive everything:
//!
//!   1. **plan diff** = `model` ↔ `planned` — what we intend to change. Rendered
//!      git-status style; the agent's work queue ("make code satisfy the plan").
//!      An element commits (planned folds into model) once the code backs it.
//!   2. **drift diff** = `model` ↔ `code` — where committed spec and reality
//!      disagree (the reconcile-from-codebase surface).
//!
//! [`diff`] is the one engine behind both: given a `from` model (the base) and a
//! `to` model (the target), it reports per element how `to` diverges from `from`
//! — `Added` / `Deleted` / `Moved` / `Repointed` / `Reworded`. For the plan
//! diff, call `diff(model, planned)`: `Added` then means "in the plan, not yet
//! committed", `Deleted` means "marked for removal".
//!
//! Identity is carried by stable ids (nodes, links, responsibilities), so a
//! reparent reads as `Moved` and a relabel as `Reworded` — never as a spurious
//! delete-plus-add.
//!
//! Coverage: nodes, links, responsibilities, properties, and groups.
//! Properties have no id, so they are keyed by `(owner node, label)` — a label
//! change reads as delete-plus-add, which is acceptable for plain data fields.

use crate::{Kind, ScryModel};
use serde::Serialize;
use std::collections::BTreeMap;

/// Stable string label for a node kind, for surfacing a `kind` change in a diff.
fn kind_label(k: Kind) -> &'static str {
    match k {
        Kind::Person => "person",
        Kind::System => "system",
        Kind::Container => "container",
        Kind::Component => "component",
        Kind::Symbol => "symbol",
    }
}

/// Which kind of element a change concerns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ElementKind {
    Node,
    Link,
    Responsibility,
    Property,
    Group,
}

/// A single divergence of `to` from `from` for one element. Several can stack on
/// one element (e.g. a responsibility both `Moved` and `Reworded`).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Change {
    /// Present in `to`, absent in `from`.
    Added,
    /// Present in `from`, absent in `to`.
    Deleted,
    /// Same id, different owner — a node's `parentId`, or the node/group a
    /// responsibility lives in. `from`/`to` are the owner ids (None = root).
    Moved {
        from: Option<String>,
        to: Option<String>,
    },
    /// A link's endpoints changed. Endpoints that didn't move have equal
    /// `from`/`to`; the UI shows whichever differs.
    Repointed {
        src_from: String,
        src_to: String,
        dst_from: String,
        dst_to: String,
    },
    /// A truth-bearing text field changed.
    Reworded {
        field: String,
        from: String,
        to: String,
    },
    /// A group's membership changed (member node ids added / removed).
    MembersChanged {
        added: Vec<String>,
        removed: Vec<String>,
    },
}

/// Every divergence recorded against one element.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ElementChange {
    pub kind: ElementKind,
    pub id: String,
    /// Owning node/group id for a responsibility; None for nodes and links.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_id: Option<String>,
    /// Human-facing label for display (the element's name / statement).
    pub label: String,
    pub changes: Vec<Change>,
}

/// The full set of element changes taking `from` to `to`.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelDiff {
    pub changes: Vec<ElementChange>,
}

impl ModelDiff {
    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }
}

/// Push a `Reworded` change when two strings differ.
fn reword(changes: &mut Vec<Change>, field: &str, from: &str, to: &str) {
    if from != to {
        changes.push(Change::Reworded {
            field: field.to_string(),
            from: from.to_string(),
            to: to.to_string(),
        });
    }
}

/// Compute how `to` diverges from `from`. For the plan diff, pass
/// `diff(model, planned)`; for the drift diff, `diff(model, code_model)`.
pub fn diff(from: &ScryModel, to: &ScryModel) -> ModelDiff {
    let mut out = ModelDiff::default();
    diff_nodes(from, to, &mut out);
    diff_links(from, to, &mut out);
    diff_responsibilities(from, to, &mut out);
    diff_properties(from, to, &mut out);
    diff_groups(from, to, &mut out);
    out
}

/// The plan-diff ELEMENTS outstanding — one per diverging element (a reworded
/// claim, an added property, a repointed link), which is the queue `get_pending`
/// hands the agent and the finer of the two altitudes every status surface
/// reports. Vagrant (code-discovered) elements are excluded: they await a drift
/// verdict and are never implement-queue work.
///
/// The counterpart to [`plan_carrier_count`], which folds these same diffs under
/// their owning node/group. Report BOTH or the app and the agent end up quoting
/// different numbers for the same plan.
pub fn pending_elements(committed: &ScryModel, planned: &ScryModel) -> Vec<ElementChange> {
    let plan = diff(committed, planned);
    plan.changes
        .into_iter()
        .filter(|ch| {
            let vagrant = match ch.kind {
                ElementKind::Node => planned
                    .nodes
                    .iter()
                    .any(|n| n.id == ch.id && n.vagrant == Some(true)),
                ElementKind::Responsibility => planned
                    .nodes
                    .iter()
                    .flat_map(|n| n.responsibilities.iter())
                    .chain(planned.groups.iter().flat_map(|g| g.responsibilities.iter()))
                    .any(|r| r.id == ch.id && r.vagrant == Some(true)),
                ElementKind::Property => ch.owner_id.as_deref().is_some_and(|oid| {
                    planned.nodes.iter().any(|n| {
                        n.id == oid
                            && n.properties
                                .iter()
                                .any(|p| p.label == ch.id && p.vagrant == Some(true))
                    })
                }),
                _ => false,
            };
            !vagrant
        })
        .collect()
}

/// [`pending_elements`] counted — the agent-facing "N pending".
pub fn pending_element_count(committed: &ScryModel, planned: &ScryModel) -> usize {
    pending_elements(committed, planned).len()
}

/// Count the plan-change CARRIERS — the node/group cards the Changes page lists
/// and the tree's Changes lens counts, i.e. the number a user reads in-app.
///
/// This differs from `diff(committed, planned).changes.len()`: element diffs
/// (a reworded claim, an added property, a repointed link) fold under the ONE
/// node or group that owns them, so a node that grew six claims is one carrier,
/// not six changes; and drift (vagrant) content is stripped, since that is the
/// reconcile axis, never a planned edit. A carrier left with no real change
/// after that stripping is not counted.
///
/// Kept in lockstep with the frontend's `collectPlanEntries`
/// (src/changeMarks.ts) — the single definition every in-app surface counts —
/// so the ambient status line agrees with what the canvas shows.
pub fn plan_carrier_count(committed: &ScryModel, planned: &ScryModel) -> usize {
    use std::collections::{HashMap, HashSet};
    let plan = diff(committed, planned);

    // Group ids across both layers — a deleted group lives only in committed.
    let is_group: HashSet<&str> = planned
        .groups
        .iter()
        .chain(committed.groups.iter())
        .map(|g| g.id.as_str())
        .collect();
    let node_by_id: HashMap<&str, &crate::Node> =
        planned.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    // Links by id across both layers — a dropped link lives only in committed.
    let link_by_id: HashMap<&str, &crate::Link> = committed
        .links
        .iter()
        .chain(planned.links.iter())
        .map(|l| (l.id.as_str(), l))
        .collect();

    // A changed link is carried by its source node — the side that performs the
    // relationship. Collect each source node's outgoing link changes.
    let mut links_by_src: HashMap<&str, Vec<&[Change]>> = HashMap::new();
    // Each node/group's own field/structural changes, and the resp/property
    // changes grouped under their owning id.
    let mut node_own: HashMap<&str, &[Change]> = HashMap::new();
    let mut group_own: HashMap<&str, &[Change]> = HashMap::new();
    let mut by_owner: HashMap<&str, Vec<&ElementChange>> = HashMap::new();
    for ec in &plan.changes {
        match ec.kind {
            ElementKind::Node => {
                node_own.insert(ec.id.as_str(), ec.changes.as_slice());
            }
            ElementKind::Group => {
                group_own.insert(ec.id.as_str(), ec.changes.as_slice());
            }
            ElementKind::Responsibility | ElementKind::Property => {
                if let Some(owner) = ec.owner_id.as_deref() {
                    by_owner.entry(owner).or_default().push(ec);
                }
            }
            ElementKind::Link => {
                if let Some(link) = link_by_id.get(ec.id.as_str()) {
                    links_by_src
                        .entry(link.src.as_str())
                        .or_default()
                        .push(ec.changes.as_slice());
                }
            }
        }
    }

    // Every node/group that carries a change: its own, content it owns, or a
    // link it performs (links attach to their source node only).
    let mut node_ids: HashSet<&str> = node_own.keys().copied().collect();
    let mut group_ids: HashSet<&str> = group_own.keys().copied().collect();
    for owner in by_owner.keys() {
        if is_group.contains(owner) {
            group_ids.insert(owner);
        } else {
            node_ids.insert(owner);
        }
    }
    for host in links_by_src.keys() {
        if !is_group.contains(host) {
            node_ids.insert(host);
        }
    }

    // A carrier counts when it still holds a real (non-drift) plan change once
    // vagrant content is stripped — the `classifyPlan` null case dropped here.
    let carries = |is_node: bool, id: &str| -> bool {
        let node = if is_node { node_by_id.get(id).copied() } else { None };
        // A vagrant node's own change is code-first review, not a plan edit.
        let node_vagrant = node.is_some_and(|n| n.vagrant == Some(true));
        let own: Option<&[Change]> = if node_vagrant {
            None
        } else if is_node {
            node_own.get(id).copied()
        } else {
            group_own.get(id).copied()
        };
        // diff never emits an empty change set, so a present `own` is a real edit.
        if own.is_some_and(|o| !o.is_empty()) {
            return true;
        }
        // Owned resp/property changes, with vagrant (drift) ones filtered out.
        let vagrant_resps: HashSet<&str> = node
            .map(|n| {
                n.responsibilities
                    .iter()
                    .filter(|r| r.vagrant == Some(true))
                    .map(|r| r.id.as_str())
                    .collect()
            })
            .unwrap_or_default();
        let vagrant_props: HashSet<&str> = node
            .map(|n| {
                n.properties
                    .iter()
                    .filter(|p| p.vagrant == Some(true))
                    .map(|p| p.label.as_str())
                    .collect()
            })
            .unwrap_or_default();
        let has_child = by_owner.get(id).is_some_and(|ecs| {
            ecs.iter().any(|ec| match ec.kind {
                ElementKind::Responsibility => !vagrant_resps.contains(ec.id.as_str()),
                ElementKind::Property => !vagrant_props.contains(ec.id.as_str()),
                _ => true,
            })
        });
        let has_link = is_node && links_by_src.contains_key(id);
        has_child || has_link
    };

    node_ids.iter().filter(|id| carries(true, id)).count()
        + group_ids.iter().filter(|id| carries(false, id)).count()
}

fn diff_nodes(from: &ScryModel, to: &ScryModel, out: &mut ModelDiff) {
    let from_by: BTreeMap<&str, _> = from.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    let to_by: BTreeMap<&str, _> = to.nodes.iter().map(|n| (n.id.as_str(), n)).collect();

    for (id, n) in &to_by {
        match from_by.get(id) {
            None => out.changes.push(ElementChange {
                kind: ElementKind::Node,
                id: (*id).to_string(),
                owner_id: None,
                label: n.name.clone(),
                changes: vec![Change::Added],
            }),
            Some(prev) => {
                let mut changes = Vec::new();
                if prev.parent_id != n.parent_id {
                    changes.push(Change::Moved {
                        from: prev.parent_id.clone(),
                        to: n.parent_id.clone(),
                    });
                }
                reword(&mut changes, "name", &prev.name, &n.name);
                reword(
                    &mut changes,
                    "technology",
                    prev.technology.as_deref().unwrap_or(""),
                    n.technology.as_deref().unwrap_or(""),
                );
                reword(
                    &mut changes,
                    "description",
                    prev.description.as_deref().unwrap_or(""),
                    n.description.as_deref().unwrap_or(""),
                );
                reword(
                    &mut changes,
                    "directives",
                    &prev.directives.join("\n"),
                    &n.directives.join("\n"),
                );
                // `kind` and `external` are truth-bearing, not cosmetic: `kind`
                // sets a node's altitude (parent/child legality), `external` flips
                // anchorability and link legality. A change to either is real plan
                // work, so surface it — otherwise it folds invisibly, or never.
                reword(&mut changes, "kind", kind_label(prev.kind), kind_label(n.kind));
                // Normalize None and Some(false) — both "not external" — so only a
                // genuine flip registers, never a serialization difference.
                reword(
                    &mut changes,
                    "external",
                    if prev.external == Some(true) { "true" } else { "false" },
                    if n.external == Some(true) { "true" } else { "false" },
                );
                if !changes.is_empty() {
                    out.changes.push(ElementChange {
                        kind: ElementKind::Node,
                        id: (*id).to_string(),
                        owner_id: None,
                        label: n.name.clone(),
                        changes,
                    });
                }
            }
        }
    }
    for (id, n) in &from_by {
        if !to_by.contains_key(id) {
            out.changes.push(ElementChange {
                kind: ElementKind::Node,
                id: (*id).to_string(),
                owner_id: None,
                label: n.name.clone(),
                changes: vec![Change::Deleted],
            });
        }
    }
}

fn diff_links(from: &ScryModel, to: &ScryModel, out: &mut ModelDiff) {
    let from_by: BTreeMap<&str, _> = from.links.iter().map(|l| (l.id.as_str(), l)).collect();
    let to_by: BTreeMap<&str, _> = to.links.iter().map(|l| (l.id.as_str(), l)).collect();

    for (id, l) in &to_by {
        match from_by.get(id) {
            None => out.changes.push(ElementChange {
                kind: ElementKind::Link,
                id: (*id).to_string(),
                owner_id: None,
                label: l.label.clone(),
                changes: vec![Change::Added],
            }),
            Some(prev) => {
                let mut changes = Vec::new();
                if prev.src != l.src || prev.dst != l.dst {
                    changes.push(Change::Repointed {
                        src_from: prev.src.clone(),
                        src_to: l.src.clone(),
                        dst_from: prev.dst.clone(),
                        dst_to: l.dst.clone(),
                    });
                }
                reword(&mut changes, "label", &prev.label, &l.label);
                reword(
                    &mut changes,
                    "method",
                    prev.method.as_deref().unwrap_or(""),
                    l.method.as_deref().unwrap_or(""),
                );
                if !changes.is_empty() {
                    out.changes.push(ElementChange {
                        kind: ElementKind::Link,
                        id: (*id).to_string(),
                        owner_id: None,
                        label: l.label.clone(),
                        changes,
                    });
                }
            }
        }
    }
    for (id, l) in &from_by {
        if !to_by.contains_key(id) {
            out.changes.push(ElementChange {
                kind: ElementKind::Link,
                id: (*id).to_string(),
                owner_id: None,
                label: l.label.clone(),
                changes: vec![Change::Deleted],
            });
        }
    }
}

/// A responsibility together with the id of the node or group it lives in.
struct OwnedResp<'a> {
    owner_id: String,
    resp: &'a crate::Responsibility,
}

/// Index every responsibility in the model by its id, recording its owner
/// (node or group). Responsibilities carry stable ids, so moving one between
/// owners is a `Moved`, not a delete-plus-add.
fn index_responsibilities(model: &ScryModel) -> BTreeMap<&str, OwnedResp<'_>> {
    let mut map = BTreeMap::new();
    for node in &model.nodes {
        for r in &node.responsibilities {
            map.insert(
                r.id.as_str(),
                OwnedResp {
                    owner_id: node.id.clone(),
                    resp: r,
                },
            );
        }
    }
    for group in &model.groups {
        for r in &group.responsibilities {
            map.insert(
                r.id.as_str(),
                OwnedResp {
                    owner_id: group.id.clone(),
                    resp: r,
                },
            );
        }
    }
    map
}

fn diff_responsibilities(from: &ScryModel, to: &ScryModel, out: &mut ModelDiff) {
    let from_by = index_responsibilities(from);
    let to_by = index_responsibilities(to);

    for (id, owned) in &to_by {
        match from_by.get(id) {
            None => out.changes.push(ElementChange {
                kind: ElementKind::Responsibility,
                id: (*id).to_string(),
                owner_id: Some(owned.owner_id.clone()),
                label: owned.resp.statement.clone(),
                changes: vec![Change::Added],
            }),
            Some(prev) => {
                let mut changes = Vec::new();
                if prev.owner_id != owned.owner_id {
                    changes.push(Change::Moved {
                        from: Some(prev.owner_id.clone()),
                        to: Some(owned.owner_id.clone()),
                    });
                }
                reword(
                    &mut changes,
                    "statement",
                    &prev.resp.statement,
                    &owned.resp.statement,
                );
                reword(
                    &mut changes,
                    "directives",
                    &prev.resp.directives.join("\n"),
                    &owned.resp.directives.join("\n"),
                );
                if !changes.is_empty() {
                    out.changes.push(ElementChange {
                        kind: ElementKind::Responsibility,
                        id: (*id).to_string(),
                        owner_id: Some(owned.owner_id.clone()),
                        label: owned.resp.statement.clone(),
                        changes,
                    });
                }
            }
        }
    }
    for (id, owned) in &from_by {
        if !to_by.contains_key(id) {
            out.changes.push(ElementChange {
                kind: ElementKind::Responsibility,
                id: (*id).to_string(),
                owner_id: Some(owned.owner_id.clone()),
                label: owned.resp.statement.clone(),
                changes: vec![Change::Deleted],
            });
        }
    }
}

/// Properties have no id, so identity is `(owner node id, label)`. A label
/// change therefore reads as a delete plus an add — acceptable for data fields.
fn diff_properties(from: &ScryModel, to: &ScryModel, out: &mut ModelDiff) {
    fn index(model: &ScryModel) -> BTreeMap<(String, String), &crate::SchemaProperty> {
        let mut map = BTreeMap::new();
        for node in &model.nodes {
            for p in &node.properties {
                map.insert((node.id.clone(), p.label.clone()), p);
            }
        }
        map
    }
    let from_by = index(from);
    let to_by = index(to);

    for ((owner, label), p) in &to_by {
        match from_by.get(&(owner.clone(), label.clone())) {
            None => out.changes.push(ElementChange {
                kind: ElementKind::Property,
                id: label.clone(),
                owner_id: Some(owner.clone()),
                label: label.clone(),
                changes: vec![Change::Added],
            }),
            Some(prev) => {
                let mut changes = Vec::new();
                reword(&mut changes, "description", &prev.description, &p.description);
                if !changes.is_empty() {
                    out.changes.push(ElementChange {
                        kind: ElementKind::Property,
                        id: label.clone(),
                        owner_id: Some(owner.clone()),
                        label: label.clone(),
                        changes,
                    });
                }
            }
        }
    }
    for ((owner, label), _) in &from_by {
        if !to_by.contains_key(&(owner.clone(), label.clone())) {
            out.changes.push(ElementChange {
                kind: ElementKind::Property,
                id: label.clone(),
                owner_id: Some(owner.clone()),
                label: label.clone(),
                changes: vec![Change::Deleted],
            });
        }
    }
}

/// A group's anchor — the node level or parent group it sits under. Prefer the
/// parent group (nesting); fall back to the anchoring node level.
fn group_owner(g: &crate::Group) -> Option<String> {
    g.parent_group_id.clone().or_else(|| g.parent_node_id.clone())
}

fn diff_groups(from: &ScryModel, to: &ScryModel, out: &mut ModelDiff) {
    let from_by: BTreeMap<&str, _> = from.groups.iter().map(|g| (g.id.as_str(), g)).collect();
    let to_by: BTreeMap<&str, _> = to.groups.iter().map(|g| (g.id.as_str(), g)).collect();

    for (id, g) in &to_by {
        match from_by.get(id) {
            None => out.changes.push(ElementChange {
                kind: ElementKind::Group,
                id: (*id).to_string(),
                owner_id: None,
                label: g.name.clone(),
                changes: vec![Change::Added],
            }),
            Some(prev) => {
                let mut changes = Vec::new();
                if group_owner(prev) != group_owner(g) {
                    changes.push(Change::Moved {
                        from: group_owner(prev),
                        to: group_owner(g),
                    });
                }
                reword(&mut changes, "name", &prev.name, &g.name);
                reword(
                    &mut changes,
                    "description",
                    prev.description.as_deref().unwrap_or(""),
                    g.description.as_deref().unwrap_or(""),
                );
                let added: Vec<String> = g
                    .member_ids
                    .iter()
                    .filter(|m| !prev.member_ids.contains(m))
                    .cloned()
                    .collect();
                let removed: Vec<String> = prev
                    .member_ids
                    .iter()
                    .filter(|m| !g.member_ids.contains(m))
                    .cloned()
                    .collect();
                if !added.is_empty() || !removed.is_empty() {
                    changes.push(Change::MembersChanged { added, removed });
                }
                if !changes.is_empty() {
                    out.changes.push(ElementChange {
                        kind: ElementKind::Group,
                        id: (*id).to_string(),
                        owner_id: None,
                        label: g.name.clone(),
                        changes,
                    });
                }
            }
        }
    }
    for (id, g) in &from_by {
        if !to_by.contains_key(id) {
            out.changes.push(ElementChange {
                kind: ElementKind::Group,
                id: (*id).to_string(),
                owner_id: None,
                label: g.name.clone(),
                changes: vec![Change::Deleted],
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Kind, Link, Node, Responsibility};

    fn node(id: &str, name: &str, parent: Option<&str>) -> Node {
        Node {
            style: None,
            layer: None,
            id: id.to_string(),
            kind: Kind::Component,
            name: name.to_string(),
            vagrant: None,
            stale: None,
            parent_id: parent.map(|s| s.to_string()),
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

    fn resp(id: &str, statement: &str) -> Responsibility {
        Responsibility {
            concern: None,
            id: id.to_string(),
            statement: statement.to_string(),
            vagrant: None,
            stale: None,
            stale_proposal: None,
            directives: Vec::new(),
            last_touched_at: None,
            vagrant_origin: None,
            approved_statement: None,
        }
    }

    fn link(id: &str, src: &str, dst: &str) -> Link {
        Link {
            kind: None,
            id: id.to_string(),
            src: src.to_string(),
            dst: dst.to_string(),
            label: String::new(),
            method: None,
        }
    }

    fn find<'a>(d: &'a ModelDiff, id: &str) -> &'a ElementChange {
        d.changes
            .iter()
            .find(|c| c.id == id)
            .unwrap_or_else(|| panic!("no change for {id}"))
    }

    #[test]
    fn identical_models_have_no_diff() {
        let mut m = ScryModel::new();
        m.nodes.push(node("n1", "Auth", None));
        assert!(diff(&m, &m).is_empty());
    }

    #[test]
    fn node_added_and_deleted() {
        let mut from = ScryModel::new();
        from.nodes.push(node("n1", "Auth", None));
        let mut to = ScryModel::new();
        to.nodes.push(node("n2", "Billing", None));

        let d = diff(&from, &to);
        assert_eq!(find(&d, "n2").changes, vec![Change::Added]);
        assert_eq!(find(&d, "n1").changes, vec![Change::Deleted]);
    }

    #[test]
    fn node_reparented_is_moved() {
        let mut from = ScryModel::new();
        from.nodes.push(node("a", "A", Some("p1")));
        let mut to = ScryModel::new();
        to.nodes.push(node("a", "A", Some("p2")));

        let d = diff(&from, &to);
        assert_eq!(
            find(&d, "a").changes,
            vec![Change::Moved {
                from: Some("p1".to_string()),
                to: Some("p2".to_string()),
            }]
        );
    }

    #[test]
    fn node_renamed_is_reworded() {
        let mut from = ScryModel::new();
        from.nodes.push(node("a", "Old", None));
        let mut to = ScryModel::new();
        to.nodes.push(node("a", "New", None));

        let d = diff(&from, &to);
        assert_eq!(
            find(&d, "a").changes,
            vec![Change::Reworded {
                field: "name".to_string(),
                from: "Old".to_string(),
                to: "New".to_string(),
            }]
        );
    }

    /// `kind` and `external` are truth-bearing, so a change to either must surface
    /// as a plan item — not fold invisibly. And None/Some(false) are the same
    /// "not external", so that pairing must NOT register.
    #[test]
    fn node_kind_and_external_changes_surface() {
        // kind: container → component
        let mut from = ScryModel::new();
        let mut a = node("a", "A", None);
        a.kind = Kind::Container;
        from.nodes.push(a);
        let mut to = ScryModel::new();
        let mut a2 = node("a", "A", None);
        a2.kind = Kind::Component;
        to.nodes.push(a2);
        assert_eq!(
            find(&diff(&from, &to), "a").changes,
            vec![Change::Reworded {
                field: "kind".to_string(),
                from: "container".to_string(),
                to: "component".to_string(),
            }]
        );

        // external: false → true (truth-bearing flip)
        let mut from = ScryModel::new();
        from.nodes.push(node("a", "A", None)); // external: None
        let mut to = ScryModel::new();
        let mut ext = node("a", "A", None);
        ext.external = Some(true);
        to.nodes.push(ext);
        assert_eq!(
            find(&diff(&from, &to), "a").changes,
            vec![Change::Reworded {
                field: "external".to_string(),
                from: "false".to_string(),
                to: "true".to_string(),
            }]
        );

        // None vs Some(false): both "not external" — no phantom change.
        let mut from = ScryModel::new();
        from.nodes.push(node("a", "A", None)); // None
        let mut to = ScryModel::new();
        let mut same = node("a", "A", None);
        same.external = Some(false);
        to.nodes.push(same);
        assert!(diff(&from, &to).is_empty(), "None and Some(false) must not differ");
    }

    /// A canvas placement is pure cosmetics: dragging a node on the map must
    /// never surface as a plan change (a drag is not pending work).
    #[test]
    fn position_only_change_is_not_a_plan_change() {
        let mut from = ScryModel::new();
        from.nodes.push(node("a", "A", None)); // position: None
        let mut to = ScryModel::new();
        let mut placed = node("a", "A", None);
        placed.position = Some(crate::Position { x: 42.0, y: -7.0 });
        to.nodes.push(placed);
        assert!(diff(&from, &to).is_empty(), "a drag must not enter the plan queue");
    }

    #[test]
    fn link_repointed() {
        let mut from = ScryModel::new();
        from.links.push(link("l1", "a", "b"));
        let mut to = ScryModel::new();
        to.links.push(link("l1", "a", "c"));

        let d = diff(&from, &to);
        assert_eq!(
            find(&d, "l1").changes,
            vec![Change::Repointed {
                src_from: "a".to_string(),
                src_to: "a".to_string(),
                dst_from: "b".to_string(),
                dst_to: "c".to_string(),
            }]
        );
    }

    #[test]
    fn responsibility_moved_between_nodes() {
        let mut from = ScryModel::new();
        let mut a = node("a", "A", None);
        a.responsibilities.push(resp("r1", "do the thing"));
        from.nodes.push(a);
        from.nodes.push(node("b", "B", None));

        let mut to = ScryModel::new();
        to.nodes.push(node("a", "A", None));
        let mut b = node("b", "B", None);
        b.responsibilities.push(resp("r1", "do the thing"));
        to.nodes.push(b);

        let d = diff(&from, &to);
        assert_eq!(
            find(&d, "r1").changes,
            vec![Change::Moved {
                from: Some("a".to_string()),
                to: Some("b".to_string()),
            }]
        );
    }

    #[test]
    fn responsibility_moved_and_reworded_stacks() {
        let mut from = ScryModel::new();
        let mut a = node("a", "A", None);
        a.responsibilities.push(resp("r1", "old statement"));
        from.nodes.push(a);
        from.nodes.push(node("b", "B", None));

        let mut to = ScryModel::new();
        to.nodes.push(node("a", "A", None));
        let mut b = node("b", "B", None);
        b.responsibilities.push(resp("r1", "new statement"));
        to.nodes.push(b);

        let d = diff(&from, &to);
        let c = find(&d, "r1");
        assert_eq!(c.changes.len(), 2);
        assert!(c.changes.contains(&Change::Moved {
            from: Some("a".to_string()),
            to: Some("b".to_string()),
        }));
        assert!(c.changes.contains(&Change::Reworded {
            field: "statement".to_string(),
            from: "old statement".to_string(),
            to: "new statement".to_string(),
        }));
    }

    fn prop(label: &str, description: &str) -> crate::SchemaProperty {
        crate::SchemaProperty {
            label: label.to_string(),
            description: description.to_string(),
            vagrant: None,
            stale: None,
            last_touched_at: None,
        }
    }

    fn group(id: &str, name: &str, members: &[&str]) -> crate::Group {
        crate::Group {
            id: id.to_string(),
            name: name.to_string(),
            description: None,
            member_ids: members.iter().map(|s| s.to_string()).collect(),
            parent_group_id: None,
            parent_node_id: None,
            responsibilities: Vec::new(),
            icon: None,
        }
    }

    #[test]
    fn property_added_and_reworded() {
        let mut from = ScryModel::new();
        let mut a = node("a", "A", None);
        a.properties.push(prop("email", "old desc"));
        from.nodes.push(a);

        let mut to = ScryModel::new();
        let mut a2 = node("a", "A", None);
        a2.properties.push(prop("email", "new desc"));
        a2.properties.push(prop("name", "the name"));
        to.nodes.push(a2);

        let d = diff(&from, &to);
        assert_eq!(find(&d, "name").changes, vec![Change::Added]);
        assert_eq!(
            find(&d, "email").changes,
            vec![Change::Reworded {
                field: "description".to_string(),
                from: "old desc".to_string(),
                to: "new desc".to_string(),
            }]
        );
    }

    #[test]
    fn group_membership_changed() {
        let mut from = ScryModel::new();
        from.groups.push(group("g1", "Core", &["a", "b"]));
        let mut to = ScryModel::new();
        to.groups.push(group("g1", "Core", &["a", "c"]));

        let d = diff(&from, &to);
        assert_eq!(
            find(&d, "g1").changes,
            vec![Change::MembersChanged {
                added: vec!["c".to_string()],
                removed: vec!["b".to_string()],
            }]
        );
    }

    #[test]
    fn carrier_count_folds_elements_and_drops_vagrants() {
        // Committed: A owns one claim, B is empty.
        let mut committed = ScryModel::new();
        let mut a = node("a", "A", None);
        a.responsibilities.push(resp("r1", "does one thing"));
        committed.nodes.push(a);
        committed.nodes.push(node("b", "B", None));

        // Plan: A grows two more claims (folds under A), B gains a VAGRANT claim
        // (drift, not a planned edit), a new node C appears with a link C→A.
        let mut planned = ScryModel::new();
        let mut a2 = node("a", "A", None);
        a2.responsibilities.push(resp("r1", "does one thing"));
        a2.responsibilities.push(resp("r2", "does a second thing"));
        a2.responsibilities.push(resp("r3", "does a third thing"));
        planned.nodes.push(a2);
        let mut b2 = node("b", "B", None);
        let mut rv = resp("rv", "code already does this");
        rv.vagrant = Some(true);
        b2.responsibilities.push(rv);
        planned.nodes.push(b2);
        planned.nodes.push(node("c", "C", None));
        planned.links.push(link("l1", "c", "a"));

        // Five raw element diffs (r2, r3, rv, node C, link l1)…
        assert_eq!(diff(&committed, &planned).changes.len(), 5);
        // …but two carriers: A (its two real claims) and C (new node + its
        // link). B carries only a vagrant claim, so it is not counted.
        assert_eq!(plan_carrier_count(&committed, &planned), 2);
    }

    /// The two altitudes on ONE plan: the element queue an agent implements,
    /// and the cards a user reads. They are different numbers by design — the
    /// bug this pins is a surface quoting one of them as if it were the other.
    #[test]
    fn pending_elements_counts_the_queue_the_carriers_fold() {
        let mut committed = ScryModel::new();
        let mut a = node("a", "A", None);
        a.responsibilities.push(resp("r1", "does one thing"));
        committed.nodes.push(a);
        committed.nodes.push(node("b", "B", None));

        let mut planned = ScryModel::new();
        let mut a2 = node("a", "A", None);
        a2.responsibilities.push(resp("r1", "does one thing"));
        a2.responsibilities.push(resp("r2", "does a second thing"));
        a2.responsibilities.push(resp("r3", "does a third thing"));
        planned.nodes.push(a2);
        let mut b2 = node("b", "B", None);
        let mut rv = resp("rv", "code already does this");
        rv.vagrant = Some(true);
        b2.responsibilities.push(rv);
        planned.nodes.push(b2);
        planned.nodes.push(node("c", "C", None));
        planned.links.push(link("l1", "c", "a"));

        // Four elements owed: r2, r3, node C, link l1 — the vagrant claim is a
        // drift verdict, never implement-queue work.
        assert_eq!(pending_element_count(&committed, &planned), 4);
        let ids: Vec<String> = pending_elements(&committed, &planned)
            .into_iter()
            .map(|e| e.id)
            .collect();
        assert!(!ids.iter().any(|i| i == "rv"), "vagrant content is not pending work: {ids:?}");
        // Same plan, coarser altitude — and never the number an agent is given.
        assert_eq!(plan_carrier_count(&committed, &planned), 2);
    }
}
