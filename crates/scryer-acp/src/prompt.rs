use scryer_core::drift::DriftedNode;

/// Build a focused sync prompt listing nodes whose source files changed.
pub fn sync_prompt(model_name: &str, cwd: &str, drifted: &[DriftedNode], structure_changed: bool) -> String {
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
## Instructions

1. Call `get_rules` to understand the C4 modeling rules.
2. Call `get_model` with name "{model_name}" to see the full model state.
3. For each node above, read the changed source files to understand what (if anything) changed.
4. Update the model only where the code has actually diverged:
   - Fix descriptions, technology labels, or status with `update_nodes`
   - Add new structures with `add_nodes` (status "wip") if the code introduced something the model doesn't cover — these are existing code being added to the model, not proposals
   - Remove nodes with `delete_nodes` if the code deleted what the model still shows
   - Add or remove edges with `add_edges`/`delete_edges` if relationships changed
5. Call `get_changes` to produce a summary.

Be conservative — only change what actually diverged. If nothing needs updating, say so. Report what you changed and why.

## Off limits

Do NOT do any of the following — these require verification from the user:
- Do not change contract expect item `passed` flags
- Do not change node status from "wip" to "ready"
- Do not call `get_task` or start implementing code"#
    )
}
