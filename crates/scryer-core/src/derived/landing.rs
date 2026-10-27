//! What each agent session is doing and did: per session in the window, its
//! steps in order (the files it edited, the todo tests it wrote, its turns,
//! the verdicts its claims got), the claims its edits newly proved, newly
//! planned, left planned, or made stale or failing, the surviving mutants
//! and dark files in what it touched, and which components more than one
//! session touched. Read off the session logs and the derived status: a log
//! carries the claims proven at the session's start and the claims it
//! ingested verdicts for, so what it changed is its own.

use crate::derived_status::{ClaimState, DerivedStatus};
use crate::c4_tree::{DerivedClaim, DerivedTree};
use crate::session_log::{SessionEntry, SessionLog};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LandingClaim {
    pub resp_id: String,
    pub node_id: String,
    pub statement: String,
    pub file: String,
    pub line: u32,
    pub state: ClaimState,
    /// The claim's concern tags, shown at the end of its row.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub concerns: Vec<String>,
}

impl LandingClaim {
    /// `c` as a row, in `state`.
    pub fn of(c: &DerivedClaim, state: ClaimState) -> Self {
        LandingClaim {
            resp_id: c.id.clone(),
            node_id: c.node_id.clone(),
            statement: c.description.clone(),
            file: c.file.clone(),
            line: c.line,
            state,
            concerns: c.concerns.clone(),
        }
    }
}

/// Whether editing `files` reaches claim `c`: its test file is among them,
/// or the code it is anchored to.
pub fn reaches(tree: &DerivedTree, files: &BTreeSet<&str>, c: &DerivedClaim) -> bool {
    files.contains(c.file.as_str())
        || tree
            .planned
            .source_map
            .get(&c.id)
            .is_some_and(|locs| locs.iter().any(|l| files.contains(l.pattern.as_str())))
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LandingSession {
    pub session: String,
    /// The task the session oriented on, else the display name of the first
    /// component it touched; empty when it has neither.
    pub title: String,
    /// The title is the task the agent named when it oriented.
    #[serde(default)]
    pub titled_by_task: bool,
    pub started_at: u64,
    pub ended_at: u64,
    pub newly_proven: Vec<LandingClaim>,
    pub newly_planned: Vec<LandingClaim>,
    pub still_planned: Vec<LandingClaim>,
    pub newly_stale_or_failing: Vec<LandingClaim>,
    pub surviving_mutants: u32,
    pub new_dark_files: Vec<String>,
    /// Components whose files the session touched.
    pub components: Vec<String>,
    /// The checkout the session worked in (its path), when the landing
    /// spans several worktrees.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree: Option<String>,
    /// The session is working now: mid-turn, not merely open
    /// ([`crate::session_log::working`]).
    #[serde(default)]
    pub live: bool,
    /// What it did in the window, oldest first, the last [`MAX_STEPS`].
    #[serde(default)]
    pub steps: Vec<SessionStep>,
}

/// One thing a session did, as the page follows it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum SessionStep {
    /// It edited `file`, `edits` times in a row; `at` is the last.
    Edit {
        at: u64,
        file: String,
        /// The component the file belongs to.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        node_id: Option<String>,
        edits: u32,
    },
    /// It wrote a todo test: a claim it will satisfy.
    Todo { at: u64, claim: LandingClaim },
    /// A claim its edits reach got a verdict.
    Verdict { at: u64, claim: LandingClaim },
    /// It opened or closed a probe.
    Probe { at: u64, open: bool },
    /// It finished a turn.
    Stop { at: u64 },
}

impl SessionStep {
    pub fn at(&self) -> u64 {
        match self {
            SessionStep::Edit { at, .. }
            | SessionStep::Todo { at, .. }
            | SessionStep::Verdict { at, .. }
            | SessionStep::Probe { at, .. }
            | SessionStep::Stop { at } => *at,
        }
    }
}

/// How many of a session's latest steps the landing carries.
pub const MAX_STEPS: usize = 200;

