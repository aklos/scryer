use rmcp::model::{CallToolResult, Content};
use rmcp::ErrorData as McpError;
use scryer_core::history::{append_event, EventRow, HistoryEvent};
use scryer_core::{Kind, ModelLock, ModelRef, Node, Responsibility, ScryModel};
use std::collections::HashMap;

/// Acquire the exclusive model write lock, or return an error result to surface
/// to the agent. Hold the returned guard for the whole read-modify-write of a
/// write tool so concurrent writers (parallel sessions, the canvas) serialize.
pub(crate) fn lock_or_err(model_ref: &ModelRef) -> Result<ModelLock, CallToolResult> {
    scryer_core::lock_model(model_ref)
        .map_err(|e| CallToolResult::error(vec![Content::text(e)]))
}

/// Strip empty values from a JSON tree to keep MCP responses compact.
pub(crate) fn strip_fields_compact(val: &mut serde_json::Value) {
    match val {
        serde_json::Value::Object(map) => {
            map.retain(|_, v| !matches!(v, serde_json::Value::String(s) if s.is_empty()));
            map.retain(|_, v| !v.is_null());
            map.retain(|_, v| !matches!(v, serde_json::Value::Array(a) if a.is_empty()));
            map.retain(|_, v| !matches!(v, serde_json::Value::Object(m) if m.is_empty()));
            for (_, v) in map.iter_mut() {
                strip_fields_compact(v);
            }
        }
        serde_json::Value::Array(arr) => {
            for v in arr.iter_mut() {
                strip_fields_compact(v);
            }
        }
        _ => {}
    }
}

pub(crate) fn parse_kind(s: &str) -> Result<Kind, McpError> {
    match s {
        "person" => Ok(Kind::Person),
        "system" => Ok(Kind::System),
        "container" => Ok(Kind::Container),
        "component" => Ok(Kind::Component),
        // "schema" is the legacy name for a symbol that carries only properties.
        "symbol" | "schema" => Ok(Kind::Symbol),
        _ => Err(McpError::invalid_params(
            format!(
                "Invalid kind '{}'. Must be: person, system, container, component, symbol",
                s
            ),
            None,
        )),
    }
}

pub(crate) fn kind_str(k: &Kind) -> &'static str {
    match k {
        Kind::Person => "person",
        Kind::System => "system",
        Kind::Container => "container",
        Kind::Component => "component",
        Kind::Symbol => "symbol",
    }
}

/// Build a denormalized graph view of a node for MCP responses:
/// adds `childIds`, `incomingLinks`, `outgoingLinks` to the node JSON.
pub(crate) fn denormalize_node(node: &Node, model: &ScryModel) -> serde_json::Value {
    let mut val = serde_json::to_value(node).unwrap_or(serde_json::Value::Null);
    if let serde_json::Value::Object(map) = &mut val {
        let child_ids: Vec<&str> = model
            .nodes
            .iter()
            .filter(|n| n.parent_id.as_deref() == Some(&node.id))
            .map(|n| n.id.as_str())
            .collect();
        let incoming: Vec<&str> = model
            .links
            .iter()
            .filter(|l| l.dst == node.id)
            .map(|l| l.id.as_str())
            .collect();
        let outgoing: Vec<&str> = model
            .links
            .iter()
            .filter(|l| l.src == node.id)
            .map(|l| l.id.as_str())
            .collect();
        map.insert("childIds".to_string(), serde_json::json!(child_ids));
        map.insert("incomingLinks".to_string(), serde_json::json!(incoming));
        map.insert("outgoingLinks".to_string(), serde_json::json!(outgoing));
    }
    val
}

