//! Observability rollup — how healthy is the model as a LENS over the code?
//!
//! Everything here is deterministic and derived; nothing is stored. The model's
//! two failure modes are blur (stale/missing anchors) and darkness (code the
//! model never describes); this module turns both into numbers the UI and
//! agents can act on:
//!
//! - **Discharge is computed, not declared.** A responsibility hosted on a
//!   structural node (a node with children, or a group) is discharged through
//!   its subtree — it is *never* expected to carry its own source anchor. Only
//!   leaf-hosted claims (a childless node saying "this code exists") are
//!   anchorable, and only those can be "unmapped". This kills the false
//!   "unmapped" flag on System/Container responsibilities. Persons (actors) and
//!   external systems are out-of-system — their claims are never code-backed, so
//!   they are never anchorable either.
//! - **Coverage rolls up.** Every node reports its own counts and its subtree's
//!   counts (statuses, vagrant flags, anchorable vs anchored claims, the most
//!   recent truth-bearing edit), so any altitude can answer "how much of what I
//!   claim reads through to code?".
//! - **Darkness is per boundary.** Given the project's modelable source files,
//!   each boundary-owning node reports which of its files no anchor in its
//!   subtree reaches — the code the lens cannot see.

use crate::{Kind, ScryModel, Status};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

/// Counts of responsibility/property statuses. `vagrant` is a separate axis (a
/// flag on top of a status), counted independently.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusCounts {
    pub proposed: u32,
    pub implemented: u32,
    pub verified: u32,
    pub changed: u32,
}

impl StatusCounts {
    fn add(&mut self, status: Option<Status>) {
        match status.unwrap_or(Status::Proposed) {
            Status::Proposed => self.proposed += 1,
            Status::Implemented => self.implemented += 1,
            Status::Verified => self.verified += 1,
            Status::Changed => self.changed += 1,
        }
    }

    fn merge(&mut self, other: &StatusCounts) {
        self.proposed += other.proposed;
        self.implemented += other.implemented;
        self.verified += other.verified;
        self.changed += other.changed;
    }
}

/// Health counters over one scope — a node's own content, or a whole subtree.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthCounts {
    /// Responsibility statements in scope (nodes' own + groups parented here).
    pub responsibilities: u32,
    /// Data-shape properties in scope.
    pub properties: u32,
    pub statuses: StatusCounts,
    /// Responsibilities carrying the vagrant flag (undescribed behaviour
    /// awaiting adopt/reject).
    pub vagrant: u32,
    /// Responsibilities carrying the stale flag (the drift check judged the
    /// code no longer discharges them; awaiting a verdict).
    pub stale: u32,
    /// Claims that are EXPECTED to read through to code: implemented/verified/
    /// changed content hosted on a leaf (childless, non-external) node. A claim
    /// on a structural node is discharged through the subtree instead and never
    /// counts here.
    pub anchorable: u32,
    /// Of the anchorable claims, how many actually have a source anchor.
    pub anchored: u32,
    /// `anchorable - anchored` — blind spots: the lens claims code it cannot
    /// show.
    pub unmapped: u32,
    /// Unix seconds of the most recent truth-bearing edit in scope.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_touched_at: Option<u64>,
}

impl HealthCounts {
    fn touch(&mut self, at: Option<u64>) {
        if let Some(t) = at {
            self.last_touched_at = Some(self.last_touched_at.map_or(t, |c| c.max(t)));
        }
    }

    fn merge(&mut self, other: &HealthCounts) {
        self.responsibilities += other.responsibilities;
        self.properties += other.properties;
        self.statuses.merge(&other.statuses);
        self.vagrant += other.vagrant;
        self.stale += other.stale;
        self.anchorable += other.anchorable;
        self.anchored += other.anchored;
        self.unmapped += other.unmapped;
        self.touch(other.last_touched_at);
    }
}

/// How much of a node's owned code region the lens actually reaches.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BoundaryCoverage {
    /// Modelable source files matching this node's boundary globs.
    pub total_files: u32,
    /// Of those, how many some anchor in this node's subtree reads into.
    pub anchored_files: u32,
    /// The files no anchor reaches — code the lens cannot see. Sorted.
    pub dark_files: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeHealth {
    /// The node's own responsibilities/properties only.
    pub own: HealthCounts,
    /// The node plus everything below it (descendant nodes and groups).
    pub subtree: HealthCounts,
    /// Present only for boundary-owning nodes when a file inventory was given.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub boundary: Option<BoundaryCoverage>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelHealth {
    /// Per-node health, keyed by node id (stable order for serialization).
    pub nodes: BTreeMap<String, NodeHealth>,
    /// Whole-model rollup (every node and group, wherever parented).
    pub totals: HealthCounts,
}

