//! Claims written to disk: rewordings and new todo tests in their test files,
//! the one-shot migration from an authored model, and a template scaffolded
//! into a project. [infrastructure]

pub mod claim_file_io;
pub mod migrate;
pub mod scaffold;

pub use claim_file_io::*;
pub use migrate::*;
pub use scaffold::*;