/// Build a compact outline tree of the model: each node carries its id, name,
/// kind, a one-line description, and responsibility/property COUNTS (not
/// bodies), plus its children nested under it. Lets an agent grasp a model's
/// shape without materializing every responsibility, property, link, and
/// source-map entry. Roots are nodes with no parent. When `include_symbols` is
/// false the code level is omitted — the architecture overview (drill into a
/// component with `read_model {node}` to see its symbols).
pub(crate) fn outline_tree(model: &ScryModel, include_symbols: bool) -> Vec<serde_json::Value> {
    let mut children_of: HashMap<Option<&str>, Vec<&Node>> = HashMap::new();
    for n in &model.nodes {
        if !include_symbols && n.kind == Kind::Symbol {
            continue;
        }
        children_of
            .entry(n.parent_id.as_deref())
            .or_default()
            .push(n);
    }

    fn build(
        node: &Node,
        children_of: &HashMap<Option<&str>, Vec<&Node>>,
    ) -> serde_json::Value {
        let kids: Vec<serde_json::Value> = children_of
            .get(&Some(node.id.as_str()))
            .map(|cs| cs.iter().map(|c| build(c, children_of)).collect())
            .unwrap_or_default();
        let mut v = serde_json::json!({
            "id": node.id,
            "name": node.name,
            "kind": kind_str(&node.kind),
            "description": node.description,
            "nResp": node.responsibilities.len(),
            "nProps": node.properties.len(),
            "children": kids,
        });
        strip_fields_compact(&mut v);
        v
    }

    children_of
        .get(&None)
        .map(|roots| roots.iter().map(|r| build(r, &children_of)).collect())
        .unwrap_or_default()
}

/// Breadcrumb path from a root down to `node_id`, by node name, joined with
/// " / ". Used to give search hits a location without the caller re-walking the
/// tree.
pub(crate) fn breadcrumb(model: &ScryModel, node_id: &str) -> String {
    let by_id: HashMap<&str, &Node> = model.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    let mut names = Vec::new();
    let mut cur = by_id.get(node_id).copied();
    let mut guard = 0;
    while let Some(n) = cur {
        names.push(n.name.as_str());
        cur = n.parent_id.as_deref().and_then(|p| by_id.get(p).copied());
        guard += 1;
        if guard > 64 {
            break; // cycle guard
        }
    }
    names.reverse();
    names.join(" / ")
}

/// Directives are user-authored and read-only to the AI's ordinary writes —
/// both a responsibility's `directives` and a node's own node-level
/// `directives`. Before committing any AI write, force each back to whatever
/// the prior on-disk model held for that id; ids with no prior entry get none.
/// This lets the AI create, edit, and move responsibilities and nodes while
/// leaving directives entirely under the user's control. (The interactive
/// patch path can't reach them — they're `schemars(skip)` — but the whole-node
/// generation primitives `set_model`/`set_node` rebuild nodes from JSON and
/// would otherwise drop them.) Not applied to `move_responsibilities`, which
/// preserves directives across a deliberate responsibility-id rename — nor to
/// `set_directives`, the one deliberate write path, reserved for edits the
/// user explicitly asked for.
pub(crate) fn enforce_readonly_directives(model: &mut ScryModel, prior: &ScryModel) {
    let prior_resps = prior
        .nodes
        .iter()
        .flat_map(|n| n.responsibilities.iter())
        .chain(prior.groups.iter().flat_map(|g| g.responsibilities.iter()));
    let prior_dir: HashMap<&str, &Vec<String>> = prior_resps
        .map(|r| (r.id.as_str(), &r.directives))
        .collect();
    let restore = |r: &mut Responsibility| {
        r.directives = prior_dir
            .get(r.id.as_str())
            .map(|d| (*d).clone())
            .unwrap_or_default();
    };
    // Node-level directives, keyed by node id (same read-only guarantee).
    let prior_node_dir: HashMap<&str, &Vec<String>> = prior
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), &n.directives))
        .collect();
    for n in &mut model.nodes {
        n.responsibilities.iter_mut().for_each(&restore);
        n.directives = prior_node_dir
            .get(n.id.as_str())
            .map(|d| (*d).clone())
            .unwrap_or_default();
    }
    for g in &mut model.groups {
        g.responsibilities.iter_mut().for_each(&restore);
    }
}

/// Canvas placements (`Node::position`) are user-authored and read-only to the
/// AI, exactly like directives — the typed patch tools can't reach them
/// (`schemars(skip)`), but the whole-node generation primitives
/// `set_model`/`set_node` rebuild nodes from JSON and would otherwise drop
/// them. Force each node's position back to whatever the FIRST prior model
/// containing that node held; nodes new to every prior get none (the AI never
/// places nodes — auto-layout does). Priors are ordered most-authoritative
/// first: the plan layer is where the canvas writes, so it precedes committed.
pub(crate) fn restore_node_positions(model: &mut ScryModel, priors: &[&ScryModel]) {
    let prior_pos: HashMap<&str, Option<scryer_core::Position>> = priors
        .iter()
        .rev()
        .flat_map(|m| m.nodes.iter())
        .map(|n| (n.id.as_str(), n.position))
        .collect();
    for n in &mut model.nodes {
        n.position = prior_pos.get(n.id.as_str()).copied().flatten();
    }
}


