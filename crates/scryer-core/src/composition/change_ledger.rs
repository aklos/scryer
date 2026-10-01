//! Closing a change: read the plan, take the entry out, record it in history.

use crate::composition::history_log::append_event;
use crate::composition::model_store::{read_planned_at, write_planned_at};
use crate::domain::drift_scope::now_secs;
use crate::domain::history_events::{EventKind, EventRow, HistoryEvent};
use crate::domain::model::ChangeMeta;
use crate::domain::model_ref::ModelRef;

/// Close an EMPTY open change by hand — the escape hatch for a stranded
/// ledger (opened, but its work ended up tagged or folded elsewhere), which
/// [`gc`] deliberately never touches because "no keys yet" is also what a
/// freshly opened change looks like. Refuses a change that still has tagged
/// entries: those close by folding or reverting the entries themselves, never
/// by discarding the grouping. The close is recorded as "abandoned" so the
/// rationale survives. The caller must hold the model lock.
pub fn close_change(r: &ModelRef, change_id: &str) -> Result<ChangeMeta, String> {
    let mut plan = read_planned_at(r)?;
    let Some(pos) = plan.changes.iter().position(|c| c.id == change_id) else {
        return Err(format!("no open change '{change_id}'"));
    };
    let entries = plan.change_map.values().filter(|v| *v == change_id).count();
    if entries > 0 {
        return Err(format!(
            "{change_id} still has {entries} tagged entr{} — fold or revert them; \
             the change closes itself when its last entry goes",
            if entries == 1 { "y" } else { "ies" }
        ));
    }
    let meta = plan.changes.remove(pos);
    write_planned_at(r, &plan)?;
    record_closed(r, &meta, "abandoned");
    Ok(meta)
}

/// Append a closed change's durable record to the history log — the rationale
/// finally survives the fold ("which change introduced this claim?" has an
/// answer). `driver` says how it closed: "folded" (its entries reached
/// committed) or "abandoned" (they were reverted). Best-effort like every
/// history append: a log failure must never abort the model operation.
pub fn record_closed(r: &ModelRef, meta: &ChangeMeta, driver: &str) {
    let ev = HistoryEvent::new(now_secs(), EventKind::Change, "", driver)
        .with_change(&meta.id)
        .with_rows(vec![EventRow::new("✓", meta.rationale.clone())]);
    let _ = append_event(r, &ev);
}