/// Compute the model's health. `files` is the project's modelable source-file
/// inventory (project-relative, as produced by the extractor); pass `None` to
/// skip boundary coverage (pure model math only).
pub fn compute_health(model: &ScryModel, files: Option<&BTreeSet<String>>) -> ModelHealth {
    let children = children_index(model);
    let node_by_id: HashMap<&str, &crate::Node> =
        model.nodes.iter().map(|n| (n.id.as_str(), n)).collect();

    // --- own counts per node ---------------------------------------------------
    let mut own: HashMap<&str, HealthCounts> = HashMap::new();
    for node in &model.nodes {
        let is_leaf = children
            .get(node.id.as_str())
            .map_or(true, |c| c.is_empty());
        let external = node.external == Some(true);
        // Persons are actors and externals are out-of-system; neither is backed
        // by code in this model, so their claims are never anchorable (and so
        // never "unmapped").
        let anchorable_node = is_leaf && !external && node.kind != Kind::Person;
        let mut h = HealthCounts::default();

        for resp in &node.responsibilities {
            h.responsibilities += 1;
            h.statuses.add(resp.status);
            if resp.vagrant == Some(true) {
                h.vagrant += 1;
            }
            if resp.stale == Some(true) {
                h.stale += 1;
            }
            h.touch(resp.last_touched_at);
            if anchorable_node && claims_code(resp.status) {
                h.anchorable += 1;
                if model
                    .source_map
                    .get(&resp.id)
                    .is_some_and(|locs| !locs.is_empty())
                {
                    h.anchored += 1;
                } else {
                    h.unmapped += 1;
                }
            }
        }

        // A data shape is one declaration: if any property claims code, the
        // shape's definition must anchor — one claim for the whole node.
        let mut shape_claims = false;
        for prop in &node.properties {
            h.properties += 1;
            h.statuses.add(prop.status);
            h.touch(prop.last_touched_at);
            shape_claims = shape_claims || claims_code(prop.status);
        }
        if anchorable_node && shape_claims {
            h.anchorable += 1;
            if model
                .source_map
                .get(&node.id)
                .is_some_and(|locs| !locs.is_empty())
            {
                h.anchored += 1;
            } else {
                h.unmapped += 1;
            }
        }

        own.insert(node.id.as_str(), h);
    }

    // --- group responsibilities attach to the level they organize ---------------
    // A group's responsibilities are always structural (discharged by members),
    // so they roll into the parent node's subtree counts and never anchor.
    let mut group_extra: HashMap<&str, HealthCounts> = HashMap::new();
    let mut unparented_groups = HealthCounts::default();
    for group in &model.groups {
        let mut h = HealthCounts::default();
        for resp in &group.responsibilities {
            h.responsibilities += 1;
            h.statuses.add(resp.status);
            if resp.vagrant == Some(true) {
                h.vagrant += 1;
            }
            if resp.stale == Some(true) {
                h.stale += 1;
            }
            h.touch(resp.last_touched_at);
        }
        if h == HealthCounts::default() {
            continue;
        }
        let parent = group
            .parent_node_id
            .as_deref()
            .or_else(|| {
                group
                    .member_ids
                    .first()
                    .and_then(|m| node_by_id.get(m.as_str()))
                    .and_then(|n| n.parent_id.as_deref())
            })
            .filter(|p| node_by_id.contains_key(p));
        match parent {
            Some(p) => group_extra.entry(p).or_default().merge(&h),
            None => unparented_groups.merge(&h),
        }
    }

    // --- subtree rollup (post-order) --------------------------------------------
    let mut subtree: HashMap<&str, HealthCounts> = HashMap::new();
    let roots: Vec<&str> = model
        .nodes
        .iter()
        .filter(|n| {
            n.parent_id
                .as_deref()
                .map_or(true, |p| !node_by_id.contains_key(p))
        })
        .map(|n| n.id.as_str())
        .collect();
    let mut visited: HashSet<&str> = HashSet::new();
    for root in &roots {
        accumulate(root, &children, &own, &group_extra, &mut subtree, &mut visited);
    }
    // Defensive: nodes trapped in a parent cycle never get visited above.
    for node in &model.nodes {
        if !visited.contains(node.id.as_str()) {
            accumulate(
                node.id.as_str(),
                &children,
                &own,
                &group_extra,
                &mut subtree,
                &mut visited,
            );
        }
    }

    let mut totals = unparented_groups;
    for root in &roots {
        if let Some(h) = subtree.get(root) {
            totals.merge(h);
        }
    }

    // --- boundary coverage -------------------------------------------------------
    let anchored_files = files.map(|_| anchored_files_per_subtree(model, &children));

    let mut nodes: BTreeMap<String, NodeHealth> = BTreeMap::new();
    for node in &model.nodes {
        let boundary = match (files, &anchored_files) {
            (Some(files), Some(anchored)) => {
                boundary_coverage(model, &node.id, files, anchored.get(node.id.as_str()))
            }
            _ => None,
        };
        nodes.insert(
            node.id.clone(),
            NodeHealth {
                own: own.get(node.id.as_str()).cloned().unwrap_or_default(),
                subtree: subtree.get(node.id.as_str()).cloned().unwrap_or_default(),
                boundary,
            },
        );
    }

    ModelHealth { nodes, totals }
}