/// Project root from request param, active model, or cwd.
/// Build a committed-model event diff row for a responsibility — its statement,
/// anchored to the first source location the model maps it to (if any).
pub(crate) fn resp_event_row(marker: &str, model: &ScryModel, resp: &Responsibility) -> EventRow {
    let row = EventRow::new(marker, resp.statement.clone());
    match model.source_map.get(&resp.id).and_then(|locs| locs.first()) {
        Some(loc) => row.with_source(loc.clone()),
        None => row,
    }
}

/// Record a committed-model history event, best-effort: a logging failure must
/// never abort the model operation that produced it (see [`scryer_core::history`]).
pub(crate) fn record_event(model_ref: &ModelRef, ev: HistoryEvent) {
    let _ = append_event(model_ref, &ev);
}

pub(crate) fn resolve_model_ref(req_project: Option<&str>) -> Result<ModelRef, McpError> {
    let path = match req_project {
        Some(p) => std::path::PathBuf::from(p),
        None => std::env::current_dir().map_err(|e| {
            McpError::internal_error(format!("cannot read cwd: {}", e), None)
        })?,
    };
    Ok(ModelRef::ProjectLocal(path))
}

/// Error text for a failed model/plan read. A missing file means NO MODEL
/// EXISTS yet — steer at the bootstrap path instead of stranding the agent
/// with a raw "os error 2".
pub(crate) fn read_fail(layer: &str, model_ref: &ModelRef, e: &str) -> String {
    if e.contains("os error 2") || e.contains("No such file") {
        format!(
            "No model exists at {model_ref} yet ({layer} file is absent). Start with \
             `read_codebase` to see the codebase, then build top-down: `add_system` / \
             `add_container` / `fill_container`. If you meant a different project, pass \
             its absolute path as `project`."
        )
    } else {
        format!("Failed to read {layer} at {model_ref}: {e}")
    }
}

/// True when `id` is a server-minted responsibility id (`resp-N`).
fn is_minted_resp_id(id: &str) -> bool {
    id.strip_prefix("resp-").is_some_and(|s| s.parse::<u64>().is_ok())
}

/// Re-mints caller-supplied responsibility ids in raw-write payloads.
///
/// The raw-write tools (`update_nodes`, `update_group`, `set_node`,
/// `set_groups`, `set_model`) accept full `Responsibility` structs, so a
/// caller that doesn't know the next free id invents one ("new", "", "temp")
/// — and every lookup in the system (`find_responsibility`, source_map, fold)
/// keys on that id being globally unique. Three ways a payload id fails that,
/// each re-minted with a reported reason:
///
///  1. **Invented** — unknown to both layers and not minted-format ("new").
///     Echoing an EXISTING claim keeps its identity whatever its id looks like
///     (legacy models carry ids the minters never issued, and renaming one
///     would orphan its source anchors), and a hand-written `resp-N` that
///     collides with nothing stands.
///  2. **Repeated** — the same id twice in one payload. Never legitimate: the
///     second claim would silently overwrite the first.
///  3. **Wrong host** — an id that lives on a DIFFERENT node/group in either
///     layer. This is the stale-snapshot failure: an agent working from an old
///     read picks `resp-712` for a NEW claim, and because that id is
///     minted-format and known, the write used to sail through and hijack the
///     real resp-712's identity — its directives, anchors, attached tests and
///     change tag — while leaving two claims sharing one id. Only the
///     patch-shaped tools guard on host ([`RespIdReminter::new`]); the
///     whole-payload writers ([`RespIdReminter::for_replacement`]) legitimately
///     relocate claims, so for them a host change is a move, not a collision.
///
/// Seed with BOTH layers (plan and committed), for the same union guard the
/// intent tools use: a plan-deleted claim's id must not be re-issued while
/// committed still holds it — and `enforce_readonly_directives` would staple
/// the dead claim's user directives onto the unrelated new one.
pub(crate) struct RespIdReminter {
    next: u64,
    /// Every responsibility id the floor layers already hold → the host ids it
    /// lives on. An incoming id found here is an existing claim being echoed
    /// (never an invention) — and the hosts say whether it is being echoed
    /// where it actually lives.
    known: std::collections::HashMap<String, std::collections::HashSet<String>>,
    /// Ids this payload has already consumed — the within-write duplicate guard.
    used: std::collections::HashSet<String>,
    /// Whether a claim appearing on a host it doesn't live on is a collision
    /// (patch writes) or a legitimate relocation (whole-payload writes).
    guard_host: bool,
    /// One `host: 'old' → new (why)` line per re-mint, for the tool response —
    /// the caller has to learn the real ids it should use from now on, and WHY
    /// its own id was refused.
    minted: Vec<String>,
}

