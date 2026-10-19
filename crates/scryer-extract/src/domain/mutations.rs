//! Mutation-testing reports read as probe evidence. cargo-mutants says which
//! mutants in which file were caught or missed, so its verdict lands on the
//! file's component; Stryker also says which tests killed each mutant
//! (`killedBy`) and which covered a survivor (`coveredBy`), so it lands per
//! test as well.

use scryer_core::verdicts::MutantTally;
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;

/// One report's results, paths as the tool spelled them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ParsedMutations {
    pub tool: &'static str,
    pub files: BTreeMap<String, MutantTally>,
    pub tests: BTreeMap<String, MutantTally>,
}

/// What one ingested mutation report amounted to.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MutationSummary {
    pub tool: String,
    pub mutants: u32,
    pub caught: u32,
    pub survived: u32,
    pub files: usize,
    pub tests: usize,
}

impl ParsedMutations {
    pub fn summary(&self) -> MutationSummary {
        let caught = self.files.values().map(|t| t.caught).sum();
        let survived = self.files.values().map(|t| t.survived).sum();
        MutationSummary {
            tool: self.tool.to_string(),
            mutants: caught + survived,
            caught,
            survived,
            files: self.files.len(),
            tests: self.tests.len(),
        }
    }
}

/// Parse a cargo-mutants `outcomes.json` or a Stryker `mutation.json`,
/// telling them apart by shape.
pub fn parse_mutation_report(text: &str) -> Result<ParsedMutations, String> {
    let v: Value = serde_json::from_str(text).map_err(|e| format!("mutation report is not JSON: {e}"))?;
    if v.get("outcomes").is_some_and(Value::is_array) {
        Ok(parse_cargo_mutants(&v))
    } else if v.get("files").is_some_and(Value::is_object) {
        Ok(parse_stryker(&v))
    } else {
        Err("unrecognised mutation report: expected cargo-mutants outcomes.json or Stryker mutation.json".into())
    }
}

fn parse_cargo_mutants(v: &Value) -> ParsedMutations {
    let mut out = ParsedMutations { tool: "cargo-mutants", ..Default::default() };
    for outcome in v["outcomes"].as_array().into_iter().flatten() {
        let Some(mutant) = outcome["scenario"].get("Mutant") else { continue };
        let Some(file) = mutant["file"].as_str() else { continue };
        let tally = out.files.entry(file.to_string()).or_default();
        match outcome["summary"].as_str().unwrap_or_default() {
            "CaughtMutant" | "Timeout" => tally.caught += 1,
            "MissedMutant" => {
                tally.survived += 1;
                let line = mutant["span"]["start"]["line"].as_u64().unwrap_or(0);
                let function = mutant["function"]["function_name"].as_str().unwrap_or("?");
                let replacement = mutant["replacement"].as_str().unwrap_or("?");
                tally.survivors.push(format!("{file}:{line}: replace {function} with {replacement}"));
            }
            _ => {}
        }
    }
    out.files.retain(|_, t| t.caught + t.survived > 0);
    out
}

