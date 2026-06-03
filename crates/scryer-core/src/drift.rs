//! Drift detection — SCOPING only.
//!
//! This module answers one cheap question: *which parts of the codebase have
//! changed since the model was last reconciled?* That is all it does. A changed
//! file never means "the model drifted" — it only means "re-examine this scope."
//! The actual drift verdict is semantic and belongs to an agent pass: does the
//! code do something the responsibilities don't describe? A refactor that
//! preserves behaviour produces changed files here and zero drift there, by
//! design — the user never sees "the bytes changed."
//!
//! So: [`drifted_scopes`] returns the boundary-owning nodes whose code changed,
//! to focus the semantic re-check; the flagging itself (vagrant responsibilities
//! for undescribed behaviour, `changed` status for claims the code no longer
//! discharges) is written by the agent through the `flag_drift` MCP tool.

use crate::{scan, ScryModel};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// Persisted reconcile anchor: what the model was last checked against. Stored
/// at `.scryer/.sync`. The build writes it on completion; a drift check rewrites
/// it once it has reconciled, so the next check only looks at newer changes.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncState {
    /// Unix seconds of the last reconcile — the mtime-fallback baseline.
    pub reconciled_at: u64,
    /// Git commit the model was last reconciled against, when the project is a
    /// git repo. Precise: ignores touches that didn't change content.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
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

