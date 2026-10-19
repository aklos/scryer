//! The test-verdict cache on disk and the reads behind it. Verdicts ingested
//! from JUnit reports and probe results are fingerprinted against the claim's
//! anchored code and tests, so an edit since the run reads as stale; freshness
//! is checked cheaply against file mtimes first and re-hashed only where a
//! file was touched. Claims resolve against the derived tree. Verdicts live in
//! `.scryer/.test-results.json`; mutation reports are merged into
//! `.scryer/.mutations.json`.

use crate::domain::anchors::{is_glob_pattern, resolve_span, span_hash};
use crate::domain::mutations::{parse_mutation_report, MutationSummary};
use crate::domain::test_status::*;
use crate::domain::lang;
use std::collections::HashMap;
use scryer_core::test_results::{match_report, parse_junit, ReportMatch};
use scryer_core::test_key;
use scryer_core::{ModelRef, ScryModel};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// The model every status read resolves against: the derived tree's planned
/// layer, so verdicts, probes and the radius key on derived claim ids and
/// todo stubs count.
fn working_model(r: &ModelRef) -> Result<ScryModel, String> {
    #[cfg(test)]
    if let Some(m) = TEST_MODEL.with(|c| c.borrow().clone()) {
        return Ok(m);
    }
    crate::infrastructure::derived_tree::derived_tree(r.project_path()).map(|t| t.planned)
}

#[cfg(test)]
thread_local! {
    /// A model a test hands the status code directly, standing in for the
    /// derived tree so the verdict rules can be exercised on a fixture.
    static TEST_MODEL: std::cell::RefCell<Option<ScryModel>> = const { std::cell::RefCell::new(None) };
}

/// Stand `m` in for the derived tree in this test's thread.
#[cfg(test)]
fn use_model(m: &ScryModel) {
    TEST_MODEL.with(|c| *c.borrow_mut() = Some(m.clone()));
}

/// Per-call parse memo: each touched file is parsed at most once.
pub(crate) struct FileCache {
    files: HashMap<String, Option<(String, Option<lang::FileParse>)>>,
}

impl FileCache {
    pub(crate) fn new() -> Self {
        Self {
            files: HashMap::new(),
        }
    }

    /// Returns (source, parse) for a project-relative path; `None` when the
    /// file doesn't exist. Parse is `None` for unsupported grammars.
    pub(crate) fn get(&mut self, project: &Path, rel: &str) -> Option<&(String, Option<lang::FileParse>)> {
        if !self.files.contains_key(rel) {
            let loaded = std::fs::read_to_string(project.join(rel))
                .ok()
                .map(|source| {
                    let parse = lang::parse_file(Path::new(rel), &source);
                    (source, parse)
                });
            self.files.insert(rel.to_string(), loaded);
        }
        self.files.get(rel).and_then(|o| o.as_ref())
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
                    project_files.get_or_insert_with(|| crate::infrastructure::walk::project_files(project));
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
/// since the record skip the re-hash entirely (`provably_fresh`), so this
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

/// The one-call entry point: JUnit XML in, per-claim outcomes recorded,
/// match summary out — unmatched, ambiguous, and unseen included, so the
/// caller can surface what the report did NOT settle alongside what it did.
pub fn ingest_report_file(r: &ModelRef, xml: &str) -> Result<IngestSummary, String> {
    ingest_report_on(r, &working_model(r)?, xml)
}

/// [`ingest_report_file`] matched against `model`.
fn ingest_report_on(r: &ModelRef, model: &ScryModel, xml: &str) -> Result<IngestSummary, String> {
    let cases = parse_junit(xml)?;
    let report = match_report(&model.test_map, &cases);
    let recorded = store_results_on(r, model, &report)?;
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
        // A round that tried no break learned nothing — such records predate
        // close_probe refusing them, and must not read as probed.
        if !live.contains(rec.resp_id.as_str()) || rec.probes == 0 {
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
            source: None,
        });
    }
    out.sort_by(|a, b| a.resp_id.cmp(&b.resp_id));
    Ok(out)
}

