//! Writing claims back into the code: reword a test's description, add a
//! claim as a todo stub, rename a container in `structure.toml`.

pub use crate::infrastructure::claim_files::{edit_claim_text, rename_container, stub_target, write_claim_stub};

/// One-shot migration of an authored model into the code.
pub use crate::infrastructure::migrate::{migrate, MigrationReport};
