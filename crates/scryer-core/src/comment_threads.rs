//! Comment threads: how people and agents leave each other notes on a node
//! and close them, the way issues and pull requests are worked on GitHub. [domain]
//!
//! A thread is a run of events, each its own file (see
//! `stores::comment_store`): the first event opens the thread on a
//! node; each later one is a reply, and a reply may change the status. A
//! thread is addressed only when someone says so explicitly — an agent's
//! reply never closes it on its own. Folding the events in time order gives
//! the thread; nothing is ever rewritten, so two branches replying to one
//! thread merge without conflict.

use serde::{Deserialize, Serialize};

/// Whether a thread still asks for something.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ThreadStatus {
    Open,
    Addressed,
}

/// Who wrote an event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Author {
    Human,
    Agent,
}

/// One event of a thread, as stored.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommentEvent {
    /// Unix milliseconds.
    pub at: u64,
    pub author: Author,
    pub text: String,
    /// The node the thread is on — set on the opening event only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<String>,
    /// Where that node was, in words ("scryer / Model Core / domain"), so a
    /// thread whose node has moved still says what it was about.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<String>,
    /// A status change this event makes: `addressed` closes the thread,
    /// `open` reopens it. Absent on a plain reply.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<ThreadStatus>,
}

/// A thread, folded from its events.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Thread {
    pub id: String,
    pub node: String,
    pub anchor: String,
    pub status: ThreadStatus,
    pub events: Vec<CommentEvent>,
}

impl Thread {
    /// The time of the latest event.
    pub fn updated(&self) -> u64 {
        self.events.last().map(|e| e.at).unwrap_or(0)
    }
}

/// Fold a thread's events. The earliest event carrying a node opens it;
/// without one there is no thread. Status follows the last event that
/// changes it.
pub fn fold(id: &str, mut events: Vec<CommentEvent>) -> Option<Thread> {
    events.sort_by_key(|e| e.at);
    let opening = events.iter().find(|e| e.node.is_some())?;
    let node = opening.node.clone()?;
    let anchor = opening.anchor.clone().unwrap_or_default();
    let status = events.iter().filter_map(|e| e.status).last().unwrap_or(ThreadStatus::Open);
    Some(Thread { id: id.to_string(), node, anchor, status, events })
}

/// Open threads first, then the most recently active.
pub fn order(threads: &mut [Thread]) {
    threads.sort_by(|a, b| {
        (a.status == ThreadStatus::Addressed)
            .cmp(&(b.status == ThreadStatus::Addressed))
            .then(b.updated().cmp(&a.updated()))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(at: u64, author: Author, text: &str) -> CommentEvent {
        CommentEvent { at, author, text: text.into(), node: None, anchor: None, status: None }
    }

    fn opening(at: u64) -> CommentEvent {
        CommentEvent { node: Some("node-a".into()), anchor: Some("sys / a".into()), ..ev(at, Author::Human, "why?") }
    }

    /// A thread is open until an event explicitly addresses it; a reply alone changes nothing.
    #[test]
    fn a_thread_is_open_until_an_event_addresses_it() {
        let t = fold("t1", vec![opening(1), ev(2, Author::Agent, "looking")]).unwrap();
        assert_eq!(t.status, ThreadStatus::Open);
        let done = CommentEvent { status: Some(ThreadStatus::Addressed), ..ev(3, Author::Agent, "fixed in a1b2") };
        let t = fold("t1", vec![opening(1), ev(2, Author::Agent, "looking"), done.clone()]).unwrap();
        assert_eq!(t.status, ThreadStatus::Addressed);
        let reopen = CommentEvent { status: Some(ThreadStatus::Open), ..ev(4, Author::Human, "not quite") };
        assert_eq!(fold("t1", vec![opening(1), done, reopen]).unwrap().status, ThreadStatus::Open);
    }

    /// When events arrive out of order (two branches merged), folds them in time order.
    #[test]
    fn events_fold_in_time_order() {
        let t = fold("t1", vec![ev(5, Author::Agent, "second"), opening(1), ev(3, Author::Human, "first")]).unwrap();
        let texts: Vec<_> = t.events.iter().map(|e| e.text.as_str()).collect();
        assert_eq!(texts, ["why?", "first", "second"]);
        assert_eq!((t.node.as_str(), t.anchor.as_str()), ("node-a", "sys / a"));
    }

    /// If no event opens the thread on a node, then there is no thread.
    #[test]
    fn a_thread_without_an_opening_is_dropped() {
        assert!(fold("t1", vec![ev(2, Author::Agent, "orphan reply")]).is_none());
    }

    /// Orders open threads before addressed ones, newest activity first.
    #[test]
    fn open_threads_come_first() {
        let open_old = fold("old", vec![opening(1)]).unwrap();
        let open_new = fold("new", vec![opening(5)]).unwrap();
        let done = CommentEvent { status: Some(ThreadStatus::Addressed), ..ev(9, Author::Agent, "done") };
        let closed = fold("closed", vec![opening(2), done]).unwrap();
        let mut all = vec![closed, open_old, open_new];
        order(&mut all);
        let ids: Vec<_> = all.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, ["new", "old", "closed"]);
    }
}
