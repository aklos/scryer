//! Modeling rules — the single source of truth for MCP instructions and AI
//! review prompts. Each rule is an addressable entry (`id`, `title`, `tags`,
//! `body`) so an agent can pull just the guidance it needs via
//! `get_rules{topic}` instead of carrying the whole set in context. The full
//! text is rendered for review prompts via [`rules_full`]; a compact, drillable
//! index is rendered for connect-time instructions via [`rules_index`].

/// One modeling rule. `title` is the headline (also the first match target),
/// `tags` are the curated lookup keywords surfaced in the index, `body` is the
/// full guidance returned when the rule is pulled.
pub struct Rule {
    pub id: u8,
    pub title: &'static str,
    pub tags: &'static [&'static str],
    pub body: &'static str,
}

pub const RULES: &[Rule] = &[
    Rule {
        id: 1,
        title: "Responsibilities are pure business statements",
        tags: &["responsibility", "business", "mechanism", "directives", "wording"],
        body: r#"A responsibility says what a node is accountable for in business terms — not how it does it. "restricts access to private content" — yes. "restricts access via JWT" — no, the "via JWT" is mechanism. Same for technology names, library calls, specific protocols. Keep mechanism out of the statement entirely. The `directives` field beside a responsibility holds prescriptive "must"/"never" constraints ("must verify ownership server-side", "never trust a client-supplied role") — but directives are authored by the user, not the agent. Treat any directives present as binding constraints you must satisfy when implementing; never write, edit, or delete them. Where reality discharges a responsibility belongs in the source map, not in directives."#,
    },
    Rule {
        id: 2,
        title: "Every node justifies its existence through responsibilities it serves",
        tags: &["node", "responsibility", "vagrant", "prune", "empty", "justification"],
        body: r#"A child node exists to discharge a subset of its parent's responsibilities. A node with no responsibility, or whose responsibilities serve no ancestor commitment, is structurally vagrant — prune it or restate its purpose."#,
    },
    Rule {
        id: 3,
        title: "Decompose for checkability — keep each node's responsibilities at its OWN altitude",
        tags: &["decomposition", "altitude", "scope", "responsibility", "parent", "child", "tree"],
        body: r#"A responsibility names one accountability the node holds, never the handler that discharges it; the handlers are the children one level down (a container's components, a component's symbols). A parent provides the HIGHER-LEVEL responsibilities that DECOMPOSE INTO its children's — it never simply iterates or restates them. So a parent's responsibilities are FEWER and BROADER than the union of its children's, never a per-child enumeration of what each child does ("Provides themed form controls" — yes; "Provides Button, Input, Select, Checkbox…" — no, that is the children listed, not an accountability). Test each line: if it reads as describing a single child, or as a roster of them, it is one altitude too low — lift it to the accountability those children collectively serve, or push it down onto the child. A parent that enumerates its children is the PARENT mis-scoped: lift its responsibilities. Do NOT conclude the children are redundant just because the parent lists them — whether each child earns its own node is a separate test (rule 8). If a responsibility is too coarse to verify at the parent's altitude, add child nodes whose responsibilities together discharge it. The node tree IS the responsibility tree, refined downward."#,
    },
    Rule {
        id: 4,
        title: "Groups organize peers along a secondary axis — never a substitute for parent/child decomposition",
        tags: &["group", "grouping", "logical", "architectural", "deployment", "peers"],
        body: r#"If the members only make sense as parts of the group, not as independent entities, the group is a missing parent node — promote it and make the members children. Two flavors: **Logical** (no responsibilities) — organizational signal like team ownership, feature areas, or module colocation. Agents should respect these when structuring code (e.g. keeping grouped components in the same directory) even though the group carries no explicit commitments. **Architectural** (has responsibilities) — a cross-cutting concern like a deployment boundary. Responsibilities describe what the *grouping relationship* enforces, not what individual members do."#,
    },
    Rule {
        id: 5,
        title: "One link per relationship",
        tags: &["link", "relationship", "direction", "edge"],
        body: r#"Direction points from initiator/requester toward provider/dependency. Two links between the same pair of nodes are valid only when they represent independent relationships."#,
    },
    Rule {
        id: 6,
        title: "Containers are runtime boundaries",
        tags: &["container", "runtime", "deployment", "process", "boundary", "webview"],
        body: r#"Each separately deployable process is at least one container. An embedded runtime (e.g. a webview inside a native shell, a scripting engine inside a host process) is NOT a separate container — it is a component of the host container. Group containers that deploy together."#,
    },
    Rule {
        id: 7,
        title: "Components map to code structures (classes, modules, packages, folders)",
        tags: &["component", "code", "module", "package", "library", "third-party"],
        body: r#"Third-party libraries are not components — they're implementation details mentioned in `technology`. Cluster components from code cohesion and the dependency graph, NOT one component per file."#,
    },
    Rule {
        id: 8,
        title: "Code level uses only the `symbol` kind",
        tags: &[
            "symbol", "code", "definition", "function", "class", "struct", "interface",
            "type", "properties", "data", "schema", "wrapper", "scoping", "empty", "config",
            "main", "entry-point", "binary", "bootstrap",
        ],
        body: r#"A `symbol` is exactly one addressable code definition — a function, method, handler, hook, React component, class, struct, interface, type, or config object. One symbol node = one definition in the source; its name must be the identifier as it appears in the code. A definition earns a symbol node only when it carries architecture — a behavioral responsibility at its OWN altitude, or a declared data shape. A cross-boundary link alone does NOT justify a symbol: a link is a relationship between nodes that already exist for their own reasons, never a reason to mint one. So an otherwise-empty symbol (no responsibility, no data shape) is never kept just because something links to it — fold it away. Being a real, public definition is NOT sufficient either: a wrapper, re-export, getter/setter, or test stub that discharges NO distinct responsibility folds into its component's responsibilities rather than minting a leaf. An executable entry point is one of these: `main` (or any top-level binary entry that only wires up and dispatches the program's work) carries the binary's behavior at the COMPONENT's altitude, not a sub-altitude — fold its responsibility into the component that represents that binary (the example, CLI, or bootstrap IS that single unit of architecture) rather than minting a `main` leaf; helper definitions in the same binary that hold their own distinct responsibility still earn their nodes. But the fold test is about ARCHITECTURE, not implementation size: thinness of the body is NEVER by itself grounds to fold — a three-line function that crosses a boundary (an IPC command, an API endpoint, a contract surface) or holds any distinct own-altitude accountability earns its node however few lines it is. And the two operations are NOT symmetric: declining to MINT a node for an empty definition at extraction time is cheap and reversible; DELETING an existing symbol that carries — or, mapping to real code, SHOULD carry — a distinct responsibility destroys authored architecture and its source anchor, so it demands a far higher bar. A symbol that maps to real own-altitude behavior is DEFINED — give it the responsibility — never deleted; an empty model slot for such a symbol means not-yet-authored, not absent, so "define or delete" resolves to DEFINE. A parent responsibility that enumerates the symbol ("reads, writes, and deletes …") is NOT evidence the symbol is covered: that is the parent mis-scoped (rule 3), never a license to prune the child. Fold generated mirror types (`*-types`, `*.d.ts`) into the source-of-truth symbol and leave private helper methods out. Prefer a component with a handful of meaningful symbols over one mirroring every definition in its files. A symbol has two independent facets; most carry one, some carry both:
- **responsibilities** — the behavior the definition discharges. Map each to the specific LINE RANGE that does its work (with the enclosing `symbol` named as anchor and context). A line range must be a PROPER subset of the symbol; when one responsibility is the whole definition's work, omit `line`/`endLine` entirely — a symbol-only anchor means "this whole definition". `update_source_map` enforces this: a range covering the whole symbol is stripped to the symbol anchor and reported. Two responsibilities sharing an enclosing symbol must point at different line ranges; if they point at the identical range they are one responsibility.
- **properties** — when the definition DECLARES A DATA SHAPE (a struct/class/interface/type, or a config object that defines a field schema), list its fields here: one property per field. Map the declaration to the symbol's node id via the `schemas` source array (not `entries`).
NEVER describe a data shape in a responsibility statement. "Defines the lead record schema with status, qualification, and booking fields" is WRONG — that prose belongs nowhere; enumerate `status`, `qualification`, `booking`, … as actual `properties`. A config object that both wires behavior and declares fields (e.g. an ORM/CMS collection that registers admin UI and lifecycle hooks AND defines the record's fields) gets BOTH facets: behavioral responsibilities for the hooks/UI, and a property per declared field — and those properties are the declared FIELDS (the record's columns), never the config wrapper keys (slug/admin/hooks/access). A pure data type carries only properties. Symbols carry no boundary glob (that is a container/component concept). **Scoping**: determine parentage from the import/usage graph, not from file co-location. A code-level node belongs to whichever component actually consumes it. If multiple components import the same code, parent it to the one that owns/defines it and add links from the others — the cross-boundary dependency is valuable signal."#,
    },
    Rule {
        id: 9,
        title: "External systems (`external: true`) are opaque, no children",
        tags: &["external", "system", "third-party", "opaque"],
        body: r#"Any responsibilities listed on an external are read as expectations of that external, not commitments by your system."#,
    },
    Rule {
        id: 10,
        title: "Mentions imply links",
        tags: &["link", "mention", "responsibility", "reference", "description"],
        body: r#"A responsibility statement (or a `description`) that names another node requires a structural link to it. The prose mention and the structural link are distinct concerns — naming a node in prose never substitutes for the link the mention implies; declare both."#,
    },
    Rule {
        id: 11,
        title: "System boundary = ownership boundary",
        tags: &["system", "boundary", "ownership", "deployment"],
        body: r#"Everything you build and deploy from one codebase is containers inside one system. "Separate deployment unit" does NOT mean "separate system.""#,
    },
    Rule {
        id: 12,
        title: "`technology` is node identity",
        tags: &["technology", "identity", "directives"],
        body: r#"`technology` is what the node IS as software ("Payload 3.0", "PostgreSQL 16", "S3 Bucket"). Separate from `directives` on responsibilities, which are user-authored constraints prescribing how a responsibility must be discharged — never set by the agent. Do not put technology vocabulary inside responsibility statements."#,
    },
    Rule {
        id: 14,
        title: "Observation flags: vagrant and stale",
        tags: &["vagrant", "stale", "drift", "discovered", "flag", "observation"],
        body: r#"Flags are machine/agent observations awaiting the user's verdict — a separate axis from the model→code plan (which the diff between the committed model and the planned draft already captures). `vagrant`: marks responsibilities discovered in code that no upstream commitment justifies; added by automation as a code-discovered claim flagged `vagrant: true`; the user adopts it (clear the flag) or rejects it (delete it, signaling the agent to remove the code). `stale`: set by the drift check on a responsibility whose code no longer discharges it; awaiting a verdict — re-implement (`mark_implemented` clears it), reword, or delete."#,
    },
    Rule {
        id: 15,
        title: "Write for scanning, not prose",
        tags: &["responsibility", "wording", "scanning", "description", "concise"],
        body: r#"A responsibility is ONE verb-led clause: lead with the specific verb + object that distinguishes it, then stop. Cut words that merely restate the node's own domain — in an architecture tool every line is about "the architecture model", so naming it adds nothing — and cut trailing "by/through/where/so that …" clauses (mechanism belongs out per rule 1; the obvious belongs cut). A genuine trigger or state condition is NOT such a tail — it moves to the FRONT in its EARS keyword form (rule 21). "Renders the node/link/group canvas" — yes. "Renders the visual architecture editor where users arrange nodes, links, and groups on a canvas" — no. A `description` is the node's IDENTITY in a few words (what it IS as software), never a summary of the responsibilities listed beneath it; if it reads as a comma-list of those responsibilities, drop it."#,
    },
    Rule {
        id: 16,
        title: "Relationships connect nodes at the same C4 level",
        tags: &["link", "level", "reference", "propagate", "external", "validate", "disconnected"],
        body: r#"Each diagram tells one level of the story, and `add_links` ENFORCES this (an illegal link is rejected, not saved). A link is legal only when src and dst are siblings (same parent), OR the deeper node's parent already links to the other node — which makes that node a *reference* on the deeper node's surface. References thus propagate DOWN from higher-level links: at system context a person/external links to the SYSTEM; to also wire it to a specific container or component, the relationship must exist at every level in between. So when an external is used deep inside your system, add the link at EACH level: system→external, then container→external, then component→external — each one authorizes the next. Two consequences: (a) you cannot link a deep node straight to a top-level external without the intervening links — add them parent-first (a single `add_links` batch may include all the levels at once); (b) every node down to component level still needs a relationship at its OWN level, or it appears disconnected on its own diagram — symbols are exempt: they justify themselves through their claims (rule 8), and the code-level graph is legitimately sparse. You may model a relationship only as deep as it is useful — a `container → external` link need not be refined to the component that calls it; the external simply won't appear inside the container view until a component links to it, and that is fine. Never link a node to its own ancestor/descendant — nesting already expresses containment. Run `validate_model` and fix every warning before finishing."#,
    },
    Rule {
        id: 17,
        title: "Names and language: simple, clear, concise — clarity is paramount",
        tags: &[
            "naming", "names", "clarity", "language", "simple", "concise", "wording",
            "description", "terminology", "jargon", "readable",
        ],
        body: r#"The model exists to make the architecture understood, so clarity of understanding is the highest goal — every name, description, and responsibility must be immediately legible to a reader who does NOT already know the system. Prefer the plainest word that is still precise: simple over clever, concrete over abstract, short over long. A name says what the thing IS in the domain's own vocabulary — no cute coinages, no internal codenames, no needless abbreviations or acronyms; use the SAME term for the same concept everywhere (consistent vocabulary beats variety). Cut every word that doesn't change the meaning. The test: if a reader has to pause to decode a name or read a sentence twice, rewrite it until they don't. This principle governs all authored text — rule 15 gives the responsibility-statement mechanics, rule 1 keeps mechanism out of responsibilities, and symbol names are the exception that proves it: they must be the exact code identifier (rule 8), clarity there coming from the code itself."#,
    },
    Rule {
        id: 18,
        title: "Building in layers — commit the skeleton, split coarse claims",
        tags: &[
            "partial", "scaffold", "scaffolding", "layer", "layered", "commit", "committed",
            "pending", "skeleton", "status", "mark_implemented", "incremental",
        ],
        body: r#"A node's presence in the COMMITTED model asserts only that its boundary exists and that its OWN committed responsibilities hold — NOT that everything beneath it is built. A structural node (any node with children) discharges its responsibilities THROUGH its subtree (rule 3): committing it never claims its unbuilt descendants exist. Unbuilt children simply stay in the plan and roll up as pending. So when you build bottom-up or one slice at a time, DO commit the skeleton you actually built — the spine of nodes plus each responsibility whose code exists — and leave the rest in the plan. The plan↔committed split IS how a partial build is represented; there is no separate status to set and nothing dishonest about it. `mark_implemented` takes `responsibilityIds` precisely for this: fold the responsibilities you finished, not the whole node. A node whose subtree mixes built and unbuilt work shows an intermediate completeness in `get_health` — its anchored primitives over its authored ones (rule 19) — the correct, honest state for a layered build, never a problem to avoid by withholding the node. The one real over-assertion trap is a SINGLE responsibility that bundles built + unbuilt behaviour ("streams a reply, invokes tools, and returns structured output" when only streaming exists): folding it claims code that isn't there, withholding it blocks the whole node. Do neither — that statement is one altitude too coarse (rules 3, 15). SPLIT it into the built accountability and the unbuilt one(s), commit the built half, leave the rest pending. Never withhold the structural spine just because a responsibility below it is unfinished."#,
    },
    Rule {
        id: 19,
        title: "Anchor only what's implemented — the anchor is the completeness signal",
        tags: &[
            "anchor", "anchored", "source-map", "sourcemap", "completeness", "percent",
            "coverage", "implemented", "scaffold", "scaffolding", "boundary", "glob", "primitive",
        ],
        body: r#"An anchor — a responsibility's or symbol's `source_map` file:line, a container/component's boundary glob — records that REAL code discharges the claim. So NEVER anchor a claim you have not implemented yet. Authoring the plan is anchorless; anchoring is the BUILD checkpoint, the act of recording "this exists, here". That discipline makes anchoring the completeness signal: a node's completeness is its ANCHORED primitives over its AUTHORED ones (committed + planned), rolled up over the subtree. The countable primitives are each node's own anchor (a container/component's boundary glob — "the box"; a symbol's definition), each LEAF responsibility, and each data-shape property set. A structural node's OWN responsibilities are NOT counted — they discharge through the subtree (rule 3). So a container you have scaffolded — boundary glob set over the real directory, but its responsibilities and children not yet anchored — reads as a LOW, non-zero completeness: instantly legible as "scaffolded, not built out". As you implement and anchor each leaf, the number climbs; a node with no anchorable leaf primitives beneath it yet is unmeasured (—), not complete. On a greenfield model nothing is anchored and the denominator is the authored plan, so everything reads 0% — correct: fully specced, nothing built. Because you never anchor vapor, "anchored" is a trustworthy proxy for "implemented" — but it certifies the code EXISTS, not that it fully satisfies the claim: a claim whose code only half-discharges it is still one altitude too coarse (rule 18), so split it."#,
    },
    Rule {
        id: 20,
        title: "Concerns tag cross-cutting accountability — at most one per responsibility",
        tags: &[
            "concern", "concerns", "tag", "cross-cutting", "auth", "facet", "lens",
            "idempotency", "observability",
        ],
        body: r#"A responsibility may carry ONE `concern` — a kebab-case slug naming the cross-cutting concern it serves. The standard vocabulary: `auth`, `persistence`, `failure-handling`, `idempotency`, `validation`, `observability`, `performance`, `compliance`. Tag every responsibility that discharges a cross-cutting concern as you author or edit it; leave a node's core domain flow UNTAGGED — no tag means "this is the main behavior", and that absence is signal, not an omission. Reuse before minting: check the model's concern registry and the standard slugs first, and use the SAME slug for the same concern everywhere (rule 17's consistent-vocabulary test) — mint a new slug only for a genuinely distinct concern, and keep it short and domain-generic ("rate-limiting", not "api-rate-limits"). Exactly one concern per responsibility: a statement that seems to need two bundles two accountabilities — split it (rules 3, 15). The concern is metadata BESIDE the statement, never wording inside it: with the tag carrying the category, purpose clauses like "…so duplicates aren't reprocessed" are redundant — cut them (rule 15). Registry entries (slug, description, icon) mint automatically on first use; the user curates them — never edit or delete a registry entry's description or icon yourself."#,
    },
    Rule {
        id: 21,
        title: "Statements speak EARS — condition first, response last, markup on the anchors",
        tags: &[
            "ears", "grammar", "statement", "condition", "trigger", "event", "state",
            "failure", "keyword", "markup", "bold", "wording", "split", "compound",
        ],
        body: r#"A responsibility statement follows EARS (Easy Approach to Requirements Syntax) with the subject dropped: the owning node IS the subject, so never name it and never write "shall". Clause order is FIXED — optional condition first, verb-led response last:
- Ubiquitous (always active): no keyword, just the verb-led response — exactly rule 15's form. "Authenticate every inbound POST."
- Event-driven: "When <trigger>, <response>."
- State-driven: "While <state>, <response>."
- Unwanted behaviour (failure/rejection handling): "If <condition>, then <response>."
- Optional feature: "Where <feature is present>, <response>" — legal EARS, rarely earned here; prefer modeling the feature as structure.
Clauses stack in that order when a claim genuinely needs both: "While <state>, when <trigger>, <response>". The ubiquitous form is EARNED, never the default: before leaving a claim plain, check whether it actually holds only on a trigger, state, or failure — if so, it MUST take the keyword form; a condition written as a tail ("…after acking the webhook") moves to the front in its keyword form. One pattern per claim: a statement that bundles a happy path with its rejection ("echo the challenge when the token matches, else reject with 403") is TWO claims — the When and the If — split it (rules 3, 15). Compound responses split the same way: two verbs acting on two different objects ("persist the results and sync labels to HubSpot") is TWO claims — bolding both verbs is NOT a substitute for splitting; only a genuinely atomic verb pair on one object stays together. A rationale tail ("…so sales receives only qualified ones") is not part of the response — cut it or move it to the description (rule 15).
Statements also carry a markdown-lite display markup the UI renders — and strips for comparison, search, and word-diffing, so it is presentation, never meaning. Wrap the scan anchors — the grammar keywords (including "then") and the response verb — in **bold**, and NOTHING else: "**When** a send-failure status callback arrives, **append** a failed-send event". A ubiquitous claim bolds its leading verb: "**Authenticate** every inbound POST". Never wrap the condition clause or any other run of text in markers — the keyword and its comma already delimit the clause. Markup belongs in responsibility statements ONLY — never in directives, descriptions, names, or properties."#,
    },
    Rule {
        id: 22,
        title: "Attach tests to the claims you implement — mandatory for symbols, expected everywhere testable",
        tags: &[
            "test", "tests", "attached", "attach", "tested", "testable", "untested",
            "test-map", "unit", "integration",
        ],
        body: r#"A claim either HAS tests attached or it DOESN'T — that binary is the model's highest-trust signal, because an attached test is the one artifact that at least ATTEMPTS to hold the code to the claim's words. Scryer records attachment only: it never runs the test and never certifies what it proves — attachment makes the test findable so a human can audit it. A claim in a When/While/If form (rule 21) names a concrete trigger, state, or failure, so a test can exercise it mechanically: arrange the condition, assert the response. When you implement such a claim, write that test in the project's own test suite and attach it in the SAME `mark_implemented` call (`tests`, alongside `anchors`) — implementing and attaching are one checkpoint, not two passes. On a SYMBOL host this is mandatory: a symbol-level testable claim is exactly a unit test's shape, so never fold one without its test. Higher-altitude claims take the kind of test that fits the altitude — component → integration, container → API/service, system → end-to-end. Health counts the gap deterministically: `tested` (claims with a test attached), `testable` (When/While/If claims on code-backed hosts), `untested` (testable claims with no test attached); drive `untested` toward zero on the scopes you build. The attached test must actually exercise the claim — its trigger arranged, its response asserted; never attach a test that merely touches the same code to clear the counter, and never attach one test to many claims it doesn't exercise. Anchor the test by its NAME: `symbol` takes the `it("…")`/`test("…")` description string or the test function's identifier — both resolve and fingerprint the same way. A ubiquitous (verb-led) claim may still deserve a test — that stays a judgment call; attach it the same way (or later via `update_source_map` `test_entries`). The attachment is a link, never executed — but it is fingerprint-tripwired like any anchor: when the test's code changes or vanishes, the claim's test anchor surfaces in health (`test:{id}`), so a silently rotted test is visible without running anything."#,
    },
];

