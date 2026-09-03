//! Passive JUnit report ingestion — the no-agent feeder of claim test
//! verdicts. Test runs (a terminal, CI, an IDE) drop JUnit XML into the
//! project's report directories; the project watcher routes those file
//! events here and the report is ingested into the claim status cache, so
//! the verdict badges light without anyone calling a tool. The cache write
//! then trips the `.scryer/` watcher's `test-results-changed` event, which
//! is what actually refreshes the UI — this module never talks to the
//! frontend directly.
//!
//! Two deliberate refusals: an unparseable file is skipped quietly (report
//! directories hold screenshots, traces, HTML alongside the XML), and
//! nothing pre-existing is swept when watching begins — a report written
//! before we were looking describes OLDER code, and recording it against
//! the current tree would stamp fresh fingerprints on stale evidence. Only
//! files that change under watch are believed.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Conventional runner output directories, watched when they exist.
/// `.scryer/test-reports` is the neutral home for projects whose runners
/// don't already have one.
const DEFAULT_DIRS: &[&str] = &[
    "test-results",
    "playwright-report",
    "reports",
    ".scryer/test-reports",
];

/// The optional per-project file (`.scryer/config.json`). Only
/// `testReportDirs` is read today; unknown fields pass through untouched.
#[derive(serde::Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct ProjectConfig {
    #[serde(default)]
    test_report_dirs: Vec<String>,
}

/// The report directories to watch: `testReportDirs` from
/// `.scryer/config.json` when it names any, the conventional defaults
/// otherwise — either way keeping only directories that exist right now
/// (notify cannot watch a path that isn't there; one created later is picked
/// up on the next project open).
pub(crate) fn report_dirs(project: &Path) -> Vec<PathBuf> {
    let configured = std::fs::read_to_string(project.join(".scryer").join("config.json"))
        .ok()
        .and_then(|raw| serde_json::from_str::<ProjectConfig>(&raw).ok())
        .map(|c| c.test_report_dirs)
        .filter(|dirs| !dirs.is_empty());
    configured
        .unwrap_or_else(|| DEFAULT_DIRS.iter().map(|s| s.to_string()).collect())
        .iter()
        .map(|d| project.join(d))
        .filter(|p| p.is_dir())
        .collect()
}

/// Ingest one report file into the claim status cache, serialized behind the
/// model lock like every other state write. Anything that doesn't read as
/// JUnit — or a file vanishing mid-settle — is a quiet skip, not an error:
/// report directories are shared territory.
pub(crate) fn ingest_report_file(project: &Path, path: &Path) -> bool {
    let Ok(xml) = std::fs::read_to_string(path) else {
        return false;
    };
    let model_ref = scryer_core::ModelRef::ProjectLocal(project.to_path_buf());
    let Ok(_lock) = scryer_core::lock_model(&model_ref) else {
        return false;
    };
    match scryer_extract::test_status::ingest_report(&model_ref, &xml) {
        Ok(summary) => {
            if summary.recorded > 0 {
                eprintln!(
                    "[test-reports] {} → {} claim verdict(s) from {} case(s)",
                    path.display(),
                    summary.recorded,
                    summary.cases
                );
            }
            summary.recorded > 0
        }
        Err(_) => false,
    }
}

/// Settle-then-ingest scheduling for watcher events. A runner may write a
/// report over several events (and some emit create+modify bursts), so the
/// first event for a file arms one delayed ingest and the burst collapses
/// into it; the delayed read sees the finished file. A file still being
/// written past the delay just fails the parse quietly — the runner's next
/// touch re-arms.
#[derive(Clone)]
pub(crate) struct ReportDebounce {
    pending: Arc<Mutex<HashSet<PathBuf>>>,
    settle: std::time::Duration,
}

impl ReportDebounce {
    pub(crate) fn new(settle: std::time::Duration) -> Self {
        Self {
            pending: Arc::new(Mutex::new(HashSet::new())),
            settle,
        }
    }

    pub(crate) fn schedule(&self, project: PathBuf, path: PathBuf) {
        {
            let mut pending = self.pending.lock().unwrap();
            if !pending.insert(path.clone()) {
                return; // already settling — this event joins the armed ingest
            }
        }
        let pending = Arc::clone(&self.pending);
        let settle = self.settle;
        std::thread::spawn(move || {
            std::thread::sleep(settle);
            pending.lock().unwrap().remove(&path);
            ingest_report_file(&project, &path);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scryer_core::{Kind, ModelRef, Node, Responsibility, ScryModel, SourceLocation};

    const SPEC_TS: &str = "describe(\"alpha\", () => {\n  it(\"answers one\", () => {\n    expect(alpha()).toBe(1);\n  });\n});\n";
    const REPORT: &str = r#"<testsuites><testsuite name="s">
        <testcase classname="src/m.spec.ts" name="alpha &gt; answers one"/>
    </testsuite></testsuites>"#;

    fn project_with_model() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
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
        dir
    }

    #[test]
    fn defaults_keep_only_existing_dirs_and_config_overrides_them() {
        let dir = tempfile::tempdir().unwrap();
        assert!(report_dirs(dir.path()).is_empty(), "nothing exists yet");

        std::fs::create_dir_all(dir.path().join("test-results")).unwrap();
        assert_eq!(report_dirs(dir.path()), vec![dir.path().join("test-results")]);

        // A config naming its own directory replaces the defaults entirely.
        std::fs::create_dir_all(dir.path().join(".scryer")).unwrap();
        std::fs::create_dir_all(dir.path().join("ci-out")).unwrap();
        std::fs::write(
            dir.path().join(".scryer/config.json"),
            r#"{ "testReportDirs": ["ci-out", "missing"] }"#,
        )
        .unwrap();
        assert_eq!(report_dirs(dir.path()), vec![dir.path().join("ci-out")]);
    }

    #[test]
    fn a_junit_file_records_verdicts_and_a_stray_artifact_is_skipped() {
        let dir = project_with_model();
        std::fs::write(dir.path().join("report.xml"), REPORT).unwrap();
        assert!(ingest_report_file(dir.path(), &dir.path().join("report.xml")));
        let r = ModelRef::ProjectLocal(dir.path().to_path_buf());
        let statuses = scryer_extract::test_status::test_statuses(&r).unwrap();
        assert_eq!(statuses.len(), 1);
        assert_eq!(statuses[0].resp_id, "r1");

        // An HTML artifact in the same directory: quietly nothing.
        std::fs::write(dir.path().join("index.html"), "<html>report</html>").unwrap();
        assert!(!ingest_report_file(dir.path(), &dir.path().join("index.html")));
    }

    #[test]
    fn a_burst_of_events_collapses_into_one_settled_ingest() {
        let dir = project_with_model();
        let path = dir.path().join("report.xml");
        std::fs::write(&path, REPORT).unwrap();
        let debounce = ReportDebounce::new(std::time::Duration::from_millis(150));
        for _ in 0..5 {
            debounce.schedule(dir.path().to_path_buf(), path.clone());
        }
        std::thread::sleep(std::time::Duration::from_millis(600));
        let r = ModelRef::ProjectLocal(dir.path().to_path_buf());
        let statuses = scryer_extract::test_status::test_statuses(&r).unwrap();
        assert_eq!(statuses.len(), 1, "the burst produced one recorded verdict set");
    }
}
