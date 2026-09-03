//! Modeling rules and working-loop rules — the single source of truth the MCP
//! surface cites. Each rule is an addressable entry (`slug`, `title`, `tags`,
//! `body`); a tool description names the slugs it depends on in a trailing
//! `Rules:` line and a rule body cites another as `[[slug]]`, so an agent pulls
//! exactly the guidance it is about to need via `get_rules {id}` instead of
//! carrying the whole set in context. [`rules_index`] is the drillable index;
//! [`rules_full`] renders everything for review prompts.

/// One modeling rule. `title` is the headline (also the first match target),
/// `tags` are the curated lookup keywords surfaced in the index, `body` is the
/// full guidance returned when the rule is pulled.
pub struct Rule {
    pub id: u8,
    /// Stable address — what descriptions and other rules cite, and what
    /// `get_rules {id}` resolves. Never renumbered, never reused.
    pub slug: &'static str,
    pub title: &'static str,
    pub tags: &'static [&'static str],
    pub body: &'static str,
}

pub const RULES: &[Rule] = &[
    Rule {
        id: 1,
        slug: "statement-business",
        title: "Responsibilities are pure business statements",
        tags: &["responsibility", "business", "mechanism", "directives", "wording"],
        body: r#"A responsibility says what a node is accountable for in business terms — not how it does it. "restricts access to private content" — yes. "restricts access via JWT" — no, the "via JWT" is mechanism. Same for technology names, library calls, specific protocols. Keep mechanism out of the statement entirely. The `directives` field beside a responsibility holds prescriptive "must"/"never" constraints ("must verify ownership server-side", "never trust a client-supplied role") — but directives are authored by the user, not the agent. Treat any directives present as binding constraints you must satisfy when implementing; write, edit, or delete them only when the user explicitly asks for it. Where reality discharges a responsibility belongs in the source map, not in directives."#,
    },
    Rule {
        id: 2,
        slug: "node-justification",
        title: "Every node justifies its existence through responsibilities it serves",
        tags: &["node", "responsibility", "vagrant", "prune", "empty", "justification"],
        body: r#"A child node exists to discharge a subset of its parent's responsibilities. A node with no responsibility, or whose responsibilities serve no ancestor commitment, is structurally vagrant — prune it or restate its purpose."#,
    },
    Rule {
        id: 3,
        slug: "altitude",
        title: "Decompose for checkability — keep each node's responsibilities at its OWN altitude",
        tags: &["decomposition", "altitude", "scope", "responsibility", "parent", "child", "tree"],
        body: r#"A responsibility names one accountability the node holds, never the handler that discharges it; the handlers are the children one level down (a container's components, a component's symbols). A parent provides the HIGHER-LEVEL responsibilities that DECOMPOSE INTO its children's — it never simply iterates or restates them. So a parent's responsibilities are FEWER and BROADER than the union of its children's, never a per-child enumeration of what each child does ("Provides themed form controls" — yes; "Provides Button, Input, Select, Checkbox…" — no, that is the children listed, not an accountability). Test each line: if it reads as describing a single child, or as a roster of them, it is one altitude too low — lift it to the accountability those children collectively serve, or push it down onto the child. A parent that enumerates its children is the PARENT mis-scoped: lift its responsibilities. Do NOT conclude the children are redundant just because the parent lists them — whether each child earns its own node is a separate test (see [[symbols]]). If a responsibility is too coarse to verify at the parent's altitude, add child nodes whose responsibilities together discharge it. The node tree IS the responsibility tree, refined downward."#,
    },
    Rule {
        id: 4,
        slug: "groups",
        title: "Groups organize peers along a secondary axis — never a substitute for parent/child decomposition",
        tags: &["group", "grouping", "logical", "architectural", "deployment", "peers"],
        body: r#"If the members only make sense as parts of the group, not as independent entities, the group is a missing parent node — promote it and make the members children. Two flavors: **Logical** (no responsibilities) — organizational signal like team ownership, feature areas, or module colocation. Agents should respect these when structuring code (e.g. keeping grouped components in the same directory) even though the group carries no explicit commitments. **Architectural** (has responsibilities) — a cross-cutting concern like a deployment boundary. Responsibilities describe what the *grouping relationship* enforces, not what individual members do."#,
    },
    Rule {
        id: 5,
        slug: "one-link",
        title: "One link per relationship",
        tags: &["link", "relationship", "direction", "edge"],
        body: r#"Direction points from initiator/requester toward provider/dependency. Two links between the same pair of nodes are valid only when they represent independent relationships."#,
    },
    Rule {
        id: 6,
        slug: "containers",
        title: "Containers are runtime boundaries",
        tags: &["container", "runtime", "deployment", "process", "boundary", "webview"],
        body: r#"Each separately deployable process is at least one container. An embedded runtime (e.g. a webview inside a native shell, a scripting engine inside a host process) is NOT a separate container — it is a component of the host container. Group containers that deploy together."#,
    },
    Rule {
        id: 7,
        slug: "components",
        title: "Components map to code structures (classes, modules, packages, folders)",
        tags: &["component", "code", "module", "package", "library", "third-party"],
        body: r#"Third-party libraries are not components — they're implementation details mentioned in `technology`. Cluster components from code cohesion and the dependency graph, NOT one component per file."#,
    },
    Rule {
        id: 8,
        slug: "symbols",
        title: "Code level uses only the `symbol` kind",
        tags: &[
            "symbol", "code", "definition", "function", "class", "struct", "interface",
            "type", "properties", "data", "schema", "wrapper", "scoping", "empty", "config",
            "main", "entry-point", "binary", "bootstrap",
        ],
        body: r#"A `symbol` is exactly one addressable code definition — a function, method, handler, hook, React component, class, struct, interface, type, or config object. One symbol node = one definition in the source; its name must be the identifier as it appears in the code. A definition earns a symbol node only when it carries architecture — a behavioral responsibility at its OWN altitude, or a declared data shape. A cross-boundary link alone does NOT justify a symbol: a link is a relationship between nodes that already exist for their own reasons, never a reason to mint one. So an otherwise-empty symbol (no responsibility, no data shape) is never kept just because something links to it — fold it away. Being a real, public definition is NOT sufficient either: a wrapper, re-export, getter/setter, or test stub that discharges NO distinct responsibility folds into its component's responsibilities rather than minting a leaf. An executable entry point is one of these: `main` (or any top-level binary entry that only wires up and dispatches the program's work) carries the binary's behavior at the COMPONENT's altitude, not a sub-altitude — fold its responsibility into the component that represents that binary (the example, CLI, or bootstrap IS that single unit of architecture) rather than minting a `main` leaf; helper definitions in the same binary that hold their own distinct responsibility still earn their nodes. But the fold test is about ARCHITECTURE, not implementation size: thinness of the body is NEVER by itself grounds to fold — a three-line function that crosses a boundary (an IPC command, an API endpoint, a contract surface) or holds any distinct own-altitude accountability earns its node however few lines it is. And the two operations are NOT symmetric: declining to MINT a node for an empty definition at extraction time is cheap and reversible; DELETING an existing symbol that carries — or, mapping to real code, SHOULD carry — a distinct responsibility destroys authored architecture and its source anchor, so it demands a far higher bar. A symbol that maps to real own-altitude behavior is DEFINED — give it the responsibility — never deleted; an empty model slot for such a symbol means not-yet-authored, not absent, so "define or delete" resolves to DEFINE. A parent responsibility that enumerates the symbol ("reads, writes, and deletes …") is NOT evidence the symbol is covered: that is the parent mis-scoped (see [[altitude]]), never a license to prune the child. Fold generated mirror types (`*-types`, `*.d.ts`) into the source-of-truth symbol and leave private helper methods out. Prefer a component with a handful of meaningful symbols over one mirroring every definition in its files. A symbol has two independent facets; most carry one, some carry both:
- **responsibilities** — the behavior the definition discharges. Map each to the specific LINE RANGE that does its work (with the enclosing `symbol` named as anchor and context). A line range must be a PROPER subset of the symbol; when one responsibility is the whole definition's work, omit `line`/`endLine` entirely — a symbol-only anchor means "this whole definition". `update_source_map` enforces this: a range covering the whole symbol is stripped to the symbol anchor and reported. Two responsibilities sharing an enclosing symbol must point at different line ranges; if they point at the identical range they are one responsibility.
- **properties** — when the definition DECLARES A DATA SHAPE (a struct/class/interface/type, or a config object that defines a field schema), list its fields here: one property per field. Map the declaration to the symbol's node id via the `schemas` source array (not `entries`).
NEVER describe a data shape in a responsibility statement. "Defines the lead record schema with status, qualification, and booking fields" is WRONG — that prose belongs nowhere; enumerate `status`, `qualification`, `booking`, … as actual `properties`. A config object that both wires behavior and declares fields (e.g. an ORM/CMS collection that registers admin UI and lifecycle hooks AND defines the record's fields) gets BOTH facets: behavioral responsibilities for the hooks/UI, and a property per declared field — and those properties are the declared FIELDS (the record's columns), never the config wrapper keys (slug/admin/hooks/access). A pure data type carries only properties. Symbols carry no boundary glob (that is a container/component concept). **Scoping**: determine parentage from the import/usage graph, not from file co-location. A code-level node belongs to whichever component actually consumes it. If multiple components import the same code, parent it to the one that owns/defines it and add links from the others — the cross-boundary dependency is valuable signal."#,
    },
    Rule {
        id: 9,
        slug: "externals",
        title: "External systems (`external: true`) are opaque, no children",
        tags: &["external", "system", "third-party", "opaque"],
        body: r#"Any responsibilities listed on an external are read as expectations of that external, not commitments by your system."#,
    },
    Rule {
        id: 10,
        slug: "mentions-imply-links",
        title: "Mentions imply links",
        tags: &["link", "mention", "responsibility", "reference", "description"],
        body: r#"A responsibility statement (or a `description`) that names another node requires a structural link to it. The prose mention and the structural link are distinct concerns — naming a node in prose never substitutes for the link the mention implies; declare both."#,
    },
    Rule {
        id: 11,
        slug: "system-boundary",
        title: "System boundary = ownership boundary",
        tags: &["system", "boundary", "ownership", "deployment"],
        body: r#"Everything you build and deploy from one codebase is containers inside one system. "Separate deployment unit" does NOT mean "separate system.""#,
    },
    Rule {
        id: 12,
        slug: "technology",
        title: "`technology` is node identity",
        tags: &["technology", "identity", "directives"],
        body: r#"`technology` is what the node IS as software ("Payload 3.0", "PostgreSQL 16", "S3 Bucket"). Separate from `directives` on responsibilities, which are user-authored constraints prescribing how a responsibility must be discharged — set by the agent only at the user's explicit request. Do not put technology vocabulary inside responsibility statements."#,
    },
    Rule {
        id: 14,
        slug: "vagrant-stale",
        title: "Observation flags: vagrant and stale",
        tags: &["vagrant", "stale", "drift", "discovered", "flag", "observation"],
        body: r#"Flags are machine/agent observations awaiting the user's verdict — a separate axis from the model→code plan (which the diff between the committed model and the planned draft already captures). `vagrant`: marks responsibilities discovered in code that no upstream commitment justifies; added by automation as a code-discovered claim flagged `vagrant: true`; the user adopts it (clear the flag) or rejects it (delete it, signaling the agent to remove the code). `stale`: set by the drift check on a responsibility whose code no longer discharges it; awaiting a verdict — re-implement (`mark_implemented` clears it), reword, or delete."#,
    },
    Rule {
        id: 15,
        slug: "scanning",
        title: "Write for scanning, not prose",
        tags: &["responsibility", "wording", "scanning", "description", "concise"],
        body: r#"A responsibility is ONE verb-led clause: lead with the specific verb + object that distinguishes it, then stop. Cut words that merely restate the node's own domain — in an architecture tool every line is about "the architecture model", so naming it adds nothing — and cut trailing "by/through/where/so that …" clauses (mechanism belongs out per [[statement-business]]; the obvious belongs cut). A genuine trigger or state condition is NOT such a tail — it moves to the FRONT in its EARS keyword form (see [[statement-ears]]). "Renders the node/link/group canvas" — yes. "Renders the visual architecture editor where users arrange nodes, links, and groups on a canvas" — no. A `description` is the node's IDENTITY in a few words (what it IS as software), never a summary of the responsibilities listed beneath it; if it reads as a comma-list of those responsibilities, drop it."#,
    },
    Rule {
        id: 16,
        slug: "links-same-level",
        title: "Relationships connect nodes at the same C4 level",
        tags: &["link", "level", "reference", "propagate", "external", "validate", "disconnected"],
        body: r#"Each diagram tells one level of the story, and `add_links` ENFORCES this (an illegal link is rejected, not saved). A link is legal only when src and dst are siblings (same parent), OR the deeper node's parent already links to the other node — which makes that node a *reference* on the deeper node's surface. References thus propagate DOWN from higher-level links: at system context a person/external links to the SYSTEM; to also wire it to a specific container or component, the relationship must exist at every level in between. So when an external is used deep inside your system, add the link at EACH level: system→external, then container→external, then component→external — each one authorizes the next. Two consequences: (a) you cannot link a deep node straight to a top-level external without the intervening links — add them parent-first (a single `add_links` batch may include all the levels at once); (b) every node down to component level still needs a relationship at its OWN level, or it appears disconnected on its own diagram — symbols are exempt: they justify themselves through their claims (see [[symbols]]), and the code-level graph is legitimately sparse. You may model a relationship only as deep as it is useful — a `container → external` link need not be refined to the component that calls it; the external simply won't appear inside the container view until a component links to it, and that is fine. Never link a node to its own ancestor/descendant — nesting already expresses containment. Run `validate_model` and fix every warning before finishing."#,
    },
    Rule {
        id: 17,
        slug: "naming",
        title: "Names and language: simple, clear, concise — clarity is paramount",
        tags: &[
            "naming", "names", "clarity", "language", "simple", "concise", "wording",
            "description", "terminology", "jargon", "readable",
        ],
        body: r#"The model exists to make the architecture understood, so clarity of understanding is the highest goal — every name, description, and responsibility must be immediately legible to a reader who does NOT already know the system. Prefer the plainest word that is still precise: simple over clever, concrete over abstract, short over long. A name says what the thing IS in the domain's own vocabulary — no cute coinages, no internal codenames, no needless abbreviations or acronyms; use the SAME term for the same concept everywhere (consistent vocabulary beats variety). Cut every word that doesn't change the meaning. The test: if a reader has to pause to decode a name or read a sentence twice, rewrite it until they don't. This principle governs all authored text — [[scanning]] gives the responsibility-statement mechanics, see [[statement-business]] keeps mechanism out of responsibilities, and symbol names are the exception that proves it: they must be the exact code identifier (see [[symbols]]), clarity there coming from the code itself."#,
    },
    Rule {
        id: 18,
        slug: "build-in-layers",
        title: "Building in layers — commit the skeleton, split coarse claims",
        tags: &[
            "partial", "scaffold", "scaffolding", "layer", "layered", "commit", "committed",
            "pending", "skeleton", "status", "mark_implemented", "incremental",
        ],
        body: r#"A node's presence in the COMMITTED model asserts only that its boundary exists and that its OWN committed responsibilities hold — NOT that everything beneath it is built. A structural node (any node with children) discharges its responsibilities THROUGH its subtree (see [[altitude]]): committing it never claims its unbuilt descendants exist. Unbuilt children simply stay in the plan and roll up as pending. So when you build bottom-up or one slice at a time, DO commit the skeleton you actually built — the spine of nodes plus each responsibility whose code exists — and leave the rest in the plan. The plan↔committed split IS how a partial build is represented; there is no separate status to set and nothing dishonest about it. `mark_implemented` takes `responsibilityIds` precisely for this: fold the responsibilities you finished, not the whole node. A node whose subtree mixes built and unbuilt work shows an intermediate completeness in `get_health` — its anchored primitives over its authored ones (see [[anchor-completeness]]) — the correct, honest state for a layered build, never a problem to avoid by withholding the node. The one real over-assertion trap is a SINGLE responsibility that bundles built + unbuilt behaviour ("streams a reply, invokes tools, and returns structured output" when only streaming exists): folding it claims code that isn't there, withholding it blocks the whole node. Do neither — that statement is one altitude too coarse (see [[altitude]], [[scanning]]). SPLIT it into the built accountability and the unbuilt one(s), commit the built half, leave the rest pending. Never withhold the structural spine just because a responsibility below it is unfinished."#,
    },
    Rule {
        id: 19,
        slug: "anchor-completeness",
        title: "Anchor only what's implemented — the anchor is the completeness signal",
        tags: &[
            "anchor", "anchored", "source-map", "sourcemap", "completeness", "percent",
            "coverage", "implemented", "scaffold", "scaffolding", "boundary", "glob", "primitive",
        ],
        body: r#"An anchor — a responsibility's or symbol's `source_map` file:line, a container/component's boundary glob — records that REAL code discharges the claim. So NEVER anchor a claim you have not implemented yet. Authoring the plan is anchorless; anchoring is the BUILD checkpoint, the act of recording "this exists, here". That discipline makes anchoring the completeness signal: a node's completeness is its ANCHORED primitives over its AUTHORED ones (committed + planned), rolled up over the subtree. The countable primitives are each node's own anchor (a container/component's boundary glob — "the box"; a symbol's definition), each LEAF responsibility, and each data-shape property set. A structural node's OWN responsibilities are NOT counted — they discharge through the subtree (see [[altitude]]). So a container you have scaffolded — boundary glob set over the real directory, but its responsibilities and children not yet anchored — reads as a LOW, non-zero completeness: instantly legible as "scaffolded, not built out". As you implement and anchor each leaf, the number climbs; a node with no anchorable leaf primitives beneath it yet is unmeasured (—), not complete. On a greenfield model nothing is anchored and the denominator is the authored plan, so everything reads 0% — correct: fully specced, nothing built. Because you never anchor vapor, "anchored" is a trustworthy proxy for "implemented" — but it certifies the code EXISTS, not that it fully satisfies the claim: a claim whose code only half-discharges it is still one altitude too coarse (see [[build-in-layers]]), so split it."#,
    },
    Rule {
        id: 20,
        slug: "concerns",
        title: "Concerns tag cross-cutting accountability — at most one per responsibility",
        tags: &[
            "concern", "concerns", "tag", "cross-cutting", "auth", "facet", "lens",
            "idempotency", "observability",
        ],
        body: r#"A responsibility may carry ONE `concern` — a kebab-case slug naming the cross-cutting concern it serves. The standard vocabulary: `auth`, `persistence`, `failure-handling`, `idempotency`, `validation`, `observability`, `performance`, `compliance`. Tag every responsibility that discharges a cross-cutting concern as you author or edit it; leave a node's core domain flow UNTAGGED — no tag means "this is the main behavior", and that absence is signal, not an omission. Reuse before minting: check the model's concern registry and the standard slugs first, and use the SAME slug for the same concern everywhere ([[naming]]'s consistent-vocabulary test) — mint a new slug only for a genuinely distinct concern, and keep it short and domain-generic ("rate-limiting", not "api-rate-limits"). Exactly one concern per responsibility: a statement that seems to need two bundles two accountabilities — split it (see [[altitude]], [[scanning]]). The concern is metadata BESIDE the statement, never wording inside it: with the tag carrying the category, purpose clauses like "…so duplicates aren't reprocessed" are redundant — cut them (see [[scanning]]). Registry entries (slug, description, icon) mint automatically on first use; the user curates them — never edit or delete a registry entry's description or icon yourself."#,
    },
    Rule {
        id: 21,
        slug: "statement-ears",
        title: "Statements speak EARS — condition first, response last, markup on the anchors",
        tags: &[
            "ears", "grammar", "statement", "condition", "trigger", "event", "state",
            "failure", "keyword", "markup", "bold", "wording", "split", "compound",
        ],
        body: r#"A responsibility statement follows EARS (Easy Approach to Requirements Syntax) with the subject dropped: the owning node IS the subject, so never name it and never write "shall". Clause order is FIXED — optional condition first, verb-led response last:
- Ubiquitous (always active): no keyword, just the verb-led response — exactly [[scanning]]'s form. "Authenticate every inbound POST."
- Event-driven: "When <trigger>, <response>."
- State-driven: "While <state>, <response>."
- Unwanted behaviour (failure/rejection handling): "If <condition>, then <response>."
- Optional feature: "Where <feature is present>, <response>" — legal EARS, rarely earned here; prefer modeling the feature as structure.
Clauses stack in that order when a claim genuinely needs both: "While <state>, when <trigger>, <response>". The ubiquitous form is EARNED, never the default: before leaving a claim plain, check whether it actually holds only on a trigger, state, or failure — if so, it MUST take the keyword form; a condition written as a tail ("…after acking the webhook") moves to the front in its keyword form. One pattern per claim: a statement that bundles a happy path with its rejection ("echo the challenge when the token matches, else reject with 403") is TWO claims — the When and the If — split it (see [[altitude]], [[scanning]]). Compound responses split the same way: two verbs acting on two different objects ("persist the results and sync labels to HubSpot") is TWO claims — bolding both verbs is NOT a substitute for splitting; only a genuinely atomic verb pair on one object stays together. A rationale tail ("…so sales receives only qualified ones") is not part of the response — cut it or move it to the description (see [[scanning]]).
Statements also carry a markdown-lite display markup the UI renders — and strips for comparison, search, and word-diffing, so it is presentation, never meaning. Wrap the scan anchors — the grammar keywords (including "then") and the response verb — in **bold**, and NOTHING else: "**When** a send-failure status callback arrives, **append** a failed-send event". A ubiquitous claim bolds its leading verb: "**Authenticate** every inbound POST". Never wrap the condition clause or any other run of text in markers — the keyword and its comma already delimit the clause. Markup belongs in responsibility statements ONLY — never in directives, descriptions, names, or properties."#,
    },
    Rule {
        id: 22,
        slug: "test-attachment",
        title: "Attach tests to the claims you implement — mandatory for symbols, expected everywhere testable",
        tags: &[
            "test", "tests", "attached", "attach", "tested", "testable", "untested",
            "test-map", "unit", "integration",
        ],
        body: r#"A claim either HAS tests attached or it DOESN'T — that binary is the model's highest-trust signal, because an attached test is the one artifact that at least ATTEMPTS to hold the code to the claim's words. Scryer never RUNS tests itself — attachment makes the test findable so a human can audit it — but it does track VERDICTS: ingest a run's JUnit report (`ingest_test_report`) and each attached test's outcome is recorded against its claim, fingerprint-keyed so a later edit to the implementation or the test flips the verdict to stale; `get_test_radius` then names exactly the test files whose claims hold missing or stale verdicts — run those, never the whole suite. A claim in a When/While/If form (see [[statement-ears]]) names a concrete trigger, state, or failure, so a test can exercise it mechanically: arrange the condition, assert the response. When you implement such a claim, write that test in the project's own test suite and attach it in the SAME `mark_implemented` call (`tests`, alongside `anchors`) — implementing and attaching are one checkpoint, not two passes. On a SYMBOL host this is mandatory: a symbol-level testable claim is exactly a unit test's shape, so never fold one without its test. Higher-altitude claims take the kind of test that fits the altitude — component → integration, container → API/service, system → end-to-end. Health counts the gap deterministically: `tested` (claims with a test attached), `testable` (When/While/If claims on code-backed hosts), `untested` (testable claims with no test attached); drive `untested` toward zero on the scopes you build. The attached test must actually exercise the claim — its trigger arranged, its response asserted; never attach a test that merely touches the same code to clear the counter, and never attach one test to many claims it doesn't exercise. Anchor the test by its NAME: `symbol` takes the `it("…")`/`test("…")` description string or the test function's identifier — both resolve and fingerprint the same way. A ubiquitous (verb-led) claim may still deserve a test — that stays a judgment call; attach it the same way (or later via `update_source_map` `test_entries`). The attachment is a link, never executed — but it is fingerprint-tripwired like any anchor: when the test's code changes or vanishes, the claim's test anchor surfaces in health (`test:{id}`), so a silently rotted test is visible without running anything."#,
    },
    // ---- The working loop, one rule per phase. Bodies moved out of the
    // connect-time instructions so a session pays for a phase only when it
    // reaches it. ----
    Rule {
        id: 23,
        slug: "loop-orient",
        title: "ORIENT — find which phase you are in before touching anything",
        tags: &["orient", "phase", "start", "entry", "locate", "health", "directives"],
        body: r#"Figure out which phase you're in first. For a CODING task, start from `orient {task, files}` — one call returns the governing nodes, their claims and binding directives, the scoped pending items and drift, the matching rule slugs, and which phase you're in ([[orient-phases]]). `locate {file, symbol?}` is its single-file sibling. `get_health` reports how well the COMMITTED model maps to code, so it is the right entry point only once code exists: if committed is empty (a design-first model whose whole architecture lives in the plan, before anything is built), it has nothing to report — `get_pending` and `read_model` show the authored plan; never read an empty health report as "nothing authored". The whole-model reads are for model-building sessions: lead with `get_health` to see where work is needed ([[health-reading]]), then `search_model` / `read_model` to load the governing nodes, their responsibilities, and any binding `directives` ([[directives-binding]])."#,
    },
    Rule {
        id: 24,
        slug: "loop-plan",
        title: "PLAN — author the intended change into the model before writing code",
        tags: &["plan", "planned", "intent", "author", "draft", "change"],
        body: r#"Author the intended change into the model BEFORE writing code: add/extend the nodes, responsibilities, and links it implies, at the right altitude ([[altitude]]), with the intent tools (`add_person` / `add_system` / `add_container` / `add_component` / `add_symbol`, `update_nodes`, `add_links`, …). These write the PLAN — a draft on the user's canvas — not code ([[model-layers]]). If the change conflicts with an existing responsibility or directive, surface it; don't silently diverge. Every plan write lands in the session's open change ([[change-ledger]]); with none open the write is refused, so `open_change {rationale}` comes first. Only changes that alter what the model claims need a plan entry at all ([[proportionality]])."#,
    },
    Rule {
        id: 25,
        slug: "loop-sign-off",
        title: "SIGN-OFF — the plan is a proposal until the user approves it",
        tags: &["sign-off", "approve", "go-ahead", "proposal", "user"],
        body: r#"The plan is a proposal on the user's canvas; before building it, tell the user what you planned and get their go-ahead. The user owns the spec ([[user-owns-intent]]) — skip this only when they already approved the change in this conversation or explicitly told you to run ahead. Record the go-ahead with `sign_off`: it snapshots the change as the approved intent, and from then on a claim you reword or add under it lands as vagrant for the developer's verdict at the fold — it does not fold ([[sign-off]], [[fold-after-sign-off]])."#,
    },
    Rule {
        id: 26,
        slug: "loop-build",
        title: "BUILD — implement to the plan, each claim with its test",
        tags: &["build", "implement", "code", "test", "write"],
        body: r#"Implement the code to that plan, responsibility by responsibility, WITH ITS TESTS: for each testable (When/While/If) claim you implement, the statement already names the trigger, state, or failure to arrange and the response to assert — write that test in the project's own suite as part of the same work. On symbol-level claims the test is MANDATORY ([[test-attachment]]); at higher altitudes attach the kind of test that fits (component → integration, container → API/service, system → end-to-end). Never anchor a claim you have not implemented: anchoring is the build checkpoint ([[anchor-completeness]])."#,
    },
    Rule {
        id: 27,
        slug: "loop-close",
        title: "CLOSE — fold what you built, verify its tests, reconcile, continue",
        tags: &["close", "fold", "mark_implemented", "verify", "reconcile", "finish"],
        body: r#"`mark_implemented` what you built (folds it from the plan into the committed model), passing `anchors` and `tests` so fold + anchor + attach is one atomic call, and `flag_drift` anything the code does that the plan didn't capture. The fold is gated on evidence ([[fold-evidence-gate]]); claims reworded or added after sign-off wait for the developer ([[fold-after-sign-off]]); you may fold a node in layers ([[fold-in-layers]]); the response ends with a scoped post-flight to act on ([[fold-post-flight]]). `mark_implemented {change}` folds exactly the session's change; when the last entry folds the change closes and its rationale is recorded in the history log. A change that filed nothing (a bugfix, a refactor) is closed by hand with `close_change {change_id}` so the rationale is still recorded. Then VERIFY: `get_test_radius` names the test files your change invalidated (missing or stale verdicts) — run exactly those with the runner's JUnit reporter on and `ingest_test_report` each report file, so the claims' verdicts are current before you move on ([[test-verdicts]]). Then reconcile and continue: `reconcile_drift` advances the anchor once the scope is clean, and the next responsibility starts the loop again at BUILD."#,
    },
    Rule {
        id: 28,
        slug: "proportionality",
        title: "Proportionality — what earns a plan entry",
        tags: &["proportionality", "ceremony", "bugfix", "refactor", "chore", "spike", "plan-entry"],
        body: r#"Match the ceremony to the change; the full loop is for changes that alter what the model claims. A change is opened for every task regardless ([[change-ledger]]) — the ledger records what was done even when nothing is filed under it, and an empty one is closed with `close_change` at the end. NO plan entry is needed for: bugfixes that restore behaviour the model already claims, pure refactors (moved-but-unchanged symbols re-anchor themselves), or docs/tests/chores. A plan entry IS needed for: new, changed, or removed responsibilities; new nodes; changed links. For exploratory spikes, spiking freely is legitimate — but before the result is kept, reconcile via `flag_drift` so the model catches up; drift exists precisely so the code can lead when it must ([[drift-first]]).

One obligation is NEVER waived, because it is cheap only in the moment: if you changed the behaviour of an anchored symbol, confirm or reword its claims BEFORE you finish, while the diff is still in your context — `locate {file, symbol}` returns just those claims, a few lines to check. Deferred, the same reconciliation costs a later session thousands of tokens to reconstruct."#,
    },
    Rule {
        id: 29,
        slug: "drift-first",
        title: "Drift — why you plan first, and how drift is worked",
        tags: &["drift", "changed", "reconcile", "get_drift", "scope", "sync"],
        body: r#"Drift is a code change the PLAN does not account for. That is why you plan first: code you change in service of a pending plan item is expected churn and stays silent, but changing already-mapped code with no plan item to explain it is flagged the moment you make it — the signal to either revert a mistake or put the change in the plan. `get_drift` reports the scopes whose code changed since the last reconcile — cheap and deterministic (file mtimes + git diff), and a changed file is NOT a verdict that the model drifted, only "re-check this scope". The loop: for each scope, `read_model {node}` to load its claims, compare them against what the changed code now does, then `flag_drift` to record undescribed behaviour and stale claims ([[drift-directions]]). When you have examined every scope, `reconcile_drift` advances the anchor so the same changes don't resurface — it asserts you reviewed everything that changed, so anything you skipped will not come back. A model with no reconcile anchor yet is seeded as in-sync as of now and reports clean."#,
    },
    Rule {
        id: 30,
        slug: "directives-binding",
        title: "Directives are the user's binding HOW-constraints — read them, never write them unasked",
        tags: &["directives", "directive", "constraint", "must", "never", "inherited", "set_directives"],
        body: r#"Directives are user-authored, read-only HOW-constraints ("must"/"never" rules). They attach to a responsibility OR to a node, and node-level directives CARRY DOWN: a node is bound by its own plus every ancestor's. `read_model` returns the inherited set in `inheritedDirectives`; `orient` and `locate` return the binding set for a location. Honor all of them when implementing. `set_directives` is the ONE write path and every other tool leaves directives untouched: call it ONLY when the user has explicitly asked, in this conversation, for directives to be written, edited, or deleted (e.g. a bulk reword they dictated) — never on your own initiative, and never to relax a constraint you find inconvenient while implementing. Each item names a `node_id` (binding that node's whole subtree) OR a `responsibility_id`, plus `directives` as the FULL replacement array (empty clears); it writes the plan layer so the change surfaces in the plan diff for the user to see."#,
    },
    Rule {
        id: 31,
        slug: "user-owns-intent",
        title: "The user owns intent; you are the editor",
        tags: &["intent", "user", "scope", "spec", "editor", "ownership"],
        body: r#"The model is the user's spec; you are the editor. Translating the change the user asked for into the model deltas it implies — the nodes, responsibilities, and links that express it — is your job; do it without asking. Inventing scope BEYOND the request is not: don't add elements the code merely suggests, and if implementing reveals a higher-level boundary is wrong, surface the question rather than silently restructuring. The modeling rules are AUTHORITATIVE: before any modeling judgment — what earns a symbol, how to pitch a responsibility's altitude, when a group is right, how links propagate — fetch the rule and follow it; never infer the conventions from existing nodes."#,
    },
    Rule {
        id: 32,
        slug: "codebase-as-evidence",
        title: "The codebase is evidence, not the source of truth",
        tags: &["codebase", "evidence", "transcribe", "file-tree", "elicit"],
        body: r#"Elicit responsibilities the system already holds; don't transcribe the file tree into nodes. A good responsibility survives a rewrite in another language ("authenticate requests"); a bad one ("uses jsonwebtoken@9") will not ([[statement-business]]). When a description or responsibility names another node, declare the structural link the mention implies — the prose mention and the structural link are distinct; declare both ([[mentions-imply-links]])."#,
    },
    Rule {
        id: 33,
        slug: "model-layers",
        title: "Two layers on disk: the committed model and the planned draft",
        tags: &["layers", "committed", "planned", "plan", "storage", "diff", "pending"],
        body: r#"The committed `model` is the source of truth — what the code is believed to satisfy. The `planned` draft is what you and the canvas edit. Their difference is the PLAN — the model→code work queue (`get_pending`): each entry names an element and what to do — `added` (implement new code), `reworded` (re-implement to the new spec), `moved`, `repointed`, `deleted` (remove the code). Authoring tools write the PLAN; the committed model changes only when work is implemented and folds in (`mark_implemented`), or when you extract from code that already exists (the generation primitives, `descope`). Reads return the PLAN layer by default, so what you read back reflects what you just authored; pass `layer: "committed"` only to inspect what the code currently satisfies."#,
    },
    // ---- The fold. ----
    Rule {
        id: 34,
        slug: "fold-evidence-gate",
        title: "The fold is gated on evidence — a testable claim folds only with a passing verdict",
        tags: &["fold", "gate", "evidence", "verdict", "force", "unverified", "mark_implemented", "untested"],
        body: r#"`mark_implemented` is THE build checkpoint, and it is one atomic statement with three parts: the fold ("I built this"), `anchors` ("here is where it lives"), and `tests` ("here is the test I attached to it") — pass all three in the SAME call rather than folding now and anchoring/attaching later. For a claim in a When/While/If form the test is EXPECTED, not opportunistic — and on a symbol host it is MANDATORY ([[test-attachment]]). A testable claim on a code-backed host folds only with a test attached AND a current passing verdict (report ingested after the last edit to the implementation and the test); otherwise that claim STAYS IN THE PLAN and the response names the missing fact (no test / no verdict / stale / failing) and the test files to run — the order is write test → attach (`tests`, or `update_source_map test_entries`) → run with a JUnit reporter → `ingest_test_report` → fold ([[test-verdicts]]). Leaving a claim pending is an honest exit, not a loop to fight; the rest of the fold proceeds. `force: true` folds anyway and records an `unverified` history event (never the default). Ubiquitous claims stay a judgment call and are not gated; never attach a test that doesn't genuinely exercise its claim just to clear the counter. Folding overwrites the committed claim with the clean planned copy, clearing the `stale` drift flag on anything it folds. Vagrant (code-discovered) claims and properties are never folded: they stay in the plan awaiting the user's adopt/reject verdict ([[vagrant-stale]])."#,
    },
    Rule {
        id: 35,
        slug: "fold-after-sign-off",
        title: "After sign-off, a reword or addition is an amendment — it waits for the developer",
        tags: &["sign-off", "amendment", "addition", "vagrant", "reword", "fold"],
        body: r#"Claims you reworded or added after the developer signed off their change land as vagrant (origin `amendment` or `addition`) for the developer's verdict; they do not fold. If implementing shows a planned claim is wrong, reword it and fold the rest — the reword waits. A claim you dropped after sign-off is restored to the plan for the same verdict. Cosmetic edits (whitespace, markup) are not amendments; a change of statement, host, or directives is ([[sign-off]])."#,
    },
    Rule {
        id: 36,
        slug: "fold-in-layers",
        title: "Fold what you built, not the whole node — skeleton first, claims as they land",
        tags: &["fold", "partial", "layers", "responsibilityIds", "commit_ancestors", "skeleton", "links", "groups"],
        body: r#"You need not finish a whole node before committing: when you build in layers ([[build-in-layers]]), fold only the responsibilities you actually built (`mark_implemented` accepts `responsibilityIds`, and `propertyLabels` for data fields) and leave the rest in the plan. With neither, a node fold takes every planned responsibility and property on the node. Committing a structural node asserts only that its boundary exists, never that its unbuilt descendants do — so commit the skeleton you built and let the pending work roll up; a node whose subtree mixes built and unbuilt work shows an intermediate completeness ([[completeness-layered]]), the honest state for a layered build, never a reason to withhold the skeleton. In a DESIGN-FIRST model (never committed), folding a built leaf is refused while its ancestors are plan-only — pass `commit_ancestors: true` to fold the ancestor chain structure-only first. A whole-node fold also pulls in the plan links touching the node once BOTH endpoints are committed, and any group the node completes; standalone link/group changes — and EVERY link/group DELETION, which never rides a node fold — fold by their own ids (`link_ids` / `group_ids`). A node you DELETED in the plan folds by its node id once the code is gone."#,
    },
    Rule {
        id: 37,
        slug: "fold-post-flight",
        title: "Read the fold's post-flight; a separate validate_model run is for structural sessions",
        tags: &["fold", "post-flight", "validate", "warnings", "pending", "unanchored"],
        body: r#"Every node fold's response ends with a scoped POST-FLIGHT: what's still pending on that node, which of its committed claims lack anchors (an unanchored claim reads as scaffolding and carries no drift tripwire), and any validation warnings this fold introduced — act on those lines. A separate `validate_model` run is for structural sessions that touched many nodes, not for every fold; it runs over your WORKING model (the plan with committed's anchors overlaid) and reports parent-kind mismatches, unknown link endpoints, mixed-level group members, empty symbols, source-map entries on unknown ids, and line ranges that cover their whole symbol. It never judges wording."#,
    },
    // ---- Reading health. ----
    Rule {
        id: 38,
        slug: "health-reading",
        title: "Reading get_health — the test counts lead, everything else supports",
        tags: &["health", "tested", "testable", "untested", "anchor", "changed", "broken", "coverage", "silent", "link-audit"],
        body: r#"`get_health` is deterministic — no semantic judgment. THE HEADLINE NUMBERS ARE THE TEST COUNTS: `tested` (claims with a test ATTACHED), `testable` (claims in a When/While/If form on code-backed hosts, classified from the leading keyword), and `untested` (testable claims with NO test attached — the work queue; drive it to zero on the scopes you build). `tested` is a separate dimension from anchoring, not gated on leafness (a structural claim carrying an integration test counts); anchor observations keyed `test:{id}` are that claim's attached test changing/breaking, not its implementation. Per node: own + subtree rollups of responsibility/property counts, vagrant/stale flags, and anchor coverage (anchorable = any committed claim on LEAF nodes; claims on structural nodes discharge through their subtree and are never "unmapped"). Anchor state comes from the git-free fingerprint check — `changed` (the anchored span differs from what the model last saw), `broken` (the symbol is gone), `fileMissing` — with moved-but-unchanged symbols silently re-anchored; the whole-model summary aggregates anchors per container scope (`anchorSummary.byScope`), the flat per-anchor list appears only on the node-scoped call. The declared-link audit compares links against the extracted import graph (edge_count 0 = asserted-only; `unmodeled` = sibling pairs the code connects but no link declares); `coverage.linkAudit` says which languages resolve FULLY vs by name heuristic — a declared link between name-heuristic files can read asserted-only even when real. `silentAnchors` are sourceMap anchors holding no fingerprint tripwire — drift can never fire for them, so their green is silence, not health. `broadBoundaries` flags boundary globs with no directory prefix (e.g. `**/*`), which silently own every otherwise-unowned file. `totals.stale` counts claims semantically FLAGGED stale by a drift review (`flag_drift`) — it is NOT the anchor tripwire count, so 0 stale next to changed/broken anchors means those anchors AWAIT review. `disconnected` lists architecture nodes no relationship link names as source or target — they read as edgeless on every diagram; wire each into the relationship it actually performs, or confirm it belongs (symbols are exempt). `edgeGraph` says whether the link audit had a dependency cache to work from; absent means run a model build first. Use the report to decide WHERE work is needed before reading full subtrees; `completeness` is explained in [[completeness-layered]]."#,
    },
    Rule {
        id: 39,
        slug: "completeness-layered",
        title: "Completeness is anchored primitives over authored ones — honest for a layered build",
        tags: &["completeness", "pct", "percent", "primitive", "greenfield", "scaffold"],
        body: r#"Per node, `completeness` is how much of the node's AUTHORED subtree (committed + planned) reads through to real code, so it is defined from greenfield onward. `pct` (0–100) is anchored primitives over authored ones, where a primitive is a node's boundary box (counted only when its glob owns a real file), a leaf responsibility, or a data shape (counted when its anchor resolves and is not broken/missing); a scaffolded container reads low but non-zero, greenfield reads 0. `pct` is ABSENT ("—", unmeasured) when the subtree has no leaf primitives (a bare box), so an undecomposed shell never reads 100%. Only anchor what you have implemented — that discipline is what makes the figure trustworthy ([[anchor-completeness]])."#,
    },
    // ---- Drift findings. ----
    Rule {
        id: 40,
        slug: "drift-directions",
        title: "Every drift finding has a direction: take-code (undescribed) or take-model (stale)",
        tags: &["drift", "flag_drift", "undescribed", "stale", "vagrant", "proposedStatement", "diverged"],
        body: r#"`flag_drift` records SEMANTIC drift for a node after comparing its code against its responsibilities. `undescribed` is the *take-code* direction: behaviours the code has that NO responsibility describes — each is proposed into the PLAN as a vagrant adoption (a code-discovered `added` claim), which the user adopts (commit — the code already exists) or rejects (mark the code for deletion); do NOT report code that changed but still satisfies an existing responsibility. `stale` is the *take-model* direction: existing responsibilities the model still asserts but whose code regressed — flagged so the user can give a verdict: re-implement (the model is right, the code is rebuilt) or drop (the behaviour was removed on purpose). When the behaviour did NOT vanish but DIVERGED — the code still does a related thing, just differently than the claim says — also set `proposedStatement` to the corrected wording that matches what the code now does; the user can accept it (folding the new wording with no rebuild) instead of choosing re-implement/drop. Omit `proposedStatement` when the behaviour is truly gone. Call with empty arrays (or don't call) when the code and the model still agree. Where a finding lands is [[drift-homing]]; whole nodes are [[drift-stale-nodes]]; data fields are [[drift-properties]]."#,
    },
    Rule {
        id: 41,
        slug: "drift-homing",
        title: "Home each undescribed finding at its true altitude — mint the missing rungs",
        tags: &["drift", "homing", "nodeId", "nodeKey", "newNodes", "mint", "altitude"],
        body: r#"Each undescribed finding is HOMED on a node: it routes automatically to the finest node that already owns its `symbol`/file (falling back to the reviewed container), or set `nodeId` to force an existing node. When the model has NO node for the code, MINT the missing rungs in `newNodes` (a `key`, `kind`, `name`, and a parent via `parentId` on an existing node or `parentKey` on a shallower mint — list ancestors first) and point the finding at the leaf with `nodeKey`, so it lands at its true altitude instead of bubbling up to the reviewed container. Minted nodes are vagrant in the plan: they fold when a claim hung on them is adopted and drop on reject."#,
    },
    Rule {
        id: 42,
        slug: "drift-properties",
        title: "A data field drifts as a property, never as a responsibility",
        tags: &["drift", "properties", "field", "undescribedProperties", "staleProperties", "data"],
        body: r#"Properties have the SAME two directions as behaviour. A newly-declared struct field / interface member that no property describes is DATA, not behaviour — report it under `undescribedProperties` (its `label`, `sourceFile`, enclosing `symbol`, homed like `undescribed`) so it lands as a vagrant property, NEVER as a responsibility ([[symbols]]). A property whose backing field was removed or materially changed goes under `staleProperties` (`nodeId` + `label`, since properties have no id)."#,
    },
    Rule {
        id: 43,
        slug: "drift-stale-nodes",
        title: "When a node's backing code is gone entirely, flag the node, not each claim",
        tags: &["drift", "staleNodes", "deleted", "file", "subtree", "node"],
        body: r#"`staleNodes` is the node-level take-model direction: when a deleted file or folder wipes out a whole modeled node — a symbol, a component, an entire container subtree — flag the NODE (by `nodeId`) instead of listing each of its claims; the verdict then applies to the whole subtree. Use `staleNodes` when the node's backing code is gone entirely, `stale` when only one of a still-present node's claims regressed ([[drift-directions]])."#,
    },
    // ---- Changes and sign-off. ----
    Rule {
        id: 44,
        slug: "change-ledger",
        title: "A change is a named partition of the plan carrying the developer's rationale",
        tags: &["change", "open_change", "close_change", "refile", "rationale", "unfiled", "resume", "workstream"],
        body: r#"A change is a named partition of the plan, and every task beyond a one-line fix opens one before anything else: `open_change {rationale}` (the task in one sentence, as the dev put it) registers it and points this session's plan writes at it, so parallel workstreams stay separable and review/fold can work per task; `open_change {change_id}` resumes an open one from a prior session (listed in `get_pending`'s `openChanges`). Every plan write is tagged to the session's change automatically, and REFUSED while no change is open; `mark_implemented {change}` folds exactly its entries, and the change closes when its last entry folds — the rationale survives in the history log. `close_change {change_id}` closes an EMPTY stranded change as abandoned — refused while it has tagged entries. `refile {ids, to}` MOVES work that is already pending into another change — a node/group id takes that carrier and everything pending under it, a responsibility/link id takes just that element, a change id takes everything filed under it, and "unfiled" takes everything untagged; `to` accepts a change id or "unfiled" and defaults to this session's change. Use it when work landed in the wrong change or one task turns out to be two — never re-write elements just to re-file them. The session's selection is per project and in-memory: a fresh session re-selects."#,
    },
    Rule {
        id: 45,
        slug: "sign-off",
        title: "Sign-off snapshots the approved intent; later edits are amendments",
        tags: &["sign-off", "snapshot", "approved", "intent", "amendment", "sign_off"],
        body: r#"`sign_off` (with `change_id`, or alone for the current change) records that the developer approved the plan: it snapshots the change's entries — statement, host, and directives of each — as the approved intent. From then on any claim the agent rewords or adds under the change lands as vagrant (origin `amendment` / `addition`) for the developer's verdict at `mark_implemented` instead of folding, and a dropped signed-off claim is restored for the same verdict ([[fold-after-sign-off]]). The developer's own canvas saves re-stamp the snapshot, so only agent edits count as amendments. `get_pending` and the plan diff mark entries AMENDMENT / ADDITION / DROPPED against the snapshot."#,
    },
    // ---- Evidence. ----
    Rule {
        id: 46,
        slug: "test-verdicts",
        title: "Scryer never runs tests; it records verdicts and computes the radius to re-run",
        tags: &["verdict", "junit", "ingest_test_report", "get_test_radius", "stale", "failing", "fingerprint", "radius"],
        body: r#"Scryer never RUNS tests itself, but it tracks their VERDICTS: after a run, `ingest_test_report {path}` reads the runner's JUnit XML and records each attached test's result against its claim — ONE call per report file, never per test — fingerprint-keyed on the claim's implementation and attached tests, so a later edit to either flips the verdict to stale (no watcher, nothing re-runs). The response says what the report settled (recorded, failing) and what it did not — unmatched cases (normal: attachment is curated, the suite is not), ambiguous names, attachments the report never mentioned (normal for a partial run) — plus the remaining radius. Any runner that emits JUnit XML works: vitest/playwright `--reporter=junit`, pytest `--junitxml=`, jest-junit, cargo-nextest, gotestsum, surefire. `get_test_radius` answers "what needs running": exactly the test files whose claims hold missing or stale verdicts, never the whole suite; an empty radius means every attached claim holds a current verdict, and claims with NO attached test never appear — that gap is health's `untested`. The `untested` count in every status line is your standing work signal; `tests: N failing/stale` joins it only when something needs attention."#,
    },
    Rule {
        id: 47,
        slug: "source-map",
        title: "The source map: anchors, attached tests, schema declarations, boundaries",
        tags: &["source-map", "sourcemap", "anchor", "entries", "test_entries", "schemas", "boundaries", "line", "range", "glob"],
        body: r#"`update_source_map` writes the code-side mapping (agent-produced, regenerable). `entries` set source locations keyed by responsibility id — the conformance numerator (where reality discharges a responsibility). Each location is the SPECIFIC line range that does the work: `pattern` = file, `line`/`endLine` = the range, `symbol` = the enclosing definition (anchor + context). A line range must be a PROPER subset of its symbol — when one responsibility is the whole definition's work, omit `line`/`endLine` (a symbol-only anchor means the whole definition); ranges that cover the whole symbol are normalized to symbol-only and reported back. `test_entries` ATTACH TESTS to claims — keyed by responsibility id like `entries`, pointing at the test that exercises the claim (`pattern` = test file, `symbol` = the test function or its `it("…")` string; symbol-only means the whole test) — a separate dimension from where a claim is implemented, and the tool to attach a test AFTER a fold (the fix for an `untested` callout). `schemas` set the declaration location of a schema-kind node (properties, not responsibilities) keyed by node id: `pattern` = file, `symbol` = the type name. `boundaries` set directory globs keyed by node id — the coverage denominator (the code region a node owns); use for containers/components, keeping a child's boundary within its parent's. An empty `locations`/`sources` array clears an entry. Anchoring a PLAN-ADDED claim before it is committed is premature — the build checkpoint is the fold ([[anchor-completeness]])."#,
    },
    // ---- Generation, probing, orient. ----
    Rule {
        id: 48,
        slug: "generation-fill",
        title: "Generating a model from code: seed the skeleton, then fill each container atomically",
        tags: &["generation", "generate", "fill_container", "replace_model", "replace_subtree", "replace_groups", "read_codebase", "extract"],
        body: r#"If no model exists yet, build one first: `read_codebase` to see the codebase (deployable units, data stores, external integrations, frameworks), then build top-down. The generation primitives write BOTH layers — the subtree describes code that already exists, so it lands as built, never as a pending queue: `replace_model` seeds the system + container skeleton (JSON with `version: "0.3"`, `nodes`, `links`, optional `groups`/`sourceMap`; warnings are returned but the write commits regardless), `replace_subtree` replaces one node's whole subtree, `replace_groups` creates groups in bulk. `fill_container` fills the complete component + symbol model for ONE existing container in a single write — you never assemble ids by hand: components and nested symbols use unique request-local `key` values, group `memberKeys` are local component keys. You do NOT author code-level links: the server wires component→component and symbol→symbol links from the deterministic dependency graph; the optional `links` field is only for cross-boundary relationships (to an external/other-container node id) the graph can't infer, and any link it can't place legally is dropped and reported, never fatal. The server validates the proposal, mints ids, resolves references, derives links, and performs one write; structural problems (missing symbols, duplicate keys) reject the whole proposal with a specific reason — fix exactly that and retry. Filling records structure and anchors only — it attaches no tests, so testable claims land as `untested`: immediately after a fill, attach the tests the codebase ALREADY has via `update_source_map test_entries`; claims with no existing test stay honestly untested until one is written ([[test-attachment]]). For interactive editing use the typed add_*/update_*/move_* tools instead."#,
    },
    Rule {
        id: 49,
        slug: "probe-loop",
        title: "Probing a claim: would its test actually fail if the code stopped honouring it?",
        tags: &["probe", "open_probe", "close_probe", "falsification", "mutation", "worktree", "survivor", "subagent"],
        body: r#"A green verdict says the test passes; it does not say the test would notice a defect, and a test that asserts nothing passes forever. `open_probe` answers that. DELEGATE IT TO A SUBAGENT on a cheap model — the mutate/run/revert loop is repetitive, produces a lot of test output, and none of it belongs in the context of the session that asked. Nothing happens in the developer's working tree: scryer syncs an isolated git worktree (their uncommitted work included) and returns ITS path, so every edit and every test run happens THERE. The call returns the claim's statement, the worktree, the exact file and line span to break, and the attached test files — then make ONE deliberate breaking edit inside that span, run ONLY those test files, and expect RED. Green means the break survived: the test does not hold the claim, and that is the finding. Aim each break at what the claim actually SAYS (a When/If claim names a trigger and a response — attack those), not at whatever is easiest: deleting a function body proves only that something notices. Up to three distinct breaks, stopping early on the first survivor, then `close_probe {probes, survivors}` — always, including when a probe went wrong: it resets the worktree and records the round. No survivors means the claim reads as probed, NOT proven — you sampled, you did not exhaust. Survivors are the real finding: strengthen the test, re-run for a fresh verdict, probe again. The result is fingerprint-keyed like a verdict. Refused when the claim has no attached test, when its verdict is missing, stale, or not passing, or when the project is not a git repository."#,
    },
    Rule {
        id: 50,
        slug: "orient-phases",
        title: "What orient returns, and what its phase verdict tells you to do",
        tags: &["orient", "phase", "plan-execution", "reconcile", "free", "matches", "scope"],
        body: r#"`orient {task, files}` returns, scoped to what you're about to do: per file the governing node chain, anchored claims (each flagged `untested` when it is testable with NO test attached — the gap you are expected to close as you work here), and BINDING directives (own + inherited, same as `locate`); per task the best-matching model nodes with their responsibilities and an `untestedClaims` count; the pending plan entries touching that scope (the work queue you may be executing); the drift scopes inside it; the matching rule slugs; a `phase` verdict; and the whole-loop `state` line. The phases: `plan-execution` — pending intent exists in this scope: implement it, tests included, then fold; `reconcile` — code changed outside the plan: compare against the claims, `flag_drift` findings, `reconcile_drift` when done; both — get_drift the scope before building on it; `free` — model and code agree: plan model deltas first if your change alters what the model claims ([[proportionality]]), and claims marked `untested` are standing work. `locate {file, symbol?}` is the single-file form: the claims anchored there (a TEST file returns the claims it is attached to, marked `viaTest`), the owning node chain finest-first, the boundary owner, binding directives, pending entries touching those elements, and `scopeHealth`. Both read the working view, so claims you just authored are visible."#,
    },
    Rule {
        id: 51,
        slug: "descope-vs-delete",
        title: "descope removes from the model; delete_nodes stages removal of the code",
        tags: &["descope", "delete", "delete_nodes", "remove", "boilerplate", "entry-point"],
        body: r#"`delete_nodes` is a forward modeling intent: the CODE the nodes model should go away. Descendants, connected links, and source-map entries go with them, and the deletion shows as pending until you remove the code and fold it with `mark_implemented`. `descope` is the model-only correction: the code is fine and stays untouched, it just shouldn't be modeled (an entry-point `main`, a trivial wrapper, generated boilerplate). Each target's own responsibilities relocate up to its parent component, keeping their anchors, so the parent still covers that code and no darkness appears; the node and its descendants are then removed from BOTH layers at once, so nothing enters the pending queue. Reach for `descope` when the model over-claims relative to code reality, `delete_nodes` when the code itself should go."#,
    },
    Rule {
        id: 52,
        slug: "styles",
        title: "Every container declares a style; every component carries one of its layers",
        tags: &[
            "style", "layer", "hexagonal", "feature-sliced", "core-shell", "pipeline",
            "architecture", "import", "dependency", "placement", "path", "domain",
            "application", "infrastructure", "presentation",
        ],
        body: r#"A style is the model's horizontal axis — the fixed vocabulary that says what KIND of thing a container is and what ROLE each component plays inside it, the way EARS fixes a claim's shape and a concern fixes a slug's meaning. Pick the style when you add the container: `hexagonal` for services, backends and library cores (layers presentation, infrastructure, application, domain); `feature-sliced` for SPAs, docs and static sites (app, pages, widgets, features, entities, shared); `core-shell` for CLIs, small libraries and script repos (shell, core); `pipeline` for ETL and dbt work (source, staging, intermediate, marts). There is no unstyled container. Give every component its layer when you add it — the tools reject a layer outside the style's list. Symbols inherit their component's layer.
The style is enforced, not advised: `add_links` rejects a link the layer matrix forbids (in hexagonal: domain depends on nothing outside domain; application on domain; infrastructure on application and domain; presentation on application; never presentation ↔ infrastructure), a same-layer link between two components must be `kind: uses`, and a link entering a container from outside lands on its inbound layer (presentation or application; app or pages; shell). Health then checks the CODE the same way from the real import graph — a domain file importing an infrastructure file, a banned package (domain importing React or sqlx), a file sitting on another layer's path — and every such line is a real import or file to fix, never a judgment call.
Placement is not yours to choose: `orient` and the read overlay give one line per file — its layer, what it may import, the directory its layer lives in. Put new files there. `scaffold` returns the manifest for a planned node (path, layer, symbols to define, layers it may import); materialise it in the project's own language and idiom, then anchor. Whether a component is REALLY domain or application stays your judgment: domain is the business model and its rules with no I/O and no framework; application is the use cases and the ports they expose; infrastructure is the adapters the application drives; presentation is what drives the application."#,
    },
];