/// The full ruleset as one numbered block — for AI-review prompts and any
/// consumer that wants the complete text rather than a lookup.
pub fn rules_full() -> String {
    let mut s = String::new();
    for r in RULES {
        s.push_str(&format!("{}. {}. {}\n", r.id, r.title, r.body));
    }
    s
}

/// A compact, drillable index — one line per rule (`id`, title, tags) — for
/// connect-time instructions. Small enough to ship every session; the agent
/// pulls a rule's full `body` on demand with `get_rules{topic}`.
pub fn rules_index() -> String {
    let mut s = String::new();
    for r in RULES {
        s.push_str(&format!("{}. {} [{}]\n", r.id, r.title, r.tags.join(", ")));
    }
    // Ids are stable references (instructions cite rules by number), so a
    // retired rule leaves a gap rather than renumbering everything after it.
    s.push_str("(Rule ids are stable references; 13 was retired, not omitted by mistake.)\n");
    s
}

/// Filler words stripped from a lookup topic. `orient` feeds whole task
/// sentences ("tag the concerns for the model") through here, so these must
/// never count as hits — before ranking existed, "the" alone dragged in
/// every rule whose title contained it.
const STOPWORDS: &[&str] = &[
    "the", "a", "an", "and", "or", "for", "of", "to", "in", "on", "at", "with", "is", "are",
    "be", "it", "its", "this", "that", "these", "those", "how", "what", "when", "where", "why",
    "do", "does", "did", "can", "should", "must", "my", "our", "your", "their", "all", "any",
    "per", "as", "by", "we", "you", "not", "no", "up", "out", "into", "from", "about",
];

