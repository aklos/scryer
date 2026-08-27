//! The claim probe ledger — what a falsification probe broke, and how to put
//! it back.
//!
//! A probe deliberately mutates the code behind one claim and re-runs that
//! claim's attached test. If the test stays green, the test does not hold the
//! claim. The mutation lands in the real working tree, so for the length of a
//! probe the tree is wrong on purpose — which is only safe if the way back is
//! written down first. This ledger is that record: the file's content is
//! captured to disk BEFORE any edit reaches it, so an interrupted session is
//! always recoverable, and an entry that outlives its session is reported
//! rather than silently left behind.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::ModelRef;

/// One open probe: the claim under test, the span it targets, and the file
/// content to restore.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProbeEntry {
    pub resp_id: String,
    /// Project-relative file holding the claim's implementation span.
    pub file: String,
    /// 1-based inclusive line span the probe targets. Advisory — it tells the
    /// agent where to aim and the reader what was mutated; restoration never
    /// depends on it.
    pub start_line: u32,
    pub end_line: u32,
    /// The file's FULL content as it stood before the probe. Whole-file, not
    /// span-slice: a mutation is free to add or remove lines, which would
    /// leave any span-relative restore writing back over the wrong region.
    pub original: String,
    pub opened_at: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeLedger {
    #[serde(default)]
    pub open: Vec<ProbeEntry>,
}

fn ledger_path(project: &Path) -> PathBuf {
    project.join(".scryer").join(".probes.json")
}

fn read_at(path: &Path) -> ProbeLedger {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn write_at(project: &Path, ledger: &ProbeLedger) -> Result<(), String> {
    let path = ledger_path(project);
    if ledger.open.is_empty() {
        // An empty ledger is absence, not an empty file: nothing to recover
        // means nothing left on disk to misread later.
        return match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.to_string()),
        };
    }
    let json = serde_json::to_string_pretty(ledger).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(project.join(".scryer")).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())
}

/// Every probe currently open, oldest first.
pub fn open_probes(r: &ModelRef) -> Vec<ProbeEntry> {
    read_at(&ledger_path(r.project_path())).open
}

/// The files an open probe is currently mutating. Path-based so the drift
/// engine — which holds a project path, not a model ref — can consult it
/// without a load.
pub fn probed_files(project: &Path) -> BTreeSet<String> {
    read_at(&ledger_path(project))
        .open
        .into_iter()
        .map(|e| e.file)
        .collect()
}

/// Open a probe on one claim: capture the file as it stands and record the
/// entry. The ledger is written BEFORE the caller is allowed to mutate
/// anything — a capture that isn't on disk yet is not a way back.
///
/// Refuses a second probe on a file that already has one open: two overlapping
/// captures would restore each other's mutations.
pub fn open_probe(r: &ModelRef, entry: ProbeEntry) -> Result<(), String> {
    let project = r.project_path();
    let mut ledger = read_at(&ledger_path(project));
    if let Some(existing) = ledger.open.iter().find(|e| e.file == entry.file) {
        return Err(format!(
            "a probe is already open on {} (claim {}) — close it before opening another",
            existing.file, existing.resp_id
        ));
    }
    ledger.open.retain(|e| e.resp_id != entry.resp_id);
    ledger.open.push(entry);
    write_at(project, &ledger)
}

/// Close a probe: restore the captured content and drop the entry. Called on
/// every outcome — a caught mutant and a survivor are both finished probes,
/// and both leave the tree as they found it.
///
/// Returns the entry that was closed, or `None` when no probe was open for
/// that claim (so a double close is quiet, not an error).
pub fn close_probe(r: &ModelRef, resp_id: &str) -> Result<Option<ProbeEntry>, String> {
    let project = r.project_path();
    let mut ledger = read_at(&ledger_path(project));
    let Some(idx) = ledger.open.iter().position(|e| e.resp_id == resp_id) else {
        return Ok(None);
    };
    let entry = ledger.open.remove(idx);
    std::fs::write(project.join(&entry.file), &entry.original)
        .map_err(|e| format!("restoring {}: {e}", entry.file))?;
    write_at(project, &ledger)?;
    Ok(Some(entry))
}

