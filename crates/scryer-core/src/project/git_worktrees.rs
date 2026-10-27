//! The project's git worktrees: the probe tree a falsification probe mutates,
//! and the live checkouts agents work in. The git plumbing is the
//! `probe_tree` adapter's; this is the surface other containers
//! call.

use crate::probe_tree as adapter;
use std::path::{Path, PathBuf};

/// One of the project's git worktrees.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitWorktree {
    pub path: PathBuf,
    /// `None` for a detached HEAD.
    pub branch: Option<String>,
}

/// The project's live git worktrees, its main checkout first, leaving out
/// scryer's own probe trees. Empty when the project is not a git repository.
pub fn list_worktrees(project: &Path) -> Vec<GitWorktree> {
    adapter::list_worktrees(project).into_iter().map(|(path, branch)| GitWorktree { path, branch }).collect()
}

/// What `checkout` changed against the `base` branch, committed since their
/// merge base or not, and its commits ahead of and behind it; without a
/// base, only what is uncommitted.
pub fn changes_against(checkout: &Path, base: Option<&str>) -> Result<BranchChanges, String> {
    adapter::changes_against(checkout, base).map(|(files, ahead, behind)| BranchChanges { files, ahead, behind })
}

/// A checkout's changes against its base branch.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BranchChanges {
    /// Changed and untracked files, relative to the checkout.
    pub files: Vec<String>,
    pub ahead: u32,
    pub behind: u32,
}

/// Where the project's probe tree lives, whether or not it exists yet.
pub fn worktree_path(project: &Path) -> PathBuf {
    adapter::worktree_path(project)
}

/// The probe tree synced to the project's current state, uncommitted edits
/// included, created on first use. Refuses a project that is not a git
/// repository.
pub fn ensure_synced(project: &Path) -> Result<PathBuf, String> {
    adapter::ensure_synced(project)
}

/// Whether the probe tree differs from the project in any of `files`.
pub fn holds_change(project: &Path, files: &[String]) -> bool {
    adapter::holds_change(project, files)
}

/// Put the probe tree back to the project's state, dropping the probe's break.
pub fn reset(project: &Path) -> Result<(), String> {
    adapter::reset(project)
}
