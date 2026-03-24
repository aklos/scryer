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
