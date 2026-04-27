use scryer_core::drift::DriftedNode;
use scryer_core::C4ModelData;

/// Serialize a model for embedding in the sync prompt.
/// Strips UI-only fields (position, type, refPositions) and compact-strips
/// empty values — same as what `get_model` returns to agents.
pub fn serialize_model_for_prompt(model: &C4ModelData) -> String {
    let mut val = serde_json::to_value(model).unwrap();
    strip_compact(&mut val);
    serde_json::to_string(&val).unwrap()
}

fn strip_compact(val: &mut serde_json::Value) {
    match val {
        serde_json::Value::Object(map) => {
            map.remove("position");
            map.remove("type");
            map.remove("refPositions");
            map.remove("notes");
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

/// Build a prompt for initial model creation from a codebase.
/// Guides the agent to scan the project and build system + container levels only.
pub fn initial_model_prompt(model_name: &str, cwd: &str) -> String {
    format!(
        r#"You have access to the scryer MCP server. Build a C4 architecture model named "{model_name}" from the codebase at {cwd}.

## Instructions

1. Call `get_rules` to load the full modeling workflow and C4 rules.
2. Call `get_structure` with path "{cwd}" to get the annotated directory tree.
3. Read the manifests that `get_structure` surfaces (package.json, Cargo.toml, go.mod, etc.) to identify runtime dependencies, external services, databases, and frameworks.
4. Build the model level by level — follow the workflow from `get_rules`:
   - First: `set_model` with persons, the system, external systems, and system-level edges only. Fix any warnings before proceeding.
   - Second: `set_node` on the system to add all containers plus container-level edges (Person→Container, Container→Container, Container→ExternalSystem). Fix any warnings.
   - Group containers that deploy together using `set_groups`.
5. **Stop at the container level.** Do NOT add components or operations.
6. Set `status: "implemented"` on all nodes that already exist in the codebase. Do NOT use "verified" — that requires contract items to be checked and passed.
7. Set source mappings for containers using `update_source_map` — use glob patterns pointing to each container's directory.
8. Call `get_changes` to produce a summary of what was modeled.

Be thorough — identify all deployable units, data stores, external integrations, background workers, and user-facing surfaces. Model for production, not for demos. Name nodes by their role, not their technology."#
    )
}

/// Build a prompt for filling out the children of a specific node from existing code.
/// The agent reads the codebase and populates the next C4 level down.
pub fn node_fill_prompt(
    model_name: &str,
    cwd: &str,
    node_id: &str,
    node_name: &str,
    node_kind: &str,
    model_json: &str,
) -> String {
    let child_kind = match node_kind {
        "system" => "containers (applications, services, data stores)",
        "container" => "components (logical modules within the container)",
        "component" => "operations (functions/methods), processes (multi-step flows), and models (data types)",
        _ => "child nodes",
    };

    let extra_instructions = match node_kind {
        "system" => r#"
   - Add all containers: APIs, web apps, workers, databases, message queues, caches.
   - Set `technology` on every container (e.g. "Next.js", "PostgreSQL", "Redis").
   - Add edges between containers showing data flow and dependencies.
   - Group containers that deploy together using `set_groups`."#,
        "container" => r#"
   - Identify logical components by reading the source directories mapped to this container.
   - Components should represent cohesive modules — not one per file, but logical groupings.
   - Add edges between components showing internal dependencies.
   - Set `technology` where relevant (framework, library)."#,
        "component" => r#"
   - Read the source files for this component to identify functions, methods, and data types.
   - Operations = individual functions or handlers (name must be a valid identifier, match the language convention).
   - Processes = multi-step workflows that orchestrate multiple operations.
   - Models = data types with properties (name should be PascalCase).
   - Add descriptions explaining what each operation/process/model does."#,
        _ => "",
    };

    format!(
        r#"You have access to the scryer MCP server. Fill out the internals of the "{node_name}" node in model "{model_name}" from the codebase at {cwd}.

## Current model state

Do NOT call `get_model` — the current state is provided here:

{model_json}

## Instructions

1. Call `get_rules` to load the C4 modeling rules.
2. Call `get_node` with id "{node_id}" to see this node's full context (description, contract, source mappings, existing edges).
3. Use `get_structure` with path "{cwd}" and read relevant source files to understand what {child_kind} belong inside "{node_name}".
4. Add {child_kind} using `set_node` on "{node_id}" — include both child nodes and edges between them.{extra_instructions}
5. Set `status: "implemented"` on nodes that already exist in the codebase. Leave new/proposed items as `status: "proposed"`.
6. Update source mappings for new nodes using `update_source_map` with glob patterns.
7. Call `get_changes` to produce a summary.

Focus only on "{node_name}" — do not modify nodes outside this scope. Be thorough — identify all {child_kind} from the actual code, not just the obvious ones."#
    )
}

/// Build a focused sync prompt listing nodes whose source files changed.
/// Includes the full model JSON so the agent can skip calling `get_model`.
pub fn sync_prompt(
    model_name: &str,
    cwd: &str,
    drifted: &[DriftedNode],
    structure_changed: bool,
    model_json: &str,
) -> String {
    let mut drift_list = String::new();
    for d in drifted {
        drift_list.push_str(&format!(
            "- **{}** ({}): changed files matching: {}\n",
            d.node_name,
            d.node_id,
            d.patterns.join(", ")
        ));
    }

    let structure_section = if structure_changed {
        "\n## Project structure changes\n\nNew or deleted files were detected in the project since the last sync. Call `get_structure` to see the current project layout, then check whether any new code needs to be added to the model or any removed code should be cleaned up.\n"
    } else {
        ""
    };

    format!(
        r#"You have access to the scryer MCP server. The architecture model "{model_name}" may be out of sync with the codebase at {cwd}.

## Potentially drifted nodes

The following nodes have source files that were modified since the model was last updated. The code may or may not have changed in ways that affect the model — check each one.

{drift_list}{structure_section}
## Current model state

Do NOT call `get_model` — the current state is provided here:

{model_json}

## Instructions

1. For each drifted node above, read the changed source files to understand what (if anything) changed.
2. Update the model only where the code has actually diverged:
   - Fix descriptions, technology labels, or status with `update_nodes`
   - Add new structures with `add_nodes` (status "wip") if the code introduced something the model doesn't cover — these are existing code being added to the model, not proposals
   - Remove nodes with `delete_nodes` if the code deleted what the model still shows
   - Add or remove edges with `add_edges`/`delete_edges` if relationships changed
3. Call `get_changes` to produce a summary.

Do NOT call `get_model` — the model state is already above. Do NOT call `get_rules` unless you need to create entirely new architectural structures.

Be conservative — only change what actually diverged. If nothing needs updating, say so. Report what you changed and why.

## Off limits

Do NOT do any of the following — these require verification from the user:
- Do not change contract expect item `passed` flags
- Do not change node status from "wip" to "ready"
- Do not call `get_task` or start implementing code"#
    )
}
