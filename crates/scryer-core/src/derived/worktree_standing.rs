//! Where each of the project's git worktrees stands against its base branch:
//! the files it changed since branching, the claims those changes reach and
//! their status in that checkout, whether it is ready to merge, and the
//! components more than one worktree changed — where merges may collide.
//! Shapes and rules only; the git reads and each checkout's derived status
//! are the caller's.

use crate::derived_status::{ClaimState, DerivedStatus};
use crate::c4_tree::DerivedTree;
use crate::landing::{reaches, LandingClaim};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// A component, by id and by the name its checkout gives it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentRef {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeWork {
    pub path: String,
    /// `None` for a detached HEAD.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// The checkout the app has open.
    pub current: bool,
    /// The branch its changes are read against; `None` for the main
    /// checkout's own branch, whose changes are what is uncommitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    /// Files changed since the merge base, committed or not.
    pub files: Vec<String>,
    /// Components those files belong to.
    pub components: Vec<ComponentRef>,
    /// Claims the changes reach, with their status in this checkout.
    pub claims: Vec<LandingClaim>,
    /// It changed something, and every claim that reaches is proven.
    pub ready: bool,
    /// Why this checkout could not be read, when it could not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// A component changed in more than one worktree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeOverlap {
    pub component: ComponentRef,
    /// The worktrees' paths.
    pub worktrees: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Worktrees {
    pub worktrees: Vec<WorktreeWork>,
    pub overlaps: Vec<WorktreeOverlap>,
}

/// Fill `w` from the files it changed, read against its own tree and status:
/// the components they belong to, the claims they reach, and readiness.
pub fn fill_work(w: &mut WorktreeWork, files: Vec<String>, tree: &DerivedTree, status: &DerivedStatus) {
    let states: HashMap<&str, ClaimState> = status.claims.iter().map(|c| (c.resp_id.as_str(), c.state)).collect();
    let changed: BTreeSet<&str> = files.iter().map(String::as_str).collect();
    let ids: BTreeSet<&str> = changed.iter().filter_map(|f| tree.file_nodes.get(*f)).map(String::as_str).collect();
    w.components = ids
        .into_iter()
        .map(|id| ComponentRef {
            id: id.to_string(),
            name: tree.planned.nodes.iter().find(|n| n.id == id).map_or_else(|| id.to_string(), |n| n.name.clone()),
        })
        .collect();
    w.claims = tree
        .claims
        .iter()
        .filter(|c| reaches(tree, &changed, c))
        .filter_map(|c| Some(LandingClaim::of(c, *states.get(c.id.as_str())?)))
        .collect();
    w.ready = !files.is_empty() && w.claims.iter().all(|c| c.state == ClaimState::Proven);
    w.files = files;
}

/// The components more than one worktree changed, by name.
pub fn overlaps(work: &[WorktreeWork]) -> Vec<WorktreeOverlap> {
    let mut by: BTreeMap<&str, (&ComponentRef, Vec<String>)> = BTreeMap::new();
    for w in work {
        for c in &w.components {
            by.entry(c.id.as_str()).or_insert((c, Vec::new())).1.push(w.path.clone());
        }
    }
    let mut out: Vec<WorktreeOverlap> = by
        .into_values()
        .filter(|(_, ws)| ws.len() > 1)
        .map(|(c, worktrees)| WorktreeOverlap { component: c.clone(), worktrees })
        .collect();
    out.sort_by(|a, b| a.component.name.cmp(&b.component.name));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::derived_status::ClaimStatus;
    use crate::c4_tree::{ClaimLevel, DerivedClaim};
    use crate::c4_model::{ScryModel, SourceLocation};

    fn tree() -> DerivedTree {
        let mut planned = ScryModel::new();
        planned.nodes.push(
            serde_json::from_value(serde_json::json!({"id": "store", "kind": "component", "name": "Store"})).unwrap(),
        );
        planned.source_map.insert(
            "saves".into(),
            vec![SourceLocation { pattern: "src/store.rs".into(), symbol: None, line: None, end_line: None }],
        );
        let claim = |id: &str, file: &str| DerivedClaim {
            id: id.into(),
            node_id: "store".into(),
            level: ClaimLevel::Component,
            file: file.into(),
            line: 1,
            identifier: id.into(),
            description: id.into(),
            concerns: Vec::new(),
            junit_name: id.into(),
            planned: false,
        };
        DerivedTree {
            fingerprint: 0,
            committed: planned.clone(),
            planned,
            claims: vec![claim("saves", "tests/store.rs"), claim("loads", "tests/load.rs")],
            doc_files: Default::default(),
            file_nodes: [("src/store.rs".to_string(), "store".to_string())].into(),
            dark_files: Vec::new(),
            style_violations: Vec::new(),
            layer_exempt: Vec::new(),
            tooling: BTreeMap::new(),
        }
    }

    fn status(states: &[(&str, ClaimState)]) -> DerivedStatus {
        DerivedStatus {
            claims: states
                .iter()
                .map(|(id, state)| ClaimStatus {
                    resp_id: id.to_string(),
                    node_id: "store".into(),
                    state: *state,
                    probe: None,
                    outcome: None,
                    recorded_at: None,
                    proven_as: None,
                })
                .collect(),
            ..DerivedStatus::default()
        }
    }

    /// A worktree's changes reach the claims anchored to the code they
    /// changed, and it is ready to merge only once each of those is proven
    /// and it changed something at all.
    #[test]
    fn a_worktree_is_ready_when_every_claim_its_changes_reach_is_proven() {
        let t = tree();
        let mut w = WorktreeWork::default();
        fill_work(&mut w, vec!["src/store.rs".into()], &t, &status(&[("saves", ClaimState::Stale), ("loads", ClaimState::Proven)]));
        assert_eq!(w.claims.iter().map(|c| c.resp_id.as_str()).collect::<Vec<_>>(), ["saves"]);
        assert_eq!(w.components, [ComponentRef { id: "store".into(), name: "Store".into() }]);
        assert!(!w.ready);
        fill_work(&mut w, vec!["src/store.rs".into()], &t, &status(&[("saves", ClaimState::Proven)]));
        assert!(w.ready);
        fill_work(&mut w, Vec::new(), &t, &status(&[("saves", ClaimState::Proven)]));
        assert!(!w.ready);
    }

    /// A component changed in two worktrees is an overlap naming both; one
    /// changed in only one is not.
    #[test]
    fn a_component_changed_in_two_worktrees_is_an_overlap() {
        let store = ComponentRef { id: "store".into(), name: "Store".into() };
        let ui = ComponentRef { id: "ui".into(), name: "UI".into() };
        let w = |path: &str, cs: Vec<ComponentRef>| WorktreeWork { path: path.into(), components: cs, ..WorktreeWork::default() };
        let o = overlaps(&[w("/a", vec![store.clone(), ui]), w("/b", vec![store.clone()]), w("/c", Vec::new())]);
        assert_eq!(o, [WorktreeOverlap { component: store, worktrees: vec!["/a".into(), "/b".into()] }]);
    }
}