/// Does this status claim that code exists? Those are the claims that must
/// read through to source on a leaf.
fn claims_code(status: Option<Status>) -> bool {
    matches!(
        status,
        Some(Status::Implemented) | Some(Status::Verified) | Some(Status::Changed)
    )
}

fn children_index(model: &ScryModel) -> HashMap<&str, Vec<&str>> {
    let ids: HashSet<&str> = model.nodes.iter().map(|n| n.id.as_str()).collect();
    let mut idx: HashMap<&str, Vec<&str>> = HashMap::new();
    for node in &model.nodes {
        if let Some(parent) = node.parent_id.as_deref() {
            if ids.contains(parent) {
                idx.entry(parent).or_default().push(node.id.as_str());
            }
        }
    }
    idx
}

fn accumulate<'a>(
    id: &'a str,
    children: &HashMap<&'a str, Vec<&'a str>>,
    own: &HashMap<&'a str, HealthCounts>,
    group_extra: &HashMap<&'a str, HealthCounts>,
    subtree: &mut HashMap<&'a str, HealthCounts>,
    visited: &mut HashSet<&'a str>,
) {
    if !visited.insert(id) {
        return;
    }
    let mut h = own.get(id).cloned().unwrap_or_default();
    if let Some(extra) = group_extra.get(id) {
        h.merge(extra);
    }
    if let Some(kids) = children.get(id) {
        for kid in kids {
            accumulate(kid, children, own, group_extra, subtree, visited);
            if let Some(kh) = subtree.get(kid) {
                let kh = kh.clone();
                h.merge(&kh);
            }
        }
    }
    subtree.insert(id, h);
}

/// For every node: the set of files some anchor in its SUBTREE reads into —
/// source_map entries keyed by the subtree's responsibility ids or node ids.
fn anchored_files_per_subtree<'a>(
    model: &'a ScryModel,
    children: &HashMap<&'a str, Vec<&'a str>>,
) -> HashMap<&'a str, BTreeSet<&'a str>> {
    // Own anchored files per node: its definition entry + its responsibilities'.
    let mut own: HashMap<&str, BTreeSet<&str>> = HashMap::new();
    for node in &model.nodes {
        let mut set: BTreeSet<&str> = BTreeSet::new();
        if let Some(locs) = model.source_map.get(&node.id) {
            set.extend(locs.iter().map(|l| l.pattern.as_str()));
        }
        for resp in &node.responsibilities {
            if let Some(locs) = model.source_map.get(&resp.id) {
                set.extend(locs.iter().map(|l| l.pattern.as_str()));
            }
        }
        own.insert(node.id.as_str(), set);
    }

    let mut out: HashMap<&str, BTreeSet<&str>> = HashMap::new();
    let mut visited: HashSet<&str> = HashSet::new();
    fn walk<'a>(
        id: &'a str,
        children: &HashMap<&'a str, Vec<&'a str>>,
        own: &HashMap<&'a str, BTreeSet<&'a str>>,
        out: &mut HashMap<&'a str, BTreeSet<&'a str>>,
        visited: &mut HashSet<&'a str>,
    ) {
        if !visited.insert(id) {
            return;
        }
        let mut set = own.get(id).cloned().unwrap_or_default();
        if let Some(kids) = children.get(id) {
            for kid in kids {
                walk(kid, children, own, out, visited);
                if let Some(ks) = out.get(kid) {
                    let ks = ks.clone();
                    set.extend(ks);
                }
            }
        }
        out.insert(id, set);
    }
    for node in &model.nodes {
        walk(node.id.as_str(), children, &own, &mut out, &mut visited);
    }
    out
}

