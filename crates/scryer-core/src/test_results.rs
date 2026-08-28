//! Ingests runner-emitted JUnit XML and matches each test case back to the
//! model's attached tests — the read side of claim-level test status.
//!
//! Scryer still never runs tests: a run (by the dev, an agent, or CI) leaves a
//! JUnit report behind, and this module reads the receipt. JUnit is the one
//! format every supported ecosystem can emit natively or with one flag, but
//! its conventions vary per emitter — vitest puts file paths in `classname`
//! and joins suites into `name` with `" > "`; pytest puts dotted module paths
//! in `classname` and parametrizes names with `[…]` suffixes. Matching is
//! therefore name-first (normalized leaf test name), with the attachment's
//! stored file path as tie-breaker only — reports are rooted at the
//! invocation directory, so paths never compare exactly.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::model::SourceLocation;

// --- Parsed report ---

/// One test case's result, ordered so `max()` is the worst outcome.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "camelCase")]
pub enum TestOutcome {
    Passed,
    Skipped,
    Failed,
    /// The runner never got to assert anything — import/collection breakage,
    /// harness crash. Distinct from `Failed` so broken never reads as red on
    /// the claim's own terms, and never as green at all.
    Errored,
}

/// One `<testcase>` as the report stated it, conventions untouched.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TestCase {
    /// The `classname` attribute: a file path (vitest), a dotted module path
    /// (pytest, JUnit proper), or empty (collection errors).
    pub classname: String,
    /// The `name` attribute: bare (pytest), or a `" > "`-joined suite path
    /// (vitest), possibly with a `[param]` suffix.
    pub name: String,
    pub outcome: TestOutcome,
    /// The failure/error message, when the report carried one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Parse a JUnit XML document into flat test cases.
///
/// Accepts both a `<testsuites>` root and a bare `<testsuite>` root, and
/// walks suites at any nesting depth. A malformed document or a root that
/// isn't JUnit at all is an error — an empty report must never look like a
/// healthy run.
pub fn parse_junit(xml: &str) -> Result<Vec<TestCase>, String> {
    let doc = roxmltree::Document::parse(xml).map_err(|e| format!("not well-formed XML: {e}"))?;
    let root = doc.root_element();
    if !matches!(root.tag_name().name(), "testsuites" | "testsuite") {
        return Err(format!(
            "not a JUnit report: root element is <{}>, expected <testsuites> or <testsuite>",
            root.tag_name().name()
        ));
    }
    let mut cases = Vec::new();
    for node in root.descendants().filter(|n| n.has_tag_name("testcase")) {
        let mut outcome = TestOutcome::Passed;
        let mut message = None;
        for child in node.children().filter(|c| c.is_element()) {
            let verdict = match child.tag_name().name() {
                "error" => TestOutcome::Errored,
                "failure" => TestOutcome::Failed,
                "skipped" => TestOutcome::Skipped,
                _ => continue,
            };
            outcome = outcome.max(verdict);
            if message.is_none() {
                message = child
                    .attribute("message")
                    .map(str::to_string)
                    .or_else(|| child.text().map(|t| t.trim().to_string()));
            }
        }
        cases.push(TestCase {
            classname: node.attribute("classname").unwrap_or_default().to_string(),
            name: node.attribute("name").unwrap_or_default().to_string(),
            outcome,
            message,
        });
    }
    Ok(cases)
}

// --- Matching report cases to attached tests ---

/// A claim's aggregated verdict from one report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ClaimOutcome {
    /// Worst outcome across every case that matched this claim's attachments
    /// (a parametrized test is one attachment, many cases).
    pub outcome: TestOutcome,
    /// How many report cases fed the verdict.
    pub cases: usize,
}

/// An attached test the report never mentioned. Normal for a partial run
/// (one runner of several) — the caller decides what silence means.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UnseenAttachment {
    pub resp_id: String,
    pub pattern: String,
    /// `None` means the attachment stores no test name at all, so no report
    /// could ever match it — rot of a different kind than a renamed test.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
}

