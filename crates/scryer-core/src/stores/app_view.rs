//! The desktop app's view as agents see it: whether the app has the project open, and the latest direction an
//! agent sent it. [infrastructure]
//!
//! Two files under `.scryer/view/`, never committed:
//!
//! - `app.json`: `{pid}`, written by the app while it has the project open. A record whose process
//!   has exited is no app, so a crash never leaves the tool advertised.
//! - `direction.json`: the latest direction. One file, replaced whole: an agent pointing somewhere new supersedes
//!   where it pointed before, and the app only ever acts on the newest.
//!
//! Files rather than a socket because every other channel between the agent's processes and the app already is
//! one (session logs, comments, verdicts), and the app's watcher already covers `.scryer/`.
//!
//! A direction also travels as a link the developer clicks: `scryer://show?project=…&node=…`, which the OS hands
//! to the app (launching it when closed). The link is built and read here so both ends agree on it. A link names
//! its project, since the app may have another open, and carries no time: the app stamps it when it arrives.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The app's record that it has this project open.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppPresence {
    pub pid: u32,
}

/// Where an agent asks the app to look.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Direction {
    /// The agent session that sent it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
    /// What the notice calls the sender: its task, else a short session name.
    pub sender: String,
    /// The node to show.
    pub node: String,
    /// `"map"` or `"wiki"`; absent keeps the view the developer is in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view: Option<String>,
    /// Claim ids on the node's page to flash.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub claims: Vec<String>,
    /// Page fields to flash: `"description"`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<String>,
    /// Other nodes to flash: their cards on the map.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nodes: Vec<String>,
    /// One line the notice shows beside the sender.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Milliseconds since the epoch: tells two directions to the same place apart.
    pub at: u64,
}

/// `.scryer/view/` of `project`.
pub fn view_dir(project: &Path) -> PathBuf {
    project.join(".scryer").join("view")
}

fn app_file(project: &Path) -> PathBuf {
    view_dir(project).join("app.json")
}

/// The direction file of `project`.
pub fn direction_file(project: &Path) -> PathBuf {
    view_dir(project).join("direction.json")
}

