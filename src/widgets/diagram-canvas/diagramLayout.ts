/**
 * Diagram layout adapter — turns the v0.3 model into a positioned, renderable
 * scene for the secondary Diagram view. All layout math lives here so the
 * renderer (DiagramView) stays a pure projection of a `DiagramScene`.
 *
 * One "scene" is one level: the children of a focus node (or the top-level
 * nodes when focus is null), the links lifted to that level, and a position
 * per node. The mode is chosen by the children's altitude:
 *   - architecture tiers (system/container) → the C4 arrangement
 *     (`c4Layout`): users above, the level inside its boundary, what it
 *     uses below.
 *   - the component level of a styled container → the style's own drawing
 *     (`styledLayout`): rows, columns, rings or Cockburn's hexagon, with each
 *     card fixed to its layer's band and only the order within it free.
 *   - the code tier (symbols) → a force-directed dot graph, where boxes-and-
 *     lines would be noise and relationships read better as a constellation.
 *
 * A level with components is drawn at their altitude, as the tree reads it:
 * the root module's own symbols beside them (a crate's `main`, a constant in
 * `lib.rs`) are code, listed under the node in the tree, never drawn as cards.
 */

import { c4Layout } from "./c4Layout";
import type { ScryModel, Kind } from "../../entities/model/viewmodel";
import { isDataShape } from "../../entities/model/viewmodel";
import { styleTable, type ModelHealthReport } from "../../entities/model/health";
import { governingStyleDef, layerOf, type Drawing, type StyleDef } from "../../entities/model/styles";
import { classifyStyledEdge, toolingStrip, styledLayout, type LayerRegion } from "./styled";
import { depthLayout } from "./depth";

export type DiagramMode = "arch" | "styled" | "code";

/** How a code-tier symbol reads at a glance — the three things a symbol can be
 *  (mirrors `kindIcon`/`typeTag`): a data shape (`model`), a component the
 *  preview sidecar can render (`visual`, derived — never stored on the node),
 *  or plain `code`. Picks the symbol's glyph. */
export type SymbolClass = "code" | "model" | "visual";

export interface DiagramNode {
  id: string;
  kind: Kind;
  name: string;
  external: boolean;
  /** Manually placed: the position below is the user's stored placement, not
   *  auto-layout's. Pinned nodes stay put; layout only places the rest. */
  pinned: boolean;
  /** Drawn in the column beside the level's rows (an external system it
   *  calls): its links run sideways, not down the rows. */
  aside?: boolean;
  technology?: string;
  description?: string;
  /** Has its own children — so the renderer can mark it drillable. */
  hasChildren: boolean;
  /** Its direct children — what a drill into it shows; the card's foot
   *  counts them by state. */
  childIds: string[];
  /** Fan-in at this level (how many symbols depend on it) — encodes centrality. */
  degree: number;
  /** Rendered dot diameter (px) on the code tier, sized relative to the graph's
   *  busiest hub so the most depended-upon symbol reads largest. */
  dotSize: number;
  /** A ghost: a node referenced from this level but living outside it. Rendered
   *  hollow; double-click navigates to where it actually lives. */
  reference: boolean;
  /** Code-tier classification — drives the muted class line under the dot. */
  symbolClass: SymbolClass;
  /** The architectural style this node declares (containers; a component
   *  overriding its container's). Drives the miniature glyph on the card. */
  style?: string;
  /** The layer this node plays in its governing style (components; symbols
   *  inherit). Drives the card's layer tag and the styled layout. */
  layer?: string;
  /** Carries responsibilities or a data shape of its own. A symbol without
   *  any reads hollow on the code tier — a hub nobody has described. */
  hasClaims: boolean;
  x: number;
  y: number;
}