/// Look up rules by free-text topic, ranked by relevance — a whole task
/// sentence is a valid input (`orient` passes the task verbatim). The topic is
/// tokenized (hyphenated words like "cross-cutting" stay whole), stopwords and
/// sub-3-char fragments drop, and each surviving term is matched WORD-level
/// against titles and tags: exact, or a one-way prefix of ≥4 chars so plurals
/// and stems land ("concerns" → "concern", "links" → "link") without "the"
/// hitting "then". Title/tag hits are the curated surface and outrank body
/// substring hits, which only surface at all when NO rule matched on
/// title/tag — so a common body word can't drag in every rule. Ties keep rule
/// order; zero-score rules are omitted.
pub fn lookup(topic: &str) -> Vec<&'static Rule> {
    let terms: Vec<String> = topic
        .split(|c: char| !c.is_alphanumeric() && c != '-')
        .map(|t| t.trim_matches('-').to_lowercase())
        .filter(|t| t.len() >= 3 && !STOPWORDS.contains(&t.as_str()))
        .collect();
    if terms.is_empty() {
        return Vec::new();
    }

    let word_hit = |term: &str, word: &str| {
        term == word
            || (term.len() >= 4 && word.starts_with(term))
            || (word.len() >= 4 && term.starts_with(word))
    };

    let mut scored: Vec<(usize, usize, &'static Rule)> = Vec::new();
    for r in RULES.iter() {
        let title = r.title.to_lowercase();
        let title_words: Vec<&str> = title
            .split(|c: char| !c.is_alphanumeric() && c != '-')
            .filter(|w| !w.is_empty())
            .collect();
        let body = r.body.to_lowercase();
        let mut curated = 0;
        let mut in_body = 0;
        for t in &terms {
            if title_words.iter().any(|w| word_hit(t, w))
                || r.tags.iter().any(|tag| word_hit(t, tag))
            {
                curated += 1;
            } else if body.contains(t.as_str()) {
                in_body += 1;
            }
        }
        if curated + in_body > 0 {
            scored.push((curated, in_body, r));
        }
    }

    let any_curated = scored.iter().any(|(c, _, _)| *c > 0);
    if any_curated {
        scored.retain(|(c, _, _)| *c > 0);
    }
    // Stable sort: equal scores keep rule order.
    scored.sort_by(|a, b| (b.0 * 2 + b.1).cmp(&(a.0 * 2 + a.1)));
    scored.into_iter().map(|(_, _, r)| r).collect()
}