fn boundary_coverage(
    model: &ScryModel,
    node_id: &str,
    files: &BTreeSet<String>,
    anchored: Option<&BTreeSet<&str>>,
) -> Option<BoundaryCoverage> {
    let sources = model.boundaries.get(node_id)?;
    let patterns: Vec<glob::Pattern> = sources
        .iter()
        .filter_map(|s| glob::Pattern::new(&s.pattern).ok())
        .collect();
    if patterns.is_empty() {
        return None;
    }
    let mut total = 0u32;
    let mut hit = 0u32;
    let mut dark: Vec<String> = Vec::new();
    for file in files {
        if !patterns.iter().any(|p| p.matches(file)) {
            continue;
        }
        total += 1;
        if anchored.is_some_and(|a| a.contains(file.as_str())) {
            hit += 1;
        } else {
            dark.push(file.clone());
        }
    }
    Some(BoundaryCoverage {
        total_files: total,
        anchored_files: hit,
        dark_files: dark,
    })
}

/// Convenience for callers that need the leaf test outside `compute_health`
/// (e.g. UI affordances): is this node structural (discharges through children)?
pub fn is_structural(model: &ScryModel, node_id: &str) -> bool {
    model
        .nodes
        .iter()
        .any(|n| n.parent_id.as_deref() == Some(node_id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Kind, Node, Responsibility, SourceLocation};

    fn node(id: &str, kind: Kind, parent: Option<&str>) -> Node {
        Node {
            id: id.into(),
            kind,
            name: id.into(),
            parent_id: parent.map(Into::into),
            external: None,
            technology: None,
            description: None,
            responsibilities: Vec::new(),
            properties: Vec::new(),
            icon: None,
            visual: None,
            appearance: None,
            deprecated: None,
            relocated: None,
            locked: None,
            relocated_to: None,
            relocated_from: None,
        }
    }

    fn resp(id: &str, status: Status) -> Responsibility {
        Responsibility {
            id: id.into(),
            statement: format!("does {id}"),
            status: Some(status),
            vagrant: None,
            stale: None,
            locked: None,
            relocated_to: None,
            relocated_from: None,
            directives: Vec::new(),
            last_touched_at: Some(100),
            changed_from: None,
        }
    }

    fn loc(file: &str) -> SourceLocation {
        SourceLocation {
            pattern: file.into(),
            symbol: Some("f".into()),
            line: Some(10),
            end_line: Some(20),
            command: None,
        }
    }

    /// The discharge rule: a System's implemented responsibility with no anchor
    /// is NOT unmapped — its children discharge it. The same claim on a leaf IS.
    #[test]
    fn structural_responsibilities_are_never_unmapped() {
        let mut m = ScryModel::new();
        let mut sys = node("sys", Kind::System, None);
        sys.responsibilities.push(resp("r-sys", Status::Implemented));
        m.nodes.push(sys);
        let mut leaf = node("leaf", Kind::Symbol, Some("sys"));
        leaf.responsibilities.push(resp("r-leaf", Status::Implemented));
        m.nodes.push(leaf);

        let h = compute_health(&m, None);
        let sys_h = &h.nodes["sys"];
        assert_eq!(sys_h.own.anchorable, 0, "system claims discharge structurally");
        assert_eq!(sys_h.own.unmapped, 0);
        // The leaf's unanchored claim is the real blind spot, and it rolls up.
        assert_eq!(h.nodes["leaf"].own.unmapped, 1);
        assert_eq!(sys_h.subtree.unmapped, 1);
        assert_eq!(sys_h.subtree.responsibilities, 2);
    }

    /// A person is an actor, not code. Its implemented responsibilities are
    /// never anchorable, so they never count as unmapped.
    #[test]
    fn person_responsibilities_are_never_unmapped() {
        let mut m = ScryModel::new();
        let mut dev = node("dev", Kind::Person, None);
        dev.responsibilities.push(resp("r-1", Status::Implemented));
        dev.responsibilities.push(resp("r-2", Status::Implemented));
        m.nodes.push(dev);

        let h = compute_health(&m, None);
        let dev_h = &h.nodes["dev"];
        assert_eq!(dev_h.own.anchorable, 0, "a person's claims are not code-backed");
        assert_eq!(dev_h.own.unmapped, 0);
        assert_eq!(dev_h.own.responsibilities, 2);
    }

    /// Anchored leaf claims count as coverage; proposed claims aren't expected
    /// to anchor at all.
    #[test]
    fn leaf_coverage_counts_anchors_and_ignores_proposed() {
        let mut m = ScryModel::new();
        m.nodes.push(node("c", Kind::Component, None));
        let mut a = node("a", Kind::Symbol, Some("c"));
        a.responsibilities.push(resp("r-a", Status::Implemented));
        m.nodes.push(a);
        let mut b = node("b", Kind::Symbol, Some("c"));
        b.responsibilities.push(resp("r-b", Status::Proposed));
        m.nodes.push(b);
        m.source_map.insert("r-a".into(), vec![loc("src/a.ts")]);

        let h = compute_health(&m, None);
        let c = &h.nodes["c"];
        assert_eq!(c.subtree.anchorable, 1, "only the implemented leaf claim");
        assert_eq!(c.subtree.anchored, 1);
        assert_eq!(c.subtree.unmapped, 0);
        assert_eq!(c.subtree.statuses.proposed, 1);
    }

    /// A data shape (leaf with implemented properties) is one anchorable claim,
    /// anchored by the node's own definition entry.
    #[test]
    fn data_shape_is_one_claim() {
        let mut m = ScryModel::new();
        let mut shape = node("shape", Kind::Symbol, None);
        shape.properties.push(crate::SchemaProperty {
            label: "field".into(),
            description: String::new(),
            status: Some(Status::Implemented),
            last_touched_at: Some(50),
        });
        m.nodes.push(shape);

        let h = compute_health(&m, None);
        assert_eq!(h.nodes["shape"].own.anchorable, 1);
        assert_eq!(h.nodes["shape"].own.unmapped, 1, "no definition anchor yet");

        m.source_map.insert("shape".into(), vec![loc("src/types.ts")]);
        let h = compute_health(&m, None);
        assert_eq!(h.nodes["shape"].own.unmapped, 0);
    }

    /// Boundary coverage: files in a node's boundary that no subtree anchor
    /// reaches are dark.
    #[test]
    fn boundary_dark_files() {
        let mut m = ScryModel::new();
        m.nodes.push(node("api", Kind::Container, None));
        let mut s = node("s", Kind::Symbol, Some("api"));
        s.responsibilities.push(resp("r-s", Status::Implemented));
        m.nodes.push(s);
        m.source_map.insert("r-s".into(), vec![loc("api/src/handler.rs")]);
        m.boundaries.insert(
            "api".into(),
            vec![crate::Source { pattern: "api/**/*".into(), comment: None }],
        );

        let files: BTreeSet<String> = ["api/src/handler.rs", "api/src/dark.rs", "web/app.ts"]
            .into_iter()
            .map(String::from)
            .collect();
        let h = compute_health(&m, Some(&files));
        let b = h.nodes["api"].boundary.as_ref().expect("boundary coverage");
        assert_eq!(b.total_files, 2, "web/app.ts is outside the boundary");
        assert_eq!(b.anchored_files, 1);
        assert_eq!(b.dark_files, vec!["api/src/dark.rs".to_string()]);
    }

    /// Group responsibilities roll into the level they organize, never anchor.
    #[test]
    fn group_responsibilities_roll_into_parent_subtree() {
        let mut m = ScryModel::new();
        m.nodes.push(node("sys", Kind::System, None));
        m.nodes.push(node("c1", Kind::Container, Some("sys")));
        m.groups.push(crate::Group {
            id: "g1".into(),
            name: "Edge".into(),
            description: None,
            member_ids: vec!["c1".into()],
            parent_group_id: None,
            parent_node_id: None,
            responsibilities: vec![resp("r-g", Status::Implemented)],
            icon: None,
        });

        let h = compute_health(&m, None);
        assert_eq!(h.nodes["sys"].subtree.responsibilities, 1);
        assert_eq!(h.nodes["sys"].subtree.anchorable, 0, "group claims are structural");
        assert_eq!(h.totals.responsibilities, 1);
    }

    #[test]
    fn vagrant_and_freshness_roll_up() {
        let mut m = ScryModel::new();
        m.nodes.push(node("sys", Kind::System, None));
        let mut leaf = node("leaf", Kind::Symbol, Some("sys"));
        let mut r = resp("r1", Status::Implemented);
        r.vagrant = Some(true);
        r.last_touched_at = Some(999);
        leaf.responsibilities.push(r);
        m.nodes.push(leaf);
        m.source_map.insert("r1".into(), vec![loc("src/x.ts")]);

        let h = compute_health(&m, None);
        assert_eq!(h.nodes["sys"].subtree.vagrant, 1);
        assert_eq!(h.nodes["sys"].subtree.last_touched_at, Some(999));
        assert_eq!(h.totals.vagrant, 1);
    }
}
