//! The model's durable timeline.

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
