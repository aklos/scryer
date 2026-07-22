use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use crate::model::{Responsibility, SchemaProperty, ScryModel};
use crate::model_ref::ModelRef;
use crate::SCRY_VERSION;
use crate::{changes, diff, drift};

// --- Storage ---

fn ensure_project_gitignore(scryer_dir: &Path) -> Result<(), String> {
    let gitignore = scryer_dir.join(".gitignore");
    if !gitignore.exists() {
        fs::write(
            &gitignore,
            "*.baseline.scry\n.sync\n.tmp.*\n.lock\n.anchors.json\n.build_edges.json\nhook.json\npreview/\n",
        )
        .map_err(|e| format!("Failed to create .gitignore: {}", e))?;
    }
    Ok(())
}

/// Check the `version` field on a raw model JSON; return Err with a clear
/// message if it isn't `SCRY_VERSION`.
fn check_version(v: &serde_json::Value) -> Result<(), String> {
    let version = v.get("version").and_then(|x| x.as_str()).unwrap_or("");
    if version != SCRY_VERSION {
        return Err(format!(
            "Model file uses schema version '{}', but this version of scryer requires '{}'. Legacy models cannot be loaded.",
            if version.is_empty() { "<missing>" } else { version },
            SCRY_VERSION
        ));
    }
    Ok(())
}

pub fn read_model_raw_at(r: &ModelRef) -> Result<String, String> {
    fs::read_to_string(&r.model_path()).map_err(|e| e.to_string())
}

pub fn read_model_at(r: &ModelRef) -> Result<ScryModel, String> {
    let raw = read_model_raw_at(r)?;
    let v: serde_json::Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    check_version(&v)?;
    serde_json::from_value(v).map_err(|e| e.to_string())
}

/// RAII guard holding the exclusive write lock for a model. The lock is an
/// advisory OS file lock on `.scryer/.lock`; it is released when this guard is
/// dropped (or the process exits, so a crash never strands it).
///
/// Hold it across the WHOLE read-modify-write cycle of a model edit. That
/// serializes concurrent writers — parallel agent sessions (each its own MCP
/// process) and the canvas — so they can't clobber each other, and it makes the
/// `max+1` id minters correct (the read and the write are atomic together, so
/// two writers can never observe the same max).
#[must_use = "the lock is released as soon as the guard is dropped"]
pub struct ModelLock {
    _file: fs::File,
}

/// Acquire the exclusive write lock for a model, blocking until it is available.
/// Creates the `.scryer` directory and lock file if absent.
pub fn lock_model(r: &ModelRef) -> Result<ModelLock, String> {
    let dir = r.dir();
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let file = fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(r.lock_path())
        .map_err(|e| format!("Failed to open model lock: {}", e))?;
    file.lock()
        .map_err(|e| format!("Failed to acquire model lock: {}", e))?;
    Ok(ModelLock { _file: file })
}

/// Write raw JSON to the model path. Uses an atomic temp-file + rename so the
/// frontend file watcher sees a single inotify event.
pub fn write_model_raw_at(r: &ModelRef, data: &str) -> Result<(), String> {
    let dir = r.dir();
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    ensure_project_gitignore(&dir)?;
    let model_path = r.model_path();
    let tmp = dir.join(".tmp.model.scry");
    fs::write(&tmp, data).map_err(|e| e.to_string())?;
    fs::rename(&tmp, &model_path).map_err(|e| e.to_string())
}

pub fn write_model_at(r: &ModelRef, model: &ScryModel) -> Result<(), String> {
    // Date each responsibility/property against the version currently on disk
    // (the prior, read under the same lock the caller holds), then write. This
    // is the agent-side fossilization clock; the canvas stamps its own edits in
    // the frontend mutation helpers before it writes raw JSON.
    let prior = read_model_at(r).ok();
    let mut stamped = model.clone();
    stamp_touches(&mut stamped, prior.as_ref(), drift::now_secs());
    crate::concerns::register_concerns(&mut stamped);
    // Change state lives and dies with the DRAFT (see `crate::changes`); the
    // committed layer never carries it. Strip here rather than trust every
    // caller — a plan-derived model (fold, set_model) rides through this path.
    stamped.changes.clear();
    stamped.change_map.clear();
    let json = serde_json::to_string_pretty(&stamped).map_err(|e| e.to_string())?;
    write_model_raw_at(r, &json)
}

/// Whether two responsibilities differ in any *truth-bearing* field — the spec
/// statement, drift flags, or directives. Excludes `last_touched_at`
/// itself (that's the output) so an unchanged responsibility keeps its date,
/// and `concern` — a tag is presentation metadata, so retagging never resets
/// the fossilization patina.
fn resp_truth_changed(a: &Responsibility, b: &Responsibility) -> bool {
    a.statement != b.statement
        || a.vagrant != b.vagrant
        || a.stale != b.stale
        || a.directives != b.directives
}