/// Restore every open probe — the recovery path for a session that died mid
/// probe. Returns the entries it put back.
pub fn close_all_probes(r: &ModelRef) -> Result<Vec<ProbeEntry>, String> {
    let mut restored = Vec::new();
    for entry in open_probes(r) {
        if let Some(closed) = close_probe(r, &entry.resp_id)? {
            restored.push(closed);
        }
    }
    Ok(restored)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORIGINAL: &str = "fn limit(n: u32) -> bool {\n    n > 10\n}\n";

    fn project() -> (tempfile::TempDir, ModelRef) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/lib.rs"), ORIGINAL).unwrap();
        let r = ModelRef::ProjectLocal(dir.path().to_path_buf());
        (dir, r)
    }

    fn entry(resp_id: &str, original: &str) -> ProbeEntry {
        ProbeEntry {
            resp_id: resp_id.into(),
            file: "src/lib.rs".into(),
            start_line: 1,
            end_line: 3,
            original: original.into(),
            opened_at: 0,
        }
    }

    /// resp-744: the capture is on disk before the caller may edit — an open
    /// probe is readable immediately, with the pre-edit content in it.
    #[test]
    fn opening_records_the_capture_before_any_edit() {
        let (dir, r) = project();
        open_probe(&r, entry("r1", ORIGINAL)).unwrap();

        let open = open_probes(&r);
        assert_eq!(open.len(), 1, "the probe is recorded");
        assert_eq!(open[0].original, ORIGINAL, "with the pre-edit content");
        assert!(
            dir.path().join(".scryer/.probes.json").exists(),
            "and it is on disk, not just in memory — a crash must be recoverable"
        );
    }

    /// resp-745: closing restores the captured content and clears the entry,
    /// and does so for a survivor exactly as for a caught mutant — the caller
    /// reports the outcome, the ledger just puts the tree back.
    #[test]
    fn closing_restores_the_capture_whatever_the_outcome() {
        let (dir, r) = project();
        open_probe(&r, entry("r1", ORIGINAL)).unwrap();
        std::fs::write(dir.path().join("src/lib.rs"), "fn limit(n: u32) -> bool {\n    n >= 10\n}\n")
            .unwrap();

        let closed = close_probe(&r, "r1").unwrap();

        assert_eq!(closed.unwrap().resp_id, "r1");
        assert_eq!(
            std::fs::read_to_string(dir.path().join("src/lib.rs")).unwrap(),
            ORIGINAL,
            "the mutation is gone"
        );
        assert!(open_probes(&r).is_empty(), "and the entry with it");
        assert!(
            !dir.path().join(".scryer/.probes.json").exists(),
            "an empty ledger leaves no file to misread"
        );
    }

    /// A mutation that changes the line count still restores exactly — which
    /// is why the capture is whole-file and not a span slice.
    #[test]
    fn restore_survives_a_mutation_that_changes_the_line_count() {
        let (dir, r) = project();
        open_probe(&r, entry("r1", ORIGINAL)).unwrap();
        std::fs::write(dir.path().join("src/lib.rs"), "fn limit(_: u32) -> bool { true }\n").unwrap();

        close_probe(&r, "r1").unwrap();

        assert_eq!(
            std::fs::read_to_string(dir.path().join("src/lib.rs")).unwrap(),
            ORIGINAL
        );
    }

    /// resp-746: a probe left open by a dead session is still there to be
    /// found and named — the tree must never read as clean while mutated.
    #[test]
    fn an_entry_outliving_its_session_stays_findable() {
        let (dir, r) = project();
        open_probe(&r, entry("r1", ORIGINAL)).unwrap();
        std::fs::write(dir.path().join("src/lib.rs"), "broken").unwrap();

        // A new session sees only what is on disk.
        let fresh = ModelRef::ProjectLocal(dir.path().to_path_buf());
        let open = open_probes(&fresh);
        assert_eq!(open.len(), 1);
        assert_eq!(open[0].file, "src/lib.rs", "and names the mutated file");

        let restored = close_all_probes(&fresh).unwrap();
        assert_eq!(restored.len(), 1, "recovery puts it back");
        assert_eq!(
            std::fs::read_to_string(dir.path().join("src/lib.rs")).unwrap(),
            ORIGINAL
        );
    }

    /// Two open probes on one file would restore each other's mutations, so
    /// the second is refused rather than silently corrupting the first.
    #[test]
    fn a_second_probe_on_the_same_file_is_refused() {
        let (_dir, r) = project();
        open_probe(&r, entry("r1", ORIGINAL)).unwrap();

        let err = open_probe(&r, entry("r2", ORIGINAL)).unwrap_err();

        assert!(err.contains("already open"), "{err}");
        assert_eq!(open_probes(&r).len(), 1, "the first probe is untouched");
    }

    /// Closing a claim with no open probe is quiet, not an error: an agent
    /// that closes twice must not be told the tree is in trouble.
    #[test]
    fn closing_an_unopened_probe_answers_none() {
        let (_dir, r) = project();
        assert!(close_probe(&r, "r1").unwrap().is_none());
    }

    /// resp-752's input: the drift engine asks which files are mid-probe.
    #[test]
    fn probed_files_names_the_open_captures() {
        let (dir, r) = project();
        open_probe(&r, entry("r1", ORIGINAL)).unwrap();

        let files = probed_files(dir.path());

        assert!(files.contains("src/lib.rs"));
    }
}