export interface DiagramEdge {
  id: string;
  source: string;
  target: string;
  label: string;
  method?: string;
  /** What the link is, when declared (styled containers). */
  kind?: string;
  /** Styled mode: the drawing already says this (a step into the innermost
   *  layer, an adapter onto its port) — hidden until one end is selected. */
  implied: boolean;
  /** Styled mode: both ends sit on one ring or band, so the straight chord
   *  would cut through what lies between — bow it away from `(cx, cy)`, the
   *  curve's middle displaced `offset` px from the chord's midpoint. */
  bow?: { cx: number; cy: number; offset: number };
  /** Box tiers: a link spanning rows runs through these points, down a lane
   *  between the cards it passes. */
  waypoints?: { x: number; y: number }[];
  /** Styled mode: every structural violation charged on this pair — drawn
   *  red, the reasons listed when the user clicks the edge. */
  violations?: string[];
}

/** The word for each box kind, as the boundary label names the level. */
const KIND_WORD: Partial<Record<Kind, string>> = {
  system: "software system",
  container: "container",
  component: "component",
};

/** The drawing behind a styled level: which style, and the region per layer. */
export interface StyledScene {
  name: string;
  drawing: Drawing;
  regions: LayerRegion[];
}

export interface DiagramScene {
  mode: DiagramMode;
  /** The focus level: null = top-level (systems/persons). */
  focusId: string | null;
  nodes: DiagramNode[];
  edges: DiagramEdge[];
  /** Present in `styled` mode. */
  styled?: StyledScene;
  /** Regions drawn behind the nodes: a style's layers, the code tier's
   *  depth bands, or a box tier's boundary and tooling band. */
  regions?: LayerRegion[];
}

// Card footprint on the arch tiers — used for handle routing before React Flow
// has measured the cards (DiagramView) and by the C4 arrangement.
export const CARD_W = 180;
export const CARD_H = 160;


// Relative dot sizing: the most depended-upon symbol fills MAX_DOT, a leaf sits
// at MIN_DOT, scaled against the graph's own busiest hub (not an absolute count)
// so the contrast is dramatic without ever producing a sun-sized dot. Sized to
// read like the C4 arch tiers — substantial circles, not pinpricks.
const MIN_DOT = 18;
const MAX_DOT = 64;
function dotSizeFor(inDegree: number, maxInDegree: number): number {
  if (maxInDegree <= 0) return MIN_DOT;
  const t = Math.min(1, inDegree / maxInDegree);
  return MIN_DOT + (MAX_DOT - MIN_DOT) * Math.pow(t, 0.85);
}

// Label geometry for the centered-below layout: the glyph sits on top, the
// name beneath it, both horizontally centered. The collision box is the union
// of the glyph and the name.
const DISC_LABEL_GAP = 5; // vertical gap between disc bottom and the name
const NAME_H = 15; // text-xs name line
const LABEL_BLOCK_H = NAME_H; // the name under the glyph
// Estimated rendered label width (px): text-xs ≈ 6.2px/glyph, capped at the
// 130px truncate the dot label uses.
const estLabelWidth = (name: string) =>
  Math.min(130, (name || "·").length * 6.2 + 6);

/** Collision radius (px) of a code-tier dot row — the disc plus its centered
 *  label block, approximated as a circle. Shared with the live simulation's
 *  collide force (`useDotSim`) so physics and pixels agree on how much room a
 *  dot claims. */
export function dotCollideRadius(name: string, dotSize: number): number {
  const w = Math.max(dotSize, estLabelWidth(name));
  const h = dotSize + DISC_LABEL_GAP + LABEL_BLOCK_H;
  return Math.max(w, h) / 2;
}

/**
 * Build the scene for a level.
 */
