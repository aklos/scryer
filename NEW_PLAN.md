# NEW_PLAN — Drop the canvas; make scryer a wiki for your architecture

## Premise

The canvas was the wrong primitive. It spent enormous incidental complexity
(pan/zoom, the zoom-scaling discipline, packing, perimeter nodes, group boxes)
to arrange *boxes of text* on a grid — and it never delivered the one thing it
promised, **seeing** the thing. Click a symbol like `GroupOverlay` today and you
get a 540px side panel with a title, a list of responsibilities, and a lazy
one-span code teaser. A box around cards never told you what a node *is*.

The thing we've actually been missing is **observability of a node — especially
a symbol — itself.** Not a summary. The real code, the real status, the real
responsibilities and properties and connections, shown properly, on a page that
belongs to that node.

So: kill the canvas. scryer becomes a **wiki over the C4 model**.

- **Left:** the C4 model as a navigation tree — whole-architecture visibility.
- **Main:** the selected node *is the current page* — the full observability and
  planning surface for that node.

The model on disk (`.scryer/model.scry`) stays the source of truth, unchanged.
This is a **view-layer pivot**, not a schema rewrite.

## Principles (non-negotiable)

1. **scryer never writes code.** It is the model — the planning and
   observability layer over your codebase. Your agent implements.
2. **Editing the page = proposing.** Add a responsibility (or property, or
   directive, or visual change) on a node's page and it lands as `proposed`.
   When you ask your agent, it reads the model and knows what to implement.
3. **Obs and planning are the same surface.** You propose in the same place you
   verify: the page's code visibility + status + drift is how you *see* whether
   the agent built what you specified.

## The shape

### Left — the navigation tree

The `parentId` containment hierarchy, rendered as a tree: system → container →
component → symbol, navigable **all the way down to symbols**. (Today
`handleNavigate` explicitly refuses to descend into `symbol`/`person` — that
rule is exactly what made symbols second-class, and it goes away. Every node is
reachable and has a page.)

Groups appear in the tree as **folders** (see below). Breadcrumbs stay as the
secondary "where am I" affordance.

### Main — the node page

The heart of the build. For **every** node kind, a page that actually shows the
node:

- **Header** — name, kind, technology, status (rolled up from responsibilities).
- **Description** — the prose about what this node is.
- **Responsibilities** — editable in place. Add one → it's `proposed`. Each row
  shows its effective status and its mapped code. This is the planning surface.
- **Properties** — for symbols that declare a data shape; same proposable edit
  model.
- **Code visibility** — *proper*, not a teaser. The node's mapped source shown
  full and line-anchored, reusing the existing `CodeBlock` / `read_source_span`
  machinery the inspector already has — promoted from a cramped panel to a
  first-class region of the page.
- **Connections** — incoming/outgoing links as a panel (the existing `ConnRow`),
  each partner clickable to navigate. This is how the relationship graph
  survives without drawn lines: you walk it page to page.
- **Boundary** — the node's code boundary globs, when mapped.

The data for all of this already exists in the model and is already computed by
`InspectorPanel`. The work is presentation: give it a page's worth of room and
make the code the hero, not a 1-span afterthought.

### Visual component pages (the one genuinely new tool)

If a symbol is a visual component, **render it right there.** A node can be
marked as visual and pointed at its Storybook story; its page embeds the live
story so you see the real component, not a label.

Visual changes are proposed the same way responsibilities are: prompt a change →
get ~3 variations to choose between → pick one with a comment. The variations are
**ephemeral planning renders** (Storybook sandbox); nothing lands in the
codebase. What persists is a **decision artifact** on the node — the chosen
direction (screenshot + comment, optionally a reference sketch) in `proposed`
state. Your agent reads it and implements.

This is the riskiest, least-proven piece (how variations get generated/rendered
without writing code is unvalidated), so it is **spiked and built last**, in
isolation, after the wiki + page + obs surface is working. The durable contract
is the decision artifact; the generation mechanism is the spike.

### Groups — folders with metadata

A group is anchored to one surface (`parentNodeId`) and its members are children
of that same parent, so a group is a **sub-partition of one node's children** —
unambiguous in a tree.

- **In the tree:** an expandable folder under its parent node, wrapping its
  member children. Grouped nodes nest under the folder (each node appears once);
  ungrouped siblings sit directly under the parent. `parentGroupId` nesting is
  just nested folders.
- **As a page:** clicking the folder opens the group's page — finally a real home
  for it. It shows the group's `description`, its **own `responsibilities`**
  (the deployment/package unit's accountabilities — proposable like any node's),
  its member roster (each linking to its page), and **rolled-up observability**
  (aggregate status and drift across members — "is this unit healthy?", which a
  box could never answer).
- A group has no code of its own, so no code panel. (A future extension could
  map a package/deployment unit to a directory boundary; not now.)

## What gets removed

The entire spatial-canvas layer and its supporting machinery — the card
renderers, the pan/zoom container and its scaling discipline, the packing/layout
engine, the perimeter and connection overlays, and the canvas-only derived view
types. `cell`/`size` on nodes and groups become vestigial on-disk fields (left in
place for now; removed in a later schema cleanup).

## What stays and gets reused

- The on-disk schema in full: nodes, links, groups, responsibilities,
  properties, `sourceMap`, `boundaries`, status.
- All model-mutation intents (the `editor` object: update/add/move
  responsibilities, properties, nodes, etc.). These edits now happen on the page
  instead of on cards.
- The code-rendering path (`CodeBlock`, `read_source_span`, syntax tokenizing) —
  promoted into the page.
- `SyncBar`, drift detection, the build flow, the agent session, project
  picking, the `proposed/implemented/verified/vagrant` status model and its
  `lastTouchedAt` stamping (the age signal stays useful as obs even though the
  canvas patina is gone).

## Schema impact

Minimal and additive:

- `cell` / `size` → vestigial (defer removal).
- A way to mark a node as a visual component and reference its Storybook story
  (additive optional field).
- A decision-artifact shape for visual proposals (additive; defined during the
  visual spike).

No changes to the load-bearing truth (nodes/responsibilities/links/groups/
sourceMap/boundaries). The Rust `scryer-core` types and the MCP tools keep
working as-is.

## Order of work

1. **Tree + page shell.** Replace the canvas surface with the left tree (full
   containment, symbols included) + a main pane that routes the selected node to
   a page. Breadcrumbs + SyncBar stay.
2. **The node page — real observability.** Build the page properly: status
   header, description, responsibilities (in-place proposable), properties,
   first-class code visibility, connections, boundary. This is where most of the
   value lands.
3. **Editing = proposing.** Wire in-place page edits to the existing mutation
   intents, defaulting new items to `proposed`; surface status/drift on the page
   so build-vs-spec is visible.
4. **Groups as folders + group page.** Tree folders wrapping members; group page
   with description, own responsibilities, member roster, rolled-up status/drift.
5. **Remove the dead canvas layer.** Delete the spatial machinery once nothing
   routes to it.
6. **Visual component pages (spiked separately).** Storybook embed for visual
   symbols, then the prompt → variations → pick+comment decision artifact.

## Open risks

- **Visual variation generation** — the one real unknown. Resolved by a spike
  before committing to a mechanism; the persisted decision artifact is the stable
  contract regardless of how variations are produced.
- **Whole-system graph** — the canvas's one real loss is the at-a-glance
  cross-cutting graph. The bet is that architecture is verified node-by-node via
  the connections panel + navigation, not by staring at a wire diagram. If that
  bet is wrong, a dedicated relationship view becomes its own page type later.
</content>
