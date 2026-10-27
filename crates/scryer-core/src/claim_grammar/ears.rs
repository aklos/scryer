//! The EARS grammar's deterministic slice (rule `statement-ears`): the pattern
//! classifier and the lint of a behaviour test's description.
//!
//! A claim's pattern is decidable from its leading keyword alone (the grammar
//! fixes clause order: condition first, response last), so neither needs
//! semantic judgment. Statements may carry display markup (`**bold**` on the
//! keyword); classification reads through it. The event, state and unwanted
//! forms count as testable — a test can arrange the condition and assert the
//! response — which is what the health counters' testable/untested split reads.

/// The EARS form of one responsibility statement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EarsPattern {
    /// No leading keyword — always-active, verb-led.
    Ubiquitous,
    /// `When <trigger>, …` — fires on an event.
    Event,
    /// `While <state>, …` — holds during a state.
    State,
    /// `If <condition>, then …` — failure/rejection handling.
    Unwanted,
    /// `Where <feature>, …` — feature-conditional (rarely earned; rule 21).
    Optional,
}

impl EarsPattern {
    /// Whether the pattern names a concrete trigger, state, or failure — the
    /// forms a test can exercise mechanically (arrange the condition, assert
    /// the response). Ubiquitous claims may still deserve tests; that call
    /// needs judgment and stays with the agent.
    pub fn testable(self) -> bool {
        matches!(self, Self::Event | Self::State | Self::Unwanted)
    }
}

/// Classify a statement by its leading keyword, reading through a leading
/// `**` marker. The keyword must end at a word boundary (whitespace or its
/// closing marker) — "Whenever…" is not "When".
pub fn classify(statement: &str) -> EarsPattern {
    let s = statement.trim_start();
    let s = s.strip_prefix("**").unwrap_or(s);
    for (kw, pattern) in [
        ("while", EarsPattern::State),
        ("when", EarsPattern::Event),
        ("if", EarsPattern::Unwanted),
        ("where", EarsPattern::Optional),
    ] {
        if s.len() > kw.len() && s[..kw.len()].eq_ignore_ascii_case(kw) {
            let rest = &s[kw.len()..];
            if rest.starts_with("**") || rest.starts_with(char::is_whitespace) {
                return pattern;
            }
        }
    }
    EarsPattern::Ubiquitous
}

/// One way a test description breaks the claim grammar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EarsIssue {
    /// Stable kebab-case name, for filtering.
    pub code: &'static str,
    pub message: &'static str,
}

/// Lint a behaviour test's description as an EARS claim (concern tags
/// already stripped into `concerns`). Only what the grammar decides
/// mechanically: the clause shape, `shall`, a condition trailing the
/// response, two patterns bundled in one test, more than one concern.
/// Wording quality stays a judgment.
pub fn lint(description: &str, concerns: &[String]) -> Vec<EarsIssue> {
    let mut out = Vec::new();
    let mut flag = |code, message| out.push(EarsIssue { code, message });
    let text = description.trim();
    if text.is_empty() {
        flag("empty", "a claim needs a description: the test's doc comment, `it(\"…\")` string or DisplayName");
        return out;
    }
    let lower = text.to_lowercase();
    let words: Vec<&str> = lower.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).collect();
    if words.contains(&"shall") {
        flag("shall", "drop `shall`: the claim states the response, `When <trigger>, <response>`");
    }
    let pattern = classify(text);
    match pattern {
        EarsPattern::Event | EarsPattern::State | EarsPattern::Unwanted | EarsPattern::Optional => {
            if !text.contains(',') {
                flag("no-response-clause", "a condition needs its response after a comma: `When <trigger>, <response>`");
            } else if pattern == EarsPattern::Unwanted && !lower.contains(", then ") {
                flag("if-without-then", "an unwanted-behaviour claim reads `If <condition>, then <response>`");
            }
        }
        EarsPattern::Ubiquitous => {
            if [" when ", " while ", " if ", " whenever "].iter().any(|kw| lower.contains(kw)) {
                flag("trailing-condition", "move the condition to the front in its keyword form: `When <trigger>, <response>`");
            }
        }
    }
    if [", else ", " otherwise ", ", otherwise"].iter().any(|kw| lower.contains(kw)) {
        flag("bundled-patterns", "a happy path and its rejection are two claims: split the test into the When and the If");
    }
    if concerns.len() > 1 {
        flag("several-concerns", "one concern per claim: a claim that needs two tags bundles two claims");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn codes(description: &str, concerns: &[&str]) -> Vec<&'static str> {
        let concerns: Vec<String> = concerns.iter().map(|c| c.to_string()).collect();
        lint(description, &concerns).into_iter().map(|i| i.code).collect()
    }

    /// Well-formed claims in each pattern pass.
    #[test]
    fn lint_passes_well_formed_claims() {
        for ok in [
            "Rejects an empty title",
            "When a save fails, keeps the draft",
            "While offline, queues each edit",
            "If the signature is invalid, then rejects the request",
            "While a sync runs, when an edit arrives, queues it",
        ] {
            assert!(codes(ok, &["persistence"]).is_empty(), "{ok}: {:?}", codes(ok, &[]));
        }
    }

    /// Each mechanical break of the grammar is named.
    #[test]
    fn lint_names_each_grammar_break() {
        assert_eq!(codes("", &[]), ["empty"]);
        assert_eq!(codes("When a save fails, the store shall keep the draft", &[]), ["shall"]);
        assert_eq!(codes("When a save fails keep the draft", &[]), ["no-response-clause"]);
        assert_eq!(codes("If the token is bad, reject it", &[]), ["if-without-then"]);
        assert_eq!(codes("Keeps the draft when a save fails", &[]), ["trailing-condition"]);
        assert_eq!(codes("When the token matches, echoes it, else rejects", &[]), ["bundled-patterns"]);
        assert_eq!(codes("Keeps the draft", &["persistence", "auth"]), ["several-concerns"]);
    }

    #[test]
    fn classifies_each_keyword_form() {
        assert_eq!(classify("When a callback arrives, append an event"), EarsPattern::Event);
        assert_eq!(classify("While a reconcile runs, queue edits"), EarsPattern::State);
        assert_eq!(classify("If the signature is invalid, then reject"), EarsPattern::Unwanted);
        assert_eq!(classify("Where previews are enabled, render live"), EarsPattern::Optional);
        assert_eq!(classify("Authenticate every inbound POST"), EarsPattern::Ubiquitous);
    }

    #[test]
    fn reads_through_display_markup() {
        assert_eq!(classify("**When** a callback arrives, **append** an event"), EarsPattern::Event);
        assert_eq!(classify("**Authenticate** every inbound POST"), EarsPattern::Ubiquitous);
    }

    #[test]
    fn keyword_needs_a_word_boundary() {
        assert_eq!(classify("Whenever possible, batch the writes"), EarsPattern::Ubiquitous);
        assert_eq!(classify("Whereas the ledger…"), EarsPattern::Ubiquitous);
        assert_eq!(classify("If"), EarsPattern::Ubiquitous);
    }

    #[test]
    fn testable_covers_trigger_state_failure_only() {
        assert!(EarsPattern::Event.testable());
        assert!(EarsPattern::State.testable());
        assert!(EarsPattern::Unwanted.testable());
        assert!(!EarsPattern::Ubiquitous.testable());
        assert!(!EarsPattern::Optional.testable());
    }
}
