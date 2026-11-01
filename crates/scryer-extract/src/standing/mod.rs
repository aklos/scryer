//! Where the project stands, for other containers: each claim's test evidence,
//! what each agent session is doing and did, each worktree against its base
//! branch, the model a checkout's changes are read against, and the
//! project's activity: its commits and what each changed in the model.
//! [composition]

pub mod activity;
pub mod base_model;
pub mod landing;
pub mod test_status;
pub mod worktrees;

pub use landing::*;
pub use test_status::*;
pub use worktrees::*;