/// The derived tree with every claim's status and the health counts, each
/// verdict re-verified against the working tree: a cheap mtime check first,
/// and where a file was touched, a re-hash of the claim's anchors.
pub fn read_derived_status(
    r: &ModelRef,
) -> Result<(scryer_core::derived_tree::DerivedTree, scryer_core::derived_tree::DerivedStatus), String> {
    use scryer_core::derived_tree::{compute_derived_status, read_mutations_at};
    let project = r.project_path();
    let tree = crate::infrastructure::derived_tree::derived_tree(project)?;
    let cache = read_cache(r);
    let mutations = read_mutations_at(r);
    let mut files = FileCache::new();
    let mut project_files: Option<BTreeSet<String>> = None;
    let mut fresh = |id: &str, fp: &BTreeMap<String, String>, at: u64| {
        scryer_core::derived_tree::anchors_untouched_since(&tree.planned, project, id, fp, at)
            || claim_fingerprints(&tree.planned, id, project, &mut files, &mut project_files) == *fp
    };
    let status = compute_derived_status(&tree, &cache, &mutations, &mut fresh);
    Ok((tree, status))
}

/// Ingest a cargo-mutants `outcomes.json` or Stryker `mutation.json`. Each
/// file and test the report covers replaces what an earlier report said
/// about it; the rest is kept, so reports from several tools add up.
pub fn ingest_mutation_file(r: &ModelRef, report: &str) -> Result<MutationSummary, String> {
    use scryer_core::derived_tree::{read_mutations_at, write_mutations_at};
    let parsed = parse_mutation_report(report)?;
    let project = r.project_path();
    let known = crate::infrastructure::walk::project_files(project);
    let prefix = format!("{}/", project.display());
    let resolve = |path: &str| -> String {
        let rel = path.strip_prefix(&prefix).unwrap_or(path).trim_start_matches("./").replace('\\', "/");
        if known.contains(&rel) {
            return rel;
        }
        // Tools report package-relative paths in a workspace.
        let tail = format!("/{rel}");
        let mut hits = known.iter().filter(|f| f.ends_with(&tail));
        match (hits.next(), hits.next()) {
            (Some(only), None) => only.clone(),
            _ => rel,
        }
    };
    let mut cache = read_mutations_at(r);
    for (file, tally) in &parsed.files {
        cache.files.insert(resolve(file), tally.clone());
    }
    for (test, tally) in &parsed.tests {
        cache.tests.insert(test.clone(), tally.clone());
    }
    cache.ingested_at = now_secs();
    write_mutations_at(r, &cache)?;
    Ok(parsed.summary())
}

