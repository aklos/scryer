//! The codebase as a map: tree-sitter parsing per language, the per-scope
//! dependency context, and the anchors that pin a span of code. [domain]

pub mod lang;
pub mod context;
pub mod anchors;

pub use lang::*;
pub use context::*;
pub use anchors::*;
