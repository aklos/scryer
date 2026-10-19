//! The rules for writing claims — the reference text the MCP's instructions
//! and tool descriptions are condensed from. A claim is a behaviour test's
//! description; these say how to word it, where it goes, and how to check
//! that its test has teeth. Each rule is addressable by `slug`; a body cites
//! another as `[[slug]]`.
//!
//! Holds the full numbered set of modeling rules (business-statement discipline, symbols, styles, sign-off, etc.) as the single source agents fetch guidance from.
//!
//! Extracts every [[slug]] cross-reference from a rule's body, for wiring validation.
//!
//! Resolves one rule by its slug, falling back to a numeric id so a stale citation still resolves.
//!
//! Looks up rules by slug or free-text topic, ranked by relevance, so an agent can pull exactly the guidance a task needs without assuming conventions.
//!
//! Ranks rules by free-text topic relevance — a whole task sentence is valid input — tokenizing, stemming, and matching against titles and tags.
//!
//! The complete authored ruleset — every modeling convention agents must fetch rather than assume, from responsibility grammar to styles to sign-off.

/// One rule. `title` is the headline, `body` the full guidance.
pub struct Rule {
    pub id: u8,
    /// Stable address. Never renumbered, never reused.
    pub slug: &'static str,
    pub title: &'static str,
    pub body: &'static str,
}

pub const RULES: &[Rule] = &[
    Rule {
        id: 17,
        slug: "naming",
        title: "Names and language: simple, clear, concise",
        body: r#"Every claim, module description and display name must be immediately legible to a reader who does NOT already know the system. Prefer the plainest word that is still precise: simple over clever, concrete over abstract, short over long. Say what a thing IS in the domain's own vocabulary — no coinages, internal codenames or needless abbreviations — and use the SAME term for the same concept everywhere. Cut every word that doesn't change the meaning. If a reader has to pause to decode a name or read a sentence twice, rewrite it. A test's identifier stays the code's; the description beside it is where clarity is owed."#,
    },
    Rule {
        id: 20,
        slug: "concerns",
        title: "Concerns tag cross-cutting accountability — at most one per claim",
        body: r#"A claim may carry ONE concern: a kebab-case `[tag]` at the end of its test description naming the cross-cutting concern it serves — `When a save fails, keeps the draft [persistence]`. The standard vocabulary: `auth`, `persistence`, `failure-handling`, `idempotency`, `validation`, `observability`, `performance`, `compliance`. Leave a component's core domain flow UNTAGGED: no tag means "this is the main behaviour". Reuse before minting, and use the same slug for the same concern everywhere ([[naming]]); a claim that seems to need two tags bundles two claims — split the test. The tag carries the category, so purpose clauses like "…so duplicates aren't reprocessed" are redundant: cut them."#,
    },
    Rule {
        id: 21,
        slug: "statement-ears",
        title: "A test description speaks EARS — condition first, response last",
        body: r#"A behaviour test's description is a claim in EARS (Easy Approach to Requirements Syntax) with the subject dropped: the code under test IS the subject, so never name it and never write "shall". Clause order is FIXED — optional condition first, verb-led response last:
- Ubiquitous (always true): no keyword, just the verb-led response. "Rejects an empty title."
- Event-driven: "When <trigger>, <response>."
- State-driven: "While <state>, <response>."
- Unwanted behaviour: "If <condition>, then <response>."
Clauses stack in that order when a claim genuinely needs both: "While <state>, when <trigger>, <response>". The ubiquitous form is earned, never the default: a claim that holds only on a trigger, state or failure takes the keyword form, and a condition written as a tail moves to the front. One pattern and one response per test: a happy path and its rejection are two tests, the When and the If; two verbs on two objects are two tests. A rationale tail is not part of the response — cut it, or move it to the module description. Descriptive, non-testable text ("holds the user's draft") is a module doc comment, not a test."#,
    },
    Rule {
        id: 49,
        slug: "probe-loop",
        title: "Probing a test: would it actually fail if the code stopped honouring its claim?",
        body: r#"A green test says it passes, not that it would notice a defect; a test that asserts nothing passes forever. `open_probe {test}` answers that: before a session stops, probe up to two tests the session wrote or changed. Run it YOURSELF, never through a subagent — a helper may lack permission to run the tests in the worktree, or lose the worktree instruction and revert the developer's files. Nothing happens in the developer's tree: scryer syncs an isolated git worktree (their uncommitted work included) and returns its path, the claim, and the files to break. Make ONE deliberate breaking edit there aimed at what the claim SAYS — its trigger, its response — run only that test, and expect RED. Green means the break survived: the test does not hold the claim, and that is the finding. Up to three breaks, stopping at the first survivor, then `close_probe {test, probes, survivors}` — always, including when a probe went wrong: it resets the worktree and records the round. NEVER revert a break or run git that discards changes (`checkout`, `restore`, `stash`, `reset --hard`, `clean`): leave the last break for `close_probe`, which refuses a round whose worktree matches the developer's code. While a probe is open the hook refuses discarding git outside the worktree. No survivors means probed, not proven. A survivor is a finding: strengthen the test, re-run it, probe again. Refused for a todo test, a failing or stale one, or outside a git repository."#,
    },
    Rule {
        id: 52,
        slug: "styles",
        title: "A container declares the style its code actually has; violations are fixed by moving code",
        body: r#"A style says what KIND of thing a container is and what ROLE each component plays inside it. It is declared in `.scryer/structure.toml` under `[containers.<name>]` (`style`, and `layers` to name them) only when the code ACTUALLY has that shape: `hexagonal` for services and backends (composition, presentation, infrastructure, application, domain); `library` for crates and packages other containers import, with hexagonal's layers entered at composition, application or domain; `feature-sliced` for SPAs and static sites (app, pages, widgets, features, entities, shared); `core-shell` for CLIs and small libraries (shell, core); `pipeline` for ETL and dbt work (source, staging, intermediate, marts); `ecs` for simulations built as entities, components, events and systems; `mvvm` for UI clients (view, viewmodel, model). A project may define its own style as `.scryer/styles/<name>.json`. Never guess a style to make the map look organised: a container with no recognisable shape stays unstyled, and choosing the shape it SHOULD take is a refactoring done as its own change.
Under a style, a component's layer is its directory, and the real import graph is checked against the layer matrix (in hexagonal: domain depends on nothing outside domain; application on domain; infrastructure on application and domain; presentation on application; composition on every layer, and nothing on composition). A forbidden import, a banned package, a file on another layer's path and an import cycle between components are STRUCTURAL VIOLATIONS. Fix them in the CODE: invert the dependency through a port the inner layer owns, move adapter construction into composition, move the file to its layer's directory. Never by un-declaring the style, dropping a layer, or relabelling a component to whichever layer makes the import legal."#,
    },
    Rule {
        id: 53,
        slug: "test-levels",
        title: "A claim's C4 level is its test's level",
        body: r#"Where a test lives decides which node its claim belongs to. An e2e/acceptance test claims for the whole SYSTEM; an integration test at a deployable's boundary claims for its CONTAINER (a crate, package or project); a unit test claims for its COMPONENT (the module or directory it sits in, or the one it tests). Code symbols carry no claims of their own. `.scryer/structure.toml`'s `[levels]` globs say which paths are e2e and integration (defaults: `**/e2e/**`, `**/integration/**`, and a Cargo crate's `tests/`); everything else is unit. Decompose by placement: write a claim at the level it SPEAKS for — a system behaviour goes in an e2e test even when it exercises one container — and never repeat a claim at several levels. To plan work, write each claim you will satisfy as a todo test (`it.todo`, `#[ignore]` or a `todo!()` body, `pytest.mark.skip`, `[Fact(Skip=…)]`) in the test file `orient` names; a todo test is work to do, finished when it passes. Word it per [[statement-ears]]."#,
    },
];

