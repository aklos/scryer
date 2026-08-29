/**
 * Diagram layout adapter — turns the v0.3 model into a positioned, renderable
 * scene for the secondary Diagram view. All layout math lives here so the
 * renderer (DiagramView) stays a pure projection of a `DiagramScene`.
 *
 * One "scene" is one level: the children of a focus node (or the top-level
 * nodes when focus is null), the links lifted to that level, and a position
 * per node. The mode is chosen by the children's altitude:
 *   - architecture tiers (system/container/component) → planar box layout
 *     (`layoutGraph`), the formal C4 boxes-and-lines.
 *   - the code tier (symbols) → a force-directed dot graph, where boxes-and-
 *     lines would be noise and relationships read better as a constellation.
 */

import Graph from "graphology";
import forceAtlas2 from "graphology-layout-forceatlas2";
import { layoutGraph } from "./layout/planar";
import type { EdgePair } from "./layout/planar";
import type { ScryModel, Kind } from "./viewmodel";
import { isDataShape } from "./viewmodel";
import type { ModelHealthReport } from "./health";

export type DiagramMode = "arch" | "code";

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
  x: number;
  y: number;
}

export interface DiagramEdge {
  id: string;
  source: string;
  target: string;
  label: string;
  method?: string;
  /** Couldn't be embedded planar — the renderer routes it loosely. */
  nonPlanar: boolean;
}

export interface DiagramScene {
  mode: DiagramMode;
  /** The focus level: null = top-level (systems/persons). */
  focusId: string | null;
  nodes: DiagramNode[];
  edges: DiagramEdge[];
}

// Grid cell size for the planar box layout (matches the canvas on `main`).
const CELL_W = 300;
const CELL_H = 180;

// Card footprint on the arch tiers — used for handle routing before React Flow
// has measured the cards (DiagramView) and for keeping auto-laid cards off
// pinned ones (the locked relax pass below).
export const CARD_W = 180;
export const CARD_H = 160;

// ForceAtlas2 tuning for the code tier lives with the layout (`fa2Settings`).

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

  const ghostIds = new Set<string>();
  const edgeMap = new Map<string, DiagramEdge>();
  const addEdge = (linkId: string, label: string, method: string | undefined, source: string, target: string) => {
    if (source === target) return;
    const key = `${source}\0${target}`;
    if (edgeMap.has(key)) return;
    edgeMap.set(key, { id: linkId, source, target, label, method, nonPlanar: false });
  };
  for (const link of model.links) {
    const s = liftToLevel(link.src);
    const t = liftToLevel(link.dst);
    if (s && t) addEdge(link.id, link.label ?? "", link.method, s, t);
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
    };
  };

  const mode: DiagramMode =
    children.length > 0 && children.every((c) => c.kind === "symbol")
      ? "code"
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

  const positions =
    mode === "code"
      ? dotLayout(
          layoutIds.map((id) => ({ id, name: byId.get(id)!.name, size: sizeById.get(id)! })),
          edges,
        )
      : await archLayout(layoutIds, edges, pinnedPos);

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

  return { mode, focusId, nodes, edges };
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

// ── Code tier: force-directed dot graph ─────────────────────────────────────

interface DotDescriptor {
  id: string;
  name: string;
  size: number; // rendered dot diameter (px)
}

// ForceAtlas2 settings, ported from gitnexus's code-graph layout (`useSigma.ts`):
// a free constellation where repulsion scales with node degree (hubs claim room)
// and `outboundAttractionDistribution` divides a hub's edge pull across its
// spokes so they fan out instead of collapsing onto it. Tiered by node count.
// The small tier diverges from gitnexus (lower gravity, higher scalingRatio):
// sigma hides most labels so it can pack tight, but we draw every label, so the
// constellation needs more room.
function fa2Settings(nodeCount: number) {
  const small = nodeCount < 500;
  const medium = nodeCount >= 500 && nodeCount < 2000;
  const large = nodeCount >= 2000 && nodeCount < 10000;
  return {
    gravity: small ? 0.4 : medium ? 0.5 : large ? 0.3 : 0.15,
    scalingRatio: small ? 50 : medium ? 30 : large ? 60 : 100,
    slowDown: small ? 1 : medium ? 2 : large ? 3 : 5,
    barnesHutOptimize: nodeCount > 200,
    barnesHutTheta: large ? 0.8 : 0.6,
    strongGravityMode: false,
    outboundAttractionDistribution: true,
    linLogMode: false,
    adjustSizes: true,
    edgeWeightInfluence: 1,
  };
}

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

/**
 * Force-directed layout for the symbol tier — gitnexus's code-graph algorithm:
 * ForceAtlas2 settled into a constellation (no dependency ranking, no axis
 * clamps — the graph finds its own equilibrium and hubs push their spokes out),
 * then a separation pass that gives every dot + label room.
 *
 * The cleanup is ours, not sigma's: sigma hides colliding labels, so a tight
 * pack is fine there. We draw every label, so we resolve the actual rendered
 * rows (`relaxLabelBoxes`) and run a more spread-out FA2 (`fa2Settings`). Runs
 * synchronously to a settled state.
 */
function dotLayout(
  descriptors: DotDescriptor[],
  edges: DiagramEdge[],
): Map<string, { x: number; y: number }> {
  const out = new Map<string, { x: number; y: number }>();
  const n = descriptors.length;
  if (n === 0) return out;
  if (n === 1) {
    out.set(descriptors[0].id, { x: 0, y: 0 });
    return out;
  }

  // Stacked box per node (disc on top, label block centered beneath). Width is
  // the wider of the disc and the label; height is the disc plus the label
  // block. Half the larger dimension is the radius FA2 uses to pre-spread; the
  // full box is what the final pass separates.
  const boxes = new Map<string, Box>(
    descriptors.map((d) => [
      d.id,
      {
        w: Math.max(d.size, estLabelWidth(d.name)),
        h: d.size + DISC_LABEL_GAP + LABEL_BLOCK_H,
      },
    ]),
  );

  // Seed on a circle so no two nodes share coordinates — ForceAtlas2 divides by
  // inter-node distance, so coincident nodes produce NaN. Radius scales with the
  // node count so the sim starts roughly spread rather than piled at the origin.
  const graph = new Graph({ type: "directed" });
  const seedR = 40 + n * 12;
  descriptors.forEach((d, i) => {
    const a = (2 * Math.PI * i) / n;
    const b = boxes.get(d.id)!;
    graph.addNode(d.id, {
      x: seedR * Math.cos(a),
      y: seedR * Math.sin(a),
      size: Math.max(b.w, b.h) / 2,
    });
  });
  for (const e of edges) {
    if (!graph.hasNode(e.source) || !graph.hasNode(e.target)) continue;
    if (e.source === e.target || graph.hasDirectedEdge(e.source, e.target)) continue;
    graph.addDirectedEdge(e.source, e.target);
  }

  const settings = { ...forceAtlas2.inferSettings(graph), ...fa2Settings(n) };
  // ForceAtlas2 needs many iterations to settle into a smooth shape — small
  // graphs were under-converging (and reading as randomly placed) at the old
  // ~n*25. Give them a high fixed budget; bound it for large levels so the
  // synchronous layout never janks.
  forceAtlas2.assign(graph, {
    iterations: n <= 150 ? 2500 : n <= 600 ? 1200 : 600,
    settings,
  });
  graph.forEachNode((id, attrs) =>
    out.set(id, { x: attrs.x as number, y: attrs.y as number }),
  );

  // Give every dot + label room (FA2 only keeps the discs apart).
  relaxLabelBoxes(out, boxes, LABEL_PAD, 500);
  return out;
}
