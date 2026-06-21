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

use crate::ScryModel;
use serde::Serialize;
use std::collections::BTreeMap;

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
#[serde(tag = "type", rename_all = "camelCase")]
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
            visual: None,
            appearance: None,
            notes: None,
        }
    }

    fn resp(id: &str, statement: &str) -> Responsibility {
        Responsibility {
            id: id.to_string(),
            statement: statement.to_string(),
            vagrant: None,
            stale: None,
            stale_proposal: None,
            directives: Vec::new(),
            last_touched_at: None,
        }
    }

    fn link(id: &str, src: &str, dst: &str) -> Link {
        Link {
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
}