/// A case whose name matched attachments in several files and whose file
/// hint couldn't break the tie. Surfaced, never guessed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AmbiguousCase {
    pub case: TestCase,
    /// The distinct attachment files that claimed the name.
    pub candidates: Vec<String>,
}

/// Everything one report says about the model's attached tests.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReportMatch {
    /// Responsibility id → aggregated verdict.
    pub claims: HashMap<String, ClaimOutcome>,
    /// Report cases naming no attached test. Expected to be most of any
    /// suite — attachment is curated, the suite is not.
    pub unmatched_cases: usize,
    /// Attachments the report never mentioned.
    pub unseen: Vec<UnseenAttachment>,
    /// Cases that matched several files' attachments irresolvably.
    pub ambiguous: Vec<AmbiguousCase>,
}

/// Match parsed cases against the model's `test_map`, aggregating to
/// per-claim outcomes. Name-first: a case matches an attachment when their
/// normalized leaf names agree; the stored file path only breaks ties.
pub fn match_report(
    test_map: &HashMap<String, Vec<SourceLocation>>,
    cases: &[TestCase],
) -> ReportMatch {
    // Index attachments by normalized leaf name. One test may be attached to
    // several claims — all of them receive its outcome.
    struct Attachment<'a> {
        resp_id: &'a str,
        pattern: &'a str,
    }
    let mut by_name: HashMap<String, Vec<Attachment>> = HashMap::new();
    for (resp_id, locs) in test_map {
        for loc in locs {
            if let Some(symbol) = &loc.symbol {
                by_name
                    .entry(normalize_leaf(symbol))
                    .or_default()
                    .push(Attachment {
                        resp_id,
                        pattern: &loc.pattern,
                    });
            }
        }
    }

    let mut matched: HashMap<(String, String), TestOutcome> = HashMap::new(); // (resp, pattern) → worst
    let mut case_counts: HashMap<String, usize> = HashMap::new();
    let mut seen_names: HashMap<String, bool> = HashMap::new(); // leaf → matched anywhere
    let mut unmatched_cases = 0usize;
    let mut ambiguous = Vec::new();

    for case in cases {
        let leaf = normalize_leaf(&case.name);
        let Some(candidates) = by_name.get(&leaf) else {
            unmatched_cases += 1;
            continue;
        };
        let mut files: Vec<&str> = candidates.iter().map(|a| a.pattern).collect();
        files.sort_unstable();
        files.dedup();
        let chosen: Vec<&Attachment> = if files.len() <= 1 {
            candidates.iter().collect()
        } else {
            // Same name in several files: keep the attachments whose file the
            // case's classname corroborates; refuse to guess past that.
            let surviving: Vec<&Attachment> = candidates
                .iter()
                .filter(|a| hint_names_file(&case.classname, a.pattern))
                .collect();
            let mut surviving_files: Vec<&str> = surviving.iter().map(|a| a.pattern).collect();
            surviving_files.sort_unstable();
            surviving_files.dedup();
            if surviving_files.len() != 1 {
                ambiguous.push(AmbiguousCase {
                    case: case.clone(),
                    candidates: files.iter().map(|f| f.to_string()).collect(),
                });
                continue;
            }
            surviving
        };
        seen_names.insert(leaf, true);
        for a in chosen {
            let worst = matched
                .entry((a.resp_id.to_string(), a.pattern.to_string()))
                .or_insert(case.outcome);
            *worst = (*worst).max(case.outcome);
            *case_counts.entry(a.resp_id.to_string()).or_default() += 1;
        }
    }

    let mut claims: HashMap<String, ClaimOutcome> = HashMap::new();
    for ((resp_id, _), outcome) in &matched {
        let entry = claims.entry(resp_id.clone()).or_insert(ClaimOutcome {
            outcome: *outcome,
            cases: *case_counts.get(resp_id).unwrap_or(&0),
        });
        entry.outcome = entry.outcome.max(*outcome);
    }

    let mut unseen = Vec::new();
    for (resp_id, locs) in test_map {
        for loc in locs {
            let seen = loc
                .symbol
                .as_ref()
                .is_some_and(|s| seen_names.contains_key(&normalize_leaf(s)));
            if !seen {
                unseen.push(UnseenAttachment {
                    resp_id: resp_id.clone(),
                    pattern: loc.pattern.clone(),
                    symbol: loc.symbol.clone(),
                });
            }
        }
    }
    unseen.sort_by(|a, b| (&a.resp_id, &a.pattern).cmp(&(&b.resp_id, &b.pattern)));

    ReportMatch {
        claims,
        unmatched_cases,
        unseen,
        ambiguous,
    }
}

