//! The derived tree, read-only. `scryer_extract::derived_tree` derives it and
//! writes it here; readers that cannot depend on the extractor read the last
//! derivation back.

use crate::derived::{compute_derived_status, DerivedStatus};
use crate::derived::DerivedTree;
use crate::evidence::MutationCache;
use crate::{derived_tree_cache, verdict_store};
use crate::c4_model::{ModelRef, ScryModel};
use std::path::Path;

/// The last derived tree for this project, when one was written.
pub fn read_derived_tree_at(r: &ModelRef) -> Option<DerivedTree> {
    derived_tree_cache::read_derived_file(&r.derived_tree_path())
}

/// The derived tree's planned layer, shaped like `read_planned_at` returns
/// the authored one, so the app and MCP read it unchanged.
pub fn read_derived_planned_at(r: &ModelRef) -> Result<ScryModel, String> {
    read_derived_tree_at(r)
        .map(|t| t.planned)
        .ok_or_else(|| format!("no derived tree for '{}' yet", r.display_name()))
}

/// The derived tree's committed layer, shaped like `read_model_at`.
pub fn read_derived_committed_at(r: &ModelRef) -> Result<ScryModel, String> {
    read_derived_tree_at(r)
        .map(|t| t.committed)
        .ok_or_else(|| format!("no derived tree for '{}' yet", r.display_name()))
}

/// Store a derived tree for later reads.
pub fn write_derived_tree_at(r: &ModelRef, tree: &DerivedTree) -> Result<(), String> {
    derived_tree_cache::write_derived_file(&r.derived_tree_path(), tree)
}

/// The derived tree with every claim's status and the health counts, from
/// what is on disk: the last derived tree, the last ingested verdicts, probes
/// and mutation report. Freshness is judged by file mtimes alone, so a file
/// touched without changing reads stale here until
/// `scryer_extract::test_status::derived_status` re-hashes it.
pub fn derived_status(project: &Path) -> Result<(DerivedTree, DerivedStatus), String> {
    let r = ModelRef::ProjectLocal(project.to_path_buf());
    let tree = read_derived_tree_at(&r)
        .ok_or_else(|| format!("no derived tree for '{}' yet", r.display_name()))?;
    let verdicts = verdict_store::read_verdicts(&r.test_results_path());
    let mutations = read_mutations_at(&r);
    let mut fresh = |id: &str, fp: &std::collections::BTreeMap<String, String>, at: u64| {
        verdict_store::anchors_untouched_since(&tree.planned, project, id, fp, at)
    };
    let status = compute_derived_status(&tree, &verdicts, &mutations, &mut fresh);
    Ok((tree, status))
}

/// Whether a record's anchors are provably untouched since it was taken,
/// by file mtimes alone (see `derived_status`).
pub fn anchors_untouched_since(
    model: &ScryModel,
    project: &Path,
    resp_id: &str,
    fingerprints: &std::collections::BTreeMap<String, String>,
    recorded_at: u64,
) -> bool {
    verdict_store::anchors_untouched_since(model, project, resp_id, fingerprints, recorded_at)
}

/// What changed since `since`, per agent session, against this tree and
/// status.
pub fn landing_at(
    r: &ModelRef,
    tree: &DerivedTree,
    status: &DerivedStatus,
    since: u64,
) -> crate::derived::Landing {
    let sessions = sessions_since(r, since);
    let mut out = crate::derived::landing(&sessions, tree, status, since);
    mark_live(&live_session_ids(r), &mut out);
    out
}

/// The sessions working in checkout `r` now: mid-turn, not merely open.
pub fn live_session_ids(r: &ModelRef) -> std::collections::BTreeSet<String> {
    crate::session_hooks::working_sessions(r)
}

/// Mark the sessions of `landing` that are in `live`.
pub fn mark_live(live: &std::collections::BTreeSet<String>, landing: &mut crate::derived::Landing) {
    for s in &mut landing.sessions {
        s.live = live.contains(&s.session);
    }
}

/// This checkout's session logs written to since `since`, with their entries.
pub fn sessions_since(r: &ModelRef, since: u64) -> Vec<(String, Vec<crate::session_log::SessionEntry>)> {
    crate::stores::list(r)
        .into_iter()
        .filter(|(_, updated)| *updated >= since)
        .map(|(id, _)| {
            let entries = crate::stores::read(r, &id);
            (id, entries)
        })
        .collect()
}

/// The project's exclusive lock, held until dropped.
pub struct ProjectLock {
    _held: verdict_store::ProjectLock,
}

/// Take the project's exclusive lock for a read-modify-write of its caches.
pub fn lock_project(r: &ModelRef) -> Result<ProjectLock, String> {
    verdict_store::lock_project_dir(&r.dir()).map(|held| ProjectLock { _held: held })
}

