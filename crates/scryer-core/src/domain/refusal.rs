//! Why the fold declined a claim — the record itself. The ledger file it is
//! stored in belongs to `infrastructure::refusals`.

use serde::{Deserialize, Serialize};

/// One claim the fold declined to commit, and why.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Refusal {
    pub resp_id: String,
    /// The node or group the claim sits on.
    pub host_id: String,
    /// Short kind tag: `no-test`, `no-verdict`, `stale`, `failing`,
    /// `amendment`, `addition`.
    pub kind: String,
    /// The missing fact in the fold's own words ("no test attached",
    /// "verdict stale: run tests/foo.test.ts and ingest").
    pub reason: String,
    /// Test files whose run would clear the refusal, when any.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub run: Vec<String>,
    /// Unix seconds of the refusal.
    pub at: u64,
}