impl RespIdReminter {
    /// For the patch-shaped writers (`update_nodes`, `update_group`,
    /// `set_groups`), where a claim cannot change host: moving one is
    /// `move_responsibilities`' job, so an id arriving on a foreign host is a
    /// collision.
    pub(crate) fn new(floors: &[&ScryModel]) -> Self {
        Self::build(floors, true)
    }

    /// For the whole-payload writers (`set_model`, `set_node`), which restate
    /// an entire model or subtree and may legitimately carry a claim to a new
    /// host. Duplicate and invented ids are still re-minted.
    pub(crate) fn for_replacement(floors: &[&ScryModel]) -> Self {
        Self::build(floors, false)
    }

    fn build(floors: &[&ScryModel], guard_host: bool) -> Self {
        let mut me = Self {
            next: 1,
            known: std::collections::HashMap::new(),
            used: std::collections::HashSet::new(),
            guard_host,
            minted: Vec::new(),
        };
        for m in floors {
            me.absorb(
                m.nodes
                    .iter()
                    .flat_map(|n| n.responsibilities.iter())
                    .chain(m.groups.iter().flat_map(|g| g.responsibilities.iter())),
            );
            let hosted = m
                .nodes
                .iter()
                .flat_map(|n| n.responsibilities.iter().map(move |r| (&n.id, r)))
                .chain(
                    m.groups
                        .iter()
                        .flat_map(|g| g.responsibilities.iter().map(move |r| (&g.id, r))),
                );
            for (host, r) in hosted {
                me.known.entry(r.id.clone()).or_default().insert(host.clone());
            }
        }
        me
    }

    /// Raise the counter past every minted-format id in `resps`. Run this over
    /// the WHOLE payload before the first `remint` call: a payload may carry a
    /// hand-written `resp-N` above the model's own maximum, and minting below
    /// it would collide inside the very write that's being repaired.
    pub(crate) fn absorb<'a>(&mut self, resps: impl Iterator<Item = &'a Responsibility>) {
        let max = resps
            .filter_map(|r| r.id.strip_prefix("resp-").and_then(|s| s.parse::<u64>().ok()))
            .max()
            .unwrap_or(0);
        self.next = self.next.max(max + 1);
    }

    /// Re-mint every id in `resps` that cannot safely stand — invented,
    /// repeated within this payload, or (patch writes only) belonging to a
    /// claim that lives on another host. `host` is the node or group being
    /// written; it names the report line and is what the host guard compares
    /// against.
    pub(crate) fn remint<'a>(
        &mut self,
        host: &str,
        resps: impl Iterator<Item = &'a mut Responsibility>,
    ) {
        for r in resps {
            let homes = self.known.get(&r.id);
            let reason = if self.used.contains(&r.id) {
                Some("repeated in this write")
            } else if homes.is_some_and(|h| self.guard_host && !h.contains(host)) {
                // The stale-snapshot collision: this id is a REAL claim, and it
                // lives somewhere else. Taking it here would hijack that claim.
                Some("that id belongs to a claim on another node")
            } else if homes.is_none() && !is_minted_resp_id(&r.id) {
                Some("ids are server-assigned")
            } else {
                None
            };
            let Some(reason) = reason else {
                self.used.insert(r.id.clone());
                continue;
            };
            let fresh = format!("resp-{}", self.next);
            self.next += 1;
            self.minted.push(format!("{host}: '{}' → {fresh} ({reason})", r.id));
            self.used.insert(fresh.clone());
            r.id = fresh;
        }
    }

    /// Append the re-mint report to a tool response. Silent when nothing was
    /// re-minted — the common case must not grow a no-op section.
    pub(crate) fn report_into(&self, msg: &mut String) {
        if self.minted.is_empty() {
            return;
        }
        msg.push_str(&format!(
            "\n{} caller-supplied responsibility id(s) re-minted (ids are server-assigned; \
             use the new ids from here on):",
            self.minted.len()
        ));
        for line in &self.minted {
            msg.push_str(&format!("\n- {line}"));
        }
    }
}