#[cfg(test)]
mod tests {
    use super::*;
    use scryer_core::test_results::TestOutcome;
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
        use_model(&m);
        (dir, r)
    }

    fn fresh_statuses(r: &ModelRef) -> Vec<ClaimTestStatus> {
        ingest_report_file(r, REPORT).unwrap();
        read_test_statuses(r).unwrap()
    }

    /// While nothing changed, a recorded outcome reads fresh.
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
        let mut m = working_model(&r).unwrap();
        m.test_map.get_mut("r1").unwrap().push(SourceLocation {
            pattern: "src/m.spec.ts".into(),
            symbol: Some("alpha".into()),
            line: None,
            end_line: None,
        });
        use_model(&m);
        assert!(read_test_statuses(&r).unwrap()[0].stale);
    }

    #[test]
    fn a_claim_gone_from_the_model_is_omitted_not_a_ghost() {
        let (_dir, r) = project();
        assert_eq!(fresh_statuses(&r).len(), 1);
        let mut m = working_model(&r).unwrap();
        m.nodes[0].responsibilities.clear();
        m.source_map.clear();
        m.test_map.clear();
        use_model(&m);
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
        // The fast path proves freshness only for files older than the
        // record's second: date the fixture's files an hour back.
        let hour_ago = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
        for f in ["src/m.ts", "src/m.spec.ts"] {
            let file = std::fs::File::options().write(true).open(r.project_path().join(f)).unwrap();
            file.set_modified(hour_ago).unwrap();
        }
        ingest_report_file(&r, REPORT).unwrap();
        let model = working_model(&r).unwrap();
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
        let mut m = working_model(&r).unwrap();
        m.test_map.get_mut("r1").unwrap().push(SourceLocation {
            pattern: "src/m.spec.ts".into(),
            symbol: Some("alpha".into()),
            line: None,
            end_line: None,
        });
        assert_eq!(provably_fresh(&m, &rec, r.project_path()), None);
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

    /// A stored round with no break tried — written before close_probe
    /// refused them — reads as unprobed, not as probed clean.
    #[test]
    fn a_stored_round_with_no_break_tried_is_not_a_probe() {
        let (_dir, r) = project();
        fresh_statuses(&r);
        store_probe_result(&r, "r1", 0, Vec::new()).unwrap();
        assert!(read_probe_statuses(&r).unwrap().is_empty(), "unprobed, not clean");
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
        let mut m = working_model(&r).unwrap();
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
        use_model(&m);
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

    // --- probe check ---

    /// End to end on the derived tree: a JUnit report keyed by test name
    /// makes a green claim proven and a red one failing, a claim no report
    /// covered reads stale and a todo stub planned; editing the exercised
    /// file ages the green verdict; a cargo-mutants report marks the file's
    /// component and counts its survivors.
    #[test]
    fn derived_claims_take_status_from_reports_and_fingerprints() {
        use scryer_core::derived_tree::{ClaimState, ProbeMark};
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        let write = |rel: &str, text: &str| {
            let path = d.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        write("Cargo.toml", "[workspace]\nmembers = [\"crates/*\"]\n");
        write("crates/core/Cargo.toml", "[package]\nname = \"core\"\nversion = \"0.1.0\"\n");
        write(
            "crates/core/src/draft.rs",
            "pub fn save() -> bool { true }\n#[cfg(test)]\nmod tests {\n    /// Saves\n    #[test]\n    fn saves() {}\n    /// Loads\n    #[test]\n    fn loads() {}\n    /// Merges\n    #[test]\n    fn merges() { todo!() }\n    /// Splits\n    #[test]\n    fn splits() {}\n}\n",
        );
        let r = ModelRef::ProjectLocal(d.to_path_buf());
        let tree = crate::infrastructure::derived_tree::derived_tree(d).unwrap();
        let junit = r#"<testsuite name="core">
  <testcase classname="core" name="draft::tests::saves"/>
  <testcase classname="core" name="draft::tests::loads"><failure message="boom"/></testcase>
</testsuite>"#;
        // The verdict lands in the same second the files were written: its
        // freshness rests on the content hashes, not the clock.
        let summary = ingest_report_on(&r, &tree.planned, junit).unwrap();
        assert_eq!(summary.recorded, 2);

        let state_of = |status: &scryer_core::derived_tree::DerivedStatus, ident: &str| {
            let id = &tree.claims.iter().find(|c| c.identifier == ident).unwrap().id;
            status.claims.iter().find(|c| &c.resp_id == id).unwrap().state
        };
        let (_, status) = read_derived_status(&r).unwrap();
        assert_eq!(state_of(&status, "saves"), ClaimState::Proven);
        assert_eq!(state_of(&status, "loads"), ClaimState::Failing);
        assert_eq!(state_of(&status, "splits"), ClaimState::Stale);
        assert_eq!(state_of(&status, "merges"), ClaimState::Planned);
        assert_eq!((status.totals.proven, status.totals.failing, status.totals.stale, status.totals.planned), (1, 1, 1, 1));

        let mutants = r#"{"outcomes":[
          {"scenario":{"Mutant":{"package":"core","file":"src/draft.rs","function":{"function_name":"save"},"span":{"start":{"line":1,"column":1}},"replacement":"false"}},"summary":"MissedMutant"}
        ]}"#;
        let m = ingest_mutation_file(&r, mutants).unwrap();
        assert_eq!((m.mutants, m.survived), (1, 1));
        let (_, status) = read_derived_status(&r).unwrap();
        let draft = &tree.file_nodes["crates/core/src/draft.rs"];
        assert_eq!(status.nodes[draft].probe, Some(ProbeMark::Crosshair), "package-relative path resolved");
        assert_eq!(status.totals.surviving_mutants, 1);

        // Editing the exercised code (the inline tests' own file) ages it.
        write(
            "crates/core/src/draft.rs",
            "pub fn save() -> bool { false }\n#[cfg(test)]\nmod tests {\n    /// Saves\n    #[test]\n    fn saves() {}\n    /// Loads\n    #[test]\n    fn loads() {}\n    /// Merges\n    #[test]\n    fn merges() { todo!() }\n    /// Splits\n    #[test]\n    fn splits() {}\n}\n",
        );
        let (_, status) = read_derived_status(&r).unwrap();
        assert_eq!(state_of(&status, "saves"), ClaimState::Stale);
    }

    /// Ad-hoc: the derived tree's status for a repo, after ingesting any
    /// JUnit reports and mutation report given [manual]
    ///
    /// `SCRYER_DERIVED_TREE=1 REPO=/path JUNIT=a.xml,b.xml MUTANTS=outcomes.json
    ///  cargo test -p scryer-extract dump_derived_status -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn dump_derived_status() {
        let repo = std::path::PathBuf::from(std::env::var("REPO").expect("set REPO"));
        let r = ModelRef::ProjectLocal(repo);
        for path in std::env::var("JUNIT").unwrap_or_default().split(',').filter(|p| !p.is_empty()) {
            let xml = std::fs::read_to_string(path).unwrap();
            let s = ingest_report_file(&r, &xml).unwrap();
            eprintln!(
                "ingested {path}: {} cases, {} claims recorded, {} unmatched, {} ambiguous",
                s.cases,
                s.recorded,
                s.report.unmatched_cases,
                s.report.ambiguous.len()
            );
        }
        if let Ok(path) = std::env::var("MUTANTS") {
            let m = ingest_mutation_file(&r, &std::fs::read_to_string(path).unwrap()).unwrap();
            eprintln!("mutants: {m:?}");
        }
        let (tree, status) = read_derived_status(&r).unwrap();
        let t = &status.totals;
        eprintln!(
            "claims {} — proven {}, stale {}, failing {}, planned {}",
            t.claims, t.proven, t.stale, t.failing, t.planned
        );
        eprintln!(
            "untested components {}, surviving mutants {}, style violations {}, dark files {}",
            t.untested_components, t.surviving_mutants, t.style_violations, t.dark_files
        );
        use scryer_core::derived_tree::ProbeMark;
        let marks = |m: ProbeMark| status.claims.iter().filter(|c| c.probe == Some(m)).count();
        let node_marks = |m: ProbeMark| status.nodes.values().filter(|n| n.probe == Some(m)).count();
        eprintln!(
            "probed claims: {} crosshair, {} double check; probed nodes: {} crosshair, {} double check",
            marks(ProbeMark::Crosshair),
            marks(ProbeMark::DoubleCheck),
            node_marks(ProbeMark::Crosshair),
            node_marks(ProbeMark::DoubleCheck)
        );
        let (_, by_mtime) = scryer_core::derived_tree::derived_status(r.project_path()).unwrap();
        eprintln!(
            "core's mtime-only read: proven {}, stale {}",
            by_mtime.totals.proven, by_mtime.totals.stale
        );
        let mut kinds: BTreeMap<&str, usize> = BTreeMap::new();
        for v in &tree.style_violations {
            *kinds.entry(v.kind.as_str()).or_default() += 1;
        }
        eprintln!("style violations by kind: {kinds:?}");
    }
}
