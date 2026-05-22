/// Modeling rules — single source of truth for MCP instructions and AI review prompts.
pub const RULES: &str = "\
1. Responsibilities are pure business statements. A responsibility says what a node is accountable for in business terms — \
not how it does it. \"restricts access to private content\" — yes. \"restricts access via JWT\" — no, the \"via JWT\" is \
mechanism. Same for technology names, library calls, specific protocols. If the implementation matters, put it in the \
responsibility's `implementationRules` field beside the statement, never in the statement itself.\n\
2. Every node justifies its existence through responsibilities it serves. A child node exists to discharge a subset of its \
parent's responsibilities. A node with no responsibility, or whose responsibilities serve no ancestor commitment, is \
structurally vagrant — prune it or restate its purpose.\n\
3. Decompose for checkability. If a responsibility is too coarse to verify at the parent's altitude, add child nodes whose \
responsibilities together discharge it. The node tree IS the responsibility tree, refined downward.\n\
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
implementation details mentioned in `technology` or `implementationRules`.\n\
8. Code level uses only `operation` and `model` kinds. An operation is the smallest behavioral unit inside a component — \
a function, handler, hook, or UI sub-component (e.g. a React component used only within one module). Operation names must \
be valid identifiers in the target language. Model nodes carry `properties` (field declarations) instead of \
`responsibilities`. **Scoping**: determine parentage from the import/usage graph, not from file co-location. A code-level \
node belongs to whichever component actually consumes it. If multiple components import the same code, parent it to the \
one that owns/defines it and add links from the others — the cross-boundary dependency is valuable signal.\n\
9. External systems (`external: true`) are opaque, no children. Any responsibilities listed on an external are read \
as expectations of that external, not commitments by your system.\n\
10. Mentions imply links. A responsibility statement that names another node requires a link to it.\n\
11. System boundary = ownership boundary. Everything you build and deploy from one codebase is containers inside one system. \
\"Separate deployment unit\" does NOT mean \"separate system.\"\n\
12. `technology` is node identity (\"Payload 3.0\", \"PostgreSQL 16\", \"S3 Bucket\"). Separate from `implementationRules` on \
responsibilities, which describe how a specific responsibility is discharged. Do not put technology vocabulary inside \
responsibility statements.\n\
13. Status lives on responsibilities, not nodes. Values: `proposed` (planned, no code yet), `implemented` (code exists), \
`verified` (production-ready, checked against code), `changed` (spec was modified after implementation — needs re-implementation). \
Lifecycle: proposed → implemented → verified. Editing a responsibility's statement or implementationRules while status is \
`implemented` or `verified` flips it to `changed`. After re-implementation, `changed` returns to `implemented`. \
A node's lifecycle is the aggregate of its responsibilities. Always set status explicitly on each responsibility.\n\
14. The `vagrant` flag (separate from status) marks responsibilities discovered in code that no upstream commitment justifies. \
A vagrant responsibility is always added by automation with `status: implemented, vagrant: true`. The user adopts it (clear \
the flag) or rejects it (delete it, signaling the agent to remove the code).\n\
";