/// Re-mint payload NODE ids that name a node living outside the region this
/// write replaces — the node-level twin of [`RespIdReminter`]'s wrong-host
/// guard, and the same stale-snapshot failure: an agent reading the model,
/// then writing a subtree minted against that old snapshot, picks `node-41`
/// for something new while `node-41` has since been taken. `replace_subtree`
/// would push it in beside the real one, leaving two nodes sharing an id —
/// after which every id lookup in the system silently picks whichever comes
/// first.
///
/// `replaced` is the id set this write legitimately owns (the subtree being
/// swapped out); anything else live in either layer is taken. Renamed ids are
/// repointed inside the payload — a child's `parentId` and both endpoints of
/// every payload link — so the incoming shape survives the rename intact.
/// Ids repeated WITHIN the payload are re-minted too, but not repointed: which
/// of the twins a reference meant is not knowable.
pub(crate) fn remint_colliding_node_ids(
    nodes: &mut [Node],
    links: &mut [scryer_core::Link],
    replaced: &std::collections::HashSet<String>,
    floors: &[&ScryModel],
) -> Vec<String> {
    use std::collections::{HashMap, HashSet};
    let taken: HashSet<&str> = floors
        .iter()
        .flat_map(|m| m.nodes.iter().map(|n| n.id.as_str()))
        .filter(|id| !replaced.contains(*id))
        .collect();
    // Mint past every id either layer has seen AND every id this payload
    // carries, so a fresh id can't collide with a sibling later in the batch.
    let mut next = floors
        .iter()
        .flat_map(|m| m.nodes.iter().map(|n| n.id.as_str()))
        .chain(nodes.iter().map(|n| n.id.as_str()))
        .filter_map(|id| id.strip_prefix("node-")?.parse::<u64>().ok())
        .max()
        .unwrap_or(0)
        + 1;

    let mut renamed: HashMap<String, String> = HashMap::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut report = Vec::new();
    for n in nodes.iter_mut() {
        let reason = if !seen.insert(n.id.clone()) {
            "repeated in this write"
        } else if taken.contains(n.id.as_str()) {
            "that id belongs to a node outside this subtree"
        } else {
            continue;
        };
        let fresh = format!("node-{next}");
        next += 1;
        report.push(format!("'{}' → {fresh} ({reason})", n.id));
        // Only a first-seen collision can be repointed; a duplicate's
        // references are ambiguous, so they are left pointing at the twin.
        if reason.starts_with("that id") {
            renamed.insert(n.id.clone(), fresh.clone());
        }
        seen.insert(fresh.clone());
        n.id = fresh;
    }
    if !renamed.is_empty() {
        for n in nodes.iter_mut() {
            if let Some(p) = n.parent_id.as_ref().and_then(|p| renamed.get(p)) {
                n.parent_id = Some(p.clone());
            }
        }
        for l in links.iter_mut() {
            if let Some(s) = renamed.get(&l.src) {
                l.src = s.clone();
            }
            if let Some(d) = renamed.get(&l.dst) {
                l.dst = d.clone();
            }
        }
    }
    report
}

