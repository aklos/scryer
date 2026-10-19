//! What each derived claim's evidence says, never written by a tool:
//! `proven` (green, and neither the test nor the code it exercises has
//! changed since), `stale` (either changed, or no report has covered it since
//! it was written), `failing` (red), `planned` (a todo stub). Probing is a
//! separate axis: a surviving break marks a crosshair, every break caught a
//! double check. Health counts roll the states up the derived tree.

use crate::domain::derived_tree::{DerivedClaim, DerivedTree};
use crate::domain::model::Kind;
use crate::domain::test_results::TestOutcome;
use crate::domain::verdicts::{
    ClaimProbeStatus, ClaimTestStatus, MutantTally, MutationCache, TestStatusCache,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ClaimState {
    Proven,
    Stale,
    Failing,
    Planned,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProbeMark {
    /// A deliberate break survived: the test would not notice that bug.
    Crosshair,
    /// Every break tried was caught.
    DoubleCheck,
}

/// One claim's derived status.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimStatus {
    pub resp_id: String,
    pub node_id: String,
    pub state: ClaimState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub probe: Option<ProbeMark>,
    /// The last recorded outcome, when a report ever covered the claim.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<TestOutcome>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recorded_at: Option<u64>,
}

/// Health counters over one scope — a node's own content, or its subtree.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DerivedCounts {
    pub claims: u32,
    pub proven: u32,
    pub stale: u32,
    pub failing: u32,
    pub planned: u32,
    /// Components with code and not one claim in their subtree.
    pub untested_components: u32,
    pub surviving_mutants: u32,
    pub style_violations: u32,
    /// Product files no test reaches by import.
    pub dark_files: u32,
}