/// Whether two properties differ in any truth-bearing field (label /
/// description). Excludes `last_touched_at`.
fn prop_truth_changed(a: &SchemaProperty, b: &SchemaProperty) -> bool {
    a.label != b.label
        || a.description != b.description
        || a.vagrant != b.vagrant
        || a.stale != b.stale
}

/// Stamp `last_touched_at = now` on every responsibility/property whose
/// truth-bearing content is new or changed relative to `prior`; carry the prior
/// date forward when the content is unchanged. With no prior (the very first
/// write of a model file) everything is stamped. Responsibilities are matched
/// per host (node/group) by id — ids are only unique within a host — and
/// properties per node by label. No layout lives on a responsibility/property,
/// so a pure position change can't reach here and won't re-date anything.
fn stamp_touches(model: &mut ScryModel, prior: Option<&ScryModel>, now: u64) {
    let node_resps: HashMap<&str, HashMap<&str, &Responsibility>> = prior
        .map(|p| {
            p.nodes
                .iter()
                .map(|n| {
                    let m: HashMap<&str, &Responsibility> =
                        n.responsibilities.iter().map(|r| (r.id.as_str(), r)).collect();
                    (n.id.as_str(), m)
                })
                .collect()
        })
        .unwrap_or_default();
    let node_props: HashMap<&str, HashMap<&str, &SchemaProperty>> = prior
        .map(|p| {
            p.nodes
                .iter()
                .map(|n| {
                    let m: HashMap<&str, &SchemaProperty> =
                        n.properties.iter().map(|pr| (pr.label.as_str(), pr)).collect();
                    (n.id.as_str(), m)
                })
                .collect()
        })
        .unwrap_or_default();
    let group_resps: HashMap<&str, HashMap<&str, &Responsibility>> = prior
        .map(|p| {
            p.groups
                .iter()
                .map(|g| {
                    let m: HashMap<&str, &Responsibility> =
                        g.responsibilities.iter().map(|r| (r.id.as_str(), r)).collect();
                    (g.id.as_str(), m)
                })
                .collect()
        })
        .unwrap_or_default();

    let date_resp = |r: &mut Responsibility, host: Option<&HashMap<&str, &Responsibility>>| {
        let prev = host.and_then(|m| m.get(r.id.as_str()).copied());
        r.last_touched_at = match prev {
            Some(pv) if !resp_truth_changed(pv, r) => pv.last_touched_at,
            _ => Some(now),
        };
    };

    for n in &mut model.nodes {
        let hr = node_resps.get(n.id.as_str());
        for r in &mut n.responsibilities {
            date_resp(r, hr);
        }
        let hp = node_props.get(n.id.as_str());
        for pr in &mut n.properties {
            let prev = hp.and_then(|m| m.get(pr.label.as_str()).copied());
            pr.last_touched_at = match prev {
                Some(pv) if !prop_truth_changed(pv, pr) => pv.last_touched_at,
                _ => Some(now),
            };
        }
    }
    for g in &mut model.groups {
        let hr = group_resps.get(g.id.as_str());
        for r in &mut g.responsibilities {
            date_resp(r, hr);
        }
    }
}

// --- Baseline snapshots (for MCP diff) ---

pub fn save_baseline_at(r: &ModelRef, model: &ScryModel) -> Result<(), String> {
    let dir = r.dir();
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(model).map_err(|e| e.to_string())?;
    fs::write(&r.baseline_path(), json).map_err(|e| e.to_string())
}

// --- Reconcile (drift) sync anchor ---

