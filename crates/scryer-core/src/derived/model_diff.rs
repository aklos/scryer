//! What one model changed against another, node by node, the way a commit
//! diff reads file by file: each node added, modified or removed, its
//! claims added, reworded or removed under it, and its other changes in
//! words (renamed, moved, re-layered, redescribed, a dependency gained or
//! lost).
//!
//! A claim's id is its test file and test name, so rewording a test whose
//! name is its claim (`it("…")`) gives it a new id. It pairs with its old
//! copy when it is still the same test, same file and same identifier or
//! same line, and reads as reworded. The app's `alignClaims` pairs the same
//! way, so the page and the activity feed agree.

use crate::{Claim, Node, ScryModel, SourceLocation};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Change {
    Added,
    Modified,
    Removed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimChange {
    /// Its id now; the old id for a removed claim.
    pub id: String,
    pub statement: String,
    /// The wording before, for a reworded claim.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub was: Option<String>,
    /// `Modified` reads as reworded.
    pub change: Change,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeChange {
    pub id: String,
    pub name: String,
    /// The node's kind, as the model names it.
    pub kind: String,
    /// Its ancestors' names, outermost first.
    pub path: Vec<String>,
    pub change: Change,
    pub claims: Vec<ClaimChange>,
    /// Its other changes, in words.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelDiff {
    /// Changed nodes in the order of the model they are read from: the
    /// newer for the ones still there, then the removed.
    pub nodes: Vec<NodeChange>,
}

fn same_test(a: Option<&SourceLocation>, b: Option<&SourceLocation>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) if a.pattern == b.pattern => {
            (a.symbol.is_some() && a.symbol == b.symbol) || (a.line.is_some() && a.line == b.line)
        }
        _ => false,
    }
}

/// `before`'s claims paired with the claim in `after` each became, by index
/// into `after`: the same id, else an unpaired claim on the same test.
fn pair(before: &[Claim], after: &[Claim], tests_before: &BTreeMap<String, Vec<SourceLocation>>, tests_after: &BTreeMap<String, Vec<SourceLocation>>) -> Vec<Option<usize>> {
    let before_ids: HashSet<&str> = before.iter().map(|c| c.id.as_str()).collect();
    let mut open: Vec<usize> = (0..after.len()).filter(|&i| !before_ids.contains(after[i].id.as_str())).collect();
    before
        .iter()
        .map(|b| {
            if let Some(i) = after.iter().position(|a| a.id == b.id) {
                return Some(i);
            }
            let at = tests_before.get(&b.id).and_then(|l| l.first());
            let k = open.iter().position(|&i| same_test(at, tests_after.get(&after[i].id).and_then(|l| l.first())))?;
            Some(open.remove(k))
        })
        .collect()
}

fn claim_changes(b: Option<&Node>, a: Option<&Node>, before: &ScryModel, after: &ScryModel) -> Vec<ClaimChange> {
    let (bc, ac) = (b.map_or(&[][..], |n| &n.claims[..]), a.map_or(&[][..], |n| &n.claims[..]));
    let pairs = pair(bc, ac, &before.test_map, &after.test_map);
    let mut out = Vec::new();
    let mut paired = vec![false; ac.len()];
    let mut removed = Vec::new();
    for (c, p) in bc.iter().zip(&pairs) {
        match p {
            Some(i) => {
                paired[*i] = true;
                if ac[*i].statement != c.statement || ac[*i].concerns != c.concerns {
                    out.push((*i, ClaimChange {
                        id: ac[*i].id.clone(),
                        statement: ac[*i].statement.clone(),
                        was: (ac[*i].statement != c.statement).then(|| c.statement.clone()),
                        change: Change::Modified,
                    }));
                }
            }
            None => removed.push(ClaimChange { id: c.id.clone(), statement: c.statement.clone(), was: None, change: Change::Removed }),
        }
    }
    for (i, c) in ac.iter().enumerate().filter(|(i, _)| !paired[*i]) {
        out.push((i, ClaimChange { id: c.id.clone(), statement: c.statement.clone(), was: None, change: Change::Added }));
    }
    out.sort_by_key(|(i, _)| *i);
    out.into_iter().map(|(_, c)| c).chain(removed).collect()
}

fn kind_word(n: &Node) -> String {
    serde_json::to_value(n.kind).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default()
}

fn path(by_id: &HashMap<&str, &Node>, n: &Node) -> Vec<String> {
    let mut out = Vec::new();
    let mut at = n.parent_id.as_deref();
    while let Some(id) = at {
        let Some(p) = by_id.get(id) else { break };
        out.push(p.name.clone());
        at = p.parent_id.as_deref();
    }
    out.reverse();
    out
}

fn uses(model: &ScryModel, id: &str) -> Vec<String> {
    let mut out: Vec<String> = model.links.iter().filter(|l| l.src == id).map(|l| l.dst.clone()).collect();
    out.sort();
    out.dedup();
    out
}