/// The whole ruleset as one block — for review prompts and any consumer that
/// wants the complete text.
pub fn rules_full() -> String {
    let mut s = String::new();
    for r in RULES {
        s.push_str(&format!("{} — {}. {}\n", r.slug, r.title, r.body));
    }
    s
}

/// Resolve one rule by its slug, or by its number.
pub fn get(slug: &str) -> Option<&'static Rule> {
    let key = slug.trim();
    if let Ok(n) = key.parse::<u8>() {
        return RULES.iter().find(|r| r.id == n);
    }
    RULES.iter().find(|r| r.slug == key)
}

/// Every `[[slug]]` citation in a body of text, in order of appearance.
pub fn citations(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("[[") {
        let after = &rest[start + 2..];
        match after.find("]]") {
            Some(end) => {
                out.push(&after[..end]);
                rest = &after[end + 2..];
            }
            None => break,
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The six rules, each reachable by slug and number.
    #[test]
    fn the_six_rules_resolve_by_slug_and_number() {
        let slugs: Vec<&str> = RULES.iter().map(|r| r.slug).collect();
        assert_eq!(slugs, ["naming", "concerns", "statement-ears", "probe-loop", "styles", "test-levels"]);
        for r in RULES {
            assert_eq!(get(r.slug).map(|x| x.id), Some(r.id));
            assert_eq!(get(&r.id.to_string()).map(|x| x.slug), Some(r.slug));
        }
        assert!(get("loop-plan").is_none(), "the loop rules are gone");
    }

    /// Every citation in a body names a rule that exists.
    #[test]
    fn every_body_citation_resolves() {
        for r in RULES {
            for c in citations(r.body) {
                assert!(get(c).is_some(), "rule {} cites unknown [[{}]]", r.slug, c);
            }
        }
        assert_eq!(citations("a [[x]] b [[y-z]]"), vec!["x", "y-z"]);
        assert!(citations("no cite [[dangling").is_empty());
    }

    /// The full render carries every body.
    #[test]
    fn full_renders_every_body() {
        let full = rules_full();
        for r in RULES {
            assert!(full.contains(r.body));
        }
    }
}