/// A session's steps from `entries` at or after `from`, oldest first: runs
/// of edits to one file and runs of stops fold into one step, a stub is its
/// claim when the tree has it, and each claim in `verdicts` is a step at its
/// verdict's time.
fn session_steps(
    entries: &[SessionEntry],
    from: u64,
    tree: &DerivedTree,
    verdicts: Vec<(u64, LandingClaim)>,
) -> Vec<SessionStep> {
    use crate::session_log::SessionEvent;
    let mut steps: Vec<SessionStep> = verdicts.into_iter().map(|(at, claim)| SessionStep::Verdict { at, claim }).collect();
    for e in entries.iter().filter(|e| e.at >= from) {
        let step = match &e.event {
            SessionEvent::Touch { file } => SessionStep::Edit {
                at: e.at,
                file: file.clone(),
                node_id: tree.file_nodes.get(file).cloned(),
                edits: 1,
            },
            SessionEvent::Stub { file, test } => {
                let Some(c) = tree.claims.iter().find(|c| &c.file == file && &c.junit_name == test) else { continue };
                SessionStep::Todo { at: e.at, claim: LandingClaim::of(c, ClaimState::Planned) }
            }
            SessionEvent::ProbeOpen => SessionStep::Probe { at: e.at, open: true },
            SessionEvent::ProbeClose => SessionStep::Probe { at: e.at, open: false },
            SessionEvent::Stop => SessionStep::Stop { at: e.at },
            _ => continue,
        };
        steps.push(step);
    }
    // Stable, so log order holds within a second and a verdict lands after
    // the edits of its second.
    steps.sort_by_key(SessionStep::at);
    let mut out: Vec<SessionStep> = Vec::with_capacity(steps.len());
    for step in steps {
        match (out.last_mut(), &step) {
            (Some(SessionStep::Edit { at, file, edits, .. }), SessionStep::Edit { at: next, file: f, .. }) if file == f => {
                *at = *next;
                *edits += 1;
            }
            (Some(SessionStep::Stop { at }), SessionStep::Stop { at: next }) => *at = *next,
            _ => out.push(step),
        }
    }
    let cut = out.len().saturating_sub(MAX_STEPS);
    out.drain(..cut);
    out
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedComponent {
    pub node_id: String,
    pub sessions: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Landing {
    pub since: u64,
    /// The open checkout's branch (`None` when detached or not a git
    /// repository): the landing titles a session that has neither a task
    /// nor a touched component by it and the session's time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// The project's live git worktrees, each with its branch and how many
    /// sessions it had in the window; empty for a single checkout.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub worktrees: Vec<LandingWorktree>,
    /// Newest first.
    pub sessions: Vec<LandingSession>,
    /// Components touched by more than one session in the window.
    pub shared: Vec<SharedComponent>,
}

/// One live worktree of the project, as the landing lists it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LandingWorktree {
    pub path: String,
    /// `None` for a detached HEAD.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// The checkout the app has open.
    pub current: bool,
    pub sessions: usize,
}

/// One landing over several worktrees: each part's sessions labelled with
/// its worktree, newest first, and the components more than one session
/// touched counted across all of them (a component's id is the same in
/// every checkout of the repo).
pub fn merge_worktrees(since: u64, parts: Vec<(LandingWorktree, Landing)>) -> Landing {
    let mut out = Landing { since, ..Default::default() };
    for (mut wt, landing) in parts {
        wt.sessions = landing.sessions.len();
        for mut s in landing.sessions {
            s.worktree = Some(wt.path.clone());
            out.sessions.push(s);
        }
        out.worktrees.push(wt);
    }
    out.sessions.sort_by(|a, b| b.ended_at.cmp(&a.ended_at).then(a.session.cmp(&b.session)));
    let mut touched_by: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for s in &out.sessions {
        for n in &s.components {
            touched_by.entry(n.clone()).or_default().insert(s.session.clone());
        }
    }
    out.shared = touched_by
        .into_iter()
        .filter(|(_, ss)| ss.len() > 1)
        .map(|(node_id, ss)| SharedComponent { node_id, sessions: ss.into_iter().collect() })
        .collect();
    out
}

/// Take out of `sessions` the ones that worked in a nested worktree: every
/// file they read, edited or stubbed sits under `prefix` (that worktree's
/// path relative to this checkout, with a trailing `/`). Those paths are
/// rewritten relative to the worktree, so its own tree can read them.
pub fn take_prefixed(
    sessions: &mut Vec<(String, Vec<SessionEntry>)>,
    prefix: &str,
) -> Vec<(String, Vec<SessionEntry>)> {
    use crate::session_log::SessionEvent;
    fn file_of(event: &mut SessionEvent) -> Option<&mut String> {
        match event {
            SessionEvent::Touch { file } | SessionEvent::Overlay { file, .. } | SessionEvent::Stub { file, .. } => {
                Some(file)
            }
            _ => None,
        }
    }
    let inside = |entries: &[SessionEntry]| {
        let mut files = entries.iter().filter_map(|e| match &e.event {
            SessionEvent::Touch { file } | SessionEvent::Overlay { file, .. } | SessionEvent::Stub { file, .. } => {
                Some(file)
            }
            _ => None,
        });
        let first = files.next();
        first.is_some_and(|f| f.starts_with(prefix)) && files.all(|f| f.starts_with(prefix))
    };
    let (taken, kept): (Vec<_>, Vec<_>) = std::mem::take(sessions).into_iter().partition(|(_, e)| inside(e));
    *sessions = kept;
    taken
        .into_iter()
        .map(|(id, entries)| {
            let entries = entries
                .into_iter()
                .map(|mut e| {
                    if let Some(file) = file_of(&mut e.event) {
                        *file = file[prefix.len()..].to_string();
                    }
                    e
                })
                .collect();
            (id, entries)
        })
        .collect()
}

/// The display name of the first component a session touched, in touch
/// order: its title when the agent never oriented on a task.
fn first_component_name(touched: &[String], tree: &DerivedTree) -> Option<String> {
    let id = touched.iter().find_map(|f| tree.file_nodes.get(f))?;
    tree.planned.nodes.iter().find(|n| &n.id == id).map(|n| n.name.clone())
}

/// The landing view for sessions active since `since`, each read for what
/// it did after `since`. A planned claim is a session's newly planned work
/// when the session's log records it as a stub it wrote; other planned
/// claims its edits reach are still planned.
///
/// A session whose log has its start's baseline is read for the changes of
/// state it made: it newly proved a claim proven now that was not at its
/// start, when it recorded that claim's verdict or wrote its test; it broke
/// one proven at its start and failing now, or made it stale with its
/// edits. A log from before the baseline is read by reach: every claim its
/// edits reach, by its state now.
pub fn landing(
    sessions: &[(String, Vec<SessionEntry>)],
    tree: &DerivedTree,
    status: &DerivedStatus,
    since: u64,
) -> Landing {
    let states: HashMap<&str, &crate::derived_status::ClaimStatus> =
        status.claims.iter().map(|c| (c.resp_id.as_str(), c)).collect();
    let dark: BTreeSet<&str> = tree.dark_files.iter().map(String::as_str).collect();
    let mut out = Landing { since, ..Default::default() };
    let mut touched_by: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (id, entries) in sessions {
        let (Some(start), Some(end)) = (entries.iter().map(|e| e.at).min(), entries.iter().map(|e| e.at).max()) else {
            continue;
        };
        if end < since {
            continue;
        }
        // A session that started before the window is read for what it did
        // in it: its touches and stubs since, and verdicts recorded since.
        let log = SessionLog::from_events(entries);
        let recent: Vec<SessionEntry> = entries.iter().filter(|e| e.at >= since).cloned().collect();
        let now = SessionLog::from_events(&recent);
        let from = start.max(since);
        let touched: BTreeSet<&str> = now.touched.iter().map(String::as_str).collect();
        let reached = |c: &&DerivedClaim| reaches(tree, &touched, c);
        let mut s = LandingSession {
            session: id.clone(),
            title: log.task.clone().or_else(|| first_component_name(&log.touched, tree)).unwrap_or_default(),
            titled_by_task: log.task.is_some(),
            started_at: start,
            ended_at: end,
            newly_proven: Vec::new(),
            newly_planned: Vec::new(),
            still_planned: Vec::new(),
            newly_stale_or_failing: Vec::new(),
            surviving_mutants: 0,
            new_dark_files: touched.iter().filter(|f| dark.contains(*f)).map(|f| f.to_string()).collect(),
            components: Vec::new(),
            worktree: None,
            live: false,
            steps: Vec::new(),
        };
        let mut verdicts = Vec::new();
        let stubbed = |c: &DerivedClaim| {
            now.stubs.iter().any(|(f, t)| *f == c.file && *t == c.junit_name)
        };
        let baseline: Option<BTreeSet<&str>> =
            log.proven_at_start.as_ref().map(|ids| ids.iter().map(String::as_str).collect());
        let ingested: BTreeSet<&str> = log.verdicts.iter().map(String::as_str).collect();
        for c in tree.claims.iter().filter(|c| reached(c) || stubbed(c) || ingested.contains(c.id.as_str())) {
            let Some(st) = states.get(c.id.as_str()) else { continue };
            let row = LandingClaim::of(c, st.state);
            let verdict_in_window = st.recorded_at.is_some_and(|at| at >= from);
            let own = match &baseline {
                Some(_) => ingested.contains(c.id.as_str()) || stubbed(c),
                None => reached(&c) || stubbed(c),
            };
            if let (Some(at), true, true, ClaimState::Proven | ClaimState::Failing) =
                (st.recorded_at, verdict_in_window, own, st.state)
            {
                verdicts.push((at, row.clone()));
            }
            let was_proven = baseline.as_ref().map(|b| b.contains(c.id.as_str()));
            match (st.state, was_proven) {
                (ClaimState::Planned, _) if stubbed(c) => s.newly_planned.push(row),
                (ClaimState::Planned, _) if reached(&c) => s.still_planned.push(row),
                (ClaimState::Proven, Some(false)) if own && verdict_in_window => s.newly_proven.push(row),
                (ClaimState::Proven, None) if own && verdict_in_window => s.newly_proven.push(row),
                (ClaimState::Stale, Some(true)) if reached(&c) => s.newly_stale_or_failing.push(row),
                (ClaimState::Stale, None) if reached(&c) => s.newly_stale_or_failing.push(row),
                (ClaimState::Failing, Some(was)) if was || own => s.newly_stale_or_failing.push(row),
                (ClaimState::Failing, None) if verdict_in_window || touched.contains(c.file.as_str()) => {
                    s.newly_stale_or_failing.push(row)
                }
                _ => {}
            }
        }
        let components: BTreeSet<&str> =
            touched.iter().filter_map(|f| tree.file_nodes.get(*f)).map(String::as_str).collect();
        s.surviving_mutants = components
            .iter()
            .filter_map(|n| status.nodes.get(*n))
            .map(|n| n.own.surviving_mutants)
            .sum();
        for n in &components {
            touched_by.entry(n.to_string()).or_default().insert(id.clone());
        }
        s.components = components.into_iter().map(str::to_string).collect();
        s.steps = session_steps(entries, from, tree, verdicts);
        out.sessions.push(s);
    }
    out.sessions.sort_by(|a, b| b.ended_at.cmp(&a.ended_at).then(a.session.cmp(&b.session)));
    out.shared = touched_by
        .into_iter()
        .filter(|(_, ss)| ss.len() > 1)
        .map(|(node_id, ss)| SharedComponent { node_id, sessions: ss.into_iter().collect() })
        .collect();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::derived_status::{ClaimStatus, DerivedCounts, NodeStatus};
    use crate::c4_tree::{ClaimLevel, DerivedClaim};
    use crate::session_log::SessionEvent;
    use crate::c4_model::{ScryModel, SourceLocation};

    fn claim(id: &str, file: &str, planned: bool) -> DerivedClaim {
        DerivedClaim {
            id: id.into(),
            node_id: "comp".into(),
            level: ClaimLevel::Component,
            file: file.into(),
            line: 1,
            identifier: id.into(),
            description: format!("claim {id}"),
            concerns: Vec::new(),
            junit_name: id.into(),
            planned,
        }
    }

    fn status(id: &str, state: ClaimState, at: Option<u64>) -> ClaimStatus {
        ClaimStatus { resp_id: id.into(), node_id: "comp".into(), state, probe: None, outcome: None, recorded_at: at, proven_as: None }
    }

    fn touch(at: u64, file: &str) -> SessionEntry {
        SessionEntry { at, event: SessionEvent::Touch { file: file.into() } }
    }

    /// Each column follows from the session's edits and the claims' status:
    /// a verdict in its window is newly proven, a stub its log records
    /// newly planned, an older stub still planned, a claim whose code
    /// it touched and that now reads stale newly stale; dark files and
    /// mutant survivors count over what it touched, and a component two
    /// sessions touched is shared.
    #[test]
    fn sessions_in_the_window_sort_their_claims() {
        let mut planned = ScryModel::new();
        planned.nodes.push(
            serde_json::from_value(serde_json::json!({"id": "comp", "kind": "component", "name": "drafts"})).unwrap(),
        );
        planned.source_map.insert(
            "exercised".into(),
            vec![SourceLocation { pattern: "src/a.rs".into(), symbol: None, line: None, end_line: None }],
        );
        let tree = DerivedTree {
            fingerprint: 0,
            committed: planned.clone(),
            planned,
            claims: vec![
                claim("green", "src/a.rs", false),
                claim("exercised", "tests/a.rs", false),
                claim("stub", "src/a.rs", true),
                claim("old_stub", "src/a.rs", true),
                claim("elsewhere", "src/b.rs", false),
            ],
            doc_files: Default::default(),
            file_nodes: BTreeMap::from([
                ("src/a.rs".to_string(), "comp".to_string()),
                ("src/b.rs".to_string(), "other".to_string()),
            ]),
            dark_files: vec!["src/a.rs".into()],
            style_violations: Vec::new(),
            layer_exempt: Vec::new(),
            tooling: Default::default(),
        };
        let status = DerivedStatus {
            claims: vec![
                status("green", ClaimState::Proven, Some(150)),
                status("exercised", ClaimState::Stale, Some(50)),
                status("stub", ClaimState::Planned, None),
                status("old_stub", ClaimState::Planned, None),
                status("elsewhere", ClaimState::Proven, Some(150)),
            ],
            nodes: BTreeMap::from([(
                "comp".to_string(),
                NodeStatus { own: DerivedCounts { surviving_mutants: 2, ..Default::default() }, ..Default::default() },
            )]),
            ..Default::default()
        };
        let sessions = vec![
            (
                "aaaaaaaa-1".to_string(),
                vec![
                    SessionEntry { at: 100, event: SessionEvent::Task { text: "Queue offline saves".into() } },
                    touch(110, "src/a.rs"),
                    SessionEntry { at: 150, event: SessionEvent::Stub { file: "src/a.rs".into(), test: "stub".into() } },
                    touch(200, "src/a.rs"),
                ],
            ),
            ("bbbbbbbb-2".to_string(), vec![touch(300, "src/a.rs"), touch(310, "src/b.rs")]),
            ("cccccccc-3".to_string(), vec![touch(5, "src/b.rs")]),
        ];
        let l = landing(&sessions, &tree, &status, 90);
        assert_eq!(l.sessions.iter().map(|s| s.session.as_str()).collect::<Vec<_>>(), vec!["bbbbbbbb-2", "aaaaaaaa-1"]);
        let a = &l.sessions[1];
        assert_eq!(a.title, "Queue offline saves");
        assert!(a.titled_by_task);
        let ids = |rows: &[LandingClaim]| rows.iter().map(|r| r.resp_id.clone()).collect::<Vec<_>>();
        assert_eq!(ids(&a.newly_proven), vec!["green"]);
        assert_eq!(ids(&a.newly_planned), vec!["stub"]);
        assert_eq!(ids(&a.still_planned), vec!["old_stub"]);
        assert_eq!(ids(&a.newly_stale_or_failing), vec!["exercised"]);
        assert_eq!(a.surviving_mutants, 2);
        assert_eq!(a.new_dark_files, vec!["src/a.rs"]);
        assert_eq!(l.sessions[0].title, "drafts", "no task: the first component it touched");
        assert!(!l.sessions[0].titled_by_task);
        assert!(l.sessions[0].newly_proven.is_empty(), "verdicts before its window are not its doing");
        assert_eq!(l.shared, vec![SharedComponent { node_id: "comp".into(), sessions: vec!["aaaaaaaa-1".into(), "bbbbbbbb-2".into()] }]);
    }

    /// When a session neither oriented on a task nor touched a file of any
    /// component, its title is left empty, for the page to name it by branch
    /// and time.
    /// When a session started before the window, shows only what it did in it:
    /// files touched before the window reach nothing, and verdicts recorded
    /// before it are not newly proven.
    /// When a session's log has the baseline its start recorded, counts the changes of state it made: a claim it got a verdict for that is proven now and was not at its start is newly proven, one already proven is not; one proven at its start and failing now is broken; a todo test already in the code its edits reach is not its newly planned work.
    #[test]
    fn a_session_with_a_baseline_counts_the_changes_it_made() {
        let planned = ScryModel::new();
        let tree = DerivedTree {
            fingerprint: 0,
            committed: planned.clone(),
            planned,
            claims: vec![
                claim("fixed", "src/a.rs", false),
                claim("rerun", "src/a.rs", false),
                claim("broke", "src/a.rs", false),
                claim("theirs", "src/a.rs", false),
                claim("backlog", "src/a.rs", true),
                claim("stub", "src/a.rs", true),
            ],
            doc_files: Default::default(),
            file_nodes: BTreeMap::from([("src/a.rs".to_string(), "comp".to_string())]),
            dark_files: Vec::new(),
            style_violations: Vec::new(),
            layer_exempt: Vec::new(),
            tooling: Default::default(),
        };
        let status = DerivedStatus {
            claims: vec![
                status("fixed", ClaimState::Proven, Some(150)),
                status("rerun", ClaimState::Proven, Some(150)),
                status("broke", ClaimState::Failing, Some(150)),
                status("theirs", ClaimState::Proven, Some(150)),
                status("backlog", ClaimState::Planned, None),
                status("stub", ClaimState::Planned, None),
            ],
            ..Default::default()
        };
        let ids = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let entries = vec![
            SessionEntry { at: 100, event: SessionEvent::Proven { ids: ids(&["rerun", "broke"]) } },
            touch(110, "src/a.rs"),
            SessionEntry { at: 120, event: SessionEvent::Stub { file: "src/a.rs".into(), test: "stub".into() } },
            SessionEntry { at: 150, event: SessionEvent::Verdicts { ids: ids(&["fixed", "rerun", "broke"]) } },
        ];
        let l = landing(&[("s".to_string(), entries)], &tree, &status, 0);
        let s = &l.sessions[0];
        let names = |v: &[LandingClaim]| v.iter().map(|c| c.resp_id.as_str()).collect::<Vec<_>>().join(",");
        assert_eq!(names(&s.newly_proven), "fixed", "re-proving what was proven, or another session's verdict, is not this session's");
        assert_eq!(names(&s.newly_stale_or_failing), "broke");
        assert_eq!(names(&s.newly_planned), "stub");
        assert_eq!(names(&s.still_planned), "backlog");
    }

    #[test]
    fn a_session_spanning_the_window_start_shows_only_what_came_after() {
        let mut planned = ScryModel::new();
        planned.nodes.push(
            serde_json::from_value(serde_json::json!({"id": "comp", "kind": "component", "name": "drafts"})).unwrap(),
        );
        let tree = DerivedTree {
            fingerprint: 0,
            committed: planned.clone(),
            planned,
            claims: vec![claim("early", "src/a.rs", false), claim("late", "src/b.rs", false), claim("rerun", "src/b.rs", false)],
            doc_files: Default::default(),
            file_nodes: BTreeMap::from([("src/a.rs".to_string(), "comp".to_string()), ("src/b.rs".to_string(), "comp".to_string())]),
            dark_files: Vec::new(),
            style_violations: Vec::new(),
            layer_exempt: Vec::new(),
            tooling: Default::default(),
        };
        let status = DerivedStatus {
            claims: vec![
                status("early", ClaimState::Proven, Some(60)),
                status("late", ClaimState::Proven, Some(160)),
                status("rerun", ClaimState::Proven, Some(80)),
            ],
            ..Default::default()
        };
        let ids = |rows: &[LandingClaim]| rows.iter().map(|r| r.resp_id.clone()).collect::<Vec<_>>();
        let sessions = vec![("s".to_string(), vec![touch(50, "src/a.rs"), touch(150, "src/b.rs")])];
        let s = &landing(&sessions, &tree, &status, 100).sessions[0];
        assert_eq!(ids(&s.newly_proven), vec!["late"]);
        assert_eq!(s.components, vec!["comp"]);
        let all = &landing(&sessions, &tree, &status, 0).sessions[0];
        assert_eq!(ids(&all.newly_proven), vec!["early", "late", "rerun"]);
    }

    /// A session's steps run oldest first: edits to one file in a row fold
    /// into one step with their count, a stub it wrote is its claim, a
    /// verdict its claims got lands at the verdict's time, and steps before
    /// the window are left out.
    #[test]
    fn a_sessions_steps_follow_what_it_did_in_order() {
        let tree = DerivedTree {
            fingerprint: 0,
            committed: ScryModel::new(),
            planned: ScryModel::new(),
            claims: vec![claim("green", "src/a.rs", false), claim("stub", "src/a.rs", true)],
            doc_files: Default::default(),
            file_nodes: BTreeMap::from([("src/a.rs".to_string(), "comp".to_string())]),
            dark_files: Vec::new(),
            style_violations: Vec::new(),
            layer_exempt: Vec::new(),
            tooling: Default::default(),
        };
        let status = DerivedStatus {
            claims: vec![status("green", ClaimState::Proven, Some(130)), status("stub", ClaimState::Planned, None)],
            ..Default::default()
        };
        let at = |at, event| SessionEntry { at, event };
        let sessions = vec![(
            "s".to_string(),
            vec![
                touch(5, "src/old.rs"),
                touch(100, "src/a.rs"),
                touch(110, "src/a.rs"),
                at(115, SessionEvent::Stub { file: "src/a.rs".into(), test: "stub".into() }),
                touch(120, "src/b.rs"),
                at(140, SessionEvent::Stop),
                at(141, SessionEvent::Stop),
            ],
        )];
        let steps = &landing(&sessions, &tree, &status, 50).sessions[0].steps;
        let shape: Vec<String> = steps
            .iter()
            .map(|s| match s {
                SessionStep::Edit { at, file, node_id, edits } => format!("{at} edit {file} {node_id:?} x{edits}"),
                SessionStep::Todo { at, claim } => format!("{at} todo {}", claim.resp_id),
                SessionStep::Verdict { at, claim } => format!("{at} verdict {}", claim.resp_id),
                SessionStep::Probe { at, open } => format!("{at} probe {open}"),
                SessionStep::Stop { at } => format!("{at} stop"),
            })
            .collect();
        assert_eq!(
            shape,
            vec![
                "110 edit src/a.rs Some(\"comp\") x2",
                "115 todo stub",
                "120 edit src/b.rs None x1",
                "130 verdict green",
                "141 stop",
            ]
        );
    }

    #[test]
    fn a_session_without_task_or_component_has_no_title() {
        let sessions = vec![("dddddddd-4".to_string(), vec![touch(10, "README.md")])];
        let tree = DerivedTree {
            fingerprint: 0,
            committed: ScryModel::new(),
            planned: ScryModel::new(),
            claims: Vec::new(),
            doc_files: Default::default(),
            file_nodes: BTreeMap::new(),
            dark_files: Vec::new(),
            style_violations: Vec::new(),
            layer_exempt: Vec::new(),
            tooling: Default::default(),
        };
        let l = landing(&sessions, &tree, &DerivedStatus::default(), 0);
        assert_eq!(l.sessions[0].title, "");
    }

    /// When sessions come from several worktrees, each is labelled with its
    /// worktree, a worktree without sessions is still listed, and a
    /// component touched in two worktrees counts as shared.
    #[test]
    fn worktrees_merge_into_one_landing() {
        let session = |id: &str, at: u64, comp: &str| LandingSession {
            session: id.into(),
            title: id.into(),
            titled_by_task: false,
            started_at: at,
            ended_at: at,
            newly_proven: Vec::new(),
            newly_planned: Vec::new(),
            still_planned: Vec::new(),
            newly_stale_or_failing: Vec::new(),
            surviving_mutants: 0,
            new_dark_files: Vec::new(),
            components: vec![comp.into()],
            worktree: None,
            live: false,
            steps: Vec::new(),
        };
        let wt = |path: &str, branch: &str| LandingWorktree {
            path: path.into(),
            branch: Some(branch.into()),
            current: path == "/main",
            sessions: 0,
        };
        let part = |sessions: Vec<LandingSession>| Landing { sessions, ..Default::default() };
        let l = merge_worktrees(
            0,
            vec![
                (wt("/main", "main"), part(vec![session("a", 10, "comp-x")])),
                (wt("/wt/feature", "feature"), part(vec![session("b", 20, "comp-x"), session("c", 5, "comp-y")])),
                (wt("/wt/idle", "idle"), part(Vec::new())),
            ],
        );
        assert_eq!(l.sessions.iter().map(|s| s.session.as_str()).collect::<Vec<_>>(), vec!["b", "a", "c"]);
        assert_eq!(l.sessions[0].worktree.as_deref(), Some("/wt/feature"));
        assert_eq!(l.worktrees.iter().map(|w| w.sessions).collect::<Vec<_>>(), vec![1, 2, 0]);
        assert_eq!(l.shared, vec![SharedComponent { node_id: "comp-x".into(), sessions: vec!["a".into(), "b".into()] }]);
    }

    /// When a session read or touched only files under a nested worktree's
    /// prefix, it moves to that worktree with those paths made relative to
    /// it; a session that touched anything else stays.
    #[test]
    fn sessions_in_a_nested_worktree_move_to_it() {
        let mut sessions = vec![
            ("in".to_string(), vec![touch(1, ".claude/worktrees/x/src/a.rs"), touch(2, ".claude/worktrees/x/src/b.rs")]),
            ("mixed".to_string(), vec![touch(1, ".claude/worktrees/x/src/a.rs"), touch(2, "src/c.rs")]),
            ("read".to_string(), vec![SessionEntry { at: 1, event: SessionEvent::Overlay { file: ".claude/worktrees/x/src/a.rs".into(), hash: 7 } }]),
            ("none".to_string(), vec![SessionEntry { at: 1, event: SessionEvent::Task { text: "t".into() } }]),
        ];
        let taken = take_prefixed(&mut sessions, ".claude/worktrees/x/");
        assert_eq!(taken.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>(), vec!["in", "read"]);
        assert_eq!(taken[0].1[0], touch(1, "src/a.rs"));
        assert_eq!(taken[1].1[0].event, SessionEvent::Overlay { file: "src/a.rs".into(), hash: 7 });
        assert_eq!(sessions.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>(), vec!["mixed", "none"]);
    }
}
