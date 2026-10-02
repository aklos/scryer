//! The test-verdict cache on disk and the model reads behind it: fingerprints
//! computed from real files, freshness against mtimes, and every read/write of
//! `.scryer/.test-results.json`.

use crate::domain::anchors::{is_glob_pattern, resolve_span, span_hash};
use crate::domain::test_status::*;
use crate::infrastructure::anchors::FileCache;
use scryer_core::test_results::{match_report, parse_junit, ReportMatch, TestOutcome};
use scryer_core::test_key;
use scryer_core::{read_model_at, read_planned_at, working_view, ModelRef, ScryModel};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// The model every status read resolves against: the committed model with the
/// PLAN's own claims and anchors overlaid (`working_view`). A claim that has not
/// folded yet lives only in the plan — so do the test it attached and, often,
/// its implementation anchor — and the verdict-gated fold needs its verdict to
/// record and read BEFORE it folds. Committed alone would make every unfolded
/// claim invisible here, and the gate could never be satisfied.
fn working_model(r: &ModelRef) -> Result<ScryModel, String> {
    let committed = read_model_at(r)?;
    match read_planned_at(r) {
        Ok(planned) => Ok(working_view(&committed, &planned)),
        Err(_) => Ok(committed),
    }
}

fn read_cache(r: &ModelRef) -> TestStatusCache {
    std::fs::read_to_string(r.test_results_path())
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn write_cache(r: &ModelRef, cache: &TestStatusCache) -> Result<(), String> {
    let json = serde_json::to_string(cache).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(r.dir()).map_err(|e| e.to_string())?;
    std::fs::write(r.test_results_path(), json).map_err(|e| e.to_string())
}

/// Fingerprint every anchor behind one claim — implementation locations
/// under the bare key, attached tests under `test:{id}` — against the working
/// tree as it stands. Unresolvable locations (missing file, gone symbol)
/// simply contribute nothing: their absence makes the map differ from any
/// record taken when they resolved, which is exactly the stale signal.
fn claim_fingerprints(
    model: &ScryModel,
    resp_id: &str,
    project: &Path,
    cache: &mut FileCache,
    project_files: &mut Option<BTreeSet<String>>,
) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let dims = [
        (resp_id.to_string(), model.source_map.get(resp_id)),
        (test_key(resp_id), model.test_map.get(resp_id)),
    ];
    for (key, locs) in dims {
        for loc in locs.into_iter().flatten() {
            let mut fingerprint = |file: &str, from_glob: bool| {
                let Some((source, parse)) = cache.get(project, file) else {
                    return;
                };
                let lines: Vec<&str> = source.lines().collect();
                let (line, end_line) = if from_glob { (None, None) } else { (loc.line, loc.end_line) };
                let Ok((start, end)) = resolve_span(
                    source,
                    parse.as_ref(),
                    loc.symbol.as_deref(),
                    line,
                    line,
                    end_line,
                ) else {
                    return;
                };
                out.insert(
                    format!("{key}|{file}|{}", loc.symbol.as_deref().unwrap_or("")),
                    span_hash(&lines, start, end),
                );
            };
            if is_glob_pattern(&loc.pattern) {
                let Ok(pattern) = glob::Pattern::new(&loc.pattern) else {
                    continue;
                };
                let files =
                    project_files.get_or_insert_with(|| crate::list_project_files(project));
                for file in files.iter().filter(|f| pattern.matches(f)) {
                    fingerprint(file, true);
                }
            } else {
                fingerprint(&loc.pattern, false);
            }
        }
    }
    out
}

/// Merge one matched report's per-claim outcomes into the cache. Each claim's
/// anchors are fingerprinted against the working tree as it stands — the
/// record means "this outcome was true of THIS code". A claim already in the
/// cache is replaced: the cache holds the latest word, not a history.
pub fn store_test_results(r: &ModelRef, report: &ReportMatch) -> Result<usize, String> {
    if report.claims.is_empty() {
        return Ok(0);
    }
    store_results_on(r, &working_model(r)?, report)
}

/// [`store_test_results`] fingerprinted against `model` — the working model,
/// or one with attachments about to be written.
fn store_results_on(r: &ModelRef, model: &ScryModel, report: &ReportMatch) -> Result<usize, String> {
    if report.claims.is_empty() {
        return Ok(0);
    }
    let model = model.clone();
    let project = r.project_path();
    let mut cache = read_cache(r);
    let mut files = FileCache::new();
    let mut project_files: Option<BTreeSet<String>> = None;
    let recorded_at = now_secs();

    let mut claims: Vec<(&String, &scryer_core::test_results::ClaimOutcome)> =
        report.claims.iter().collect();
    claims.sort_by_key(|(id, _)| id.as_str());
    for (resp_id, verdict) in &claims {
        let fingerprints =
            claim_fingerprints(&model, resp_id, project, &mut files, &mut project_files);
        let record = ClaimRecord {
            resp_id: (*resp_id).clone(),
            outcome: verdict.outcome,
            cases: verdict.cases,
            recorded_at,
            fingerprints,
            millis: verdict.millis.clone(),
        };
        match cache.results.iter_mut().find(|c| &&c.resp_id == resp_id) {
            Some(existing) => *existing = record,
            None => cache.results.push(record),
        }
    }
    write_cache(r, &cache)?;
    Ok(claims.len())
}

/// Cheap freshness check: `Some(true)` when the record is provably still
/// fresh WITHOUT parsing anything — the claim's anchor locations spell
/// exactly the keys the record fingerprinted, and none of their files
/// changed since the record was taken (a stat, not a parse, is the
/// ambient-frequency cost). `None` means "can't tell cheaply" — a glob to
/// expand, a key-set difference, a touched or missing file — and the caller
/// must re-hash. Never answers `Some(false)`: only the hashes may declare
/// staleness.
fn provably_fresh(model: &ScryModel, rec: &ClaimRecord, project: &Path) -> Option<bool> {
    let recorded_at =
        std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(rec.recorded_at);
    let mut loc_keys: BTreeSet<String> = BTreeSet::new();
    let mut files: BTreeSet<&str> = BTreeSet::new();
    let dims = [
        (rec.resp_id.clone(), model.source_map.get(&rec.resp_id)),
        (test_key(&rec.resp_id), model.test_map.get(&rec.resp_id)),
    ];
    for (key, locs) in dims {
        for loc in locs.into_iter().flatten() {
            if is_glob_pattern(&loc.pattern) {
                return None; // expansion needs the project walk — not cheap
            }
            loc_keys.insert(format!(
                "{key}|{}|{}",
                loc.pattern,
                loc.symbol.as_deref().unwrap_or("")
            ));
            files.insert(&loc.pattern);
        }
    }
    // Every fingerprint must come from a spelled location and vice versa —
    // an attachment added since the record (even into an untouched file)
    // breaks the correspondence and must go through the full re-hash.
    if !rec.fingerprints.keys().all(|k| loc_keys.contains(k)) {
        return None;
    }
    if loc_keys.iter().any(|k| !rec.fingerprints.contains_key(k)) {
        return None;
    }
    for file in files {
        let mtime = std::fs::metadata(project.join(file)).and_then(|m| m.modified()).ok()?;
        if mtime >= recorded_at {
            return None; // touched since the record — the hashes decide
        }
    }
    Some(true)
}

