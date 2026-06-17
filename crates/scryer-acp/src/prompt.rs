use scryer_core::{Node, ScryModel};
use std::collections::HashSet;

/// Serialize a ScryModel as compact JSON for embedding in an agent prompt.
/// Strips empty arrays / null fields so the agent context isn't bloated.
pub fn serialize_model_for_prompt(model: &ScryModel) -> String {
    let mut val = serde_json::to_value(model).unwrap_or(serde_json::Value::Null);
    strip_compact(&mut val);
    serde_json::to_string_pretty(&val).unwrap_or_else(|_| "{}".to_string())
}

/// Serialize just one node's subtree (the node + all descendants, their
/// responsibilities/properties, and the source-map entries for them) as compact
/// JSON. Used to feed a drift check ONLY the claims for the scope it's checking,
/// instead of the whole model — the cost scales with the scope, not the model.
pub fn serialize_subtree_for_prompt(model: &ScryModel, node_id: &str) -> String {
    let mut ids: HashSet<&str> = HashSet::new();
    ids.insert(node_id);
    let mut frontier = vec![node_id.to_string()];
    while let Some(id) = frontier.pop() {
        for n in &model.nodes {
            if n.parent_id.as_deref() == Some(id.as_str()) && ids.insert(n.id.as_str()) {
                frontier.push(n.id.clone());
            }
        }
    }

    let nodes: Vec<&Node> = model.nodes.iter().filter(|n| ids.contains(n.id.as_str())).collect();
    let resp_ids: HashSet<&str> = nodes
        .iter()
        .flat_map(|n| n.responsibilities.iter().map(|r| r.id.as_str()))
        .collect();
    // source_map is keyed by responsibility id OR a property-bearing node id —
    // keep entries for either when they belong to the subtree.
    let source_map: serde_json::Map<String, serde_json::Value> = model
        .source_map
        .iter()
        .filter(|(k, _)| resp_ids.contains(k.as_str()) || ids.contains(k.as_str()))
        .map(|(k, v)| (k.clone(), serde_json::to_value(v).unwrap_or(serde_json::Value::Null)))
        .collect();

    let mut val = serde_json::json!({ "nodes": nodes, "sourceMap": source_map });
    strip_compact(&mut val);
    serde_json::to_string_pretty(&val).unwrap_or_else(|_| "{}".to_string())
}

fn strip_compact(val: &mut serde_json::Value) {
    match val {
        serde_json::Value::Object(map) => {
            map.retain(|_, v| {
                !matches!(v, serde_json::Value::String(s) if s.is_empty())
                    && !v.is_null()
                    && !matches!(v, serde_json::Value::Array(a) if a.is_empty())
                    && !matches!(v, serde_json::Value::Object(m) if m.is_empty())
            });
            for v in map.values_mut() {
                strip_compact(v);
            }
        }
        serde_json::Value::Array(arr) => {
            for v in arr.iter_mut() {
                strip_compact(v);
            }
        }
        _ => {}
    }
}

/// Prompt for initial model creation from a codebase. Builds the system + container levels.
pub fn initial_model_prompt(project_path: &str) -> String {
    format!(
        r#"You have access to the scryer MCP server (schema v0.3). Build the architecture model for the project at {project_path}.

## Core principles

- **The codebase is evidence, not the source of truth.** You read code to elicit responsibilities the system already holds — not to transcribe the file tree into nodes.
- **Responsibilities are pure business statements.** A responsibility says what a node is accountable for in business terms, never how. "restricts access to private content" — yes. "restricts access via JWT" — no, the "via JWT" is mechanism; keep it out of the statement. (`directives` beside a responsibility hold prescriptive "must"/"never" constraints, but those are user-authored — never write them.)
- **Write terse, scannable statements — not prose.** One verb-led clause per responsibility: lead with the distinguishing verb + object, then stop. No trailing "by/through/where/so that …" tails, and don't repeat the obvious domain ("the architecture model") on every line. "Renders the node/link/group canvas" — yes. "Renders the visual architecture editor where users arrange nodes, links, and groups on a canvas" — no. A node's `description` is its identity in a few words (what it IS), never a re-listing of its responsibilities — if it's a comma-list of them, omit it.
- **Mention other nodes as wikilinks, by id.** When a description or responsibility statement mentions another node, write the mention as `[[node-id]]` — the UI resolves it to the node's current name — or `[[node-id|shown text]]` to fit the sentence. A wikilink never replaces the structural link the mention implies; declare both.
- **Every node justifies its existence through responsibilities, stated at its own altitude.** A child node exists to discharge a subset of its parent's responsibilities — so a parent's responsibilities are fewer and broader than the union of its children's, never a per-child enumeration of what each child does. A node with no responsibility — or whose responsibilities serve no ancestor commitment — is structurally vagrant.

## Procedure

1. Call `get_rules` to load the modeling rules.
2. Call `read_codebase` with path "{project_path}" to get the annotated directory tree. Read the manifests it surfaces (package.json, Cargo.toml, fly.toml, Dockerfile, .env.example, etc.) to identify deployable units, data stores, external services, and frameworks.3. **Build the system level.** Call `set_model` with the persons (real users / actors), the system itself, and external systems (third-party services the system depends on — Stripe, S3, Resend, etc.; mark these `external: true`). Add system-level links: persons and external systems connect to the SYSTEM itself, not to its internal containers — those are container-level relationships added when you drill in. Every person/external must link to the system, or it appears disconnected on the system-context diagram. For each node, write 1–4 responsibilities. Set responsibility status to `implemented` on responsibilities derived from existing code, `proposed` on anything speculative.
4. **Add containers.** Call `set_node` on the system id with a payload containing the containers (web apps, APIs, workers, databases, message queues, file stores). For each container:
   - Set `kind: "container"`, `name` describes the role ("Website", "Worker", "CMS"), `technology` describes what it IS as software ("Next.js 14", "PostgreSQL 16", "S3 Bucket").
   - Write 2–6 responsibilities — pure business statements about what the container is accountable for. No technology words in the statement.
   - Include container-level links (Person→Container, Container→Container, Container→External).
5. **Group containers.** Call `set_groups` to create deployment-unit groups for containers that ship together (e.g. multiple containers running inside one Next.js app, multiple AWS resources provisioned by one Terraform module). A group can carry its own deployment-shaped responsibilities ("deploys atomically", "must fit in 256 MB").
6. **Stop here.** Do not add components or code-level nodes. The user requests component detail explicitly.
7. Call `update_source_map` with `boundaries` to attach a directory glob to each container that has code (a boundary entry per node, e.g. pattern "apps/web/**/*").
8. Call `validate_model` and fix every warning — especially "appears disconnected", which means a node has no relationship at its own C4 level (e.g. a person/external linked to a container but not to the system). Re-link and re-validate until clean.
9. Call `read_model` (no node) to confirm the architecture overview you built, then summarize it for the user.

## Don'ts

- Don't add responsibilities the codebase doesn't already evidence. If the codebase doesn't handle a concern, the model shouldn't claim it does.
- Don't put technology vocabulary inside responsibility statements. The `technology` field is the place for that. (`directives` are user-authored constraints — never set them.)
- Don't model framework internals (e.g. ORM layers, admin panels that come with a CMS) as separate containers unless they have a distinct user-facing surface that warrants its own tour.
- Don't draw a separate edge for each interaction between two nodes — one link per relationship.
"#
    )
}

