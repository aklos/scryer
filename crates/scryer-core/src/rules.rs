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
        ],
        body: r#"A `symbol` is exactly one addressable code definition — a function, method, handler, hook, React component, class, struct, interface, type, or config object. One symbol node = one definition in the source; its name must be the identifier as it appears in the code. A definition earns a symbol node only when it carries architecture — a behavioral responsibility at its OWN altitude, or a declared data shape. A cross-boundary link alone does NOT justify a symbol: a link is a relationship between nodes that already exist for their own reasons, never a reason to mint one. So an otherwise-empty symbol (no responsibility, no data shape) is never kept just because something links to it — fold it away. Being a real, public definition is NOT sufficient either: a wrapper, re-export, getter/setter, or test stub that discharges NO distinct responsibility folds into its component's responsibilities rather than minting a leaf. But the fold test is about ARCHITECTURE, not implementation size: thinness of the body is NEVER by itself grounds to fold — a three-line function that crosses a boundary (an IPC command, an API endpoint, a contract surface) or holds any distinct own-altitude accountability earns its node however few lines it is. And the two operations are NOT symmetric: declining to MINT a node for an empty definition at extraction time is cheap and reversible; DELETING an existing symbol that carries — or, mapping to real code, SHOULD carry — a distinct responsibility destroys authored architecture and its source anchor, so it demands a far higher bar. A symbol that maps to real own-altitude behavior is DEFINED — give it the responsibility — never deleted; an empty model slot for such a symbol means not-yet-authored, not absent, so "define or delete" resolves to DEFINE. A parent responsibility that enumerates the symbol ("reads, writes, and deletes …") is NOT evidence the symbol is covered: that is the parent mis-scoped (rule 3), never a license to prune the child. Fold generated mirror types (`*-types`, `*.d.ts`) into the source-of-truth symbol and leave private helper methods out. Prefer a component with a handful of meaningful symbols over one mirroring every definition in its files. A symbol has two independent facets; most carry one, some carry both:
- **responsibilities** — the behavior the definition discharges. Map each to the specific LINE RANGE that does its work (with the enclosing `symbol` named as anchor and context). A line range must be a PROPER subset of the symbol; when one responsibility is the whole definition's work, omit `line`/`endLine` entirely — a symbol-only anchor means "this whole definition". `update_source_map` enforces this: a range covering the whole symbol is stripped to the symbol anchor and reported. Two responsibilities sharing an enclosing symbol must point at different line ranges; if they point at the identical range they are one responsibility.
- **properties** — when the definition DECLARES A DATA SHAPE (a struct/class/interface/type, or a config object that defines a field schema), list its fields here: one property per field, each with a status. Map the declaration to the symbol's node id via the `schemas` source array (not `entries`).
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
        title: "Mentions imply links — and are written as wikilinks",
        tags: &["link", "mention", "responsibility", "reference", "wikilink", "description"],
        body: r#"A responsibility statement that names another node requires a link to it. Write the mention itself as a wikilink by node id: `[[node-12]]` — the UI resolves it to the node's current name, so renames never break prose — or `[[node-12|shown text]]` when the sentence needs different wording ("publishes events to the [[node-12|billing pipeline]]"). Prose becomes navigation. Use wikilinks in `description` text too whenever it names another node. A wikilink is the prose-level mention; it never replaces the structural link — declare both."#,
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
        id: 13,
        title: "Status lives on responsibilities and on `properties`, not nodes",
        tags: &["status", "lifecycle", "proposed", "implemented", "verified", "changed", "properties"],
        body: r#"Values: `proposed` (planned, no code yet), `implemented` (code exists), `verified` (production-ready, checked against code), `changed` (spec was modified after implementation — needs re-implementation). Lifecycle: proposed → implemented → verified. Editing a responsibility's statement or directives while status is `implemented` or `verified` flips it to `changed`. After re-implementation, `changed` returns to `implemented`. A node's lifecycle is the aggregate of its responsibilities and properties. Always set status explicitly on each responsibility and each property."#,
    },
    Rule {
        id: 14,
        title: "Observation flags (separate from status)",
        tags: &["vagrant", "stale", "drift", "discovered", "flag", "observation"],
        body: r#"Statuses are the prescription; flags are machine/agent observations awaiting the user's verdict — never conflate the axes. `vagrant`: marks responsibilities discovered in code that no upstream commitment justifies; always added by automation with `status: implemented, vagrant: true`; the user adopts it (clear the flag) or rejects it (delete it, signaling the agent to remove the code). `stale`: set by the drift check on a responsibility whose code no longer discharges it; the status stays untouched until the verdict — re-implement (`mark_implemented` clears it), reword, or delete. Drift never moves a status."#,
    },
    Rule {
        id: 15,
        title: "Write for scanning, not prose",
        tags: &["responsibility", "wording", "scanning", "description", "concise"],
        body: r#"A responsibility is ONE verb-led clause: lead with the specific verb + object that distinguishes it, then stop. Cut words that merely restate the node's own domain — in an architecture tool every line is about "the architecture model", so naming it adds nothing — and cut trailing "by/through/where/so that …" clauses (mechanism belongs out per rule 1; the obvious belongs cut). "Renders the node/link/group canvas" — yes. "Renders the visual architecture editor where users arrange nodes, links, and groups on a canvas" — no. A `description` is the node's IDENTITY in a few words (what it IS as software), never a summary of the responsibilities listed beneath it; if it reads as a comma-list of those responsibilities, drop it."#,
    },
    Rule {
        id: 16,
        title: "Relationships connect nodes at the same C4 level",
        tags: &["link", "level", "reference", "propagate", "external", "validate", "disconnected"],
        body: r#"Each diagram tells one level of the story, and `add_links` ENFORCES this (an illegal link is rejected, not saved). A link is legal only when src and dst are siblings (same parent), OR the deeper node's parent already links to the other node — which makes that node a *reference* on the deeper node's surface. References thus propagate DOWN from higher-level links: at system context a person/external links to the SYSTEM; to also wire it to a specific container or component, the relationship must exist at every level in between. So when an external is used deep inside your system, add the link at EACH level: system→external, then container→external, then component→external — each one authorizes the next. Two consequences: (a) you cannot link a deep node straight to a top-level external without the intervening links — add them parent-first (a single `add_links` batch may include all the levels at once); (b) every node still needs a relationship at its OWN level, or it appears disconnected on its own diagram. You may model a relationship only as deep as it is useful — a `container → external` link need not be refined to the component that calls it; the external simply won't appear inside the container view until a component links to it, and that is fine. Never link a node to its own ancestor/descendant — nesting already expresses containment. Run `validate_model` and fix every warning before finishing."#,
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
    s
}

/// Look up rules by free-text topic. Space-separated terms match (case-insensitive
/// substring) against each rule's title and tags first — the curated, high-signal
/// surface — and fall back to the body only when nothing matches there, so a
/// common word doesn't drag in every rule. Returns the matches in rule order.
pub fn lookup(topic: &str) -> Vec<&'static Rule> {
    let terms: Vec<String> = topic.split_whitespace().map(|t| t.to_lowercase()).collect();
    if terms.is_empty() {
        return Vec::new();
    }
    let matches = |hay: &str| {
        let h = hay.to_lowercase();
        terms.iter().any(|t| h.contains(t.as_str()))
    };

    // Primary pass: title + tags.
    let titled: Vec<&Rule> = RULES
        .iter()
        .filter(|r| matches(r.title) || r.tags.iter().any(|t| matches(t)))
        .collect();
    if !titled.is_empty() {
        return titled;
    }
    // Fallback: full body text.
    RULES.iter().filter(|r| matches(r.body)).collect()
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
        assert!(lookup("status lifecycle").iter().any(|r| r.id == 13));
        assert!(lookup("naming").iter().any(|r| r.id == 17));
    }

    #[test]
    fn lookup_misses_return_empty_not_everything() {
        // A word in no title/tag and no body must not drag in the whole set.
        assert!(lookup("kubernetes helm istio").is_empty());
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
