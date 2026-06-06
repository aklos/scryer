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

/// Wave 1 of the auto-context build: create the system + container levels from
/// the deterministic codebase context (the containers were already discovered
/// from manifests, so the agent skips structure discovery and goes straight to
/// naming roles + writing responsibilities). Built with the intent tools only.
/// `containers_json` is the serialized container facts (dir, name, technology,
/// dependency edges) from the context engine.
pub fn build_system_prompt(project_path: &str, containers_json: &str) -> String {
    format!(
        r#"You have the scryer MCP server (schema v0.3). Build the SYSTEM and CONTAINER levels of the architecture model for the project at {project_path}. The codebase has already been scanned: its deployable units are given below, so you do NOT need to discover structure — go straight to the meaning.

## How to build (use the intent tools — never emit model JSON)
- `add_system` — create the system being modeled (the project itself). Add external third-party systems it depends on as separate systems with external=true (only if evident from manifests/config — e.g. Stripe, S3, a managed database).
- `add_person` — real users / actors, if the codebase evidences them.
- `add_container` — one per deployable unit below. Pass `parentId` = the system id, `name` = its role, `technology` = what it IS as software (refine the declared technology into a clear label), and `boundaryDir` = its `dir` from the context (sets the boundary glob for you). One unit may have an EMPTY `dir` — that is the project root itself (the primary application, e.g. the web app the repo is built around). It is a real container distinct from the system node: create it too, name its role, and pass `boundaryDir: ""`. Do NOT collapse it into the system or skip it.
- `add_links` — connect persons and externals to the SYSTEM (not to containers), and containers to each other along the dependency edges, and containers to externals.
- `add_group` — OPTIONAL, and a SECONDARY axis (never a substitute for the container decomposition). Group containers that deploy or package together (e.g. several services provisioned by one module / compose file). Pass `parent_id` = the system id and `member_ids` = the container ids. SKIP grouping when the containers are independent services — most small projects ship each unit on its own and need no groups.

Each tool takes plain responsibility statements (one terse verb-led business clause each — no mechanism/technology words) at the NODE'S OWN altitude: a container's responsibilities say what the container is accountable for, not what its individual components do — that finer detail lands when you model the components in a later pass. Each tool returns the node it created so you have its id for links. Responsibility status is set to implemented for you.

## Procedure (minimize round-trips)
1. Read the manifests and a few entry-point files for each unit below — only enough to state what each unit is accountable for. Do NOT enumerate components or symbols; that is a later pass.
2. `add_system` for the project; add any externals and persons.
3. `add_container` for each unit below (carry over its dependency edges as container→container links).
4. `add_links` for person→system, container→container, container→external.
5. ONLY if some containers clearly deploy/package together, `add_group` them. Independent services get no groups.
6. Call `validate_model` and fix every warning — especially rejected cross-level links and "appears disconnected". Re-link and re-validate until clean.
7. Stop at the container level. Do not add components or code-level nodes.

## Deployable units (from the codebase scan)
```json
{containers_json}
```

Write only what the code evidences. Keep responsibilities terse and business-level; put technology in the `technology` field, never in a responsibility.
"#
    )
}

