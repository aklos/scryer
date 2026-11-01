//! What a claim's test evidence is: verdict and probe records, mutation
//! reports read as probes, and per-test coverage. [domain]

pub mod coverage;
pub mod evidence;
pub mod mutations;

pub use coverage::*;
pub use evidence::*;
pub use mutations::*;