/// The full ruleset as one numbered block — for AI-review prompts and any
/// consumer that wants the complete text rather than a lookup.
pub fn rules_full() -> String {
    let mut s = String::new();
    for r in RULES {
        s.push_str(&format!("{} — {}. {}\n", r.slug, r.title, r.body));
    }
    s
}

/// Resolve one rule by its slug — the exact lookup behind `get_rules {id}`.
/// A numeric id still resolves so a stale citation degrades to the right rule
/// instead of nothing.
pub fn get(slug: &str) -> Option<&'static Rule> {
    let key = slug.trim();
    if let Ok(n) = key.parse::<u8>() {
        return RULES.iter().find(|r| r.id == n);
    }
    RULES.iter().find(|r| r.slug == key)
}

/// Every `[[slug]]` citation in a body of text, in order of appearance.
/// Descriptions and rule bodies cite each other this way, so a wiring test
/// can walk the graph and prove every citation resolves.
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

/// A compact, drillable index — one line per rule (slug, title, tags). The
/// agent pulls a rule's full `body` on demand with `get_rules {id}`.
pub fn rules_index() -> String {
    let mut s = String::new();
    for r in RULES {
        s.push_str(&format!("{} — {} [{}]\n", r.slug, r.title, r.tags.join(", ")));
    }
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
                || r.slug.split('-').any(|w| word_hit(t, w))
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
        s.push_str(&format!("{} — {}\n{}\n\n", r.slug, r.title, r.body));
    }
    s.trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_are_unique_and_get_resolves_every_one() {
        let mut seen = std::collections::HashSet::new();
        for r in RULES {
            assert!(seen.insert(r.slug), "duplicate slug {}", r.slug);
            assert!(
                r.slug.chars().all(|c| c.is_ascii_lowercase() || c == '-' || c.is_ascii_digit()),
                "slug {} is not kebab-case",
                r.slug
            );
            assert_eq!(get(r.slug).map(|x| x.id), Some(r.id));
            assert_eq!(get(&r.id.to_string()).map(|x| x.slug), Some(r.slug), "numeric alias");
        }
        assert!(get("no-such-rule").is_none());
    }

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

    #[test]
    fn lookup_matches_slug_words() {
        assert!(lookup("post-flight").iter().any(|r| r.slug == "fold-post-flight"));
        assert!(lookup("sign-off").iter().any(|r| r.slug == "sign-off"));
    }

    #[test]
    fn lookup_resolves_topics_via_title_and_tags() {
        // The empty-symbol incident: "symbol" must reach the symbols rule (the bar).
        let hits = lookup("symbol");
        assert!(hits.iter().any(|r| r.id == 8), "symbol → symbols rule");

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
        // and despite the concerns rule being dead last in rule order.
        let hits = lookup("tag the concerns for the model");
        assert_eq!(hits.first().map(|r| r.id), Some(20), "concerns rule outranks stopword noise");

        // Plural/stem forms reach the singular vocabulary.
        assert!(lookup("links").iter().any(|r| r.id == 5), "links → link (one-link)");
        assert!(lookup("groups").iter().any(|r| r.id == 4), "groups → group (groups)");
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