/// Render a set of pulled rules as full text for a tool response.
pub fn render(rules: &[&Rule]) -> String {
    let mut s = String::new();
    for r in rules {
        s.push_str(&format!("{}. {}\n{}\n\n", r.id, r.title, r.body));
    }
    s.trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_resolves_topics_via_title_and_tags() {
        // The empty-symbol incident: "symbol" must reach rule 8 (the bar).
        let hits = lookup("symbol");
        assert!(hits.iter().any(|r| r.id == 8), "symbol → rule 8");

        // "empty" is tagged on both the prune rule (2) and the symbol rule (8).
        let empty = lookup("empty");
        assert!(empty.iter().any(|r| r.id == 2));
        assert!(empty.iter().any(|r| r.id == 8));

        // Curated tags resolve domain words to the right rule.
        assert!(lookup("group").iter().any(|r| r.id == 4));
        assert!(lookup("altitude").iter().any(|r| r.id == 3));
        assert!(lookup("vagrant stale").iter().any(|r| r.id == 14));
        assert!(lookup("naming").iter().any(|r| r.id == 17));
        assert!(lookup("concern").iter().any(|r| r.id == 20));
        assert!(lookup("cross-cutting").iter().any(|r| r.id == 20));

        // The statement grammar: "when"/"while"/"if" are stopwords, so the
        // durable routes in are the notation's name and its display markup.
        assert!(lookup("ears").iter().any(|r| r.id == 21));
        assert!(lookup("markup").iter().any(|r| r.id == 21));
        assert!(lookup("trigger condition").first().map(|r| r.id) == Some(21));
    }

    #[test]
    fn lookup_misses_return_empty_not_everything() {
        // A word in no title/tag and no body must not drag in the whole set.
        assert!(lookup("kubernetes helm istio").is_empty());
        // Pure stopwords/fragments match nothing rather than everything.
        assert!(lookup("the for and a of").is_empty());
    }

    #[test]
    fn lookup_ranks_a_task_sentence_by_relevance() {
        // orient passes the user's task VERBATIM and keeps the top 3 — so the
        // rule the sentence is about must rank first despite the stopwords
        // and despite rule 20 being dead last in rule order.
        let hits = lookup("tag the concerns for the model");
        assert_eq!(hits.first().map(|r| r.id), Some(20), "rule 20 outranks stopword noise");

        // Plural/stem forms reach the singular vocabulary.
        assert!(lookup("links").iter().any(|r| r.id == 5), "links → link (rule 5)");
        assert!(lookup("groups").iter().any(|r| r.id == 4), "groups → group (rule 4)");
    }

    #[test]
    fn index_lists_every_rule_full_renders_all() {
        let idx = rules_index();
        for r in RULES {
            assert!(idx.contains(r.title), "index lists '{}'", r.title);
        }
        // index is the compact surface — no full bodies in it
        assert!(!idx.contains(RULES[7].body));
        // full render carries the bodies
        assert!(rules_full().contains(RULES[7].body));
    }
}
