//! Session logs on disk: `.scryer/sessions/<session_id>.jsonl`, one
//! [`SessionEntry`] per line, append-only.
//!
//! Each append is one `write` of one line on a file opened `O_APPEND`, so the
//! hook processes and the MCP server — separate processes writing the same
//! session — never interleave a partial line. A malformed line (a torn write
//! from a killed process) is skipped on read, never fatal.

use crate::domain::session::{SessionEntry, SessionEvent};
use crate::ModelRef;
use std::io::Write;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// The log path for `session`, or `None` when the id could escape the
/// sessions directory. Harness session ids are UUID-like; anything else is
/// refused rather than sanitized, so two ids never collide on one file.
pub fn session_path(r: &ModelRef, session: &str) -> Option<PathBuf> {
    let ok = !session.is_empty()
        && session.len() <= 128
        && session
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
        && !session.starts_with('.');
    ok.then(|| r.sessions_dir().join(format!("{session}.jsonl")))
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Create the sessions directory with its own `.gitignore`, so session logs
/// never reach a commit whatever the project's `.scryer/.gitignore` predates.
pub fn ensure_dir(r: &ModelRef) -> Result<(), String> {
    let dir = r.sessions_dir();
    if !dir.exists() {
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let _ = std::fs::write(dir.join(".gitignore"), "*\n");
    }
    Ok(())
}

/// Append one event to the session's log.
pub fn append(r: &ModelRef, session: &str, event: SessionEvent) -> Result<(), String> {
    let path = session_path(r, session).ok_or_else(|| format!("invalid session id '{session}'"))?;
    ensure_dir(r)?;
    let mut line = serde_json::to_string(&SessionEntry { at: now(), event }).map_err(|e| e.to_string())?;
    line.push('\n');
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| e.to_string())?;
    f.write_all(line.as_bytes()).map_err(|e| e.to_string())
}

/// Every readable entry of the session's log, in order. No file → empty.
pub fn read(r: &ModelRef, session: &str) -> Vec<SessionEntry> {
    let Some(path) = session_path(r, session) else { return Vec::new() };
    let Ok(raw) = std::fs::read_to_string(path) else { return Vec::new() };
    raw.lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

/// Every session with a log, most recently written first, with that mtime in
/// unix seconds.
pub fn list(r: &ModelRef) -> Vec<(String, u64)> {
    let Ok(entries) = std::fs::read_dir(r.sessions_dir()) else { return Vec::new() };
    let mut out: Vec<(String, u64)> = entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            let id = name.strip_suffix(".jsonl")?.to_string();
            let mtime = e
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            Some((id, mtime))
        })
        .collect();
    out.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    out
}


/// Project files modified after `ns` (unix nanoseconds) — whatever wrote them.
pub fn files_modified_since_ns(project: &Path, ns: u64) -> BTreeSet<String> {
    mtime_changed_files(project, ns / 1_000_000_000, Some(ns))
}

/// Files whose mtime is newer than the anchor. With a ns anchor the comparison
/// is nanosecond-exact (a same-second edit is visible); without one (old
/// `.sync` files) it falls back to whole seconds. Honors the same directory
/// skipping as the rest of scryer (SKIP_DIRS / SKIP_BUILD_DIRS, .gitignore).
fn mtime_changed_files(
    project: &Path,
    baseline_secs: u64,
    baseline_ns: Option<u64>,
) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for entry in project_walker(project).flatten() {
        if !entry.file_type().is_some_and(|ft| ft.is_file()) {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        let Ok(mtime) = meta.modified() else { continue };
        let since_epoch = mtime.duration_since(UNIX_EPOCH).unwrap_or_default();
        let newer = match baseline_ns {
            Some(anchor_ns) => since_epoch.as_nanos() as u64 > anchor_ns,
            None => since_epoch.as_secs() > baseline_secs,
        };
        if newer {
            if let Ok(rel) = entry.path().strip_prefix(project) {
                out.insert(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    out
}

/// The shared project walk: everything except vendor/build dirs.
fn project_walker(project: &Path) -> ignore::Walk {
    ignore::WalkBuilder::new(project)
        .hidden(false)
        .filter_entry(|entry| {
            if entry.file_type().is_some_and(|ft| ft.is_dir()) {
                let name = entry.file_name().to_string_lossy();
                if crate::domain::product_code::SKIP_DIRS.iter().any(|&s| name == s)
                    || crate::domain::product_code::SKIP_BUILD_DIRS.iter().any(|&s| name == s)
                {
                    return false;
                }
            }
            true
        })
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appends_read_back_in_order_and_skip_torn_lines() {
        let dir = tempfile::tempdir().unwrap();
        let r = ModelRef::ProjectLocal(dir.path().to_path_buf());
        append(&r, "s-1", SessionEvent::Touch { file: "a.rs".into() }).unwrap();
        append(&r, "s-1", SessionEvent::Stop).unwrap();
        // A torn line from a killed writer.
        let path = session_path(&r, "s-1").unwrap();
        let mut f = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
        f.write_all(b"{\"at\":1,\"ev").unwrap();
        drop(f);
        append(&r, "s-1", SessionEvent::Touch { file: "b.rs".into() }).unwrap();

        let events: Vec<SessionEvent> = read(&r, "s-1").into_iter().map(|e| e.event).collect();
        assert_eq!(events.len(), 2, "the torn line swallows the next line too, nothing else: {events:?}");
        assert_eq!(events[0], SessionEvent::Touch { file: "a.rs".into() });
        assert!(read(&r, "other").is_empty());
        assert_eq!(std::fs::read_to_string(r.sessions_dir().join(".gitignore")).unwrap(), "*\n");
        assert_eq!(list(&r).iter().map(|(s, _)| s.as_str()).collect::<Vec<_>>(), vec!["s-1"]);
    }

    #[test]
    fn ids_that_could_escape_the_directory_are_refused() {
        let r = ModelRef::ProjectLocal("/p".into());
        assert!(session_path(&r, "../x").is_none());
        assert!(session_path(&r, "a/b").is_none());
        assert!(session_path(&r, "").is_none());
        assert!(session_path(&r, ".hidden").is_none());
        assert!(session_path(&r, "3f2a-b_c.1").is_some());
    }
}
