use scryer_core::{Node, ScryModel};
use std::collections::HashSet;

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

1. **Refine the minted containers** via `update_nodes` (batch ONE call): `name` = the unit's role ("Desktop App", "MCP Server", "Docs Site" — the minted names are raw manifest names), `technology` = what it IS as software, as a short badge ("Tauri 2 + React", "Rust MCP server" — a few words, prose goes in `description`), and 2–6 terse, verb-led business responsibilities per container (status `implemented`). Write the system node's own responsibilities (1–4, broader than any container's) and a short description of what the system IS.
2. **Add persons and externals.** `add_person` for real users/actors the code evidences; `add_system` with external=true for third-party systems it depends on (only if evident from manifests/config — e.g. Stripe, S3, a managed database). Link them to the SYSTEM (id below) with `add_links`, never to containers.
3. **Add non-code containers** the manifests evidence but the scan can't mint — a managed database, a queue, a bucket — with `add_container` (parentId = the system id). Do NOT add, remove, rename-to-something-unrelated, or re-parent the minted code-bearing containers; refining their name/technology/responsibilities is yours, their existence is not.
4. **Label the links.** The minted container→container links carry no labels: `update_links` each with a clear label ("invokes", "reads models from"). Add missing container→container, container→external links with `add_links`.
5. **Group deploy units** with `add_group` ONLY when several containers ship/package together. Independent services get no groups — most small projects need none.

## Rules
- Responsibilities are pure business statements at the node's own altitude — no technology words, no mechanism, no per-component enumeration. The stack NAME belongs in the `technology` field (a short badge); mechanism explanations belong in `description`.
- When a description/statement names another node, declare the structural link the mention implies.
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

Call `fill_container` ONCE with:
- `containerId`: the id in the task section
- `components`: the complete component decomposition. Each component and nested symbol has a unique request-local `key`, plus its C4 name/description/responsibilities.
- `groups`: optional secondary groupings whose `memberKeys` use component local keys.
- `links`: OPTIONAL and almost always empty. You do NOT author code-level links — the server wires component→component and symbol→symbol links automatically from the dependency graph below. Only use `links` for a cross-boundary relationship the graph can't infer (a component reaching an external or another container by its existing node id). Anything illegal is dropped and reported, so never let a link block you.

The server validates the proposal, resolves local keys, mints every id, anchors source locations, derives the code-level links, and performs one model write. Do NOT call `add_component`, `add_symbol`, `add_links`, or `add_group` for this container.

### Components

CLUSTER components from cohesion + the dependency graph below: a component groups the files/symbols that work together toward one responsibility. Do NOT make one component per file. Give each a few terse business responsibilities AT THE COMPONENT'S ALTITUDE: each names one accountability the component holds, NOT what an individual symbol inside it does — the per-handler detail belongs on the symbols below. A component's responsibilities are FEWER and BROADER than the union of its symbols'; if a line reads as describing a single symbol, it is one altitude too low.

### Mandatory symbols

Every component must contain at least one architecturally meaningful symbol. For each nested symbol, pass a scope-unique `key`, `name`, `sourceFile`, and `line`/`endLine` for the full definition straight from the context. A responsibility is a plain string (use the plain form by default; only attach `{{"statement", "line", "endLine"}}` when a precise sub-range genuinely adds value). Give `responsibilities` for behavior and `properties` for declared data fields; set `visual: true` on UI-rendering symbols. **Every symbol you emit MUST carry semantic content of its own: at least one business/process responsibility, or — for a data type — its declared `properties`. There is no empty symbol.** A definition with no business responsibility and no declared data shape — a trivial UI leaf (a `Chevron`, a `Logo`), a thin wrapper, a re-export, a private helper — does NOT earn a node: OMIT it and let its parent symbol's responsibility cover it. **An entry point folds UP, not away:** a top-level `main` (or any binary entry that only wires up and dispatches the program's work — parses args, selects a subcommand, starts the server) carries the BINARY's accountability at the COMPONENT's altitude, not a symbol's, EVEN WHEN it clearly does something. Put that one responsibility on the enclosing COMPONENT and emit NO `main` symbol; mint symbols beneath it only for helpers that hold their own distinct responsibility. When a definition has nothing of its own to say, the answer is to leave it out — NEVER to fabricate filler, and NEVER to emit it with empty responsibilities and properties. A definition earns a symbol when it carries architectural behavior, a declared data shape, or a cross-boundary role. Skip trivial wrappers, thin re-exports, getters/setters, and test stubs:
  - **Framework registration objects** (a CMS collection config, an ORM model, a settings/route object): descend INTO its `fields[]` array (the schema columns) and emit one property per entry there. Its `properties` are those DECLARED FIELDS — the record's columns — NEVER the sibling config wrapper keys (`slug`, `admin`, `hooks`, `access`, …), which are framework plumbing, not the record's data shape.
  - **Generated / mirror type files** (a `*-types` file, a `*.d.ts`, or any file that just re-declares a definition that already lives in real source): do NOT create a parallel symbol for it. Fold its fields into the source-of-truth symbol so each thing is modeled exactly once — never two sibling symbols for the same record.
  - **Classes/objects with internal helpers**: model the class as ONE symbol carrying its public operations as responsibilities; do NOT add private/internal helper methods (e.g. `_rpc`, `_get_uid`, `_execute`) as their own symbols. Only addressable, public definitions become symbols.

