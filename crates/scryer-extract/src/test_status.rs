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
use scryer_core::{read_model_at, test_key, ModelRef, ScryModel};
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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestStatusCache {
    #[serde(default)]
    pub results: Vec<ClaimRecord>,
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
    let model = read_model_at(r)?;
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
    let model = read_model_at(r)?;
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

/// One test file the blast radius says to run, and why.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RadiusFile {
    /// The attached test file (as the attachment spells it).
    pub pattern: String,
    /// Distinct run commands the attachments recorded for this file —
    /// advisory, never executed by scryer.
    pub commands: Vec<String>,
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
    let model = read_model_at(r)?;
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
                commands: Vec::new(),
                claims: Vec::new(),
                stale: 0,
            });
            if let Some(cmd) = &loc.command {
                if !entry.commands.contains(cmd) {
                    entry.commands.push(cmd.clone());
                }
            }
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
    let model = read_model_at(r)?;
    let cases = parse_junit(xml)?;
    let report = match_report(&model.test_map, &cases);
    let recorded = record_test_results(r, &report)?;
    Ok(IngestSummary { cases: cases.len(), recorded, report })
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
                command: None,
            }],
        );
        m.test_map.insert(
            "r1".into(),
            vec![SourceLocation {
                pattern: "src/m.spec.ts".into(),
                symbol: Some("answers one".into()),
                line: None,
                end_line: None,
                command: None,
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
            command: None,
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
            command: None,
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
    fn radius_groups_claims_per_file_with_their_commands() {
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
                command: Some("pnpm test".into()),
            }],
        );
        scryer_core::write_model_at(&r, &m).unwrap();
        let radius = test_blast_radius(&r).unwrap();
        assert_eq!(radius.len(), 1, "one file, one invocation: {radius:?}");
        assert_eq!(radius[0].claims, vec!["r1", "r2"]);
        assert_eq!(radius[0].commands, vec!["pnpm test"]);
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
}
