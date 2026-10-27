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
//!
//! Layered as a `library`, each module naming its layer at the end of its
//! doc's first paragraph: the domain is the pure core (parsing, the
//! dependency context, claims lifted from tests, the derived tree,
//! `structure.toml`, test evidence) with no I/O; the infrastructure reads and
//! writes; the composition wires the two into the entry points other
//! containers call.

pub mod claim_files;
pub mod derived_tree;
pub mod extract;
pub mod tree_from_disk;
pub mod verdict_cache;
pub mod code_map;
pub use code_map::{lang, context, anchors};
pub mod claim_text;
pub use claim_text::{claims, claim_edit};
pub mod derivation;
pub use derivation::{derive, structure};
pub mod test_evidence;
pub use test_evidence::{evidence, mutations};
pub mod discovery;
pub use discovery::{walk, manifest, tsconfig};
pub mod claim_writing;
pub use claim_writing::{claim_file_io, migrate};
pub mod standing;
pub use standing::{landing, test_status, worktrees};

// The public surface, spelled the way consumers use it today.
pub use extract::{
    extract_context, extract_context_with_stats, list_project_files, ExtractionStats,
};
pub use context::{
    build_context, compact_scope, compact_scope_with_evidence, slice_container, slice_scope,
    ContainerFacts, Edge, FileContext, ProjectContext, PromptScopeContext, ScopeContext,
    SymbolContext,
};
pub use context::{Container, TsAliases};
