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
4. Technology labels must be accurate. Don't label a database container with \"React\" or a \
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
9. Split multi-role deployments into separate containers. When a single deployable unit serves \
multiple distinct roles, model each role as its own container and use a deployment group to show \
they ship together. Signals to look for: multiple distinct UIs (public site vs admin panel), \
API routes that serve external callers (webhooks, callbacks) separately from the frontend, \
different auth models for different parts of the app, framework namespacing (Next.js route groups, \
Rails engines, Django apps). Example: a Next.js app with Payload CMS should be at least two containers \
(\"Website\" + \"CMS Admin\"), possibly three if the API routes handle external webhooks independently \
(\"Website\" + \"API\" + \"CMS Admin\"), all in one deployment group. The container diagram captures \
logical separation, not deployment topology.\n\
10. Containers are independently addressable, not framework internals. A container must be something \
a user, external system, or other container can address directly (a URL, a queue, a database connection). \
Auto-generated framework layers (e.g. Payload CMS REST API, Django admin ORM, Rails ActiveRecord) are \
implementation details of the container that uses them — not separate containers. If you can only reach \
it through another container's process, it's a component of that container, not its own container.\n\
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
2. **Before calling `set_model`, explore the codebase to discover all deployable units.** Don't model from \
memory or assumption — search the repo systematically:\n\
   - Read project README, CLAUDE.md, and root package.json/Cargo.toml for an overview.\n\
   - Search for infrastructure-as-code: SAM/CDK/Terraform templates, docker-compose, k8s manifests, \
Dockerfiles, fly.toml, serverless.yml.\n\
   - Look for separate deployment directories: `lambda/`, `functions/`, `workers/`, `services/`, `cmd/`, \
`jobs/`. Each is likely a separate container in C4.\n\
   - Find all entry points: `handler.ts`, `main.rs`, `index.ts`, `main.go`, etc. across the entire repo.\n\
   - Check for background processors, scheduled jobs, CLI tools, or webhook handlers that deploy independently.\n\
   - Within each deployable unit, identify distinct roles. A single app that serves a public website, an admin \
panel, and webhook API routes is three logical containers in one deployment group — not one container.\n\
   Common miss: serverless functions, background workers, and sidecar services in subdirectories are separate \
containers — don't model the project as a single application if it deploys as multiple units.\n\
3. Start with two levels only: persons and systems (top-level), then containers inside systems. Do NOT add \
components unless the user explicitly asks for deeper detail.\n\
   **Model for production, not for demos.** Look for cross-cutting concerns: authentication, input validation, \
data migrations, background jobs, observability. Model them explicitly — do not leave them implied.\n\
4. **Edges must exist at every abstraction level.** The UI shows one level at a time. Always include:\n\
   - System-level edges: Person→System, System→System\n\
   - Container-level edges: Person→Container, Container→Container, Container→ExternalSystem\n\
   - Component-level edges (when components exist): Component→Component, Component→ExternalSystem\n\
   When adding components to containers, ALWAYS also add component-level edges that reflect the container-level \
relationships.\n\
5. **After creating the model structure, define core flows** using `set_flows`. Every model should have at \
least 2-3 flows covering the primary user journeys.\n\
6. **When adding components, ALWAYS also add operation nodes** inside each component. Name each operation after \
a concrete responsibility (e.g. \"handleLogin\", \"validateToken\"). Use `update_source_map` to link operations \
to source files.\n\
7. Use `get_task` to get the next implementation task. Build it, mark nodes as implemented via \
`update_nodes`, then call `get_task` again. **Repeat this loop until `get_task` returns \"All tasks complete.\"** \
Do not read the full model and plan your own work order — `get_task` handles dependency ordering, contract \
inheritance, and progress tracking. Parent containers and systems are marked implemented via completion hints \
from `get_task` once all their children are done.\n\
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