/// Read every cached verdict, re-verified against the working tree: the same
/// anchors are re-resolved and re-hashed, and ANY difference from the record
/// — content changed, an anchor now unresolvable, an attachment added or
/// removed since — reads as stale. A record with no fingerprints at all is
/// stale by construction. Claims whose anchor files are provably untouched
/// since the record skip the re-hash entirely ([`provably_fresh`]), so this
/// is cheap enough to ride every response. Claims that have left the model
/// are omitted, not reported as ghosts.
pub fn read_test_statuses(r: &ModelRef) -> Result<Vec<ClaimTestStatus>, String> {
    statuses_for(r, None)
}

/// [`read_test_statuses`] for `only` these claims (all when `None`): the
/// staleness check re-fingerprints each claim's code, so a caller asking about
/// a handful must not pay for the whole model.
fn statuses_for(r: &ModelRef, only: Option<&BTreeSet<&str>>) -> Result<Vec<ClaimTestStatus>, String> {
    let cache = read_cache(r);
    if cache.results.is_empty() {
        return Ok(Vec::new());
    }
    let model = working_model(r)?;
    let project = r.project_path();
    let live: BTreeSet<&str> = model
        .nodes
        .iter()
        .flat_map(|n| n.responsibilities.iter())
        .chain(model.groups.iter().flat_map(|g| g.responsibilities.iter()))
        .map(|resp| resp.id.as_str())
        .collect();
    let mut files = FileCache::new();
    let mut project_files: Option<BTreeSet<String>> = None;
    let mut out = Vec::new();
    for rec in &cache.results {
        if !live.contains(rec.resp_id.as_str())
            || only.is_some_and(|o| !o.contains(rec.resp_id.as_str()))
        {
            continue;
        }
        let stale = if rec.fingerprints.is_empty() {
            true
        } else if provably_fresh(&model, rec, project) == Some(true) {
            false
        } else {
            claim_fingerprints(&model, &rec.resp_id, project, &mut files, &mut project_files)
                != rec.fingerprints
        };
        out.push(ClaimTestStatus {
            resp_id: rec.resp_id.clone(),
            outcome: rec.outcome,
            cases: rec.cases,
            stale,
            recorded_at: rec.recorded_at,
        });
    }
    out.sort_by(|a, b| a.resp_id.cmp(&b.resp_id));
    Ok(out)
}

/// The evidence behind each named claim, resolved against the working view
/// (plan-layer attachments count — see [`working_model`]). Reads the cached
/// verdicts once for the whole set. Unknown ids read as `NoTest`.
pub fn evidence_for_claim(
    r: &ModelRef,
    resp_ids: &[String],
) -> Result<BTreeMap<String, Evidence>, String> {
    if resp_ids.is_empty() {
        return Ok(BTreeMap::new());
    }
    let model = working_model(r)?;
    let only: BTreeSet<&str> = resp_ids.iter().map(String::as_str).collect();
    let verdicts = statuses_for(r, Some(&only))?;
    let mut out = BTreeMap::new();
    for id in resp_ids {
        let tests: Vec<String> = model
            .test_map
            .get(id)
            .map(|locs| {
                let mut files: Vec<String> = locs.iter().map(|l| l.pattern.clone()).collect();
                files.sort();
                files.dedup();
                files
            })
            .unwrap_or_default();
        let ev = if tests.is_empty() {
            Evidence::NoTest
        } else {
            match verdicts.iter().find(|s| &s.resp_id == id) {
                None => Evidence::NoVerdict { tests },
                Some(s) if s.stale => Evidence::Stale { tests },
                Some(s) if s.outcome != TestOutcome::Passed => {
                    Evidence::Failing { outcome: s.outcome, tests }
                }
                Some(_) => Evidence::Verified,
            }
        };
        out.insert(id.clone(), ev);
    }
    Ok(out)
}

/// Exactly which attached test files need re-running: every test-attached
/// claim whose verdict is missing or stale contributes its test files;
/// claims whose verdict is current contribute nothing. Grouped per file so
/// each entry is one targeted invocation — the radius is what needs
/// re-running, never the whole suite. (A claim with NO attached test never
/// appears here — that gap is health's `untested`, not a radius entry.)
pub fn compute_blast_radius(r: &ModelRef) -> Result<Vec<RadiusFile>, String> {
    let model = working_model(r)?;
    let verdicts = read_test_statuses(r)?;
    let stale_of: BTreeMap<&str, bool> =
        verdicts.iter().map(|s| (s.resp_id.as_str(), s.stale)).collect();
    let live: BTreeSet<&str> = model
        .nodes
        .iter()
        .flat_map(|n| n.responsibilities.iter())
        .chain(model.groups.iter().flat_map(|g| g.responsibilities.iter()))
        .map(|resp| resp.id.as_str())
        .collect();
    let mut by_file: BTreeMap<&str, RadiusFile> = BTreeMap::new();
    for (resp_id, locs) in &model.test_map {
        if !live.contains(resp_id.as_str()) {
            continue;
        }
        let stale = match stale_of.get(resp_id.as_str()) {
            Some(false) => continue, // current verdict — nothing to re-run
            Some(true) => true,
            None => false, // never recorded
        };
        for loc in locs {
            let entry = by_file.entry(&loc.pattern).or_insert_with(|| RadiusFile {
                pattern: loc.pattern.clone(),
                claims: Vec::new(),
                stale: 0,
            });
            if !entry.claims.contains(resp_id) {
                entry.claims.push(resp_id.clone());
                entry.stale += stale as usize;
            }
        }
    }
    let mut out: Vec<RadiusFile> = by_file.into_values().collect();
    for f in &mut out {
        f.claims.sort();
    }
    Ok(out)
}

/// Whether any of `resp_id`'s anchors — implementation or attached test —
/// lies in one of `touched`.
fn claim_touched(model: &ScryModel, resp_id: &str, touched: &[String]) -> bool {
    [model.source_map.get(resp_id), model.test_map.get(resp_id)]
        .into_iter()
        .flatten()
        .flatten()
        .any(|loc| {
            if is_glob_pattern(&loc.pattern) {
                glob::Pattern::new(&loc.pattern)
                    .is_ok_and(|p| touched.iter().any(|f| p.matches(f)))
            } else {
                touched.iter().any(|f| f == &loc.pattern)
            }
        })
}