/// What `after` changed against `before`.
pub fn model_diff(before: &ScryModel, after: &ScryModel) -> ModelDiff {
    let old: HashMap<&str, &Node> = before.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    let new: HashMap<&str, &Node> = after.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    let name_in = |m: &HashMap<&str, &Node>, id: &str| m.get(id).map_or_else(|| id.to_string(), |n| n.name.clone());
    let mut nodes = Vec::new();
    for a in &after.nodes {
        let b = old.get(a.id.as_str()).copied();
        let claims = claim_changes(b, Some(a), before, after);
        let mut notes = Vec::new();
        if let Some(b) = b {
            if b.name != a.name {
                notes.push(format!("renamed from {}", b.name));
            }
            if b.parent_id != a.parent_id {
                notes.push(format!("moved from {}", b.parent_id.as_deref().map_or_else(|| "the top".into(), |p| name_in(&old, p))));
            }
            if b.layer != a.layer {
                notes.push(match &a.layer {
                    Some(l) => format!("now in the {l} layer"),
                    None => "no longer in a layer".into(),
                });
            }
            if b.description != a.description {
                notes.push("description changed".into());
            }
            if b.properties != a.properties {
                notes.push("properties changed".into());
            }
            let (was, now) = (uses(before, &b.id), uses(after, &a.id));
            for d in now.iter().filter(|d| !was.contains(d)) {
                notes.push(format!("now uses {}", name_in(&new, d)));
            }
            for d in was.iter().filter(|d| !now.contains(d)) {
                notes.push(format!("no longer uses {}", name_in(&old, d)));
            }
        }
        let change = match b {
            None => Change::Added,
            Some(_) if !claims.is_empty() || !notes.is_empty() => Change::Modified,
            Some(_) => continue,
        };
        nodes.push(NodeChange { id: a.id.clone(), name: a.name.clone(), kind: kind_word(a), path: path(&new, a), change, claims, notes });
    }
    for b in before.nodes.iter().filter(|b| !new.contains_key(b.id.as_str())) {
        nodes.push(NodeChange {
            id: b.id.clone(),
            name: b.name.clone(),
            kind: kind_word(b),
            path: path(&old, b),
            change: Change::Removed,
            claims: claim_changes(Some(b), None, before, after),
            notes: Vec::new(),
        });
    }
    ModelDiff { nodes }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(v: serde_json::Value) -> ScryModel {
        serde_json::from_value(v).unwrap()
    }

    fn base() -> serde_json::Value {
        serde_json::json!({
            "version": "1",
            "nodes": [
                { "id": "app", "kind": "container", "name": "App" },
                { "id": "notes", "kind": "component", "name": "notes", "parentId": "app", "claims": [
                    { "id": "c1", "statement": "Saves a note" },
                    { "id": "c2", "statement": "Deletes a note" }
                ] },
                { "id": "tags", "kind": "component", "name": "tags", "parentId": "app" },
                { "id": "old", "kind": "component", "name": "old", "parentId": "app", "claims": [{ "id": "c9", "statement": "Prints" }] }
            ],
            "links": [],
            "testMap": {
                "c1": [{ "pattern": "notes.test.ts", "symbol": "Saves a note", "line": 3 }],
                "c2": [{ "pattern": "notes.test.ts", "symbol": "Deletes a note", "line": 7 }]
            }
        })
    }

    /// Lists each node added, modified or removed with its path, and under it each claim added, reworded with its old wording, or removed.
    #[test]
    fn lists_changed_nodes_with_their_claims() {
        let before = model(base());
        let mut v = base();
        let nodes = v["nodes"].as_array_mut().unwrap();
        nodes.retain(|n| n["id"] != "old");
        nodes[1]["claims"] = serde_json::json!([
            { "id": "c1b", "statement": "Saves a draft" },
            { "id": "c3", "statement": "Shares a note" }
        ]);
        nodes.push(serde_json::json!({ "id": "search", "kind": "component", "name": "search", "parentId": "app" }));
        v["testMap"] = serde_json::json!({
            "c1b": [{ "pattern": "notes.test.ts", "symbol": "Saves a draft", "line": 3 }],
            "c3": [{ "pattern": "notes.test.ts", "symbol": "Shares a note", "line": 12 }]
        });
        let diff = model_diff(&before, &model(v));
        let read: Vec<(String, Change, Vec<String>)> = diff
            .nodes
            .iter()
            .map(|n| {
                let claims = n
                    .claims
                    .iter()
                    .map(|c| format!("{:?} {}{}", c.change, c.statement, c.was.as_deref().map_or(String::new(), |w| format!(" (was {w})"))))
                    .collect();
                (n.name.clone(), n.change, claims)
            })
            .collect();
        assert_eq!(
            read,
            vec![
                ("notes".into(), Change::Modified, vec![
                    "Modified Saves a draft (was Saves a note)".into(),
                    "Added Shares a note".into(),
                    "Removed Deletes a note".into(),
                ]),
                ("search".into(), Change::Added, vec![]),
                ("old".into(), Change::Removed, vec!["Removed Prints".into()]),
            ]
        );
        assert_eq!(diff.nodes[1].path, ["App"]);
    }

    /// Says in words what else changed on a node: renamed, moved, re-layered, redescribed, or a dependency gained or lost; an unchanged model reads no changes.
    #[test]
    fn names_a_nodes_other_changes() {
        let before = model(base());
        assert!(model_diff(&before, &before).nodes.is_empty());
        let mut v = base();
        v["nodes"][2]["name"] = "labels".into();
        v["nodes"][2]["layer"] = "domain".into();
        v["nodes"][2]["description"] = "Labels notes".into();
        v["links"] = serde_json::json!([{ "id": "l", "src": "tags", "dst": "notes", "label": "reads" }]);
        let diff = model_diff(&before, &model(v));
        assert_eq!(diff.nodes.len(), 1);
        assert_eq!(diff.nodes[0].notes, ["renamed from tags", "now in the domain layer", "description changed", "now uses notes"]);
    }
}
