//! The fold-refusal ledger — which claims `mark_implemented` last declined to
//! fold, and the exact missing fact each was refused for.
//!
//! A refusal is a deterministic, cheap exit: the claim stays in the plan and
//! the agent moves on. But the developer should SEE it — an unverified claim
//! the agent tried to close is a review item, not a log line — so the fold
//! records each refusal here and the app's inbox reads them. Regenerable and
//! git-free like the other `.scryer/.*.json` caches: a refusal disappears the
//! moment the same claim folds (or leaves the plan), never by hand.

use crate::ModelRef;
use serde::{Deserialize, Serialize};

pub use crate::domain::refusal::*;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Ledger {
    #[serde(default)]
    refusals: Vec<Refusal>,
}

fn read(r: &ModelRef) -> Ledger {
    std::fs::read_to_string(r.fold_refusals_path())
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn write(r: &ModelRef, ledger: &Ledger) -> Result<(), String> {
    std::fs::create_dir_all(r.dir()).map_err(|e| e.to_string())?;
    if ledger.refusals.is_empty() {
        // An empty ledger is no ledger — keep `.scryer/` free of husks.
        match std::fs::remove_file(r.fold_refusals_path()) {
            Ok(()) => return Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e.to_string()),
        }
    }
    let json = serde_json::to_string_pretty(ledger).map_err(|e| e.to_string())?;
    std::fs::write(r.fold_refusals_path(), json).map_err(|e| e.to_string())
}

/// Every standing refusal, oldest first.
pub fn read_refusal_file(r: &ModelRef) -> Vec<Refusal> {
    read(r).refusals
}

/// Record (or replace) the refusal for each claim in `refused`, and CLEAR any
/// standing refusal for the claims in `folded` — a later successful fold is
/// what resolves a refusal. One read-modify-write; best-effort callers may
/// ignore the result.
pub fn update_refusal_file(
    r: &ModelRef,
    refused: &[Refusal],
    folded: &[String],
) -> Result<(), String> {
    let mut ledger = read(r);
    ledger
        .refusals
        .retain(|x| !folded.contains(&x.resp_id) && !refused.iter().any(|n| n.resp_id == x.resp_id));
    ledger.refusals.extend(refused.iter().cloned());
    write(r, &ledger)
}

/// Drop refusals for claims that no longer exist in the plan (the agent or
/// developer removed them) — the inbox must never show a card for a ghost.
pub fn prune_refusal_file(r: &ModelRef, live_resp_ids: &std::collections::HashSet<String>) {
    let mut ledger = read(r);
    let before = ledger.refusals.len();
    ledger.refusals.retain(|x| live_resp_ids.contains(&x.resp_id));
    if ledger.refusals.len() != before {
        let _ = write(r, &ledger);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn refusal(id: &str, kind: &str) -> Refusal {
        Refusal {
            resp_id: id.into(),
            host_id: "n1".into(),
            kind: kind.into(),
            reason: format!("{kind} for {id}"),
            run: vec!["tests/a.test.ts".into()],
            at: 1,
        }
    }

    #[test]
    fn a_refusal_is_recorded_replaced_and_cleared_by_a_fold() {
        let dir = tempdir().unwrap();
        let r = ModelRef::ProjectLocal(dir.path().to_path_buf());
        update_refusal_file(&r, &[refusal("r1", "no-test"), refusal("r2", "stale")], &[]).unwrap();
        assert_eq!(read_refusal_file(&r).len(), 2);

        // Same claim refused again for a new reason: one entry, the latest word.
        update_refusal_file(&r, &[refusal("r1", "no-verdict")], &[]).unwrap();
        let now = read_refusal_file(&r);
        assert_eq!(now.len(), 2);
        assert_eq!(now.iter().find(|x| x.resp_id == "r1").unwrap().kind, "no-verdict");

        // A successful fold clears its refusal; the other stands.
        update_refusal_file(&r, &[], &["r1".into()]).unwrap();
        let now = read_refusal_file(&r);
        assert_eq!(now.len(), 1);
        assert_eq!(now[0].resp_id, "r2");

        // Clearing the last one removes the file rather than leaving a husk.
        update_refusal_file(&r, &[], &["r2".into()]).unwrap();
        assert!(read_refusal_file(&r).is_empty());
        assert!(!r.fold_refusals_path().exists());
    }

    #[test]
    fn pruning_drops_refusals_for_claims_gone_from_the_plan() {
        let dir = tempdir().unwrap();
        let r = ModelRef::ProjectLocal(dir.path().to_path_buf());
        update_refusal_file(&r, &[refusal("r1", "no-test"), refusal("r2", "stale")], &[]).unwrap();
        let live: std::collections::HashSet<String> = ["r2".to_string()].into_iter().collect();
        prune_refusal_file(&r, &live);
        let now = read_refusal_file(&r);
        assert_eq!(now.len(), 1);
        assert_eq!(now[0].resp_id, "r2");
    }
}
