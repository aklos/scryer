//! Comment threads for a project: read across its live worktrees, written
//! into the checkout the caller works in.

use crate::git_worktrees::list_worktrees;
use crate::comment_threads::{fold, order, Author, CommentEvent, Thread, ThreadStatus};
use crate::stores as comment_store;
use std::path::{Path, PathBuf};

pub use crate::comment_threads::{Author as CommentAuthor, Thread as CommentThread, ThreadStatus as CommentStatus};

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
    let mut event = CommentEvent {
        id: String::new(),
        at,
        author,
        text: text.to_string(),
        node: Some(node.to_string()),
        anchor: Some(anchor.to_string()),
        status: None,
        retracts: None,
    };
    event.id = comment_store::write(project, &id, &event)?;
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
    let event = CommentEvent {
        id: String::new(),
        at: now_ms(),
        author,
        text: text.to_string(),
        node: None,
        anchor: None,
        status,
        retracts: None,
    };
    comment_store::write(project, id, &event)?;
    thread(project, id).ok_or_else(|| format!("no comment thread '{id}'"))
}

/// Delete an event of a thread; only its author may. Deleting the opening
/// event deletes the whole thread. The event's file leaves this checkout,
/// and a retraction is written beside it so copies in other worktrees and
/// on other branches stop showing too.
pub fn delete(project: &Path, id: &str, event: &str, author: Author) -> Result<(), String> {
    let Some(t) = thread(project, id) else {
        return Err(format!("no comment thread '{id}'"));
    };
    let Some(target) = t.events.iter().find(|e| e.id == event) else {
        return Err(format!("no comment '{event}' in thread '{id}'"));
    };
    if target.author != author {
        return Err("only its author can delete a comment".into());
    }
    let retraction = CommentEvent {
        id: String::new(),
        at: now_ms(),
        author,
        text: String::new(),
        node: None,
        anchor: None,
        status: None,
        retracts: Some(event.to_string()),
    };
    comment_store::write(project, id, &retraction)?;
    comment_store::remove_event(project, id, event)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// When its author deletes a reply, the thread no longer shows it.
    #[test]
    fn deleting_a_reply_drops_it() {
        let dir = tempfile::tempdir().unwrap();
        let t = open_thread(dir.path(), "node-a", "sys / a", Author::Human, "why?").unwrap();
        let t = reply(dir.path(), &t.id, Author::Human, "never mind", None).unwrap();
        delete(dir.path(), &t.id, &t.events[1].id, Author::Human).unwrap();
        let texts: Vec<_> = thread(dir.path(), &t.id).unwrap().events.into_iter().map(|e| e.text).collect();
        assert_eq!(texts, ["why?"]);
    }

    /// When its author deletes a thread's opening comment, the thread is gone.
    #[test]
    fn deleting_the_opening_drops_the_thread() {
        let dir = tempfile::tempdir().unwrap();
        let t = open_thread(dir.path(), "node-a", "sys / a", Author::Human, "what?").unwrap();
        delete(dir.path(), &t.id, &t.events[0].id, Author::Human).unwrap();
        assert!(threads(dir.path()).is_empty());
    }

    /// If someone other than its author deletes a comment, then it is refused and the comment stays.
    #[test]
    fn only_the_author_deletes() {
        let dir = tempfile::tempdir().unwrap();
        let t = open_thread(dir.path(), "node-a", "sys / a", Author::Human, "why?").unwrap();
        assert!(delete(dir.path(), &t.id, &t.events[0].id, Author::Agent).is_err());
        assert_eq!(threads(dir.path()).len(), 1);
    }
}
