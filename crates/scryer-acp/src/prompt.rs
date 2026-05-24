use scryer_core::ScryModel;

/// Serialize a ScryModel as compact JSON for embedding in an agent prompt.
/// Strips empty arrays / null fields so the agent context isn't bloated.
pub fn serialize_model_for_prompt(model: &ScryModel) -> String {
    let mut val = serde_json::to_value(model).unwrap_or(serde_json::Value::Null);
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
- **Every node justifies its existence through responsibilities.** A child node exists to discharge a subset of its parent's responsibilities. A node with no responsibility — or whose responsibilities serve no ancestor commitment — is structurally vagrant.

## Procedure

1. Call `get_rules` to load the modeling rules.
2. Call `get_structure` with path "{project_path}" to get the annotated directory tree. Read the manifests it surfaces (package.json, Cargo.toml, fly.toml, Dockerfile, .env.example, etc.) to identify deployable units, data stores, external services, and frameworks. Use subagents to read multiple source directories in parallel — don't serialize file reads across unrelated parts of the tree.
3. **Build the system level.** Call `set_model` with the persons (real users / actors), the system itself, and external systems (third-party services the system depends on — Stripe, S3, Resend, etc.; mark these `external: true`). Add system-level links: persons and external systems connect to the SYSTEM itself, not to its internal containers — those are container-level relationships added when you drill in. Every person/external must link to the system, or it appears disconnected on the system-context diagram. For each node, write 1–4 responsibilities. Set responsibility status to `implemented` on responsibilities derived from existing code, `proposed` on anything speculative.
4. **Add containers.** Call `set_node` on the system id with a payload containing the containers (web apps, APIs, workers, databases, message queues, file stores). For each container:
   - Set `kind: "container"`, `name` describes the role ("Website", "Worker", "CMS"), `technology` describes what it IS as software ("Next.js 14", "PostgreSQL 16", "S3 Bucket").
   - Write 2–6 responsibilities — pure business statements about what the container is accountable for. No technology words in the statement.
   - Include container-level links (Person→Container, Container→Container, Container→External).
5. **Group containers.** Call `set_groups` to create deployment-unit groups for containers that ship together (e.g. multiple containers running inside one Next.js app, multiple AWS resources provisioned by one Terraform module). A group can carry its own deployment-shaped responsibilities ("deploys atomically", "must fit in 256 MB").
6. **Stop here.** Do not add components or code-level nodes. The user requests component detail explicitly.
7. Call `update_source_map` with `boundaries` to attach a directory glob to each container that has code (a boundary entry per node, e.g. pattern "apps/web/**/*").
8. Call `validate_model` and fix every warning — especially "appears disconnected", which means a node has no relationship at its own C4 level (e.g. a person/external linked to a container but not to the system). Re-link and re-validate until clean.
9. Call `get_changes` to summarize what was modeled.

## Don'ts

- Don't add responsibilities the codebase doesn't already evidence. If the codebase doesn't handle a concern, the model shouldn't claim it does.
- Don't put technology vocabulary inside responsibility statements. The `technology` field is the place for that. (`directives` are user-authored constraints — never set them.)
- Don't model framework internals (e.g. ORM layers, admin panels that come with a CMS) as separate containers unless they have a distinct user-facing surface that warrants its own tour.
- Don't draw a separate edge for each interaction between two nodes — one link per relationship.
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
            "symbols (code definitions) and schemas (data types)",
            r#"   - Symbol: exactly one addressable code definition — a function, method, handler, hook, React component, or class. The `name` is the identifier as it appears in the source. ONE symbol node = ONE definition; do not collapse a whole multi-function file into a single symbol. Give each symbol the 1–3 responsibilities that *that one definition* discharges.
   - Map each responsibility to the SPECIFIC LINES that do its work, via `update_source_map`: `pattern` = file, `line`/`endLine` = the exact line range of the statements discharging it, `symbol` = the enclosing definition's name (the anchor + context frame). Do NOT map a responsibility to the whole symbol — pick the lines that actually implement it. Several responsibilities may share an enclosing `symbol` but must point at different line ranges; if two would point at the identical range, they are one responsibility — merge them. Symbols get NO boundary glob.
   - Schema: a data type (struct, class, interface). Name is a valid type name. Schemas carry `properties` (label/description pairs each with a status), NOT responsibilities. Map each schema to where its type is declared via `update_source_map`'s `schemas` array: `nodeId` = the schema node, one location with `pattern` = file, `symbol` = the type name, `line`/`endLine` = the declaration range.
   - **Scoping**: determine parentage from the import/usage graph, not from file co-location. A code-level node belongs to whichever component actually owns/defines it. If sibling components import the same code, parent it to its owner and add links from the consumers — the cross-boundary dependency is valuable signal the user needs to see. Don't restructure to hide it.
   - Add links between symbols and to other components / containers / externals as needed.
   - The code level uses only `symbol` and `schema` — no `process` kind exists."#,
        ),
        _ => ("child nodes", ""),
    };

    format!(
        r#"You have access to the scryer MCP server (schema v0.3). Fill out the internals of node "{node_name}" (id {node_id}) at {project_path}.

## Current model state

The model is provided here so you can avoid calling `get_model`:

```json
{model_json}
```

## Core principles

- **The codebase is evidence, not source of truth.** Read code to elicit responsibilities; don't transcribe.
- **Responsibilities are pure business statements.** No mechanism vocabulary, no technology names, no specific protocols. (`directives` carry required mechanisms, but they're user-authored — never set them.)
- **Write terse, scannable statements — not prose.** One verb-led clause per responsibility; lead with the distinguishing verb + object and stop. No trailing "by/where/so that …" tails, no repeating the obvious domain on every line. A `description` is the node's identity in a few words, not a summary of the responsibilities beneath it.
- **Each child discharges a subset of "{node_name}"'s responsibilities.** Verify after writing: every child node's responsibilities map back to something its parent is accountable for. If a child's responsibilities don't ladder up, it's vagrant.

## Procedure

1. Call `get_rules` to load the modeling rules.
2. Call `get_node` with id "{node_id}" to see this node's full context (description, responsibilities, sources, existing links).
3. Use `get_structure` with path "{project_path}" if you need to inspect source files. Open relevant files to identify {child_kind_label}. Use subagents to read multiple source directories in parallel — don't serialize file reads across unrelated parts of the tree.
4. Call `set_node` on "{node_id}" with the new subtree (nodes + links). Add only nodes whose responsibilities ladder up to "{node_name}". Relationships must connect nodes at THIS level: link the new children to each other and to the reference nodes that surround "{node_name}" (the persons/externals/siblings that link to it). A person or external that used "{node_name}" should now link to the specific child it actually uses — otherwise it appears disconnected when you drill in. Every child needs at least one relationship at this level.
{child_guidance}
5. Set responsibility status to `implemented` on responsibilities derived from existing code; `proposed` on speculative ones.
6. Call `update_source_map` to write the code-side mapping. `boundaries`: node-level directory globs. `entries`: for each responsibility, the **specific lines** that do its work — `pattern` = file, `line`/`endLine` = the exact range of the statements implementing it, `symbol` = the enclosing definition's name (anchor + context). Map to the lines that actually discharge the responsibility, NOT the whole enclosing symbol or file. A responsibility may map to several ranges, possibly across files. `schemas`: for each schema node, its type declaration — `nodeId`, one location with `pattern` = file, `symbol` = the type name, `line`/`endLine` = the declaration range.
7. Call `validate_model` and fix every warning — especially "appears disconnected", which means a node (or a reference node) has no relationship at this level. Re-link and re-validate until clean.
8. Call `get_changes` to summarize what was added.

Stay within the "{node_name}" subtree. Do not modify nodes outside this scope.
"#
    )
}
