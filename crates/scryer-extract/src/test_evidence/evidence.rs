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

/// One line of the test-command file: the file globs whose tests it runs
/// (`[*.rs] cargo nextest …`), or none for the bare line that runs the rest.
#[derive(Debug, Clone, PartialEq)]
pub struct RunnerCommand {
    pub globs: Vec<String>,
    pub template: String,
}

/// Read the test-command file: one runner per line, `[glob glob] template`
/// or a bare template. A mixed project needs one per runner: vitest can't
/// run a `.rs` file, nor cargo a `.ts` one.
pub fn parse_test_commands(raw: &str) -> Vec<RunnerCommand> {
    raw.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| match l.strip_prefix('[').and_then(|r| r.split_once(']')) {
            Some((globs, template)) => RunnerCommand {
                globs: globs.split_whitespace().map(str::to_string).collect(),
                template: template.trim().to_string(),
            },
            None => RunnerCommand { globs: Vec::new(), template: l.to_string() },
        })
        .collect()
}

/// The test-command file with `line` set: it replaces the line naming the
/// same globs (the bare line for a bare one) and keeps the others.
pub fn set_runner_command(raw: &str, line: &str) -> String {
    let new = parse_test_commands(line).into_iter().next();
    let mut lines: Vec<String> = Vec::new();
    let mut placed = false;
    for l in raw.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let same = match (&new, parse_test_commands(l).first()) {
            (Some(n), Some(old)) => n.globs == old.globs,
            _ => false,
        };
        if same {
            if !placed {
                lines.push(line.trim().to_string());
                placed = true;
            }
        } else {
            lines.push(l.to_string());
        }
    }
    if !placed {
        lines.push(line.trim().to_string());
    }
    lines.join("\n")
}

/// One command to run for the radius, and the report it writes.
#[derive(Debug, Clone, PartialEq)]
pub struct RadiusRun {
    pub command: String,
    pub report: String,
}

/// Split the radius among the runners: each test goes to the first line
/// whose globs match its file, else to the bare line. Returns the commands
/// in file order and the tests no line runs. With one command its report is
/// `radius.xml`; with several, each is named for its globs (`radius-rs.xml`).
pub fn plan_radius_runs(commands: &[RunnerCommand], run: &[RadiusTest], reports: &str) -> (Vec<RadiusRun>, Vec<RadiusTest>) {
    let matches = |c: &RunnerCommand, file: &str| {
        c.globs.iter().any(|g| glob::Pattern::new(g).is_ok_and(|p| p.matches(file)))
    };
    let mut groups: Vec<Vec<RadiusTest>> = vec![Vec::new(); commands.len()];
    let mut unrun = Vec::new();
    for t in run {
        let at = commands
            .iter()
            .position(|c| matches(c, &t.pattern))
            .or_else(|| commands.iter().position(|c| c.globs.is_empty()));
        match at {
            Some(i) => groups[i].push(t.clone()),
            None => unrun.push(t.clone()),
        }
    }
    let used: Vec<usize> = (0..commands.len()).filter(|&i| !groups[i].is_empty()).collect();
    let runs = used
        .iter()
        .map(|&i| {
            let c = &commands[i];
            let report = if used.len() == 1 || c.globs.is_empty() {
                format!("{reports}/radius.xml")
            } else {
                let slug: String = c
                    .globs
                    .iter()
                    .map(|g| g.chars().filter(|ch| ch.is_ascii_alphanumeric()).collect::<String>())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join("-");
                format!("{reports}/radius-{}.xml", if slug.is_empty() { i.to_string() } else { slug })
            };
            RadiusRun { command: render_test_command(&c.template, &groups[i], &report, reports), report }
        })
        .collect();
    (runs, unrun)
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

    /// When the test command names runners by file glob, renders one command per runner over the radius tests its globs match, each writing its own report.
    #[test]
    fn each_runner_runs_the_tests_its_globs_match() {
        let cmds = parse_test_commands(
            "# one runner per line\n[*.rs] cargo nextest run -E '{names:|}'; cp junit.xml {report}\n[*.ts *.tsx] npx vitest run {files} --outputFile={report}\n",
        );
        let run = [test("src/a.rs", Some("saves")), test("tests/b.test.tsx", Some("x")), test("src/c.rs", Some("loads"))];
        let (runs, unrun) = plan_radius_runs(&cmds, &run, "r");
        assert_eq!(
            runs,
            vec![
                RadiusRun { command: "cargo nextest run -E 'saves|loads'; cp junit.xml r/radius-rs.xml".into(), report: "r/radius-rs.xml".into() },
                RadiusRun { command: "npx vitest run tests/b.test.tsx --outputFile=r/radius-ts-tsx.xml".into(), report: "r/radius-ts-tsx.xml".into() },
            ]
        );
        assert!(unrun.is_empty());
        let (one, _) = plan_radius_runs(&cmds, &run[..1], "r");
        assert_eq!(one[0].report, "r/radius.xml", "a lone runner keeps the plain name");
    }

    /// If a radius test matches no runner's globs, then runs it with the command that names none, or names it unrun when there is none.
    #[test]
    fn a_test_no_glob_matches_falls_back_to_the_bare_command() {
        let run = [test("src/a.rs", Some("saves")), test("tests/b.py", Some("x"))];
        let with_bare = parse_test_commands("[*.rs] cargo t {names}\npytest {files} --junitxml={report}");
        let (runs, unrun) = plan_radius_runs(&with_bare, &run, "r");
        assert_eq!(runs[1].command, "pytest tests/b.py --junitxml=r/radius.xml");
        assert!(unrun.is_empty());
        let (runs, unrun) = plan_radius_runs(&with_bare[..1], &run, "r");
        assert_eq!((runs.len(), unrun), (1, vec![run[1].clone()]));
    }

    /// When a runner's command is set, replaces only the line naming the same globs.
    #[test]
    fn setting_a_runner_replaces_only_its_line() {
        let raw = "npx vitest run {files}";
        let both = set_runner_command(raw, "[*.rs] cargo t {names}");
        assert_eq!(both, "npx vitest run {files}\n[*.rs] cargo t {names}", "a new runner is added");
        assert_eq!(set_runner_command(&both, "[*.rs] cargo nextest {names}"), "npx vitest run {files}\n[*.rs] cargo nextest {names}");
        assert_eq!(set_runner_command(&both, "vitest {files}"), "vitest {files}\n[*.rs] cargo t {names}");
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