/// Write the plan, first tagging what THIS write changed to the session's
/// current change (see `ScryerServer::session_change`) — computed as the diff
/// between the plan on disk and the model being written, so every authoring
/// tool gets attribution without bespoke bookkeeping, deletions included. No
/// current change (or one this plan's registry doesn't know) writes unfiled,
/// exactly as before. Returns conflict warnings for the tool's response: a key
/// already tagged by a DIFFERENT change is two tasks touching the same element
/// — the collision the ledger exists to catch before the code merges.
pub(crate) fn write_planned_tagged(
    model_ref: &ModelRef,
    model: &mut ScryModel,
    change_id: Option<&str>,
) -> Result<Vec<String>, String> {
    let mut warnings = Vec::new();
    if let Some(cid) = change_id {
        if model.changes.iter().any(|c| c.id == cid) {
            let before = scryer_core::read_planned_at(model_ref).unwrap_or_default();
            let keys: Vec<String> = scryer_core::diff::diff(&before, model)
                .changes
                .iter()
                .map(scryer_core::changes::key_for)
                .collect();
            for (key, prev) in scryer_core::changes::tag(model, &keys, cid) {
                let rationale = model
                    .changes
                    .iter()
                    .find(|c| c.id == prev)
                    .map(|c| c.rationale.clone())
                    .unwrap_or_default();
                warnings.push(format!(
                    "conflict: {key} was tagged by {prev} (\"{rationale}\") and is now \
                     retagged to {cid} — two changes are touching the same element"
                ));
            }
        }
    }
    scryer_core::write_planned_at(model_ref, model)?;
    Ok(warnings)
}

/// Plan-diff element count with vagrants excluded — the same queue
/// `get_pending` reports. Lives in `scryer_core::diff` so the desktop app's
/// hook endpoint counts identically; re-exported here for the call sites that
/// have always spelled it this way.
pub(crate) use scryer_core::diff::pending_elements as pending_changes;

pub(crate) fn pending_change_count(committed: &ScryModel, planned: &ScryModel) -> usize {
    pending_changes(committed, planned).len()
}

/// The loop-state counts behind every ambient status line — shared by the MCP
/// response headers and the `status`/`statusline` CLI subcommands.
pub(crate) struct StatusCounts {
    /// The agent's implement queue: one per diverging element (a reworded
    /// claim, an added property…), vagrants excluded — the same list
    /// `get_pending` returns. What the agent-facing header reports.
    pub pending: usize,
    /// Plan-change carriers: element diffs folded under the owning node/group,
    /// the number the Changes page and tree lens show. What the human-facing
    /// status line reports, so it agrees with the canvas.
    pub carriers: usize,
    /// Open changes in the plan's ledger — named workstreams in flight.
    pub open_changes: usize,
    /// Testable claims with no test attached, whole-model (health's
    /// `untested` total) — the primary work signal, kept ambient in the
    /// header so the agent sees it move without re-polling get_health.
    pub untested: u32,
    /// None until a reconcile baseline exists — drift and anchor states have
    /// nothing to measure against, and reporting zeros would fake certainty.
    pub baseline: Option<BaselineCounts>,
    /// None until a test report has been ingested — with no recorded verdicts
    /// there is no test state to report, and zeros would fake certainty.
    pub tests: Option<TestCounts>,
    /// Claims whose last probe round left a survivor: the attached test went
    /// green on a deliberate break. Never a count of the UNPROBED — that
    /// population is nearly everything, and a standing nag for it would be
    /// noise the reader learns to skip. Findings speak; absence does not.
    pub probe_survivors: usize,
}

pub(crate) struct BaselineCounts {
    pub drift_scopes: usize,
    pub anchors_changed: usize,
    pub anchors_broken: usize,
}

/// Recorded test-verdict counts. Verified-green is the norm and stays silent
/// in the headers; `failing` (a current verdict that is red — rare, and an
/// alarm precisely because the loop normally fixes red before it lands) and
/// `stale` (verdicts the code has moved past — the standing signal that
/// feeds `get_test_radius`) are what get spoken.
pub(crate) struct TestCounts {
    pub failing: usize,
    pub stale: usize,
    pub recorded: usize,
}

