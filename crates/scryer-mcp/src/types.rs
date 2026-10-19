//! Accepts a responsibility as a plain statement string or a rich form carrying a concern slug and source line range
//!
//! Declares the typed, field-level request contract for every MCP tool call
//!
//! Defines the shared C4 authoring vocabulary — plan layer, statement/concern, link kind — reused across the add_*, fill_container, and drift payloads

use serde::Deserialize;

/// Which layer of the derived tree a read returns.
#[derive(Debug, Clone, Copy, Default, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Layer {
    /// Every claim, todo stubs included (default).
    #[default]
    Plan,
    /// Only claims whose tests are written.
    Committed,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct ReadModelRequest {
    pub project: Option<String>,
    /// Node id to read as a full subtree; omit for the overview down to components.
    pub node: Option<String>,
    /// "plan" (default, todo stubs included) or "committed".
    #[serde(default)]
    pub layer: Layer,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct LocateRequest {
    pub project: Option<String>,
    /// Project-relative file to look up.
    pub file: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct SearchModelRequest {
    pub project: Option<String>,
    /// Text to find; space-separated terms must ALL match on the node (name, description, technology, claims, labels).
    pub query: String,
    /// Optional kind filter: "person", "system", "container", "component", or "symbol".
    pub kind: Option<String>,
    /// "plan" (default, todo stubs included) or "committed".
    #[serde(default)]
    pub layer: Layer,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct OrientRequest {
    pub project: Option<String>,
    /// The task in a few words; give this, `files`, or both.
    pub task: Option<String>,
    /// Project-relative files the task touches.
    pub files: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct IngestReportRequest {
    pub project: Option<String>,
    /// The report file, absolute or project-relative.
    pub path: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct GetTestRadiusRequest {
    pub project: Option<String>,
    /// Project-relative files changed; omit for this session's edits.
    #[serde(default)]
    pub files: Option<Vec<String>>,
    /// Set the test command once: {names} {files} {report}.
    #[serde(default)]
    pub command: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct ProbeRequest {
    pub project: Option<String>,
    /// The test's JUnit name, as locate and orient list it.
    pub test: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct CloseProbeRequest {
    pub project: Option<String>,
    /// The test the probe was opened on.
    pub test: String,
    /// How many deliberate breaks you tried in total, survivors included.
    pub probes: u32,
    /// One entry per break the test did NOT catch, saying what you changed; empty means every break was caught.
    #[serde(default)]
    pub survivors: Vec<String>,
}
