//! Claims as text: behaviour tests read as claims and modules as descriptions,
//! and claims written back into test files. [domain]

pub mod claims;
pub mod claim_edit;
pub(crate) mod head_doc;

pub use claims::*;
pub use claim_edit::*;
