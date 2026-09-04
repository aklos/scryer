//! The fold's two gates — what `mark_implemented` refuses to commit, and why.
//!
//! 1. **Sign-off** (forward vagrancy): a claim the agent reworded, moved, or
//!    added AFTER the developer signed off its change is a proposal, not
//!    intent. It is flagged `vagrant` with a `vagrant_origin` and the approved
//!    text, left in the plan, and reported as awaiting the developer's verdict.
//!    A signed-off claim the agent dropped is restored as pending intent.
//! 2. **Evidence**: a testable (When/While/If) claim on a code-backed host
//!    folds only with a test attached AND a current passing verdict
//!    (`scryer_extract::test_status::claim_evidence`). Otherwise it stays in the
//!    plan and the response names the missing fact and the test files to run.
//!    `force` bypasses this gate visibly (an `unverified` history event).
//!
//! Both gates return a WITHHOLD set the fold engine honours
//! (`commit_element_withholding`), so the rest of the fold proceeds — leaving
//! a claim pending is a legitimate, honest exit, never a loop.

use scryer_core::changes::{self, Classification};
use scryer_core::diff::{self, ElementKind as EK};
use scryer_core::refusals::Refusal;
use scryer_core::{ears, Kind, ModelRef, Responsibility, ScryModel};
use scryer_extract::test_status::{claim_evidence, Evidence};
use std::collections::{BTreeSet, HashMap, HashSet};

/// What the gates decided for one fold call.
#[derive(Debug, Default)]
pub(crate) struct GateOutcome {
    /// Claims the fold must leave in the plan.
    pub withhold: HashSet<String>,
    /// The ledger entries to record for them.
    pub refusals: Vec<Refusal>,
    /// Response lines, in order.
    pub lines: Vec<String>,
    /// Claims that failed the evidence gate but fold anyway under `force`.
    pub forced: Vec<String>,
    /// Whether the gates wrote to the plan (vagrant flags set, dropped claims
    /// restored) and the caller must persist it before folding.
    pub plan_dirty: bool,
}

/// The claims PENDING on `node_id` — the ones a whole-node fold would actually
/// change in committed — excluding vagrants (they never fold) and claims tagged
/// to a different change than the node (another task's work, which the fold
/// engine leaves behind on its own).
pub(crate) fn pending_claims_on(
    committed: &ScryModel,
    planned: &ScryModel,
    node_id: &str,
) -> Vec<String> {
    let host_key = changes::element_key(EK::Node, None, node_id);
    let vagrant: HashSet<&str> = planned
        .nodes
        .iter()
        .flat_map(|n| n.responsibilities.iter())
        .filter(|r| r.vagrant == Some(true))
        .map(|r| r.id.as_str())
        .collect();
    diff::diff(committed, planned)
        .changes
        .iter()
        .filter(|ch| {
            ch.kind == EK::Responsibility
                && ch.owner_id.as_deref() == Some(node_id)
                && !ch.changes.contains(&diff::Change::Deleted)
                && !vagrant.contains(ch.id.as_str())
                && !changes::foreign_to_host(
                    &planned.change_map,
                    &host_key,
                    &changes::element_key(EK::Responsibility, None, &ch.id),
                )
        })
        .map(|ch| ch.id.clone())
        .collect()
}

fn find_resp_mut<'a>(
    model: &'a mut ScryModel,
    id: &str,
) -> Option<(String, &'a mut Responsibility)> {
    for n in &mut model.nodes {
        if let Some(r) = n.responsibilities.iter_mut().find(|r| r.id == id) {
            return Some((n.id.clone(), r));
        }
    }
    for g in &mut model.groups {
        if let Some(r) = g.responsibilities.iter_mut().find(|r| r.id == id) {
            return Some((g.id.clone(), r));
        }
    }
    None
}