fn write_atomic(path: &Path, text: &str) -> Result<(), String> {
    let dir = path.parent().ok_or("no parent directory")?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let tmp = dir.join(format!(
        ".{}.{}.tmp",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("file"),
        std::process::id()
    ));
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

/// Record this process as the app with `project` open.
pub fn mark_open(project: &Path) -> Result<(), String> {
    let record = AppPresence { pid: std::process::id() };
    write_atomic(&app_file(project), &serde_json::to_string(&record).map_err(|e| e.to_string())?)
}

/// Clear this process's record for `project`; another process's stays.
pub fn mark_closed(project: &Path) {
    mark_closed_by(project, std::process::id());
}

fn mark_closed_by(project: &Path, pid: u32) {
    let path = app_file(project);
    let mine = std::fs::read_to_string(&path)
        .ok()
        .and_then(|t| serde_json::from_str::<AppPresence>(&t).ok())
        .is_some_and(|p| p.pid == pid);
    if mine {
        let _ = std::fs::remove_file(path);
    }
}

/// The app's record for `project`, while the process that wrote it runs.
pub fn app_open(project: &Path) -> Option<AppPresence> {
    app_open_with(project, process_alive)
}

/// [`app_open`] with `alive` telling whether a pid still runs.
pub fn app_open_with(project: &Path, alive: impl Fn(u32) -> bool) -> Option<AppPresence> {
    let record: AppPresence = serde_json::from_str(&std::fs::read_to_string(app_file(project)).ok()?).ok()?;
    alive(record.pid).then_some(record)
}

/// Whether process `pid` runs. Known on Linux; elsewhere the record is trusted.
fn process_alive(pid: u32) -> bool {
    !cfg!(target_os = "linux") || Path::new(&format!("/proc/{pid}")).exists()
}

/// Write `direction` as the latest for `project`, replacing the one before.
pub fn write_direction(project: &Path, direction: &Direction) -> Result<(), String> {
    write_atomic(&direction_file(project), &serde_json::to_string_pretty(direction).map_err(|e| e.to_string())?)
}

/// The latest direction for `project`, if one parses.
pub fn read_direction(project: &Path) -> Option<Direction> {
    serde_json::from_str(&std::fs::read_to_string(direction_file(project)).ok()?).ok()
}

/// Characters a link keeps as they are; `/` too, so a project path stays readable.
fn keep(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~' | b'/')
}

fn encode(s: &str) -> String {
    s.bytes().map(|b| if keep(b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

fn decode(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' => {
                out.push(u8::from_str_radix(std::str::from_utf8(bytes.get(i + 1..i + 3)?).ok()?, 16).ok()?);
                i += 3;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8(out).ok()
}

/// The link that opens the app at `direction` in `project`.
pub fn link_for(project: &Path, d: &Direction) -> String {
    let mut q = vec![format!("project={}", encode(&project.to_string_lossy())), format!("node={}", encode(&d.node))];
    if let Some(v) = &d.view {
        q.push(format!("view={}", encode(v)));
    }
    for (key, list) in [("claims", &d.claims), ("fields", &d.fields), ("nodes", &d.nodes)] {
        if !list.is_empty() {
            q.push(format!("{key}={}", list.iter().map(|x| encode(x)).collect::<Vec<_>>().join(",")));
        }
    }
    if let Some(n) = &d.note {
        q.push(format!("note={}", encode(n)));
    }
    q.push(format!("from={}", encode(&d.sender)));
    format!("scryer://show?{}", q.join("&"))
}

/// The project and direction a link names; `None` for anything but a `scryer://show` link with a project and a
/// node. Its time is left 0 for the receiver to stamp.
pub fn parse_link(link: &str) -> Option<(PathBuf, Direction)> {
    let rest = link.strip_prefix("scryer://show")?;
    let query = rest.strip_prefix('/').unwrap_or(rest).strip_prefix('?')?;
    let mut project = None;
    let mut d = Direction { sender: "a link".into(), ..Default::default() };
    let list = |v: &str| -> Option<Vec<String>> { v.split(',').filter(|x| !x.is_empty()).map(decode).collect() };
    for pair in query.split('&') {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        match k {
            "project" => project = Some(PathBuf::from(decode(v)?)),
            "node" => d.node = decode(v)?,
            "view" => d.view = Some(decode(v)?).filter(|v| v == "map" || v == "wiki"),
            "claims" => d.claims = list(v)?,
            "fields" => d.fields = list(v)?,
            "nodes" => d.nodes = list(v)?,
            "note" => d.note = Some(decode(v)?).filter(|n| !n.is_empty()),
            "from" => d.sender = decode(v)?,
            _ => {}
        }
    }
    let project = project.filter(|p| p.is_absolute())?;
    (!d.node.is_empty()).then_some((project, d))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// While the process that recorded the app as open runs, reads the app as open.
    #[test]
    fn reads_the_app_open_while_its_process_runs() {
        let dir = tempfile::tempdir().unwrap();
        mark_open(dir.path()).unwrap();
        assert_eq!(app_open_with(dir.path(), |_| true), Some(AppPresence { pid: std::process::id() }));
        assert!(app_open(dir.path()).is_some());
    }

    /// If the process that recorded the app as open has exited, then reads the app as closed.
    #[test]
    fn a_record_left_by_an_exited_process_is_no_app() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(app_open(dir.path()), None, "no record");
        mark_open(dir.path()).unwrap();
        assert_eq!(app_open_with(dir.path(), |_| false), None);
    }

    /// When the app closes the project, clears its own record but leaves another process's.
    #[test]
    fn closing_clears_only_its_own_record() {
        let dir = tempfile::tempdir().unwrap();
        mark_open(dir.path()).unwrap();
        mark_closed_by(dir.path(), std::process::id() + 1);
        assert!(app_open_with(dir.path(), |_| true).is_some(), "another process's close leaves it");
        mark_closed(dir.path());
        assert_eq!(app_open_with(dir.path(), |_| true), None);
    }

    /// Reads back from a link the project and direction it was made from, whatever the path and note hold.
    #[test]
    fn a_link_carries_its_project_and_direction() {
        let project = Path::new("/home/a/My Projects/notes & co");
        let d = Direction {
            sender: "fix the draft save".into(),
            node: "node-1".into(),
            view: Some("wiki".into()),
            claims: vec!["resp-a".into(), "resp-b".into()],
            fields: vec!["description".into()],
            nodes: vec!["node-2".into()],
            note: Some("why save: 100% & \"ok\"?".into()),
            ..Default::default()
        };
        let link = link_for(project, &d);
        assert!(link.starts_with("scryer://show?project=/home/a/My%20Projects/notes%20%26%20co&node=node-1"), "{link}");
        assert_eq!(parse_link(&link), Some((project.to_path_buf(), d)));
    }

    /// If a link is not a show link, or lacks an absolute project or a node, then reads nothing from it.
    #[test]
    fn rejects_links_it_cannot_follow() {
        assert_eq!(parse_link("https://example.com/show?project=/p&node=n"), None);
        assert_eq!(parse_link("scryer://show?node=n"), None);
        assert_eq!(parse_link("scryer://show?project=relative&node=n"), None);
        assert_eq!(parse_link("scryer://show?project=/p"), None);
        assert_eq!(parse_link("scryer://show?project=/p&node=%zz"), None);
        let (_, d) = parse_link("scryer://show/?project=/p&node=n&view=graph").unwrap();
        assert_eq!((d.view, d.sender.as_str()), (None, "a link"));
    }

    /// When a direction is written, replaces the one before it.
    #[test]
    fn the_latest_direction_replaces_the_one_before() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(read_direction(dir.path()), None);
        let first = Direction { sender: "a".into(), node: "n1".into(), at: 1, ..Default::default() };
        let second = Direction { sender: "b".into(), node: "n2".into(), view: Some("wiki".into()), claims: vec!["c".into()], at: 2, ..Default::default() };
        write_direction(dir.path(), &first).unwrap();
        write_direction(dir.path(), &second).unwrap();
        assert_eq!(read_direction(dir.path()), Some(second));
        let leftovers: Vec<_> = std::fs::read_dir(view_dir(dir.path())).unwrap().flatten().map(|e| e.file_name()).collect();
        assert_eq!(leftovers, vec![std::ffi::OsString::from("direction.json")], "no temp file left behind");
    }
}