impl DerivedCounts {
    fn merge(&mut self, o: &DerivedCounts) {
        self.claims += o.claims;
        self.proven += o.proven;
        self.stale += o.stale;
        self.failing += o.failing;
        self.planned += o.planned;
        self.untested_components += o.untested_components;
        self.surviving_mutants += o.surviving_mutants;
        self.style_violations += o.style_violations;
        self.dark_files += o.dark_files;
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeStatus {
    pub own: DerivedCounts,
    pub subtree: DerivedCounts,
    /// A mutation tool's verdict on this node's own files.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub probe: Option<ProbeMark>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dark_files: Vec<String>,
}

/// The derived tree's status: per claim, per node, and in total. `tests` and
/// `probes` are the lists the Tests lens reads today, keyed by derived claim
/// id; `nodes`/`totals` mirror get_health's per-node and rollup shape.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DerivedStatus {
    pub claims: Vec<ClaimStatus>,
    pub tests: Vec<ClaimTestStatus>,
    pub probes: Vec<ClaimProbeStatus>,
    pub nodes: BTreeMap<String, NodeStatus>,
    pub totals: DerivedCounts,
}

/// Is a record taken at `recorded_at` with these anchor fingerprints still
/// true of the code for this claim? The caller decides how hard to look.
pub type Freshness<'a> = dyn FnMut(&str, &BTreeMap<String, String>, u64) -> bool + 'a;

fn mark(t: &MutantTally) -> Option<ProbeMark> {
    if t.survived > 0 {
        Some(ProbeMark::Crosshair)
    } else if t.caught > 0 {
        Some(ProbeMark::DoubleCheck)
    } else {
        None
    }
}

fn worst(a: Option<ProbeMark>, b: Option<ProbeMark>) -> Option<ProbeMark> {
    match (a, b) {
        (Some(ProbeMark::Crosshair), _) | (_, Some(ProbeMark::Crosshair)) => Some(ProbeMark::Crosshair),
        (Some(m), _) | (_, Some(m)) => Some(m),
        _ => None,
    }
}

fn norm(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

/// The names a claim answers to in a mutation report: its JUnit name (suite
/// path joined by spaces, as Stryker spells it), its leaf, its identifier.
fn claim_names(c: &DerivedClaim) -> Vec<String> {
    let leaf = c.junit_name.rsplit(" > ").next().unwrap_or(&c.junit_name);
    vec![norm(&c.junit_name.replace(" > ", " ")), norm(leaf), norm(&c.identifier)]
}

/// The names a reported test could mean: whole, or its last `.`/`::` segment
/// (Stryker.NET and nextest report qualified names).
fn reported_names(name: &str) -> Vec<String> {
    let mut out = vec![norm(name)];
    if !name.contains(' ') {
        if let Some(last) = name.rsplit(['.', ':']).next() {
            out.push(norm(last));
        }
    }
    out
}

pub fn compute_derived_status(
    tree: &DerivedTree,
    cache: &TestStatusCache,
    mutations: &MutationCache,
    fresh: &mut Freshness,
) -> DerivedStatus {
    let mut out = DerivedStatus::default();

    // Mutation results per test, attributed to the one claim the name picks.
    let mut by_name: HashMap<String, Vec<&str>> = HashMap::new();
    for c in &tree.claims {
        let mut names = claim_names(c);
        names.dedup();
        for n in names {
            by_name.entry(n).or_default().push(c.id.as_str());
        }
    }
    let mut claim_mutants: HashMap<&str, MutantTally> = HashMap::new();
    for (name, tally) in &mutations.tests {
        let hit = reported_names(name).into_iter().find_map(|n| match by_name.get(&n) {
            Some(ids) if ids.len() == 1 => Some(ids[0]),
            _ => None,
        });
        if let Some(id) = hit {
            let t = claim_mutants.entry(id).or_default();
            t.caught += tally.caught;
            t.survived += tally.survived;
        }
    }

    let mut own: HashMap<&str, DerivedCounts> = HashMap::new();
    for c in &tree.claims {
        let record = cache.results.iter().find(|r| r.resp_id == c.id);
        let current = record.filter(|r| !r.fingerprints.is_empty() && fresh(&c.id, &r.fingerprints, r.recorded_at));
        let state = if c.planned {
            ClaimState::Planned
        } else {
            match current {
                None => ClaimState::Stale,
                Some(r) if r.outcome == TestOutcome::Passed => ClaimState::Proven,
                Some(_) => ClaimState::Failing,
            }
        };
        if let Some(r) = record {
            out.tests.push(ClaimTestStatus {
                resp_id: c.id.clone(),
                outcome: r.outcome,
                cases: r.cases,
                stale: current.is_none(),
                recorded_at: r.recorded_at,
            });
        }
        let probe_record = cache.probes.iter().find(|p| p.resp_id == c.id && p.probes > 0);
        let mut probe = None;
        if let Some(p) = probe_record {
            let live = !p.fingerprints.is_empty() && fresh(&c.id, &p.fingerprints, p.recorded_at);
            out.probes.push(ClaimProbeStatus {
                resp_id: c.id.clone(),
                probes: p.probes,
                survived: p.survived,
                survivors: p.survivors.clone(),
                stale: !live,
                recorded_at: p.recorded_at,
            });
            if live {
                probe = Some(if p.survived > 0 { ProbeMark::Crosshair } else { ProbeMark::DoubleCheck });
            }
        }
        probe = worst(probe, claim_mutants.get(c.id.as_str()).and_then(mark));
        let counts = own.entry(c.node_id.as_str()).or_default();
        counts.claims += 1;
        match state {
            ClaimState::Proven => counts.proven += 1,
            ClaimState::Stale => counts.stale += 1,
            ClaimState::Failing => counts.failing += 1,
            ClaimState::Planned => counts.planned += 1,
        }
        out.claims.push(ClaimStatus {
            resp_id: c.id.clone(),
            node_id: c.node_id.clone(),
            state,
            probe,
            outcome: record.map(|r| r.outcome),
            recorded_at: record.map(|r| r.recorded_at),
        });
    }

    let mut node_probe: HashMap<&str, MutantTally> = HashMap::new();
    for (file, tally) in &mutations.files {
        let Some(node) = tree.file_nodes.get(file) else { continue };
        own.entry(node.as_str()).or_default().surviving_mutants += tally.survived;
        let t = node_probe.entry(node.as_str()).or_default();
        t.caught += tally.caught;
        t.survived += tally.survived;
    }
    for v in &tree.style_violations {
        own.entry(v.node_id.as_str()).or_default().style_violations += 1;
    }
    let mut dark: HashMap<&str, Vec<String>> = HashMap::new();
    for f in &tree.dark_files {
        if let Some(node) = tree.file_nodes.get(f) {
            own.entry(node.as_str()).or_default().dark_files += 1;
            dark.entry(node.as_str()).or_default().push(f.clone());
        }
    }

    // Roll up children-first: a node's subtree is its own plus its children's.
    let nodes = &tree.planned.nodes;
    let mut children: HashMap<&str, Vec<&str>> = HashMap::new();
    for n in nodes {
        if let Some(p) = n.parent_id.as_deref() {
            children.entry(p).or_default().push(n.id.as_str());
        }
    }
    fn subtree<'a>(
        id: &'a str,
        own: &HashMap<&str, DerivedCounts>,
        children: &HashMap<&'a str, Vec<&'a str>>,
        memo: &mut HashMap<&'a str, DerivedCounts>,
    ) -> DerivedCounts {
        if let Some(c) = memo.get(id) {
            return c.clone();
        }
        let mut total = own.get(id).cloned().unwrap_or_default();
        for child in children.get(id).into_iter().flatten() {
            total.merge(&subtree(child, own, children, memo));
        }
        memo.insert(id, total.clone());
        total
    }
    let mut memo: HashMap<&str, DerivedCounts> = HashMap::new();
    for n in nodes.iter().filter(|n| n.kind == Kind::Component) {
        if subtree(&n.id, &own, &children, &mut memo).claims == 0 {
            own.entry(n.id.as_str()).or_default().untested_components = 1;
        }
    }
    memo.clear();
    for n in nodes {
        let o = own.get(n.id.as_str()).cloned().unwrap_or_default();
        out.totals.merge(&o);
        out.nodes.insert(
            n.id.clone(),
            NodeStatus {
                subtree: subtree(&n.id, &own, &children, &mut memo),
                own: o,
                probe: node_probe.get(n.id.as_str()).and_then(mark),
                dark_files: dark.remove(n.id.as_str()).unwrap_or_default(),
            },
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::derived_tree::{ClaimLevel, DerivedViolation};
    use crate::domain::verdicts::{ClaimRecord, ProbeRecord};
    use crate::{Node, ScryModel};

    fn node(id: &str, kind: Kind, parent: Option<&str>) -> Node {
        Node {
            id: id.into(),
            kind,
            name: id.into(),
            parent_id: parent.map(Into::into),
            external: None,
            technology: None,
            description: None,
            vagrant: None,
            stale: None,
            responsibilities: Vec::new(),
            properties: Vec::new(),
            icon: None,
            notes: None,
            position: None,
            directives: Vec::new(),
            style: None,
            layer: None,
        }
    }

    fn claim(id: &str, node: &str, planned: bool) -> DerivedClaim {
        DerivedClaim {
            id: id.into(),
            node_id: node.into(),
            level: ClaimLevel::Component,
            file: "src/a.rs".into(),
            line: 1,
            identifier: id.into(),
            description: id.into(),
            concerns: Vec::new(),
            junit_name: id.into(),
            planned,
        }
    }

    fn record(id: &str, outcome: TestOutcome, fp: &str) -> ClaimRecord {
        ClaimRecord {
            resp_id: id.into(),
            outcome,
            cases: 1,
            recorded_at: 100,
            fingerprints: BTreeMap::from([(format!("test:{id}|src/a.rs|{id}"), fp.to_string())]),
            millis: BTreeMap::new(),
        }
    }

    fn tree() -> DerivedTree {
        let mut m = ScryModel::new();
        m.nodes.push(node("sys", Kind::System, None));
        m.nodes.push(node("app", Kind::Container, Some("sys")));
        m.nodes.push(node("core", Kind::Component, Some("app")));
        m.nodes.push(node("bare", Kind::Component, Some("app")));
        DerivedTree {
            fingerprint: 1,
            committed: m.clone(),
            planned: m,
            claims: vec![
                claim("green", "core", false),
                claim("edited", "core", false),
                claim("red", "core", false),
                claim("unrun", "core", false),
                claim("todo", "core", false),
            ]
            .into_iter()
            .map(|mut c| {
                c.planned = c.id == "todo";
                c
            })
            .collect(),
            file_nodes: BTreeMap::from([
                ("src/a.rs".to_string(), "core".to_string()),
                ("src/b.rs".to_string(), "bare".to_string()),
            ]),
            dark_files: vec!["src/b.rs".into()],
            style_violations: vec![DerivedViolation {
                node_id: "bare".into(),
                file: "src/b.rs".into(),
                kind: "layer".into(),
                detail: "x".into(),
            }],
        }
    }

    /// Each state is reachable: a current green verdict is proven, an edited
    /// test or code stale, a red verdict failing, a claim no report covered
    /// stale, a todo stub planned whatever its record says.
    #[test]
    fn every_state_follows_from_report_and_fingerprints() {
        let cache = TestStatusCache {
            results: vec![
                record("green", TestOutcome::Passed, "h1"),
                record("edited", TestOutcome::Passed, "old"),
                record("red", TestOutcome::Failed, "h1"),
                record("todo", TestOutcome::Passed, "h1"),
            ],
            ..Default::default()
        };
        // The working tree hashes every anchor to "h1".
        let mut fresh = |_: &str, fp: &BTreeMap<String, String>, _: u64| fp.values().all(|v| v == "h1");
        let s = compute_derived_status(&tree(), &cache, &MutationCache::default(), &mut fresh);
        let state = |id: &str| s.claims.iter().find(|c| c.resp_id == id).unwrap().state;
        assert_eq!(state("green"), ClaimState::Proven);
        assert_eq!(state("edited"), ClaimState::Stale);
        assert_eq!(state("red"), ClaimState::Failing);
        assert_eq!(state("unrun"), ClaimState::Stale);
        assert_eq!(state("todo"), ClaimState::Planned);
        assert_eq!(s.tests.len(), 4, "the Tests lens lists every recorded verdict");
        assert!(s.tests.iter().find(|t| t.resp_id == "edited").unwrap().stale);

        let core = &s.nodes["core"].own;
        assert_eq!((core.claims, core.proven, core.stale, core.failing, core.planned), (5, 1, 2, 1, 1));
        assert_eq!(s.nodes["bare"].own.untested_components, 1, "code and no claims");
        assert_eq!(s.nodes["core"].own.untested_components, 0);
        assert_eq!(s.nodes["bare"].own.style_violations, 1);
        assert_eq!(s.nodes["bare"].dark_files, vec!["src/b.rs".to_string()]);
        let app = &s.nodes["app"].subtree;
        assert_eq!((app.claims, app.untested_components, app.dark_files), (5, 1, 1));
        assert_eq!(s.totals, s.nodes["sys"].subtree);
    }

    /// When a probe survived, or a mutation report's surviving mutant was
    /// covered by the claim's test, the claim carries a crosshair; when every
    /// break was caught, a double check; a stale probe marks nothing.
    #[test]
    fn probes_and_mutants_mark_the_probed_axis() {
        let probe = |id: &str, survived: u32, fp: &str| ProbeRecord {
            resp_id: id.into(),
            probes: 3,
            survived,
            survivors: Vec::new(),
            recorded_at: 100,
            fingerprints: BTreeMap::from([("k".to_string(), fp.to_string())]),
        };
        let cache = TestStatusCache {
            probes: vec![probe("green", 1, "h1"), probe("red", 0, "h1"), probe("edited", 0, "old")],
            ..Default::default()
        };
        let mutations = MutationCache {
            files: BTreeMap::from([(
                "src/a.rs".to_string(),
                MutantTally { caught: 4, survived: 2, survivors: Vec::new() },
            )]),
            tests: BTreeMap::from([
                ("crate::tests::unrun".to_string(), MutantTally { caught: 2, survived: 0, survivors: Vec::new() }),
                ("todo".to_string(), MutantTally { caught: 1, survived: 1, survivors: Vec::new() }),
            ]),
            ingested_at: 0,
        };
        let mut fresh = |_: &str, fp: &BTreeMap<String, String>, _: u64| fp.values().all(|v| v == "h1");
        let s = compute_derived_status(&tree(), &cache, &mutations, &mut fresh);
        let probe_of = |id: &str| s.claims.iter().find(|c| c.resp_id == id).unwrap().probe;
        assert_eq!(probe_of("green"), Some(ProbeMark::Crosshair));
        assert_eq!(probe_of("red"), Some(ProbeMark::DoubleCheck));
        assert_eq!(probe_of("edited"), None, "a stale probe proves nothing");
        assert_eq!(probe_of("unrun"), Some(ProbeMark::DoubleCheck), "matched by qualified name");
        assert_eq!(probe_of("todo"), Some(ProbeMark::Crosshair));
        assert_eq!(s.nodes["core"].probe, Some(ProbeMark::Crosshair));
        assert_eq!(s.nodes["core"].own.surviving_mutants, 2);
        assert_eq!(s.totals.surviving_mutants, 2);
    }
}