/// Prompt for the FAST semantic pass: the structure already exists (built
/// deterministically from the code); the agent only adds the semantic layer
/// (responsibilities, descriptions, statuses, and — at the top — persons /
/// externals / link meaning) to the existing subtree under `node_id`. It does
/// NOT rebuild structure, hunt for files, or loop on validation — that's what
/// makes it fast. `node_kind` is one of "system", "container", "component".
pub fn enrich_subtree_prompt(
    project_path: &str,
    node_id: &str,
    node_name: &str,
    node_kind: &str,
    model_json: &str,
) -> String {
    // What the agent should focus on, per level.
    let focus = match node_kind {
        "system" => "the system and its containers (the high-level shape). If real persons (actors) or external third-party systems are evident from manifests/config, ADD them and link them — these are the only nodes you may add, because structure extraction can't infer them.",
        "container" => "this container and its components — what each module is accountable for.",
        "component" => "this component and its symbols — what each definition is accountable for. A pure data type (a node that already carries `properties` and no behavior) needs only a one-line description, not responsibilities.",
        _ => "this node and its children.",
    };

    format!(
        r#"You have access to the scryer MCP server (schema v0.3). The architecture model for the project at {project_path} is ALREADY built structurally — every container, component, and symbol below was extracted directly from the code. Your ONLY job is to add the SEMANTIC layer to the subtree under "{node_name}" (id {node_id}): what each node is accountable for. Do NOT add, remove, rename, or re-parent the existing nodes, and do NOT rebuild structure.

## Core principles

- **Responsibilities are pure business statements.** What a node is accountable for in business terms, never how. "restricts access to private content" — yes. "restricts access via JWT" — no (mechanism). No technology names, no protocols. (`directives` are user-authored — never set them.)
- **Terse, scannable, verb-led.** One clause per responsibility: distinguishing verb + object, then stop. No "by/through/so that …" tails. A `description` is the node's identity in a few words (what it IS), never a re-list of its responsibilities.
- **Mention other nodes as wikilinks, by id.** When a statement or description mentions another node, write it as `[[node-id]]` — the UI resolves it to the node's current name — or `[[node-id|shown text]]` to fit the sentence.
- **Ladder up — stay at each node's altitude.** Each child's responsibilities discharge a subset of its parent's, but a node states what IT is accountable for, not what a child does: a parent's responsibilities are fewer and broader than the union of its children's, never a per-child enumeration. If a line reads as describing a single child, it's one altitude too low. Since the structure came from real code, set responsibility status to `implemented` (use `proposed` only for something genuinely speculative you add).

## Current model — structure is authoritative

The full model is below so you don't call `read_model`. Each component's `boundaries` entry is its source FILE; that's where its code (and its symbols') lives.

```json
{model_json}
```

## Procedure (optimized for speed — minimize round-trips)

1. Focus on {focus}
2. **Read the located source directly.** For the components in scope, read their boundary files (you already have the exact paths — read them in parallel). Do NOT call `read_codebase` and do NOT search for files; the model already tells you where everything is.
3. **Batch the writes.** Call `update_nodes` with an ARRAY patching many nodes at once — `responsibilities` (+ `status`) and `description` for each existing node id in scope. One or a few calls, not one per node. Do not touch `kind`, `name`, `parentId`, or `properties` of existing nodes.
4. Add relationship meaning: set `label` on existing links via `update_links` where the relationship is clear from the code. At the system level only, add any missing persons/externals and their links.
5. Do NOT loop on `validate_model` — the structure is already valid and you aren't changing it. Run it at most once at the end if you added persons/externals.

Stay within the "{node_name}" subtree. The goal is a complete, accurate semantic pass over existing nodes in as few tool calls as possible."#
    )
}

