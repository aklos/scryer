//! Cross-cutting concerns: the trailing `[tags]` on claims, the model's third axis.
//!
//! The containment tree answers "what's inside X?", links answer "what talks to
//! what?"; a concern answers "where does auth (persistence, idempotency, …)
//! live?". Code answers that with factoring plus convention, so the tree can't
//! hold it; a concern is a LENS over the claims instead. A claim says which
//! concerns it speaks for by tagging its test, `… [auth]` or, where one
//! behaviour serves two, `… [auth] [idempotency]`, and that tag is the
//! whole of a concern: there is no registry beside the claims,
//! so nothing to curate and nothing that can disagree with the tests. The
//! concerns a model has are the tags its claims carry.
//!
//! A tag is normalized to a kebab-case slug so the vocabulary can't fork on
//! casing or spacing. A `key:value` tag is an internal marker, not a concern.
//! Descriptions and icons for the common slugs are presentation and live with
//! the UI.

/// Normalize a raw concern value to a kebab-case slug: lowercase, every run of
/// non-alphanumerics collapses to one hyphen, leading/trailing hyphens trimmed.
/// Returns an empty string for input with no alphanumerics — callers treat
/// that as "no concern".
pub fn normalize_slug(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut pending_hyphen = false;
    for c in raw.chars() {
        if c.is_alphanumeric() {
            if pending_hyphen && !out.is_empty() {
                out.push('-');
            }
            pending_hyphen = false;
            out.extend(c.to_lowercase());
        } else {
            pending_hyphen = true;
        }
    }
    out
}

/// The concerns a claim speaks for, from its test's trailing tags: each
/// normalized, in order of appearance, once each. Internal `key:value`
/// markers and tags with no alphanumerics are not concerns.
pub fn concern_tags(tags: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for slug in tags.iter().filter(|t| !t.contains(':')).map(|t| normalize_slug(t)) {
        if !slug.is_empty() && !out.contains(&slug) {
            out.push(slug);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(raw: &[&str]) -> Vec<String> {
        raw.iter().map(|t| t.to_string()).collect()
    }

    #[test]
    fn normalize_slug_kebabs_and_collapses() {
        assert_eq!(normalize_slug("Auth"), "auth");
        assert_eq!(normalize_slug("Failure  Handling"), "failure-handling");
        assert_eq!(normalize_slug("--rate__limiting--"), "rate-limiting");
        assert_eq!(normalize_slug("  "), "");
    }

    /// Keeps every concern tag a claim carries, normalized, in order, once each.
    #[test]
    fn keeps_every_tag_normalized() {
        assert_eq!(
            concern_tags(&tags(&["Auth", "Failure Handling", "auth", "idempotency"])),
            tags(&["auth", "failure-handling", "idempotency"])
        );
    }

    /// Drops internal `key:value` markers and tags that normalize to nothing.
    #[test]
    fn drops_markers_and_empty_tags() {
        assert_eq!(concern_tags(&tags(&["session:ab12", "!!", "io"])), tags(&["io"]));
        assert!(concern_tags(&[]).is_empty());
    }
}
