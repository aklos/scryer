//! The claim test-status cache — each claim's last reported test outcome,
//! keyed by what the code looked like when the tests said it.
//!
//! Scryer never runs tests; results arrive as JUnit reports (see
//! `scryer_core::test_results`) and are recorded here beside the content
//! fingerprints of the claim's implementation and test anchors, taken from
//! the working tree at record time. Reading a status re-resolves those same
//! anchors and compares: any difference — the implementation edited, the test
//! edited, an attachment added or removed — flips the result to STALE without
//! running anything. There is no watcher and no timestamp heuristics; the
//! fingerprints are the whole invalidation story, the same machinery the
//! anchor tripwire trusts (`anchors`).

use crate::anchors::{is_glob_pattern, resolve_span, span_hash, FileCache};
use scryer_core::test_results::{parse_junit, match_report, ReportMatch, TestOutcome};
use scryer_core::{read_model_at, read_planned_at, test_key, working_view, ModelRef, ScryModel};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// One claim's cached verdict and the anchor content it was true of.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimRecord {
    pub resp_id: String,
    pub outcome: TestOutcome,
    /// How many report cases fed the verdict.
    pub cases: usize,
    /// Seconds since the epoch at record time — display only, never used for
    /// invalidation.
    pub recorded_at: u64,
    /// Anchor identity (`{key}|{file}|{symbol}`) → span content hash at
    /// record time, across BOTH dimensions: the claim's implementation
    /// anchors under its bare key and its attached tests under `test:{id}`.
    /// Empty means nothing was resolvable when the result landed — such a
    /// record can only ever read as stale.
    #[serde(default)]
    pub fingerprints: BTreeMap<String, String>,
}

/// One claim's probe history: how many deliberate breaks were tried against
/// its span, and how many its attached test failed to catch.
///
/// Kept apart from [`ClaimRecord`] on purpose. A verdict answers "does the
/// test pass"; a probe answers "would it fail if the code were wrong". They
/// go stale together — same anchors, same fingerprints — but they are never
/// the same claim about the code, and collapsing them would let a green
/// verdict read as proof it isn't.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeRecord {
    pub resp_id: String,
    /// Deliberate breaks tried against the claim's span.
    pub probes: u32,
    /// How many of them the attached test did NOT catch.
    pub survived: u32,
    /// What each survivor was, in the prober's words — the audit trail for a
    /// claim that reads as probed, and the to-do list for one that doesn't.
    #[serde(default)]
    pub survivors: Vec<String>,
    pub recorded_at: u64,
    /// Same anchor identity → content hash map a verdict records, taken the
    /// same way, so an edit to the implementation or the test ages a probe
    /// result exactly as it ages a verdict.
    #[serde(default)]
    pub fingerprints: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestStatusCache {
    #[serde(default)]
    pub results: Vec<ClaimRecord>,
    #[serde(default)]
    pub probes: Vec<ProbeRecord>,
}

/// A cached verdict as the caller should present it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimTestStatus {
    pub resp_id: String,
    pub outcome: TestOutcome,
    pub cases: usize,
    /// The code behind the claim (implementation or attached test) no longer
    /// hashes as it did when this outcome was reported.
    pub stale: bool,
    pub recorded_at: u64,
}

/// A cached probe result as the caller should present it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimProbeStatus {
    pub resp_id: String,
    pub probes: u32,
    pub survived: u32,
    pub survivors: Vec<String>,
    /// The code behind the claim no longer hashes as it did when these
    /// probes ran — the result describes code that has since moved on.
    pub stale: bool,
    pub recorded_at: u64,
}

