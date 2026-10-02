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
//! So: [`scan_drifted_scopes`] returns the boundary-owning nodes whose code changed,
//! to focus the semantic re-check; the flagging itself (vagrant responsibilities
//! for undescribed behaviour, `changed` status for claims the code no longer
//! discharges) is written by the agent through the `flag_drift` MCP tool.

use crate::infrastructure::scan;
use crate::ScryModel;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;
use std::time::UNIX_EPOCH;

pub use crate::domain::drift_scope::*;

/// The current git HEAD commit of `project`, if it is a git repo. Used to anchor
/// a precise diff for the next reconcile.
pub fn git_head_commit(project: &Path) -> Option<String> {
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
///
/// Deletions ride a different signal entirely: the sync inventory. A file the
/// reconcile saw that no longer exists is a change — the walk can never see it
/// (there is no mtime to read), so it bypasses the gate and the refinement.
pub fn files_changed_since(project: &Path, sync: &SyncState) -> BTreeSet<String> {
    let mut touched = mtime_changed_files(project, sync.reconciled_at, sync.reconciled_at_ns);
    if !touched.is_empty() {
        if let Some(commit) = &sync.commit {
            if let Some(content_changed) = git_changed_files(project, commit) {
                touched = touched.intersection(&content_changed).cloned().collect();
            }
        }
    }
    for file in &sync.files {
        if !project.join(file).exists() {
            touched.insert(file.clone());
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

/// Project files modified after `ns` (unix nanoseconds) — whatever wrote them.
pub fn files_modified_since_ns(project: &Path, ns: u64) -> BTreeSet<String> {
    mtime_changed_files(project, ns / 1_000_000_000, Some(ns))
}

/// Files whose mtime is newer than the anchor. With a ns anchor the comparison
/// is nanosecond-exact (a same-second edit is visible); without one (old
/// `.sync` files) it falls back to whole seconds. Honors the same directory
/// skipping as the rest of scryer (SKIP_DIRS / SKIP_BUILD_DIRS, .gitignore).
fn mtime_changed_files(
    project: &Path,
    baseline_secs: u64,
    baseline_ns: Option<u64>,
) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for entry in project_walker(project).flatten() {
        if !entry.file_type().is_some_and(|ft| ft.is_file()) {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        let Ok(mtime) = meta.modified() else { continue };
        let since_epoch = mtime.duration_since(UNIX_EPOCH).unwrap_or_default();
        let newer = match baseline_ns {
            Some(anchor_ns) => since_epoch.as_nanos() as u64 > anchor_ns,
            None => since_epoch.as_secs() > baseline_secs,
        };
        if newer {
            if let Ok(rel) = entry.path().strip_prefix(project) {
                out.insert(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    out
}

/// The shared project walk: everything except vendor/build dirs.
fn project_walker(project: &Path) -> ignore::Walk {
    ignore::WalkBuilder::new(project)
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
        .build()
}

/// The product-code files present right now — the deletion tripwire's
/// inventory, snapshotted into `.sync` at every reconcile.
pub fn walk_product_files(project: &Path) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for entry in project_walker(project).flatten() {
        if !entry.file_type().is_some_and(|ft| ft.is_file()) {
            continue;
        }
        if let Ok(rel) = entry.path().strip_prefix(project) {
            let rel = rel.to_string_lossy().replace('\\', "/");
            if scan::is_product_code(&rel) {
                out.insert(rel);
            }
        }
    }
    out
}

/// The boundary-owning nodes whose code changed since the sync anchor. A node
/// drifts (needs re-check) when a changed file matches one of its boundary
/// globs. Changed files are gated to product code plus model-anchored files —
/// drift asks whether changed code still satisfies claims, so assets,
/// lockfiles, manifests, and generated churn under a boundary never demand
/// semantic reconciliation (anything the user anchored a claim to stays in,
/// whatever its extension). Returned in stable node-id order so a run is
/// reproducible.
pub fn scan_drifted_scopes(model: &ScryModel, project: &Path, sync: &SyncState) -> Vec<DriftScope> {
    let global_changed = files_changed_since(project, sync);

    // Files the model explicitly anchors (exact paths and glob anchors) are
    // always drift-relevant, even when they aren't parseable source.
    let anchor_paths: std::collections::HashSet<&str> = model
        .source_map
        .values()
        .flatten()
        .map(|l| l.pattern.as_str())
        .collect();
    let anchor_globs: Vec<glob::Pattern> = anchor_paths
        .iter()
        .filter_map(|p| glob::Pattern::new(p).ok())
        .collect();
    let relevant = |f: &str| {
        crate::infrastructure::scan::is_product_code(f)
            || anchor_paths.contains(f)
            || anchor_globs.iter().any(|p| p.matches(f))
    };

    // A node dismissed on its own measures its changes against its own (later)
    // anchor, not the global one. A node and its subtree are dismissed together
    // and share one anchor, so cache the changed-file set per distinct anchor to
    // avoid re-walking the tree per node.
    let mut override_cache: HashMap<(u64, Option<String>), BTreeSet<String>> = HashMap::new();
    // Resolve changed files to their most-specific boundary owner, so a broad
    // glob (a root container's `**/*`) isn't flagged as drifted by changes that
    // a nested container actually owns.
    let ownership = crate::domain::ownership::BoundaryOwnership::new(model);

    let mut scopes: Vec<DriftScope> = Vec::new();
    for node in &model.nodes {
        match model.boundaries.get(&node.id) {
            Some(sources) if !sources.is_empty() => {}
            _ => continue,
        }
        let changed: &BTreeSet<String> = match sync.nodes.get(&node.id) {
            Some(anchor) => override_cache
                // ns key: two dismissals in the same second stay distinct.
                .entry((
                    anchor.reconciled_at_ns.unwrap_or(anchor.reconciled_at),
                    anchor.commit.clone(),
                ))
                .or_insert_with(|| {
                    // The node measures against its own (later) time anchor,
                    // and against the GLOBAL inventory minus the deletions its
                    // dismissal already reconciled (`missing`).
                    files_changed_since(
                        project,
                        &SyncState {
                            reconciled_at: anchor.reconciled_at,
                            reconciled_at_ns: anchor.reconciled_at_ns,
                            commit: anchor.commit.clone(),
                            files: sync.files.difference(&anchor.missing).cloned().collect(),
                            nodes: BTreeMap::new(),
                        },
                    )
                }),
            None => &global_changed,
        };
        let hits: Vec<String> = changed
            .iter()
            .filter(|f| relevant(f) && ownership.owns(&node.id, f))
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
            style: None,
            layer: None,
            id: id.into(),
            kind,
            name: name.into(),
            vagrant: None,
            stale: None,
            parent_id: None,
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
        let sync = SyncState { reconciled_at: now_secs(), commit: None, ..Default::default() };
        std::thread::sleep(std::time::Duration::from_millis(1100));
        std::fs::write(root.join("api/src/server.rs"), "fn changed() {}").unwrap();

        let scopes = scan_drifted_scopes(&model, root, &sync);
        assert_eq!(scopes.len(), 1, "only the API scope changed");
        assert_eq!(scopes[0].node_id, "node-1");
        assert!(scopes[0]
            .changed_files
            .iter()
            .any(|f| f == "api/src/server.rs"));
    }

    #[test]
    fn non_product_changes_never_create_scopes_unless_anchored() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("api")).unwrap();

        let mut model = ScryModel::new();
        model.nodes.push(node("node-1", "API", Kind::Container));
        model.boundaries.insert(
            "node-1".into(),
            vec![Source { pattern: "api/**/*".into(), comment: None }],
        );
        // One claim is anchored to a non-source config file.
        model.source_map.insert(
            "resp-1".into(),
            vec![serde_json::from_value(
                serde_json::json!({ "pattern": "api/settings.yaml" }),
            )
            .unwrap()],
        );

        let sync = SyncState { reconciled_at: now_secs(), commit: None, ..Default::default() };
        std::thread::sleep(std::time::Duration::from_millis(1100));
        // Asset + lockfile churn under the boundary: no drift.
        std::fs::write(root.join("api/icon.png"), [0u8; 4]).unwrap();
        std::fs::write(root.join("api/pnpm-lock.yaml"), "lock").unwrap();
        assert!(
            scan_drifted_scopes(&model, root, &sync).is_empty(),
            "assets and lockfiles must not demand reconciliation"
        );

        // The anchored config file, though non-source, IS drift-relevant.
        std::fs::write(root.join("api/settings.yaml"), "changed: true").unwrap();
        let scopes = scan_drifted_scopes(&model, root, &sync);
        assert_eq!(scopes.len(), 1);
        assert_eq!(scopes[0].changed_files, vec!["api/settings.yaml".to_string()]);

        // Product source under the boundary drifts as before.
        std::fs::write(root.join("api/server.rs"), "fn f() {}").unwrap();
        let scopes = scan_drifted_scopes(&model, root, &sync);
        assert!(scopes[0].changed_files.iter().any(|f| f == "api/server.rs"));
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
        let commit = git_head_commit(root).expect("HEAD after commit");

        let mut model = ScryModel::new();
        model.nodes.push(node("node-1", "API", Kind::Container));
        model.boundaries.insert(
            "node-1".into(),
            vec![Source { pattern: "api/**/*".into(), comment: None }],
        );

        // Dirty the working tree (uncommitted), THEN reconcile against it.
        std::fs::write(root.join("api/src/server.rs"), "fn v2() {}").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let sync = SyncState { reconciled_at: now_secs(), commit: Some(commit), ..Default::default() };

        // Working tree differs from HEAD, but nothing changed since reconcile.
        assert!(
            scan_drifted_scopes(&model, root, &sync).is_empty(),
            "edits already incorporated at reconcile must not read as drift"
        );

        // Touch the file AFTER the reconcile → now it surfaces.
        std::thread::sleep(std::time::Duration::from_millis(1100));
        std::fs::write(root.join("api/src/server.rs"), "fn v3() {}").unwrap();
        let scopes = scan_drifted_scopes(&model, root, &sync);
        assert_eq!(scopes.len(), 1, "a post-reconcile edit drifts");
        assert_eq!(scopes[0].node_id, "node-1");
    }

    #[test]
    fn per_node_override_clears_only_that_node() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("api/src")).unwrap();
        std::fs::create_dir_all(root.join("web/src")).unwrap();
        std::fs::write(root.join("api/src/server.rs"), "fn a() {}").unwrap();
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

        // Global anchor in the past; both boundaries are touched after it.
        let global = now_secs();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        std::fs::write(root.join("api/src/server.rs"), "fn a2() {}").unwrap();
        std::fs::write(root.join("web/src/app.ts"), "const x = 2;").unwrap();

        let mut sync = SyncState { reconciled_at: global, commit: None, ..Default::default() };
        assert_eq!(
            scan_drifted_scopes(&model, root, &sync).len(),
            2,
            "both boundaries drift before any dismiss"
        );

        // Dismiss node-1 only: its own anchor is later than its file edits, so it
        // clears — while node-2 still measures against the old global anchor.
        std::thread::sleep(std::time::Duration::from_millis(1100));
        sync.nodes.insert(
            "node-1".into(),
            NodeAnchor::now(None, std::collections::BTreeSet::new()),
        );
        let scopes = scan_drifted_scopes(&model, root, &sync);
        assert_eq!(scopes.len(), 1, "only the non-dismissed node still drifts");
        assert_eq!(scopes[0].node_id, "node-2");
    }

    /// An edit landing in the same second as the reconcile is visible: the ns
    /// anchor closes the 1s-granularity window. No sleep — that's the point.
    #[test]
    fn same_second_edit_is_visible() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.rs"), "fn a() {}").unwrap();
        let sync = SyncState::anchored_now(None);
        std::thread::sleep(std::time::Duration::from_millis(5));
        std::fs::write(dir.path().join("a.rs"), "fn a2() {}").unwrap();
        let changed = files_changed_since(dir.path(), &sync);
        assert!(changed.contains("a.rs"), "{changed:?}");
    }

    /// Deletion-only changes surface: the sync inventory remembers what
    /// existed, a missing inventory file is a change the mtime walk can never
    /// see, and scan_drifted_scopes maps it to the boundary owner.
    #[test]
    fn deletion_only_change_creates_a_scope() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("api/src")).unwrap();
        std::fs::write(root.join("api/src/server.rs"), "fn a() {}").unwrap();

        let mut model = ScryModel::new();
        model.nodes.push(node("node-1", "API", Kind::Container));
        model.boundaries.insert(
            "node-1".into(),
            vec![Source { pattern: "api/**/*".into(), comment: None }],
        );

        let r = crate::ModelRef::ProjectLocal(root.to_path_buf());
        crate::infrastructure::storage::write_sync_file(&r, &SyncState::anchored_now(None)).unwrap();
        let sync = crate::infrastructure::storage::read_sync_file(&r);
        assert!(
            sync.files.contains("api/src/server.rs"),
            "the reconcile snapshots the product-file inventory"
        );

        std::fs::remove_file(root.join("api/src/server.rs")).unwrap();
        let changed = files_changed_since(root, &sync);
        assert!(changed.contains("api/src/server.rs"), "{changed:?}");
        let scopes = scan_drifted_scopes(&model, root, &sync);
        assert_eq!(scopes.len(), 1, "the deletion reaches its boundary owner");
        assert_eq!(scopes[0].node_id, "node-1");
        assert!(scopes[0].changed_files.iter().any(|f| f == "api/src/server.rs"));
    }

    /// A dismissal reconciles the deletions it saw (`missing`): they stop
    /// counting for that node while other owners keep seeing their own.
    #[test]
    fn dismissed_deletion_clears_for_that_node_only() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("api/src")).unwrap();
        std::fs::create_dir_all(root.join("web/src")).unwrap();
        std::fs::write(root.join("api/src/server.rs"), "fn a() {}").unwrap();
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

        let r = crate::ModelRef::ProjectLocal(root.to_path_buf());
        crate::infrastructure::storage::write_sync_file(&r, &SyncState::anchored_now(None)).unwrap();
        let mut sync = crate::infrastructure::storage::read_sync_file(&r);

        std::fs::remove_file(root.join("api/src/server.rs")).unwrap();
        std::fs::remove_file(root.join("web/src/app.ts")).unwrap();
        assert_eq!(scan_drifted_scopes(&model, root, &sync).len(), 2);

        // Dismiss node-1: its anchor records the deletions it reconciled.
        sync.nodes.insert(
            "node-1".into(),
            NodeAnchor::now(
                None,
                std::iter::once("api/src/server.rs".to_string()).collect(),
            ),
        );
        let scopes = scan_drifted_scopes(&model, root, &sync);
        assert_eq!(scopes.len(), 1, "the dismissed deletion cleared");
        assert_eq!(scopes[0].node_id, "node-2");
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
        let sync = SyncState { reconciled_at: now_secs() + 10, commit: None, ..Default::default() };
        assert!(scan_drifted_scopes(&model, dir.path(), &sync).is_empty());
    }

    /// A malformed `parent_id` loop must not hang the subtree walk. Before the
    /// cycle guard this spun forever, re-pushing the loop members unboundedly.
    #[test]
    fn subtree_ids_terminates_on_a_parent_cycle() {
        let mut model = ScryModel::new();
        let mut a = node("node-1", "A", Kind::Container);
        let mut b = node("node-2", "B", Kind::Container);
        a.parent_id = Some("node-2".into());
        b.parent_id = Some("node-1".into());
        model.nodes.push(a);
        model.nodes.push(b);
        let ids = subtree_ids(&model, "node-1");
        assert!(ids.contains(&"node-1".to_string()));
        assert!(ids.contains(&"node-2".to_string()));
        assert_eq!(ids.len(), 2, "each cycle member visited exactly once");
    }

}
