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
- **Responsibilities are pure business statements.** A responsibility says what a node is accountable for in business terms, never how. "restricts access to private content" — yes. "restricts access via JWT" — no, the "via JWT" is mechanism. Implementation detail goes in the responsibility's `implementationRules` field, never in the statement.
- **Every node justifies its existence through responsibilities.** A child node exists to discharge a subset of its parent's responsibilities. A node with no responsibility — or whose responsibilities serve no ancestor commitment — is structurally vagrant.

## Procedure

1. Call `get_rules` to load the modeling rules.
2. Call `get_structure` with path "{project_path}" to get the annotated directory tree. Read the manifests it surfaces (package.json, Cargo.toml, fly.toml, Dockerfile, .env.example, etc.) to identify deployable units, data stores, external services, and frameworks.
3. **Build the system level.** Call `set_model` with the persons (real users / actors), the system itself, and external systems (third-party services the system depends on — Stripe, S3, Resend, etc.; mark these `external: true`). Add system-level links between them. For each node, write 1–4 responsibilities. Set responsibility status to `implemented` on responsibilities derived from existing code, `proposed` on anything speculative.
4. **Add containers.** Call `set_node` on the system id with a payload containing the containers (web apps, APIs, workers, databases, message queues, file stores). For each container:
   - Set `kind: "container"`, `name` describes the role ("Website", "Worker", "CMS"), `technology` describes what it IS as software ("Next.js 14", "PostgreSQL 16", "S3 Bucket").
   - Write 2–6 responsibilities — pure business statements about what the container is accountable for. No technology words in the statement.
   - Include container-level links (Person→Container, Container→Container, Container→External).
5. **Group containers.** Call `set_groups` to create deployment-unit groups for containers that ship together (e.g. multiple containers running inside one Next.js app, multiple AWS resources provisioned by one Terraform module). A group can carry its own deployment-shaped responsibilities ("deploys atomically", "must fit in 256 MB").
6. **Stop here.** Do not add components or code-level nodes. The user requests component detail explicitly.
7. Call `update_source_map` to attach a directory glob to each container that has code (`pattern: "apps/web/**/*"`).
8. Call `get_changes` to summarize what was modeled.

## Don'ts

- Don't add responsibilities the codebase doesn't already evidence. If the codebase doesn't handle a concern, the model shouldn't claim it does.
- Don't put technology vocabulary inside responsibility statements. `technology` and `implementationRules` are the places for that.
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
            "operations (leaf behaviors) and models (data types)",
            r#"   - Operation: the smallest behavioral unit inside a component — a function, handler, hook, or UI sub-component (e.g. a React component that lives inside one module). Name must be a valid identifier in the codebase's language (snake_case for Rust/Python/Ruby/Go; camelCase for JS/TS/Java). Each operation gets 1–3 business responsibilities.
   - Model: a data type (struct, class, interface). Name is a valid type name (PascalCase typical). Models carry `properties` (label/description pairs), NOT responsibilities.
   - **Scoping**: determine parentage from the import/usage graph, not from file co-location. A code-level node belongs to whichever component actually owns/defines it. If sibling components import the same code, parent it to its owner and add links from the consumers — the cross-boundary dependency is valuable signal the user needs to see. Don't restructure to hide it.
   - Add links between operations and to other components / containers / externals as needed.
   - The code level uses only `operation` and `model` — no `process` kind exists."#,
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
- **Responsibilities are pure business statements.** No mechanism vocabulary, no technology names, no specific protocols. Implementation detail goes in `implementationRules`.
- **Each child discharges a subset of "{node_name}"'s responsibilities.** Verify after writing: every child node's responsibilities map back to something its parent is accountable for. If a child's responsibilities don't ladder up, it's vagrant.

## Procedure

1. Call `get_rules` to load the modeling rules.
2. Call `get_node` with id "{node_id}" to see this node's full context (description, responsibilities, sources, existing links).
3. Use `get_structure` with path "{project_path}" if you need to inspect source files. Open relevant files to identify {child_kind_label}.
4. Call `set_node` on "{node_id}" with the new subtree (nodes + links). Add only nodes whose responsibilities ladder up to "{node_name}".
{child_guidance}
5. Set responsibility status to `implemented` on responsibilities derived from existing code; `proposed` on speculative ones.
6. Call `update_source_map` to attach glob/file patterns to nodes that map to code.
7. Call `get_changes` to summarize what was added.

Stay within the "{node_name}" subtree. Do not modify nodes outside this scope.
"#
    )
}