/// Compute [`StatusCounts`] straight from disk. Best-effort: None when there
/// is no committed model to report on. MUST be called with the model lock
/// RELEASED — the anchor check takes the lock itself (it re-anchors
/// moved-but-unchanged symbols in place).
pub(crate) fn status_counts(model_ref: &ModelRef) -> Option<StatusCounts> {
    let committed = scryer_core::read_model_at(model_ref).ok()?;
    let planned = scryer_core::read_planned_at(model_ref).ok()?;
    let pending = pending_change_count(&committed, &planned);
    let carriers = scryer_core::diff::plan_carrier_count(&committed, &planned);
    let open_changes = planned.changes.len();
    let untested =
        scryer_core::health::compute_health(&committed, Some(&planned), None).totals.untested;
    // Recorded test verdicts, re-verified against the tree (fingerprint
    // compare with an mtime fast path — cheap enough for every response).
    let verdicts = scryer_extract::test_status::test_statuses(model_ref).unwrap_or_default();
    let tests = (!verdicts.is_empty()).then(|| TestCounts {
        failing: verdicts
            .iter()
            .filter(|s| {
                !s.stale
                    && matches!(
                        s.outcome,
                        scryer_core::test_results::TestOutcome::Failed
                            | scryer_core::test_results::TestOutcome::Errored
                    )
            })
            .count(),
        stale: verdicts.iter().filter(|s| s.stale).count(),
        recorded: verdicts.len(),
    });
    // A stale probe result describes code that has since moved on, so it is
    // not a live finding — the next round decides.
    let probe_survivors = scryer_extract::test_status::probe_statuses(model_ref)
        .unwrap_or_default()
        .into_iter()
        .filter(|p| !p.stale && p.survived > 0)
        .count();
    if !model_ref.sync_path().exists() {
        return Some(StatusCounts {
            pending,
            carriers,
            open_changes,
            untested,
            baseline: None,
            tests,
            probe_survivors,
        });
    }
    let sync = scryer_core::read_sync_state(model_ref);
    let scopes =
        scryer_core::drift::drifted_scopes(&committed, model_ref.project_path(), &sync).len();
    let check = scryer_extract::anchors::check_anchors(model_ref).unwrap_or_default();
    let broken = check
        .observations
        .iter()
        .filter(|o| !matches!(o.state, scryer_extract::anchors::AnchorState::Changed))
        .count();
    let changed = check.observations.len() - broken;
    Some(StatusCounts {
        pending,
        carriers,
        open_changes,
        untested,
        baseline: Some(BaselineCounts {
            drift_scopes: scopes,
            anchors_changed: changed,
            anchors_broken: broken,
        }),
        tests,
        probe_survivors,
    })
}

/// The header/statusline fragment for recorded test verdicts — empty unless
/// something is failing or stale. Verified-green is the norm; the line only
/// speaks when there is work: red left behind, or verdicts the code moved
/// past (run `get_test_radius` for exactly what to re-run).
pub(crate) fn tests_phrase(c: &StatusCounts) -> String {
    match &c.tests {
        Some(t) if t.failing > 0 && t.stale > 0 => {
            format!(" · tests: {} failing, {} stale", t.failing, t.stale)
        }
        Some(t) if t.failing > 0 => format!(" · tests: {} failing", t.failing),
        Some(t) if t.stale > 0 => format!(" · tests: {} stale", t.stale),
        _ => String::new(),
    }
}

/// The header fragment for probe findings — empty unless a probe left a
/// survivor. Deliberately asymmetric with `untested`: a claim that has never
/// been probed is the normal state of almost every claim, so counting it here
/// would bury the one thing worth reacting to. A survivor is different — it
/// says a test the model reports as green does not actually hold its claim.
pub(crate) fn probes_phrase(c: &StatusCounts) -> String {
    match c.probe_survivors {
        0 => String::new(),
        1 => " · probes: 1 claim with a surviving break".to_string(),
        n => format!(" · probes: {n} claims with surviving breaks"),
    }
}

/// One-line loop-state header for write responses — `plan: N pending ·
/// untested: N · drift: N scope(s) · anchors: N changed, N broken` — so the
/// model's state stays ambient across a coding session without the agent
/// re-polling the orientation tools. `untested` (testable claims with no test
/// attached) rides directly after the plan count because it is the primary
/// work signal. Same locking contract as [`status_counts`].
pub(crate) fn status_header(model_ref: &ModelRef) -> Option<String> {
    let c = status_counts(model_ref)?;
    // Open changes ride the header only when the ledger is in use — the
    // serial (unfiled) workflow keeps its unchanged line.
    let changes = if c.open_changes > 0 {
        format!(" · changes: {} open", c.open_changes)
    } else {
        String::new()
    };
    // Test verdicts speak only when red or stale — verified-green is silence.
    let tests = tests_phrase(&c);
    // A surviving break is a finding about a test the model calls green.
    let probes = probes_phrase(&c);
    Some(match c.baseline {
        // Never reconciled: drift/anchors have no baseline to report against.
        None => format!(
            "plan: {} pending · untested: {} · drift: no reconcile anchor yet{tests}{probes}{changes}",
            c.pending, c.untested
        ),
        Some(b) => format!(
            "plan: {} pending · untested: {} · drift: {} scope(s) · anchors: {} changed, {} broken{tests}{probes}{changes}",
            c.pending, c.untested, b.drift_scopes, b.anchors_changed, b.anchors_broken
        ),
    })
}