/// The radius for one session: claims with missing or stale verdicts whose
/// anchored code or tests `touched` reaches, and the tests to run for them.
/// A stale verdict on a claim the session never touched is listed apart as
/// someone else's. With no session (`None`) every claim counts and nothing
/// is touched, so every slow test is left out of the run.
pub fn compute_session_radius(
    r: &ModelRef,
    touched: Option<&[String]>,
) -> Result<SessionRadius, String> {
    let model = working_model(r)?;
    let verdicts = read_test_statuses(r)?;
    let stale_of: BTreeMap<&str, bool> =
        verdicts.iter().map(|s| (s.resp_id.as_str(), s.stale)).collect();
    let cache = read_cache(r);
    let live: BTreeSet<&str> = model
        .nodes
        .iter()
        .flat_map(|n| n.responsibilities.iter())
        .chain(model.groups.iter().flat_map(|g| g.responsibilities.iter()))
        .map(|resp| resp.id.as_str())
        .collect();
    let mut out = SessionRadius { scoped: touched.is_some(), ..Default::default() };
    let mut by_file: BTreeMap<&str, RadiusFile> = BTreeMap::new();
    let mut tests: BTreeMap<(&str, Option<&str>), RadiusTest> = BTreeMap::new();
    let mut touched_claims: BTreeSet<&str> = BTreeSet::new();
    for (resp_id, locs) in &model.test_map {
        if !live.contains(resp_id.as_str()) {
            continue;
        }
        let stale = match stale_of.get(resp_id.as_str()) {
            Some(false) => continue,
            Some(true) => true,
            None => false,
        };
        if let Some(t) = touched {
            if !claim_touched(&model, resp_id, t) {
                if stale {
                    out.not_yours.push(resp_id.clone());
                }
                continue;
            }
            touched_claims.insert(resp_id);
        }
        for loc in locs {
            let entry = by_file.entry(&loc.pattern).or_insert_with(|| RadiusFile {
                pattern: loc.pattern.clone(),
                claims: Vec::new(),
                stale: 0,
            });
            if !entry.claims.contains(resp_id) {
                entry.claims.push(resp_id.clone());
                entry.stale += stale as usize;
            }
            let test = tests
                .entry((&loc.pattern, loc.symbol.as_deref()))
                .or_insert_with(|| RadiusTest {
                    pattern: loc.pattern.clone(),
                    name: loc.symbol.clone(),
                    claims: Vec::new(),
                    millis: None,
                });
            if !test.claims.contains(resp_id) {
                test.claims.push(resp_id.clone());
            }
        }
    }
    // A test's duration is the slowest any verdict recorded for its name.
    for test in tests.values_mut() {
        let Some(name) = &test.name else { continue };
        test.millis = cache.results.iter().filter_map(|rec| rec.millis.get(name).copied()).max();
    }
    for test in tests.into_values() {
        let slow = test.millis.is_some_and(|ms| ms > SLOW_TEST_MILLIS)
            && !test.claims.iter().any(|c| touched_claims.contains(c.as_str()));
        if slow {
            out.slow.push(test);
        } else {
            out.run.push(test);
        }
    }
    out.files = by_file.into_values().collect();
    for f in &mut out.files {
        f.claims.sort();
    }
    Ok(out)
}

/// Where the project's radius test command is stored: one template line,
/// set once (see [`render_test_command`]).
fn test_command_path(r: &ModelRef) -> std::path::PathBuf {
    r.dir().join("test-command")
}

pub fn read_test_command(r: &ModelRef) -> Option<String> {
    let raw = std::fs::read_to_string(test_command_path(r)).ok()?;
    let t = raw.trim();
    (!t.is_empty()).then(|| t.to_string())
}

pub fn write_test_command(r: &ModelRef, template: &str) -> Result<(), String> {
    std::fs::create_dir_all(r.dir()).map_err(|e| e.to_string())?;
    std::fs::write(test_command_path(r), format!("{}\n", template.trim())).map_err(|e| e.to_string())
}

/// The directory radius runs write their JUnit reports into, created with
/// its own `.gitignore` so reports never reach a commit. Project-relative.
pub fn ensure_reports_dir(r: &ModelRef) -> Result<String, String> {
    let dir = r.dir().join("reports");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let ignore = dir.join(".gitignore");
    if !ignore.exists() {
        std::fs::write(ignore, "*\n").map_err(|e| e.to_string())?;
    }
    Ok(".scryer/reports".to_string())
}

/// The Stop hook's probe check for a session. Its claims under test are the
/// ones whose attached tests sit in a file the session touched and whose
/// verdict is current and passing — tests it wrote or changed, already green.
/// Until [`probe_budget`] of them hold a probe result on the current code,
/// `block` names which to probe, ranked by [`rank_probe_candidates`].
/// `summary` names any of them whose probe let a break survive.
pub fn session_probe_check(r: &ModelRef, touched: &[String]) -> ProbeCheck {
    if touched.is_empty() {
        return ProbeCheck::default();
    }
    let Ok(model) = working_model(r) else {
        return ProbeCheck::default();
    };
    let in_touched_test = |resp_id: &str| {
        model.test_map.get(resp_id).is_some_and(|locs| {
            locs.iter().any(|l| touched.iter().any(|f| f == &l.pattern))
        })
    };
    // A probe breaks the claim's code: a claim anchored to none has nothing
    // to break, so it is never asked for.
    let has_code = |resp_id: &str| model.source_map.get(resp_id).is_some_and(|locs| !locs.is_empty());
    let verdicts = read_test_statuses(r).unwrap_or_default();
    let eligible: Vec<&str> = verdicts
        .iter()
        .filter(|s| {
            !s.stale && s.outcome == TestOutcome::Passed && in_touched_test(&s.resp_id) && has_code(&s.resp_id)
        })
        .map(|s| s.resp_id.as_str())
        .collect();
    if eligible.is_empty() {
        return ProbeCheck::default();
    }
    let all_probes: Vec<ClaimProbeStatus> = read_probe_statuses(r)
        .unwrap_or_default()
        .into_iter()
        .filter(|p| eligible.contains(&p.resp_id.as_str()))
        .collect();
    let probes: Vec<&ClaimProbeStatus> = all_probes.iter().filter(|p| !p.stale).collect();

    let survivors: Vec<String> = probes
        .iter()
        .filter(|p| p.survived > 0)
        .map(|p| match p.survivors.first() {
            Some(s) => format!("{} ({s})", p.resp_id),
            None => p.resp_id.clone(),
        })
        .collect();
    let summary = (!survivors.is_empty()).then(|| {
        format!(
            "probe: {} claim(s) whose test let a break survive — {}",
            survivors.len(),
            survivors.join("; ")
        )
    });

    let needed = probe_budget(eligible.len());
    let block = (probes.len() < needed).then(|| {
        // The touched test files behind each claim, and how many qualifying
        // claims each of those files backs.
        let tests_of = |id: &str| -> Vec<String> {
            let mut files: Vec<String> = model
                .test_map
                .get(id)
                .into_iter()
                .flatten()
                .filter(|l| touched.iter().any(|f| f == &l.pattern))
                .map(|l| l.pattern.clone())
                .collect();
            files.sort();
            files.dedup();
            files
        };
        let mut backs: BTreeMap<String, usize> = BTreeMap::new();
        for id in &eligible {
            for t in tests_of(id) {
                *backs.entry(t).or_default() += 1;
            }
        }
        let candidates = eligible
            .iter()
            .filter(|id| !probes.iter().any(|p| p.resp_id == **id))
            .map(|id| {
                let urgency = match all_probes.iter().find(|p| p.resp_id == *id) {
                    Some(p) if p.survived > 0 => ProbeUrgency::SurvivorRecheck,
                    Some(_) => ProbeUrgency::Changed,
                    None => ProbeUrgency::Unprobed,
                };
                let tests = tests_of(id).into_iter().map(|t| { let n = backs[&t]; (t, n) }).collect();
                ProbeCandidate { resp_id: id.to_string(), urgency, tests }
            })
            .collect();
        let todo: Vec<String> =
            rank_probe_candidates(candidates).into_iter().take(needed - probes.len()).collect();
        format!(
            "Scryer probe check — this session wrote or changed tests behind {} green claim(s), \
             and {} of them must be probed before you stop. Probe {}: hand each to a subagent on \
             a cheap model with open_probe {{resp_id}} → up to 3 breaks in the probe worktree → \
             close_probe {{probes, survivors}}. A survivor goes in the user's summary; strengthen \
             that test. This check fires once per session.",
            eligible.len(),
            needed,
            todo.join(", "),
        )
    });
    ProbeCheck { block, summary }
}

