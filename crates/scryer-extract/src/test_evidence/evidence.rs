//! What a claim's test evidence IS: the cached verdict and probe records
//! (declared in `scryer_core::verdicts`), what one ingested report amounted
//! to, and the radius — the attached tests a change reaches, rendered into
//! the project's test-command template. Reading and writing the cache is
//! `verdict_cache`; the entry points other containers call are
//! in `standing::test_status`.

use scryer_core::test_results::ReportMatch;
use serde::Serialize;

pub use scryer_core::verdicts::{
    predecessor, ClaimProbeStatus, ClaimRecord, ClaimTestStatus, KeptCase, ProbeRecord, ProvenAs,
    TestStatusCache,
};

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

pub(crate) fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// A test the radius would run past this wall time is slow: it stays out of
/// the radius command unless a claim it backs was touched this session.
pub const SLOW_TEST_MILLIS: u64 = 5_000;

/// One attached test in the radius.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RadiusTest {
    /// The test file, as the attachment spells it.
    pub pattern: String,
    /// The test's name; `None` for an attachment that stores none (only its
    /// file can select it).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Radius claims a run of this test would refresh.
    pub claims: Vec<String>,
    /// Its wall time on the last recorded run, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub millis: Option<u64>,
}

/// Fill a test-command template for `run`. `{names}` and `{files}` expand to
/// the tests' names and their distinct files, space-joined; `{names:SEP}` /
/// `{files:SEP}` join with SEP instead, so a per-item prefix rides in the
/// separator (`Name~{names:|Name~}` → `Name~a|Name~b`). `{report}` is the
/// JUnit file to write, `{reports}` its directory. Items are inserted raw:
/// the template owns its quoting.
pub fn render_test_command(template: &str, run: &[RadiusTest], report: &str, reports: &str) -> String {
    let mut names: Vec<&str> = Vec::new();
    let mut files: Vec<&str> = Vec::new();
    for t in run {
        if let Some(n) = t.name.as_deref() {
            if !names.contains(&n) {
                names.push(n);
            }
        }
        if !files.contains(&t.pattern.as_str()) {
            files.push(&t.pattern);
        }
    }
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else {
            out.push_str(&rest[open..]);
            return out;
        };
        let inner = &after[..close];
        let (key, sep) = inner.split_once(':').unwrap_or((inner, " "));
        match key {
            "names" => out.push_str(&names.join(sep)),
            "files" => out.push_str(&files.join(sep)),
            "report" if inner == key => out.push_str(report),
            "reports" if inner == key => out.push_str(reports),
            _ => out.push_str(&rest[open..open + close + 2]),
        }
        rest = &after[close + 1..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test(pattern: &str, name: Option<&str>) -> RadiusTest {
        RadiusTest { pattern: pattern.into(), name: name.map(Into::into), claims: Vec::new(), millis: None }
    }

    #[test]
    fn a_command_template_takes_names_files_and_the_report() {
        let run = [
            test("tests/Shared/a.cs", Some("Docks")),
            test("tests/Shared/a.cs", Some("Undocks")),
            test("tests/Server/b.cs", None),
        ];
        let cmd = render_test_command(
            "tools/test.sh \"FullyQualifiedName~{names:|FullyQualifiedName~}\" {report} -- {files}",
            &run,
            ".scryer/reports/radius.xml",
            ".scryer/reports",
        );
        assert_eq!(
            cmd,
            "tools/test.sh \"FullyQualifiedName~Docks|FullyQualifiedName~Undocks\" \
             .scryer/reports/radius.xml -- tests/Shared/a.cs tests/Server/b.cs"
        );
    }

    #[test]
    fn unknown_placeholders_and_stray_braces_pass_through() {
        let run = [test("t.rs", Some("x"))];
        assert_eq!(
            render_test_command("run {other} {names} {reports} {", &run, "r.xml", "d"),
            "run {other} x d {"
        );
    }
}