/// Which claim-keyed anchor map a batch of entries writes: implementation
/// anchors (`source_map`) or attached tests (`test_map`).
#[derive(Clone, Copy)]
pub(crate) enum RespAnchorDim {
    Source,
    Test,
}

impl RespAnchorDim {
    fn map_of<'m>(
        &self,
        m: &'m mut ScryModel,
    ) -> &'m mut std::collections::HashMap<String, Vec<scryer_core::SourceLocation>> {
        match self {
            RespAnchorDim::Source => &mut m.source_map,
            RespAnchorDim::Test => &mut m.test_map,
        }
    }
}

/// Apply responsibility anchor entries (the `entries` shape of
/// `update_source_map`) to their SINGLE home: the committed model owns every
/// committed claim's anchor; the planned draft holds anchors only for claims it
/// ADDS. `dim` picks the dimension — implementation anchors or attached tests —
/// which share the routing and normalization verbatim. Whole-symbol line ranges
/// are normalized to symbol-only anchors (the honest encoding for "this whole
/// definition" — for a test entry, "this whole test"), reported in the
/// returned notes. Mutates the models in place; the CALLER validates ids
/// beforehand and persists both layers afterwards (writing `committed` only
/// when the returned flag is true). Shared by `update_source_map` and
/// `mark_implemented`'s fold-time `anchors`/`tests`.
pub(crate) fn apply_resp_anchor_entries(
    project: &std::path::Path,
    planned: &mut ScryModel,
    committed: &mut Option<ScryModel>,
    mut entries: Vec<crate::types::SourceMapEntry>,
    dim: RespAnchorDim,
) -> (Vec<String>, bool) {
    let mut normalized: Vec<String> = Vec::new();
    {
        let mut resolver = scryer_extract::anchors::ExtentResolver::new(project);
        for entry in &mut entries {
            for loc in &mut entry.locations {
                let (Some(sym), Some(line)) = (loc.symbol.clone(), loc.line) else {
                    continue;
                };
                let end = loc.end_line.unwrap_or(line);
                let Some(extent) = resolver.extent(&loc.pattern, &sym, Some(line)) else {
                    continue;
                };
                if scryer_extract::anchors::covers_extent(line, end, extent) {
                    loc.line = None;
                    loc.end_line = None;
                    normalized.push(format!(
                        "{}: {} L{}-{} covered the whole symbol `{}` (L{}-{})",
                        entry.responsibility_id, loc.pattern, line, end, sym, extent.0, extent.1
                    ));
                }
            }
        }
    }

    let committed_resp_ids: std::collections::HashSet<String> = match committed.as_ref() {
        Some(c) => c
            .nodes
            .iter()
            .flat_map(|n| n.responsibilities.iter())
            .chain(c.groups.iter().flat_map(|g| g.responsibilities.iter()))
            .map(|r| r.id.clone())
            .collect(),
        None => Default::default(),
    };
    let mut committed_dirty = false;
    for entry in entries {
        let key = entry.responsibility_id;
        if entry.locations.is_empty() {
            dim.map_of(planned).remove(&key);
            if committed_resp_ids.contains(&key) {
                if let Some(c) = committed.as_mut() {
                    committed_dirty |= dim.map_of(c).remove(&key).is_some();
                }
            }
        } else if committed_resp_ids.contains(&key) {
            dim.map_of(planned).remove(&key);
            if let Some(c) = committed.as_mut() {
                dim.map_of(c).insert(key, entry.locations);
                committed_dirty = true;
            }
        } else {
            dim.map_of(planned).insert(key, entry.locations);
        }
    }
    (normalized, committed_dirty)
}