## Procedure (minimize round-trips)
1. Decide the component clustering from the dependency graph below (group cohesive files/symbols). The evidence already embeds each symbol's source (`code`) — work from it directly. Open a source file ONLY when a truncated excerpt (`… +N lines`) leaves a symbol's accountability genuinely unclear; never re-read what is already inline.
2. Build the full proposal in memory with components and their mandatory nested symbols.
3. Do NOT build a `links` array for internal code dependencies — the server derives every component→component and symbol→symbol link from the dependency graph after it mints ids. Add a `links` entry ONLY for a cross-boundary relationship to an external or another container (by existing node id); call `read_model` once for the task's container if you need those ids. When in doubt, leave `links` empty.
4. Include optional groups only when several components form a cohesive secondary module.
5. Call `fill_container` once. It returns the ids it minted, the count of links it derived, and any links it dropped. If it rejects the proposal it is a STRUCTURAL problem (a missing symbol, a duplicate key) — fix exactly that and retry. Dropped links are informational, not a failure; do not retry over them. Do not run a separate validation loop.

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
/// container's compact code index with source evidence embedded for the changed
/// files only (so the agent judges inline instead of re-reading them);
/// `changed_files_json` is the changed-file list.
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
- **Undescribed behaviour:** the code provides a capability that NO responsibility in this subtree describes *at any altitude*. Report it under `undescribed` (a terse business statement + the `sourceFile` + the enclosing `symbol`). Each becomes a vagrant adoption the user adopts (commit — the code exists) or rejects (mark the code for deletion). State the behaviour at the altitude of the node it lands on (a symbol-level capability on a symbol, never a container-level summary). HOME each finding on the right node:
  - **An existing node already models this symbol/file** → cite the `symbol` precisely and it routes there automatically; or set `nodeId` to force a specific existing node.
  - **The model has NO node for this code** (a new function, a new file, a whole new area) → MINT the missing rungs in `newNodes` and point the finding at the leaf with `nodeKey`. Give each new node a `key`, `kind` ("component"/"symbol"), and `name` (a symbol's name is the exact identifier); hang the shallowest new rung off an existing node with `parentId`, and chain deeper rungs with `parentKey` (list ancestors before descendants). You know how this code is organised — define the structure it belongs in. NEVER let a symbol-level finding bubble up to this container because no finer node exists: mint the node instead.
- **Stale claims:** an existing responsibility whose code no longer discharges it — the implementation was removed, or now does something materially different. Report its `responsibilityId` under `stale` with a short factual `reason`.
- **Stale nodes (code gone entirely):** when a deleted file or folder wipes out a whole modeled node — a symbol whose definition is gone, a component or container subtree whose directory no longer exists — flag the NODE itself under `staleNodes` (its `nodeId` + a short `reason`) instead of listing each of its claims. The verdict then applies to the whole subtree. This is the mirror of minting a `newNodes` chain for code the model has no node for: here the model has a node for code that's gone.
- **Undescribed data fields:** a newly declared field on a data type — a struct field, an interface/record member, an enum case — that NO `property` on the owning data node describes. This is DATA, not behaviour: report it under `undescribedProperties` (its `label`, an optional one-line `description`, the `sourceFile`, and the enclosing type's `symbol`), homed exactly like an undescribed behaviour — cite the `symbol` to auto-route, or set `nodeId`/`nodeKey`. Each becomes a vagrant property the user adopts or rejects. Describe what the field HOLDS, never the behaviour it enables — a `confirm_launch: bool` field is "whether to confirm before launching", NOT "gates launches behind confirmation" (that behaviour, if real, lives on the code that enforces it).
- **Stale properties:** an existing property whose backing field was removed or materially changed (renamed, retyped, repurposed). Report it under `staleProperties` (`nodeId` + the property `label` + a short `reason`) — the data-shape mirror of a stale claim.

## What is NOT drift (do not flag)
- Code that changed but still satisfies an existing responsibility. A refactor that preserves behaviour is not drift. The user does not care that lines moved or bytes changed — only that the model's description still matches reality.
- **Mechanism beneath an existing responsibility.** A responsibility is described at its own altitude and SUBSUMES its implementation detail. If a node already claims "Validate structural correctness", then each individual check it performs (a length cap, an id-uniqueness rule, a kind constraint) is HOW that claim is discharged — already described, do NOT flag it. Likewise a new branch inside a handler, a new field on a validator, a new helper call. Only flag a behaviour that is a genuinely NEW capability the model names nowhere — never decompose one responsibility into a list of its mechanics.
- **A data field is not a behaviour.** When the only change to a type is its SHAPE — a field added, removed, or retyped — that is a `property` finding (`undescribedProperties` / `staleProperties`), NEVER a responsibility. Do not invent a verb-led "behaviour" to describe a plain field; if a struct gains `confirm_launch: bool`, the finding is a property named `confirm_launch`, not a responsibility about gating. A field already covered by one of the node's properties is not drift at all.

## Focus — these files changed
```json
{changed_files_json}
```
Their current source is already embedded in the code index below — every symbol in a changed file carries its `code`. Judge behaviour from that evidence directly. Open a file ONLY when a truncated excerpt (`… +N lines`) leaves the behaviour genuinely unclear, or when a changed file has no entry in the index (deleted, or not parseable source — a deleted file's absence is itself the evidence for `staleNodes`). Compare against the claims below.

## The model's current claims for "{node_name}" and everything under it
```json
{subtree_json}
```

## Code index for this container (where everything is)
The payload is compact and indexed:
- `paths[n]` is a project-relative source path.
- each file has `path` = index into `paths`.
- each symbol has a scope-global integer `id`, `name`, inclusive `lines`, optional `fields`, and `data: true` for data shapes.
- symbols in the CHANGED files carry `code` — doc comment + signature + body; a trailing `… +N lines` marker means the definition continues in the file. Unchanged files carry no `code`: they are the map of what exists, not the evidence.
- `symbolEdges` entries are `[sourceSymbolId, destinationSymbolId]`.
- `fileEdges` entries are `[sourcePathIndex, destinationPathIndex]`, partitioned into `internal`, `outbound`, and `inbound`.

```json
{scope_json}
```

Call `flag_drift` for "{node_id}" with everything you found. If the code and the model still agree, call it with empty arrays. Do not call the `add_*` tools and do not edit existing responsibilities — express any new structure through `flag_drift`'s `newNodes`, nothing else.
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
            vagrant: None,
            stale: None,
            parent_id: parent.map(|s| s.into()),
            external: None,
            technology: None,
            description: None,
            responsibilities: resp
                .map(|rid| {
                    vec![Responsibility {
                        concern: None,
                        id: rid.into(),
                        statement: "does a thing".into(),
                        vagrant: None,
                        stale: None,
                        stale_proposal: None,
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
            position: None,
            directives: Vec::new(),
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
