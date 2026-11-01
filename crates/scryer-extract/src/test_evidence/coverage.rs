//! Per-test coverage read as evidence of what code each claim's test runs.
//!
//! The report is LCOV with one `TN:` section per test, named
//! `<classname>$<name>` the way a JUnit report names the test (nextest's
//! binary id and test path), so a section matches its claim exactly as a
//! JUnit case does. A section's `DA:` lines with a count above zero are the
//! lines that test ran; every `DA:` line in any section is an executable
//! line, and a section with an empty `TN:` carries executable lines only.
//! `scryer-mcp coverage run` writes this shape; any producer that names its
//! sections per test can.

use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

/// The lines one test ran, paths as the report spelled them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TestLines {
    pub classname: String,
    pub name: String,
    pub files: BTreeMap<String, BTreeSet<u32>>,
}

/// One report: what each test ran, and every executable line it saw.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ParsedCoverage {
    pub tests: Vec<TestLines>,
    pub executable: BTreeMap<String, BTreeSet<u32>>,
}

/// What one ingested coverage report amounted to.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoverageSummary {
    /// Named sections in the report.
    pub tests: usize,
    /// Claims whose test a section matched.
    pub claims: usize,
    /// Product files with executable lines.
    pub files: usize,
    /// Sections that matched no claim's test.
    pub unmatched: usize,
}

/// A section's test name split into the JUnit classname and name: nextest
/// names a test process `<binary id>$<test path>`.
pub fn split_test_name(tn: &str) -> (String, String) {
    match tn.split_once('$') {
        Some((class, name)) => (class.to_string(), name.to_string()),
        None => (String::new(), tn.to_string()),
    }
}

/// Read an LCOV report section by section. Refuses text with no `SF:`
/// record, so a wrong file is never read as no coverage.
pub fn parse_lcov(text: &str) -> Result<ParsedCoverage, String> {
    let mut out = ParsedCoverage::default();
    let mut by_name: BTreeMap<String, usize> = BTreeMap::new();
    let mut test: Option<usize> = None;
    let mut file: Option<String> = None;
    let mut saw_file = false;
    for line in text.lines().map(str::trim) {
        if let Some(tn) = line.strip_prefix("TN:") {
            let tn = tn.trim();
            test = (!tn.is_empty()).then(|| {
                *by_name.entry(tn.to_string()).or_insert_with(|| {
                    let (classname, name) = split_test_name(tn);
                    out.tests.push(TestLines { classname, name, files: BTreeMap::new() });
                    out.tests.len() - 1
                })
            });
        } else if let Some(sf) = line.strip_prefix("SF:") {
            file = Some(sf.trim().to_string());
            saw_file = true;
        } else if let Some(da) = line.strip_prefix("DA:") {
            let Some(f) = &file else { continue };
            let mut parts = da.split(',');
            let (Some(Ok(n)), Some(Ok(count))) =
                (parts.next().map(str::parse::<u32>), parts.next().map(|c| c.trim().parse::<u64>()))
            else {
                continue;
            };
            out.executable.entry(f.clone()).or_default().insert(n);
            if let (Some(t), true) = (test, count > 0) {
                out.tests[t].files.entry(f.clone()).or_default().insert(n);
            }
        } else if line == "end_of_record" {
            file = None;
        }
    }
    if !saw_file {
        return Err("not an LCOV report: no `SF:` record".into());
    }
    Ok(out)
}

/// Write a report in the shape [`parse_lcov`] reads: an unnamed section with
/// every executable line, then one section per test with only the lines it
/// ran, so a large suite does not repeat every file's untouched lines per test.
pub fn write_lcov(parsed: &ParsedCoverage) -> String {
    let mut out = String::new();
    let section = |out: &mut String, tn: &str, files: &BTreeMap<String, BTreeSet<u32>>, count: u8| {
        for (file, lines) in files.iter().filter(|(_, l)| !l.is_empty()) {
            out.push_str(&format!("TN:{tn}\nSF:{file}\n"));
            for n in lines {
                out.push_str(&format!("DA:{n},{count}\n"));
            }
            out.push_str("end_of_record\n");
        }
    };
    section(&mut out, "", &parsed.executable, 0);
    for t in &parsed.tests {
        let tn = if t.classname.is_empty() { t.name.clone() } else { format!("{}${}", t.classname, t.name) };
        section(&mut out, &tn, &t.files, 1);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(ns: &[u32]) -> BTreeSet<u32> {
        ns.iter().copied().collect()
    }

    /// Reads each test's run lines from its own named section, and every `DA:` line as executable.
    #[test]
    fn reads_each_tests_lines_from_its_section() {
        let lcov = "TN:gate$api::tests::accepts\nSF:/p/src/api.rs\nDA:3,1\nDA:4,0\nend_of_record\n\
                    TN:gate$billing::tests::charges\nSF:/p/src/billing.rs\nDA:2,5\nend_of_record\n\
                    TN:gate$api::tests::accepts\nSF:/p/src/session.rs\nDA:2,1\nend_of_record\n";
        let p = parse_lcov(lcov).unwrap();
        assert_eq!(p.tests.len(), 2, "a test's sections fold into one");
        let accepts = &p.tests[0];
        assert_eq!((accepts.classname.as_str(), accepts.name.as_str()), ("gate", "api::tests::accepts"));
        assert_eq!(accepts.files["/p/src/api.rs"], lines(&[3]), "an unrun line is not the test's");
        assert_eq!(accepts.files["/p/src/session.rs"], lines(&[2]));
        assert_eq!(p.executable["/p/src/api.rs"], lines(&[3, 4]));
    }

    /// Refuses text with no source file record rather than reading it as no coverage.
    #[test]
    fn refuses_text_that_is_not_lcov() {
        assert!(parse_lcov("<testsuites/>").is_err());
    }

    /// Writes executable lines once, then each test's run lines, in a shape it reads back unchanged.
    #[test]
    fn writes_a_report_it_reads_back() {
        let mut p = ParsedCoverage::default();
        p.executable.insert("src/a.rs".into(), lines(&[1, 2, 3]));
        p.tests.push(TestLines {
            classname: "gate".into(),
            name: "a::tests::one".into(),
            files: BTreeMap::from([("src/a.rs".to_string(), lines(&[2]))]),
        });
        let text = write_lcov(&p);
        assert_eq!(text.matches("DA:").count(), 4, "{text}");
        assert_eq!(parse_lcov(&text).unwrap(), p);
    }
}
