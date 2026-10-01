//! The shape of the model's durable timeline: what kinds of events it records
//! and how one reads back. Appending them to the log is
//! `infrastructure::history`.

use crate::SourceLocation;
use serde::{Deserialize, Serialize};

/// What kind of committed-model event this is — drives the timeline glyph/colour.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum EventKind {
    /// Plan claims folded into the committed model + code (`mark_implemented`).
    Impl,
    /// Drift reconciled — code-side reality folded back (`reconcile_drift`).
    Drift,
    /// Structural move — reparent / repoint (`move_nodes`, `move_responsibilities`).
    Move,
    /// A node first entered the committed model (`fill_container`).
    Born,
    /// A plan change closed — its last pending entry folded (or was reverted).
    /// The one event kind that spans nodes: `node_id` is empty, `change_id`
    /// names the change, and the rows carry its rationale.
    Change,
}

/// One diff row inside an event: a marker glyph, its text, and an optional source
/// anchor (e.g. an `impl` row pointing at the code that now discharges the claim).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventRow {
    /// Single-char marker — `+` added, `−` removed, `!` stale, `→` moved.
    pub marker: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceLocation>,
}

impl EventRow {
    pub fn new(marker: &str, text: impl Into<String>) -> Self {
        Self { marker: marker.to_string(), text: text.into(), source: None }
    }

    pub fn with_source(mut self, source: SourceLocation) -> Self {
        self.source = Some(source);
        self
    }
}

/// One durable event in the committed-model timeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEvent {
    /// Unix seconds.
    pub at: u64,
    /// Who drove it. Agent-only in v0.3 — users edit the plan, not the model.
    pub by: String,
    /// Short driver/intent label shown beside the actor, e.g. "fill", "build",
    /// "took code".
    pub driver: String,
    pub kind: EventKind,
    /// The node this event is about — the per-node History tab filters on it.
    /// Empty for [`EventKind::Change`] events, which span nodes.
    pub node_id: String,
    /// The plan change this event belonged to, when its work was tagged — how
    /// "which change introduced this claim?" gets answered after the fold.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub change_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rows: Vec<EventRow>,
}

impl HistoryEvent {
    pub fn new(at: u64, kind: EventKind, node_id: impl Into<String>, driver: &str) -> Self {
        Self {
            at,
            by: "agent".to_string(),
            driver: driver.to_string(),
            kind,
            node_id: node_id.into(),
            change_id: None,
            rows: Vec::new(),
        }
    }

    pub fn with_rows(mut self, rows: Vec<EventRow>) -> Self {
        self.rows = rows;
        self
    }

    pub fn with_change(mut self, change_id: impl Into<String>) -> Self {
        self.change_id = Some(change_id.into());
        self
    }
}
