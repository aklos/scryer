//! Comment threads on disk: `.scryer/comments/<thread>/<event>.json`, one
//! file per event, committed with the code.
//!
//! An event is never edited, only added, so a reply on one branch and a reply
//! on another merge as two new files. Every live worktree has its own copy of
//! the directory; reading takes the union across them by file name, so a
//! reply an agent writes on its branch shows at once, before any merge.

use crate::comment_threads::CommentEvent;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The comments directory of one checkout.
pub fn dir(root: &Path) -> PathBuf {
    root.join(".scryer").join("comments")
}

/// Whether `id` is safe as a directory or file stem: no path tricks.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// Every thread's events across `roots`, keyed by thread id, each event
/// once however many checkouts hold it. A file that does not parse (a torn
/// write) is skipped.
pub fn read_all(roots: &[PathBuf]) -> BTreeMap<String, Vec<CommentEvent>> {
    let mut seen: BTreeMap<String, BTreeMap<String, CommentEvent>> = BTreeMap::new();
    for root in roots {
        let Ok(threads) = std::fs::read_dir(dir(root)) else { continue };
        for thread in threads.flatten() {
            let id = thread.file_name().to_string_lossy().to_string();
            if !valid_id(&id) || !thread.path().is_dir() {
                continue;
            }
            let Ok(events) = std::fs::read_dir(thread.path()) else { continue };
            for event in events.flatten() {
                let name = event.file_name().to_string_lossy().to_string();
                if !name.ends_with(".json") || seen.get(&id).is_some_and(|m| m.contains_key(&name)) {
                    continue;
                }
                let Ok(text) = std::fs::read_to_string(event.path()) else { continue };
                let Ok(mut parsed) = serde_json::from_str::<CommentEvent>(&text) else { continue };
                parsed.id = name.trim_end_matches(".json").to_string();
                seen.entry(id.clone()).or_default().insert(name, parsed);
            }
        }
    }
    seen.into_iter().map(|(id, events)| (id, events.into_values().collect())).collect()
}

/// Write one event of `thread` into `root`'s comments and return its id. The
/// file name is the event's time plus a nonce, so two writers never pick the
/// same one; the id is that name's stem.
pub fn write(root: &Path, thread: &str, event: &CommentEvent) -> Result<String, String> {
    if !valid_id(thread) {
        return Err(format!("invalid thread id '{thread}'"));
    }
    let dir = dir(root).join(thread);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let id = format!("{:013}-{}", event.at, nonce());
    let name = format!("{id}.json");
    let stored = CommentEvent { id: String::new(), ..event.clone() };
    let text = serde_json::to_string_pretty(&stored).map_err(|e| e.to_string())?;
    let tmp = dir.join(format!(".{name}.tmp"));
    std::fs::write(&tmp, text + "\n").map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, dir.join(name)).map_err(|e| e.to_string())?;
    Ok(id)
}

/// Remove one event's file from `root`'s copy of `thread`, if it is there,
/// and the thread's directory once it is empty.
pub fn remove_event(root: &Path, thread: &str, event: &str) -> Result<(), String> {
    if !valid_id(thread) || !valid_id(event) {
        return Err(format!("invalid comment id '{thread}/{event}'"));
    }
    let dir = dir(root).join(thread);
    match std::fs::remove_file(dir.join(format!("{event}.json"))) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.to_string()),
    }
    let _ = std::fs::remove_dir(&dir); // fails while other events remain
    Ok(())
}

/// Six hex digits from the clock and the process id: enough to keep two
/// events of the same millisecond apart.
pub fn nonce() -> String {
    let ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    format!("{:06x}", (ns ^ std::process::id().rotate_left(13)) & 0xff_ffff)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::comment_threads::Author;

    fn ev(at: u64, text: &str) -> CommentEvent {
        CommentEvent { id: String::new(), at, author: Author::Agent, text: text.into(), node: None, anchor: None, status: None, retracts: None }
    }

    /// When two checkouts hold events of one thread, reads each event once, from either.
    #[test]
    fn reads_the_union_of_checkouts() {
        let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        write(a.path(), "t1", &ev(1, "opened")).unwrap();
        // The same event reached the other checkout by a merge, beside a reply of its own.
        let first = std::fs::read_dir(dir(a.path()).join("t1")).unwrap().next().unwrap().unwrap();
        std::fs::create_dir_all(dir(b.path()).join("t1")).unwrap();
        std::fs::copy(first.path(), dir(b.path()).join("t1").join(first.file_name())).unwrap();
        write(b.path(), "t1", &ev(2, "replied on a branch")).unwrap();
        let all = read_all(&[a.path().to_path_buf(), b.path().to_path_buf()]);
        let texts: Vec<_> = all["t1"].iter().map(|e| e.text.as_str()).collect();
        assert_eq!(texts, ["opened", "replied on a branch"]);
    }

    /// If a thread id could leave the comments directory, then refuses to write it.
    #[test]
    fn refuses_an_id_that_escapes() {
        let a = tempfile::tempdir().unwrap();
        assert!(write(a.path(), "../x", &ev(1, "no")).is_err());
        assert!(!valid_id(""));
    }
}
