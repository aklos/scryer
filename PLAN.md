# PLAN — fast semantic model builds + automagic UI previews

Two tracks. A: cut codebase→model wall-clock from minutes to ~60–90s without
giving up the agent-led semantic layer. B: render UI components deterministically
by default, with the LLM demoted to a repair path.

## Grounding (what we tested and read)

- **Deterministic component clustering is a dead end as the final answer.**
  Louvain on our file dependency graph scored pairwise F1 ≈ 0.28 against the
  agent-built model of this repo (experiment: `crates/scryer-extract/examples/cluster.rs`).
  The literature says that's the expected ceiling: pure dependency-graph
  clustering is the *worst* family in architecture-recovery research (Garcia et
  al. ASE 2013: best fully-automatic techniques average MoJoFM ~55–60%; modularity
  methods bottom out at 18–33%). Nothing shipping does better than
  directories/namespaces; Structurizr's docs call generic automatic component
  discovery infeasible.
- **The agent's component layer is bimodal**: 1-file-1-component (named) in
  small-file crates; sub-file splits in big files (src-tauri lib.rs → 7
  components). File-level clustering can't match either mode. Component
  decomposition stays LLM judgment.
- **Scaffold-then-generate beats generate-from-scratch, measurably.** ArchAgent
  (arXiv 2601.13007): deterministic dependency context in the prompt is worth
  +0.07–0.11 F1, and with full scaffolding the model choice stops mattering.
  Their context strategy — token-bounded DFS partition of the dependency graph
  with ~10% overlap — is directly reusable (A3). The 340-repo study (arXiv
  2603.21178) found free-roaming general-purpose agents were the *worst*
  configuration; tightly-constrained pipelines win — which is what our
  intent-tool + `fill_container` bottleneck already is.
