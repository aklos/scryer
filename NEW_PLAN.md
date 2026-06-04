# NEW_PLAN — scryer becomes a planning surface, not a visualization tool

## The problem

Two problems, actually.

**"Visualize your codebase" is useless.** Nobody needs boxes on a grid. The
canvas ate months on layout algorithms, zoom scaling, collision detection, and
drag-and-drop — all to render text inside rectangles that told you nothing you
didn't already know.

**Prose descriptions of code are useless for planning.** A responsibility like
"renders a thing that nodes go into" is strictly less useful than both the source
code AND the rendered output. You can't plan against it because it doesn't show
you the thing you're actually trying to change. You look at GroupOverlay in the
model, you see a card with two vague sentences. You look at GroupOverlay in the
app and you see it looks like shit — but you can't describe how to fix it in
words, and even if you could, the description would be too lossy for an agent to
act on precisely.

## The core insight

**The right representation for planning depends on what you're looking at.**

- For a **handler or service** — a list of responsibilities IS the right
  abstraction. "Authenticates the user, updates the record, returns X." You can
  read that, decide what to change, add implementation directives ("auth using
  JWTs"), and see the exact source lines each responsibility maps to. That's a
  useful planning surface.
- For a **visual component** — responsibilities are useless for planning. You
  need to **see the rendered thing.** The planning primitive is the live preview,
  and changes are planned visually (prompt → variations → pick), not through
  prose.
- For a **data type** — the properties list (type fields, their shapes) is the
  right representation. Responsibilities don't make sense for a type.

scryer's node page adapts its primary representation based on what the node
actually is. Source code is always visible as reference material, but it's not
the planning primitive — the semantic representation is.

## What scryer becomes

A planning and reconciliation surface. Two panels:

### Left panel — the model tree (definition surface)

The C4 hierarchy rendered as an IDE-style tree explorer. This is where you
**define** the model — create systems, containers, components, symbols, groups.
Add, rename, move, delete, reorganize. Same role a file tree plays in a code
editor: the structural definition of what exists and where it lives.

Every node is reachable and first-class. No more `handleNavigate` refusing to
descend into symbols. Symbols are the things you actually want to look at.

Groups appear as folders wrapping their members. A group is just a folder — the
tree represents nesting natively, no spatial enclosures needed.

Node status (rolled up from responsibilities) is reflected directly in the tree
as colored dots/icons, giving an at-a-glance health view of the whole model
without opening any pages.

### Main panel — node pages (adaptive representation)

Click a node in the tree → its page opens. Wikipedia-style layout:

- **Page header** — node name, kind badge, rolled-up status. Page-level action
  tabs/controls along the top: trigger render (visual components), run drift
  check, start agent fill, view change history.
- **Main content area** (center) — the adaptive representation. Scrollable,
  section-based. Each section header has an `[edit]` link — click it and that
  section swaps to edit mode in place (no modals, no separate forms, you edit
  where you read). Adding/editing anything defaults to `proposed`.
- **Infobox** (right sidebar) — structured at-a-glance metadata: technology,
  status, connections as wiki links, boundary globs. Always visible, not mixed
  into the main content flow.

The main content area **leads with whatever representation actually lets you
understand and plan changes to this specific kind of thing.**

#### Handlers, services, process logic → responsibilities

Responsibilities are the hero. Each one is a contract item, editable in place.
Add one → it lands as `proposed`. Each shows:

- Its status (proposed / implemented / verified / vagrant).
- **The exact source lines it maps to** — the specific lines this responsibility
  touches, rendered with syntax highlighting. Not a file-head teaser. If a
  responsibility says "validate input schema" you see the validation function.
- Implementation directives — how the agent should implement or change this.
  The responsibility says *what*, the directive says *how* ("use JSON web
  tokens"), the agent reads both.

This is the reconciliation surface for backend/process work: you propose, the
agent implements, you verify on the same page by checking status + reading the
mapped code.

#### Visual components → live rendered preview

The live rendered component is the hero — the actual running component embedded
on the page, interactive (hover, click, resize, test states). Not a screenshot,
not a description.

**Rendering mechanism:** the agent (via MCP/ACP) generates a render harness for
the component — entry point, provider wrapping, fixture props — inside
`.scryer/preview/` in the target project. A minimal Vite config there resolves
dependencies from the target project's own `node_modules`. The agent runs the
build into `.scryer/preview/dist/`, and scryer loads the output in an iframe on
the node page. All preview artifacts stay scoped to `.scryer/`, gitignored. The
agent handles everything that requires understanding the codebase (dependency
resolution, context providers, sensible props); scryer just loads and displays.

During model generation, the agent flags visual components with a `visual`
marker on the node. Rendering is **user-initiated**: you open a visual symbol's
page and trigger the render yourself. The visual representation persists in the
model so it's there next time you open the page.

**Planning visual changes:**

1. **Prompt** — describe what you want ("make the header sticky, reduce padding,
   swap the icon set").
2. **Receive variations** — ~3 live rendered alternatives. Ephemeral sandbox
   renders — interactive, not screenshots.
3. **Iterate** — pick one, keep prompting refinements. This is all ephemeral,
   just a back-and-forth in the sandbox.
4. **Accept** — once you're happy with the full change set, it writes to the
   model: the node's visual representation updates and the node gets marked
   proposed/changed. One write, at the end.

Responsibilities may still exist on visual components ("supports drag resize,"
"renders group boundary") but they're secondary — useful for tracking behavioral
contracts, not for planning visual changes.

This is the riskiest piece (the rendering + variation mechanism is unvalidated),
so it's spiked and built last.

#### Data types → properties

The properties list is the hero — type fields, their shapes, their status. Same
proposable edit model as responsibilities. Responsibilities don't apply here.

### Common page elements (all node kinds)

- **Header** — name, kind, technology, rolled-up status.
- **Description** — what this node is.
- **Connections** — incoming/outgoing links as wiki-style links. Click →
  navigate to that node's page.
- **Source code** — always visible as reference. The mapped source rendered with
  syntax highlighting. Engineers need to see the code, but it's reference
  material, not the planning primitive.
- **Boundary** — code boundary globs, when mapped.

### Groups — folders with their own pages

A group is just a folder under its parent, wrapping its member nodes. The tree
represents nesting natively — no spatial enclosures, no collision detection, no
resize handles.

- **In the tree:** an expandable folder. Create it, drag members into it.
- **As a page:** the group's own description, its responsibilities (proposable
  like any node's), member roster (each linking to its page), and rolled-up
  status across members.

## What gets deleted

The entire spatial layer: Surface, PackBox, PanZoom, EntryCard grid rendering,
ConnectionsOverlay, PerimeterNode, pack.ts (collision/layout), gridcontext,
dndTransform, the drag-and-drop system, the zoom-scaling discipline, the
auto-layout engine. ~5,000 lines of code that existed to arrange rectangles.

`cell`/`size` on nodes and groups become vestigial on-disk fields (cleaned up in
a later schema pass).

## What stays

- The on-disk schema: nodes, links, groups, responsibilities, properties,
  sourceMap, boundaries, status.
- All model-mutation intents (the `editor` object). Edits happen on pages
  instead of cards.
- The code-rendering path (CodeBlock, read_source_span, syntax tokenizing) —
  promoted from a cramped panel to a first-class region of every page.
- SyncBar, drift detection, build flow, agent session, project picker.
- The proposed/implemented/verified/vagrant status model and lastTouchedAt
  stamping.
- Model generation via MCP/ACP — unchanged. The agent generates the same model
  (responsibilities, source mapping, properties, links), just without spatial
  positioning. It additionally flags visual components with a `visual` marker.

## Schema changes

Minimal and additive:

- `cell` / `size` → vestigial (defer removal).
- Implementation directives field on responsibilities (additive).
- `visual` flag on nodes — marks visual components (set by agent during model
  generation, used by the page to determine representation).
- Visual representation field on nodes — persists the rendered state (written
  on user-initiated render or after accepting visual changes).

No changes to the load-bearing schema. Rust types and MCP tools keep working.

## Build order

1. **Tree + page shell.** Replace the canvas with the left tree (full hierarchy,
   symbols included, status dots) + a main pane routing the selected node to a
   page. Breadcrumbs + SyncBar stay.
2. **Node pages — adaptive representation.** Build the page framework with the
   common elements (header, description, connections, source, boundary). Then
   the kind-specific heroes: responsibilities for handlers/services, properties
   for data types. Each responsibility shows its exact mapped source lines.
3. **Editing as contracts.** Wire in-place edits to existing mutation intents.
   Add implementation directives field. New items default to `proposed`. Status +
   drift visible on the page.
4. **Groups as folders + group pages.** Tree folders, group page with
   description, own responsibilities, member roster, rolled-up status.
5. **Delete the canvas.** Remove the spatial layer once nothing routes to it.
6. **Visual component rendering (spiked separately).** Agent generates render
   harnesses + minimal Vite config in `.scryer/preview/`, builds into dist,
   scryer loads in iframe. User-initiated on visual symbol pages.
7. **Visual change planning.** The prompt → variations → iterate → accept
   workflow. This is where scryer becomes genuinely better than prompting for
   UI work.

## Graph view (secondary, deferred)

If wiki-style connection links aren't enough for navigating relationships, add a
graph view as a secondary panel — not the primary surface, just an optional way
to see and click through the connection topology.

The legacy codebase on `main` has working ReactFlow graph rendering that can be
pulled in if needed. This is a "pull from the shelf" option, not a build
commitment.

## Risks

- **Visual component rendering** — the agent generates harnesses in
  `.scryer/preview/` and builds via Vite against the target project's
  `node_modules`. The agent-side is the hard part (correct provider wrapping,
  fixture props, dependency resolution). Spike before committing.
- **Graph view might never be needed** — the legacy rendering code is there if
  wiki linking falls short, but it might not. Don't build it preemptively.
- **Determining node kind automatically** — the agent flags visual components
  during model generation (inferrable from the code). For higher-level nodes
  the page representation is determined by what content the node has
  (responsibilities → responsibility view, properties → property view, visual
  flag → rendered preview).
