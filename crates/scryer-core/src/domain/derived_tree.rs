//! The C4 tree derived from the code instead of authored: containers from
//! packaging, components from modules, claims from test descriptions, placed
//! by test level. `scryer-extract` builds it; this is its shape and what a
//! reader needs to tell it apart from the authored model.

use crate::domain::model::ScryModel;
use serde::{Deserialize, Serialize};

/// Where a claim sits in the tree, decided by where its test lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ClaimLevel {
    /// Acceptance/e2e: nests under the system.
    System,
    /// Integration at a deployable's boundary: nests under its container.
    Container,
    /// Unit: nests under its component.
    Component,
}

/// One behaviour test read as a claim.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DerivedClaim {
    /// The responsibility id the claim carries in the tree's models.
    pub id: String,
    /// The node the claim nests under.
    pub node_id: String,
    pub level: ClaimLevel,
    /// Project-relative test file.
    pub file: String,
    pub line: u32,
    /// The test's code identifier (fn/method name, or the `it` string).
    pub identifier: String,
    /// The claim text, concern tags stripped.
    pub description: String,
    /// Trailing `[tag]`s, in order of appearance.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub concerns: Vec<String>,
    /// The name a JUnit report gives this test.
    pub junit_name: String,
    /// A todo/skip stub: the claim is written but not yet implemented.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub planned: bool,
}

/// The derived tree in the two layers a reader of the authored model expects:
/// `committed` holds every implemented claim, `planned` adds the stubs, so the
/// diff between them is exactly the todo list.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DerivedTree {
    /// Hash of the inputs it was derived from (file mtimes and sizes); equal
    /// fingerprints mean the tree is current.
    pub fingerprint: u64,
    pub committed: ScryModel,
    pub planned: ScryModel,
    pub claims: Vec<DerivedClaim>,
}
