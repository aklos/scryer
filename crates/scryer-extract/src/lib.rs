//! Deterministic, parser-only codebase CONTEXT extraction.
//!
//! scryer's modeling speed lever: instead of making the AI agent discover a
//! codebase from scratch, [`extract_context`] precomputes a MAP of it — every
//! container (declared build/deploy unit), every symbol with its exact line
//! range, and a conservative dependency graph — using only manifests and
//! tree-sitter parse trees (no LLM, no curated "known-X" lookup tables). The
//! orchestrator slices this per scope ([`slice_scope`]) and hands each modeling
//! subagent exactly the facts it needs, so the agent skips discovery and goes
//! straight to the semantic work: choosing components and writing
//! responsibilities. The context is a map the agent reads FROM — it is never a
//! C4 model, and it is never persisted to disk.

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
pub use domain::facts::{Container, TsAliases};
pub use domain::{context, lang};
pub use composition::{anchors, test_status};
pub use infrastructure::{manifest, tsconfig};
