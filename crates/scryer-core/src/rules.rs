/// C4 modeling rules — single source of truth for AI review prompts and MCP instructions.
pub const RULES: &str = "\
1. One edge per relationship. Edges represent dependencies or interactions between nodes, \
not individual API calls or data flows. Do NOT add \"return\" or \"response\" edges — a single \
edge with an arrow captures the dependency direction.\n\
2. Arrow direction = dependency. The arrow points from the initiator/requester toward the \
provider/dependency (e.g. \"Web App\" → \"API Server\" → \"Database\").\n\
3. Descriptions match abstraction level. System = high-level purpose (\"Handles user authentication\"). \
Container = what it deploys as (\"Spring Boot REST API\"). Component = specific responsibility \
(\"Password hashing service\").\n\
4. Technology labels must be accurate and concise (max 28 characters). Don't label a database container with \"React\" or a \
frontend with \"PostgreSQL\". Technology describes the implementation, not what it talks to.\n\
5. External systems are opaque. They should not have child nodes. They represent third-party \
systems the team doesn't control.\n\
6. No frontend-to-database shortcuts. A frontend container should talk to an API/backend, \
not directly to a data store. If the diagram shows this, flag it.\n\
7. One node per real thing. Don't duplicate nodes at the same level to represent the same \
system/container/component.\n\
8. Cross-level edges are intentional. The model stores all abstraction levels together. \
A Person→System edge (system level) and a Person→Container edge (container level) can coexist — \
the system-level edge is correct at that zoom level, and the container-level edge adds detail. \
Similarly, a Container→System deployment edge is not redundant with a System→System edge. \
Do not flag cross-level edges as duplicates or suggest removing them.\n\
9. Split for coherent inner graphs. The test for whether something should be one container or two: \
would the component-level view make sense? If combining two concerns would produce an inner graph \
with unrelated components mixed together, split them into separate containers. Each container should \
tell one coherent story at component level. Example: a Next.js app with Payload CMS should be separate \
containers (\"Website\" + \"CMS Admin\") because their components are entirely unrelated — page routes \
vs admin panels vs content schemas. Use a deployment group if they ship together. The container diagram \
captures logical separation; groups handle deployment topology.\n\
10. Auto-generated framework layers are not containers. If something only exists as an implementation \
detail of another container (e.g. Payload CMS REST API, Django admin ORM, Rails ActiveRecord) and \
cannot be addressed independently, it's a component, not a container. But if it has its own distinct \
set of concerns that would clutter the parent's component view, it may warrant its own container — \
apply rule 9.\n\
11. Components map to code structures. A component should correspond to a concrete code unit: \
a class in OOP languages (C#, C++, Java), a module or package in Go/Rust/Python, or a folder/file \
boundary in JavaScript/TypeScript. If a component is too abstract to point at a specific place in \
the codebase, it is probably a container or a vague grouping that should be rethought.\n\
12. Message queues and topics are explicit. A queue, topic, or event bus (e.g. RabbitMQ, Kafka, \
SQS) should be its own container node — not hidden inside an edge label. If service A publishes \
to a queue and service B consumes from it, model as A → Queue → B, not A → B with a \"via queue\" \
label. The queue is infrastructure that can fail, scale, and be monitored independently.\n\
13. Node names describe roles, not technology stacks. A node name should say what it IS \
(\"Website\", \"CMS\", \"API Gateway\"), not list its technologies with \"+\" or \"&\". \
Technology details belong in the technology field. If a container uses multiple frameworks \
that run as a single unit (e.g. a CMS embedded in a web framework), suggest a clearer role-based \
name rather than suggesting a split.\n\
14. Parent-child nesting IS the system-to-container relationship. In C4, expanding a system \
reveals its containers — the parent_id field captures this. A system node should NOT have edges \
to its own child containers. Such edges are redundant with nesting and are not a modeling \
omission. Do not suggest adding them.\n\
15. Do not suggest reorganizing valid decompositions. If the author has separated concerns into \
distinct containers with clear role-based names and different responsibilities, that decomposition \
is intentional. Do not suggest splitting or merging containers based on technology assumptions \
about how the underlying code is structured.\n\
16. System boundary = ownership boundary. A system in C4 represents a codebase or product owned by one \
team. Everything you build and deploy from that codebase — web apps, APIs, Lambda functions, workers, \
cron jobs, CLI tools — are containers INSIDE that system, not separate systems. A Lambda function in your \
repo is a container. An S3 bucket you provision is a container (shape: cylinder). Only model something as \
a separate system if it's a genuinely independent product with its own team, repo, and lifecycle. External \
systems (external: true) are third-party services you don't control (e.g. Stripe, AWS Rekognition, \
Twilio). \"Separate deployment unit\" does NOT mean \"separate system.\"\n\
17. The C4 hierarchy is an authority hierarchy. System-level decisions (which systems exist, their \
boundaries and responsibilities) constrain what containers can exist inside them. Container decisions \
constrain components. Component decisions constrain operations. If implementing at a lower level reveals \
that a higher-level boundary is wrong, that is an architectural decision requiring human review — not \
a refactoring detail an agent should resolve silently.\n\
\n\
## Workflow\n\
1. `list_models` to see existing diagrams.\n\
2. **Call `get_structure` with the project path** to get an annotated directory tree. This shows manifests \
(`[manifest]`), infrastructure configs (`[infrastructure]`), and environment templates (`[environment]`) at their \
location in the tree. Read the manifests it surfaces to identify runtime dependencies (external services, \
databases, frameworks). Each directory with its own manifest + infrastructure config is likely a separate \
deployable unit → a container in C4. Do NOT manually explore the codebase — `get_structure` provides the \
complete picture.\n\
3. **Model one level at a time.** Each call creates one view that gets validated for edge completeness.\n\
   - **First call (`set_model`):** persons, the system, external systems, and system-level edges only. \
No containers yet. This establishes the system landscape. Fix any warnings before proceeding.\n\
   - **Second call (`set_node` on the system):** add all containers plus container-level edges \
(Person→Container, Container→Container, Container→ExternalSystem). Fix any warnings. \
**Then group containers that deploy together** using `set_groups` — e.g. if Website, CMS Admin, and API \
are all part of one Next.js app, group them. If two S3 buckets are provisioned together, group them. \
Containers split for inner-graph clarity (rule 9) should almost always be grouped.\n\
   - **Later (`set_node` per container):** add components only when the user asks for deeper detail, \
plus component-level edges. Fix warnings.\n\
   Do NOT dump all levels into a single `set_model` call — the tool validates edges per view level, and \
creating everything at once makes it easy to miss gaps that leave nodes disconnected.\n\
   Do NOT add components unless the user explicitly asks for deeper detail.\n\
   **Model for production, not for demos.** Look for cross-cutting concerns: authentication, input validation, \
data migrations, background jobs, observability. Model them explicitly — do not leave them implied.\n\
   **Set status on every node.** When modeling an existing codebase, set `status: \"implemented\"` on all nodes \
that already exist in the code. Set `status: \"proposed\"` on new nodes being added as part of a feature or change. \
Nodes without status appear grey and unactionable in the UI — always be explicit.\n\
   **When you do add components** to a container (because the user asked for component-level detail or you're \
adding a feature), model ALL components in that container — not just the new ones. Use `set_node` to populate \
the full component set. Partial component views are misleading.\n\
4. **Edges must exist at every abstraction level.** The UI shows one level at a time. Always include:\n\
   - System-level edges: Person→System, System→System\n\
   - Container-level edges: Person→Container, Container→Container, Container→ExternalSystem\n\
   - Component-level edges (when components exist): Component→Component, Component→ExternalSystem\n\
   When adding components to containers, ALWAYS also add component-level edges that reflect the container-level \
relationships. If container A→B and A→ExternalDB exist, then when you detail A with components via `set_node`, \
include edges in the subtree data from the relevant components to B, ExternalDB, etc. The `set_node` tool \
accepts edges to any node in the model, not just nodes within the subtree. If you forget, the tool will \
warn you — fix missing edges immediately with `add_edges`.\n\
5. **Do NOT create flows during initial modeling.** Flows are added later by the user or on explicit request. \
Focus on the structural model (persons, systems, containers, components, operations).\n\
6. **When adding components, populate them with all three code-level node kinds:**\n\
   - **model** nodes for data structures. **Always include the `properties` array** — each property has a `label` \
(valid identifier) and `description`. Do NOT just describe fields in the description text. Example: a `todo` model \
with `properties: [{label: \"id\", description: \"unique identifier\"}, {label: \"title\", description: \"todo text\"}, \
{label: \"completed\", description: \"whether the todo is done\"}]`. Models are the nouns of the system.\n\
   - **operation** nodes for individual functions, methods, or handlers — anything that maps to one function in code \
(e.g. `handleCreate`, `validateInput`, `insertRecord`, `hashPassword`). Most code-level nodes are operations. If you \
can point to a single function/method, it's an operation.\n\
   - **process** nodes for multi-step behavioral flows that orchestrate multiple operations — sagas, pipelines, or \
workflows (e.g. `orderFulfillment` — validate payment, reserve inventory, send confirmation). If it maps to a single \
function, it's an operation, not a process.\n\
   Use **@[Name]** mentions in descriptions to cross-reference sibling nodes: \"Validates the @[todo] model before \
persisting\", \"Calls @[insertRecord] then returns the created @[todo]\". The square bracket syntax is required — \
`@[todo]` renders as a clickable pill, `@todo` does not. This creates a navigable web of relationships at code level. \
Use `update_source_map` to link operations to source files.\n\
7. **Default workflow: model first, then wait.** After modeling proposed changes, stop and let the user review \
the diagram before implementing. Don't automatically call `get_task` and start writing code — the point of \
scryer is visual verification before implementation. If the user asks you to implement, build, or code in the \
same request, go ahead.\n\
8. **Implementation loop.** Use `get_task` to get the next implementation task. Build it, mark nodes as \
implemented via `update_nodes`, then call `get_task` again. **Repeat this loop until `get_task` returns \
\"All tasks complete.\"** Do not read the full model and plan your own work order — `get_task` handles dependency \
ordering, contract inheritance, and progress tracking. Parent containers and systems are marked implemented via \
completion hints from `get_task` once all their children are done.\n\
\n\
## Authority Hierarchy\n\
The model is a specification, not just documentation. Higher-level nodes have authority over lower-level ones.\n\
\n\
**Changes flow down.** System boundaries constrain containers. Container definitions constrain components. \
When implementing code, the model above is the spec — work within it.\n\
\n\
**Questions flow up.** If implementation reveals a higher-level boundary is wrong, do NOT silently modify \
the model. Flag the conflict and wait for human approval.\n\
\n\
Requires human approval: adding/removing/renaming systems, restructuring containers, moving components \
between containers, any change that alters boundaries at a higher level than where you're working.\n\
\n\
Does not require approval: adding/modifying components and operations within existing boundaries, adding \
edges between existing nodes, updating descriptions/technology/status/source map, detailing a node's \
internals when the user explicitly asked you to.";
