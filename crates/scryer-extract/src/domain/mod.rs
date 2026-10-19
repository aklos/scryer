//! The pure core: parsing, the dependency context, claims lifted from tests,
//! the derived C4 tree, `structure.toml`, and test evidence. No I/O — the
//! infrastructure reads and writes, the composition wires the two.

pub mod anchors;
pub mod claim_edit;
pub mod claims;
pub mod context;
pub mod derive;
pub mod facts;
pub mod lang;
pub mod mutations;
pub mod structure;
pub mod test_status;
