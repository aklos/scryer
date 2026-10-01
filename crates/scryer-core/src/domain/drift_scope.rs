//! What a drift check remembers and reports: the sync state it compares
//! against, the per-node anchor snapshot, and the scopes a regression rolls up
//! into. Finding the changed files is `infrastructure::drift`.

use crate::ScryModel;
use std::time::{SystemTime, UNIX_EPOCH};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Persisted reconcile anchor: what the model was last checked against. Stored
/// at `.scryer/.sync`. The build writes it on completion; a drift check rewrites
/// it once it has reconciled, so the next check only looks at newer changes.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncState {
    /// Unix seconds of the last reconcile — the mtime-fallback baseline.
    pub reconciled_at: u64,
    /// Unix NANOSECONDS of the last reconcile. At whole-second granularity an
    /// edit landing in the same second as the reconcile was permanently
    /// invisible; a ns anchor closes that window on filesystems that store ns
    /// mtimes. `None` (old `.sync` files) falls back to the seconds rule.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reconciled_at_ns: Option<u64>,
    /// Git commit the model was last reconciled against, when the project is a
    /// git repo. Precise: ignores touches that didn't change content.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    /// Product-code files present at the reconcile — the deletion tripwire.
    /// The mtime walk only sees files that exist, so a deletion-only change
    /// used to produce zero drift; inventory files that no longer exist now
    /// count as changed. Populated by `write_sync_file` when empty.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub files: BTreeSet<String>,
    /// Per-node reconcile overrides. A node dismissed on its own (with its whole
    /// subtree) gets its own anchor here, so its boundary's changes clear without
    /// moving the project-wide anchor and silencing every other node. Empty in
    /// the common case; a node falls back to the global anchor above.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub nodes: BTreeMap<String, NodeAnchor>,
}

impl SyncState {
    /// A fresh global anchor at this instant — seconds for compatibility, ns
    /// for the same-second gate — against the given commit. The file
    /// inventory is left empty; `write_sync_file` snapshots it.
    pub fn anchored_now(commit: Option<String>) -> Self {
        SyncState {
            reconciled_at: now_secs(),
            reconciled_at_ns: Some(now_ns()),
            commit,
            ..Default::default()
        }
    }
}

/// A single node's reconcile anchor — same shape as the global one, applied only
/// to that node's boundary (see [`SyncState::nodes`]).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeAnchor {
    pub reconciled_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reconciled_at_ns: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    /// Global-inventory files already missing when this node was dismissed —
    /// deletions the dismissal reconciled. Excluded from this node's deletion
    /// set so they stop re-reporting here while other owners still see them.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub missing: BTreeSet<String>,
}

impl NodeAnchor {
    /// A per-node anchor at this instant. `missing` should carry the global
    /// inventory's currently-deleted files (the deletions being dismissed).
    pub fn now(commit: Option<String>, missing: BTreeSet<String>) -> Self {
        NodeAnchor {
            reconciled_at: now_secs(),
            reconciled_at_ns: Some(now_ns()),
            commit,
            missing,
        }
    }
}

/// A boundary-owning node whose code changed since the last reconcile, so it
/// needs a semantic drift re-check. Carries the changed files to focus on.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriftScope {
    pub node_id: String,
    pub node_name: String,
    /// Project-relative files under this node's boundary that changed.
    pub changed_files: Vec<String>,
}

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn now_ns() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

/// `node_id` plus every transitive descendant (via `parent_id`). Used to
/// reconcile a node's whole subtree at once — each descendant can be its own
/// boundary owner, so they must clear together.
pub fn subtree_ids(model: &ScryModel, node_id: &str) -> Vec<String> {
    let mut out = vec![node_id.to_string()];
    let mut seen: std::collections::HashSet<String> =
        std::iter::once(node_id.to_string()).collect();
    let mut i = 0;
    while i < out.len() {
        let cur = out[i].clone();
        for n in &model.nodes {
            // `seen` doubles as a cycle guard: a `parent_id` loop can never
            // re-push a node that's already in the worklist.
            if n.parent_id.as_deref() == Some(cur.as_str()) && seen.insert(n.id.clone()) {
                out.push(n.id.clone());
            }
        }
        i += 1;
    }
    out
}
