//! What changed since you last looked: per agent session in the window, the
//! claims its edits newly proved, newly planned, left planned, or made stale
//! or failing, the surviving mutants and dark files in what it touched, and
//! which components more than one session touched. Read off the session
//! logs and the derived status; nothing is recorded for it.

use crate::domain::derived_status::{ClaimState, DerivedStatus};
use crate::domain::derived_tree::DerivedTree;
use crate::domain::session::{SessionEntry, SessionLog};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// A stub is "newly planned" by a session that saw it appear within its
/// window, give or take this many seconds for the derive to notice it.
const FIRST_SEEN_SLACK: u64 = 600;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LandingClaim {
    pub resp_id: String,
    pub node_id: String,
    pub statement: String,
    pub file: String,
    pub line: u32,
    pub state: ClaimState,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LandingSession {
    pub session: String,
    /// The task the session oriented on, else its id.
    pub title: String,
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
    /// Newest first.
    pub sessions: Vec<LandingSession>,
    /// Components touched by more than one session in the window.
    pub shared: Vec<SharedComponent>,
}

/// The landing view for sessions active since `since`. `first_seen` dates
/// each claim's first appearance in a derived tree.
pub fn landing(
    sessions: &[(String, Vec<SessionEntry>)],
    tree: &DerivedTree,
    status: &DerivedStatus,
    first_seen: &BTreeMap<String, u64>,
    since: u64,
) -> Landing {
    let states: HashMap<&str, &crate::domain::derived_status::ClaimStatus> =
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
        let log = SessionLog::from_events(entries);
        let touched: BTreeSet<&str> = log.touched.iter().map(String::as_str).collect();
        let reached = |c: &&crate::domain::derived_tree::DerivedClaim| {
            touched.contains(c.file.as_str())
                || tree
                    .planned
                    .source_map
                    .get(&c.id)
                    .is_some_and(|locs| locs.iter().any(|l| touched.contains(l.pattern.as_str())))
        };
        let mut s = LandingSession {
            session: id.clone(),
            title: log.task.clone().unwrap_or_else(|| format!("Session {}", &id[..id.len().min(8)])),
            started_at: start,
            ended_at: end,
            newly_proven: Vec::new(),
            newly_planned: Vec::new(),
            still_planned: Vec::new(),
            newly_stale_or_failing: Vec::new(),
            surviving_mutants: 0,
            new_dark_files: touched.iter().filter(|f| dark.contains(*f)).map(|f| f.to_string()).collect(),
            components: Vec::new(),
        };
        for c in tree.claims.iter().filter(reached) {
            let Some(st) = states.get(c.id.as_str()) else { continue };
            let row = LandingClaim {
                resp_id: c.id.clone(),
                node_id: c.node_id.clone(),
                statement: c.description.clone(),
                file: c.file.clone(),
                line: c.line,
                state: st.state,
            };
            let verdict_in_window = st.recorded_at.is_some_and(|at| at >= start);
            match st.state {
                ClaimState::Proven if verdict_in_window => s.newly_proven.push(row),
                ClaimState::Planned => {
                    let seen = first_seen.get(&c.id).copied().unwrap_or(0);
                    if seen >= start && seen <= end + FIRST_SEEN_SLACK {
                        s.newly_planned.push(row);
                    } else {
                        s.still_planned.push(row);
                    }
                }
                ClaimState::Stale => s.newly_stale_or_failing.push(row),
                ClaimState::Failing if verdict_in_window || touched.contains(c.file.as_str()) => {
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
    use crate::domain::derived_status::{ClaimStatus, DerivedCounts, NodeStatus};
    use crate::domain::derived_tree::{ClaimLevel, DerivedClaim};
    use crate::domain::session::SessionEvent;
    use crate::{ScryModel, SourceLocation};

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
        ClaimStatus { resp_id: id.into(), node_id: "comp".into(), state, probe: None, outcome: None, recorded_at: at }
    }

    fn touch(at: u64, file: &str) -> SessionEntry {
        SessionEntry { at, event: SessionEvent::Touch { file: file.into() } }
    }

    /// Each column follows from the session's edits and the claims' status:
    /// a verdict in its window is newly proven, a stub first seen in its
    /// window newly planned, an older stub still planned, a claim whose code
    /// it touched and that now reads stale newly stale; dark files and
    /// mutant survivors count over what it touched, and a component two
    /// sessions touched is shared.
    #[test]
    fn sessions_since_last_look_sort_their_claims() {
        let mut planned = ScryModel::new();
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
            file_nodes: BTreeMap::from([
                ("src/a.rs".to_string(), "comp".to_string()),
                ("src/b.rs".to_string(), "other".to_string()),
            ]),
            dark_files: vec!["src/a.rs".into()],
            style_violations: Vec::new(),
            layer_exempt: Vec::new(),
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
        let first_seen = BTreeMap::from([("stub".to_string(), 120), ("old_stub".to_string(), 10)]);
        let sessions = vec![
            (
                "aaaaaaaa-1".to_string(),
                vec![
                    SessionEntry { at: 100, event: SessionEvent::Task { text: "Queue offline saves".into() } },
                    touch(110, "src/a.rs"),
                    touch(200, "src/a.rs"),
                ],
            ),
            ("bbbbbbbb-2".to_string(), vec![touch(300, "src/a.rs"), touch(310, "src/b.rs")]),
            ("cccccccc-3".to_string(), vec![touch(5, "src/b.rs")]),
        ];
        let l = landing(&sessions, &tree, &status, &first_seen, 90);
        assert_eq!(l.sessions.iter().map(|s| s.session.as_str()).collect::<Vec<_>>(), vec!["bbbbbbbb-2", "aaaaaaaa-1"]);
        let a = &l.sessions[1];
        assert_eq!(a.title, "Queue offline saves");
        let ids = |rows: &[LandingClaim]| rows.iter().map(|r| r.resp_id.clone()).collect::<Vec<_>>();
        assert_eq!(ids(&a.newly_proven), vec!["green"]);
        assert_eq!(ids(&a.newly_planned), vec!["stub"]);
        assert_eq!(ids(&a.still_planned), vec!["old_stub"]);
        assert_eq!(ids(&a.newly_stale_or_failing), vec!["exercised"]);
        assert_eq!(a.surviving_mutants, 2);
        assert_eq!(a.new_dark_files, vec!["src/a.rs"]);
        assert_eq!(l.sessions[0].title, "Session bbbbbbbb");
        assert!(l.sessions[0].newly_proven.is_empty(), "verdicts before its window are not its doing");
        assert_eq!(l.shared, vec![SharedComponent { node_id: "comp".into(), sessions: vec!["aaaaaaaa-1".into(), "bbbbbbbb-2".into()] }]);
    }
}