/// The one-call entry point: JUnit XML in, per-claim outcomes recorded,
/// match summary out — unmatched, ambiguous, and unseen included, so the
/// caller can surface what the report did NOT settle alongside what it did.
pub fn ingest_report_file(r: &ModelRef, xml: &str) -> Result<IngestSummary, String> {
    let model = working_model(r)?;
    let cases = parse_junit(xml)?;
    let report = match_report(&model.test_map, &cases);
    let recorded = store_test_results(r, &report)?;
    keep_cases(r, &cases)?;
    Ok(IngestSummary { cases: cases.len(), recorded, report })
}

/// Keep every case of an ingested report, attached or not — the latest run
/// per (classname, name) — for tests attached later.
fn keep_cases(r: &ModelRef, cases: &[scryer_core::test_results::TestCase]) -> Result<(), String> {
    let mut cache = read_cache(r);
    let ingested_ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or_default();
    for case in cases {
        let kept = KeptCase { case: case.clone(), ingested_ns };
        match cache
            .cases
            .iter_mut()
            .find(|k| k.case.classname == case.classname && k.case.name == case.name)
        {
            Some(existing) => *existing = kept,
            None => cache.cases.push(kept),
        }
    }
    write_cache(r, &cache)
}

/// Tests just attached (`attached`: claim → test locations) whose report was
/// ingested before they were: record each claim's verdict from the kept cases,
/// as if the report had been ingested now — but only when none of the claim's
/// code or test files changed since that ingest, so an old run never vouches
/// for new code. Returns the claims recorded.
pub fn replay_kept_cases(
    r: &ModelRef,
    attached: &BTreeMap<String, Vec<scryer_core::SourceLocation>>,
) -> Result<Vec<String>, String> {
    let cache = read_cache(r);
    if cache.cases.is_empty() || attached.is_empty() {
        return Ok(Vec::new());
    }
    let mut model = working_model(r)?;
    for (id, locs) in attached {
        model.test_map.insert(id.clone(), locs.clone());
    }
    let cases: Vec<_> = cache.cases.iter().map(|k| k.case.clone()).collect();
    let mut report = match_report(attached, &cases);
    let project = r.project_path();
    let modified_after = |file: &str, ns: u64| {
        std::fs::metadata(project.join(file))
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .is_none_or(|t| t.as_nanos() as u64 > ns)
    };
    report.claims.retain(|id, _| {
        // The oldest ingest among the cases this claim's tests matched.
        let leaves: Vec<String> = attached[id]
            .iter()
            .filter_map(|l| l.symbol.as_deref())
            .map(scryer_core::test_results::normalize_leaf)
            .collect();
        let Some(ingested_ns) = cache
            .cases
            .iter()
            .filter(|k| leaves.contains(&scryer_core::test_results::normalize_leaf(&k.case.name)))
            .map(|k| k.ingested_ns)
            .min()
        else {
            return false;
        };
        let files = model
            .source_map
            .get(id)
            .into_iter()
            .flatten()
            .chain(attached[id].iter())
            .map(|l| l.pattern.as_str());
        !files.into_iter().any(|f| modified_after(f, ingested_ns))
    });
    let mut recorded: Vec<String> = report.claims.keys().cloned().collect();
    recorded.sort();
    store_results_on(r, &model, &report)?;
    Ok(recorded)
}

/// Resolve one claim into a probe target, or explain why it can't be probed.
///
/// The refusals are the point. Breaking code behind a claim with no attached
/// test proves nothing — there is no assertion to fail. Breaking code behind a
/// test whose verdict is missing or stale proves nothing either: a probe reads
/// "the test went red", which only means something when you already know it
/// was green.
pub fn resolve_probe_target(r: &ModelRef, resp_id: &str) -> Result<ProbeTarget, String> {
    let model = working_model(r)?;
    let statement = model
        .nodes
        .iter()
        .flat_map(|n| n.responsibilities.iter())
        .chain(model.groups.iter().flat_map(|g| g.responsibilities.iter()))
        .find(|resp| resp.id == resp_id)
        .map(|resp| resp.statement.clone())
        .ok_or_else(|| format!("no claim {resp_id} in the model"))?;

    let attachments = model
        .test_map
        .get(resp_id)
        .filter(|locs| !locs.is_empty())
        .ok_or_else(|| {
            format!(
                "{resp_id} has no attached test — a probe asks whether the test would \
                 catch the break, so there must be one to ask about. Attach it first \
                 (update_source_map test_entries), then probe."
            )
        })?;

    match read_test_statuses(r)?.into_iter().find(|s| s.resp_id == resp_id) {
        None => {
            return Err(format!(
                "{resp_id} has no recorded verdict — run its tests and ingest_test_report \
                 first. A probe means 'the test went red on a break', which says nothing \
                 unless it was green to begin with."
            ))
        }
        Some(status) if status.stale => {
            return Err(format!(
                "{resp_id}'s verdict is stale — the code or test changed since it was \
                 recorded. Re-run and ingest before probing."
            ))
        }
        Some(status) if status.outcome != TestOutcome::Passed => {
            return Err(format!(
                "{resp_id}'s test is not passing ({:?}) — fix it before probing. A probe \
                 cannot tell a break it caught from one it was already failing on.",
                status.outcome
            ))
        }
        Some(_) => {}
    }

    let project = r.project_path();
    let mut files = FileCache::new();
    let loc = model
        .source_map
        .get(resp_id)
        .into_iter()
        .flatten()
        .find(|loc| !is_glob_pattern(&loc.pattern))
        .ok_or_else(|| {
            format!("{resp_id} has no file-level source anchor to break — nothing to probe")
        })?;
    let (source, parse) = files
        .get(project, &loc.pattern)
        .ok_or_else(|| format!("{} is not readable", loc.pattern))?;
    let (start_line, end_line) = resolve_span(
        source,
        parse.as_ref(),
        loc.symbol.as_deref(),
        loc.line,
        loc.line,
        loc.end_line,
    )
    .map_err(|()| {
        format!(
            "{resp_id}'s anchor no longer resolves in {} — re-anchor it before probing",
            loc.pattern
        )
    })?;

    let mut tests = Vec::new();
    for t in attachments {
        if let Some(sym) = &t.symbol {
            let entry = format!("{} :: {sym}", t.pattern);
            if !tests.contains(&entry) {
                tests.push(entry);
            }
        } else if !tests.contains(&t.pattern) {
            tests.push(t.pattern.clone());
        }
    }

    Ok(ProbeTarget {
        resp_id: resp_id.to_string(),
        statement,
        file: loc.pattern.clone(),
        start_line,
        end_line,
        symbol: loc.symbol.clone(),
        tests,
    })
}

/// Record what a finished round of probes found. Fingerprinted against the
/// same anchors a verdict uses, so an edit to the implementation or the test
/// ages the probe result exactly as it ages the verdict — a probe proves
/// something about the code that was there, never about code that came after.
pub fn store_probe_result(
    r: &ModelRef,
    resp_id: &str,
    probes: u32,
    survivors: Vec<String>,
) -> Result<(), String> {
    let model = working_model(r)?;
    let project = r.project_path();
    let mut files = FileCache::new();
    let mut project_files: Option<BTreeSet<String>> = None;
    let record = ProbeRecord {
        resp_id: resp_id.to_string(),
        probes,
        survived: survivors.len() as u32,
        survivors,
        recorded_at: now_secs(),
        fingerprints: claim_fingerprints(&model, resp_id, project, &mut files, &mut project_files),
    };
    let mut cache = read_cache(r);
    match cache.probes.iter_mut().find(|p| p.resp_id == resp_id) {
        Some(existing) => *existing = record,
        None => cache.probes.push(record),
    }
    write_cache(r, &cache)
}

