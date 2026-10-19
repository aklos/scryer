//! Shapes the session hooks and the session view share.

use serde::Serialize;

/// One anchor the fingerprint check reports out of sync. The check itself
/// lives outside this crate.
#[derive(Debug, Clone)]
pub struct AnchorFlag {
    pub key: String,
    pub host_name: String,
    pub file: String,
    pub symbol: Option<String>,
    /// `changed`, `broken` or `fileMissing`.
    pub state: String,
}

/// A plan entry the session planned and has not folded, with the agent's
/// progress note on it when it left one.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Unfolded {
    /// Change-map key (`resp:…`, `node:…`).
    pub key: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}