/// The current git HEAD commit of `project`, if it is a git repo. Used to anchor
/// a precise diff for the next reconcile.
pub fn head_commit(project: &Path) -> Option<String> {
    if !project.join(".git").exists() {
        return None;
    }
    let out = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(project)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

/// Project-relative files that changed since the sync anchor.
///
/// The hard gate is mtime: only files touched *after* the reconcile moment can
/// count. This is what makes "right after a build" report zero — the model is
/// reconciled against the working tree as it stands (uncommitted edits and all),
/// so nothing is newer than the anchor yet. Anchoring on the git commit alone
/// would wrongly re-report every uncommitted file as drift on a dirty tree.
///
/// When a git commit anchor is present we *refine* (never expand) the touched
/// set with a content diff: keep only files whose bytes actually differ from the
/// anchor commit (or are untracked). That drops touch-without-edit and
/// edit-then-revert noise while still respecting the mtime gate.
pub fn changed_files_since(project: &Path, sync: &SyncState) -> BTreeSet<String> {
    let touched = mtime_changed_files(project, sync.reconciled_at);
    if touched.is_empty() {
        return touched;
    }
    if let Some(commit) = &sync.commit {
        if let Some(content_changed) = git_changed_files(project, commit) {
            return touched.intersection(&content_changed).cloned().collect();
        }
    }
    touched
}

/// `git diff --name-only <commit>` (tracked changes since the anchor, incl.
/// working tree) ∪ untracked files. `None` if not a repo or git fails.
fn git_changed_files(project: &Path, commit: &str) -> Option<BTreeSet<String>> {
    if !project.join(".git").exists() {
        return None;
    }
    let mut out = BTreeSet::new();

    let diff = std::process::Command::new("git")
        .args(["diff", "--name-only", commit, "--"])
        .current_dir(project)
        .output()
        .ok()?;
    if !diff.status.success() {
        return None;
    }
    for line in String::from_utf8_lossy(&diff.stdout).lines() {
        let l = line.trim();
        if !l.is_empty() {
            out.insert(l.replace('\\', "/"));
        }
    }

    // Untracked files (created since the anchor and not yet committed).
    if let Ok(untracked) = std::process::Command::new("git")
        .args(["ls-files", "--others", "--exclude-standard"])
        .current_dir(project)
        .output()
    {
        if untracked.status.success() {
            for line in String::from_utf8_lossy(&untracked.stdout).lines() {
                let l = line.trim();
                if !l.is_empty() {
                    out.insert(l.replace('\\', "/"));
                }
            }
        }
    }

    Some(out)
}

/// Files whose mtime is newer than `baseline_secs`. Honors the same directory
/// skipping as the rest of scryer (SKIP_DIRS / SKIP_BUILD_DIRS, .gitignore).
fn mtime_changed_files(project: &Path, baseline_secs: u64) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let walker = ignore::WalkBuilder::new(project)
        .hidden(false)
        .filter_entry(|entry| {
            if entry.file_type().is_some_and(|ft| ft.is_dir()) {
                let name = entry.file_name().to_string_lossy();
                if scan::SKIP_DIRS.iter().any(|&s| name == s)
                    || scan::SKIP_BUILD_DIRS.iter().any(|&s| name == s)
                {
                    return false;
                }
            }
            true
        })
        .build();

    for entry in walker.flatten() {
        if !entry.file_type().is_some_and(|ft| ft.is_file()) {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        let Ok(mtime) = meta.modified() else { continue };
        let secs = mtime
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        if secs > baseline_secs {
            if let Ok(rel) = entry.path().strip_prefix(project) {
                out.insert(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    out
}

/// The boundary-owning nodes whose code changed since the sync anchor. A node
/// drifts (needs re-check) when a changed file matches one of its boundary
/// globs. Returned in stable node-id order so a run is reproducible.
pub fn drifted_scopes(model: &ScryModel, project: &Path, sync: &SyncState) -> Vec<DriftScope> {
    let changed = changed_files_since(project, sync);
    if changed.is_empty() {
        return Vec::new();
    }

    let mut scopes: Vec<DriftScope> = Vec::new();
    for node in &model.nodes {
        let Some(sources) = model.boundaries.get(&node.id) else {
            continue;
        };
        let patterns: Vec<glob::Pattern> = sources
            .iter()
            .filter_map(|s| glob::Pattern::new(&s.pattern).ok())
            .collect();
        if patterns.is_empty() {
            continue;
        }
        let hits: Vec<String> = changed
            .iter()
            .filter(|f| patterns.iter().any(|p| p.matches(f)))
            .cloned()
            .collect();
        if !hits.is_empty() {
            scopes.push(DriftScope {
                node_id: node.id.clone(),
                node_name: node.name.clone(),
                changed_files: hits,
            });
        }
    }
    scopes.sort_by(|a, b| a.node_id.cmp(&b.node_id));
    scopes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Kind, Node, Source};

    fn node(id: &str, name: &str, kind: Kind) -> Node {
        Node {
            id: id.into(),
            kind,
            name: name.into(),
            parent_id: None,
            external: None,
            technology: None,
            description: None,
            responsibilities: Vec::new(),
            properties: Vec::new(),
            cell: None,
            icon: None,
            deprecated: None,
            relocated: None,
            locked: None,
            relocated_to: None,
            relocated_from: None,
        }
    }

    #[test]
    fn mtime_scoping_maps_changed_files_to_boundary_owners() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("api/src")).unwrap();
        std::fs::create_dir_all(root.join("web/src")).unwrap();
        std::fs::write(root.join("api/src/server.rs"), "fn old() {}").unwrap();
        std::fs::write(root.join("web/src/app.ts"), "const x = 1;").unwrap();

        let mut model = ScryModel::new();
        model.nodes.push(node("node-1", "API", Kind::Container));
        model.nodes.push(node("node-2", "Web", Kind::Container));
        model.boundaries.insert(
            "node-1".into(),
            vec![Source { pattern: "api/**/*".into(), comment: None }],
        );
        model.boundaries.insert(
            "node-2".into(),
            vec![Source { pattern: "web/**/*".into(), comment: None }],
        );

        // Reconcile anchor in the past; only the API file is touched afterwards.
        let sync = SyncState { reconciled_at: now_secs(), commit: None };
        std::thread::sleep(std::time::Duration::from_millis(1100));
        std::fs::write(root.join("api/src/server.rs"), "fn changed() {}").unwrap();

        let scopes = drifted_scopes(&model, root, &sync);
        assert_eq!(scopes.len(), 1, "only the API scope changed");
        assert_eq!(scopes[0].node_id, "node-1");
        assert!(scopes[0]
            .changed_files
            .iter()
            .any(|f| f == "api/src/server.rs"));
    }

    #[test]
    fn dirty_working_tree_is_not_drift_right_after_reconcile() {
        // Regression for the "Check drift (N)" nudge firing right after a build:
        // anchoring on HEAD alone re-reported every uncommitted edit as drift on
        // a dirty tree. The mtime gate fixes it — reconciling against the working
        // tree as it stands reports zero until a file is touched *again*.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let git = |args: &[&str]| -> bool {
            std::process::Command::new("git")
                .args(["-c", "user.email=t@t", "-c", "user.name=t"])
                .args(args)
                .current_dir(root)
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
        };
        if !git(&["init", "-q"]) {
            eprintln!("git unavailable — skipping");
            return;
        }
        std::fs::create_dir_all(root.join("api/src")).unwrap();
        std::fs::write(root.join("api/src/server.rs"), "fn v1() {}").unwrap();
        assert!(git(&["add", "-A"]) && git(&["commit", "-q", "-m", "c0"]));
        let commit = head_commit(root).expect("HEAD after commit");

        let mut model = ScryModel::new();
        model.nodes.push(node("node-1", "API", Kind::Container));
        model.boundaries.insert(
            "node-1".into(),
            vec![Source { pattern: "api/**/*".into(), comment: None }],
        );

        // Dirty the working tree (uncommitted), THEN reconcile against it.
        std::fs::write(root.join("api/src/server.rs"), "fn v2() {}").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let sync = SyncState { reconciled_at: now_secs(), commit: Some(commit) };

        // Working tree differs from HEAD, but nothing changed since reconcile.
        assert!(
            drifted_scopes(&model, root, &sync).is_empty(),
            "edits already incorporated at reconcile must not read as drift"
        );

        // Touch the file AFTER the reconcile → now it surfaces.
        std::thread::sleep(std::time::Duration::from_millis(1100));
        std::fs::write(root.join("api/src/server.rs"), "fn v3() {}").unwrap();
        let scopes = drifted_scopes(&model, root, &sync);
        assert_eq!(scopes.len(), 1, "a post-reconcile edit drifts");
        assert_eq!(scopes[0].node_id, "node-1");
    }

    #[test]
    fn no_changes_no_scopes() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.rs"), "fn a() {}").unwrap();
        let mut model = ScryModel::new();
        model.nodes.push(node("node-1", "Root", Kind::Container));
        model
            .boundaries
            .insert("node-1".into(), vec![Source { pattern: "**/*".into(), comment: None }]);
        // Anchor in the future → nothing is newer.
        let sync = SyncState { reconciled_at: now_secs() + 10, commit: None };
        assert!(drifted_scopes(&model, dir.path(), &sync).is_empty());
    }
}
