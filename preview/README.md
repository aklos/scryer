# scryer preview sidecar (Track B spike: B2 + B3)

Deterministic, zero-LLM component previews. A Node sidecar with **no
dependencies of its own** — it resolves `vite` and `typescript` out of the
*target project's* `node_modules`, so previews compile with exactly the
project's own toolchain and plugins (the project's `vite.config` is reused).

| file | role |
| --- | --- |
| `props.mjs` | B2 — discovers exported React components in `.tsx` files via the TS compiler API and synthesizes placeholder props from their prop types (literals for scalars, first member of literal unions, no-op callbacks, empty collections, recursive object shapes; optionals omitted except `children`). Also lists every `.vue` single-file component (when the package depends on `vue`) as a default export named after its file, tagged `framework: "vue"` — its props are synthesized at mount time from the compiled component's `props` definition instead. |
| `plugin.mjs` | B3 — Vite plugin serving `GET /__preview?file=src/Foo.tsx&export=Foo` as a virtual entry: import component, apply synthesized props, auto-import the app's global CSS, render into `#root`. The entry is generated per framework — React (`createRoot`) or Vue (`createApp`, required props filled from `Component.props`, default slot filled) — sharing the render-verdict and fixture contract. Plus `/__components.json` — discovered components + synthesized props. |
| `server.mjs` | boots the shared dev server: `node preview/server.mjs [projectRoot] [--port N] [--no-wrapper]` |

If `{project}/.scryer/preview/Wrapper.tsx` exists (B4 — the one agent-written
provider fix per project), every entry wraps the component in it. For scryer
itself the wrapper shims `__TAURI_INTERNALS__` so Tauri calls hang in loading
states instead of crashing.

## A project's own preview config

`{project}/.scryer/preview/vite.config.*`, when present, is loaded **instead of
the package's `vite.config.*`** (`previewConfigFile`). Reuse of the project's
real config is the default because it is usually right, but not always: a
framework plugin can own the whole app (remix, next) in a way that has nothing
to do with rendering one component in isolation, and a monorepo root that
resolves `vite` while its vite app lives in a sub-package has no usable config
at that root at all. Such a project states just what previews need — path
aliases, the css pipeline — next to its fixtures and wrapper, instead of
carrying a preview-only file in its repo root. One preview config serves every
package server.

Dependency-optimizer caches live in `.scryer/preview/.vite/<package-slot>`
(`previewCacheDir`), never the package's own `node_modules/.vite`: the project
normally has its own dev server on the same root with a different config, and
one shared cache means each server re-optimizes on the other's heels until the
pages already open against the project's server fail with "504 Outdated
Optimize Dep". Preview machinery is dot-prefixed inside `.scryer/preview/` so
the content watcher (fixtures, wrapper, variations) ignores it.

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