/// What one ingested report amounted to.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IngestSummary {
    /// Cases the report held (all of them, matched or not).
    pub cases: usize,
    /// Claims whose outcome was recorded.
    pub recorded: usize,
    pub report: ReportMatch,
}

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

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Merge one matched report's per-claim outcomes into the cache. Each claim's
/// anchors are fingerprinted against the working tree as it stands — the
/// record means "this outcome was true of THIS code". A claim already in the
/// cache is replaced: the cache holds the latest word, not a history.
pub fn record_test_results(r: &ModelRef, report: &ReportMatch) -> Result<usize, String> {
    if report.claims.is_empty() {
        return Ok(0);
    }
    let model = working_model(r)?;
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
pub fn test_statuses(r: &ModelRef) -> Result<Vec<ClaimTestStatus>, String> {
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
        if !live.contains(rec.resp_id.as_str()) {
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

/// What the model can say about one claim's test evidence — the fold's gate.
/// Deterministic: a test is attached or it isn't, and its recorded verdict is
/// current-and-passing or it isn't. `tests` names the attached test files so a
/// refusal can say exactly what to run.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind", rename_all_fields = "camelCase")]
pub enum Evidence {
    /// No test attached at all.
    NoTest,
    /// A test is attached but no report has ever been ingested for it.
    NoVerdict { tests: Vec<String> },
    /// The recorded verdict's fingerprints no longer match the tree.
    Stale { tests: Vec<String> },
    /// The current verdict is not `Passed` (failed, errored, or skipped).
    Failing { outcome: TestOutcome, tests: Vec<String> },
    /// A test is attached and its verdict is current and passing.
    Verified,
}

impl Evidence {
    pub fn verified(&self) -> bool {
        matches!(self, Evidence::Verified)
    }

    /// The missing fact, in the words a refusal uses.
    pub fn reason(&self) -> String {
        match self {
            Evidence::NoTest => "no test attached".to_string(),
            Evidence::NoVerdict { tests } => {
                format!("no verdict recorded: run {} and ingest_test_report", tests.join(", "))
            }
            Evidence::Stale { tests } => {
                format!("verdict stale: run {} and ingest_test_report", tests.join(", "))
            }
            Evidence::Failing { outcome, tests } => {
                format!("verdict {outcome:?}: fix and re-run {}", tests.join(", "))
            }
            Evidence::Verified => "verified".to_string(),
        }
    }

    /// The attached test files, when any.
    pub fn tests(&self) -> &[String] {
        match self {
            Evidence::NoVerdict { tests }
            | Evidence::Stale { tests }
            | Evidence::Failing { tests, .. } => tests,
            _ => &[],
        }
    }
}

/// The evidence behind each named claim, resolved against the working view
/// (plan-layer attachments count — see [`working_model`]). Reads the cached
/// verdicts once for the whole set. Unknown ids read as `NoTest`.
pub fn claim_evidence(
    r: &ModelRef,
    resp_ids: &[String],
) -> Result<BTreeMap<String, Evidence>, String> {
    let model = working_model(r)?;
    let verdicts = test_statuses(r)?;
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

/// One test file the blast radius says to run, and why.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RadiusFile {
    /// The attached test file (as the attachment spells it).
    pub pattern: String,
    /// The claims whose verdicts running this file would refresh.
    pub claims: Vec<String>,
    /// How many of those claims have a stale verdict (the rest have none).
    pub stale: usize,
}

/// Exactly which attached test files need re-running: every test-attached
/// claim whose verdict is missing or stale contributes its test files;
/// claims whose verdict is current contribute nothing. Grouped per file so
/// each entry is one targeted invocation — the radius is what needs
/// re-running, never the whole suite. (A claim with NO attached test never
/// appears here — that gap is health's `untested`, not a radius entry.)
pub fn test_blast_radius(r: &ModelRef) -> Result<Vec<RadiusFile>, String> {
    let model = working_model(r)?;
    let verdicts = test_statuses(r)?;
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

/// The one-call entry point: JUnit XML in, per-claim outcomes recorded,
/// match summary out — unmatched, ambiguous, and unseen included, so the
/// caller can surface what the report did NOT settle alongside what it did.
pub fn ingest_report(r: &ModelRef, xml: &str) -> Result<IngestSummary, String> {
    let model = working_model(r)?;
    let cases = parse_junit(xml)?;
    let report = match_report(&model.test_map, &cases);
    let recorded = record_test_results(r, &report)?;
    Ok(IngestSummary { cases: cases.len(), recorded, report })
}

/// Where a probe should aim, and what to run afterwards.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeTarget {
    pub resp_id: String,
    pub statement: String,
    /// Project-relative file holding the claim's implementation.
    pub file: String,
    /// 1-based inclusive span to break. Resolved the same way a fingerprint
    /// resolves it, so the probe lands inside exactly the region the claim is
    /// anchored to and nowhere else.
    pub start_line: u32,
    pub end_line: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// The attached tests to re-run.
    pub tests: Vec<String>,
}

/// Resolve one claim into a probe target, or explain why it can't be probed.
///
/// The refusals are the point. Breaking code behind a claim with no attached
/// test proves nothing — there is no assertion to fail. Breaking code behind a
/// test whose verdict is missing or stale proves nothing either: a probe reads
/// "the test went red", which only means something when you already know it
/// was green.
pub fn probe_target(r: &ModelRef, resp_id: &str) -> Result<ProbeTarget, String> {
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

    match test_statuses(r)?.into_iter().find(|s| s.resp_id == resp_id) {
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
pub fn record_probe_result(
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
pub fn probe_statuses(r: &ModelRef) -> Result<Vec<ClaimProbeStatus>, String> {
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
                vagrant_origin: None,
                approved_statement: None,
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
        ingest_report(r, REPORT).unwrap();
        test_statuses(r).unwrap()
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
        assert!(test_statuses(&r).unwrap()[0].stale);
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
        assert!(test_statuses(&r).unwrap()[0].stale);
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
        assert!(test_statuses(&r).unwrap()[0].stale);
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
        assert!(test_statuses(&r).unwrap().is_empty());
    }

    #[test]
    fn unresolvable_anchors_at_record_time_can_only_read_stale() {
        let (_dir, r) = project();
        // Both anchor files vanish before the report lands: the outcome is
        // recorded with no fingerprints, so it must never read as current.
        std::fs::remove_file(r.project_path().join("src/m.ts")).unwrap();
        std::fs::remove_file(r.project_path().join("src/m.spec.ts")).unwrap();
        ingest_report(&r, REPORT).unwrap();
        let statuses = test_statuses(&r).unwrap();
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
        assert!(test_statuses(&r).unwrap()[0].stale);
        let failing = REPORT.replace(
            "/>",
            "><failure message=\"expected 1\"/></testcase>",
        );
        ingest_report(&r, &failing).unwrap();
        let statuses = test_statuses(&r).unwrap();
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
        ingest_report(&r, REPORT).unwrap();
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
        assert!(!test_statuses(&r).unwrap()[0].stale);

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
        let radius = test_blast_radius(&r).unwrap();
        assert_eq!(radius.len(), 1);
        assert_eq!(radius[0].pattern, "src/m.spec.ts");
        assert_eq!(radius[0].claims, vec!["r1"]);
        assert_eq!(radius[0].stale, 0, "missing verdict, not a stale one");

        // Current verdict: the radius is empty — nothing needs re-running.
        ingest_report(&r, REPORT).unwrap();
        assert!(test_blast_radius(&r).unwrap().is_empty());

        // The implementation changes: the claim's verdict goes stale and its
        // test file re-enters the radius.
        std::fs::write(
            r.project_path().join("src/m.ts"),
            IMPL_TS.replace("return 1", "return 2"),
        )
        .unwrap();
        let radius = test_blast_radius(&r).unwrap();
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
            vagrant_origin: None,
            approved_statement: None,
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
        let radius = test_blast_radius(&r).unwrap();
        assert_eq!(radius.len(), 1, "one file, one invocation: {radius:?}");
        assert_eq!(radius[0].claims, vec!["r1", "r2"]);
    }

    #[test]
    fn ingest_returns_the_match_summary_for_the_caller_to_surface() {
        let (_dir, r) = project();
        let summary = ingest_report(&r, REPORT).unwrap();
        assert_eq!(summary.cases, 1);
        assert_eq!(summary.recorded, 1);
        assert!(summary.report.unseen.is_empty());
        // A report about tests the model never attached records nothing but
        // still reports what it saw.
        let stranger = r#"<testsuite><testcase classname="x" name="unknown"/></testsuite>"#;
        let summary = ingest_report(&r, stranger).unwrap();
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

        let err = probe_target(&r, "r1").unwrap_err();

        assert!(err.contains("no attached test"), "{err}");
    }

    /// resp-765: a probe reads "the test went red on a break", which says
    /// nothing unless the test was known green first.
    #[test]
    fn probing_without_a_recorded_verdict_is_refused() {
        let (_dir, r) = project();
        let err = probe_target(&r, "r1").unwrap_err();
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

        let err = probe_target(&r, "r1").unwrap_err();

        assert!(err.contains("stale"), "{err}");
    }

    /// resp-764's payload: the span to break, resolved the same way a
    /// fingerprint resolves it, plus the tests to re-run.
    #[test]
    fn a_probe_target_names_the_span_and_the_tests() {
        let (_dir, r) = project();
        fresh_statuses(&r);

        let target = probe_target(&r, "r1").unwrap();

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
        assert!(probe_statuses(&r).unwrap().is_empty(), "unprobed is absent, not clean");

        record_probe_result(&r, "r1", 3, vec!["boundary at line 2 survived".into()]).unwrap();

        let statuses = probe_statuses(&r).unwrap();
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
        record_probe_result(&r, "r1", 2, Vec::new()).unwrap();
        assert!(!probe_statuses(&r).unwrap()[0].stale);

        std::fs::write(
            r.project_path().join("src/m.ts"),
            IMPL_TS.replace("return 1", "return 2"),
        )
        .unwrap();

        assert!(probe_statuses(&r).unwrap()[0].stale);
    }

    /// Editing the TEST ages it too — the probe was about that test's power
    /// to catch a break, and a rewritten test is a different test.
    #[test]
    fn editing_the_attached_test_ages_a_probe_result() {
        let (_dir, r) = project();
        fresh_statuses(&r);
        record_probe_result(&r, "r1", 2, Vec::new()).unwrap();
        assert!(!probe_statuses(&r).unwrap()[0].stale);

        std::fs::write(
            r.project_path().join("src/m.spec.ts"),
            SPEC_TS.replace("toBe(1)", "toBe(1); expect(true).toBe(true)"),
        )
        .unwrap();

        assert!(probe_statuses(&r).unwrap()[0].stale);
    }

    /// A verdict and a probe are separate claims about the code: recording
    /// probes must not disturb the verdict cache beside it.
    #[test]
    fn recording_probes_leaves_the_verdict_alone() {
        let (_dir, r) = project();
        fresh_statuses(&r);
        record_probe_result(&r, "r1", 1, Vec::new()).unwrap();

        let verdicts = test_statuses(&r).unwrap();

        assert_eq!(verdicts.len(), 1);
        assert_eq!(verdicts[0].outcome, TestOutcome::Passed);
        assert!(!verdicts[0].stale);
    }

    /// `claim_evidence` is the fold's gate in one call: no attachment, an
    /// attachment with no verdict, a current passing verdict, and a verdict
    /// the tree has since moved past each read as their own kind.
    #[test]
    fn claim_evidence_names_the_missing_fact() {
        let (dir, r) = project();
        let ids = vec!["r1".to_string(), "ghost".to_string()];
        let ev = claim_evidence(&r, &ids).unwrap();
        assert_eq!(ev["ghost"], Evidence::NoTest);
        assert_eq!(ev["r1"], Evidence::NoVerdict { tests: vec!["src/m.spec.ts".into()] });
        assert!(ev["r1"].reason().contains("src/m.spec.ts"), "{}", ev["r1"].reason());

        ingest_report(&r, REPORT).unwrap();
        let ev = claim_evidence(&r, &ids).unwrap();
        assert_eq!(ev["r1"], Evidence::Verified);
        assert!(ev["r1"].verified());

        std::fs::write(dir.path().join("src/m.ts"), IMPL_TS.replace("1", "2")).unwrap();
        let ev = claim_evidence(&r, &ids).unwrap();
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
            vagrant_origin: None,
            approved_statement: None,
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
        let summary = ingest_report(&r, report).unwrap();
        assert_eq!(summary.recorded, 1, "{:?}", summary.report);
        let statuses = test_statuses(&r).unwrap();
        let s = statuses.iter().find(|s| s.resp_id == "r2").expect("plan-only claim has a verdict");
        assert!(!s.stale);
        assert_eq!(claim_evidence(&r, &["r2".to_string()]).unwrap()["r2"], Evidence::Verified);
    }
}
