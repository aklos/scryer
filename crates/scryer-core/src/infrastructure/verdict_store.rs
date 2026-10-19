//! The evidence caches read straight off disk: test verdicts, probes and
//! mutation results. The extractor writes them; this side only reads,
//! except for the mutation cache, which is plain data with no fingerprints.

use crate::domain::model::{test_key, ScryModel};
use crate::domain::verdicts::{MutationCache, TestStatusCache};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub fn read_verdicts(path: &Path) -> TestStatusCache {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

pub fn read_mutations(path: &Path) -> MutationCache {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

pub fn write_mutations(path: &Path, cache: &MutationCache) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string(cache).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| e.to_string())
}

/// Freshness without parsing: the claim's anchors spell exactly the keys the
/// record fingerprinted, and none of their files was modified since the
/// record. Anything it cannot prove this way counts as changed.
pub fn anchors_untouched_since(
    model: &ScryModel,
    project: &Path,
    resp_id: &str,
    fingerprints: &BTreeMap<String, String>,
    recorded_at: u64,
) -> bool {
    let recorded = std::time::UNIX_EPOCH + std::time::Duration::from_secs(recorded_at);
    let mut keys = BTreeSet::new();
    let mut files = BTreeSet::new();
    for (key, locs) in [
        (resp_id.to_string(), model.source_map.get(resp_id)),
        (test_key(resp_id), model.test_map.get(resp_id)),
    ] {
        for loc in locs.into_iter().flatten() {
            keys.insert(format!("{key}|{}|{}", loc.pattern, loc.symbol.as_deref().unwrap_or("")));
            files.insert(loc.pattern.as_str());
        }
    }
    if !keys.iter().eq(fingerprints.keys()) {
        return false;
    }
    files.iter().all(|f| {
        std::fs::metadata(project.join(f))
            .and_then(|m| m.modified())
            .is_ok_and(|mtime| mtime < recorded)
    })
}

/// The project's exclusive lock, held across a read-modify-write of the
/// evidence caches so concurrent sessions serialize. Released on drop.
pub struct ProjectLock {
    _file: std::fs::File,
}

/// Acquire the project lock, blocking until it is free.
pub fn lock_project_dir(dir: &Path) -> Result<ProjectLock, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let file = std::fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(dir.join(".lock"))
        .map_err(|e| format!("Failed to open the project lock: {e}"))?;
    file.lock().map_err(|e| format!("Failed to acquire the project lock: {e}"))?;
    Ok(ProjectLock { _file: file })
}
