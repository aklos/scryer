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

/// The commit a checkout's model changes are read against.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeBase {
    pub commit: String,
    /// The branch the checkout diverged from; `None` when the base is HEAD
    /// and the changes are what is uncommitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub against: Option<String>,
    /// Commits ahead of `against`.
    pub ahead: u32,
    /// Something is uncommitted.
    pub dirty: bool,
}

/// Where `checkout`'s changes start: HEAD on the default branch or a
/// detached HEAD, the merge base with the default branch on any other.
pub fn change_base(checkout: &Path) -> Result<ChangeBase, String> {
    adapter::change_base(checkout).map(|(commit, against, ahead, dirty)| ChangeBase { commit, against, ahead, dirty })
}

/// A commit in the project's history.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Commit {
    pub sha: String,
    /// Its first parent; `None` for the first commit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    pub subject: String,
    pub author: String,
    /// Unix seconds.
    pub at: u64,
}

/// HEAD's commits along first parents, newest first, `limit` after `skip`.
pub fn history(project: &Path, skip: usize, limit: usize) -> Result<Vec<Commit>, String> {
    adapter::log(project, skip, limit).map(|cs| {
        cs.into_iter().map(|(sha, parent, subject, author, at)| Commit { sha, parent, subject, author, at }).collect()
    })
}

/// Where the models derived at past commits are cached:
/// `~/.scryer/bases`, or `SCRYER_BASES_DIR` when set, as for tests that
/// must not write into the home directory of whoever runs them.
pub fn bases_root() -> PathBuf {
    match std::env::var_os("SCRYER_BASES_DIR") {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => crate::settings_file::global_dir().join("bases"),
    }
}

/// A stable directory name for `project`'s caches outside it: its name
/// and a hash of its path.
pub fn project_slug(project: &Path) -> String {
    adapter::slug_for(project)
}

/// The git tree `rev` points at.
pub fn tree_of(project: &Path, rev: &str) -> Result<String, String> {
    adapter::tree_of(project, rev)
}

/// Write the files `rev` holds into `dest`, leaving the checkout alone.
pub fn export_revision(project: &Path, rev: &str, dest: &Path) -> Result<(), String> {
    adapter::export_revision(project, rev, dest)
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
