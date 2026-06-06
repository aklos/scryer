/// Modeling rules — single source of truth for MCP instructions and AI review prompts.
pub const RULES: &str = "\
1. Responsibilities are pure business statements. A responsibility says what a node is accountable for in business terms — \
not how it does it. \"restricts access to private content\" — yes. \"restricts access via JWT\" — no, the \"via JWT\" is \
mechanism. Same for technology names, library calls, specific protocols. Keep mechanism out of the statement entirely. The \
`directives` field beside a responsibility holds prescriptive \"must\"/\"never\" constraints (\"must verify ownership \
server-side\", \"never trust a client-supplied role\") — but directives are authored by the user, not the agent. Treat any \
directives present as binding constraints you must satisfy when implementing; never write, edit, or delete them. Where reality \
discharges a responsibility belongs in the source map, not in directives.\n\
2. Every node justifies its existence through responsibilities it serves. A child node exists to discharge a subset of its \
parent's responsibilities. A node with no responsibility, or whose responsibilities serve no ancestor commitment, is \
structurally vagrant — prune it or restate its purpose.\n\
3. Decompose for checkability — and keep each node's responsibilities at its OWN altitude. A responsibility names one accountability \
the node holds, never the handler that discharges it; the handlers are the children one level down (a container's components, a \
component's symbols). So a parent's responsibilities are FEWER and BROADER than the union of its children's — never a per-child \
enumeration of what each child does. Test each line: if it reads as describing a single child, it is one altitude too low — lift it \
to the accountability those children collectively serve, or push it down onto the child. If a responsibility is too coarse to verify \
at the parent's altitude, add child nodes whose responsibilities together discharge it. The node tree IS the responsibility tree, \
refined downward.\n\
4. Groups organize peers along a secondary axis — they never substitute for parent/child decomposition. If the members only \
make sense as parts of the group, not as independent entities, the group is a missing parent node — promote it and make the \
members children. Two flavors: **Logical** (no responsibilities) — organizational signal like team ownership, feature areas, \
or module colocation. Agents should respect these when structuring code (e.g. keeping grouped components in the same directory) \
even though the group carries no explicit commitments. **Architectural** (has responsibilities) — a cross-cutting concern like \
a deployment boundary. Responsibilities describe what the *grouping relationship* enforces, not what individual members do.\n\
5. One link per relationship. Direction points from initiator/requester toward provider/dependency. Two links between the same \
pair of nodes are valid only when they represent independent relationships.\n\
6. Containers are runtime boundaries. Each separately deployable process is at least one container. An embedded runtime \
(e.g. a webview inside a native shell, a scripting engine inside a host process) is NOT a separate container — it is a \
component of the host container. Group containers that deploy together.\n\
7. Components map to code structures (classes, modules, packages, folders). Third-party libraries are not components — they're \
implementation details mentioned in `technology`.\n\
8. Code level uses only the `symbol` kind. A `symbol` is exactly one addressable code definition — a function, \
method, handler, hook, React component, class, struct, interface, type, or config object. One symbol node = one \
definition in the source; its name must be the identifier as it appears in the code. A definition earns a symbol \
node only when it carries architecture — a behavioral responsibility at its OWN altitude, a declared data shape, or \
a cross-boundary link. Being a real, public definition is NOT sufficient: trivial pass-through wrappers, thin \
re-exports, getters/setters, and test stubs do NOT earn a node — fold whatever they do into their component's \
responsibilities rather than minting a leaf for each. Prefer a component with a handful of meaningful symbols over \
one mirroring every definition in its files. A symbol has two independent \
facets; most carry one, some carry both:\n\
   - **responsibilities** — the behavior the definition discharges. Map each to the specific LINE RANGE that does its \
work (with the enclosing `symbol` named as anchor and context) — not to the whole symbol. Two responsibilities sharing \
an enclosing symbol must point at different line ranges; if they point at the identical range they are one \
responsibility.\n\
   - **properties** — when the definition DECLARES A DATA SHAPE (a struct/class/interface/type, or a config object \
that defines a field schema), list its fields here: one property per field, each with a status. Map the declaration to \
the symbol's node id via the `schemas` source array (not `entries`).\n\
   NEVER describe a data shape in a responsibility statement. \"Defines the lead record schema with status, qualification, \
and booking fields\" is WRONG — that prose belongs nowhere; enumerate `status`, `qualification`, `booking`, … as actual \
`properties`. A config object that both wires behavior and declares fields (e.g. an ORM/CMS collection that registers \
admin UI and lifecycle hooks AND defines the record's fields) gets BOTH facets: behavioral responsibilities for the \
hooks/UI, and a property per declared field. A pure data type carries only properties. Symbols carry no boundary glob \
(that is a container/component concept). **Scoping**: determine parentage from the import/usage graph, not from file co-location. A code-level \
node belongs to whichever component actually consumes it. If multiple components import the same code, parent it to the \
one that owns/defines it and add links from the others — the cross-boundary dependency is valuable signal.\n\
9. External systems (`external: true`) are opaque, no children. Any responsibilities listed on an external are read \
as expectations of that external, not commitments by your system.\n\
10. Mentions imply links. A responsibility statement that names another node requires a link to it.\n\
11. System boundary = ownership boundary. Everything you build and deploy from one codebase is containers inside one system. \
\"Separate deployment unit\" does NOT mean \"separate system.\"\n\
12. `technology` is node identity (\"Payload 3.0\", \"PostgreSQL 16\", \"S3 Bucket\"). Separate from `directives` on \
responsibilities, which are user-authored constraints prescribing how a responsibility must be discharged — never set by the \
agent. Do not put technology vocabulary inside responsibility statements.\n\
13. Status lives on responsibilities and on `properties`, not nodes. Values: `proposed` (planned, no code yet), `implemented` (code exists), \
`verified` (production-ready, checked against code), `changed` (spec was modified after implementation — needs re-implementation). \
Lifecycle: proposed → implemented → verified. Editing a responsibility's statement or directives while status is \
`implemented` or `verified` flips it to `changed`. After re-implementation, `changed` returns to `implemented`. \
A node's lifecycle is the aggregate of its responsibilities and properties. Always set status explicitly on each responsibility and each property.\n\
14. The `vagrant` flag (separate from status) marks responsibilities discovered in code that no upstream commitment justifies. \
A vagrant responsibility is always added by automation with `status: implemented, vagrant: true`. The user adopts it (clear \
the flag) or rejects it (delete it, signaling the agent to remove the code).\n\
15. Write for scanning, not prose. A responsibility is ONE verb-led clause: lead with the specific verb + object that \
distinguishes it, then stop. Cut words that merely restate the node's own domain — in an architecture tool every line is \
about \"the architecture model\", so naming it adds nothing — and cut trailing \"by/through/where/so that …\" clauses \
(mechanism belongs out per rule 1; the obvious belongs cut). \"Renders the node/link/group canvas\" — yes. \"Renders the \
visual architecture editor where users arrange nodes, links, and groups on a canvas\" — no. A `description` is the node's \
IDENTITY in a few words (what it IS as software), never a summary of the responsibilities listed beneath it; if it reads as \
a comma-list of those responsibilities, drop it.\n\
16. Relationships connect nodes at the same C4 level — each diagram tells one level of the story, and `add_links` ENFORCES \
this (an illegal link is rejected, not saved). A link is legal only when src and dst are siblings (same parent), OR the \
deeper node's parent already links to the other node — which makes that node a *reference* on the deeper node's surface. \
References thus propagate DOWN from higher-level links: at system context a person/external links to the SYSTEM; to also wire \
it to a specific container or component, the relationship must exist at every level in between. So when an external is used \
deep inside your system, add the link at EACH level: system→external, then container→external, then component→external — each \
one authorizes the next. Two consequences: (a) you cannot link a deep node straight to a top-level external without the \
intervening links — add them parent-first (a single `add_links` batch may include all the levels at once); (b) every node \
still needs a relationship at its OWN level, or it appears disconnected on its own diagram. You may model a relationship only \
as deep as it is useful — a `container → external` link need not be refined to the component that calls it; the external simply \
won't appear inside the container view until a component links to it, and that is fine. Never link a node to its own \
ancestor/descendant — nesting already expresses containment. Run `validate_model` and fix every warning before finishing.\n\
";