/// Wave 2 of the auto-context build: model the components + symbols of ONE
/// container from its sliced code context. The agent clusters components from
/// the dependency graph (NOT one-per-file) and writes symbols with their
/// responsibilities/properties, all via the intent tools. `scope_json` is the
/// serialized per-scope context (files, symbols with line ranges, internal
/// symbol/file edges, cross-boundary edges).
pub fn build_container_prompt(
    project_path: &str,
    container_name: &str,
    container_id: &str,
    scope_json: &str,
) -> String {
    format!(
        r#"You have the scryer MCP server (schema v0.3). Model the COMPONENTS and SYMBOLS inside the container "{container_name}" (id {container_id}) of the project at {project_path}. Its code has already been indexed — every file, every symbol with its line range, and the dependency edges are given below, so you do NOT need to discover structure. Go straight to the meaning.

## How to build (use the intent tools — never emit model JSON)
- `add_component` — `parentId` = "{container_id}". CLUSTER components from cohesion + the dependency graph below: a component groups the files/symbols that work together toward one responsibility. Do NOT make one component per file. Give each a few terse business responsibilities AT THE COMPONENT'S ALTITUDE: each names one accountability the component holds, NOT what an individual symbol inside it does — the per-handler detail belongs on the symbols below. A component's responsibilities are FEWER and BROADER than the union of its symbols'; if a line reads as describing a single symbol (e.g. "Verify HMAC-SHA256 signatures", "Parse payloads into InboundMessage structs"), it is one altitude too low — lift it to what those symbols collectively serve (e.g. "Rejects forged and duplicate events", "Hands valid messages to the router").
- `add_symbol` — `parentId` = the component id you just created. Pass `name` (the identifier), `sourceFile` and `line`/`endLine` (for the full definition) straight from the context (the source map is anchored for you). Each responsibility can be a plain string OR `{{"statement", "line", "endLine"}}` with the specific line sub-range within the symbol that does the work — use the rich form when the symbol is large and the responsibility covers a distinct section. Give `responsibilities` for behavior — one verb-led clause each (split run-on, multi-clause prose into separate responsibilities); give `properties` (one per field) when the symbol declares a data shape — never fold a data shape into a responsibility. Set `visual: true` on React components, Vue components, Svelte components, or any symbol that renders UI — anything whose output is visual and would benefit from a rendered preview. A definition earns a symbol ONLY when it carries architecture — a behavioral responsibility at its own altitude, a declared data shape, or a cross-boundary link. Being a real public definition is NOT enough: skip trivial pass-through wrappers, thin re-exports, getters/setters, and test stubs — fold what they do into the component's responsibilities. Aim for a handful of meaningful symbols per component, NOT a mirror of every definition in its files:
  - **Framework registration objects** (a CMS collection config, an ORM model, a settings/route object): descend INTO its `fields[]` array (the schema columns) and emit one property per entry there. Its `properties` are those DECLARED FIELDS — the record's columns — NEVER the sibling config wrapper keys (`slug`, `admin`, `hooks`, `access`, …), which are framework plumbing, not the record's data shape.
  - **Generated / mirror type files** (a `*-types` file, a `*.d.ts`, or any file that just re-declares a definition that already lives in real source): do NOT create a parallel symbol for it. Fold its fields into the source-of-truth symbol so each thing is modeled exactly once — never two sibling symbols for the same record.
  - **Classes/objects with internal helpers**: model the class as ONE symbol carrying its public operations as responsibilities; do NOT add private/internal helper methods (e.g. `_rpc`, `_get_uid`, `_execute`) as their own symbols. Only addressable, public definitions become symbols.
- `add_links` — connect components to each other along the internal dependency edges below. Where a specific component is the one that actually uses an external system or other container that "{container_name}" links to (see its cross-boundary edges below), link that component to it — e.g. the component that calls OpenAI links to OpenAI. (You can only link to a node "{container_name}" already links to; that container-level link is what makes it available here. No need to push every container-level relationship down — only wire the components that genuinely use it.)
- `add_group` — OPTIONAL, a SECONDARY axis (never a substitute for component decomposition). When several components form one cohesive module (e.g. a set of external-integration components, a group of CMS/data-model components, a batch of background jobs), group them: `parent_id` = "{container_id}", `member_ids` = those component ids. SKIP it for a container with only a handful of unrelated components.

Each tool takes plain responsibility statements and returns the node it created (with its id) so you can parent symbols and draw links. Status is set to implemented for you.

## Procedure (minimize round-trips)
1. Decide the component clustering from the dependency graph below (group cohesive files/symbols). Read the actual source for each cluster — only enough to state responsibilities accurately.
2. `add_component` for each cluster under "{container_id}".
3. `add_symbol` for the architecturally meaningful definitions in each component (batch them), carrying over the provided file + line ranges; add `properties` (the declared fields, not config wrapper keys) for data shapes. A handful of meaningful symbols per component — fold generated mirror types in, leave private helpers, pass-through wrappers, and trivial definitions out.
4. `add_links` for component→component along the internal edges, AND component→external / component→other-container for each cross-boundary edge "{container_name}" has — linking the specific component that uses it.
5. If several components form one cohesive module, `add_group` them (a few components ⇒ skip).
6. Call `validate_model` and fix every warning — especially rejected cross-level links and "appears disconnected". Re-link and re-validate until clean.

## Code context for this container
```json
{scope_json}
```

Cluster by what the code DOES, not by file layout. Keep responsibilities terse and business-level.
"#
    )
}

/// Semantic drift check for one container whose code changed. The agent
/// compares what the code DOES against the model's responsibilities and reports
/// findings via `flag_drift` only — undescribed behaviour (→ vagrant
/// responsibilities) and stale claims (→ `changed`). It must NOT report mere
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
- **Undescribed behaviour:** the code does something that NO responsibility in this subtree describes. Report it under `undescribed` (a terse business statement + the `sourceFile` + the enclosing `symbol`). Each becomes a vagrant responsibility for the user to adopt or reject.
- **Stale claims:** an existing responsibility whose code no longer discharges it — the implementation was removed, or now does something materially different. Report its `responsibilityId` under `stale` with a short factual `reason`.