/// Reduce a test name to its normalized leaf: last `" > "` segment (vitest
/// joins suite paths into `name`), last `::` segment of a whitespace-free
/// Rust path (nextest reports `module::tests::fn_name`; a sentence-style
/// title containing `::` keeps its spaces and is left whole), `[param]`
/// suffix dropped (pytest expands one parametrized test into many ids),
/// typographic quotes folded, and whitespace/case collapsed — real suites
/// drift on exactly these.
fn normalize_leaf(name: &str) -> String {
    let leaf = name.rsplit(" > ").next().unwrap_or(name);
    let leaf = if !leaf.chars().any(char::is_whitespace) {
        leaf.rsplit("::").next().unwrap_or(leaf)
    } else {
        leaf
    };
    let leaf = match leaf.find('[') {
        Some(i) if leaf.ends_with(']') => &leaf[..i],
        _ => leaf,
    };
    let mut out = String::with_capacity(leaf.len());
    let mut pending_space = false;
    for ch in leaf.chars() {
        let ch = match ch {
            '\u{2018}' | '\u{2019}' => '\'',
            '\u{201C}' | '\u{201D}' => '"',
            c => c,
        };
        if ch.is_whitespace() {
            pending_space = !out.is_empty();
            continue;
        }
        if pending_space {
            out.push(' ');
            pending_space = false;
        }
        out.extend(ch.to_lowercase());
    }
    out
}

