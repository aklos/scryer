//! The model's durable timeline.
//!
//! Appends one event to the append-only JSONL log, best-effort so a log failure never aborts the model operation.
//!
//! Reads the whole event log in file order, skipping malformed lines so a torn write never breaks the timeline.

use crate::domain::history_events::HistoryEvent;
use crate::infrastructure::history;
use crate::ModelRef;

/// Append one event to the project's history log.
pub fn append_event(r: &ModelRef, ev: &HistoryEvent) -> Result<(), String> {
    history::append_event_file(r, ev)
}

/// Read the project's history, oldest first.
pub fn read_history(r: &ModelRef) -> Vec<HistoryEvent> {
    history::read_history_file(r)
}