## What is NOT drift (do not flag)
- Code that changed but still satisfies an existing responsibility. A refactor that preserves behaviour is not drift. The user does not care that lines moved or bytes changed — only that the model's description still matches reality.

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
pub fn preview_render_prompt(
    project_path: &str,
    node_id: &str,
    node_name: &str,
    source_file: &str,
    source_lines: &str,
) -> String {
    format!(
        r#"You are rendering a live preview of the visual component "{node_name}" (id {node_id}) in the project at {project_path}.

## The component

Source file: `{source_file}`

```
{source_lines}
```

## Your task

Write `main.tsx` for a preview harness that renders this component with realistic fixture data. The harness boilerplate (index.html, vite.config.ts, preview.css, Tauri stubs) is already set up at `.scryer/preview/{node_id}/` — you only need to write `main.tsx`.

### Steps

1. **Read the component source** (and its imports) to understand:
   - What props it expects (from TypeScript types or PropTypes)
   - What context providers it needs (Router, Theme, Store, etc.)
   - What data shape it expects (from types/interfaces)

2. **Write `.scryer/preview/{node_id}/main.tsx`**:
   - Import `./preview.css` (already set up to include the project's CSS + Tailwind scanning)
   - Import the component from its project-relative path (e.g. `../../../src/MyComponent`)
   - Import `createRoot` from `react-dom/client`
   - Wrap the component in any required providers, pass realistic fixture props, render into `#root`

3. **Generate realistic fixture data** — not placeholder text. Infer sensible values from prop types and the component's purpose. If the component renders a list, include 3-5 items. If it shows user data, use realistic names/emails.

The preview is built for you after you write `main.tsx` — you do not run any build yourself.

### Rules

- Do NOT modify any project source files.
- Do NOT create or modify index.html, vite.config.ts, preview.css, or stubs/ — they are pre-generated.
- The ONLY file you write is `.scryer/preview/{node_id}/main.tsx`.
- Keep the harness minimal. The component IS the preview — don't add navigation chrome, debug tools, or extra UI.
"#
    )
}

/// Prompt for generating visual variations of a component. The agent creates N
/// different `main.tsx` files, each a distinct visual interpretation of the
/// user's request — the caller builds each afterward. Each variation imports the
/// original component and applies changes through CSS overrides, wrappers, modified
/// props, or inline reimplementation — the project source is never modified.
pub fn visual_variation_prompt(
    project_path: &str,
    node_id: &str,
    node_name: &str,
    source_file: &str,
    source_lines: &str,
    user_prompt: &str,
    existing_main_tsx: &str,
    variation_count: usize,
) -> String {
    let existing_section = if existing_main_tsx.is_empty() {
        String::new()
    } else {
        format!(
            "\n## Current render harness (main.tsx — use as starting point)\n\n```tsx\n{existing_main_tsx}\n```\n"
        )
    };

    let last_idx = variation_count - 1;
    let source_stripped = source_file.trim_start_matches('/');

    format!(
        r#"You are generating {variation_count} visual variations of the component "{node_name}" (id {node_id}) in the project at {project_path}.

## The component

Source file: `{source_file}`

```
{source_lines}
```
{existing_section}
## The user's request

{user_prompt}

## Your task

Create {variation_count} DISTINCT visual interpretations of the user's request. Each variation must be a genuinely different approach, not minor tweaks of the same idea.

For EACH variation (0 through {last_idx}):

1. **Write `.scryer/preview/{node_id}/variations/{{n}}/main.tsx`** where `{{n}}` is the variation index (0, 1, 2).

2. Each `main.tsx` must:
   - Import `./preview.css` (harness CSS with the project's styles + Tailwind)
   - Import the original component from its project-relative path: `../../../../../{source_stripped}` (5 directories up from the variation dir to the project root)
   - Import `createRoot` from `react-dom/client`
   - Apply the requested visual change through one or more of:
     - **CSS overrides** — inject a `<style>` element or use inline styles on a wrapper div
     - **Wrapper components** — wrap the original to modify layout, spacing, or behavior
     - **Modified props** — change props that affect appearance
     - **Inline reimplementation** — for structural changes, reimplement the relevant parts inline while importing shared dependencies from the project
   - Render into `#root`

Each variation is built for you after you write its `main.tsx` — you do not run any build yourself.

### Path reference

From a variation directory (`.scryer/preview/{node_id}/variations/{{n}}/`), the project root is 5 directories up: `../../../../..`

### Variation guidelines

- Each variation should be a genuinely different visual approach
- Example — "make the header sticky": one uses CSS `position: sticky` with shadow, another `position: fixed` with backdrop-blur, a third adds a condensed mode on scroll
- Example — "reduce padding": one goes minimal, another balanced with more whitespace on specific sides, a third uses asymmetric padding with tighter vertical
- Make each variation complete and functional, not half-finished sketches
- Preserve the component's functionality — only modify visual aspects

### Rules

- Do NOT modify any project source files
- Do NOT modify harness files (index.html, vite.config.ts, preview.css, stubs/) — they are pre-generated
- The ONLY files you write are the {variation_count} `main.tsx` files in `variations/{{0..{last_idx}}}/`
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
                        status: None,
                        vagrant: None,
                        locked: None,
                        relocated_to: None,
                        relocated_from: None,
                        directives: Vec::new(),
                        last_touched_at: None,
                    }]
                })
                .unwrap_or_default(),
            properties: Vec::new(),
            icon: None,
            visual: None,
            appearance: None,
            deprecated: None,
            relocated: None,
            locked: None,
            relocated_to: None,
            relocated_from: None,
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
            r#"   - The code level uses ONE kind: `symbol`. A symbol is exactly one addressable code definition — a function, method, handler, hook, React component, class, struct, interface, type, or config object. The `name` is the identifier as it appears in the source. ONE symbol node = ONE definition; do not collapse a whole multi-function file into a single symbol. Set `"visual": true` on React components, Vue components, Svelte components, or any symbol that renders UI — anything whose output is visual and would benefit from a rendered preview. Model the PUBLIC surface only: a class is ONE symbol carrying its public operations as responsibilities — do NOT add its private/internal helper methods (e.g. `_rpc`, `_get_uid`, `_execute`) as their own symbols. And a generated/mirror type file (a `*-types` file, a `*.d.ts`, or any file that just re-declares a definition that already lives in real source) is a derived artifact — do NOT create a parallel symbol for it; fold its fields into the source-of-truth symbol so each record is modeled exactly once.
   - A symbol has two facets. Most have one; some have both. Populate whichever the definition actually has:
     - **responsibilities** — the behavior it discharges (1–3, pure business statements). Map each to the SPECIFIC LINES that do its work via `update_source_map`'s `entries`: `pattern` = file, `line`/`endLine` = the exact line range, `symbol` = the enclosing definition's name (anchor + context frame). Do NOT map a responsibility to the whole symbol. Two responsibilities on the same line range are one responsibility — merge them.
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
- **Responsibilities sit at the node's own altitude.** Each child discharges a subset of "{node_name}"'s responsibilities — but a node's own responsibilities name what IT is accountable for, never what a child does to discharge it. A parent's responsibilities are fewer and broader than the union of its children's, never a per-child enumeration; if a line reads as describing a single child, it's one altitude too low. Verify after writing: every child maps back to something its parent is accountable for, or it's vagrant.

