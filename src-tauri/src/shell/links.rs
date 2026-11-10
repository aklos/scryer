//! `scryer://show` links an agent hands the developer: the OS gives the link to the app (launching it when closed,
//! or, through the single-instance plugin, to the copy already running), and the window opens the link's project
//! and goes to its node.
//!
//! A link waits here until the window takes it: at a cold start it arrives before the webview listens, so the
//! window asks on mount, and on `open-link` while it runs.

use std::sync::Mutex;

use scryer_core::app_view::{parse_link, Direction};
use serde::Serialize;
use tauri::{Emitter, Manager};

/// Where a clicked link points: the project to open and the direction to follow in it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct LinkTarget {
    pub(crate) project: String,
    pub(crate) direction: Direction,
}

/// The newest link the window hasn't taken yet.
#[derive(Default)]
pub(crate) struct PendingLink(pub(crate) Mutex<Option<LinkTarget>>);

/// The target of the last link in `urls` that is a show link, stamped `at` milliseconds.
pub(crate) fn target_of<'a>(urls: impl IntoIterator<Item = &'a str>, at: u64) -> Option<LinkTarget> {
    urls.into_iter().filter_map(parse_link).last().map(|(project, mut direction)| {
        direction.at = at;
        LinkTarget { project: project.to_string_lossy().to_string(), direction }
    })
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or_default()
}

/// Hold the show link among `urls` for the window, tell it one is waiting, and bring the window forward.
pub(crate) fn receive<R: tauri::Runtime>(app: &tauri::AppHandle<R>, urls: &[String]) {
    let Some(target) = target_of(urls.iter().map(String::as_str), now_ms()) else { return };
    *app.state::<PendingLink>().0.lock().unwrap() = Some(target);
    let _ = app.emit("open-link", ());
    if let Some(window) = app.webview_windows().values().next() {
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// Make this build the default handler for `scryer://`. The deep-link plugin writes the handler's desktop entry, but
/// runs `update-desktop-database` first and stops when it is missing (NixOS ships without it), so the `xdg-mime`
/// default is set here as well; `xdg-open` reads it without the database.
#[cfg(target_os = "linux")]
pub(crate) fn ensure_default_handler() {
    let Some(name) = std::env::current_exe().ok().and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
    else {
        return;
    };
    let entry = format!("{name}-handler.desktop");
    let current = std::process::Command::new("xdg-mime")
        .args(["query", "default", "x-scheme-handler/scryer"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    if current != entry {
        let _ = std::process::Command::new("xdg-mime").args(["default", &entry, "x-scheme-handler/scryer"]).status();
    }
}

/// The link waiting for the window, taken once.
#[tauri::command]
pub(crate) fn take_link(pending: tauri::State<'_, PendingLink>) -> Option<LinkTarget> {
    pending.0.lock().unwrap().take()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// When the OS hands over a show link, reads its project and direction, stamped with when it arrived; anything else it ignores.
    #[test]
    fn reads_the_show_link_among_what_the_os_hands_over() {
        let link = "scryer://show?project=/home/a/notes&node=node-1&view=map&from=fix%20the%20save";
        let t = target_of(["/usr/bin/scryer", link], 42).unwrap();
        assert_eq!(t.project, "/home/a/notes");
        assert_eq!((t.direction.node.as_str(), t.direction.view.as_deref(), t.direction.at), ("node-1", Some("map"), 42));
        assert_eq!(t.direction.sender, "fix the save");
        assert_eq!(target_of(["https://example.com", "--flag"], 1), None);
    }
}
