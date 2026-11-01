//! Test evidence: runner reports matched back to the claims they test, the
//! last verdict each claim earned, and the lines each claim's test runs. [domain]

pub mod coverage;
pub mod test_results;
pub mod verdicts;

pub use coverage::*;
pub use test_results::*;
pub use verdicts::*;