## Procedure

1. Call `get_rules` to load the modeling rules.
2. Call `read_model` with node "{node_id}" to see this node's full context (description, responsibilities, sources, existing links).
3. Use `read_codebase` with path "{project_path}" if you need to inspect source files. Open relevant files to identify {child_kind_label}.4. Call `set_node` on "{node_id}" with the new subtree (nodes + links). Add only nodes whose responsibilities ladder up to "{node_name}". Relationships must connect nodes at THIS level: link the new children to each other and to the reference nodes that surround "{node_name}" (the persons/externals/siblings that link to it). A person or external that used "{node_name}" should now link to the specific child it actually uses — otherwise it appears disconnected when you drill in. Every child needs at least one relationship at this level.
{child_guidance}
5. Set responsibility status to `implemented` on responsibilities derived from existing code; `proposed` on speculative ones.
6. Call `update_source_map` to write the code-side mapping. `boundaries`: node-level directory globs. `entries`: for each responsibility, the **specific lines** that do its work — `pattern` = file, `line`/`endLine` = the exact range of the statements implementing it, `symbol` = the enclosing definition's name (anchor + context). Map to the lines that actually discharge the responsibility, NOT the whole enclosing symbol or file. A responsibility may map to several ranges, possibly across files. `schemas`: for each symbol that declares a data shape (carries `properties`), its declaration block — `nodeId` = the symbol, one location with `pattern` = file, `symbol` = the type/object name, `line`/`endLine` = the declaration range.
7. Call `validate_model` and fix every warning — especially "appears disconnected", which means a node (or a reference node) has no relationship at this level. Re-link and re-validate until clean.
8. Call `read_model` with node "{node_id}" to confirm the subtree you built, then summarize it for the user.

Stay within the "{node_name}" subtree. Do not modify nodes outside this scope.
"#
    )
}
