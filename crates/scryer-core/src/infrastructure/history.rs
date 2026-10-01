//! Durable committed-model history — an append-only event log living alongside
//! the model at `.scryer/history.jsonl`.
//!
//! The committed `model.scry` only ever changes through an agent operation — a
//! fold (`mark_implemented`), a drift reconcile, a build, a structural move. Each
//! such operation appends one [`HistoryEvent`] here, so the node page's History
//! tab can show a real timeline ("implemented · 2 days ago") rather than a
//! session-only journal. The log is git-tracked like the model itself: it is not
//! regenerable, so it is the source of truth for what happened when.
//!
//! Append-only JSONL (one event per line) keeps writes cheap and crash-safe — a
//! torn final line drops exactly one event instead of corrupting the whole log.

use crate::ModelRef;
use std::fs;
use std::io::Write;

pub use crate::domain::history_events::*;

/// Append one event to the JSONL log, creating `.scryer/` and the file as needed.
/// Best-effort: a failure here must never abort the model operation that produced
/// the event, so callers ignore the result.
pub fn append_event_file(r: &ModelRef, ev: &HistoryEvent) -> Result<(), String> {
    let dir = r.dir();
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let line = serde_json::to_string(ev).map_err(|e| e.to_string())?;
    let mut f = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(r.history_path())
        .map_err(|e| e.to_string())?;
    writeln!(f, "{}", line).map_err(|e| e.to_string())
}

/// Read the whole log in file order (oldest first). Skips blank or malformed
/// lines so a torn write can never break the timeline; returns empty when absent.
pub fn read_history_file(r: &ModelRef) -> Vec<HistoryEvent> {
    let raw = match fs::read_to_string(r.history_path()) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    raw.lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::SourceLocation;
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn append_then_read_round_trips_in_order() {
        let tmp = tempdir().unwrap();
        let r = ModelRef::ProjectLocal(tmp.path().to_path_buf());

        // Empty log reads as no events.
        assert!(read_history_file(&r).is_empty());

        let born = HistoryEvent::new(100, EventKind::Born, "n1", "build")
            .with_rows(vec![EventRow::new("+", "3 responsibilities · component")]);
        let impld = HistoryEvent::new(200, EventKind::Impl, "n1", "fill").with_rows(vec![
            EventRow::new("+", "Charges the card via Stripe.").with_source(SourceLocation {
                pattern: "api/payment/handler.rs".into(),
                symbol: Some("charge".into()),
                line: Some(40),
                end_line: Some(78),
            }),
        ]);
        append_event_file(&r, &born).unwrap();
        append_event_file(&r, &impld).unwrap();

        let log = read_history_file(&r);
        assert_eq!(log.len(), 2);
        assert_eq!(log[0].kind, EventKind::Born);
        assert_eq!(log[1].kind, EventKind::Impl);
        assert_eq!(log[1].rows[0].source.as_ref().unwrap().line, Some(40));

        // A garbage line is skipped, surrounding events survive.
        fs::write(
            r.history_path(),
            format!(
                "{}\n%%not json%%\n{}\n",
                serde_json::to_string(&born).unwrap(),
                serde_json::to_string(&impld).unwrap()
            ),
        )
        .unwrap();
        assert_eq!(read_history_file(&r).len(), 2);
    }
}
