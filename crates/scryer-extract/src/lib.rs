//! Deterministic, parser-only codebase extraction, and the C4 tree derived
//! from it.
//!
//! [`extract_context`] precomputes a MAP of a codebase — every container
//! (declared build/deploy unit), every symbol with its exact line range, and a
//! conservative dependency graph — using only manifests and tree-sitter parse
//! trees (no LLM, no curated "known-X" lookup tables); [`slice_scope`] cuts it
//! down to one subtree. On that map the crate derives the C4 tree, lifts each
//! test's description as a claim, writes claims back into the code, and keeps
//! the test evidence that gives each claim its status. The context itself is
//! never persisted to disk.

pub mod composition;
pub mod domain;
pub mod infrastructure;

// The public surface, spelled the way consumers use it today.
pub use composition::extract::{
    extract_context, extract_context_with_stats, list_project_files, ExtractionStats,
};
pub use domain::context::{
    build_context, compact_scope, compact_scope_with_evidence, slice_container, slice_scope,
    ContainerFacts, Edge, FileContext, ProjectContext, PromptScopeContext, ScopeContext,
    SymbolContext,
};
pub use domain::context::{Container, TsAliases};
pub use domain::{context, lang};
pub use composition::{claim_files, derived_tree, landing, test_status};
pub use infrastructure::{manifest, tsconfig};
