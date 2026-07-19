//! The EARS pattern classifier — the deterministic slice of rule 21.
//!
//! A responsibility statement's pattern is decidable from its leading keyword
//! alone (the grammar fixes clause order: condition first, response last), so
//! classification never needs semantic judgment. Statements may carry display
//! markup (`**bold**` on the keyword); classification reads through it.

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
    /// forms a test can demonstrate mechanically (arrange the condition,
    /// assert the response). Ubiquitous claims may still deserve tests; that
    /// call needs judgment and stays with the agent.
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

#[cfg(test)]
mod tests {
    use super::*;

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
