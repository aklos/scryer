//! Where the project stands, for other containers: each claim's test evidence,
//! what each agent session is doing and did, and each worktree against its
//! base branch. [composition]

pub mod landing;
pub mod test_status;
pub mod worktrees;

pub use landing::*;
pub use test_status::*;
pub use worktrees::*;