fn parse_stryker(v: &Value) -> ParsedMutations {
    let mut out = ParsedMutations { tool: "stryker", ..Default::default() };
    let mut test_names: BTreeMap<&str, &str> = BTreeMap::new();
    for file in v["testFiles"].as_object().into_iter().flat_map(|o| o.values()) {
        for test in file["tests"].as_array().into_iter().flatten() {
            if let (Some(id), Some(name)) = (test["id"].as_str(), test["name"].as_str()) {
                test_names.insert(id, name);
            }
        }
    }
    let name_of = |id: &Value| -> Option<String> {
        let id = id.as_str()?;
        Some(test_names.get(id).copied().unwrap_or(id).to_string())
    };
    for (path, file) in v["files"].as_object().into_iter().flatten() {
        for mutant in file["mutants"].as_array().into_iter().flatten() {
            let status = mutant["status"].as_str().unwrap_or_default();
            let tally = out.files.entry(path.clone()).or_default();
            match status {
                "Killed" | "Timeout" => {
                    tally.caught += 1;
                    for test in mutant["killedBy"].as_array().into_iter().flatten().filter_map(name_of) {
                        out.tests.entry(test).or_default().caught += 1;
                    }
                }
                "Survived" | "NoCoverage" => {
                    tally.survived += 1;
                    let line = mutant["location"]["start"]["line"].as_u64().unwrap_or(0);
                    let mutator = mutant["mutatorName"].as_str().unwrap_or("?");
                    let replacement = mutant["replacement"].as_str().unwrap_or("");
                    tally.survivors.push(format!("{path}:{line}: {mutator} {replacement}").trim_end().to_string());
                    for test in mutant["coveredBy"].as_array().into_iter().flatten().filter_map(name_of) {
                        out.tests.entry(test).or_default().survived += 1;
                    }
                }
                _ => {}
            }
        }
    }
    out.files.retain(|_, t| t.caught + t.survived > 0);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// When cargo-mutants reports caught and missed mutants, each file's
    /// tally counts them and names the survivors; baselines and unviable
    /// mutants count for nothing.
    #[test]
    fn reads_cargo_mutants_outcomes_per_file() {
        let report = r#"{"outcomes":[
          {"scenario":"Baseline","summary":"Success"},
          {"scenario":{"Mutant":{"package":"core","file":"crates/core/src/draft.rs","function":{"function_name":"save"},"span":{"start":{"line":4,"column":1}},"replacement":"true","genre":"FnValue"}},"summary":"MissedMutant"},
          {"scenario":{"Mutant":{"package":"core","file":"crates/core/src/draft.rs","function":{"function_name":"save"},"span":{"start":{"line":4,"column":1}},"replacement":"false","genre":"FnValue"}},"summary":"CaughtMutant"},
          {"scenario":{"Mutant":{"package":"core","file":"crates/core/src/store.rs","function":{"function_name":"put"},"span":{"start":{"line":2,"column":1}},"replacement":"()","genre":"FnValue"}},"summary":"Unviable"}
        ],"total_mutants":3}"#;
        let p = parse_mutation_report(report).unwrap();
        assert_eq!(p.tool, "cargo-mutants");
        assert_eq!(p.files.len(), 1, "an unviable-only file carries no evidence");
        let draft = &p.files["crates/core/src/draft.rs"];
        assert_eq!((draft.caught, draft.survived), (1, 1));
        assert_eq!(draft.survivors, vec!["crates/core/src/draft.rs:4: replace save with true"]);
        assert!(p.tests.is_empty(), "cargo-mutants does not attribute per test");
        assert_eq!(p.summary().mutants, 2);
    }

    /// When Stryker names the tests that killed or covered each mutant, they
    /// are tallied per test by name, alongside the per-file tally.
    #[test]
    fn reads_stryker_kills_per_test() {
        let report = r#"{"schemaVersion":"1","thresholds":{"high":80,"low":60},
          "files":{"src/tree.ts":{"language":"typescript","source":"","mutants":[
            {"id":"1","mutatorName":"BooleanLiteral","replacement":"false","location":{"start":{"line":3,"column":1},"end":{"line":3,"column":5}},"status":"Killed","killedBy":["t1"],"coveredBy":["t1","t2"]},
            {"id":"2","mutatorName":"ConditionalExpression","replacement":"true","location":{"start":{"line":7,"column":1},"end":{"line":7,"column":5}},"status":"Survived","coveredBy":["t2"]},
            {"id":"3","mutatorName":"StringLiteral","location":{"start":{"line":9,"column":1},"end":{"line":9,"column":5}},"status":"NoCoverage"}
          ]}},
          "testFiles":{"src/tree.test.ts":{"tests":[{"id":"t1","name":"tree hides children"},{"id":"t2","name":"tree scrolls"}]}}}"#;
        let p = parse_mutation_report(report).unwrap();
        assert_eq!(p.tool, "stryker");
        let tree = &p.files["src/tree.ts"];
        assert_eq!((tree.caught, tree.survived), (1, 2));
        assert_eq!(p.tests["tree hides children"], MutantTally { caught: 1, survived: 0, survivors: vec![] });
        assert_eq!(p.tests["tree scrolls"].survived, 1);
    }

    /// Anything that is neither shape is refused, not read as zero mutants.
    #[test]
    fn refuses_unknown_reports() {
        assert!(parse_mutation_report(r#"{"mutants":[]}"#).is_err());
        assert!(parse_mutation_report("not json").is_err());
    }
}
