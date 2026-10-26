//! Comment threads for a project: read across its live worktrees, written
//! into the checkout the caller works in.

use crate::composition::worktree::list_worktrees;
use crate::domain::comments::{fold, order, Author, CommentEvent, Thread, ThreadStatus};
use crate::infrastructure::comment_store;
use std::path::{Path, PathBuf};

pub use crate::domain::comments::{Author as CommentAuthor, Thread as CommentThread, ThreadStatus as CommentStatus};

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// `project` first, then its other live worktrees.
fn roots(project: &Path) -> Vec<PathBuf> {
    let mut roots = vec![project.to_path_buf()];
    for w in list_worktrees(project) {
        if !roots.iter().any(|r| same(r, &w.path)) {
            roots.push(w.path);
        }
    }
    roots
}

fn same(a: &Path, b: &Path) -> bool {
    a.canonicalize().ok().zip(b.canonicalize().ok()).is_some_and(|(a, b)| a == b) || a == b
}

/// Every thread in the project, open ones first.
pub fn threads(project: &Path) -> Vec<Thread> {
    let mut all: Vec<Thread> = comment_store::read_all(&roots(project))
        .into_iter()
        .filter_map(|(id, events)| fold(&id, events))
        .collect();
    order(&mut all);
    all
}

/// One thread by id.
pub fn thread(project: &Path, id: &str) -> Option<Thread> {
    threads(project).into_iter().find(|t| t.id == id)
}

/// Open a thread on `node`. `anchor` says in words where the node is.
pub fn open_thread(project: &Path, node: &str, anchor: &str, author: Author, text: &str) -> Result<Thread, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("a comment needs text".into());
    }
    let at = now_ms();
    let id = format!("t{at:x}{}", comment_store::nonce());
    let event = CommentEvent {
        at,
        author,
        text: text.to_string(),
        node: Some(node.to_string()),
        anchor: Some(anchor.to_string()),
        status: None,
    };
    comment_store::write(project, &id, &event)?;
    fold(&id, vec![event]).ok_or_else(|| "the thread did not open".into())
}

/// Reply to a thread, optionally changing its status. A status change
/// needs words: what was done, or why it is reopened.
pub fn reply(
    project: &Path,
    id: &str,
    author: Author,
    text: &str,
    status: Option<ThreadStatus>,
) -> Result<Thread, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("a reply needs text — say what was done or what is missing".into());
    }
    if thread(project, id).is_none() {
        return Err(format!("no comment thread '{id}'"));
    }
    let event = CommentEvent { at: now_ms(), author, text: text.to_string(), node: None, anchor: None, status };
    comment_store::write(project, id, &event)?;
    thread(project, id).ok_or_else(|| format!("no comment thread '{id}'"))
}