fn find_resp<'a>(model: &'a ScryModel, id: &str) -> Option<(&'a str, &'a Responsibility)> {
    model
        .nodes
        .iter()
        .flat_map(|n| n.responsibilities.iter().map(move |r| (n.id.as_str(), r)))
        .chain(
            model
                .groups
                .iter()
                .flat_map(|g| g.responsibilities.iter().map(move |r| (g.id.as_str(), r))),
        )
        .find(|(_, r)| r.id == id)
}

/// Whether a host expects tests at all: a person or an external system never
/// does; a group's claims are discharged by its members (ungated, like
/// structural nodes' own claims are advisory — see the plan). Everything else
/// is code-backed.
fn code_backed_host(model: &ScryModel, host_id: &str) -> bool {
    match model.nodes.iter().find(|n| n.id == host_id) {
        Some(n) => n.kind != Kind::Person && n.external != Some(true),
        None => false, // a group
    }
}

/// Run both gates over `candidates` (the claims this fold is about to commit).
/// `tests_in_call` maps claim id → test files attached in the SAME call: an
/// attachment with no verdict yet still refuses (the verdict comes from a run
/// + ingest, which must precede the fold), but the refusal names those files.
pub(crate) fn gate(
    model_ref: &ModelRef,
    planned: &mut ScryModel,
    candidates: &[String],
    tests_in_call: &HashMap<String, Vec<String>>,
    force: bool,
    now: u64,
) -> Result<GateOutcome, String> {
    let mut out = GateOutcome::default();

    // ---- 1. Sign-off: amendments and additions stay behind as vagrant. -----
    let mut involved: BTreeSet<String> = BTreeSet::new();
    for id in candidates {
        let key = changes::element_key(EK::Responsibility, None, id);
        if let Some(cid) = planned.change_map.get(&key) {
            involved.insert(cid.clone());
        }
        let Some((cid, class, snap)) = changes::classify_key(planned, &key) else { continue };
        let Some(origin) = class.origin() else { continue };
        let Some((host, r)) = find_resp_mut(planned, id) else { continue };
        let approved = snap.as_ref().and_then(|s| s.statement.clone());
        r.vagrant = Some(true);
        r.vagrant_origin = Some(origin.to_string());
        r.approved_statement = approved.clone();
        let reason = match class {
            Classification::Amended => format!(
                "reworded after sign-off of {cid} (approved: \"{}\")",
                approved.as_deref().unwrap_or("?")
            ),
            _ => format!("added after sign-off of {cid}"),
        };
        out.lines.push(format!(
            "AWAITING VERDICT {id} (stays in the plan, flagged vagrant/{origin}): {reason} — the \
             developer adopts, rejects, or rewords it from Needs Review; it does not fold"
        ));
        out.refusals.push(Refusal {
            resp_id: id.clone(),
            host_id: host,
            kind: origin.to_string(),
            reason,
            run: Vec::new(),
            at: now,
        });
        out.withhold.insert(id.clone());
        out.plan_dirty = true;
    }

    // Dropped signed-off claims come back as the original intent.
    for cid in &involved {
        let Some(meta) = planned.changes.iter().find(|c| &c.id == cid).cloned() else { continue };
        for (key, class, snap) in changes::classify_against_signoff(planned, &meta) {
            if class != Classification::Dropped {
                continue;
            }
            let Some((EK::Responsibility, _, rid)) = changes::parse_key(&key) else { continue };
            let Some(snap) = snap else { continue };
            // Folded, not dropped: the element stands in the plan exactly as
            // approved and only lost its tag because an earlier fold carried
            // it into committed. Nothing to restore.
            if changes::entry_hash(planned, &key).is_some_and(|now| now.hash == snap.hash) {
                continue;
            }
            let (Some(stmt), Some(host)) = (snap.statement.clone(), snap.host.clone()) else {
                continue;
            };
            let restored = match find_resp_mut(planned, &rid) {
                // Reverted in place (the tag was GC'd): put the approved text back.
                Some((_, r)) => {
                    r.statement = stmt.clone();
                    true
                }
                // Gone: re-insert on its approved host, if that host still exists.
                None => match planned.nodes.iter_mut().find(|n| n.id == host) {
                    Some(n) => {
                        n.responsibilities.push(Responsibility {
                            id: rid.clone(),
                            statement: stmt.clone(),
                            concern: None,
                            vagrant: None,
                            vagrant_origin: None,
                            approved_statement: None,
                            stale: None,
                            stale_proposal: None,
                            directives: Vec::new(),
                            last_touched_at: Some(now),
                        });
                        true
                    }
                    None => false,
                },
            };
            if restored {
                planned.change_map.insert(key.clone(), cid.clone());
                // Restored means PENDING: this fold must not carry it across.
                out.withhold.insert(rid.clone());
                out.plan_dirty = true;
                out.lines.push(format!(
                    "RESTORED {rid} as pending intent (\"{stmt}\") — it was signed off in {cid} \
                     and the plan no longer carried it; the agent's proposal to drop it needs \
                     the developer's verdict, so it stays in the queue"
                ));
            } else {
                out.lines.push(format!(
                    "DROPPED {rid} (\"{stmt}\") was signed off in {cid} and is gone from the plan \
                     along with its host — the developer should know the agent dropped it"
                ));
            }
        }
    }

    // ---- 2. Evidence: testable claims need a current passing verdict. --------
    let mut gated: Vec<String> = Vec::new();
    for id in candidates {
        if out.withhold.contains(id) {
            continue;
        }
        let Some((host, r)) = find_resp(planned, id) else { continue };
        if !code_backed_host(planned, host) {
            continue;
        }
        if !ears::classify(&r.statement).testable() {
            continue;
        }
        gated.push(id.clone());
    }
    if gated.is_empty() {
        return Ok(out);
    }
    let evidence = claim_evidence(model_ref, &gated)?;
    for id in &gated {
        let mut ev = evidence.get(id).cloned().unwrap_or(Evidence::NoTest);
        if let (Evidence::NoTest, Some(files)) = (&ev, tests_in_call.get(id)) {
            ev = Evidence::NoVerdict { tests: files.clone() };
        }
        if ev.verified() {
            continue;
        }
        let host = find_resp(planned, id).map(|(h, _)| h.to_string()).unwrap_or_default();
        let kind = match &ev {
            Evidence::NoTest => "no-test",
            Evidence::NoVerdict { .. } => "no-verdict",
            Evidence::Stale { .. } => "stale",
            Evidence::Failing { .. } => "failing",
            Evidence::Verified => unreachable!(),
        };
        let reason = ev.reason();
        if force {
            out.forced.push(id.clone());
            out.lines.push(format!(
                "UNVERIFIED {id} folded under force: {reason} — recorded in history; the claim \
                 reads as committed but unproven"
            ));
            continue;
        }
        let fix = match &ev {
            Evidence::NoTest => {
                " — write the test the statement already specifies, attach it (update_source_map \
                 `test_entries`), run it with the JUnit reporter on, ingest_test_report, then fold \
                 again"
            }
            _ => ", ingest_test_report the report, then fold again",
        };
        out.lines.push(format!("REFUSED {id} (stays in the plan): {reason}{fix}"));
        out.refusals.push(Refusal {
            resp_id: id.clone(),
            host_id: host,
            kind: kind.to_string(),
            reason,
            run: ev.tests().to_vec(),
            at: now,
        });
        out.withhold.insert(id.clone());
    }
    Ok(out)
}

/// The baseline keys a fold should refresh for the claims it landed: each
/// folded claim's implementation key and its `test:` key.
pub(crate) fn baseline_keys(folded: &[String]) -> BTreeSet<String> {
    folded
        .iter()
        .flat_map(|id| [id.clone(), scryer_core::test_key(id)])
        .collect()
}