/// Whether a case's `classname` corroborates an attachment's file path.
/// The hint and the stored path are rooted differently (reports are relative
/// to the invocation directory) and the hint may be dotted rather than
/// slashed, so the one dependable signal is the file's stem: the stored
/// path's final component, extension dropped, appearing among the hint's
/// components.
fn hint_names_file(classname: &str, pattern: &str) -> bool {
    let stem = pattern
        .rsplit('/')
        .next()
        .map(|f| f.split_once('.').map_or(f, |(s, _)| s))
        .unwrap_or(pattern);
    if classname.contains('/') {
        classname
            .split('/')
            .any(|c| c.split_once('.').map_or(c, |(s, _)| s) == stem)
    } else {
        classname.split('.').any(|c| c == stem)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attach(entries: &[(&str, &str, Option<&str>)]) -> HashMap<String, Vec<SourceLocation>> {
        let mut map: HashMap<String, Vec<SourceLocation>> = HashMap::new();
        for (resp, pattern, symbol) in entries {
            map.entry(resp.to_string())
                .or_default()
                .push(SourceLocation {
                    pattern: pattern.to_string(),
                    symbol: symbol.map(str::to_string),
                    line: None,
                    end_line: None,
                });
        }
        map
    }

    // Shaped like vitest's junit reporter: file-path classnames, suite paths
    // joined into `name` with " > ", typographic apostrophes in real names.
    const VITEST_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" ?>
<testsuites name="vitest tests" tests="3" failures="1">
    <testsuite name="tests/unit/sessionKey.spec.ts" tests="3">
        <testcase classname="tests/unit/sessionKey.spec.ts" name="the session key &gt; stays stable across turns" time="0.003"></testcase>
        <testcase classname="tests/unit/sessionKey.spec.ts" name="the session key &gt; carries on when the browser won’t store one" time="0.001"></testcase>
        <testcase classname="tests/unit/sessionKey.spec.ts" name="the session key &gt; keeps two sites apart" time="0.002">
            <failure message="expected 'a' to not equal 'a'">AssertionError</failure>
        </testcase>
    </testsuite>
</testsuites>"#;

    // Shaped like pytest's --junitxml: dotted classnames, parametrized ids,
    // and a collection error for a module that failed to import.
    const PYTEST_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<testsuites><testsuite name="pytest" errors="1" failures="0" tests="4">
    <testcase classname="tests.test_voice.TestVoice" name="test_rules_out_filler[um]" time="0.001" />
    <testcase classname="tests.test_voice.TestVoice" name="test_rules_out_filler[ah]" time="0.001" />
    <testcase classname="tests.test_voice.TestVoice" name="test_keeps_the_greeting" time="0.001" />
    <testcase classname="tests.test_matchmaking" name="tests/test_matchmaking.py">
        <error message="collection failure">ImportError: libstdc++.so.6</error>
    </testcase>
</testsuite></testsuites>"#;

    #[test]
    fn parses_nested_and_flat_roots() {
        let nested = parse_junit(VITEST_XML).unwrap();
        assert_eq!(nested.len(), 3);
        let flat =
            parse_junit(r#"<testsuite tests="1"><testcase classname="a.b" name="t"/></testsuite>"#)
                .unwrap();
        assert_eq!(flat.len(), 1);
        assert_eq!(flat[0].outcome, TestOutcome::Passed);
    }

    #[test]
    fn classifies_failure_error_and_skip() {
        let cases = parse_junit(PYTEST_XML).unwrap();
        assert_eq!(cases[0].outcome, TestOutcome::Passed);
        assert_eq!(cases[3].outcome, TestOutcome::Errored);
        assert_eq!(cases[3].message.as_deref(), Some("collection failure"));
        let skipped = parse_junit(
            r#"<testsuite><testcase classname="a" name="t"><skipped/></testcase></testsuite>"#,
        )
        .unwrap();
        assert_eq!(skipped[0].outcome, TestOutcome::Skipped);
    }

    #[test]
    fn rejects_non_junit_documents() {
        assert!(parse_junit("not xml at all").is_err());
        assert!(parse_junit("<report><testcase name='t'/></report>").is_err());
    }

    #[test]
    fn matches_vitest_suite_paths_and_typographic_quotes() {
        // The attachment stores the leaf name with a plain apostrophe; the
        // report emits the typographic one inside a suite path.
        let map = attach(&[(
            "resp-1",
            "tests/unit/sessionKey.spec.ts",
            Some("carries on when the browser won't store one"),
        )]);
        let m = match_report(&map, &parse_junit(VITEST_XML).unwrap());
        assert_eq!(m.claims["resp-1"].outcome, TestOutcome::Passed);
        assert!(m.unseen.is_empty());
        assert_eq!(m.unmatched_cases, 2);
    }

    /// A nextest case names the test `module::tests::fn_name`; the attachment
    /// stores the bare function name. The Rust path reduces to its leaf — but
    /// a sentence-style title that happens to contain `::` is left whole.
    #[test]
    fn matches_nextest_rust_module_paths_by_their_leaf() {
        let xml = r#"<testsuites><testsuite name="run">
            <testcase classname="scryer-core" name="storage::tests::stamp_touches_dates_only_truth_changes"/>
            <testcase classname="app" name="reads the config :: with defaults"/>
        </testsuite></testsuites>"#;
        let map = attach(&[
            ("resp-1", "crates/scryer-core/src/storage.rs", Some("stamp_touches_dates_only_truth_changes")),
            ("resp-2", "tests/config.spec.ts", Some("reads the config :: with defaults")),
        ]);
        let m = match_report(&map, &parse_junit(xml).unwrap());
        assert_eq!(m.claims["resp-1"].outcome, TestOutcome::Passed);
        assert_eq!(m.claims["resp-2"].outcome, TestOutcome::Passed, "spaced :: is a title, not a path");
        assert!(m.unseen.is_empty());
    }

    #[test]
    fn aggregates_parametrized_cases_to_the_worst_outcome() {
        let map = attach(&[(
            "resp-2",
            "agent/tests/test_voice.py",
            Some("test_rules_out_filler"),
        )]);
        let failing = PYTEST_XML.replace(
            r#"name="test_rules_out_filler[ah]" time="0.001" />"#,
            r#"name="test_rules_out_filler[ah]"><failure message="boom"/></testcase>"#,
        );
        let m = match_report(&map, &parse_junit(&failing).unwrap());
        let claim = &m.claims["resp-2"];
        assert_eq!(claim.outcome, TestOutcome::Failed);
        assert_eq!(claim.cases, 2);
    }

    #[test]
    fn one_test_attached_to_several_claims_feeds_them_all() {
        let map = attach(&[
            (
                "resp-a",
                "tests/unit/sessionKey.spec.ts",
                Some("keeps two sites apart"),
            ),
            (
                "resp-b",
                "tests/unit/sessionKey.spec.ts",
                Some("keeps two sites apart"),
            ),
        ]);
        let m = match_report(&map, &parse_junit(VITEST_XML).unwrap());
        assert_eq!(m.claims["resp-a"].outcome, TestOutcome::Failed);
        assert_eq!(m.claims["resp-b"].outcome, TestOutcome::Failed);
    }

    #[test]
    fn same_name_across_files_is_broken_by_the_file_hint() {
        // Dotted pytest hint on one side, slashed vitest hint on the other —
        // the stem is the shared signal.
        let map = attach(&[
            (
                "resp-py",
                "agent/tests/test_voice.py",
                Some("test_keeps_the_greeting"),
            ),
            (
                "resp-ts",
                "tests/unit/voice.spec.ts",
                Some("test_keeps_the_greeting"),
            ),
        ]);
        let m = match_report(&map, &parse_junit(PYTEST_XML).unwrap());
        assert_eq!(m.claims.len(), 1);
        assert_eq!(m.claims["resp-py"].outcome, TestOutcome::Passed);
    }

    #[test]
    fn an_unbreakable_tie_is_surfaced_not_guessed() {
        let map = attach(&[
            ("resp-a", "tests/a.spec.ts", Some("round-trips")),
            ("resp-b", "tests/b.spec.ts", Some("round-trips")),
        ]);
        let cases = vec![TestCase {
            classname: String::new(),
            name: "round-trips".into(),
            outcome: TestOutcome::Passed,
            message: None,
        }];
        let m = match_report(&map, &cases);
        assert!(m.claims.is_empty());
        assert_eq!(m.ambiguous.len(), 1);
        assert_eq!(
            m.ambiguous[0].candidates,
            vec!["tests/a.spec.ts", "tests/b.spec.ts"]
        );
    }

    #[test]
    fn absent_and_nameless_attachments_are_reported_unseen() {
        let map = attach(&[
            (
                "resp-rot",
                "tests/unit/renamed.spec.ts",
                Some("a test that no longer exists"),
            ),
            ("resp-bare", "tests/unit/sessionKey.spec.ts", None),
        ]);
        let m = match_report(&map, &parse_junit(VITEST_XML).unwrap());
        assert_eq!(m.unseen.len(), 2);
        assert!(m
            .unseen
            .iter()
            .any(|u| u.resp_id == "resp-rot" && u.symbol.is_some()));
        assert!(m
            .unseen
            .iter()
            .any(|u| u.resp_id == "resp-bare" && u.symbol.is_none()));
    }

    #[test]
    fn a_collection_error_reaches_the_claim_as_errored_never_passing() {
        // pytest reports a module that failed to import as an error testcase
        // whose name is the file path itself.
        let map = attach(&[(
            "resp-m",
            "agent/tests/test_matchmaking.py",
            Some("tests/test_matchmaking.py"),
        )]);
        let m = match_report(&map, &parse_junit(PYTEST_XML).unwrap());
        assert_eq!(m.claims["resp-m"].outcome, TestOutcome::Errored);
    }
}
