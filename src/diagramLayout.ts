/**
 * Diagram layout adapter — turns the v0.3 model into a positioned, renderable
 * scene for the secondary Diagram view. All layout math lives here so the
 * renderer (DiagramView) stays a pure projection of a `DiagramScene`.
 *
 * One "scene" is one level: the children of a focus node (or the top-level
 * nodes when focus is null), the links lifted to that level, and a position
 * per node. The mode is chosen by the children's altitude:
 *   - architecture tiers (system/container) → planar box layout
 *     (`layoutGraph`), the formal C4 boxes-and-lines.
 *   - the component level of a styled container → the style's own drawing
 *     (`styledLayout`): rows, columns, rings or Cockburn's hexagon, with each
 *     card fixed to its layer's band and only the order within it free.
 *   - the code tier (symbols) → a force-directed dot graph, where boxes-and-
 *     lines would be noise and relationships read better as a constellation.
 */

import { layoutGraph } from "./layout/planar";
import type { EdgePair } from "./layout/planar";
import type { ScryModel, Kind } from "./viewmodel";
import { isDataShape } from "./viewmodel";
import type { ModelHealthReport } from "./health";
import { governingStyleDef, layerOf, styleTable, type Drawing, type StyleDef } from "./styles";
import { classifyStyledEdge, styledLayout, type LayerRegion } from "./layout/styled";
import { depthLayout } from "./layout/depth";

export type DiagramMode = "arch" | "styled" | "code";

/** How a code-tier symbol reads at a glance — the three things a symbol can be
 *  (mirrors `kindIcon`/`typeTag`): a data shape (`model`), a component the
 *  preview sidecar can render (`visual`, derived — never stored on the node),
 *  or plain `code`. Drives the muted class line under each dot. */
export type SymbolClass = "code" | "model" | "visual";

export interface DiagramNode {
  id: string;
  kind: Kind;
  name: string;
  external: boolean;
  /** Manually placed: the position below is the user's stored placement, not
   *  auto-layout's. Pinned nodes stay put; layout only places the rest. */
  pinned: boolean;
  technology?: string;
  description?: string;
  /** Has its own children — so the renderer can mark it drillable. */
  hasChildren: boolean;
  /** Number of direct children — shown on the card in place of a bare flag. */
  childCount: number;
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
  /** A styled container's miniature: its components as dots in their layer
   *  positions, with the layer regions — the real shape of its inside. */
  thumbnail?: Thumbnail;
  x: number;
  y: number;
}

export interface Thumbnail {
  regions: LayerRegion[];
  dots: { x: number; y: number }[];
}

export interface DiagramEdge {
  id: string;
  source: string;
  target: string;
  label: string;
  method?: string;
  /** What the link is, when declared (styled containers). */
  kind?: string;
  /** Couldn't be embedded planar — the renderer routes it loosely. */
  nonPlanar: boolean;
  /** Styled mode: the drawing already says this (a step into the innermost
   *  layer, an adapter onto its port) — hidden until one end is selected. */
  implied: boolean;
  /** Styled mode: both ends sit on one ring or band, so the straight chord
   *  would cut through what lies between — bow it away from `(cx, cy)`, the
   *  curve's middle displaced `offset` px from the chord's midpoint. */
  bow?: { cx: number; cy: number; offset: number };
  /** Styled mode: the layer matrix forbids this dependency — drawn red,
   *  with this reason on hover and beside the line when selected. */
  violation?: string;
}

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
  /** Regions drawn behind the nodes: a style's layers, or the code tier's
   *  depth bands. Absent on the free planar tiers. */
  regions?: LayerRegion[];
}

// Grid cell size for the planar box layout (matches the canvas on `main`).
const CELL_W = 300;
const CELL_H = 180;

// Card footprint on the arch tiers — used for handle routing before React Flow
// has measured the cards (DiagramView) and for keeping auto-laid cards off
// pinned ones (the locked relax pass below).
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

// Label geometry for the centered-below layout: the disc sits on top, the name
// (and a muted class line) stack beneath it, all horizontally centered. The
// collision box is the union of the disc and that text block.
const DISC_LABEL_GAP = 5; // vertical gap between disc bottom and the name
const NAME_H = 15; // text-xs name line
const SUB_H = 12; // text-[10px] class line
const LABEL_BLOCK_H = NAME_H + SUB_H; // both label lines stack under the disc
// Estimated rendered label width (px): text-xs ≈ 6.2px/glyph, capped at the
// 130px truncate the dot label uses.
const estLabelWidth = (name: string) =>
  Math.min(130, (name || "·").length * 6.2 + 6);