/// The last ingested mutation results.
pub fn read_mutations_at(r: &ModelRef) -> MutationCache {
    verdict_store::read_mutations(&r.mutations_path())
}

/// Replace the stored mutation results.
pub fn write_mutations_at(r: &ModelRef, cache: &MutationCache) -> Result<(), String> {
    verdict_store::write_mutations(&r.mutations_path(), cache)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// When a derived tree has been written, reads return each layer as stored.
    #[test]
    fn reads_back_the_written_layers() {
        let dir = tempfile::tempdir().unwrap();
        let r = ModelRef::ProjectLocal(dir.path().to_path_buf());
        assert!(read_derived_tree_at(&r).is_none());
        assert!(read_derived_planned_at(&r).is_err());

        let mut planned = ScryModel::new();
        planned.nodes.push(crate::c4_model::Node {
            id: "node-a".into(),
            kind: crate::c4_model::Kind::System,
            name: "sys".into(),
            parent_id: None,
            external: None,
            technology: None,
            description: None,
            vagrant: None,
            stale: None,
            responsibilities: Vec::new(),
            properties: Vec::new(),
            icon: None,
            notes: None,
            position: None,
            directives: Vec::new(),
            style: None,
            layer: None,
            exercised_by: Vec::new(),
            tooling: None,
            decisions: None,
        });
        let tree = DerivedTree {
            fingerprint: 7,
            committed: ScryModel::new(),
            planned,
            claims: Vec::new(),
            doc_files: Default::default(),
            file_nodes: Default::default(),
            dark_files: Vec::new(),
            style_violations: Vec::new(),
            layer_exempt: Vec::new(),
            tooling: Default::default(),
        };
        write_derived_tree_at(&r, &tree).unwrap();
        assert_eq!(read_derived_tree_at(&r).unwrap().fingerprint, 7);
        assert_eq!(read_derived_planned_at(&r).unwrap().nodes.len(), 1);
        assert!(read_derived_committed_at(&r).unwrap().nodes.is_empty());
    }

    /// When a claim's test file is older than its green verdict and its
    /// anchors match the record, the claim reads proven; once the file is
    /// modified after the verdict it reads stale.
    #[test]
    fn status_from_disk_ages_with_file_mtimes() {
        use crate::derived::ClaimState;
        use crate::derived::{ClaimLevel, DerivedClaim};
        use crate::evidence::{ClaimRecord, TestStatusCache};
        use crate::c4_model::SourceLocation;

        let dir = tempfile::tempdir().unwrap();
        let r = ModelRef::ProjectLocal(dir.path().to_path_buf());
        std::fs::write(dir.path().join("t.rs"), "#[test] fn ok() {}").unwrap();
        let mut planned = ScryModel::new();
        planned.test_map.insert(
            "resp-1".into(),
            vec![SourceLocation { pattern: "t.rs".into(), symbol: Some("ok".into()), line: Some(1), end_line: Some(1) }],
        );
        let tree = DerivedTree {
            fingerprint: 1,
            committed: planned.clone(),
            planned,
            claims: vec![DerivedClaim {
                id: "resp-1".into(),
                node_id: "n".into(),
                level: ClaimLevel::Component,
                file: "t.rs".into(),
                line: 1,
                identifier: "ok".into(),
                description: "ok".into(),
                concerns: Vec::new(),
                junit_name: "ok".into(),
                planned: false,
            }],
            doc_files: Default::default(),
            file_nodes: Default::default(),
            dark_files: Vec::new(),
            style_violations: Vec::new(),
            layer_exempt: Vec::new(),
            tooling: Default::default(),
        };
        write_derived_tree_at(&r, &tree).unwrap();
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
        let cache = TestStatusCache {
            results: vec![ClaimRecord {
                resp_id: "resp-1".into(),
                outcome: crate::evidence::TestOutcome::Passed,
                cases: 1,
                recorded_at: now + 2,
                fingerprints: [("test:resp-1|t.rs|ok".to_string(), "h".to_string())].into(),
                millis: Default::default(),
                proven_as: None,
            }],
            ..Default::default()
        };
        std::fs::write(r.test_results_path(), serde_json::to_string(&cache).unwrap()).unwrap();
        let (_, status) = derived_status(dir.path()).unwrap();
        assert_eq!(status.claims[0].state, ClaimState::Proven);

        let later = std::time::SystemTime::now() + std::time::Duration::from_secs(10);
        let file = std::fs::File::options().write(true).open(dir.path().join("t.rs")).unwrap();
        file.set_modified(later).unwrap();
        let (_, status) = derived_status(dir.path()).unwrap();
        assert_eq!(status.claims[0].state, ClaimState::Stale);
    }
}
