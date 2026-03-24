pub(crate) const INSTRUCTIONS: &str = r#"scryer is a C4 architecture diagramming tool. You are editing C4 model diagrams stored as .scry files (JSON format).

## C4 Hierarchy
- **Person**: A user or actor. Top-level node (no parent).
- **System**: A software system. Top-level node (no parent). Can be marked `external: true`.
- **Container**: An application, data store, or service inside a system. Parent must be a system node.
- **Component**: A logical component inside a container. Parent must be a container node.
- **Operation**: A single function, method, or handler inside a component — code you can point to in one file. Use operation for anything that maps to one function/method. Parent must be a component node. **Name must be a valid identifier** (camelCase or snake_case — match the target language's convention).
- **Process**: A multi-step behavioral flow that orchestrates multiple operations — like a saga, pipeline, or workflow. Processes describe *sequences*, not individual functions. If it maps to a single function, it's an operation, not a process. Parent must be a component node. Use `type: "process"` in node data.
- **Model**: A data model inside a component. Parent must be a component node. Has optional `properties` (array of `{label, description}`). Use `type: "model"` in node data. **Name must be a valid type name** (PascalCase or camelCase). **Property labels must be valid identifiers.**

## Node Types
All nodes use type `"c4"`, except: operation uses `"operation"`, process uses `"process"`, model uses `"model"`.

## Naming Rules
Operation and process names must be valid identifiers: start with a lowercase letter, then `[a-zA-Z0-9_]`. **Match the target language's naming convention** — use snake_case for Python/Rust/Ruby/Go, camelCase for JavaScript/TypeScript/Java/C#. Model names may start with an uppercase letter (PascalCase like `UserProfile`) or lowercase. Model property labels must be valid identifiers matching the target language convention.

## Description vs Notes
- **description**: What this node *is* — its role and purpose at the appropriate abstraction level. Visible on the diagram. Keep it concise and architectural. Do NOT include deployment details, environment config, hosting providers, or implementation specifics.
- **notes**: Implementation context, conventions, deployment details, rationale — anything useful during development but not part of the architectural identity. Notes are inherited by descendants via `get_task` and shown as context during implementation. Put things like "hosted on Fly.io", "uses replica set for change streams", "prod and dev environments" here.

## Source Map
The model has an optional `sourceMap` field: a mapping from node or flow ID to an array of source locations (`{pattern, line?, endLine?, command?}`). You can set source maps inline via the `source` field on `update_nodes`, or use `update_source_map` for bulk updates. Always set source locations when marking nodes as implemented — containers/components get glob patterns, operations get specific file patterns + line ranges. This is separate from `sources` (glob patterns on higher-level nodes). Flow IDs are also valid keys — use them to link a flow to its test file with a `command` to run the test.

## Status
Set status on nodes that represent work. Omit status for framework defaults that require no implementation effort. Nodes without status are context — visible but not actionable by `get_task`. Edges do not have status — edge color is inferred from endpoint nodes in the UI.

- **"proposed"** (blue): Planned — doesn't exist yet.
- **"implemented"** (amber): Code exists but may be incomplete (stubs, partial implementation, scaffolding).
- **"verified"** (green): Production-ready. **Gated**: can only be set when ALL inherited `expect` contract items have `passed: true`.
- **"vagrant"** (violet): Discovered during codebase sync — exists in code but was not part of the architecture plan. Needs review: keep it or remove it.

A `reason` is required on every status change via `update_nodes`. State what's still missing or what was just completed. For implemented: "Needs auth middleware and rate limiting". For verified: "All contract items pass".

**Container/system status propagates upward**: when all component children of a container are implemented/verified, `get_task` will prompt you to mark the container as implemented. Same for systems when all containers are done.

## IDs
Node IDs: "node-N" (auto-generated). Edge IDs: "edge-{source}-{target}". Use `get_model` to discover existing IDs.

## Modeling workflow
Call `get_rules` before creating or editing a model — it contains the full modeling workflow and C4 rules.

## Implementation workflow
When building code from a model, use `get_task` in a loop. Each call returns one work unit with dependency ordering, contract inheritance, and progress tracking built in.
1. Call `get_task` to get one work unit.
2. Build what the task describes. A scaffold task may cover multiple nodes at once — that's fine.
3. Mark the node(s) as `implemented` via `update_nodes` with a `reason` explaining what was built. Only mark nodes listed in the task.
4. **Call `get_task` again immediately.** Do not stop after one task — there are always more until it returns "All tasks complete."
The task system tracks what's done and what's next. Do not read the full model via `get_model` to derive your own implementation order.

### Verification (implemented → verified)
"Verified" is separate from implementation — do not set it during the implementation loop. A node is verified when:
- The implementation is complete — no stubs, TODOs, or placeholder logic.
- The code does what the node's description says.
- If tests exist for this code, they pass.
- All inherited `expect` contract items are satisfied (mark each as `passed: true`).
The user decides when to verify. When asked, check each point. If anything fails, leave the node as `implemented` and explain what's missing.

## Subagents
Do NOT delegate scryer write operations (`set_model`, `set_node`, `add_nodes`, `update_nodes`, `delete_nodes`, `add_edges`, `update_edges`, `delete_edges`, `set_flows`, `delete_flow`, `update_source_map`) to subagents. Subagents may use read tools (`list_models`, `get_model`, `get_node`, `get_rules`, `get_changes`, `get_task`) for research, but all model mutations must happen in the main conversation context."#;

pub(crate) const TASK_INSTRUCTIONS: &str = "\
The spec above is your source of truth — it tells you WHAT to build. \
Trust your training knowledge for well-known frameworks and tools. \
Do not research standard framework setup — you already know how.

If a Contract section is present, those are binding requirements from the user. \
MUST items are non-negotiable — each has a passed/failed flag that gates the `verified` status. \
ASK USER FIRST items require confirmation before deciding. \
NEVER items are hard constraints. If a contract item includes a URL, read it for context.

## Status meanings
- **proposed**: Planned, no code yet. \
- **implemented**: Code exists but may be incomplete — stubs, partial impl, scaffolding. \
- **verified**: Production-ready. Can ONLY be set when all `expect` contract items (including inherited ones) have `passed: true`. \
A `reason` is required on every status change — state what's still missing or what was just completed. For implemented: \"Needs auth middleware and rate limiting\". For verified: \"All contract items pass\".

If something is unclear or the spec doesn't cover a decision you need to make, \
ask the user — don't spiral into web searches.

## After building
1. Mark ONLY the node(s) listed above as `implemented` using update_nodes. Include a `reason` explaining what was built. \
Include `source` on every node — a glob pattern (and line/endLine for operations). \
Containers and components: `[{\"pattern\": \"src/auth/**/*.ts\"}]`. \
Operations: `[{\"pattern\": \"src/auth/handler.ts\", \"line\": 15, \"endLine\": 42}]`.
2. Call get_task immediately to get the next task. Do NOT stop — there are more tasks.
3. Repeat until get_task returns \"All tasks complete.\"

## The model is the spec
The architecture model is your source of truth. Build exactly what it describes — no more, no less. \
If a template or generator adds code that isn't in the model (extra collections, pages, blocks, routes, etc.), \
remove it. The model defines what should exist. Anything not in the model is drift and must be cleaned up.

## When modifying existing code
If you rename, move, delete, or restructure code that is source-mapped in the model, \
update the model in the same response using update_nodes. \
Delete removed nodes with delete_nodes. The model must stay in sync with the code.";