export async function buildDiagramScene(
  model: ScryModel,
  focusId: string | null,
  report: ModelHealthReport | null,
  previewable: ReadonlySet<string> = new Set(),
  styles: ReadonlyMap<string, StyleDef> = styleTable(report),
): Promise<DiagramScene> {
  const byId = new Map(model.nodes.map((n) => [n.id, n]));
  // The root's own symbols beside components: not part of the drawn level.
  const hasComponents = new Set(model.nodes.filter((n) => n.kind === "component").map((n) => n.parentId ?? null));
  const offLevel = (n: (typeof model.nodes)[number]) => n.kind === "symbol" && hasComponents.has(n.parentId ?? null);
  const childIdsOf = new Map<string, string[]>();
  for (const n of model.nodes)
    if (n.parentId && !offLevel(n)) (childIdsOf.get(n.parentId) ?? childIdsOf.set(n.parentId, []).get(n.parentId)!).push(n.id);

  const children = model.nodes.filter((n) => (n.parentId ?? null) === focusId && !offLevel(n));
  const childIds = new Set(children.map((c) => c.id));

  // Lift a link endpoint to the visible child that contains it: walk up the
  // parent chain until the parent is the focus level. Returns null when the
  // endpoint lives outside this level (so the link doesn't belong here), or
  // in a root symbol the level doesn't draw.
  const liftToLevel = (nodeId: string): string | null => {
    let cur = byId.get(nodeId);
    const seen = new Set<string>();
    while (cur && !seen.has(cur.id)) {
      seen.add(cur.id);
      const parent = cur.parentId ?? null;
      if (parent === focusId) return childIds.has(cur.id) ? cur.id : null;
      if (parent === null) return null;
      cur = byId.get(parent);
    }
    return null;
  };

  // Depth of a node (root-level = 0), memoized.
  const depthCache = new Map<string, number>();
  const depthOf = (id: string): number => {
    const cached = depthCache.get(id);
    if (cached !== undefined) return cached;
    let d = 0;
    let cur = byId.get(id);
    const seen = new Set<string>();
    while (cur && cur.parentId && !seen.has(cur.id)) {
      seen.add(cur.id);
      cur = byId.get(cur.parentId);
      d++;
    }
    depthCache.set(id, d);
    return d;
  };
  // Roll a node strictly above this level: walk up to the focus's own altitude
  // (one level above the visible children) so a cross-boundary peer reads as the
  // higher-level box that contains it (C4 rule) — a sibling symbol's component, a
  // sibling component's container. Nodes already at or above that altitude come
  // back unchanged. Cycle-guarded.
  const childDepth = focusId === null ? 0 : depthOf(focusId) + 1;
  const rollAbove = (nodeId: string): string => {
    const target = childDepth - 1;
    let cur = byId.get(nodeId);
    let d = depthOf(nodeId);
    const seen = new Set<string>();
    while (cur && d > target && cur.parentId && !seen.has(cur.id)) {
      seen.add(cur.id);
      cur = byId.get(cur.parentId);
      d--;
    }
    return cur ? cur.id : nodeId;
  };

  const ghostIds = new Set<string>();
  const edgeMap = new Map<string, DiagramEdge>();
  const addEdge = (
    linkId: string,
    label: string,
    method: string | undefined,
    source: string,
    target: string,
    kind?: string,
  ) => {
    if (source === target) return;
    const key = `${source}\0${target}`;
    if (edgeMap.has(key)) return;
    edgeMap.set(key, { id: linkId, source, target, label, method, kind, implied: false });
  };
  for (const link of model.links) {
    const s = liftToLevel(link.src);
    const t = liftToLevel(link.dst);
    if (s && t) addEdge(link.id, link.label ?? "", link.method, s, t, link.kind);
  }

  // Ghosts: the focus node's cross-boundary connections, read off the same code
  // import graph the wiki's implied connections use (report.derived.resolvedEdges).
  // For every resolved edge with exactly one end inside this level, the inside end
  // is rolled to the child it passes through, and the outside end (the peer) is
  // rolled strictly above the level — that rolled-up peer is the ghost, wired to
  // that child. Unlike the wiki we keep edges a declared link already "covers",
  // because the diagram doesn't draw those declared cross-level links separately;
  // dropping them here is what left the arch tiers ghost-less. Counts sum per
  // (child, ghost) and show as ×n.
  if (focusId !== null) {
    const implied = new Map<string, { source: string; target: string; ghost: string; count: number }>();
    for (const e of report?.derived.resolvedEdges ?? []) {
      const srcChild = liftToLevel(e.srcNode);
      const dstChild = liftToLevel(e.dstNode);
      const srcIn = srcChild !== null;
      if (srcIn === (dstChild !== null)) continue; // not crossing this level's boundary
      const child = (srcIn ? srcChild : dstChild)!;
      const ghost = rollAbove(srcIn ? e.dstNode : e.srcNode);
      // A root symbol's edge rolls up to the focus itself: no ghost.
      if (!byId.has(ghost) || ghost === child || ghost === focusId || childIds.has(ghost)) continue;
      const source = srcIn ? child : ghost;
      const target = srcIn ? ghost : child;
      const key = source + ">" + target;
      const cur = implied.get(key);
      if (cur) cur.count += e.count;
      else implied.set(key, { source, target, ghost, count: e.count });
    }
    for (const e of implied.values()) {
      ghostIds.add(e.ghost);
      addEdge("implied:" + e.source + ">" + e.target, "×" + e.count, undefined, e.source, e.target);
    }
    // A declared link to a person or an external (a container's `uses` in
    // structure.toml) has no import behind it, so it ghosts that node here.
    for (const link of model.links) {
      const srcChild = liftToLevel(link.src);
      const dstChild = liftToLevel(link.dst);
      if ((srcChild !== null) === (dstChild !== null)) continue;
      const outside = byId.get(srcChild !== null ? link.dst : link.src);
      if (!outside || outside.parentId || (outside.kind !== "person" && !outside.external)) continue;
      ghostIds.add(outside.id);
      const source = srcChild ?? outside.id;
      const target = dstChild ?? outside.id;
      addEdge(link.id, link.label ?? "", link.method, source, target, link.kind);
    }
  }
  const edges = [...edgeMap.values()];

  // Fan-in per visible node — how many other symbols depend on it. Drives dot
  // size on the code tier: the more depended-upon a symbol is, the bigger it
  // reads, so the load-bearing hubs stand out.
  const degree = new Map<string, number>();
  for (const e of edges) {
    degree.set(e.target, (degree.get(e.target) ?? 0) + 1);
  }

  const base = (id: string, reference: boolean): Omit<DiagramNode, "x" | "y" | "dotSize" | "pinned"> => {
    const n = byId.get(id)!;
    return {
      id: n.id,
      kind: n.kind,
      name: n.name,
      external: Boolean(n.external),
      technology: n.technology,
      description: n.description,
      hasChildren: childIdsOf.has(n.id),
      childIds: childIdsOf.get(n.id) ?? [],
      degree: degree.get(n.id) ?? 0,
      reference,
      symbolClass: previewable.has(n.id) ? "visual" : isDataShape(n) ? "model" : "code",
      style: n.style,
      layer: layerOf(model, n.id),
      hasClaims: (n.responsibilities?.length ?? 0) > 0 || (n.properties?.length ?? 0) > 0,
    };
  };

  // The component level of a styled container draws in its style's shape.
  const styledLevel = focusId !== null && children.length > 0 && children.every((c) => c.kind === "component");
  const styleDef = styledLevel ? governingStyleDef(model, focusId, styles) : undefined;
  const mode: DiagramMode =
    children.length > 0 && children.every((c) => c.kind === "symbol")
      ? "code"
      : styleDef
        ? "styled"
        : "arch";

  if (children.length === 0) {
    return { mode, focusId, nodes: [], edges };
  }

  // Real children first, then ghosts (referenced from this level, living
  // elsewhere). Both feed the layout so edges to ghosts route properly.
  const ghosts = [...ghostIds];
  const layoutIds = [...children.map((c) => c.id), ...ghosts];

  // Size every dot relative to the busiest hub on this level — dramatic contrast
  // without an absolute scale that could blow a dot up out of proportion.
  const maxIn = layoutIds.reduce((m, id) => Math.max(m, degree.get(id) ?? 0), 0);
  const sizeById = new Map(
    layoutIds.map((id) => [id, dotSizeFor(degree.get(id) ?? 0, maxIn)] as const),
  );

  // A position given on a node fixes it and auto-layout places the rest. The
  // app never gives one (its maps are derived); a caller drawing a curated
  // map does — the demo's film placements, an export. Arch tiers only, and
  // never ghosts: a ghost's position belongs to the surface where it lives.
  const pinnedPos = new Map<string, { x: number; y: number }>(
    mode === "arch"
      ? children
          .filter((c) => c.position)
          .map((c) => [c.id, { x: c.position!.x, y: c.position!.y }] as const)
      : [],
  );

  let styled: StyledScene | undefined;
  let regions: LayerRegion[] | undefined;
  let positions: Map<string, { x: number; y: number }>;
  let aside = new Set<string>();
  if (mode === "code") {
    // Symbols by call depth from the component's surface — importance in
    // position — instead of a free constellation.
    const laid = depthLayout(
      children.map((c) => ({ id: c.id, size: sizeById.get(c.id)!, labelW: estLabelWidth(c.name) })),
      ghosts,
      edges,
    );
    // The layout gives disc CENTRES; the dot node's origin is the disc's
    // top-left, so each dot shifts by its own radius or big dots drift.
    positions = new Map(
      [...laid.centers].map(([id, c]) => {
        const r = (sizeById.get(id) ?? MIN_DOT) / 2;
        return [id, { x: c.x - r, y: c.y - r }];
      }),
    );
    regions = laid.regions;
  } else if (mode === "styled" && styleDef) {
    // Structural violations from the health report — imports and declared
    // links the style forbids — are the only source of red. They are drawn even when no link
    // declares the pair: a declared-links-only view would read as compliance
    // the code does not have.
    for (const v of report?.structural?.violations ?? []) {
      if (!v.other) continue;
      const s = liftToLevel(v.node), t = liftToLevel(v.other);
      if (!s || !t || s === t) continue;
      const key = `${s}\0${t}`;
      const existing = edgeMap.get(key);
      if (existing) {
        existing.violations ??= [];
        if (!existing.violations.includes(v.detail)) existing.violations.push(v.detail);
        existing.implied = false;
        continue;
      }
      const e: DiagramEdge = {
        id: "violation:" + key,
        source: s,
        target: t,
        label: "",
        method: undefined,
        kind: undefined,
        implied: false,
        violations: [v.detail],
      };
      edgeMap.set(key, e);
      edges.push(e);
    }
    // Tooling components (examples, benches) hold no layer: a strip of
    // their own under the drawing.
    const toolIds = new Set(children.filter((c) => c.kind === "component" && c.tooling).map((c) => c.id));
    const members = children
      .filter((c) => c.kind === "component" && !toolIds.has(c.id))
      .map((c) => ({ id: c.id, layer: c.layer }));
    const laid = styledLayout(styleDef, members, ghosts, edges);
    for (const e of edges) e.waypoints = laid.routes?.get(`${e.source}\0${e.target}`);
    const tools = toolingStrip(laid, [...toolIds]);
    positions = new Map(
      [...laid.centers, ...tools.centers].map(([id, c]) => [id, { x: c.x - CARD_W / 2, y: c.y - CARD_H / 2 }]),
    );
    for (const [id, at] of pinnedPos) positions.set(id, at);
    const allRegions = tools.region ? [...laid.regions, tools.region] : laid.regions;
    styled = { name: styleDef.name, drawing: styleDef.drawing, regions: allRegions };
    regions = allRegions;
    // The drawing already says the legal ones: drawn quiet. Links that leave
    // the container say what the drawing cannot — who calls in, what is
    // used — so they are drawn plain.
    const layerById = new Map(children.map((c) => [c.id, c.layer] as const));
    const ringDrawing = styleDef.drawing === "rings" || styleDef.drawing === "hexagon";
    const innermost = styleDef.layers[styleDef.layers.length - 1]?.name;
    for (const e of edges) {
      if (e.id.startsWith("violation:")) continue;
      if (ghostIds.has(e.source) || ghostIds.has(e.target)) {
        e.implied = false;
        continue;
      }
      // Tooling links say little about the product: drawn quiet.
      if (toolIds.has(e.source) || toolIds.has(e.target)) {
        e.implied = true;
        continue;
      }
      const sl = layerById.get(e.source), tl = layerById.get(e.target);
      e.implied = !e.violations && classifyStyledEdge(styleDef, sl, tl) === "implied";
      // A same-layer chord crosses whatever sits between its ends — the
      // centre on a ring, the neighbouring cards on a band. Bow it clear:
      // out to the ring itself, or above the band (left of the column).
      if (sl && sl === tl) {
        const sc = laid.centers.get(e.source), tc = laid.centers.get(e.target);
        if (!sc || !tc) continue;
        const mid = { x: (sc.x + tc.x) / 2, y: (sc.y + tc.y) / 2 };
        if (ringDrawing && sl !== innermost) {
          const ring = (Math.hypot(sc.x, sc.y) + Math.hypot(tc.x, tc.y)) / 2;
          e.bow = { cx: 0, cy: 0, offset: Math.max(40, ring - Math.hypot(mid.x, mid.y)) };
        } else if (!ringDrawing && Math.abs(styleDef.drawing === "columns" ? sc.y - tc.y : sc.x - tc.x) > CARD_W * 1.2) {
          // Only when a card can sit between the ends (further apart than one pitch).
          e.bow =
            styleDef.drawing === "columns"
              ? { cx: mid.x + 1e6, cy: mid.y, offset: CARD_W * 0.7 }
              : { cx: mid.x, cy: mid.y + 1e6, offset: CARD_H * 0.75 };
        }
      }
    }
  } else {
    // C4 arrangement: who uses the level above it, the level inside its
    // boundary (tooling in a band at its foot), external systems it calls
    // beside it, the rest of what it uses below.
    const inside: string[] = [], tooling: string[] = [], above: string[] = [];
    const beside: string[] = [], below: string[] = [];
    for (const id of layoutIds) {
      const n = byId.get(id)!;
      // An outside node that mostly calls into the level uses it: above.
      const calls = edges.filter((e) => e.source === id).length;
      const called = edges.filter((e) => e.target === id).length;
      if (n.kind === "person") above.push(id);
      else if (n.external) (calls > called ? above : beside).push(id);
      else if (ghostIds.has(id)) (calls > called ? above : below).push(id);
      else if (n.tooling) tooling.push(id);
      else inside.push(id);
    }
    const laid = c4Layout({ inside, tooling, above, beside, below, edges, cardW: CARD_W, cardH: CARD_H });
    aside = new Set(beside);
    positions = laid.positions;
    for (const [id, at] of pinnedPos) positions.set(id, at);
    // A pinned end moved off the rows the lanes were laid between.
    for (const e of edges) {
      const route = laid.routes.get(`${e.source}\0${e.target}`);
      if (route && !pinnedPos.has(e.source) && !pinnedPos.has(e.target)) e.waypoints = route;
    }
    // Tooling links say little about the product: drawn quiet.
    const toolingSet = new Set(tooling);
    for (const e of edges) if (toolingSet.has(e.source) || toolingSet.has(e.target)) e.implied = true;
    const focus = focusId ? byId.get(focusId) : undefined;
    regions = [];
    if (focus && laid.boundary) {
      regions.push({ shape: "rect", ...laid.boundary, layer: `${focus.name} · ${KIND_WORD[focus.kind] ?? focus.kind}`, boundary: true });
    }
    if (laid.toolingBand) {
      regions.push({
        shape: "rect",
        ...laid.toolingBand,
        layer: "tooling",
        caption: "Dev tooling: part of the codebase, not of the product, and held to no style. Its links are drawn faint.",
      });
    }
  }

  const nodes: DiagramNode[] = layoutIds.map((id) => {
    const p = positions.get(id) ?? { x: 0, y: 0 };
    return {
      ...base(id, ghostIds.has(id)),
      pinned: pinnedPos.has(id),
      aside: aside.has(id) || undefined,
      dotSize: sizeById.get(id) ?? MIN_DOT,
      x: p.x,
      y: p.y,
    };
  });

  return { mode, focusId, nodes, edges, styled, regions };
}