const pairKey = (a: string, b: string) =>
  a < b ? `${a}\0${b}` : `${b}\0${a}`;

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
 * Build the scene for a level. Async because the planar layout is.
 */
export async function buildDiagramScene(
  model: ScryModel,
  focusId: string | null,
  report: ModelHealthReport | null,
  previewable: ReadonlySet<string> = new Set(),
  styles: ReadonlyMap<string, StyleDef> = styleTable(report),
): Promise<DiagramScene> {
  const byId = new Map(model.nodes.map((n) => [n.id, n]));
  const childCounts = new Map<string, number>();
  for (const n of model.nodes)
    if (n.parentId) childCounts.set(n.parentId, (childCounts.get(n.parentId) ?? 0) + 1);

  const children = model.nodes.filter((n) => (n.parentId ?? null) === focusId);
  const childIds = new Set(children.map((c) => c.id));

  // Lift a link endpoint to the visible child that contains it: walk up the
  // parent chain until the parent is the focus level. Returns null when the
  // endpoint lives outside this level (so the link doesn't belong here).
  const liftToLevel = (nodeId: string): string | null => {
    let cur = byId.get(nodeId);
    const seen = new Set<string>();
    while (cur && !seen.has(cur.id)) {
      seen.add(cur.id);
      const parent = cur.parentId ?? null;
      if (parent === focusId) return cur.id;
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

  // A styled container's miniature — its own components laid out in its own
  // drawing, shrunk. Computed here (cheap: a handful of containers per level)
  // so the card can draw the real shape instead of a generic glyph.
  const thumbnailFor = (id: string): Thumbnail | undefined => {
    const n = byId.get(id);
    if (!n || n.kind !== "container" || !n.style) return undefined;
    const def = styles.get(n.style);
    if (!def) return undefined;
    const comps = model.nodes.filter((c) => c.kind === "component" && c.parentId === id);
    if (comps.length === 0) return undefined;
    const compOf = new Map<string, string>();
    for (const c of comps) compOf.set(c.id, c.id);
    for (const s of model.nodes) if (s.kind === "symbol" && s.parentId && compOf.has(s.parentId)) compOf.set(s.id, s.parentId);
    const inner = model.links
      .filter((l) => compOf.has(l.src) && compOf.has(l.dst))
      .map((l) => ({ source: compOf.get(l.src)!, target: compOf.get(l.dst)! }))
      .filter((e) => e.source !== e.target);
    const laid = styledLayout(def, comps.map((c) => ({ id: c.id, layer: c.layer })), [], inner);
    return { regions: laid.regions, dots: comps.map((c) => laid.centers.get(c.id) ?? { x: 0, y: 0 }) };
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
    edgeMap.set(key, { id: linkId, source, target, label, method, kind, nonPlanar: false, implied: false });
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
      if (!byId.has(ghost) || ghost === child || childIds.has(ghost)) continue;
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
      hasChildren: (childCounts.get(n.id) ?? 0) > 0,
      childCount: childCounts.get(n.id) ?? 0,
      degree: degree.get(n.id) ?? 0,
      reference,
      symbolClass: previewable.has(n.id) ? "visual" : isDataShape(n) ? "model" : "code",
      style: n.style,
      layer: layerOf(model, n.id),
      hasClaims: (n.responsibilities?.length ?? 0) > 0 || (n.properties?.length ?? 0) > 0,
      thumbnail: thumbnailFor(n.id),
    };
  };

  // The component level of a styled container draws in its style's shape.
  const styleDef =
    focusId !== null && children.length > 0 && children.every((c) => c.kind === "component")
      ? governingStyleDef(model, focusId, styles)
      : undefined;
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

  // Manual placements pin their nodes: auto-layout only places the rest. Arch
  // tiers only, and never ghosts — a ghost's stored position belongs to the
  // surface where the node really lives, not to this one.
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
    const members = children.map((c) => ({ id: c.id, layer: c.layer }));
    const laid = styledLayout(styleDef, members, ghosts, edges);
    positions = new Map(
      [...laid.centers].map(([id, c]) => [id, { x: c.x - CARD_W / 2, y: c.y - CARD_H / 2 }]),
    );
    for (const [id, at] of pinnedPos) positions.set(id, at);
    styled = { name: styleDef.name, drawing: styleDef.drawing, regions: laid.regions };
    regions = laid.regions;
    // Code-time violations from the health report — real imports the layer
    // matrix forbids — are drawn even when no link declares the pair. The
    // map exists to expose exactly these; a declared-links-only view would
    // read as compliance the code does not have.
    for (const v of report?.style?.violations ?? []) {
      if (!v.other) continue;
      const s = liftToLevel(v.node), t = liftToLevel(v.other);
      if (!s || !t || s === t) continue;
      const key = `${s}\0${t}`;
      const existing = edgeMap.get(key);
      if (existing) {
        existing.violation = v.detail;
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
        nonPlanar: false,
        implied: false,
        violation: v.detail,
      };
      edgeMap.set(key, e);
      edges.push(e);
    }
    // The drawing already says these — hide them until a selection asks.
    const layerById = new Map(children.map((c) => [c.id, c.layer] as const));
    const ringDrawing = styleDef.drawing === "rings" || styleDef.drawing === "hexagon";
    const innermost = styleDef.layers[styleDef.layers.length - 1]?.name;
    for (const e of edges) {
      const sourceGhost = ghostIds.has(e.source), targetGhost = ghostIds.has(e.target);
      const sl = layerById.get(e.source), tl = layerById.get(e.target);
      const verdict = classifyStyledEdge(styleDef, sl, tl, { sourceGhost, targetGhost });
      // A code-time violation stays a violation whatever the declared link says.
      if (e.id.startsWith("violation:")) continue;
      e.implied = verdict === "implied";
      e.violation =
        verdict === "violation"
          ? violationReason(styleDef, byId.get(e.source)?.name ?? e.source, sl, byId.get(e.target)?.name ?? e.target, tl, sourceGhost, targetGhost)
          : undefined;
      if (sourceGhost || targetGhost) continue;
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
    positions = await archLayout(layoutIds, edges, pinnedPos);
  }

  const nodes: DiagramNode[] = layoutIds.map((id) => {
    const p = positions.get(id) ?? { x: 0, y: 0 };
    return {
      ...base(id, ghostIds.has(id)),
      pinned: pinnedPos.has(id),
      dotSize: sizeById.get(id) ?? MIN_DOT,
      x: p.x,
      y: p.y,
    };
  });

  return { mode, focusId, nodes, edges, styled, regions };
}

/** One line saying why a styled edge is red — the rule it breaks, in the
 *  style's own words, so the map never shows a red line without a reason. */
function violationReason(
  def: StyleDef,
  source: string,
  sl: string | undefined,
  target: string,
  tl: string | undefined,
  sourceGhost: boolean,
  targetGhost: boolean,
): string {
  if (sourceGhost && tl) {
    return `${source} (outside) reaches ${target} on the ${tl} layer — in ${def.name}, traffic from outside enters through ${def.inbound.join(" or ")}`;
  }
  if (targetGhost && sl) {
    return `${source} (${sl}) reaches out of the container to ${target} — in ${def.name}, only ${(def.outbound ?? []).join(" or ") || "no layer"} talks to the outside`;
  }
  if (sl && tl && sl === tl) {
    return `${source} depends on ${target}, a sibling on the ${sl} layer — ${def.name} keeps slices on a layer isolated`;
  }
  const allowed = sl ? def.matrix[sl] ?? [] : [];
  return `${source} (${sl}) depends on ${target} (${tl}) — in ${def.name}, ${sl} may depend on ${allowed.length ? allowed.join(", ") : "nothing"}`;
}

// ── Architecture tiers: planar boxes ────────────────────────────────────────

/**
 * Planar layout for the box tiers. Mirrors `autoLayout` on `main`: connected
 * nodes go through the planar embedder; isolated nodes are grid-packed below
 * so a lone container never gets stranded in the planar strip. Mutates the
 * passed `edges` to flag the non-planar ones for loose routing.
 *
 * `pinned` holds the user's manual placements: those nodes land exactly where
 * they were dragged, and only the rest take the computed spots — which are
 * deterministic for a given graph, so dragging one card never moves the
 * others. A locked relax pass then pushes any auto-laid card off a pinned one.
 */
async function archLayout(
  nodeIds: string[],
  edges: DiagramEdge[],
  pinned: Map<string, { x: number; y: number }>,
): Promise<Map<string, { x: number; y: number }>> {
  const idSet = new Set(nodeIds);
  const pairs: EdgePair[] = edges
    .filter((e) => idSet.has(e.source) && idSet.has(e.target))
    .map((e) => [e.source, e.target]);

  const connected = new Set<string>();
  for (const [u, v] of pairs) {
    connected.add(u);
    connected.add(v);
  }
  const graphIds = nodeIds.filter((id) => connected.has(id));
  const isolated = nodeIds.filter((id) => !connected.has(id));

  const grid = new Map<string, { col: number; row: number }>();
  let nonPlanar: EdgePair[] = [];
  if (graphIds.length > 0) {
    const res = await layoutGraph(graphIds, pairs);
    for (const [id, p] of res.positions) grid.set(id, p);
    nonPlanar = res.nonPlanarEdges;
  }

  // Pack isolated nodes into a grid below whatever the planar layout produced.
  if (isolated.length > 0) {
    let maxRow = -1;
    let maxCol = 0;
    for (const [, p] of grid) {
      maxRow = Math.max(maxRow, p.row);
      maxCol = Math.max(maxCol, p.col);
    }
    const cols = Math.max(
      1,
      Math.min(maxCol + 1 || 3, Math.ceil(Math.sqrt(isolated.length))),
    );
    const startRow = maxRow + 2;
    for (let i = 0; i < isolated.length; i++) {
      grid.set(isolated[i], {
        col: i % cols,
        row: startRow + Math.floor(i / cols) * 2,
      });
    }
  }

  const npKeys = new Set(nonPlanar.map(([a, b]) => pairKey(a, b)));
  for (const e of edges)
    e.nonPlanar = npKeys.has(pairKey(e.source, e.target));

  const px = new Map<string, { x: number; y: number }>();
  for (const id of nodeIds) {
    const p = grid.get(id) ?? { col: 0, row: 0 };
    px.set(id, { x: p.col * CELL_W, y: p.row * CELL_H });
  }
  if (pinned.size === 0) return px;

  // Pinned nodes sit exactly where the user put them; every other node keeps
  // its computed spot. NO global adjustment happens here: the planar layout is
  // deterministic for a given graph, so an unpinned node's position is stable
  // across drags of OTHER nodes — dragging one card must never shift the rest
  // (an earlier centroid-translation version did exactly that, and a single
  // drop read as the whole graph panning while the card "snapped back").
  for (const [id, at] of pinned) {
    px.set(id, { x: at.x, y: at.y });
  }

  // Auto-laid cards must not land under a pinned one: resolve card-footprint
  // overlaps, moving only the unpinned (the user's placements are law).
  const boxes = new Map<string, Box>(nodeIds.map((id) => [id, { w: CARD_W, h: CARD_H }]));
  relaxLabelBoxes(px, boxes, LABEL_PAD, 500, new Set(pinned.keys()));
  return px;
}

// ── Card separation (shared by the planar tiers) ────────────────────────────

// Breathing room (px) left between rendered rows by the final separation pass.
const LABEL_PAD = 24;

/** The rendered row of a dot: the disc plus its label-to-the-right, anchored at
 *  the node's (x, y) top-left. */
interface Box {
  w: number;
  h: number;
}

/**
 * Push overlapping label boxes apart along their smaller penetration axis, with
 * `pad` px of slack, until none overlap (or `iterations` runs out). This is the
 * asymmetric collision ForceAtlas2/noverlap can't do: they treat a node as a
 * circle, so they keep the *discs* apart but let a label — which extends to the
 * right of its disc — overlap a neighbour. Resolving the real rows is what gives
 * the dots and labels room to breathe.
 *
 * `locked` boxes never move (the arch tiers' pinned cards): an overlap with one
 * pushes the other box the whole way, and two locked boxes are left where the
 * user put them, overlapping or not.
 */
function relaxLabelBoxes(
  pos: Map<string, { x: number; y: number }>,
  boxes: Map<string, Box>,
  pad: number,
  iterations: number,
  locked: ReadonlySet<string> = new Set(),
): void {
  const ids = [...pos.keys()];
  for (let it = 0; it < iterations; it++) {
    let moved = false;
    for (let i = 0; i < ids.length; i++) {
      for (let j = i + 1; j < ids.length; j++) {
        const la = locked.has(ids[i]), lb = locked.has(ids[j]);
        if (la && lb) continue;
        const pa = pos.get(ids[i])!, pb = pos.get(ids[j])!;
        const ba = boxes.get(ids[i])!, bb = boxes.get(ids[j])!;
        const dx = pb.x + bb.w / 2 - (pa.x + ba.w / 2);
        const dy = pb.y + bb.h / 2 - (pa.y + ba.h / 2);
        const ox = (ba.w + bb.w) / 2 + pad - Math.abs(dx);
        const oy = (ba.h + bb.h) / 2 + pad - Math.abs(dy);
        if (ox > 0 && oy > 0) {
          // Each side takes half the push; a locked side's half goes to the
          // other, so the pair still separates by the full overlap.
          const wa = la ? 0 : lb ? 1 : 0.5;
          const wb = lb ? 0 : la ? 1 : 0.5;
          if (ox < oy) {
            const s = (dx < 0 ? -1 : 1) * ox;
            pa.x -= s * wa; pb.x += s * wb;
          } else {
            const s = (dy < 0 ? -1 : 1) * oy;
            pa.y -= s * wa; pb.y += s * wb;
          }
          moved = true;
        }
      }
    }
    if (!moved) break;
  }
}