/// The system-level SEMANTIC session of the auto-context build. The structural
/// skeleton (system node + one container per deployable unit, boundary globs,
/// dependency links) was already minted mechanically from manifest facts, and
/// the per-container modeling sessions are running IN PARALLEL with this one.
/// This session owns everything those jobs don't: persons, external systems,
/// system/container responsibilities, refined names/technology, link labels,
/// deploy groups. `structure_json` is the minted skeleton (ids included).
pub fn enrich_system_prompt(project_path: &str, system_id: &str, structure_json: &str) -> String {
    format!(
        r#"You have the scryer MCP server (schema v0.3). The architecture model for the project at {project_path} was just seeded MECHANICALLY from its manifests: a system node and one container per deployable unit (with boundary globs and raw dependency links) already exist — their ids are below. Separate agent sessions are filling each container's components IN PARALLEL with you right now. Your job is everything at the SYSTEM and CONTAINER level that a manifest can't say:

1. **Refine the minted containers** via `update_nodes` (batch ONE call): `name` = the unit's role ("Desktop App", "MCP Server", "Docs Site" — the minted names are raw manifest names), `technology` = what it IS as software ("Tauri 2 + React", "Rust MCP server"), and 2–6 terse, verb-led business responsibilities per container (status `implemented`). Write the system node's own responsibilities (1–4, broader than any container's) and a short description of what the system IS.
2. **Add persons and externals.** `add_person` for real users/actors the code evidences; `add_system` with external=true for third-party systems it depends on (only if evident from manifests/config — e.g. Stripe, S3, a managed database). Link them to the SYSTEM (id below) with `add_links`, never to containers.
3. **Add non-code containers** the manifests evidence but the scan can't mint — a managed database, a queue, a bucket — with `add_container` (parentId = the system id). Do NOT add, remove, rename-to-something-unrelated, or re-parent the minted code-bearing containers; refining their name/technology/responsibilities is yours, their existence is not.
4. **Label the links.** The minted container→container links carry no labels: `update_links` each with a clear label ("invokes", "reads models from"). Add missing container→container, container→external links with `add_links`.
5. **Group deploy units** with `add_group` ONLY when several containers ship/package together. Independent services get no groups — most small projects need none.

## Rules
- Responsibilities are pure business statements at the node's own altitude — no technology words, no mechanism, no per-component enumeration. Technology belongs in the `technology` field.
- Mention other nodes in descriptions/statements as wikilinks by id (`[[node-id]]`), and still declare the structural link.
- Do NOT touch anything below container level: no components, no symbols — the parallel sessions own container internals, and structure they commit while you work is not yours to edit.
- Read manifests and a few entry-point files only — enough to state what each unit is accountable for.

## Procedure (minimize round-trips)
1. Read the key manifests; decide roles, actors, externals.
2. One batched `update_nodes` for system + minted containers; `add_person`/`add_system`/`add_container` for the rest; `add_links`; `update_links` for labels.
3. Call `validate_model` ONCE at the end and fix only system/container-level warnings (especially "appears disconnected" persons/externals). Warnings about nodes you did not create (components/symbols from the parallel sessions) are NOT yours — leave them.

## The minted structure (ids are authoritative)
- System id: `{system_id}`
```json
{structure_json}
```

Write only what the code evidences. Keep responsibilities terse and business-level.
"#
    )
}

/// Wave 2 of the auto-context build: model the components + symbols of ONE
/// container from its sliced code context. The agent clusters components from
/// the dependency graph (NOT one-per-file) and returns the complete component +
/// mandatory-symbol subtree through one atomic MCP call. `scope_json` is the
/// serialized per-scope context (files, symbols with line ranges, internal
/// symbol/file edges, cross-boundary edges).
pub fn build_container_prompt(
    project_path: &str,
    container_name: &str,
    container_id: &str,
    scope_json: &str,
) -> String {
    format!(
        r#"You have the scryer MCP server (schema v0.3). Model the COMPONENTS and SYMBOLS for one existing code-bearing container. Its code has already been indexed — every file, every symbol with its line range AND its source excerpt, and the dependency edges are supplied at the end, so you do NOT need to discover structure or read files. Go straight to the meaning.

## How to build

Call `commit_container_model` ONCE with:
- `containerId`: the id in the task section
- `components`: the complete component decomposition. Each component and nested symbol has a unique request-local `key`, plus its C4 name/description/responsibilities.
- `groups`: optional secondary groupings whose `memberKeys` use component local keys.
- `links`: OPTIONAL and almost always empty. You do NOT author code-level links — the server wires component→component and symbol→symbol links automatically from the dependency graph below. Only use `links` for a cross-boundary relationship the graph can't infer (a component reaching an external or another container by its existing node id). Anything illegal is dropped and reported, so never let a link block you.

The server validates the proposal, resolves local keys, mints every id, anchors source locations, derives the code-level links, and performs one model write. Do NOT call `add_component`, `add_symbol`, `add_links`, or `add_group` for this container.

### Components

CLUSTER components from cohesion + the dependency graph below: a component groups the files/symbols that work together toward one responsibility. Do NOT make one component per file. Give each a few terse business responsibilities AT THE COMPONENT'S ALTITUDE: each names one accountability the component holds, NOT what an individual symbol inside it does — the per-handler detail belongs on the symbols below. A component's responsibilities are FEWER and BROADER than the union of its symbols'; if a line reads as describing a single symbol, it is one altitude too low.

### Mandatory symbols

Every component must contain at least one architecturally meaningful symbol. For each nested symbol, pass a scope-unique `key`, `name`, `sourceFile`, and `line`/`endLine` for the full definition straight from the context. A responsibility is a plain string (use the plain form by default; only attach `{{"statement", "line", "endLine"}}` when a precise sub-range genuinely adds value). Give `responsibilities` for behavior and `properties` for declared data fields; set `visual: true` on UI-rendering symbols. **Every symbol you emit MUST carry semantic content of its own: at least one business/process responsibility, or — for a data type — its declared `properties`. There is no empty symbol.** A definition with no business responsibility and no declared data shape — a trivial UI leaf (a `Chevron`, a `Logo`), a thin wrapper, a re-export, a private helper, a bare entry point — does NOT earn a node: OMIT it and let its parent symbol's responsibility cover it. When a definition has nothing of its own to say, the answer is to leave it out — NEVER to fabricate filler, and NEVER to emit it with empty responsibilities and properties. A definition earns a symbol when it carries architectural behavior, a declared data shape, or a cross-boundary role. Skip trivial wrappers, thin re-exports, getters/setters, and test stubs:
  - **Framework registration objects** (a CMS collection config, an ORM model, a settings/route object): descend INTO its `fields[]` array (the schema columns) and emit one property per entry there. Its `properties` are those DECLARED FIELDS — the record's columns — NEVER the sibling config wrapper keys (`slug`, `admin`, `hooks`, `access`, …), which are framework plumbing, not the record's data shape.
  - **Generated / mirror type files** (a `*-types` file, a `*.d.ts`, or any file that just re-declares a definition that already lives in real source): do NOT create a parallel symbol for it. Fold its fields into the source-of-truth symbol so each thing is modeled exactly once — never two sibling symbols for the same record.
  - **Classes/objects with internal helpers**: model the class as ONE symbol carrying its public operations as responsibilities; do NOT add private/internal helper methods (e.g. `_rpc`, `_get_uid`, `_execute`) as their own symbols. Only addressable, public definitions become symbols.

## Procedure (minimize round-trips)
1. Decide the component clustering from the dependency graph below (group cohesive files/symbols). The evidence already embeds each symbol's source (`code`) — work from it directly. Open a source file ONLY when a truncated excerpt (`… +N lines`) leaves a symbol's accountability genuinely unclear; never re-read what is already inline.
2. Build the full proposal in memory with components and their mandatory nested symbols.
3. Do NOT build a `links` array for internal code dependencies — the server derives every component→component and symbol→symbol link from the dependency graph after it mints ids. Add a `links` entry ONLY for a cross-boundary relationship to an external or another container (by existing node id); call `read_model` once for the task's container if you need those ids. When in doubt, leave `links` empty.
4. Include optional groups only when several components form a cohesive secondary module.
5. Call `commit_container_model` once. It returns the ids it minted, the count of links it derived, and any links it dropped. If it rejects the proposal it is a STRUCTURAL problem (a missing symbol, a duplicate key) — fix exactly that and retry. Dropped links are informational, not a failure; do not retry over them. Do not run a separate validation loop.

## Task

- Project: `{project_path}`
- Container: `{container_name}`
- Container id: `{container_id}`

## Code context
The payload is compact and indexed:
- `paths[n]` is a project-relative source path.
- each file has `path` = index into `paths`.
- each symbol has a scope-global integer `id`, `name`, inclusive `lines`, optional `fields`, and `data: true` for data shapes.
- each symbol's `code` is its source: doc comment + signature + body. A trailing `… +N lines` marker means the definition continues in the file — everything else is the complete definition.
- `symbolEdges` entries are `[sourceSymbolId, destinationSymbolId]`.
- `fileEdges` entries are `[sourcePathIndex, destinationPathIndex]`, partitioned into `internal`, `outbound`, and `inbound`.

```json
{scope_json}
```

Cluster by what the code DOES, not by file layout. Keep responsibilities terse and business-level.
"#
    )
}

/// Targeted repair pass used only when the merged parallel build fails
/// deterministic validation. The fast path never pays for this extra session.
pub fn repair_model_prompt(project_path: &str, warnings_json: &str) -> String {
    format!(
        r#"You have the scryer MCP server (schema v0.3). The parallel codebase-to-model build for {project_path} is complete, but deterministic validation found the issues below.

Fix ONLY these reported issues. Preserve the established system/container/component decomposition and mandatory symbol coverage unless a warning explicitly requires a structural correction.

## Validation issues
```json
{warnings_json}
```

## Procedure
1. Call `read_model` with no node for the overview, then drill into only the affected nodes.
2. Apply the smallest necessary fixes with the normal update/link tools.
3. Call `validate_model`.
4. Continue only until validation is clean, then stop.

Do not broaden the model, add speculative responsibilities, or reorganize unrelated nodes."#
    )
}

/// Semantic drift check for one container whose code changed. The agent
/// compares what the code DOES against the model's responsibilities and reports
/// findings via `flag_drift` only — undescribed behaviour (→ vagrant
/// responsibilities) and stale claims (→ the `stale` flag). It must NOT report mere
/// code changes that still satisfy a responsibility. `subtree_json` is just this
/// node's subtree (its claims), not the whole model; `scope_json` is the
/// container's code index; `changed_files_json` is the changed-file list.
pub fn drift_check_prompt(
    project_path: &str,
    node_name: &str,
    node_id: &str,
    subtree_json: &str,
    scope_json: &str,
    changed_files_json: &str,
) -> String {
    format!(
        r#"You have the scryer MCP server (schema v0.3). The code inside the container "{node_name}" (id {node_id}) at {project_path} has changed. Decide whether the model still describes what the code DOES — this is SEMANTIC drift, not a byte/line check.

## What to flag — use ONLY the `flag_drift` tool
- **Undescribed behaviour:** the code provides a capability that NO responsibility in this subtree describes *at any altitude*. Report it under `undescribed` (a terse business statement + the `sourceFile` + the enclosing `symbol`). Each becomes a vagrant responsibility for the user to adopt or reject. The `symbol` you cite ROUTES the adoption: it is attached to the node that owns that symbol/file, NOT to this container — so name the enclosing definition precisely and state the behaviour at *that node's* altitude (a symbol-level capability, not a container-level summary).
- **Stale claims:** an existing responsibility whose code no longer discharges it — the implementation was removed, or now does something materially different. Report its `responsibilityId` under `stale` with a short factual `reason`.

## What is NOT drift (do not flag)
- Code that changed but still satisfies an existing responsibility. A refactor that preserves behaviour is not drift. The user does not care that lines moved or bytes changed — only that the model's description still matches reality.
- **Mechanism beneath an existing responsibility.** A responsibility is described at its own altitude and SUBSUMES its implementation detail. If a node already claims "Validate structural correctness", then each individual check it performs (a length cap, an id-uniqueness rule, a kind constraint) is HOW that claim is discharged — already described, do NOT flag it. Likewise a new branch inside a handler, a new field on a validator, a new helper call. Only flag a behaviour that is a genuinely NEW capability the model names nowhere — never decompose one responsibility into a list of its mechanics.

## Focus — these files changed
```json
{changed_files_json}
```
Read them, and read enough surrounding code to judge behaviour. Compare against the claims below.

## The model's current claims for "{node_name}" and everything under it
```json
{subtree_json}
```

## Code index for this container (where everything is)
```json
{scope_json}
```

Call `flag_drift` for "{node_id}" with everything you found. If the code and the model still agree, call it with empty arrays. Do not add components or symbols and do not edit existing responsibilities — only `flag_drift`.
"#
    )
}

/// Prompt for rendering a visual component preview. The agent reads the
/// component source and writes `main.tsx` for the Vite render harness in
/// `.scryer/preview/{nodeId}/` — the caller builds it afterward. The harness must
/// wrap the component with whatever providers/context the codebase requires and
/// supply reasonable fixture props inferred from the type signatures.
/// Prompt for the preview repair path (B6): the deterministic render of a
/// component came out empty or crashed with synthesized placeholder props, so
/// the agent authors realistic data. The primary output is a SHARED,
/// type-keyed fixture set (`shared.tsx` + `manifest.json`) reused across every
/// component that touches a type; a per-node override file is the fallback for
/// what shared samples can't express. No build step, no harness.
pub fn preview_fixture_prompt(
    project_path: &str,
    node_id: &str,
    node_name: &str,
    source_file: &str,
    source_lines: &str,
    render_status: &str,
    render_error: &str,
) -> String {
    let error_section = if render_error.is_empty() {
        String::new()
    } else {
        format!("\nRender error:\n\n```\n{render_error}\n```\n")
    };

    format!(
        r#"The live preview of the visual component "{node_name}" (id {node_id}) in the project at {project_path} is rendered by a dev server that synthesizes placeholder props from the component's TypeScript types. With those placeholders the render came out **{render_status}** — the preview needs realistic data instead.
{error_section}
Generic synthesis can't invent interconnected domain data: a prop that is a graph (or any object) plus another prop that points INTO it (an id, a selection) can't be made consistent from types alone. You supply that domain knowledge ONCE, keyed by type, so every component that touches the type reuses it.

## The component

Source file: `{source_file}`

```
{source_lines}
```

## How shared fixtures work

The preview server reads `.scryer/preview/fixtures/manifest.json`. For any prop whose TypeScript type is named there, it feeds a sample export from a fixture module instead of a placeholder. The manifest maps type name → export:

```json
{{
  "byType": {{
    "ScryModel": {{ "module": "shared.tsx", "export": "sampleModel" }},
    "Node": {{ "module": "shared.tsx", "export": "sampleNode", "sourceFile": "src/viewmodel.ts" }}
  }}
}}
```

`module` is a file under `.scryer/preview/fixtures/`. `export` is a named export in it. `sourceFile` is OPTIONAL — add it only to disambiguate a common type name (e.g. `Node`) by requiring the type's declaration to live in that file, so a same-named type in some library never matches.

## Your task

1. **Read the component's prop types** (follow the imports in the source above) to see which types its props are.
2. **Read the existing `.scryer/preview/fixtures/manifest.json` and `shared.tsx` if they exist** — you are EXTENDING them, not overwriting. Reuse any type already covered; never duplicate an entry.
3. For each domain type this component needs that is NOT already in the manifest, **add a named sample export** to `shared.tsx` and a `byType` entry to the manifest. Prefer extending `shared.tsx`; these samples are shared across the whole project.
4. **Make the samples mutually consistent**: ids that resolve, collection members and link/edge endpoints reference ids that exist in the sample, and any pointer-typed prop (a `selected`/`activeId`-style string) names a real element in the sample so the component renders a populated state, not an empty/not-found one.
5. **Realistic data, not placeholders.** Lists get 3–5 plausible items; names/labels/timestamps look real. The goal is a preview that shows the component doing its job.
6. **Seed controlled state to a POPULATED snapshot.** If the component's expand/select/open/active state is driven by props (an `expanded`/`openIds` set, a `selected` id, an `isOpen` flag) with companion callbacks (`onToggle`, `onSelect`), the synthesized empty set / false flag renders it collapsed-and-blank — functionally useless as a preview, even though it "rendered". The callbacks stay no-ops (the preview is a snapshot, not interactive), so the ONLY way the component shows its structure is to seed those state props open: an `expanded` set containing the sample's ids, a real `selected`, flags set so panels are visible. These props are usually generic-typed (`Set<string>`, `boolean`), so seed them in the per-node override below rather than the shared manifest.

### Per-node override (fallback + controlled state)

Write `.scryer/preview/fixtures/{node_id}.tsx` (DEFAULT EXPORT = a partial props object for "{node_name}") when this component needs props the shared samples can't express — a particular selection, a special-case shape, or **controlled-state props seeded open per point 6**. The server spreads it OVER the shared/synthesized props, so include only the keys you override, and import the shared samples to keep ids consistent (e.g. `import {{ sampleModel }} from "/.scryer/preview/fixtures/shared.tsx"` then `expanded: new Set(sampleModel.nodes.map((n) => n.id))`). Keep domain DATA in the shared fixtures; use the override for this component's view state.

### Rules

- Import types or helpers from the project by root-absolute path (e.g. `import type {{ Node }} from "/src/viewmodel"`). Never use relative imports — fixtures live outside `src/`.
- Function props may be no-ops (`() => {{}}`).
- Do NOT modify any project source files. The only files you write are under `.scryer/preview/fixtures/`.
- You do not run, build, or render anything — the preview server picks up the files automatically.
"#
    )
}

/// Prompt for generating visual variations of a component (B6). The agent
/// writes N self-contained variant modules; the always-running preview server
/// serves each as a virtual entry instantly — there is no build step. Each
/// variant imports the original component (or reimplements parts inline) and
/// applies changes through wrappers, CSS overrides, or modified props — the
/// project source is never modified.
pub fn visual_variation_prompt(
    project_path: &str,
    node_id: &str,
    node_name: &str,
    source_file: &str,
    source_lines: &str,
    user_prompt: &str,
    base_variant: &str,
    variation_count: usize,
) -> String {
    let base_section = if base_variant.is_empty() {
        String::new()
    } else {
        format!(
            "\n## Starting point (previously chosen variant — iterate on this)\n\n```tsx\n{base_variant}\n```\n"
        )
    };

    let last_idx = variation_count - 1;

    format!(
        r#"You are generating {variation_count} visual variations of the component "{node_name}" (id {node_id}) in the project at {project_path}.

## The component

Source file: `{source_file}`

```
{source_lines}
```
{base_section}
## The user's request

{user_prompt}

## Your task

Create {variation_count} DISTINCT visual interpretations of the user's request. Each variation must be a genuinely different approach, not minor tweaks of the same idea.

For EACH variation index 0 through {last_idx}, write ONE file `.scryer/preview/variations/{node_id}/{{n}}.tsx` that:

- `export default` a self-contained React component taking NO props — it renders the varied version of "{node_name}" with realistic inline fixture data (lists get 3–5 plausible items, labels look real).
- Imports everything from the project by ROOT-ABSOLUTE path (e.g. `import {{ {node_name} }} from "/{source_file}"`, `import {{ helper }} from "/src/lib/helper"`). NEVER use relative imports — the variant module lives outside `src/`.
- Applies the requested change through one or more of:
  - **CSS overrides** — a `<style>` element or inline styles on a wrapper
  - **Wrapper components** — wrap the original to modify layout, spacing, or behavior
  - **Modified props** — change props that affect appearance
  - **Inline reimplementation** — for structural changes, reimplement the relevant parts inline while importing shared dependencies from the project
- Does NOT import the project's global CSS — the preview server injects it automatically.
- Does NOT call `createRoot` or render itself — just export the component.

An always-running dev server picks each file up instantly; there is no build step and nothing else to write.

### Variation guidelines

- Each variation should be a genuinely different visual approach
- Example — "make the header sticky": one uses CSS `position: sticky` with shadow, another `position: fixed` with backdrop-blur, a third adds a condensed mode on scroll
- Example — "reduce padding": one goes minimal, another balanced with more whitespace on specific sides, a third uses asymmetric padding with tighter vertical
- Make each variation complete and functional, not half-finished sketches
- Preserve the component's functionality — only modify visual aspects

### Rules

- Do NOT modify any project source files
- The ONLY files you write are the {variation_count} variant modules `.scryer/preview/variations/{node_id}/{{0..{last_idx}}}.tsx`
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use scryer_core::{Kind, Responsibility, Source, SourceLocation};

    fn node(id: &str, kind: Kind, parent: Option<&str>, resp: Option<&str>) -> Node {
        Node {
            id: id.into(),
            kind,
            name: id.into(),
            parent_id: parent.map(|s| s.into()),
            external: None,
            technology: None,
            description: None,
            responsibilities: resp
                .map(|rid| {
                    vec![Responsibility {
                        id: rid.into(),
                        statement: "does a thing".into(),
                        vagrant: None,
                        stale: None,
                        directives: Vec::new(),
                        last_touched_at: None,
                    }]
                })
                .unwrap_or_default(),
            properties: Vec::new(),
            icon: None,
            visual: None,
            appearance: None,
            notes: None,
        }
    }

    #[test]
    fn subtree_serialization_excludes_siblings_and_ancestors() {
        let mut model = ScryModel::new();
        model.nodes.push(node("node-1", Kind::System, None, None));
        model.nodes.push(node("node-2", Kind::Container, Some("node-1"), None));
        model.nodes.push(node("node-3", Kind::Component, Some("node-2"), None));
        model.nodes.push(node("node-4", Kind::Symbol, Some("node-3"), Some("resp-1")));
        // a sibling container that must NOT leak into node-2's subtree
        model.nodes.push(node("node-5", Kind::Container, Some("node-1"), Some("resp-2")));
        model.source_map.insert(
            "resp-1".into(),
            vec![SourceLocation { pattern: "a.rs".into(), symbol: None, line: None, end_line: None, command: None }],
        );
        model.source_map.insert(
            "resp-2".into(),
            vec![SourceLocation { pattern: "b.rs".into(), symbol: None, line: None, end_line: None, command: None }],
        );
        model
            .boundaries
            .insert("node-2".into(), vec![Source { pattern: "x/**".into(), comment: None }]);

        let json = serialize_subtree_for_prompt(&model, "node-2");
        // the node + its descendants + its responsibility's source anchor
        assert!(json.contains("node-2"));
        assert!(json.contains("node-3"));
        assert!(json.contains("node-4"));
        assert!(json.contains("resp-1"));
        assert!(json.contains("a.rs"));
        // the sibling subtree and its source anchor must be absent
        assert!(!json.contains("node-5"), "sibling container leaked: {json}");
        assert!(!json.contains("resp-2"), "sibling responsibility leaked");
        assert!(!json.contains("b.rs"), "sibling source anchor leaked");
    }

    #[test]
    fn container_prompts_share_the_full_instruction_prefix() {
        let a = build_container_prompt("/a", "API", "node-2", r#"{"scope":"api"}"#);
        let b = build_container_prompt("/b", "Worker", "node-9", r#"{"scope":"worker"}"#);
        let a_prefix = a.split("## Task").next().unwrap();
        let b_prefix = b.split("## Task").next().unwrap();
        assert_eq!(a_prefix, b_prefix);
        assert!(a_prefix.len() > 3_000, "shared prefix should contain the rules");
    }
}

/// Prompt for DESIGNING the children of a node that has no code behind it —
/// greenfield design, or proposing a not-yet-built addition to an existing
/// model. There is no codebase to extract from, so the agent IS the source of
/// truth: it must model the structure AND every relationship completely, and
/// everything it adds is `proposed` (intent, not yet implemented in code).
/// Strict: nothing floats, every link is labeled, the model validates clean.
/// `node_kind` is one of "system", "container", "component".
pub fn node_design_prompt(
    project_path: &str,
    node_id: &str,
    node_name: &str,
    node_kind: &str,
    model_json: &str,
) -> String {
    let (child_kind_label, child_guidance) = match node_kind {
        "system" => (
            "containers (web apps, APIs, workers, data stores, queues)",
            r#"   - Each container's `name` describes the role; `technology` says what it IS as software (the stack you intend to build it on).
   - Write 2–6 pure business responsibilities per container.
   - Link the containers to each other and to the persons/externals that interact with them, per the legal-link rule below.
   - Group containers that deploy together using `set_groups`."#,
        ),
        "container" => (
            "components (logical modules inside the container)",
            r#"   - Components are cohesive logical modules, not one-per-file. Name each by the responsibility it owns.
   - Write 2–5 pure business responsibilities per component.
   - Link components to each other and to the references the container passes down (other containers/externals the container connects to), per the legal-link rule below.
   - Optionally group components into modules (`set_groups`)."#,
        ),
        "component" => (
            "symbols (the code definitions you intend to build — functions, classes, configs, data types)",
            r#"   - A symbol is one addressable definition you plan to build: a function, class, hook, React component, struct, interface, type, or config object. ONE symbol = ONE definition. Set `"visual": true` on symbols that will render UI.
   - Give each symbol 1–3 responsibilities (the behavior it will discharge) and/or `properties` (the data fields it will declare — one property per field). A pure data type carries only properties. Every symbol must carry one or the other: if a planned definition would carry neither — a trivial wrapper, a UI leaf — don't model it as its own symbol; fold it into the parent that uses it.
   - Link symbols to their sibling symbols and to the references the component passes down, per the legal-link rule below."#,
        ),
        _ => ("child nodes", ""),
    };

    format!(
        r#"You have access to the scryer MCP server (schema v0.3). DESIGN the internals of node "{node_name}" (id {node_id}) at {project_path}.

There is NO code behind "{node_name}" — you are not extracting an existing system, you are designing one. Everything you add is **proposed** intent the user means to build later. Because nothing can be read from code, YOU are responsible for modeling the structure AND every relationship correctly and completely.

## Current model state

The model is provided here so you can avoid calling `read_model`:

```json
{model_json}
```

## Core principles

- **You are the source of truth.** Do NOT call `read_codebase` or read source files — there is nothing to read. Design from the user's intent and the surrounding model.
- **Everything you add is `proposed`.** Set responsibility status to `proposed` on every responsibility you write (this is unbuilt intent, not implemented code).
- **Responsibilities are pure business statements.** Terse, verb-led, one clause each. No mechanism/technology words in the statement. A `description` is the node's identity in a few words, not a summary of its responsibilities. (`directives` are user-authored — never set them.)
- **Mention other nodes as wikilinks, by id.** When a statement or description mentions another node, write it as `[[node-id]]` — the UI resolves it to the node's current name — or `[[node-id|shown text]]` to fit the sentence. The mention still requires a structural link; declare both.
- **Responsibilities sit at the node's own altitude.** Each child discharges a subset of "{node_name}"'s responsibilities; a parent's responsibilities are fewer and broader than the union of its children's, never a per-child enumeration.

## Model EVERY relationship — this is the point of designing here

A node that connects to nothing is a design error. For every node you add:
- Link it to the things it actually relates to, and give EVERY link a clear **label** ("reads from", "sends events to", "authenticates against"). No bare, unlabeled arrows.
- **Legal links only.** A link connects two nodes that share a diagram: either two nodes with the SAME parent (true siblings), or a node and a *reference passed down to this level* — i.e. something the parent already links to, which therefore appears on this inner view. There are NO cross-level links: a child may only link outside its parent to a reference the parent surfaced. If you need a child to reach something the parent doesn't yet connect to, first add the parent→target link (one level up) so it becomes a reference here.
- A person or external that related to "{node_name}" must be re-pointed to the specific child it actually uses, or it floats when you drill in.

## Procedure

1. Call `get_rules` to load the modeling rules.
2. Call `read_model` with node "{node_id}" to see this node's context (its responsibilities and the references — persons/externals/siblings — its parent passes down).
3. Decide the {child_kind_label}. Add only nodes whose responsibilities ladder up to "{node_name}".
{child_guidance}
4. Call `set_node` on "{node_id}" with the new subtree: the child nodes AND the labeled links among them and to the passed-down references. Every child gets at least one labeled relationship at this level.
5. Set every responsibility's status to `proposed`.
6. Call `validate_model` and fix EVERY warning — especially "appears disconnected", which means a node has no relationship at this level. Re-link (legally) and re-validate until it is completely clean. Do not stop with warnings outstanding.
7. Call `read_model` with node "{node_id}" to confirm the subtree, then summarize the proposed design for the user.

Stay within the "{node_name}" subtree. Do not modify nodes outside this scope.
"#
    )
}

/// Prompt for filling out the children of an existing node from the codebase.
/// `node_kind` is one of "system", "container", "component".
pub fn node_fill_prompt(
    project_path: &str,
    node_id: &str,
    node_name: &str,
    node_kind: &str,
    model_json: &str,
) -> String {
    let (child_kind_label, child_guidance) = match node_kind {
        "system" => (
            "containers (web apps, APIs, workers, data stores, queues)",
            r#"   - Each container's `name` describes the role; `technology` says what it IS as software.
   - Write 2–6 pure business responsibilities per container. No mechanism vocabulary in the statement.
   - Add container-level links between the containers and to externals.
   - Group containers that deploy together using `set_groups`."#,
        ),
        "container" => (
            "components (logical code modules inside the container)",
            r#"   - Identify cohesive modules from the source directories. Components are logical groupings, not one-per-file.
   - Write 2–5 pure business responsibilities per component.
   - Add component-level links between components and to other containers / externals.
   - Optionally group components into modules (`set_groups`); nested groups can mirror directory organization."#,
        ),
        "component" => (
            "symbols (code definitions — functions, classes, configs, and data types)",
            r#"   - The code level uses ONE kind: `symbol`. A symbol is exactly one addressable code definition — a function, method, handler, hook, React component, class, struct, interface, type, or config object. The `name` is the identifier as it appears in the source. ONE symbol node = ONE definition; do not collapse a whole multi-function file into a single symbol. Set `"visual": true` on React components, Vue components, Svelte components, or any symbol that renders UI — anything whose output is visual and would benefit from a rendered preview. Model the PUBLIC surface only: a class is ONE symbol carrying its public operations as responsibilities — do NOT add its private/internal helper methods (e.g. `_rpc`, `_get_uid`, `_execute`) as their own symbols. And a generated/mirror type file (a `*-types` file, a `*.d.ts`, or any file that just re-declares a definition that already lives in real source) is a derived artifact — do NOT create a parallel symbol for it; fold its fields into the source-of-truth symbol so each record is modeled exactly once. Every symbol must carry semantic content of its own — a business responsibility or declared properties. A definition with neither (a trivial wrapper, a UI leaf like a `Chevron`, a re-export, a bare entry point) does NOT earn a node: omit it and let the parent symbol's responsibility cover it. Never emit an empty symbol, and never fabricate filler to justify one.
   - A symbol has two facets. Most have one; some have both. Populate whichever the definition actually has:
     - **responsibilities** — the behavior it discharges (1–3, pure business statements). Map each to the SPECIFIC LINES that do its work via `update_source_map`'s `entries`: `pattern` = file, `line`/`endLine` = the exact line range, `symbol` = the enclosing definition's name (anchor + context frame). A line range must be a PROPER subset of the symbol — when one responsibility is the whole definition's work, omit `line`/`endLine` entirely (a symbol-only anchor means "this whole definition"). The tool enforces this: a range covering the whole symbol is stripped to the symbol anchor. Two responsibilities on the same line range are one responsibility — merge them.
     - **properties** — if the definition DECLARES A DATA SHAPE (a struct/class/interface/type, OR a config object that defines a field schema — e.g. an ORM/CMS collection, a settings object), you MUST enumerate its fields as `properties`: one property per field, `label` = field name, `description` = what it holds, each with a status. For a framework registration object (a Payload CollectionConfig, an ORM model) descend INTO the `fields[]` array and emit one property per entry — those are the record's COLUMNS — NOT the sibling config wrapper keys (`slug`, `admin`, `hooks`, `access`, …), which are framework plumbing and must never appear as properties. Map the declaration block to the symbol's node id via `update_source_map`'s `schemas` array: `nodeId` = the symbol, `pattern` = file, `symbol` = the type/object name, `line`/`endLine` = the declaration range.
   - CRITICAL: NEVER fold a data shape into a responsibility. A responsibility like "Defines the lead record schema with status, qualification, and booking fields" is WRONG — delete that prose and list `status`, `qualification`, `booking`, … as actual `properties`. If a definition both wires behavior and declares fields (the common case for a CMS collection: it registers admin UI + lifecycle hooks AND defines the record's columns), give it BOTH — behavioral responsibilities for the hooks/UI and a property per declared field. A pure data type (a plain interface/struct) carries only properties and no responsibilities.
   - **Scoping**: determine parentage from the import/usage graph, not from file co-location. A code-level node belongs to whichever component actually owns/defines it. If sibling components import the same code, parent it to its owner and add links from the consumers — the cross-boundary dependency is valuable signal the user needs to see. Don't restructure to hide it.
   - Add links between symbols and to other components / containers / externals as needed."#,
        ),
        _ => ("child nodes", ""),
    };

    format!(
        r#"You have access to the scryer MCP server (schema v0.3). Fill out the internals of node "{node_name}" (id {node_id}) at {project_path}.

## Current model state

The model is provided here so you can avoid calling `read_model`:

```json
{model_json}
```

## Core principles

- **The codebase is evidence, not source of truth.** Read code to elicit responsibilities; don't transcribe.
- **Responsibilities are pure business statements.** No mechanism vocabulary, no technology names, no specific protocols. (`directives` carry required mechanisms, but they're user-authored — never set them.)
- **Write terse, scannable statements — not prose.** One verb-led clause per responsibility; lead with the distinguishing verb + object and stop. No trailing "by/where/so that …" tails, no repeating the obvious domain on every line. A `description` is the node's identity in a few words, not a summary of the responsibilities beneath it.
- **Mention other nodes as wikilinks, by id.** When a statement or description mentions another node, write it as `[[node-id]]` — the UI resolves it to the node's current name — or `[[node-id|shown text]]` to fit the sentence. The mention still requires a structural link; declare both.
- **Responsibilities sit at the node's own altitude.** Each child discharges a subset of "{node_name}"'s responsibilities — but a node's own responsibilities name what IT is accountable for, never what a child does to discharge it. A parent's responsibilities are fewer and broader than the union of its children's, never a per-child enumeration; if a line reads as describing a single child, it's one altitude too low. Verify after writing: every child maps back to something its parent is accountable for, or it's vagrant.

## Procedure

1. Call `get_rules` to load the modeling rules.
2. Call `read_model` with node "{node_id}" to see this node's full context (description, responsibilities, sources, existing links).
3. Use `read_codebase` with path "{project_path}" if you need to inspect source files. Open relevant files to identify {child_kind_label}.4. Call `set_node` on "{node_id}" with the new subtree (nodes + links). Add only nodes whose responsibilities ladder up to "{node_name}". Relationships must connect nodes at THIS level: link the new children to each other and to the reference nodes that surround "{node_name}" (the persons/externals/siblings that link to it). A person or external that used "{node_name}" should now link to the specific child it actually uses — otherwise it appears disconnected when you drill in. Every child needs at least one relationship at this level.
{child_guidance}
5. Set responsibility status to `implemented` on responsibilities derived from existing code; `proposed` on speculative ones.
6. Call `update_source_map` to write the code-side mapping. `boundaries`: node-level directory globs. `entries`: for each responsibility, the **specific lines** that do its work — `pattern` = file, `line`/`endLine` = the exact range of the statements implementing it, `symbol` = the enclosing definition's name (anchor + context). A line range must be a PROPER subset of the symbol; when one responsibility is the whole definition's work, omit `line`/`endLine` (a symbol-only anchor means the whole definition — the tool strips whole-symbol ranges and tells you). A responsibility may map to several ranges, possibly across files. `schemas`: for each symbol that declares a data shape (carries `properties`), its declaration block — `nodeId` = the symbol, one location with `pattern` = file, `symbol` = the type/object name, `line`/`endLine` = the declaration range.
7. Call `validate_model` and fix every warning — especially "appears disconnected", which means a node (or a reference node) has no relationship at this level. Re-link and re-validate until clean.
8. Call `read_model` with node "{node_id}" to confirm the subtree you built, then summarize it for the user.

Stay within the "{node_name}" subtree. Do not modify nodes outside this scope.
"#
    )
}
