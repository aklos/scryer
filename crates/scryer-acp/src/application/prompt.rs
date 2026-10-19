//! The prompt asking an agent to author realistic fixture data for a
//! component's live preview: shared, type-keyed samples plus per-node
//! overrides, so interconnected domain data stays consistent.

/// Prompt for rendering a component preview. The agent reads the
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
    // A Vue single-file component takes its fixture as a plain `.ts` props
    // module — there is no JSX to build, and the shared module is `.ts` too.
    let is_vue = source_file.ends_with(".vue");
    let (fixture_ext, shared_module) = if is_vue { ("ts", "shared.ts") } else { ("tsx", "shared.tsx") };
    let vue_note = if is_vue {
        "\nThis is a Vue single-file component: its props are the `defineProps` / `props` declaration in its `<script>` block, and the server fills REQUIRED props from that declaration at mount time. Fixture modules for it are plain TypeScript (`.ts`) — export data objects, never JSX.\n"
    } else {
        ""
    };

    format!(
        r#"The live preview of the component "{node_name}" (id {node_id}) in the project at {project_path} is rendered by a dev server that synthesizes placeholder props from the component's TypeScript types.{vue_note} With those placeholders the render came out **{render_status}** — the preview needs realistic data instead.
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
    "ScryModel": {{ "module": "{shared_module}", "export": "sampleModel" }},
    "Node": {{ "module": "{shared_module}", "export": "sampleNode", "sourceFile": "src/viewmodel.ts" }}
  }}
}}
```

`module` is a file under `.scryer/preview/fixtures/`. `export` is a named export in it. `sourceFile` is OPTIONAL — add it only to disambiguate a common type name (e.g. `Node`) by requiring the type's declaration to live in that file, so a same-named type in some library never matches.

## Your task

1. **Read the component's prop types** (follow the imports in the source above) to see which types its props are.
2. **Read the existing `.scryer/preview/fixtures/manifest.json` and `{shared_module}` if they exist** — you are EXTENDING them, not overwriting. Reuse any type already covered; never duplicate an entry.
3. For each domain type this component needs that is NOT already in the manifest, **add a named sample export** to `{shared_module}` and a `byType` entry to the manifest. Prefer extending `{shared_module}`; these samples are shared across the whole project.
4. **Make the samples mutually consistent**: ids that resolve, collection members and link/edge endpoints reference ids that exist in the sample, and any pointer-typed prop (a `selected`/`activeId`-style string) names a real element in the sample so the component renders a populated state, not an empty/not-found one.
5. **Realistic data, not placeholders.** Lists get 3–5 plausible items; names/labels/timestamps look real. The goal is a preview that shows the component doing its job.
6. **Seed controlled state to a POPULATED snapshot.** If the component's expand/select/open/active state is driven by props (an `expanded`/`openIds` set, a `selected` id, an `isOpen` flag) with companion callbacks (`onToggle`, `onSelect`), the synthesized empty set / false flag renders it collapsed-and-blank — functionally useless as a preview, even though it "rendered". The callbacks stay no-ops (the preview is a snapshot, not interactive), so the ONLY way the component shows its structure is to seed those state props open: an `expanded` set containing the sample's ids, a real `selected`, flags set so panels are visible. These props are usually generic-typed (`Set<string>`, `boolean`), so seed them in the per-node override below rather than the shared manifest.

### Per-node override (fallback + controlled state)

Write `.scryer/preview/fixtures/{node_id}.{fixture_ext}` (DEFAULT EXPORT = a partial props object for "{node_name}") when this component needs props the shared samples can't express — a particular selection, a special-case shape, or **controlled-state props seeded open per point 6**. The server spreads it OVER the shared/synthesized props, so include only the keys you override, and import the shared samples to keep ids consistent (e.g. `import {{ sampleModel }} from "/.scryer/preview/fixtures/{shared_module}"` then `expanded: new Set(sampleModel.nodes.map((n) => n.id))`). Keep domain DATA in the shared fixtures; use the override for this component's view state.

### Rules

- Import types or helpers from the project by root-absolute path (e.g. `import type {{ Node }} from "/src/viewmodel"`). Never use relative imports — fixtures live outside `src/`.
- Function props may be no-ops (`() => {{}}`).
- Do NOT modify any project source files. The only files you write are under `.scryer/preview/fixtures/`.
- You do not run, build, or render anything — the preview server picks up the files automatically.
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A Vue single-file component's fixture is a plain `.ts` props module,
    /// shared samples live in `shared.ts`, and the prompt says so; a React
    /// component keeps the `.tsx` contract untouched.
    #[test]
    fn fixture_prompt_asks_vue_components_for_plain_ts_modules() {
        let vue = preview_fixture_prompt("/p", "node-1", "UserCard", "src/components/user-card.vue", "", "empty", "");
        assert!(vue.contains(".scryer/preview/fixtures/node-1.ts`"), "vue fixture is .ts");
        assert!(vue.contains("shared.ts\""), "shared module is .ts");
        assert!(vue.contains("Vue single-file component"));
        assert!(!vue.contains("node-1.tsx"));

        let react = preview_fixture_prompt("/p", "node-2", "Card", "src/Card.tsx", "", "empty", "");
        assert!(react.contains(".scryer/preview/fixtures/node-2.tsx`"));
        assert!(react.contains("shared.tsx"));
        assert!(!react.contains("Vue single-file component"));
    }
}