/// Every cached probe result, re-verified against the working tree the same
/// way verdicts are. A claim absent from this list has never been probed —
/// which is NOT the same as one probed clean, and callers must not render it
/// that way.
pub fn read_probe_statuses(r: &ModelRef) -> Result<Vec<ClaimProbeStatus>, String> {
    let cache = read_cache(r);
    if cache.probes.is_empty() {
        return Ok(Vec::new());
    }
    let model = working_model(r)?;
    let project = r.project_path();
    let live: BTreeSet<&str> = model
        .nodes
        .iter()
        .flat_map(|n| n.responsibilities.iter())
        .chain(model.groups.iter().flat_map(|g| g.responsibilities.iter()))
        .map(|resp| resp.id.as_str())
        .collect();
    let mut files = FileCache::new();
    let mut project_files: Option<BTreeSet<String>> = None;
    let mut out = Vec::new();
    for rec in &cache.probes {
        if !live.contains(rec.resp_id.as_str()) {
            continue;
        }
        let stale = rec.fingerprints.is_empty()
            || claim_fingerprints(&model, &rec.resp_id, project, &mut files, &mut project_files)
                != rec.fingerprints;
        out.push(ClaimProbeStatus {
            resp_id: rec.resp_id.clone(),
            probes: rec.probes,
            survived: rec.survived,
            survivors: rec.survivors.clone(),
            stale,
            recorded_at: rec.recorded_at,
        });
    }
    out.sort_by(|a, b| a.resp_id.cmp(&b.resp_id));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use scryer_core::{Kind, Node, Responsibility, SourceLocation};

    const IMPL_TS: &str = "export function alpha() {\n    return 1;\n}\n";
    const SPEC_TS: &str = "describe(\"alpha\", () => {\n  it(\"answers one\", () => {\n    expect(alpha()).toBe(1);\n  });\n});\n";
    const REPORT: &str = r#"<testsuites><testsuite name="s">
        <testcase classname="src/m.spec.ts" name="alpha &gt; answers one"/>
    </testsuite></testsuites>"#;

    fn project() -> (tempfile::TempDir, ModelRef) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/m.ts"), IMPL_TS).unwrap();
        std::fs::write(dir.path().join("src/m.spec.ts"), SPEC_TS).unwrap();
        let r = ModelRef::ProjectLocal(dir.path().to_path_buf());
        let mut m = ScryModel::new();
        m.nodes.push(Node {
            style: None,
            layer: None,
            id: "sym".into(),
            kind: Kind::Symbol,
            name: "alpha".into(),
            vagrant: None,
            stale: None,
            parent_id: None,
            external: None,
            technology: None,
            description: None,
            responsibilities: vec![Responsibility {
                concern: None,
                id: "r1".into(),
                statement: "answers one".into(),
                vagrant: None,
                stale: None,
                stale_proposal: None,
                directives: Vec::new(),
                last_touched_at: None,
            }],
            properties: Vec::new(),
            icon: None,
            notes: None,
            position: None,
            directives: Vec::new(),
        });
        m.source_map.insert(
            "r1".into(),
            vec![SourceLocation {
                pattern: "src/m.ts".into(),
                symbol: Some("alpha".into()),
                line: None,
                end_line: None,
            }],
        );
        m.test_map.insert(
            "r1".into(),
            vec![SourceLocation {
                pattern: "src/m.spec.ts".into(),
                symbol: Some("answers one".into()),
                line: None,
                end_line: None,
            }],
        );
        scryer_core::write_model_at(&r, &m).unwrap();
        (dir, r)
    }

    fn fresh_statuses(r: &ModelRef) -> Vec<ClaimTestStatus> {
        ingest_report_file(r, REPORT).unwrap();
        read_test_statuses(r).unwrap()
    }

    #[test]
    fn a_recorded_outcome_reads_fresh_while_nothing_changed() {
        let (_dir, r) = project();
        let statuses = fresh_statuses(&r);
        assert_eq!(statuses.len(), 1);
        assert_eq!(statuses[0].resp_id, "r1");
        assert_eq!(statuses[0].outcome, TestOutcome::Passed);
        assert!(!statuses[0].stale);
    }

    #[test]
    fn editing_the_implementation_flips_the_result_stale() {
        let (_dir, r) = project();
        assert!(!fresh_statuses(&r)[0].stale);
        std::fs::write(
            r.project_path().join("src/m.ts"),
            IMPL_TS.replace("return 1", "return 2"),
        )
        .unwrap();
        assert!(read_test_statuses(&r).unwrap()[0].stale);
    }

    #[test]
    fn editing_the_attached_test_flips_the_result_stale() {
        let (_dir, r) = project();
        assert!(!fresh_statuses(&r)[0].stale);
        std::fs::write(
            r.project_path().join("src/m.spec.ts"),
            SPEC_TS.replace("toBe(1)", "toBe(2)"),
        )
        .unwrap();
        assert!(read_test_statuses(&r).unwrap()[0].stale);
    }

    #[test]
    fn changing_the_attachments_flips_the_result_stale() {
        let (_dir, r) = project();
        assert!(!fresh_statuses(&r)[0].stale);
        // A second attached test appears after the record: the claim's
        // evidence set changed, so the old verdict no longer covers it.
        let mut m = read_model_at(&r).unwrap();
        m.test_map.get_mut("r1").unwrap().push(SourceLocation {
            pattern: "src/m.spec.ts".into(),
            symbol: Some("alpha".into()),
            line: None,
            end_line: None,
        });
        scryer_core::write_model_at(&r, &m).unwrap();
        assert!(read_test_statuses(&r).unwrap()[0].stale);
    }

    #[test]
    fn a_claim_gone_from_the_model_is_omitted_not_a_ghost() {
        let (_dir, r) = project();
        assert_eq!(fresh_statuses(&r).len(), 1);
        let mut m = read_model_at(&r).unwrap();
        m.nodes[0].responsibilities.clear();
        m.source_map.clear();
        m.test_map.clear();
        scryer_core::write_model_at(&r, &m).unwrap();
        assert!(read_test_statuses(&r).unwrap().is_empty());
    }

    #[test]
    fn unresolvable_anchors_at_record_time_can_only_read_stale() {
        let (_dir, r) = project();
        // Both anchor files vanish before the report lands: the outcome is
        // recorded with no fingerprints, so it must never read as current.
        std::fs::remove_file(r.project_path().join("src/m.ts")).unwrap();
        std::fs::remove_file(r.project_path().join("src/m.spec.ts")).unwrap();
        ingest_report_file(&r, REPORT).unwrap();
        let statuses = read_test_statuses(&r).unwrap();
        assert_eq!(statuses.len(), 1);
        assert!(statuses[0].stale);
    }

    #[test]
    fn re_recording_replaces_the_verdict_and_refreshes_it() {
        let (_dir, r) = project();
        assert!(!fresh_statuses(&r)[0].stale);
        // The implementation changes and a new (failing) run reports on it:
        // the fresh record supersedes the stale one.
        std::fs::write(
            r.project_path().join("src/m.ts"),
            IMPL_TS.replace("return 1", "return 2"),
        )
        .unwrap();
        assert!(read_test_statuses(&r).unwrap()[0].stale);
        let failing = REPORT.replace(
            "/>",
            "><failure message=\"expected 1\"/></testcase>",
        );
        ingest_report_file(&r, &failing).unwrap();
        let statuses = read_test_statuses(&r).unwrap();
        assert_eq!(statuses.len(), 1);
        assert_eq!(statuses[0].outcome, TestOutcome::Failed);
        assert!(!statuses[0].stale, "the new record owns the new code");
    }

    #[test]
    fn untouched_anchor_files_prove_freshness_by_stat_alone() {
        let (_dir, r) = project();
        // Recording must land in a LATER second than the files' mtimes, or the
        // fast path abstains (same-second edits are indistinguishable by stat).
        std::thread::sleep(std::time::Duration::from_millis(1100));
        ingest_report_file(&r, REPORT).unwrap();
        let model = read_model_at(&r).unwrap();
        let rec = read_cache(&r).results[0].clone();
        assert_eq!(
            provably_fresh(&model, &rec, r.project_path()),
            Some(true),
            "untouched files, matching keys — no parse needed"
        );

        // A touched anchor file makes the fast path abstain (the hashes decide).
        std::fs::write(r.project_path().join("src/m.ts"), IMPL_TS).unwrap();
        assert_eq!(provably_fresh(&model, &rec, r.project_path()), None);
        // Content is byte-identical, so the full re-hash still reads fresh.
        assert!(!read_test_statuses(&r).unwrap()[0].stale);

        // An attachment added since the record breaks the key correspondence —
        // abstain, even though no file changed.
        let mut m = read_model_at(&r).unwrap();
        m.test_map.get_mut("r1").unwrap().push(SourceLocation {
            pattern: "src/m.spec.ts".into(),
            symbol: Some("alpha".into()),
            line: None,
            end_line: None,
        });
        assert_eq!(provably_fresh(&m, &rec, r.project_path()), None);
    }

    #[test]
    fn the_radius_is_missing_and_stale_verdicts_never_the_whole_suite() {
        let (_dir, r) = project();
        // Never recorded: the attached test file is in the radius.
        let radius = compute_blast_radius(&r).unwrap();
        assert_eq!(radius.len(), 1);
        assert_eq!(radius[0].pattern, "src/m.spec.ts");
        assert_eq!(radius[0].claims, vec!["r1"]);
        assert_eq!(radius[0].stale, 0, "missing verdict, not a stale one");

        // Current verdict: the radius is empty — nothing needs re-running.
        ingest_report_file(&r, REPORT).unwrap();
        assert!(compute_blast_radius(&r).unwrap().is_empty());

        // The implementation changes: the claim's verdict goes stale and its
        // test file re-enters the radius.
        std::fs::write(
            r.project_path().join("src/m.ts"),
            IMPL_TS.replace("return 1", "return 2"),
        )
        .unwrap();
        let radius = compute_blast_radius(&r).unwrap();
        assert_eq!(radius.len(), 1);
        assert_eq!(radius[0].stale, 1);
    }

    #[test]
    fn radius_groups_claims_per_file_so_each_is_one_invocation() {
        let (_dir, r) = project();
        let mut m = read_model_at(&r).unwrap();
        m.nodes[0].responsibilities.push(Responsibility {
            concern: None,
            id: "r2".into(),
            statement: "also answers".into(),
            vagrant: None,
            stale: None,
            stale_proposal: None,
            directives: Vec::new(),
            last_touched_at: None,
        });
        m.test_map.insert(
            "r2".into(),
            vec![SourceLocation {
                pattern: "src/m.spec.ts".into(),
                symbol: Some("answers two".into()),
                line: None,
                end_line: None,
            }],
        );
        scryer_core::write_model_at(&r, &m).unwrap();
        let radius = compute_blast_radius(&r).unwrap();
        assert_eq!(radius.len(), 1, "one file, one invocation: {radius:?}");
        assert_eq!(radius[0].claims, vec!["r1", "r2"]);
    }

    #[test]
    fn ingest_returns_the_match_summary_for_the_caller_to_surface() {
        let (_dir, r) = project();
        let summary = ingest_report_file(&r, REPORT).unwrap();
        assert_eq!(summary.cases, 1);
        assert_eq!(summary.recorded, 1);
        assert!(summary.report.unseen.is_empty());
        // A report about tests the model never attached records nothing but
        // still reports what it saw.
        let stranger = r#"<testsuite><testcase classname="x" name="unknown"/></testsuite>"#;
        let summary = ingest_report_file(&r, stranger).unwrap();
        assert_eq!(summary.recorded, 0);
        assert_eq!(summary.report.unmatched_cases, 1);
        assert_eq!(summary.report.unseen.len(), 1, "the attached test went unseen");
    }

    // --- probes ---

    /// resp-765's core refusal: no attached test means there is no assertion
    /// to fail, so the question a probe asks cannot be asked.
    #[test]
    fn probing_a_claim_with_no_attached_test_is_refused() {
        let (_dir, r) = project();
        let mut m = read_model_at(&r).unwrap();
        m.test_map.remove("r1");
        scryer_core::write_model_at(&r, &m).unwrap();

        let err = resolve_probe_target(&r, "r1").unwrap_err();

        assert!(err.contains("no attached test"), "{err}");
    }

    /// resp-765: a probe reads "the test went red on a break", which says
    /// nothing unless the test was known green first.
    #[test]
    fn probing_without_a_recorded_verdict_is_refused() {
        let (_dir, r) = project();
        let err = resolve_probe_target(&r, "r1").unwrap_err();
        assert!(err.contains("no recorded verdict"), "{err}");
    }

    #[test]
    fn probing_on_a_stale_verdict_is_refused() {
        let (_dir, r) = project();
        fresh_statuses(&r);
        std::fs::write(
            r.project_path().join("src/m.ts"),
            IMPL_TS.replace("return 1", "return 2"),
        )
        .unwrap();

        let err = resolve_probe_target(&r, "r1").unwrap_err();

        assert!(err.contains("stale"), "{err}");
    }

    /// resp-764's payload: the span to break, resolved the same way a
    /// fingerprint resolves it, plus the tests to re-run.
    #[test]
    fn a_probe_target_names_the_span_and_the_tests() {
        let (_dir, r) = project();
        fresh_statuses(&r);

        let target = resolve_probe_target(&r, "r1").unwrap();

        assert_eq!(target.file, "src/m.ts");
        assert_eq!(target.symbol.as_deref(), Some("alpha"));
        assert_eq!((target.start_line, target.end_line), (1, 3), "the whole symbol");
        assert_eq!(target.tests, vec!["src/m.spec.ts :: answers one"]);
    }

    /// resp-768: probes-run and probes-survived are reported separately, and
    /// a claim nobody probed is simply absent — never a clean one.
    #[test]
    fn probe_results_report_runs_and_survivors_and_omit_the_unprobed() {
        let (_dir, r) = project();
        fresh_statuses(&r);
        assert!(read_probe_statuses(&r).unwrap().is_empty(), "unprobed is absent, not clean");

        store_probe_result(&r, "r1", 3, vec!["boundary at line 2 survived".into()]).unwrap();

        let statuses = read_probe_statuses(&r).unwrap();
        assert_eq!(statuses.len(), 1);
        assert_eq!(statuses[0].probes, 3);
        assert_eq!(statuses[0].survived, 1);
        assert_eq!(statuses[0].survivors, vec!["boundary at line 2 survived"]);
        assert!(!statuses[0].stale);
    }

    /// resp-767: a probe result is fingerprinted against the same anchors a
    /// verdict uses, so it ages the same way — it proved something about the
    /// code that was there, never about the code that replaced it.
    #[test]
    fn editing_the_implementation_ages_a_probe_result() {
        let (_dir, r) = project();
        fresh_statuses(&r);
        store_probe_result(&r, "r1", 2, Vec::new()).unwrap();
        assert!(!read_probe_statuses(&r).unwrap()[0].stale);

        std::fs::write(
            r.project_path().join("src/m.ts"),
            IMPL_TS.replace("return 1", "return 2"),
        )
        .unwrap();

        assert!(read_probe_statuses(&r).unwrap()[0].stale);
    }

    /// Editing the TEST ages it too — the probe was about that test's power
    /// to catch a break, and a rewritten test is a different test.
    #[test]
    fn editing_the_attached_test_ages_a_probe_result() {
        let (_dir, r) = project();
        fresh_statuses(&r);
        store_probe_result(&r, "r1", 2, Vec::new()).unwrap();
        assert!(!read_probe_statuses(&r).unwrap()[0].stale);

        std::fs::write(
            r.project_path().join("src/m.spec.ts"),
            SPEC_TS.replace("toBe(1)", "toBe(1); expect(true).toBe(true)"),
        )
        .unwrap();

        assert!(read_probe_statuses(&r).unwrap()[0].stale);
    }

    /// A verdict and a probe are separate claims about the code: recording
    /// probes must not disturb the verdict cache beside it.
    #[test]
    fn recording_probes_leaves_the_verdict_alone() {
        let (_dir, r) = project();
        fresh_statuses(&r);
        store_probe_result(&r, "r1", 1, Vec::new()).unwrap();

        let verdicts = read_test_statuses(&r).unwrap();

        assert_eq!(verdicts.len(), 1);
        assert_eq!(verdicts[0].outcome, TestOutcome::Passed);
        assert!(!verdicts[0].stale);
    }

    /// `evidence_for_claim` is the fold's gate in one call: no attachment, an
    /// attachment with no verdict, a current passing verdict, and a verdict
    /// the tree has since moved past each read as their own kind.
    #[test]
    fn claim_evidence_names_the_missing_fact() {
        let (dir, r) = project();
        let ids = vec!["r1".to_string(), "ghost".to_string()];
        let ev = evidence_for_claim(&r, &ids).unwrap();
        assert_eq!(ev["ghost"], Evidence::NoTest);
        assert_eq!(ev["r1"], Evidence::NoVerdict { tests: vec!["src/m.spec.ts".into()] });
        assert!(ev["r1"].reason().contains("src/m.spec.ts"), "{}", ev["r1"].reason());

        ingest_report_file(&r, REPORT).unwrap();
        let ev = evidence_for_claim(&r, &ids).unwrap();
        assert_eq!(ev["r1"], Evidence::Verified);
        assert!(ev["r1"].verified());

        std::fs::write(dir.path().join("src/m.ts"), IMPL_TS.replace("1", "2")).unwrap();
        let ev = evidence_for_claim(&r, &ids).unwrap();
        assert_eq!(ev["r1"], Evidence::Stale { tests: vec!["src/m.spec.ts".into()] });
    }

    /// A claim that lives only in the PLAN — with its test attached there —
    /// records and reads a verdict: statuses resolve through the working
    /// view, so the verdict-gated fold can be satisfied before the fold.
    #[test]
    fn plan_layer_attachments_record_and_read_verdicts() {
        let (dir, r) = project();
        // The plan-only claim's test must exist so its anchor fingerprints.
        std::fs::write(
            dir.path().join("src/m.spec.ts"),
            format!("{SPEC_TS}\ndescribe(\"beta\", () => {{\n  it(\"answers two\", () => {{\n    expect(2).toBe(2);\n  }});\n}});\n"),
        )
        .unwrap();
        // Committed knows nothing about r2; the plan adds it with a test.
        let committed = read_model_at(&r).unwrap();
        let mut planned = committed.clone();
        planned.nodes[0].responsibilities.push(Responsibility {
            concern: None,
            id: "r2".into(),
            statement: "answers two".into(),
            vagrant: None,
            stale: None,
            stale_proposal: None,
            directives: Vec::new(),
            last_touched_at: None,
        });
        planned.test_map.insert(
            "r2".into(),
            vec![SourceLocation {
                pattern: "src/m.spec.ts".into(),
                symbol: Some("answers two".into()),
                line: None,
                end_line: None,
            }],
        );
        scryer_core::write_planned_at(&r, &planned).unwrap();

        let report = r#"<testsuites><testsuite name="m"><testcase classname="src/m.spec.ts" name="answers two" time="0.001"/></testsuite></testsuites>"#;
        let summary = ingest_report_file(&r, report).unwrap();
        assert_eq!(summary.recorded, 1, "{:?}", summary.report);
        let statuses = read_test_statuses(&r).unwrap();
        let s = statuses.iter().find(|s| s.resp_id == "r2").expect("plan-only claim has a verdict");
        assert!(!s.stale);
        assert_eq!(evidence_for_claim(&r, &["r2".to_string()]).unwrap()["r2"], Evidence::Verified);
    }

    // --- session radius ---

    /// Two claims in two files: r1 (src/m.ts, attached in src/m.spec.ts) and
    /// r2 (src/n.ts, attached in src/n.spec.ts as "answers two").
    fn two_claim_project() -> (tempfile::TempDir, ModelRef) {
        let (dir, r) = project();
        std::fs::write(dir.path().join("src/n.ts"), IMPL_TS.replace("alpha", "beta")).unwrap();
        std::fs::write(
            dir.path().join("src/n.spec.ts"),
            SPEC_TS.replace("alpha", "beta").replace("answers one", "answers two"),
        )
        .unwrap();
        let mut m = read_model_at(&r).unwrap();
        m.nodes[0].responsibilities.push(Responsibility {
            concern: None,
            id: "r2".into(),
            statement: "answers two".into(),
            vagrant: None,
            stale: None,
            stale_proposal: None,
            directives: Vec::new(),
            last_touched_at: None,
        });
        let loc = |pattern: &str, symbol: &str| SourceLocation {
            pattern: pattern.into(),
            symbol: Some(symbol.into()),
            line: None,
            end_line: None,
        };
        m.source_map.insert("r2".into(), vec![loc("src/n.ts", "beta")]);
        m.test_map.insert("r2".into(), vec![loc("src/n.spec.ts", "answers two")]);
        scryer_core::write_model_at(&r, &m).unwrap();
        (dir, r)
    }

    const BOTH: &str = r#"<testsuites><testsuite name="s">
        <testcase classname="src/m.spec.ts" name="alpha &gt; answers one" time="0.01"/>
        <testcase classname="src/n.spec.ts" name="beta &gt; answers two" time="42.5"/>
    </testsuite></testsuites>"#;

    #[test]
    fn ingest_records_each_attached_tests_duration() {
        let (_dir, r) = two_claim_project();
        ingest_report_file(&r, BOTH).unwrap();
        let cache = read_cache(&r);
        let r2 = cache.results.iter().find(|c| c.resp_id == "r2").unwrap();
        assert_eq!(r2.millis["answers two"], 42_500);
    }

    /// The session's radius is its own claims; a stale verdict it never
    /// touched is listed as someone else's, not handed to it to run.
    #[test]
    fn a_session_radius_holds_only_the_claims_it_touched() {
        let (dir, r) = two_claim_project();
        ingest_report_file(&r, BOTH).unwrap();
        for f in ["src/m.ts", "src/n.ts"] {
            let body = std::fs::read_to_string(dir.path().join(f)).unwrap();
            std::fs::write(dir.path().join(f), body.replace("return 1", "return 2")).unwrap();
        }
        let touched = vec!["src/m.ts".to_string()];
        let radius = compute_session_radius(&r, Some(&touched)).unwrap();
        assert!(radius.scoped);
        assert_eq!(radius.files.len(), 1);
        assert_eq!(radius.files[0].claims, vec!["r1"]);
        assert_eq!(radius.run.len(), 1);
        assert_eq!(radius.run[0].name.as_deref(), Some("answers one"));
        assert_eq!(radius.not_yours, vec!["r2"]);
    }

    /// A slow test stays out of the run unless its own claim was touched.
    #[test]
    fn slow_tests_are_left_out_unless_their_claim_was_touched() {
        let (dir, r) = two_claim_project();
        ingest_report_file(&r, BOTH).unwrap();
        let body = std::fs::read_to_string(dir.path().join("src/n.ts")).unwrap();
        std::fs::write(dir.path().join("src/n.ts"), body.replace("return 1", "return 2")).unwrap();

        let unscoped = compute_session_radius(&r, None).unwrap();
        assert!(unscoped.run.is_empty(), "{unscoped:?}");
        assert_eq!(unscoped.slow.len(), 1);
        assert_eq!(unscoped.slow[0].millis, Some(42_500));

        let touched = vec!["src/n.ts".to_string()];
        let mine = compute_session_radius(&r, Some(&touched)).unwrap();
        assert!(mine.slow.is_empty());
        assert_eq!(mine.run[0].name.as_deref(), Some("answers two"));
    }

    // --- probe check ---

    #[test]
    fn the_probe_check_blocks_until_the_sessions_tested_claims_are_probed() {
        let (_dir, r) = two_claim_project();
        ingest_report_file(&r, BOTH).unwrap();
        let touched = vec!["src/m.spec.ts".to_string(), "src/n.spec.ts".to_string()];

        let check = session_probe_check(&r, &touched);
        let block = check.block.expect("two green claims behind touched tests, none probed");
        assert!(block.contains("r1, r2"), "{block}");
        assert!(check.summary.is_none());

        store_probe_result(&r, "r1", 3, Vec::new()).unwrap();
        assert!(session_probe_check(&r, &touched).block.is_some(), "one of two is not enough");

        store_probe_result(&r, "r2", 2, vec!["dropped the guard".into()]).unwrap();
        let check = session_probe_check(&r, &touched);
        assert!(check.block.is_none());
        let summary = check.summary.expect("a survivor reaches the user");
        assert!(summary.contains("r2 (dropped the guard)"), "{summary}");
    }

    /// Only tests the session wrote or changed count: editing code alone, or
    /// a test whose verdict is not green, asks for no probe.
    #[test]
    fn the_probe_check_ignores_claims_whose_tests_the_session_left_alone() {
        let (_dir, r) = two_claim_project();
        ingest_report_file(&r, BOTH).unwrap();
        assert_eq!(session_probe_check(&r, &["src/m.ts".to_string()]), ProbeCheck::default());
        // One touched test file, one green claim: one probe is the whole ask.
        let check = session_probe_check(&r, &["src/m.spec.ts".to_string()]);
        assert!(check.block.unwrap().contains("1 of them"));
    }

    /// A report ingested before a test was attached still settles it: the
    /// attachment takes the kept outcome — unless the claim's code changed
    /// after that ingest, when the old run vouches for nothing.
    #[test]
    fn a_test_attached_after_its_report_takes_the_kept_verdict() {
        let (dir, r) = two_claim_project();
        let mut m = read_model_at(&r).unwrap();
        let r1_tests = m.test_map.remove("r1").unwrap();
        let r2_tests = m.test_map.remove("r2").unwrap();
        scryer_core::write_model_at(&r, &m).unwrap();
        ingest_report_file(&r, BOTH).unwrap();
        assert!(read_cache(&r).results.is_empty(), "nothing was attached at ingest");

        // r2's code changes after the ingest; r1's does not.
        std::thread::sleep(std::time::Duration::from_millis(20));
        let body = std::fs::read_to_string(dir.path().join("src/n.ts")).unwrap();
        std::fs::write(dir.path().join("src/n.ts"), body.replace("return 1", "return 2")).unwrap();

        let attached: BTreeMap<_, _> = [("r1".to_string(), r1_tests), ("r2".to_string(), r2_tests)].into();
        let replayed = replay_kept_cases(&r, &attached).unwrap();
        assert_eq!(replayed, vec!["r1"], "r2 changed since the run");
        let mut m = read_model_at(&r).unwrap();
        m.test_map.extend(attached);
        scryer_core::write_model_at(&r, &m).unwrap();
        let statuses = read_test_statuses(&r).unwrap();
        let r1 = statuses.iter().find(|s| s.resp_id == "r1").unwrap();
        assert_eq!(r1.outcome, TestOutcome::Passed);
        assert!(!r1.stale, "fingerprinted with the attachment in place");
        assert!(statuses.iter().all(|s| s.resp_id != "r2"));
    }

    /// A claim whose last probe let a break survive is asked for first once
    /// its code or test changes — to see the strengthened test catch it.
    #[test]
    fn the_probe_check_rechecks_a_survivor_first() {
        let (dir, r) = two_claim_project();
        ingest_report_file(&r, BOTH).unwrap();
        store_probe_result(&r, "r2", 3, vec!["dropped the guard".into()]).unwrap();
        // Strengthen r2's test: its verdict re-runs green, its probe goes stale.
        let spec = dir.path().join("src/n.spec.ts");
        let body = std::fs::read_to_string(&spec).unwrap();
        std::fs::write(&spec, body.replace(".toBe(1);", ".toBe(1);\n    expect(beta()).not.toBe(0);"))
            .unwrap();
        ingest_report_file(&r, BOTH).unwrap();

        let touched = vec!["src/m.spec.ts".to_string(), "src/n.spec.ts".to_string()];
        let block = session_probe_check(&r, &touched).block.expect("two to probe");
        assert!(block.contains("Probe r2, r1"), "{block}");
    }

    /// A probe breaks the claim's code; a claim with a test but no code
    /// anchor has nothing to break and is never asked for.
    #[test]
    fn the_probe_check_skips_claims_with_no_code_anchor() {
        let (_dir, r) = two_claim_project();
        ingest_report_file(&r, BOTH).unwrap();
        let mut m = scryer_core::read_model_at(&r).unwrap();
        m.source_map.remove("r1");
        scryer_core::write_model_at(&r, &m).unwrap();

        let touched = vec!["src/m.spec.ts".to_string(), "src/n.spec.ts".to_string()];
        let block = session_probe_check(&r, &touched).block.expect("r2 still has code");
        assert!(block.contains("Probe r2") && !block.contains("r1"), "{block}");
        assert!(block.contains("1 of them"), "{block}");
    }
}
