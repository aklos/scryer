# scryer preview sidecar (Track B spike: B2 + B3)

Deterministic, zero-LLM component previews. A Node sidecar with **no
dependencies of its own** — it resolves `vite` and `typescript` out of the
*target project's* `node_modules`, so previews compile with exactly the
project's own toolchain and plugins (the project's `vite.config` is reused).

| file | role |
| --- | --- |
| `props.mjs` | B2 — discovers exported React components in `.tsx` files via the TS compiler API and synthesizes placeholder props from their prop types (literals for scalars, first member of literal unions, no-op callbacks, empty collections, recursive object shapes; optionals omitted except `children`). |
| `plugin.mjs` | B3 — Vite plugin serving `GET /__preview?file=src/Foo.tsx&export=Foo` as a virtual entry: import component, apply synthesized props, auto-import the app's global CSS, render into `#root`. Plus the spike harness endpoints (`/__components.json`, `/__harness`, `/__report`). |
| `server.mjs` | boots the shared dev server: `node preview/server.mjs [projectRoot] [--port N] [--no-wrapper]` |
| `harness.mjs` | measurement: renders every discovered component in headless Firefox and prints per-component status + timing: `node preview/harness.mjs [projectRoot] [--no-wrapper]` |

If `{project}/.scryer/preview/Wrapper.tsx` exists (B4 — the one agent-written
provider fix per project), every entry wraps the component in it. For scryer
itself the wrapper shims `__TAURI_INTERNALS__` so Tauri calls hang in loading
states instead of crashing.

The agent enters only through two repair/creative paths, both served by the
same running server with no build step:

- **Fixtures (B5)** — when a render reports `empty`/`error`, the app offers
  "Generate preview data": an agent writes
  `.scryer/preview/fixtures/{nodeId}.tsx` default-exporting realistic props.
  Entries get `&fixture=<path>` and spread it over the synthesized defaults.
- **Variations (B6)** — the agent writes N self-contained variant modules
  `.scryer/preview/variations/{nodeId}/{i}.tsx` (default export, no props,
  root-absolute imports); each is just another entry
  (`/__preview?file=…/{i}.tsx&export=default`). Accepting one copies it to
  `.scryer/preview/accepted/{nodeId}.tsx` and points the node's appearance at
  it.

A watcher on `.scryer/preview/` invalidates the virtual entries and
full-reloads open previews whenever fixtures, wrappers, or variants change.

The Tauri app embeds these three `.mjs` files at compile time
(`src-tauri/src/lib.rs`, `ensure_preview_server`) and writes them to
`{project}/.scryer/preview/server/` before spawning `node`.

Render verdicts: `ok` (something painted into `#root` or a portal), `empty`
(rendered nothing — often correct behavior with empty placeholder data; the
B5 agent-fixture repair path), `error`, `timeout`. If injected optional
`children` crash a void-element passthrough (`<input>`), the entry retries
once without them.

Spike result on scryer's own frontend (2026-06-11): **34/35 ok (97%)
zero-config**, ~200 ms per render steady-state after a one-time ~8 s cold
start. The one `empty` (`ConnectionsSection`) legitimately renders `null`
with an empty model — fixture territory, not a bug.