/// Read the drift reconcile anchor (`.scryer/.sync`). Returns the default
/// (epoch 0, no commit) when absent or unparseable — which makes a first drift
/// check examine everything.
pub fn read_sync_state(r: &ModelRef) -> drift::SyncState {
    fs::read_to_string(r.sync_path())
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

/// Write the drift reconcile anchor. Best-effort, non-atomic (like the baseline).
///
/// A state with an EMPTY file inventory gets one snapshotted here — "what
/// product files existed at this anchor" is the deletion tripwire, and it must
/// describe the tree at write time. Callers that carry a state read from disk
/// (per-node dismissals editing `nodes` only) keep their inventory, so older
/// deletions stay visible to the nodes that haven't reconciled them.
pub fn write_sync_state(r: &ModelRef, state: &drift::SyncState) -> Result<(), String> {
    let dir = r.dir();
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let mut state = state.clone();
    if state.files.is_empty() {
        state.files = drift::product_file_inventory(r.project_path());
    }
    let json = serde_json::to_string_pretty(&state).map_err(|e| e.to_string())?;
    fs::write(r.sync_path(), json).map_err(|e| e.to_string())
}

/// Read the baseline snapshot. Returns None if absent or version-mismatched.
pub fn read_baseline_at(r: &ModelRef) -> Option<ScryModel> {
    let raw = fs::read_to_string(&r.baseline_path()).ok()?;
    let v: serde_json::Value = serde_json::from_str(&raw).ok()?;
    if check_version(&v).is_err() {
        return None;
    }
    serde_json::from_value(v).ok()
}

// --- Planned (draft) layer + plan diff ---

/// Write raw JSON to the planned path. Atomic temp-file + rename, like the model
/// write, so the frontend watcher sees a single event.
pub fn write_planned_raw_at(r: &ModelRef, data: &str) -> Result<(), String> {
    let dir = r.dir();
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    ensure_project_gitignore(&dir)?;
    let tmp = dir.join(".tmp.planned.scry");
    fs::write(&tmp, data).map_err(|e| e.to_string())?;
    fs::rename(&tmp, &r.planned_path()).map_err(|e| e.to_string())?;
    // Concern-metadata write-through (plan → committed): a retag on an
    // already-built claim never folds (`diff` ignores `concern`), so it syncs
    // here — the choke point every plan write passes (canvas raw saves and
    // `write_planned_at` alike). Callers hold the model lock. Best-effort: a
    // fresh project has no committed model to sync into.
    if let Ok(planned) = serde_json::from_str::<ScryModel>(data) {
        if let Ok(mut committed) = read_model_at(r) {
            if crate::concerns::sync_concern_metadata(&mut committed, &planned) {
                write_model_at(r, &committed)?;
            }
        }
    }
    Ok(())
}

/// The seeded (clean) plan serialization: the committed model with its
/// single-home `source_map`/`test_map`/`boundaries` cleared. A fresh plan
/// adds nothing, so it starts with no anchors of its own — the working view
/// reads committed's directly.
fn seeded_plan_json(r: &ModelRef) -> Result<String, String> {
    let mut model = read_model_at(r)?;
    model.source_map.clear();
    model.test_map.clear();
    model.boundaries.clear();
    serde_json::to_string_pretty(&model).map_err(|e| e.to_string())
}

/// Read the raw planned JSON, byte-for-byte. Falls back to the SEEDED form of
/// the committed model when no planned file exists yet (planned == model,
/// anchors/boundaries cleared): the canvas echoes back what it loads on its
/// first save, and handing it committed's raw bytes would mint a draft shadowing
/// every committed anchor — the single-home violation [`ensure_planned_at`]
/// exists to prevent. The UI overlays committed's anchors for display itself.
pub fn read_planned_raw_at(r: &ModelRef) -> Result<String, String> {
    let path = r.planned_path();
    if !path.exists() {
        return seeded_plan_json(r);
    }
    fs::read_to_string(&path).map_err(|e| e.to_string())
}

/// Read the planned (draft) model. Falls back to the committed model when no
/// planned file exists yet — a fresh project has an empty plan (planned == model),
/// so the plan diff is empty.
pub fn read_planned_at(r: &ModelRef) -> Result<ScryModel, String> {
    let path = r.planned_path();
    if !path.exists() {
        return read_model_at(r);
    }
    let raw = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let v: serde_json::Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    check_version(&v)?;
    serde_json::from_value(v).map_err(|e| e.to_string())
}

/// Write the planned (draft) model, stamping fossilization dates against the
/// prior planned version (mirrors [`write_model_at`]). Hold the model lock across
/// the read-modify-write, exactly as for the committed model.
pub fn write_planned_at(r: &ModelRef, model: &ScryModel) -> Result<(), String> {
    let prior = read_planned_at(r).ok();
    let mut stamped = model.clone();
    stamp_touches(&mut stamped, prior.as_ref(), drift::now_secs());
    crate::concerns::register_concerns(&mut stamped);
    // Ledger GC on the authoring path: an edit can revert a tagged element
    // back to its committed form, killing the pending entry its tag named. A
    // change emptied that way closes as "abandoned" — the fold paths close
    // theirs as "folded" via their own gc call.
    if !(stamped.change_map.is_empty() && stamped.changes.is_empty()) {
        if let Ok(committed) = read_model_at(r) {
            let gc = changes::gc(&committed, &mut stamped);
            for meta in &gc.closed {
                changes::record_closed(r, meta, "abandoned");
            }
        }
    }
    let json = serde_json::to_string_pretty(&stamped).map_err(|e| e.to_string())?;
    write_planned_raw_at(r, &json)
}

/// Seed the planned file from the committed model when absent, so the plan starts
/// empty (planned == model). No-op if planned already exists.
pub fn ensure_planned_at(r: &ModelRef) -> Result<(), String> {
    if r.planned_path().exists() {
        return Ok(());
    }
    // Code-side mapping has a single home: the committed model owns every
    // committed element's anchor, and the plan overlays anchors only for the
    // elements it later adds. A fresh plan adds nothing, so it starts with no
    // anchors of its own — the working view reads committed's directly.
    let json = seeded_plan_json(r)?;
    write_planned_raw_at(r, &json)
}

/// Drop the draft's `source_map`/`boundaries` entries that are value-identical
/// to committed's — shadow copies minted before plan seeding existed (or by
/// `set_model`, which writes both layers whole). Strictly safe: the working
/// view falls back to committed's identical copy, so nothing any reader sees
/// changes. Entries whose values DIVERGE are left alone — those are either
/// genuine plan-added anchors or a real conflict to surface, never silently
/// picked. Returns whether anything was stripped.
fn strip_shadow_entries(committed: &ScryModel, planned: &mut ScryModel) -> bool {
    let before =
        planned.source_map.len() + planned.test_map.len() + planned.boundaries.len();
    planned.source_map.retain(|k, v| committed.source_map.get(k) != Some(v));
    planned.test_map.retain(|k, v| committed.test_map.get(k) != Some(v));
    planned.boundaries.retain(|k, v| committed.boundaries.get(k) != Some(v));
    before != planned.source_map.len() + planned.test_map.len() + planned.boundaries.len()
}

/// One-time migration for drafts minted before plan seeding: strip the shadow
/// copies of committed's anchors/boundaries so a stale shadow can't keep winning
/// the working view over committed's updated entry. Cheap no-op when the draft
/// is clean (or absent); takes the model lock only when a write is needed — do
/// NOT call while holding it (use [`read_planned_seeded_at`], which heals
/// inline, from lock-holding paths).
pub fn heal_shadow_draft(r: &ModelRef) -> Result<bool, String> {
    if !r.planned_path().exists() {
        return Ok(false);
    }
    // Unlocked fast path: almost every draft is already clean.
    let committed = read_model_at(r)?;
    let mut probe = read_planned_at(r)?;
    if !strip_shadow_entries(&committed, &mut probe) {
        return Ok(false);
    }
    let _lock = lock_model(r)?;
    let committed = read_model_at(r)?;
    let mut planned = read_planned_at(r)?;
    if !strip_shadow_entries(&committed, &mut planned) {
        return Ok(false);
    }
    let json = serde_json::to_string_pretty(&planned).map_err(|e| e.to_string())?;
    write_planned_raw_at(r, &json)?;
    Ok(true)
}

/// Seed a clean plan (if none exists) and read it — the correct entry for any
/// write that AUTHORS into the draft. Guarantees the draft owns no shadow copy of
/// committed's anchors: without the seed, [`read_planned_at`] falls back to the
/// committed model, so writing that back mints `planned.scry` carrying every
/// committed `source_map`/`boundaries` entry — the single-home violation
/// [`ensure_planned_at`] exists to prevent. The caller must hold the model lock
/// (this seeds by writing the plan file).
pub fn read_planned_seeded_at(r: &ModelRef) -> Result<ScryModel, String> {
    ensure_planned_at(r)?;
    let mut planned = read_planned_at(r)?;
    // Heal legacy shadow drafts on the way in (see `heal_shadow_draft`; the
    // caller already holds the lock, so strip and persist inline). Every write
    // path passes through here, so old dirty drafts converge without a
    // dedicated migration step.
    let committed = read_model_at(r).unwrap_or_default();
    if strip_shadow_entries(&committed, &mut planned) {
        let json = serde_json::to_string_pretty(&planned).map_err(|e| e.to_string())?;
        write_planned_raw_at(r, &json)?;
    }
    Ok(planned)
}

/// The plan diff: how the draft (`planned`) diverges from the committed `model` —
/// the planning substrate. Empty when there is no pending plan.
pub fn plan_diff_at(r: &ModelRef) -> Result<diff::ModelDiff, String> {
    let model = read_model_at(r)?;
    let planned = read_planned_at(r)?;
    Ok(diff::diff(&model, &planned))
}

/// The working view the agent operates on: the authored PLAN structure (nodes,
/// links, groups, claims) with committed's single-home anchors overlaid, so
/// nothing that lives only in committed — a committed container's boundary glob,
/// a committed claim's source anchor — vanishes from a plan-based read. Plan
/// entries win on conflict (the draft is the newer authoring). This is what a
/// gate or health read should see: `planned` alone omits committed's anchors
/// (single-home), `model` alone omits the agent's unfolded edits.
pub fn working_view(committed: &ScryModel, planned: &ScryModel) -> ScryModel {
    let mut view = planned.clone();
    // Overlay only entries whose owner is still live in the PLAN: a committed
    // anchor or boundary for a plan-deleted element is pending GC (the deletion
    // fold removes it), and carrying it into the view would make the gate warn
    // about — and health count — an element the plan already removed.
    let node_ids: HashSet<&str> = planned.nodes.iter().map(|n| n.id.as_str()).collect();
    let resp_ids: HashSet<&str> = planned
        .nodes
        .iter()
        .flat_map(|n| n.responsibilities.iter())
        .chain(planned.groups.iter().flat_map(|g| g.responsibilities.iter()))
        .map(|r| r.id.as_str())
        .collect();
    // source_map is keyed by responsibility id or by a property-bearing node id
    // (a schema's declaration site) — the same key universe `validate` checks.
    let property_node_ids: HashSet<&str> = planned
        .nodes
        .iter()
        .filter(|n| !n.properties.is_empty())
        .map(|n| n.id.as_str())
        .collect();
    for (id, sources) in &committed.boundaries {
        if node_ids.contains(id.as_str()) {
            view.boundaries.entry(id.clone()).or_insert_with(|| sources.clone());
        }
    }
    for (id, locs) in &committed.source_map {
        if resp_ids.contains(id.as_str()) || property_node_ids.contains(id.as_str()) {
            view.source_map.entry(id.clone()).or_insert_with(|| locs.clone());
        }
    }
    // test_map is keyed by responsibility id only (a test backs a claim,
    // never a declaration site).
    for (id, locs) in &committed.test_map {
        if resp_ids.contains(id.as_str()) {
            view.test_map.entry(id.clone()).or_insert_with(|| locs.clone());
        }
    }
    view
}

/// Delete the project's model and ALL state derived from it, so a model created
/// afterward starts clean. Removing only the committed file left the draft
/// (`planned.scry`) behind — reopening the project resurrected it as a ghost plan
/// — and left the history log and the anchor/build fingerprints to misattribute
/// to the next model. Infra (`.lock`, `.gitignore`) and the regenerated
/// `preview/` scaffolding are intentionally kept.
pub fn delete_model_at(r: &ModelRef) -> Result<(), String> {
    let model_path = r.model_path();
    if model_path.exists() {
        fs::remove_file(&model_path).map_err(|e| e.to_string())?;
    }
    // Best-effort: every other file is derived state a fresh model must not
    // inherit — the draft especially, which would otherwise resurrect on reopen.
    for path in [
        r.planned_path(),
        r.baseline_path(),
        r.history_path(),
        r.sync_path(),
        r.anchors_path(),
        r.build_edges_path(),
    ] {
        if path.exists() {
            let _ = fs::remove_file(&path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{next_node_id, validate, Kind, Node, Source, SourceLocation};

    fn one_resp_model(statement: &str) -> ScryModel {
        let mut m = ScryModel::new();
        m.nodes.push(Node {
            id: "n1".into(),
            kind: Kind::Component,
            name: "C".into(),
            vagrant: None,
            stale: None,
            parent_id: None,
            external: None,
            technology: None,
            description: None,
            responsibilities: vec![Responsibility {
                concern: None,
                id: "r1".into(),
                statement: statement.into(),
                vagrant: None,
                stale: None,
                stale_proposal: None,
                directives: Vec::new(),
                last_touched_at: None,
            }],
            properties: Vec::new(),
            icon: None,
            visual: None,
            appearance: None,
            notes: None,
            position: None,
            directives: Vec::new(),
        });
        m
    }

    /// The fossilization clock: a responsibility is dated when first written and
    /// when its truth changes, but a cosmetic edit carries the date forward.
    #[test]
    fn stamp_touches_dates_only_truth_changes() {
        // First write (no prior): the responsibility gets dated.
        let mut m = one_resp_model("does X");
        stamp_touches(&mut m, None, 100);
        assert_eq!(m.nodes[0].responsibilities[0].last_touched_at, Some(100));

        // Re-write with identical truth but a cosmetic change (icon): the date is
        // carried forward, not bumped — a non-truth edit is not a touch.
        let prior = m.clone();
        let mut moved = m.clone();
        moved.nodes[0].icon = Some("Box".into());
        stamp_touches(&mut moved, Some(&prior), 200);
        assert_eq!(
            moved.nodes[0].responsibilities[0].last_touched_at,
            Some(100),
            "a cosmetic-only change must not re-date the responsibility"
        );

        // Edit the statement: the responsibility is re-dated to now.
        let prior = moved.clone();
        let mut edited = one_resp_model("does Y");
        stamp_touches(&mut edited, Some(&prior), 300);
        assert_eq!(
            edited.nodes[0].responsibilities[0].last_touched_at,
            Some(300),
            "a changed statement re-dates the responsibility"
        );
    }

    fn temp_ref() -> (tempfile::TempDir, ModelRef) {
        let dir = tempfile::tempdir().unwrap();
        let r = ModelRef::ProjectLocal(dir.path().to_path_buf());
        (dir, r)
    }

    /// Deleting a model must clear the draft and every derived fingerprint, not
    /// just the committed file — otherwise `planned.scry` resurrects as a ghost
    /// plan on the next open and the history/anchor logs misattribute to the next
    /// model. Infra (`.lock`) and `.gitignore` are kept.
    #[test]
    fn delete_model_clears_the_draft_and_derived_state() {
        let (_dir, r) = temp_ref();
        let model = ScryModel::new();
        write_model_at(&r, &model).unwrap();
        write_planned_at(&r, &model).unwrap();
        save_baseline_at(&r, &model).unwrap();
        write_sync_state(&r, &drift::SyncState::default()).unwrap();
        fs::write(r.history_path(), "{}\n").unwrap();
        fs::write(r.anchors_path(), "{}").unwrap();
        fs::write(r.build_edges_path(), "{}").unwrap();
        // Keep a guard so `.lock` exists; it must survive the delete.
        let guard = lock_model(&r).unwrap();

        delete_model_at(&r).unwrap();

        assert!(!r.model_path().exists(), "committed model removed");
        assert!(!r.planned_path().exists(), "draft removed — no ghost plan");
        assert!(!r.baseline_path().exists(), "baseline removed");
        assert!(!r.history_path().exists(), "history removed");
        assert!(!r.anchors_path().exists(), "anchor fingerprints removed");
        assert!(!r.build_edges_path().exists(), "build edges removed");
        assert!(!r.sync_path().exists(), "reconcile anchor removed");
        assert!(r.lock_path().exists(), "infra lock file is kept");
        drop(guard);
    }

    /// A tag-only change never folds (`diff` ignores `concern`), so writing the
    /// plan must write the tag through to the committed copy of the claim —
    /// otherwise discarding the draft would silently lose authored metadata.
    #[test]
    fn plan_write_syncs_concern_metadata_to_committed() {
        let (_dir, r) = temp_ref();
        let mut committed = ScryModel::new();
        let mut node = mk_node("n1", "N", None);
        node.responsibilities.push(mk_resp("resp-1", "authenticates requests"));
        committed.nodes.push(node);
        write_model_at(&r, &committed).unwrap();

        // The canvas retags the committed claim in the plan draft.
        let mut planned = committed.clone();
        planned.nodes[0].responsibilities[0].concern = Some("auth".into());
        write_planned_at(&r, &planned).unwrap();

        let synced = read_model_at(&r).unwrap();
        assert_eq!(
            synced.nodes[0].responsibilities[0].concern.as_deref(),
            Some("auth"),
            "the retag wrote through to the committed layer"
        );
        assert!(
            synced.concerns.iter().any(|c| c.slug == "auth"),
            "the registry entry came along"
        );
    }

    /// The working view overlays committed's single-home anchors ONLY for
    /// elements still live in the plan. A plan-deleted node's boundary or a
    /// plan-deleted claim's anchor is pending GC, not a dangling reference —
    /// carrying it into the view made the gate warn "unknown node /
    /// responsibility" on every pending deletion, a warning the agent could not
    /// clear without folding first.
    #[test]
    fn working_view_drops_committed_anchors_of_plan_deleted_elements() {
        let mut committed = ScryModel::new();
        let mut keep = mk_node("keep", "Keep", None);
        keep.responsibilities.push(mk_resp("resp-1", "stays"));
        let mut gone = mk_node("gone", "Gone", None);
        gone.responsibilities.push(mk_resp("resp-2", "goes"));
        committed.nodes.push(keep);
        committed.nodes.push(gone);
        committed
            .boundaries
            .insert("keep".into(), vec![Source { pattern: "keep/**".into(), comment: None }]);
        committed
            .boundaries
            .insert("gone".into(), vec![Source { pattern: "gone/**".into(), comment: None }]);
        committed.source_map.insert(
            "resp-1".into(),
            vec![serde_json::from_value(serde_json::json!({ "pattern": "keep.rs" })).unwrap()],
        );
        committed.source_map.insert(
            "resp-2".into(),
            vec![serde_json::from_value(serde_json::json!({ "pattern": "gone.rs" })).unwrap()],
        );
        committed.test_map.insert(
            "resp-1".into(),
            vec![serde_json::from_value(serde_json::json!({ "pattern": "keep_test.rs" })).unwrap()],
        );
        committed.test_map.insert(
            "resp-2".into(),
            vec![serde_json::from_value(serde_json::json!({ "pattern": "gone_test.rs" })).unwrap()],
        );

        // Plan: `gone` deleted; anchors stay single-homed in committed.
        let mut planned = committed.clone();
        planned.nodes.retain(|n| n.id != "gone");
        planned.source_map.clear();
        planned.test_map.clear();
        planned.boundaries.clear();

        let view = working_view(&committed, &planned);
        assert!(view.boundaries.contains_key("keep"), "live node's boundary overlays");
        assert!(view.source_map.contains_key("resp-1"), "live claim's anchor overlays");
        assert!(view.test_map.contains_key("resp-1"), "live claim's test entry overlays");
        assert!(!view.boundaries.contains_key("gone"), "plan-deleted node's boundary does not");
        assert!(!view.source_map.contains_key("resp-2"), "plan-deleted claim's anchor does not");
        assert!(!view.test_map.contains_key("resp-2"), "plan-deleted claim's test entry does not");
        let warnings = validate::validate(&view);
        assert!(
            warnings.iter().all(|w| !w.contains("unknown")),
            "the gate is quiet on a pending deletion: {warnings:?}"
        );
    }

    /// With no draft on disk, the raw plan read must hand the canvas the SEEDED
    /// form — committed's structure without its single-home anchors/boundaries.
    /// The canvas echoes what it loads back on its first save, so the raw
    /// committed bytes would mint a full shadow draft.
    #[test]
    fn read_planned_raw_fallback_is_seeded() {
        let (_dir, r) = temp_ref();
        let mut m = ScryModel::new();
        let mut a = mk_node("a", "A", None);
        a.responsibilities.push(mk_resp("resp-1", "do the thing"));
        m.nodes.push(a);
        m.boundaries.insert("a".into(), vec![Source { pattern: "a/**".into(), comment: None }]);
        m.source_map.insert(
            "resp-1".into(),
            vec![serde_json::from_value(serde_json::json!({ "pattern": "a.rs" })).unwrap()],
        );
        m.test_map.insert(
            "resp-1".into(),
            vec![serde_json::from_value(serde_json::json!({ "pattern": "a_test.rs" })).unwrap()],
        );
        write_model_at(&r, &m).unwrap();

        let raw = read_planned_raw_at(&r).unwrap();
        let seeded: ScryModel = serde_json::from_str(&raw).unwrap();
        assert_eq!(seeded.nodes.len(), 1, "structure carries over");
        assert!(
            seeded.source_map.is_empty()
                && seeded.test_map.is_empty()
                && seeded.boundaries.is_empty(),
            "no shadow anchors in the fallback"
        );
    }

    /// Healing a legacy (pre-seeding) draft strips only the entries that are
    /// value-identical to committed's: a genuine plan-added anchor and a
    /// diverged value both survive, and a second pass is a no-op.
    #[test]
    fn heal_shadow_draft_strips_only_value_equal_copies() {
        let (_dir, r) = temp_ref();
        let loc = |p: &str| -> Vec<SourceLocation> {
            vec![serde_json::from_value(serde_json::json!({ "pattern": p })).unwrap()]
        };
        let mut m = ScryModel::new();
        m.nodes.push(mk_node("a", "A", None));
        m.boundaries.insert("a".into(), vec![Source { pattern: "a/**".into(), comment: None }]);
        m.source_map.insert("resp-1".into(), loc("same.rs"));
        m.source_map.insert("resp-2".into(), loc("committed.rs"));
        m.test_map.insert("resp-1".into(), loc("same_test.rs"));
        write_model_at(&r, &m).unwrap();

        // A pre-seeding draft: full shadow of committed, plus one diverged entry
        // and one genuinely plan-added anchor.
        let mut planned = m.clone();
        planned.source_map.insert("resp-2".into(), loc("diverged.rs"));
        planned.source_map.insert("resp-3".into(), loc("plan-added.rs"));
        let json = serde_json::to_string_pretty(&planned).unwrap();
        write_planned_raw_at(&r, &json).unwrap();

        assert!(heal_shadow_draft(&r).unwrap(), "a dirty draft reports healed");
        let healed = read_planned_at(&r).unwrap();
        assert!(!healed.source_map.contains_key("resp-1"), "value-equal shadow stripped");
        assert!(!healed.test_map.contains_key("resp-1"), "value-equal test-entry shadow stripped");
        assert!(!healed.boundaries.contains_key("a"), "value-equal boundary stripped");
        assert_eq!(
            healed.source_map.get("resp-2"),
            Some(&loc("diverged.rs")),
            "a diverged value is never silently dropped"
        );
        assert!(healed.source_map.contains_key("resp-3"), "plan-added anchor survives");
        assert!(!heal_shadow_draft(&r).unwrap(), "second pass is a no-op");
    }

    /// A second independent handle must fail to take the lock while a guard is
    /// held, and succeed once it is released.
    #[test]
    fn lock_is_exclusive_across_handles() {
        let (_dir, r) = temp_ref();
        let guard = lock_model(&r).unwrap();
        let other = fs::OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(r.lock_path())
            .unwrap();
        assert!(
            other.try_lock().is_err(),
            "a second handle must not acquire the lock while it is held"
        );
        drop(guard);
        assert!(
            other.try_lock().is_ok(),
            "the lock is free once the guard is dropped"
        );
    }

    /// Concurrent read-modify-write under the lock never loses an update: N
    /// writers each add one node, and all N land with unique ids — exactly the
    /// parallel-agent-writes case that clobbered without the lock (two writers
    /// reading the same model would both mint the same `next_node_id`).
    #[test]
    fn concurrent_writes_do_not_clobber() {
        let (_dir, r) = temp_ref();
        write_model_at(&r, &ScryModel::new()).unwrap();

        const N: usize = 12;
        std::thread::scope(|s| {
            for i in 0..N {
                let r = r.clone();
                s.spawn(move || {
                    let _lock = lock_model(&r).unwrap();
                    let mut m = read_model_at(&r).unwrap();
                    let id = next_node_id(&m);
                    let node = Node {
                        id,
                        kind: Kind::System,
                        name: format!("n{i}"),
                        vagrant: None,
                        stale: None,
                        parent_id: None,
                        external: None,
                        technology: None,
                        description: None,
                        responsibilities: Vec::new(),
                        properties: Vec::new(),
                        icon: None,
                        visual: None,
                        appearance: None,
                        notes: None,
                        position: None,
                        directives: Vec::new(),
                    };
                    m.nodes.push(node);
                    write_model_at(&r, &m).unwrap();
                });
            }
        });

        let m = read_model_at(&r).unwrap();
        assert_eq!(m.nodes.len(), N, "every concurrent write landed (no lost updates)");
        let ids: std::collections::HashSet<&str> =
            m.nodes.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(ids.len(), N, "all minted ids are unique (no collision)");
    }

    /// With no planned file, the plan diff is empty (planned falls back to model).
    /// After seeding and diverging the draft, the diff reports the change.
    #[test]
    fn plan_diff_tracks_draft_divergence() {
        let (_dir, r) = temp_ref();
        let mut m = ScryModel::new();
        m.nodes.push(Node {
            id: "n1".into(),
            kind: Kind::System,
            name: "Auth".into(),
            vagrant: None,
            stale: None,
            parent_id: None,
            external: None,
            technology: None,
            description: None,
            responsibilities: Vec::new(),
            properties: Vec::new(),
            icon: None,
            visual: None,
            appearance: None,
            notes: None,
            position: None,
            directives: Vec::new(),
        });
        write_model_at(&r, &m).unwrap();

        // No planned file yet → empty plan.
        assert!(plan_diff_at(&r).unwrap().is_empty());

        // Seed the draft from the model, then add a node to the draft.
        ensure_planned_at(&r).unwrap();
        let mut planned = read_planned_at(&r).unwrap();
        planned.nodes.push(Node {
            id: "n2".into(),
            kind: Kind::System,
            name: "Billing".into(),
            vagrant: None,
            stale: None,
            parent_id: None,
            external: None,
            technology: None,
            description: None,
            responsibilities: Vec::new(),
            properties: Vec::new(),
            icon: None,
            visual: None,
            appearance: None,
            notes: None,
            position: None,
            directives: Vec::new(),
        });
        write_planned_at(&r, &planned).unwrap();

        let d = plan_diff_at(&r).unwrap();
        assert_eq!(d.changes.len(), 1);
        assert_eq!(d.changes[0].id, "n2");
        assert_eq!(d.changes[0].changes, vec![diff::Change::Added]);
    }

    fn mk_node(id: &str, name: &str, parent: Option<&str>) -> Node {
        Node {
            id: id.into(),
            kind: Kind::Component,
            name: name.into(),
            vagrant: None,
            stale: None,
            parent_id: parent.map(|s| s.into()),
            external: None,
            technology: None,
            description: None,
            responsibilities: Vec::new(),
            properties: Vec::new(),
            icon: None,
            visual: None,
            appearance: None,
            notes: None,
            position: None,
            directives: Vec::new(),
        }
    }

    fn mk_resp(id: &str, statement: &str) -> Responsibility {
        Responsibility {
            concern: None,
            id: id.into(),
            statement: statement.into(),
            vagrant: None,
            stale: None,
            stale_proposal: None,
            directives: Vec::new(),
            last_touched_at: None,
        }
    }
}