- **Zero-fixture component rendering is proven** by Preview.js (archived
  2026-03): TS-compiler prop synthesis + one shared Vite dev server + virtual
  entry modules per component. Its prop-inference package
  (`@previewjs/type-analyzer`) is MIT and salvageable; the rendering core is
  AGPL and must be reimplemented (it's small). Nobody ever solved automatic
  provider wrapping — the proven pattern is one wrapper per project.
- **Constraints**: LLM access is agent-CLI only (no raw API fan-out — every LLM
  interaction is a full agent-loop turn, so the levers are fewer turns and more
  parallel sessions). The model is C4 by construction (closed `kind` enum,
  validated links, the commit-tool bottleneck) — none of the changes below
  touch that.
- **Non-determinism of naming/organization** is handled by lifecycle, not
  generation: the first build is the last full build. After that, drift
  reconcile edits the existing model incrementally, so names and grouping are
  sticky by construction. Denser scaffolds also empirically reduce run-to-run
  variance.

## Track A — codebase→model speed

Current anatomy: Wave 1 (full agent session) blocks everything; Wave 2 sessions
spend most turns reading source files; pool capped at 4 with big jobs eating
multiple permits; the biggest container is the long pole.

### A1. Embed the code evidence in the Wave 2 payload
Extend `compact_scope` so each symbol carries an excerpt: signature + doc
comment + body, truncated past a size cap. The Wave 2 session becomes
think → one `fill_container` call (~2 turns instead of 5–15).
Touches: `scryer-extract` (excerpt extraction), `prompt.rs` (payload format),
payload-size guard. **Biggest single win.**

### A2. Remove the Wave 1 → Wave 2 barrier
Mint container nodes mechanically at t=0 from manifest facts (name, technology,
boundary dir — all already extracted). Launch the system-level semantic session
AND every container session concurrently; the system session patches in
persons/externals/system responsibilities and refines container names while
container jobs run. Wall clock: `wave1 + slowest round` → `max(single session)`.
Touches: `start_model_build` orchestration (src-tauri), small deterministic
container-minting function.

### A3. Split oversized containers into chunked jobs
Token-bounded DFS partition over the file dependency graph, ~10% overlap
(ArchAgent recipe). Each chunk gets its own session. Requires an append mode on
`fill_container` (or per-chunk commits + one cheap merge turn for
naming/altitude). Kills the long-pole container.
Touches: partitioner in `scryer-extract`, commit semantics in `generation.rs`,
orchestrator.

### A4. Raise and expose the concurrency cap
The hardcoded clamp of 4 becomes a setting; sessions are network-bound, so test
8. Keep permits-per-big-job for memory safety.

### A5. Per-symbol semantic cache
Content-hash each symbol; cache responsibilities/properties keyed by hash.
Rebuilds and reconciles only re-pay for changed symbols. First build is the only
expensive one, ever.

Expected combined effect: several minutes → ~60–90s for a scryer-sized repo,
full semantic model, same C4 enforcement. The semantic floor with CLI-only
access is the latency of one agent completion over the largest chunk — nothing
makes that sub-10s; everything above removes the waste around it.

## Track B — automagic UI previews

Replaces NEW_PLAN.md's per-component harness + per-component `vite build`
(that's the slow shape). Deterministic render is the default; the LLM is the
repair path; nothing is built per component.

### B1. Deterministic visual detection
Done — tree-sitter already flags JSX-returning symbols. No agent involvement.

### B2. Deterministic prop synthesis
Node sidecar wrapping `@previewjs/type-analyzer` (MIT) or
`react-docgen-typescript`: read the component's TS prop types, emit placeholder
values for required props, no-op callbacks for function props, omit optionals.
Zero LLM.

### B3. One shared Vite dev server per project
Lives in `.scryer/preview/`, resolves against the target project's own
node_modules. A small virtual-module plugin serves
`/?component=src/Infobox.tsx:Infobox` as a generated entry: import component,
apply synthesized props, auto-import the project's global CSS, render into
`#root`. Node-page iframe points at it. One-time start ~1–2s, then any
component renders in tens-to-hundreds of ms.

### B4. One project wrapper as the only provider fix
When a render fails for missing context (router/store/theme), the agent writes
a single `Wrapper.tsx` for the whole project — once, cached.

### B5. Agent as repair path only
Per-component agent-written fixtures (realistic data, mocks) only when the
deterministic render fails or the user wants realistic states. Cached per
component, invalidated by the same content-hash as A5.

### B6. N-variation generator rides the same server
Variations are new code — irreducibly agent work. But the agent writes N small
entry modules in ONE turn; each loads instantly as another virtual entry on the
already-running server (no builds). Refinement = further agent turns + HMR.
Accept = persist the chosen change, as NEW_PLAN.md specifies. Loop latency
collapses to one agent turn per round + instant renders.

Scope honesty: proven core is React+TS. Plain-JS React degrades to empty-props
or agent fixtures (no types to synthesize from). Vue/Svelte are known-possible
but each needs its own framework plugin. Projects need resolvable npm deps;
exotic webpack-only loaders break individual components. Server-rendered
template stacks (ERB/Django/PHP/HTMX) are out of scope.

## Build order

1. **B-spike first** (self-contained, de-risks the whole visual track):
   B2 + B3 against scryer's own frontend, zero LLM. Success metric: how many of
   the ~30 visual components render. That number sizes the repair path.
2. **A1** (evidence embedding) — independent, measurable on this repo
   (turns-per-session before/after).
3. **A2** (barrier removal).
4. **A4** (pool setting) — trivial, piggybacks on A2 testing.
5. **A3** (chunking) — only if big-container long-poles still dominate after
   A1/A2; needs the commit-append design.
6. **A5** (semantic cache) — alongside or after A3.
7. **B4–B6** — wrapper, repair path, variations on the shared server.

Each step is a checkpoint: measure, report, decide before the next.

## Open questions

- Pool cap: is 4 grounded in actual rate-limit pain, or untested caution?
  (Decides how hard to push A4.)
- Target number: what first-build wall-clock counts as success — <60s? ~90s?
- A3 commit semantics: append mode on `fill_container` vs per-chunk
  commits + merge turn (pick when we get there, after A1/A2 measurements).
