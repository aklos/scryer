/**
 * Planar graph layout: Tutte's barycentric embedding → Kamada-Kawai.
 *
 * Pipeline:
 *   1. Classify edges: Left-Right planarity test (true test). If planar, every
 *      edge is planar and we get a combinatorial embedding directly; only a
 *      genuinely non-planar graph falls back to greedy partition + KK.
 *   2. Augment to biconnected + triangulate
 *   3. Tutte barycentric embedding (hubs pulled to neighbour centroids)
 *   4. Kamada-Kawai: stress-minimize (used for the non-planar fallback)
 *
 * All algorithms are O(n²) or better, which is instant for n ≤ 50.
 */

import { lrPlanarity } from "./lrPlanarity";

// ── Types ──────────────────────────────────────────────────────────────

export type EdgePair = [string, string];

/** Rotation system: for each vertex, clockwise-ordered list of neighbors. */
export type Embedding = Map<string, string[]>;

export interface PlanarLayoutResult {
  /** Integer grid positions (col, row). */
  positions: Map<string, { col: number; row: number }>;
  /** Edges that couldn't be embedded in the planar subgraph. */
  nonPlanarEdges: EdgePair[];
}

// ── Graph utilities ────────────────────────────────────────────────────

function buildAdj(
  nodeIds: string[],
  edges: EdgePair[],
): Map<string, Set<string>> {
  const adj = new Map<string, Set<string>>();
  for (const id of nodeIds) adj.set(id, new Set());
  for (const [u, v] of edges) {
    adj.get(u)!.add(v);
    adj.get(v)!.add(u);
  }
  return adj;
}

/** Deduplicate edges (treat as undirected). */
export function dedupeEdges(edges: EdgePair[]): EdgePair[] {
  const seen = new Set<string>();
  const result: EdgePair[] = [];
  for (const [u, v] of edges) {
    const key = u < v ? `${u}\0${v}` : `${v}\0${u}`;
    if (!seen.has(key)) {
      seen.add(key);
      result.push([u, v]);
    }
  }
  return result;
}

/** Find connected components via BFS. */
export function connectedComponents(
  nodeIds: string[],
  edges: EdgePair[],
): string[][] {
  const adj = buildAdj(nodeIds, edges);
  const visited = new Set<string>();
  const components: string[][] = [];

  for (const start of nodeIds) {
    if (visited.has(start)) continue;
    const comp: string[] = [];
    const queue = [start];
    visited.add(start);
    while (queue.length > 0) {
      const u = queue.shift()!;
      comp.push(u);
      for (const v of adj.get(u)!) {
        if (!visited.has(v)) {
          visited.add(v);
          queue.push(v);
        }
      }
    }
    components.push(comp);
  }
  return components;
}

// ── DFS and tree decomposition ─────────────────────────────────────────

interface DFSResult {
  parent: Map<string, string | null>;
  order: string[]; // DFS pre-order
  depth: Map<string, number>;
  treeEdges: EdgePair[];
  backEdges: EdgePair[]; // (descendant, ancestor)
}

function dfs(nodeIds: string[], edges: EdgePair[]): DFSResult {
  const adj = buildAdj(nodeIds, edges);
  const parent = new Map<string, string | null>();
  const order: string[] = [];
  const depth = new Map<string, number>();
  const treeEdges: EdgePair[] = [];
  const backEdges: EdgePair[] = [];
  const visited = new Set<string>();

  function visit(u: string, d: number) {
    visited.add(u);
    depth.set(u, d);
    order.push(u);
    for (const v of adj.get(u)!) {
      if (!visited.has(v)) {
        parent.set(v, u);
        treeEdges.push([u, v]);
        visit(v, d + 1);
      } else if (v !== parent.get(u)) {
        // Back edge — store as (deeper, shallower)
        const du = depth.get(u)!;
        const dv = depth.get(v)!;
        if (du > dv) {
          backEdges.push([u, v]);
        }
      }
    }
  }

  // Start DFS from first node
  if (nodeIds.length > 0) {
    parent.set(nodeIds[0], null);
    visit(nodeIds[0], 0);
  }

  return { parent, order, depth, treeEdges, backEdges };
}

// ── Planar embedding operations ────────────────────────────────────────

/** Create a trivial embedding from a tree (each vertex's children in DFS order). */
function treeEmbedding(nodeIds: string[], treeEdges: EdgePair[]): Embedding {
  const embedding: Embedding = new Map();
  for (const id of nodeIds) embedding.set(id, []);

  // Build parent-child from tree edges
  const children = new Map<string, string[]>();
  const parentMap = new Map<string, string | null>();
  for (const id of nodeIds) {
    children.set(id, []);
    parentMap.set(id, null);
  }

  // Determine root and build tree structure via BFS
  if (treeEdges.length === 0) return embedding;
  const adj = buildAdj(nodeIds, treeEdges);
  const root = nodeIds[0];
  const visited = new Set<string>([root]);
  const queue = [root];
  while (queue.length > 0) {
    const u = queue.shift()!;
    for (const v of adj.get(u)!) {
      if (!visited.has(v)) {
        visited.add(v);
        parentMap.set(v, u);
        children.get(u)!.push(v);
        queue.push(v);
      }
    }
  }

  // Build rotation: parent first (if exists), then children
  for (const id of nodeIds) {
    const p = parentMap.get(id);
    const ch = children.get(id)!;
    if (p != null) {
      embedding.set(id, [p, ...ch]);
    } else {
      embedding.set(id, [...ch]);
    }
  }

  return embedding;
}

/**
 * Walk a face of the embedding starting from half-edge (u → v).
 * Returns the vertex sequence of the face.
 */
function walkFace(
  embedding: Embedding,
  startU: string,
  startV: string,
): string[] {
  const vertices: string[] = [];
  let cur = startU;
  let next = startV;
  let limit = 1000; // safety

  do {
    vertices.push(cur);
    const prev = cur;
    cur = next;
    const neighbors = embedding.get(cur)!;
    const idx = neighbors.indexOf(prev);
    if (idx === -1) return vertices; // broken embedding
    next = neighbors[(idx + 1) % neighbors.length];
    if (--limit <= 0) return vertices; // safety bail
  } while (!(cur === startU && next === startV));

  return vertices;
}

/**
 * Find all faces of the embedding.
 * Each face is a sequence of vertices (walked clockwise).
 */
function allFaces(embedding: Embedding): string[][] {
  const visitedHalfEdges = new Set<string>();
  const faces: string[][] = [];

  for (const [u, neighbors] of embedding) {
    for (const v of neighbors) {
      const heKey = `${u}\0${v}`;
      if (visitedHalfEdges.has(heKey)) continue;

      const face = walkFace(embedding, u, v);
      // Mark all half-edges in this face as visited
      for (let i = 0; i < face.length; i++) {
        const a = face[i];
        const b = face[(i + 1) % face.length];
        visitedHalfEdges.add(`${a}\0${b}`);
      }
      faces.push(face);
    }
  }

  return faces;
}

/**
 * Try to add an edge (u, v) to the embedding.
 * Returns true if successful (edge was added to a face containing both u and v).
 */
function tryAddEdge(embedding: Embedding, u: string, v: string): boolean {
  // Already neighbors?
  if (embedding.get(u)!.includes(v)) return true;

  // Find a face containing both u and v
  const faces = allFaces(embedding);
  for (const face of faces) {
    const uIdx = face.indexOf(u);
    const vIdx = face.indexOf(v);
    if (uIdx === -1 || vIdx === -1) continue;

    // Found a face with both u and v — add the edge here.
    // At u: insert v after u's predecessor in the face walk
    const uPredInFace = face[(uIdx - 1 + face.length) % face.length];
    const uNeighbors = embedding.get(u)!;
    const uPredIdx = uNeighbors.indexOf(uPredInFace);
    uNeighbors.splice(uPredIdx + 1, 0, v);

    // At v: insert u after v's predecessor in the face walk
    const vPredInFace = face[(vIdx - 1 + face.length) % face.length];
    const vNeighbors = embedding.get(v)!;
    const vPredIdx = vNeighbors.indexOf(vPredInFace);
    vNeighbors.splice(vPredIdx + 1, 0, u);

    return true;
  }

  return false; // no face found — edge would break planarity
}

// ── Edge classification ────────────────────────────────────────────────

/**
 * Classify edges as planar or non-planar using greedy incremental embedding.
 * Builds a spanning tree first (always planar), then tries to add back edges.
 */
export function classifyEdges(
  nodeIds: string[],
  edges: EdgePair[],
): {
  planarEdges: EdgePair[];
  nonPlanarEdges: EdgePair[];
  embedding: Embedding;
} {
  if (nodeIds.length <= 2) {
    return {
      planarEdges: edges,
      nonPlanarEdges: [],
      embedding: treeEmbedding(nodeIds, edges),
    };
  }

  // Authoritative planarity verdict first: the Left-Right test is a true
  // planarity test (unlike the greedy fallback below, which is order-dependent
  // and false-positives on planar graphs). When the graph IS planar, LR yields
  // a valid combinatorial embedding directly, so every edge is planar and the
  // graph flows through the real Tutte pipeline instead of the KK fallback.
  const lr = lrPlanarity(nodeIds, edges);
  if (lr.isPlanar && lr.embedding) {
    return { planarEdges: edges, nonPlanarEdges: [], embedding: lr.embedding };
  }

  // Genuinely non-planar: fall back to the greedy incremental embedding to
  // extract a maximal planar subgraph plus the edges that don't fit. (Any such
  // partition is heuristic — for a non-planar graph there is no clean one.)
  const { treeEdges, backEdges, depth } = dfs(nodeIds, edges);
  const embedding = treeEmbedding(nodeIds, treeEdges);
  const planarEdges: EdgePair[] = [...treeEdges];
  const nonPlanarEdges: EdgePair[] = [];

  // Sort back edges: try shorter (shallower) spans first — they constrain less
  const sorted = [...backEdges].sort((a, b) => {
    const spanA = Math.abs(depth.get(a[0])! - depth.get(a[1])!);
    const spanB = Math.abs(depth.get(b[0])! - depth.get(b[1])!);
    return spanA - spanB;
  });

  for (const [u, v] of sorted) {
    if (tryAddEdge(embedding, u, v)) {
      planarEdges.push([u, v]);
    } else {
      nonPlanarEdges.push([u, v]);
    }
  }

  return { planarEdges, nonPlanarEdges, embedding };
}

// ── Biconnected augmentation ───────────────────────────────────────────

/**
 * Find articulation points and augment the graph to be biconnected
 * by adding dummy edges between biconnected components.
 */
function augmentBiconnected(
  nodeIds: string[],
  edges: EdgePair[],
  embedding: Embedding,
): { edges: EdgePair[]; dummyEdges: EdgePair[] } {
  if (nodeIds.length <= 2) return { edges, dummyEdges: [] };

  const adj = buildAdj(nodeIds, edges);
  const dummyEdges: EdgePair[] = [];

  // Find articulation points using Tarjan's algorithm
  const disc = new Map<string, number>();
  const low = new Map<string, number>();
  const parent = new Map<string, string | null>();
  const articulationPoints = new Set<string>();
  let timer = 0;

  function tarjan(u: string) {
    disc.set(u, timer);
    low.set(u, timer);
    timer++;
    let childCount = 0;

    for (const v of adj.get(u)!) {
      if (!disc.has(v)) {
        childCount++;
        parent.set(v, u);
        tarjan(v);
        low.set(u, Math.min(low.get(u)!, low.get(v)!));

        // u is articulation point if:
        // 1. u is root with 2+ children, or
        // 2. u is not root and low[v] >= disc[u]
        if (parent.get(u) === null && childCount > 1) {
          articulationPoints.add(u);
        }
        if (parent.get(u) !== null && low.get(v)! >= disc.get(u)!) {
          articulationPoints.add(u);
        }
      } else if (v !== parent.get(u)) {
        low.set(u, Math.min(low.get(u)!, disc.get(v)!));
      }
    }
  }

  parent.set(nodeIds[0], null);
  tarjan(nodeIds[0]);

  // For each articulation point, connect its subtree leaves to make biconnected
  // Simple strategy: for each articulation point with children in different
  // biconnected components, add a dummy edge between a leaf from each component
  if (articulationPoints.size === 0) return { edges, dummyEdges: [] };

  // Simpler approach: for each leaf pair that shares an articulation point
  // as their only connection, add a dummy edge between them
  const allEdges = [...edges];
  for (const ap of articulationPoints) {
    const neighbors = [...adj.get(ap)!];
    if (neighbors.length < 2) continue;
    // Connect first and last neighbor to form a cycle through the articulation point
    for (let i = 0; i < neighbors.length - 1; i++) {
      const a = neighbors[i];
      const b = neighbors[i + 1];
      if (!adj.get(a)!.has(b)) {
        const dummy: EdgePair = [a, b];
        dummyEdges.push(dummy);
        allEdges.push(dummy);
        adj.get(a)!.add(b);
        adj.get(b)!.add(a);
        tryAddEdge(embedding, a, b);
      }
    }
  }

  return { edges: allEdges, dummyEdges };
}

// ── Biconnected component decomposition ───────────────────────────────

interface Block {
  nodes: string[];
  edges: EdgePair[];
}

/**
 * Decompose a connected graph into biconnected components ("blocks") via
 * Tarjan's algorithm with an edge stack. Each block is a maximal subgraph
 * with no internal articulation point. Blocks are joined through cut
 * vertices (graph-wide articulation points), and each cut vertex appears
 * in every block that touches it.
 *
 * Used to detect pendant subgraphs hanging off cut vertices so they can be
 * laid out and stitched independently — without that, augmentation forces
 * pendant blocks into the main planar embedding, bloating the outer face.
 */
function biconnectedComponents(
  nodeIds: string[],
  edges: EdgePair[],
): { blocks: Block[]; articulationPoints: Set<string> } {
  const adj = buildAdj(nodeIds, edges);
  const disc = new Map<string, number>();
  const low = new Map<string, number>();
  const aps = new Set<string>();
  const blocks: Block[] = [];
  const stack: EdgePair[] = [];
  let timer = 0;

  const popBlockTo = (u: string, v: string): void => {
    const blockEdges: EdgePair[] = [];
    while (stack.length > 0) {
      const e = stack.pop()!;
      blockEdges.push(e);
      if ((e[0] === u && e[1] === v) || (e[0] === v && e[1] === u)) break;
    }
    if (blockEdges.length > 0) {
      blocks.push({ nodes: [...new Set(blockEdges.flat())], edges: blockEdges });
    }
  };

  const popRest = (): void => {
    if (stack.length === 0) return;
    const blockEdges = stack.splice(0);
    blocks.push({ nodes: [...new Set(blockEdges.flat())], edges: blockEdges });
  };

  const visit = (u: string, par: string | null): void => {
    disc.set(u, timer);
    low.set(u, timer);
    timer++;
    let children = 0;

    for (const v of adj.get(u) ?? []) {
      if (!disc.has(v)) {
        children++;
        stack.push([u, v]);
        visit(v, u);
        low.set(u, Math.min(low.get(u)!, low.get(v)!));

        const isRoot = par === null;
        const isCut = !isRoot && low.get(v)! >= disc.get(u)!;
        if (isCut || (isRoot && children > 1)) {
          aps.add(u);
          popBlockTo(u, v);
        }
      } else if (v !== par && disc.get(v)! < disc.get(u)!) {
        stack.push([u, v]);
        low.set(u, Math.min(low.get(u)!, disc.get(v)!));
      }
    }
  };

  for (const id of nodeIds) {
    if (!disc.has(id)) {
      visit(id, null);
      popRest();
    }
  }

  return { blocks, articulationPoints: aps };
}

// ── Triangulation ──────────────────────────────────────────────────────

/**
 * Triangulate by adding dummy edges to all non-triangular faces.
 * Makes biconnected graphs 3-connected (Whitney's theorem),
 * which is required for Tutte's crossing-free guarantee.
 */
function triangulate(embedding: Embedding): EdgePair[] {
  const dummyEdges: EdgePair[] = [];
  let changed = true;

  // Repeat until all faces are triangles (adding an edge can split a face)
  while (changed) {
    changed = false;
    const faces = allFaces(embedding);
    for (const face of faces) {
      if (face.length <= 3) continue;
      // Add a diagonal from face[0] to face[2]
      const u = face[0];
      const v = face[2];
      if (!embedding.get(u)!.includes(v)) {
        if (tryAddEdge(embedding, u, v)) {
          dummyEdges.push([u, v]);
          changed = true;
          break; // restart face enumeration since embedding changed
        }
      }
      // If face[0]-face[2] already exists, try face[1]-face[3]
      if (face.length > 3) {
        const a = face[1];
        const b = face[3];
        if (!embedding.get(a)!.includes(b)) {
          if (tryAddEdge(embedding, a, b)) {
            dummyEdges.push([a, b]);
            changed = true;
            break;
          }
        }
      }
    }
  }

  return dummyEdges;
}

// ── Leaf direction classification ─────────────────────────────────────

type LeafDirection = "in" | "out" | "both";

function buildLeafDirections(
  leaves: { id: string; parent: string }[],
  directedEdges: EdgePair[],
): Map<string, LeafDirection> {
  const dirs = new Map<string, LeafDirection>();
  for (const leaf of leaves) {
    let hasIn = false,
      hasOut = false;
    for (const [src, tgt] of directedEdges) {
      if (src === leaf.id && tgt === leaf.parent) hasIn = true;
      if (src === leaf.parent && tgt === leaf.id) hasOut = true;
    }
    dirs.set(
      leaf.id,
      hasIn && hasOut ? "both" : hasIn ? "in" : "out",
    );
  }
  return dirs;
}

// ── Face-aware leaf placement ─────────────────────────────────────────

/**
 * Two-pass leaf placement for the star / non-planar / multi-block paths
 * (the Tutte path seeds leaves into faces and relaxes them via
 * hierarchicalBalloon instead):
 *   1. assign each leaf to one of its parent's faces (or -1 for outer-contour fan)
 *   2. position leaves on the layout
 *
 * Returns leafId → face index (−1 for outer-contour fan), used for diagnostics
 * and for any later passes that want to reason about which face a leaf landed in.
 */
function placeLeaves(
  positions: Map<string, { col: number; row: number }>,
  leaves: { id: string; parent: string }[],
  faces: string[][],
  outerFace: string[],
  outerContour: Set<string>,
  coreEdges: EdgePair[],
  leafDirections?: Map<string, LeafDirection>,
): Map<string, number> {
  if (leaves.length === 0) return new Map();
  const assignments = assignLeavesToFaces(leaves, faces, outerFace, outerContour);
  positionLeaves(positions, leaves, faces, assignments, coreEdges, leafDirections);
  return assignments;
}

/** Round-robin across parent's interior faces (largest first). */
function assignLeavesToFaces(
  leaves: { id: string; parent: string }[],
  faces: string[][],
  _outerFace: string[],
  _outerContour: Set<string>,
): Map<string, number> {
  const assignments = new Map<string, number>();

  // The outer face is the largest face in the original (pre-augmentation)
  // embedding. We can't reuse the outerContour from Tutte: that's the
  // post-augmentation simple cycle, which excludes vertices that sit on
  // cut-vertex boundary walks (the augmentation "internalises" them, even
  // though topologically they're still on the outer boundary). Detecting
  // the outer face here from `faces` directly handles both cases.
  let outerFi = -1;
  for (let i = 0; i < faces.length; i++) {
    if (faces[i].length < 3) continue;
    if (outerFi === -1 || faces[i].length > faces[outerFi].length) outerFi = i;
  }
  const outerVertices = outerFi >= 0
    ? new Set(faces[outerFi])
    : new Set<string>();

  const parentFaces = new Map<string, number[]>();
  for (let fi = 0; fi < faces.length; fi++) {
    if (fi === outerFi) continue;
    const f = faces[fi];
    if (f.length < 3) continue;
    for (const v of f) {
      if (!parentFaces.has(v)) parentFaces.set(v, []);
      parentFaces.get(v)!.push(fi);
    }
  }

  const byParent = new Map<string, { id: string; parent: string }[]>();
  for (const leaf of leaves) {
    if (!byParent.has(leaf.parent)) byParent.set(leaf.parent, []);
    byParent.get(leaf.parent)!.push(leaf);
  }

  for (const [parentId, parentLeaves] of byParent) {
    const parentOnOuter = outerVertices.has(parentId);
    const available = parentOnOuter ? [] : (parentFaces.get(parentId) ?? []);
    if (available.length === 0) {
      for (const leaf of parentLeaves) assignments.set(leaf.id, -1);
      continue;
    }
    const sorted = [...available].sort((a, b) => faces[b].length - faces[a].length);
    for (let i = 0; i < parentLeaves.length; i++) {
      assignments.set(parentLeaves[i].id, sorted[i % sorted.length]);
    }
  }
  return assignments;
}

// ── Pixel-space box geometry ───────────────────────────────────────────
//
// Grid units are anisotropic (1 col = 300px, 1 row = 180px on the canvas),
// so all clearance math runs on PIXEL coordinates and treats every node as
// its real card rectangle. Margins are folded into the box half-extents —
// horizontal larger than vertical because edge labels need horizontal room —
// so box-box and box-edge checks inherit the anisotropy from one place.
// (Min center-to-center: 300px horizontally, 240px vertically.)

const PX_CELL_W = 300; // must match CELL_W/CELL_H in diagramLayout.ts
const PX_CELL_H = 180;
const CARD_HALF_W = 90; // 180×160 px card (DiagramView CARD_W/CARD_H)
const CARD_HALF_H = 80;
const MARGIN_X = 60; // per-side → 120px horizontal border gap
const MARGIN_Y = 40; // per-side → 80px vertical border gap
const NODE_HALF_W = CARD_HALF_W + MARGIN_X;
const NODE_HALF_H = CARD_HALF_H + MARGIN_Y;
const EDGE_GAP = 30; // desired extra space beyond the inflated box
const EDGE_GAP_MIN = 4; // hard floor beyond the inflated box
const NODE_GAP = 30; // soft zone beyond the inflated extents

/** Extent of the inflated node box in direction (nx, ny) (unit vector). */
function rectExtent(nx: number, ny: number): number {
  return NODE_HALF_W * Math.abs(nx) + NODE_HALF_H * Math.abs(ny);
}

/** Pixel-space working copy of a layout: flat coordinate arrays + edge list. */
interface PxCtx {
  n: number;
  ids: string[];
  idx: Map<string, number>;
  x: number[];
  y: number[];
  es: [number, number][];
  incident: Set<number>[];
}

function buildPxCtx(
  positions: Map<string, { col: number; row: number }>,
  ids: string[],
  edges: EdgePair[],
): PxCtx {
  const idx = new Map<string, number>();
  ids.forEach((id, i) => idx.set(id, i));
  const x = ids.map((id) => (positions.get(id)?.col ?? 0) * PX_CELL_W);
  const y = ids.map((id) => (positions.get(id)?.row ?? 0) * PX_CELL_H);
  const es: [number, number][] = [];
  const seen = new Set<string>();
  for (const [u, v] of edges) {
    const ui = idx.get(u),
      vi = idx.get(v);
    if (ui === undefined || vi === undefined || ui === vi) continue;
    const key = ui < vi ? `${ui},${vi}` : `${vi},${ui}`;
    if (!seen.has(key)) {
      seen.add(key);
      es.push([ui, vi]);
    }
  }
  const incident: Set<number>[] = Array.from(
    { length: ids.length },
    () => new Set(),
  );
  es.forEach(([a, b], ei) => {
    incident[a].add(ei);
    incident[b].add(ei);
  });
  return { n: ids.length, ids, idx, x, y, es, incident };
}

function writeBackPx(
  ctx: PxCtx,
  positions: Map<string, { col: number; row: number }>,
): void {
  for (let i = 0; i < ctx.n; i++)
    positions.set(ctx.ids[i], {
      col: ctx.x[i] / PX_CELL_W,
      row: ctx.y[i] / PX_CELL_H,
    });
}

function pxSegDist(
  px2: number,
  py2: number,
  ax: number,
  ay: number,
  bx: number,
  by: number,
): { d: number; cx: number; cy: number } {
  const dx = bx - ax,
    dy = by - ay;
  const len2 = dx * dx + dy * dy;
  let t = len2 > 0 ? ((px2 - ax) * dx + (py2 - ay) * dy) / len2 : 0;
  t = Math.max(0, Math.min(1, t));
  const cx = ax + t * dx,
    cy = ay + t * dy;
  return { d: Math.hypot(px2 - cx, py2 - cy), cx, cy };
}

/** Does any edge incident to v properly cross any other edge, at current coords? */
function ctxVertexMoveCrosses(ctx: PxCtx, v: number): boolean {
  const { x, y, es, incident } = ctx;
  for (const ei of incident[v]) {
    const [a, b] = es[ei];
    for (let j = 0; j < es.length; j++) {
      if (j === ei) continue;
      const [c, d] = es[j];
      if (c === a || c === b || d === a || d === b) continue;
      const d1x = x[b] - x[a],
        d1y = y[b] - y[a];
      const d2x = x[d] - x[c],
        d2y = y[d] - y[c];
      const cr = d1x * d2y - d1y * d2x;
      if (Math.abs(cr) < 1e-12) continue;
      const t = ((x[c] - x[a]) * d2y - (y[c] - y[a]) * d2x) / cr;
      const u = ((x[c] - x[a]) * d1y - (y[c] - y[a]) * d1x) / cr;
      if (t > 0 && t < 1 && u > 0 && u < 1) return true;
    }
  }
  return false;
}

/** Clearance (px) of box v's inflated border to edge ei. */
function ctxClearance(ctx: PxCtx, v: number, ei: number): number {
  const { x, y, es } = ctx;
  const [a, b] = es[ei];
  const { d, cx: qx, cy: qy } = pxSegDist(x[v], y[v], x[a], y[a], x[b], y[b]);
  let nx = x[v] - qx,
    ny = y[v] - qy;
  const nd = Math.hypot(nx, ny) || 1e-6;
  nx /= nd;
  ny /= nd;
  return d - rectExtent(nx, ny);
}

/** Border gap (px) between the inflated boxes of vertices i and j. */
function ctxBoxGap(ctx: PxCtx, i: number, j: number): number {
  const { x, y } = ctx;
  const dx = x[j] - x[i],
    dy = y[j] - y[i];
  const d = Math.hypot(dx, dy) || 1e-6;
  return d - 2 * rectExtent(dx / d, dy / d);
}

/**
 * Relax a subset of vertices ("movers") against the whole drawing: springs on
 * their edges, repulsion from every box, repulsion + hard projection away
 * from every non-incident edge — each move verified by an exact crossing
 * check (with step-halving), so planarity is never lost. Everything outside
 * the mover set stays fixed. This is relaxation, not placement: movers drift
 * to where there is room.
 *
 * Wedges between edges can only open if the offending edge's endpoints give
 * way, so mover endpoints of a too-close edge receive a counter-push.
 */
function relaxMovers(ctx: PxCtx, movers: number[], iters: number): void {
  if (movers.length === 0) return;
  const { n, x, y, es, incident } = ctx;
  const moverSet = new Set(movers);
  for (let iter = 0; iter < iters; iter++) {
    const fx = new Map<number, number>();
    const fy = new Map<number, number>();
    for (const v of movers) {
      fx.set(v, 0);
      fy.set(v, 0);
    }
    // springs on incident edges (only mover ends move)
    for (const [a, b] of es) {
      const am = moverSet.has(a),
        bm = moverSet.has(b);
      if (!am && !bm) continue;
      const dx = x[b] - x[a],
        dy = y[b] - y[a];
      const d = Math.hypot(dx, dy) || 1e-6;
      const f = (0.12 * (d - 620)) / d;
      if (am) {
        fx.set(a, fx.get(a)! + dx * f);
        fy.set(a, fy.get(a)! + dy * f);
      }
      if (bm) {
        fx.set(b, fx.get(b)! - dx * f);
        fy.set(b, fy.get(b)! - dy * f);
      }
    }
    for (const v of movers) {
      // box repulsion (movers vs everything)
      for (let w = 0; w < n; w++) {
        if (w === v) continue;
        const dx = x[v] - x[w],
          dy = y[v] - y[w];
        const d = Math.hypot(dx, dy) || 1e-6;
        const nx = dx / d,
          ny = dy / d;
        const gap = d - 2 * rectExtent(nx, ny);
        if (gap > NODE_GAP * 2) continue;
        const f = (1.2 * Math.max(0.05, NODE_GAP * 2 - gap)) / (NODE_GAP * 2);
        fx.set(v, fx.get(v)! + nx * f * 40);
        fy.set(v, fy.get(v)! + ny * f * 40);
      }
      // edge clearance repulsion (movers vs all non-incident edges)
      for (let ei = 0; ei < es.length; ei++) {
        if (incident[v].has(ei)) continue;
        const [a, b] = es[ei];
        const { d, cx: qx, cy: qy } = pxSegDist(
          x[v],
          y[v],
          x[a],
          y[a],
          x[b],
          y[b],
        );
        let nx = x[v] - qx,
          ny = y[v] - qy;
        const nd = Math.hypot(nx, ny) || 1e-6;
        nx /= nd;
        ny /= nd;
        const clearance = d - rectExtent(nx, ny);
        if (clearance >= EDGE_GAP * 1.5) continue;
        const f =
          (1.4 * Math.max(0.1, EDGE_GAP * 1.5 - clearance)) / (EDGE_GAP * 1.5);
        fx.set(v, fx.get(v)! + nx * f * 40);
        fy.set(v, fy.get(v)! + ny * f * 40);
        const wgt = clearance < 0 ? 30 : 15;
        if (moverSet.has(a)) {
          fx.set(a, fx.get(a)! - nx * f * wgt);
          fy.set(a, fy.get(a)! - ny * f * wgt);
        }
        if (moverSet.has(b)) {
          fx.set(b, fx.get(b)! - nx * f * wgt);
          fy.set(b, fy.get(b)! - ny * f * wgt);
        }
      }
    }
    let moved = 0;
    for (const v of movers) {
      let mx = fx.get(v)!,
        my = fy.get(v)!;
      const m = Math.hypot(mx, my);
      const lim = 50;
      if (m > lim && m > 0) {
        mx *= lim / m;
        my *= lim / m;
      }
      for (let h = 0; h < 4; h++) {
        x[v] += mx;
        y[v] += my;
        if (!ctxVertexMoveCrosses(ctx, v)) {
          moved = Math.max(moved, Math.hypot(mx, my));
          break;
        }
        x[v] -= mx;
        y[v] -= my;
        mx /= 2;
        my /= 2;
      }
    }
    // clearance constraint projection for movers
    for (const v of movers) {
      for (let ei = 0; ei < es.length; ei++) {
        if (incident[v].has(ei)) continue;
        const [a, b] = es[ei];
        const { d, cx: qx, cy: qy } = pxSegDist(
          x[v],
          y[v],
          x[a],
          y[a],
          x[b],
          y[b],
        );
        let nx = x[v] - qx,
          ny = y[v] - qy;
        const nd = Math.hypot(nx, ny) || 1e-6;
        nx /= nd;
        ny /= nd;
        const clearance = d - rectExtent(nx, ny);
        if (clearance >= EDGE_GAP_MIN) continue;
        const deficit = EDGE_GAP_MIN - clearance;
        x[v] += nx * deficit * 0.6;
        y[v] += ny * deficit * 0.6;
        if (ctxVertexMoveCrosses(ctx, v)) {
          x[v] -= nx * deficit * 0.6;
          y[v] -= ny * deficit * 0.6;
        }
      }
    }
    if (moved < 0.1) break;
  }
}

/**
 * Backstop: fix absolute-space clearance deficits by uniform scale-up — but
 * only while scaling actually helps (a wedge is a shape problem: scaling a
 * near-zero clearance keeps it near zero, so bail on stalls).
 */
function backstopScale(
  positions: Map<string, { col: number; row: number }>,
  nodeIds: string[],
  edges: EdgePair[],
): void {
  let prev = -Infinity;
  for (let pass = 0; pass < 6; pass++) {
    const worst = minRectEdgeClearance(positions, nodeIds, edges);
    if (worst >= EDGE_GAP_MIN) break;
    if (worst <= prev + 2) break;
    prev = worst;
    let cx = 0,
      cy = 0;
    for (const p of positions.values()) {
      cx += p.col;
      cy += p.row;
    }
    cx /= positions.size;
    cy /= positions.size;
    for (const p of positions.values()) {
      p.col = cx + (p.col - cx) * 1.2;
      p.row = cy + (p.row - cy) * 1.2;
    }
  }
}

/** Min clearance (px) between any inflated box border and a non-incident edge. */
function minRectEdgeClearance(
  positions: Map<string, { col: number; row: number }>,
  nodeIds: string[],
  edges: EdgePair[],
): number {
  let worst = Infinity;
  const px = new Map(
    [...positions].map(([id, p]) => [
      id,
      { x: p.col * PX_CELL_W, y: p.row * PX_CELL_H },
    ]),
  );
  for (const id of nodeIds) {
    const p = px.get(id);
    if (!p) continue;
    for (const [u, v] of edges) {
      if (u === id || v === id) continue;
      const pu = px.get(u),
        pv = px.get(v);
      if (!pu || !pv) continue;
      const dx = pv.x - pu.x,
        dy = pv.y - pu.y;
      const len2 = dx * dx + dy * dy;
      let t = len2 > 0 ? ((p.x - pu.x) * dx + (p.y - pu.y) * dy) / len2 : 0;
      t = Math.max(0, Math.min(1, t));
      const cx = pu.x + t * dx,
        cy = pu.y + t * dy;
      let nx = p.x - cx,
        ny = p.y - cy;
      const nd = Math.hypot(nx, ny) || 1e-6;
      nx /= nd;
      ny /= nd;
      worst = Math.min(worst, nd - rectExtent(nx, ny));
    }
  }
  return worst;
}

// ── Hierarchical minimal ballooning ────────────────────────────────────
//
// Replaces both the old global min-edge rescale (which blew the whole graph
// up to fix one cramped spot) and the force-only balloonRelax (which had no
// planarity guarantee). The control structure:
//
//   Loop: find the worst cramped box (too close to a non-incident edge, or
//   overlapping another box). Take its MINIMAL containing region — the union
//   of real faces incident to it (for a leaf: the face it lives in). First
//   RELAX the region's interior inside the fixed boundary (springs +
//   clearance projection + exact crossing checks — relaxation, not
//   placement). If the region genuinely lacks room, DILATE it rigidly about
//   its centroid — checked against the untouched outside; if blocked, ASCEND
//   to the parent region (add all faces incident to the boundary) and retry.
//   Then re-check everything and iterate.
//
// Room flows top-down (a region only grows into slack its parent already
// has, and the need only propagates outward when a level is genuinely
// blocked), content relaxes bottom-up into the granted room. The far field —
// in particular the outer contour — never moves unless the need truly
// reaches it, which is what preserves the Tutte look. Rigid dilation is
// shape-preserving, so stall detection stops the loop when the remaining
// violations are wedges that scaling cannot fix (the final backstop in the
// caller handles pure absolute-space deficits).

interface BalloonRegion {
  interior: Set<string>;
  boundary: string[]; // ordered cycle
  memberFaces: Set<number>;
}

function hierarchicalBalloon(
  positions: Map<string, { col: number; row: number }>,
  allIds: string[],
  allEdges: EdgePair[],
  faces: string[][],
  outerFi: number,
  leafFace: Map<string, number>, // leafId → face index it inhabits (−1: outer fan)
  leafParent: Map<string, string>,
): void {
  const ctx = buildPxCtx(positions, allIds, allEdges);
  const { n, idx, x, y, es, incident } = ctx;
  if (n <= 2) return;

  const vertexMoveCrosses = (v: number) => ctxVertexMoveCrosses(ctx, v);
  const clearanceOf = (v: number, ei: number) => ctxClearance(ctx, v, ei);
  const boxGap = (i: number, j: number) => ctxBoxGap(ctx, i, j);

  interface Violation {
    v: number;
    kind: "edge" | "box";
    other: number;
    amount: number;
  }
  function worstViolation(): Violation | null {
    let worst: Violation | null = null;
    for (let v = 0; v < n; v++) {
      for (let ei = 0; ei < es.length; ei++) {
        if (incident[v].has(ei)) continue;
        const c = clearanceOf(v, ei);
        if (c < EDGE_GAP_MIN && (worst === null || c < worst.amount))
          worst = { v, kind: "edge", other: ei, amount: c };
      }
      for (let w = v + 1; w < n; w++) {
        const g = boxGap(v, w) - NODE_GAP;
        if (g < 0) {
          const score = g - EDGE_GAP_MIN; // comparable scale to edge clearance
          if (worst === null || score < worst.amount)
            worst = { v, kind: "box", other: w, amount: score };
        }
      }
    }
    return worst;
  }

  // ── region machinery (faces = REAL pre-augmentation face cycles) ─────
  function regionFromFaces(memberFaces: Set<number>): BalloonRegion | null {
    // boundary = directed face edges whose reverse is not present
    const dir = new Set<string>();
    for (const fi of memberFaces) {
      const f = faces[fi];
      for (let i = 0; i < f.length; i++)
        dir.add(`${f[i]}>${f[(i + 1) % f.length]}`);
    }
    const succ = new Map<string, string[]>();
    let startTail: string | null = null;
    for (const de of dir) {
      const [tail, head] = de.split(">");
      if (dir.has(`${head}>${tail}`)) continue; // internal (shared) edge
      if (!succ.has(tail)) succ.set(tail, []);
      succ.get(tail)!.push(head);
      startTail = tail;
    }
    if (!startTail) return null;
    const boundary: string[] = [];
    let cur = startTail;
    const usedHeads = new Set<string>();
    for (let guard = 0; guard < 500; guard++) {
      boundary.push(cur);
      const outs = succ.get(cur) ?? [];
      let next: string | null = null;
      for (const h of outs) {
        const k = `${cur}>${h}`;
        if (!usedHeads.has(k)) {
          usedHeads.add(k);
          next = h;
          break;
        }
      }
      if (next === null) break;
      cur = next;
      if (cur === startTail) break;
    }
    const boundarySet = new Set(boundary);
    const interior = new Set<string>();
    for (const fi of memberFaces)
      for (const vtx of faces[fi]) if (!boundarySet.has(vtx)) interior.add(vtx);
    // leaves are interior inhabitants wherever their parent is part of the
    // region (by face assignment when known, else by parent membership)
    for (const [leafId, fi] of leafFace) {
      const par = leafParent.get(leafId);
      if (
        memberFaces.has(fi) ||
        (par && (interior.has(par) || boundarySet.has(par)))
      )
        interior.add(leafId);
    }
    return { interior, boundary, memberFaces };
  }
  function minimalRegionFor(id: string): BalloonRegion | null {
    const member = new Set<number>();
    if (leafFace.has(id)) {
      const fi = leafFace.get(id)!;
      if (fi >= 0 && fi !== outerFi) member.add(fi);
      else {
        // outer-fan leaf: region = faces incident to its parent
        const par = leafParent.get(id);
        for (let fi2 = 0; fi2 < faces.length; fi2++)
          if (fi2 !== outerFi && par && faces[fi2].includes(par))
            member.add(fi2);
      }
    } else {
      for (let fi = 0; fi < faces.length; fi++)
        if (fi !== outerFi && faces[fi].includes(id)) member.add(fi);
    }
    if (member.size === 0) return null;
    return regionFromFaces(member);
  }
  function parentRegion(r: BalloonRegion): BalloonRegion | null {
    const member = new Set(r.memberFaces);
    const before = member.size;
    for (const b of r.boundary)
      for (let fi = 0; fi < faces.length; fi++)
        if (fi !== outerFi && faces[fi].includes(b)) member.add(fi);
    if (member.size === before) return null; // top of the hierarchy
    return regionFromFaces(member);
  }

  // ── relax: interior of region only, boundary + outside fixed ─────────
  function relax(region: BalloonRegion, iters: number): void {
    const movers = [...region.interior]
      .map((id) => idx.get(id)!)
      .filter((i) => i !== undefined);
    relaxMovers(ctx, movers, iters);
  }

  // ── dilate region rigidly about boundary centroid; blocked → false ───
  function tryDilate(region: BalloonRegion, s: number): boolean {
    const members = new Set<number>(
      [
        ...region.boundary.map((id) => idx.get(id)!),
        ...[...region.interior].map((id) => idx.get(id)!),
      ].filter((i) => i !== undefined) as number[],
    );
    let cx2 = 0,
      cy2 = 0,
      cnt = 0;
    for (const b of region.boundary) {
      const i = idx.get(b);
      if (i === undefined) continue;
      cx2 += x[i];
      cy2 += y[i];
      cnt++;
    }
    if (cnt === 0) return false;
    cx2 /= cnt;
    cy2 /= cnt;
    const sx = x.slice(),
      sy = y.slice();
    for (const i of members) {
      x[i] = cx2 + (x[i] - cx2) * s;
      y[i] = cy2 + (y[i] - cy2) * s;
    }
    const revert = () => {
      for (let k = 0; k < n; k++) {
        x[k] = sx[k];
        y[k] = sy[k];
      }
    };
    // exact planarity check for all moved vertices
    for (const i of members) {
      if (vertexMoveCrosses(i)) {
        revert();
        return false;
      }
    }
    // slack check: moved boxes must keep floor clearance to OUTSIDE edges,
    // and outside boxes to moved edges — otherwise we did not have room.
    for (const i of members) {
      for (let ei = 0; ei < es.length; ei++) {
        if (incident[i].has(ei)) continue;
        const [a, b] = es[ei];
        if (members.has(a) && members.has(b)) continue; // internal: relax's job
        if (clearanceOf(i, ei) < EDGE_GAP_MIN) {
          revert();
          return false;
        }
      }
    }
    for (let v2 = 0; v2 < n; v2++) {
      if (members.has(v2)) continue;
      for (let ei = 0; ei < es.length; ei++) {
        const [a, b] = es[ei];
        if (!members.has(a) && !members.has(b)) continue;
        if (incident[v2].has(ei)) continue;
        if (clearanceOf(v2, ei) < EDGE_GAP_MIN) {
          revert();
          return false;
        }
      }
    }
    return true;
  }

  // ── main loop ─────────────────────────────────────────────────────
  let prevWorst = -Infinity;
  let stall = 0;
  for (let pass = 0; pass < 80; pass++) {
    const viol = worstViolation();
    if (!viol) break;
    // stall detection: rigid dilation is shape-preserving, so violations that
    // scaling can't fix (wedges, slivers) must not drive unbounded growth
    if (viol.amount <= prevWorst + 1) {
      if (++stall >= 4) break;
    } else stall = 0;
    prevWorst = viol.amount;

    const id = allIds[viol.v];
    let region = minimalRegionFor(id);
    if (!region) break;
    let handled = false;
    for (let level = 0; level < 8 && !handled; level++) {
      relax(region, 60);
      const still = worstViolation();
      if (
        !still ||
        (still.amount >= viol.amount - 1e-6 &&
          !(still.v === viol.v && still.kind === viol.kind))
      ) {
        handled = true;
        break;
      }
      if (still.amount >= 0) {
        handled = true;
        break;
      }
      // need room: dilate, growth sized to worst deficit vs region radius
      let rad = 0;
      let bcx = 0,
        bcy = 0,
        bcnt = 0;
      for (const b of region.boundary) {
        const i = idx.get(b);
        if (i === undefined) continue;
        bcx += x[i];
        bcy += y[i];
        bcnt++;
      }
      if (bcnt > 0) {
        bcx /= bcnt;
        bcy /= bcnt;
      }
      for (const b of region.boundary) {
        const i = idx.get(b);
        if (i === undefined) continue;
        rad = Math.max(rad, Math.hypot(x[i] - bcx, y[i] - bcy));
      }
      const need = Math.max(80, -still.amount * 2);
      const s = Math.min(1.6, 1 + need / Math.max(rad, 200));
      if (tryDilate(region, s)) {
        relax(region, 60);
        const after = worstViolation();
        if (!after || after.amount >= 0 || after.v !== viol.v) {
          handled = true;
          break;
        }
        continue; // same level: dilate more if still short of room
      }
      // blocked → ascend; residue at the top is the backstop's job
      const parent = parentRegion(region);
      if (!parent) break;
      region = parent;
    }
  }
  writeBackPx(ctx, positions);
}

/**
 * Seed each leaf INSIDE its assigned face, close to its parent — deliberately
 * conservative: within the parent's clearance disk (a segment from the parent
 * shorter than the parent's distance to any non-incident edge cannot cross
 * anything). Real placement is the balloon relaxation's job; this only
 * provides a crossing-free start.
 */
function seedLeaves(
  positions: Map<string, { col: number; row: number }>,
  leaves: { id: string; parent: string }[],
  assignments: Map<string, number>,
  faces: string[][],
  coreEdges: EdgePair[],
): void {
  if (leaves.length === 0) return;
  const toPx = (p: { col: number; row: number }) => ({
    col: p.col * PX_CELL_W,
    row: p.row * PX_CELL_H,
  });
  const segDist = (
    p: { col: number; row: number },
    a: { col: number; row: number },
    b: { col: number; row: number },
  ) => {
    const dx = b.col - a.col,
      dy = b.row - a.row;
    const len2 = dx * dx + dy * dy;
    let t = len2 > 0 ? ((p.col - a.col) * dx + (p.row - a.row) * dy) / len2 : 0;
    t = Math.max(0, Math.min(1, t));
    return Math.hypot(p.col - (a.col + t * dx), p.row - (a.row + t * dy));
  };

  // group leaves by (parent, face) for fanning
  const groups = new Map<string, { id: string; parent: string }[]>();
  for (const leaf of leaves) {
    const key = `${leaf.parent}\0${assignments.get(leaf.id) ?? -1}`;
    if (!groups.has(key)) groups.set(key, []);
    groups.get(key)!.push(leaf);
  }
  // graph centroid for outer-fan direction
  let gcx = 0,
    gcy = 0;
  for (const p of positions.values()) {
    gcx += p.col;
    gcy += p.row;
  }
  gcx /= positions.size;
  gcy /= positions.size;

  for (const [key, group] of groups) {
    const parentId = group[0].parent;
    const fi = Number(key.split("\0")[1]);
    const pp = positions.get(parentId);
    if (!pp) {
      for (const l of group) positions.set(l.id, { col: 0, row: 0 });
      continue;
    }
    const ppx = toPx(pp);
    let clearPx = Infinity;
    for (const [u, v] of coreEdges) {
      if (u === parentId || v === parentId) continue;
      const pu = positions.get(u),
        pv = positions.get(v);
      if (!pu || !pv) continue;
      clearPx = Math.min(clearPx, segDist(ppx, toPx(pu), toPx(pv)));
    }
    const safeDistPx = Math.min(540, clearPx === Infinity ? 540 : clearPx * 0.45);
    // direction: face centroid (interior) or outward from graph centroid (fan)
    let baseAngle: number;
    if (fi >= 0 && faces[fi]) {
      const face = faces[fi].filter((v) => v !== parentId);
      let ox = 0,
        oy = 0,
        cnt = 0;
      for (const v of face) {
        const p = positions.get(v);
        if (p) {
          ox += p.col;
          oy += p.row;
          cnt++;
        }
      }
      if (cnt > 0) {
        ox /= cnt;
        oy /= cnt;
      }
      baseAngle = Math.atan2(oy - pp.row, ox - pp.col);
    } else {
      baseAngle = Math.atan2(pp.row - gcy, pp.col - gcx);
      if (!isFinite(baseAngle)) baseAngle = 0;
    }
    const N = group.length;
    const spread = Math.min(Math.PI * 0.5, 0.5 * (N - 1));
    for (let i = 0; i < N; i++) {
      const t = N === 1 ? 0 : (i / (N - 1) - 0.5) * spread;
      // stagger distances slightly so siblings never coincide
      const dPx = safeDistPx * (0.7 + (0.3 * ((i % 3) + 1)) / 3);
      positions.set(group[i].id, {
        col: pp.col + (dPx * Math.cos(baseAngle + t)) / PX_CELL_W,
        row: pp.row + (dPx * Math.sin(baseAngle + t)) / PX_CELL_H,
      });
    }
  }
}

const DIR_ORDER: Record<string, number> = { in: 0, both: 1, out: 2 };

function directionSort<T extends { id: string }>(
  leaves: T[],
  leafDirections?: Map<string, LeafDirection>,
): T[] {
  if (!leafDirections || leaves.length <= 2) return leaves;
  return [...leaves].sort(
    (a, b) =>
      (DIR_ORDER[leafDirections.get(a.id) ?? "out"] ?? 2) -
      (DIR_ORDER[leafDirections.get(b.id) ?? "out"] ?? 2),
  );
}

/**
 * Sector-based placement for star graphs (hub at centroid, full circle).
 * Groups leaves by edge direction (in/out/both), places each group in its
 * own angular sector with gaps between sectors.
 * Returns true if placement was done, false to fall back to even distribution.
 */
function sectorPlace(
  positions: Map<string, { col: number; row: number }>,
  hub: { col: number; row: number },
  leaves: { id: string; parent: string }[],
  N: number,
  leafDirections?: Map<string, LeafDirection>,
): boolean {
  if (!leafDirections || N <= 2) return false;

  const sorted = directionSort(leaves, leafDirections);

  // Count distinct direction groups in the circular arrangement
  const groups = new Set<string>();
  for (const l of sorted) groups.add(leafDirections.get(l.id) ?? "out");
  if (groups.size <= 1) return false;

  // In a circle of N items with G groups, there are G direction boundaries
  // (including the wrap-around between last and first).
  const numGaps = groups.size;
  const GAP_SLOTS = 2;
  const s = (2 * Math.PI) / (N + numGaps * GAP_SLOTS);
  const MIN_CHORD = 1.4;
  const dist = Math.max(1.8, MIN_CHORD / (2 * Math.sin(s / 2)));

  // Center the first direction group at its preferred angle
  const SECTOR_CENTERS: Record<string, number> = {
    in: -Math.PI / 2,
    both: 0,
    out: Math.PI / 2,
  };
  const firstDir = leafDirections.get(sorted[0].id) ?? "out";
  let firstGroupSize = 0;
  for (const l of sorted) {
    if ((leafDirections.get(l.id) ?? "out") === firstDir) firstGroupSize++;
    else break;
  }
  let angle =
    (SECTOR_CENTERS[firstDir] ?? -Math.PI / 2) -
    ((firstGroupSize - 1) * s) / 2;

  for (let i = 0; i < sorted.length; i++) {
    positions.set(sorted[i].id, {
      col: hub.col + dist * Math.cos(angle),
      row: hub.row + dist * Math.sin(angle),
    });
    if (i < sorted.length - 1) {
      const curDir = leafDirections.get(sorted[i].id) ?? "out";
      const nextDir = leafDirections.get(sorted[i + 1].id) ?? "out";
      angle += s * (curDir !== nextDir ? 1 + GAP_SLOTS : 1);
    }
  }
  return true;
}

/** Final leaf positioning, after any face expansion. */
function positionLeaves(
  positions: Map<string, { col: number; row: number }>,
  leaves: { id: string; parent: string }[],
  faces: string[][],
  assignments: Map<string, number>,
  coreEdges: EdgePair[],
  leafDirections?: Map<string, LeafDirection>,
): void {
  const byParent = new Map<string, { id: string; parent: string }[]>();
  for (const leaf of leaves) {
    if (!byParent.has(leaf.parent)) byParent.set(leaf.parent, []);
    byParent.get(leaf.parent)!.push(leaf);
  }

  // Graph centroid for outer-contour fanning
  let gcx = 0, gcy = 0, gcN = 0;
  for (const p of positions.values()) { gcx += p.col; gcy += p.row; gcN++; }
  if (gcN > 0) { gcx /= gcN; gcy /= gcN; }

  // Star / non-planar paths pass `faces=[]`; Tutte passes the actual face list.
  // The sector/neighbor-gap fan heuristics ignore faces, so on Tutte graphs they
  // can place leaves through interior faces and cause crossings. Restrict them
  // to star mode and fall back to a plain outward fan for Tutte.
  const isStarMode = faces.length === 0;

  for (const [parentId, parentLeaves] of byParent) {
    const pp = positions.get(parentId);
    if (!pp) {
      for (const leaf of parentLeaves) positions.set(leaf.id, { col: 0, row: 0 });
      continue;
    }

    const firstFi = assignments.get(parentLeaves[0].id);
    if (firstFi === -1 || firstFi === undefined) {
      // Outer-contour fan
      const outDx = pp.col - gcx;
      const outDy = pp.row - gcy;
      const outLen = Math.sqrt(outDx * outDx + outDy * outDy);
      const N = parentLeaves.length;
      const isFullCircle = outLen < 0.001;
      const fanSpread = Math.PI * 0.4;
      const angGap = isFullCircle
        ? (2 * Math.PI) / Math.max(N, 3)
        : N > 1 ? fanSpread / (N - 1) : Math.PI / 2;
      const MIN_CHORD = 1.4;
      const dist = Math.max(1.8, MIN_CHORD / (2 * Math.sin(angGap / 2)));

      if (isFullCircle) {
        if (isStarMode && sectorPlace(positions, pp, parentLeaves, N, leafDirections)) {
          // placed by sector logic
        } else {
          for (let i = 0; i < N; i++) {
            const angle = (2 * Math.PI * i) / N - Math.PI / 2;
            positions.set(parentLeaves[i].id, {
              col: pp.col + dist * Math.cos(angle),
              row: pp.row + dist * Math.sin(angle),
            });
          }
        }
      } else {
        const ordered = directionSort(parentLeaves, leafDirections);
        const baseAngle = Math.atan2(outDy, outDx);

        let placed = false;
        if (isStarMode && N >= 3 && coreEdges.length > 0) {
          const coreNeighborIds: string[] = [];
          for (const [u, v] of coreEdges) {
            if (u === parentId) coreNeighborIds.push(v);
            else if (v === parentId) coreNeighborIds.push(u);
          }

          const neighborAngles = coreNeighborIds
            .map((nid) => {
              const np = positions.get(nid);
              if (!np) return null;
              return Math.atan2(np.row - pp.row, np.col - pp.col);
            })
            .filter((a): a is number => a !== null)
            .sort((a, b) => a - b);

          if (neighborAngles.length >= 2) {
            let maxGap = 0;
            let gapStart = 0;
            for (let i = 0; i < neighborAngles.length; i++) {
              const next = (i + 1) % neighborAngles.length;
              let gap = neighborAngles[next] - neighborAngles[i];
              if (next === 0) gap += 2 * Math.PI;
              if (gap > maxGap) {
                maxGap = gap;
                gapStart = neighborAngles[i];
              }
            }

            const MARGIN = Math.PI / 12;
            const safeArc = Math.min(Math.PI, Math.max(maxGap - 2 * MARGIN, Math.PI / 6));
            const safeMid = gapStart + maxGap / 2;
            const arcGap = N > 1 ? safeArc / (N - 1) : 0;

            let clearDist = 1.8;
            const arcHalf = safeArc / 2;
            for (const [nid, np] of positions) {
              if (nid === parentId) continue;
              const dx = np.col - pp.col;
              const dy = np.row - pp.row;
              let angleDiff = Math.atan2(dy, dx) - safeMid;
              while (angleDiff > Math.PI) angleDiff -= 2 * Math.PI;
              while (angleDiff < -Math.PI) angleDiff += 2 * Math.PI;
              if (Math.abs(angleDiff) < arcHalf + MARGIN) {
                clearDist = Math.max(clearDist, Math.hypot(dx, dy) + 1.5);
              }
            }
            const arcDist = Math.max(
              clearDist,
              MIN_CHORD / (2 * Math.sin(Math.min(arcGap, Math.PI) / 2)),
            );

            for (let i = 0; i < N; i++) {
              const t = N === 1 ? 0 : (i / (N - 1) - 0.5) * safeArc;
              positions.set(ordered[i].id, {
                col: pp.col + arcDist * Math.cos(safeMid + t),
                row: pp.row + arcDist * Math.sin(safeMid + t),
              });
            }
            placed = true;
          }
        }

        if (!placed) {
          // Within the outward semicircle (baseAngle ± π/2 — anything wider
          // would wrap back into the graph interior), enumerate unblocked
          // sub-arcs. Distribute leaves across all gaps proportional to gap
          // size — picking only the single largest gap forces every leaf
          // through one side of an obstacle, which blows up the leaf
          // distance when the obstacle sits near baseAngle.
          const HALF_RANGE = Math.PI / 2;
          const MARGIN = Math.PI / 12;
          const blocked: [number, number][] = [];
          for (const [nid, np] of positions) {
            if (nid === parentId) continue;
            let rel = Math.atan2(np.row - pp.row, np.col - pp.col) - baseAngle;
            while (rel > Math.PI) rel -= 2 * Math.PI;
            while (rel < -Math.PI) rel += 2 * Math.PI;
            const lo = Math.max(-HALF_RANGE, rel - MARGIN);
            const hi = Math.min(HALF_RANGE, rel + MARGIN);
            if (lo < hi) blocked.push([lo, hi]);
          }
          blocked.sort((a, b) => a[0] - b[0]);
          const merged: [number, number][] = [];
          for (const iv of blocked) {
            const last = merged[merged.length - 1];
            if (last && iv[0] <= last[1]) last[1] = Math.max(last[1], iv[1]);
            else merged.push([iv[0], iv[1]]);
          }
          const gaps: { lo: number; hi: number }[] = [];
          let cursor = -HALF_RANGE;
          for (const [lo, hi] of merged) {
            if (lo > cursor) gaps.push({ lo: cursor, hi: lo });
            cursor = Math.max(cursor, hi);
          }
          if (cursor < HALF_RANGE) gaps.push({ lo: cursor, hi: HALF_RANGE });

          const totalGap = gaps.reduce((s, g) => s + (g.hi - g.lo), 0);

          // Allocate leaf count per gap proportional to size, with largest
          // remainders winning the rounding.
          const allocations: number[] = [];
          if (gaps.length === 0 || totalGap <= 0) {
            // Fully blocked — fall back to placing the whole fan along
            // baseAngle. Edges may pass close to obstacles but at least leaves
            // don't disappear.
            gaps.push({ lo: -HALF_RANGE, hi: HALF_RANGE });
            allocations.push(N);
          } else {
            const exact = gaps.map((g) => (N * (g.hi - g.lo)) / totalGap);
            const floored = exact.map((x) => Math.floor(x));
            let remaining = N - floored.reduce((s, x) => s + x, 0);
            const remainders = exact
              .map((x, i) => ({ i, frac: x - Math.floor(x) }))
              .sort((a, b) => b.frac - a.frac);
            for (let k = 0; k < remainders.length && remaining > 0; k++) {
              floored[remainders[k].i]++;
              remaining--;
            }
            for (const v of floored) allocations.push(v);
          }

          const TARGET_DIST = 1.8;
          let placedIdx = 0;
          for (let gi = 0; gi < gaps.length; gi++) {
            const gapN = allocations[gi];
            if (gapN === 0) continue;
            const gap = gaps[gi];
            const gapSize = gap.hi - gap.lo;
            const gapCenter = (gap.lo + gap.hi) / 2;

            // Spread the gapN leaves across this gap, capped at the gap width.
            const minArc = gapN > 1
              ? 2 * (gapN - 1) * Math.asin(
                  Math.min(0.9, MIN_CHORD / (2 * TARGET_DIST)),
                )
              : 0;
            const arc = gapN > 1
              ? Math.min(gapSize, Math.max(fanSpread, minArc))
              : 0;
            const arcDist = gapN > 1
              ? Math.max(
                  TARGET_DIST,
                  MIN_CHORD / (2 * Math.sin(Math.min(arc / (gapN - 1), Math.PI) / 2)),
                )
              : TARGET_DIST;

            for (let i = 0; i < gapN; i++) {
              const t = gapN === 1 ? 0 : (i / (gapN - 1) - 0.5) * arc;
              positions.set(ordered[placedIdx + i].id, {
                col: pp.col + arcDist * Math.cos(baseAngle + gapCenter + t),
                row: pp.row + arcDist * Math.sin(baseAngle + gapCenter + t),
              });
            }
            placedIdx += gapN;
          }
        }
      }
      continue;
    }

    // In-face placement: group by face, place along parent→opposite-side direction
    const byFace = new Map<number, { id: string; parent: string }[]>();
    for (const leaf of parentLeaves) {
      const fi = assignments.get(leaf.id)!;
      if (!byFace.has(fi)) byFace.set(fi, []);
      byFace.get(fi)!.push(leaf);
    }

    for (const [fi, group] of byFace) {
      const face = faces[fi];
      const opposite = face.filter((v) => v !== parentId);
      if (opposite.length === 0) continue;
      let ox = 0, oy = 0;
      for (const v of opposite) {
        const p = positions.get(v)!;
        ox += p.col; oy += p.row;
      }
      ox /= opposite.length; oy /= opposite.length;
      const dx = ox - pp.col;
      const dy = oy - pp.row;
      const dLen = Math.sqrt(dx * dx + dy * dy);
      if (dLen < 0.001) continue;

      const baseAngle = Math.atan2(dy, dx);
      const N = group.length;
      // Leaf distance: match the outer-contour fan's 1.8 unit minimum so
      // inner-face leaves aren't visibly tighter than star-graph spokes.
      const NODE_MARGIN = 1.8;
      const leafDist = Math.max(NODE_MARGIN, Math.min(dLen * 0.8, 2.2));
      const MIN_CHORD = 1.4;
      const halfGap = Math.asin(Math.min(0.9, MIN_CHORD / (2 * leafDist)));
      const spread = Math.min(Math.PI * 0.6, halfGap * 2 * Math.max(N - 1, 0));

      for (let j = 0; j < N; j++) {
        const t = N === 1 ? 0 : (j / (N - 1) - 0.5) * spread;
        const angle = baseAngle + t;
        positions.set(group[j].id, {
          col: pp.col + leafDist * Math.cos(angle),
          row: pp.row + leafDist * Math.sin(angle),
        });
      }
    }
  }
}

// ── Tutte's barycentric embedding ─────────────────────────────────────

/**
 * Tutte's embedding on a triangulated (3-connected) graph.
 *
 * Outer face vertices placed on a circle. Interior vertices iteratively
 * moved to the barycenter (average) of their neighbors. Guaranteed
 * crossing-free for 3-connected planar graphs.
 *
 * Produces balanced, compact layouts where hub nodes naturally end up
 * central (pulled by many neighbors).
 */
function tuttePlace(
  embedding: Embedding,
  outerFaceHint?: string[],
): {
  positions: Map<string, { col: number; row: number }>;
  outerContour: Set<string>;
} {
  const allIds = [...embedding.keys()];
  const n = allIds.length;
  const pos = new Map<string, { col: number; row: number }>();

  if (n === 0) return { positions: pos, outerContour: new Set<string>() };
  if (n === 1) {
    pos.set(allIds[0], { col: 0, row: 0 });
    return { positions: pos, outerContour: new Set(allIds) };
  }
  if (n === 2) {
    pos.set(allIds[0], { col: 0, row: 0 });
    pos.set(allIds[1], { col: 2, row: 0 });
    return { positions: pos, outerContour: new Set(allIds) };
  }

  // Use provided outer face (pre-triangulation boundary) or fall back to largest
  let outerFace: string[];
  if (outerFaceHint && outerFaceHint.length >= 3) {
    outerFace = outerFaceHint;
  } else {
    const faces = allFaces(embedding);
    outerFace = faces[0] ?? allIds.slice(0, 3);
    for (const f of faces) {
      if (f.length > outerFace.length) outerFace = f;
    }
  }

  // Place outer face on a circle
  const outerSet = new Set(outerFace);
  const R = Math.max(2, n * 0.4); // radius scales with node count
  const px = new Map<string, number>();
  const py = new Map<string, number>();

  for (let i = 0; i < outerFace.length; i++) {
    const angle = (2 * Math.PI * i) / outerFace.length - Math.PI / 2;
    px.set(outerFace[i], R * Math.cos(angle));
    py.set(outerFace[i], R * Math.sin(angle));
  }

  // Initialize interior vertices at centroid
  const interiorIds = allIds.filter((id) => !outerSet.has(id));
  for (const id of interiorIds) {
    px.set(id, 0);
    py.set(id, 0);
  }

  // Solve the Tutte system exactly: for each interior vertex i,
  //   deg(i)·p_i − Σ_{j∈N(i)∩interior} p_j = Σ_{j∈N(i)∩boundary} p_j
  // by dense Gaussian elimination with partial pivoting (n ≤ 50, so a dense
  // solve is instant). Iterative relaxation is NOT an option here: its
  // convergence stalls on chain-like interiors — after the iteration cap the
  // interior is still mid-collapse, with vertices stacked on each other,
  // which reads as zero-length edges and crossings downstream.
  {
    const m = interiorIds.length;
    const iIdx = new Map(interiorIds.map((id, i) => [id, i]));
    const A: number[][] = Array.from({ length: m }, () => new Array(m).fill(0));
    const bx = new Array(m).fill(0);
    const by = new Array(m).fill(0);
    for (let i = 0; i < m; i++) {
      const neighbors = embedding.get(interiorIds[i]) ?? [];
      A[i][i] = Math.max(neighbors.length, 1);
      for (const nbr of neighbors) {
        const j = iIdx.get(nbr);
        if (j !== undefined) A[i][j] -= 1;
        else {
          bx[i] += px.get(nbr) ?? 0;
          by[i] += py.get(nbr) ?? 0;
        }
      }
    }
    for (let col = 0; col < m; col++) {
      let piv = col;
      for (let r = col + 1; r < m; r++)
        if (Math.abs(A[r][col]) > Math.abs(A[piv][col])) piv = r;
      if (Math.abs(A[piv][col]) < 1e-12) continue;
      [A[col], A[piv]] = [A[piv], A[col]];
      [bx[col], bx[piv]] = [bx[piv], bx[col]];
      [by[col], by[piv]] = [by[piv], by[col]];
      for (let r = col + 1; r < m; r++) {
        const f = A[r][col] / A[col][col];
        if (f === 0) continue;
        for (let c = col; c < m; c++) A[r][c] -= f * A[col][c];
        bx[r] -= f * bx[col];
        by[r] -= f * by[col];
      }
    }
    for (let row = m - 1; row >= 0; row--) {
      if (Math.abs(A[row][row]) < 1e-12) continue;
      let sx = bx[row],
        sy = by[row];
      for (let c = row + 1; c < m; c++) {
        sx -= A[row][c] * (px.get(interiorIds[c]) ?? 0);
        sy -= A[row][c] * (py.get(interiorIds[c]) ?? 0);
      }
      px.set(interiorIds[row], sx / A[row][row]);
      py.set(interiorIds[row], sy / A[row][row]);
    }
  }

  // Build positions
  for (const id of allIds) {
    pos.set(id, { col: px.get(id)!, row: py.get(id)! });
  }

  return { positions: pos, outerContour: outerSet };
}

// ── Kamada-Kawai stress minimization ──────────────────────────────────

/**
 * Refine positions using Kamada-Kawai stress minimization.
 *
 * Minimizes: E = Σ k_ij * (|p_i - p_j| - l_ij)²
 * where l_ij = L * d_ij (graph distance × desired edge length)
 * and k_ij = K / d_ij² (closer graph neighbors → stronger springs)
 *
 * Uses Newton-Raphson, moving one vertex at a time (the one with largest
 * gradient). O(n²) per iteration, O(n³) for APSP. Fine for n ≤ 50.
 */
function kamadaKawai(
  positions: Map<string, { col: number; row: number }>,
  nodeIds: string[],
  edges: EdgePair[],
  desiredEdgeLength: number = 2.5,
): void {
  const n = nodeIds.length;
  if (n <= 2) return;

  const idx = new Map<string, number>();
  nodeIds.forEach((id, i) => idx.set(id, i));

  // All-pairs shortest paths (BFS — unweighted graph)
  const dist: number[][] = Array.from({ length: n }, () =>
    new Array(n).fill(Infinity),
  );
  const adj = buildAdj(nodeIds, edges);
  for (let i = 0; i < n; i++) {
    dist[i][i] = 0;
    const queue = [nodeIds[i]];
    let head = 0;
    while (head < queue.length) {
      const u = queue[head++];
      const ui = idx.get(u)!;
      for (const v of adj.get(u) ?? []) {
        const vi = idx.get(v)!;
        if (dist[i][vi] === Infinity) {
          dist[i][vi] = dist[i][ui] + 1;
          queue.push(v);
        }
      }
    }
  }

  // Graph diameter
  let maxDist = 1;
  for (let i = 0; i < n; i++)
    for (let j = i + 1; j < n; j++)
      if (dist[i][j] < Infinity) maxDist = Math.max(maxDist, dist[i][j]);

  const L = desiredEdgeLength;
  const K = 1;

  // Ideal lengths and spring strengths
  const l: number[][] = Array.from({ length: n }, (_, i) =>
    Array.from({ length: n }, (_, j) => {
      const d = dist[i][j];
      return d < Infinity ? L * d : L * maxDist * 2;
    }),
  );
  // Weight 1/d (not 1/d² from original Kamada-Kawai). The squared version
  // makes distant pairs near-invisible, causing nodes at graph distance 2+
  // to cluster. 1/d gives enough force for all pairs to find proper spacing.
  const k: number[][] = Array.from({ length: n }, (_, i) =>
    Array.from({ length: n }, (_, j) => {
      const d = dist[i][j];
      return d > 0 && d < Infinity ? K / d : 0;
    }),
  );

  // Ensure all nodes have positions (fallback for nodes missed by FPP)
  for (const id of nodeIds) {
    if (!positions.has(id)) {
      positions.set(id, { col: Math.random() * n, row: Math.random() * n });
    }
  }

  // Working positions (mutable)
  const x = nodeIds.map((id) => positions.get(id)!.col);
  const y = nodeIds.map((id) => positions.get(id)!.row);

  // Build edge index list for crossing checks
  const kkEdges: [number, number][] = [];
  {
    const edgeSet = new Set<string>();
    for (const [u, v] of edges) {
      const ui = idx.get(u),
        vi = idx.get(v);
      if (ui === undefined || vi === undefined) continue;
      const key = ui < vi ? `${ui},${vi}` : `${vi},${ui}`;
      if (!edgeSet.has(key)) {
        edgeSet.add(key);
        kkEdges.push([ui, vi]);
      }
    }
  }

  // Check if moving vertex m to its current position causes any edge crossing
  function moveCausesCrossing(m: number): boolean {
    for (let i = 0; i < kkEdges.length; i++) {
      const [ai, bi] = kkEdges[i];
      if (ai !== m && bi !== m) continue; // only check edges involving m
      for (let j = 0; j < kkEdges.length; j++) {
        if (i === j) continue;
        const [ci, di] = kkEdges[j];
        if (ci === ai || ci === bi || di === ai || di === bi) continue; // shared endpoint
        const d1x = x[bi] - x[ai],
          d1y = y[bi] - y[ai];
        const d2x = x[di] - x[ci],
          d2y = y[di] - y[ci];
        const cross = d1x * d2y - d1y * d2x;
        if (Math.abs(cross) < 1e-10) continue;
        const t = ((x[ci] - x[ai]) * d2y - (y[ci] - y[ai]) * d2x) / cross;
        const u = ((x[ci] - x[ai]) * d1y - (y[ci] - y[ai]) * d1x) / cross;
        if (t > 0.01 && t < 0.99 && u > 0.01 && u < 0.99) return true;
      }
    }
    return false;
  }

  // Newton-Raphson with per-vertex crossing prevention
  const MAX_OUTER = 200;
  const MAX_INNER = 20;
  const EPSILON = 0.001;

  for (let outer = 0; outer < MAX_OUTER; outer++) {
    let maxDelta = 0;
    let maxM = -1;

    for (let m = 0; m < n; m++) {
      let dEdx = 0,
        dEdy = 0;
      for (let i = 0; i < n; i++) {
        if (i === m || k[m][i] === 0) continue;
        const dx = x[m] - x[i];
        const dy = y[m] - y[i];
        const d = Math.sqrt(dx * dx + dy * dy);
        if (d < 0.001) continue;
        dEdx += k[m][i] * (dx - (l[m][i] * dx) / d);
        dEdy += k[m][i] * (dy - (l[m][i] * dy) / d);
      }
      const delta = Math.sqrt(dEdx * dEdx + dEdy * dEdy);
      if (delta > maxDelta) {
        maxDelta = delta;
        maxM = m;
      }
    }

    if (maxDelta < EPSILON || maxM === -1) break;

    const m = maxM;
    const savedX = x[m],
      savedY = y[m];

    for (let inner = 0; inner < MAX_INNER; inner++) {
      let dEdx = 0,
        dEdy = 0;
      let d2Edx2 = 0,
        d2Edy2 = 0,
        d2Edxdy = 0;

      for (let i = 0; i < n; i++) {
        if (i === m || k[m][i] === 0) continue;
        const dx = x[m] - x[i];
        const dy = y[m] - y[i];
        const d = Math.sqrt(dx * dx + dy * dy);
        if (d < 0.001) continue;
        const d3 = d * d * d;

        dEdx += k[m][i] * (dx - (l[m][i] * dx) / d);
        dEdy += k[m][i] * (dy - (l[m][i] * dy) / d);
        d2Edx2 += k[m][i] * (1 - (l[m][i] * dy * dy) / d3);
        d2Edy2 += k[m][i] * (1 - (l[m][i] * dx * dx) / d3);
        d2Edxdy += k[m][i] * ((l[m][i] * dx * dy) / d3);
      }

      const det = d2Edx2 * d2Edy2 - d2Edxdy * d2Edxdy;
      if (Math.abs(det) < 1e-10) break;

      const deltaX = -(d2Edy2 * dEdx - d2Edxdy * dEdy) / det;
      const deltaY = -(d2Edx2 * dEdy - d2Edxdy * dEdx) / det;

      x[m] += deltaX;
      y[m] += deltaY;

      // Revert if this move introduced a crossing
      if (moveCausesCrossing(m)) {
        x[m] -= deltaX;
        y[m] -= deltaY;
        break;
      }

      if (deltaX * deltaX + deltaY * deltaY < EPSILON * EPSILON) break;
    }

    // If the entire inner loop was reverted, mark this vertex as stuck
    if (moveCausesCrossing(m)) {
      x[m] = savedX;
      y[m] = savedY;
    }
  }

  // Write back
  for (let i = 0; i < n; i++) {
    positions.set(nodeIds[i], { col: x[i], row: y[i] });
  }
}

// ── Public API ─────────────────────────────────────────────────────────

/**
 * Layout a connected graph using the FPP → Kamada-Kawai pipeline.
 *
 * 1. Classify edges: planar vs non-planar
 * 2. Augment to biconnected, triangulate
 * 3. FPP shift method: crossing-free integer grid placement
 * 4. Kamada-Kawai: stress-minimize using graph distances (all edges)
 */
/**
 * Stitch a separately-laid-out block onto already-placed positions at a
 * shared cut vertex. The block is rotated so its centroid points outward
 * from the placed graph's centroid through the shared vertex, then
 * translated so the shared vertex aligns. The shared vertex keeps the
 * already-placed position; all other block vertices are written in.
 */
function stitchBlock(
  globalPositions: Map<string, { col: number; row: number }>,
  blockPositions: Map<string, { col: number; row: number }>,
  sharedVertex: string,
): void {
  let gx = 0, gy = 0, gn = 0;
  for (const p of globalPositions.values()) { gx += p.col; gy += p.row; gn++; }
  if (gn > 0) { gx /= gn; gy /= gn; }

  let bx = 0, by = 0, bn = 0;
  for (const p of blockPositions.values()) { bx += p.col; by += p.row; bn++; }
  if (bn > 0) { bx /= bn; by /= bn; }

  const gp = globalPositions.get(sharedVertex);
  const bp = blockPositions.get(sharedVertex);
  if (!gp || !bp) return;

  // Outward direction in global frame: from global centroid through the cut vertex.
  const outAngle = Math.atan2(gp.row - gy, gp.col - gx);
  // Block's centroid direction from the shared vertex (in local frame).
  const localAngle = Math.atan2(by - bp.row, bx - bp.col);

  const baseRotation = outAngle - localAngle;

  const place = (rotation: number, into: Map<string, { col: number; row: number }>) => {
    const cosR = Math.cos(rotation);
    const sinR = Math.sin(rotation);
    for (const [v, p] of blockPositions) {
      if (v === sharedVertex) continue;
      const dx = p.col - bp.col;
      const dy = p.row - bp.row;
      const rx = dx * cosR - dy * sinR;
      const ry = dx * sinR + dy * cosR;
      into.set(v, { col: gp.col + rx, row: gp.row + ry });
    }
  };

  // Worst box-border gap (px) between the candidate block placement and the
  // already-placed nodes — the stitch rotation is a free parameter, so scan
  // for the orientation that avoids landing the block on its siblings.
  const gapScore = (candidate: Map<string, { col: number; row: number }>): number => {
    let worst = Infinity;
    for (const [, cp] of candidate) {
      for (const [gid, gpp] of globalPositions) {
        if (candidate.has(gid)) continue;
        const dx = (gpp.col - cp.col) * PX_CELL_W;
        const dy = (gpp.row - cp.row) * PX_CELL_H;
        const d = Math.hypot(dx, dy) || 1e-6;
        worst = Math.min(worst, d - 2 * rectExtent(dx / d, dy / d));
      }
    }
    return worst;
  };

  const OFFSETS = [
    0,
    Math.PI / 6, -Math.PI / 6,
    Math.PI / 3, -Math.PI / 3,
    Math.PI / 2, -Math.PI / 2,
    (2 * Math.PI) / 3, -(2 * Math.PI) / 3,
    (5 * Math.PI) / 6, -(5 * Math.PI) / 6,
    Math.PI,
  ];
  let best: Map<string, { col: number; row: number }> | null = null;
  let bestScore = -Infinity;
  for (const off of OFFSETS) {
    const candidate = new Map<string, { col: number; row: number }>();
    place(baseRotation + off, candidate);
    const score = gapScore(candidate);
    if (score >= 0) {
      best = candidate;
      break; // outward-most collision-free orientation wins
    }
    if (score > bestScore) {
      bestScore = score;
      best = candidate;
    }
  }
  if (best) for (const [v, p] of best) globalPositions.set(v, p);
}

/**
 * Lay out a connected graph that decomposes into multiple biconnected
 * blocks. Anchor = largest block (by node count); lay it out via the
 * single-block planar pipeline. For each remaining block, lay it out
 * independently and stitch onto the placed positions at the shared cut
 * vertex, fanning outward from the global centroid.
 *
 * Leaves (degree-1 vertices stripped at the planarLayout entry) are
 * placed at the end on the combined positions, all as outer-fan placements
 * — pendant-block vertices sit on the periphery, so any leaves attached
 * to them belong outside, not in some interior face that doesn't exist
 * across blocks.
 */
async function layoutMultiBlock(
  blocks: Block[],
  articulationPoints: Set<string>,
  leaves: { id: string; parent: string }[],
  leafDirections: Map<string, LeafDirection>,
): Promise<PlanarLayoutResult> {
  let anchor = blocks[0];
  for (const b of blocks) {
    if (b.nodes.length > anchor.nodes.length) anchor = b;
  }

  const allPositions = new Map<string, { col: number; row: number }>();
  const allNonPlanar: EdgePair[] = [];

  const anchorResult = await planarLayout(anchor.nodes, anchor.edges);
  for (const [id, p] of anchorResult.positions) allPositions.set(id, p);
  allNonPlanar.push(...anchorResult.nonPlanarEdges);

  // BFS through the block-cut tree, placing each non-anchor block.
  const placed = new Set<Block>([anchor]);
  const queue: Block[] = [anchor];
  while (queue.length > 0) {
    const current = queue.shift()!;
    for (const b of blocks) {
      if (placed.has(b)) continue;
      const shared = b.nodes.find(
        (n) => articulationPoints.has(n) && current.nodes.includes(n),
      );
      if (!shared) continue;

      const result = await planarLayout(b.nodes, b.edges);
      allNonPlanar.push(...result.nonPlanarEdges);
      stitchBlock(allPositions, result.positions, shared);
      placed.add(b);
      queue.push(b);
    }
  }

  // Place leaves on the combined layout. Pass a placeholder non-empty
  // faces array so positionLeaves picks the Tutte-mode (gap-finding) fan
  // rather than the star-mode sector logic; with no real face data, every
  // parent gets assignment -1 (outer fan) which is right for a stitched
  // multi-block layout where blocks already sit on the periphery.
  if (leaves.length > 0) {
    placeLeaves(
      allPositions,
      leaves,
      [[]],
      [],
      new Set<string>(),
      [],
      leafDirections,
    );
  }

  // Clearance pass over the stitched whole: the fan placement and the seams
  // between blocks carry no clearance guarantee, so relax the leaves against
  // everything (blocks stay rigid), then let the backstop fix any absolute
  // deficit that remains.
  const allIds = [...allPositions.keys()];
  const np = new Set(allNonPlanar.map(([a, b]) => (a < b ? `${a}\0${b}` : `${b}\0${a}`)));
  const clearEdges: EdgePair[] = [
    ...blocks.flatMap((b) => b.edges),
    ...leaves.map((l) => [l.id, l.parent] as EdgePair),
  ].filter(([a, b]) => !np.has(a < b ? `${a}\0${b}` : `${b}\0${a}`));
  if (leaves.length > 0) {
    const ctx = buildPxCtx(allPositions, allIds, clearEdges);
    relaxMovers(
      ctx,
      leaves.map((l) => ctx.idx.get(l.id)!).filter((i) => i !== undefined),
      120,
    );
    writeBackPx(ctx, allPositions);
  }
  backstopScale(allPositions, allIds, clearEdges);

  return { positions: allPositions, nonPlanarEdges: allNonPlanar };
}

export async function planarLayout(
  nodeIds: string[],
  edges: EdgePair[],
): Promise<PlanarLayoutResult> {
  const dedupedEdges = dedupeEdges(edges);

  // Trivial cases
  if (nodeIds.length === 0)
    return { positions: new Map(), nonPlanarEdges: [] };
  if (nodeIds.length === 1) {
    return {
      positions: new Map([[nodeIds[0], { col: 0, row: 0 }]]),
      nonPlanarEdges: [],
    };
  }
  if (nodeIds.length === 2) {
    return {
      positions: new Map([
        [nodeIds[0], { col: 0, row: 0 }],
        [nodeIds[1], { col: 2, row: 0 }],
      ]),
      nonPlanarEdges: [],
    };
  }

  // Step 1: Iteratively strip degree-1 vertices. A single pass leaves
  // tree-like chains hanging off the core (e.g. a leaf parent that itself
  // has only one other connection) which then end up as separate "pendant
  // blocks" in BCC and get stitched as if they were significant — the
  // pendant placement collides with the leaf fan around the cut vertex.
  // Iterating until fixpoint reduces tree pendants to ordinary leaves so
  // they fan with their siblings; only genuine biconnected pendants (with
  // at least one cycle) survive into the block decomposition step.
  const realAdj = buildAdj(nodeIds, dedupedEdges);
  const remainingDeg = new Map<string, number>();
  for (const id of nodeIds) remainingDeg.set(id, realAdj.get(id)?.size ?? 0);
  const stripped = new Set<string>();
  const leaves: { id: string; parent: string }[] = [];
  let changed = true;
  while (changed) {
    changed = false;
    for (const id of nodeIds) {
      if (stripped.has(id)) continue;
      if ((remainingDeg.get(id) ?? 0) !== 1) continue;
      const parent = [...realAdj.get(id) ?? []].find((n) => !stripped.has(n));
      if (!parent) continue;
      leaves.push({ id, parent });
      stripped.add(id);
      remainingDeg.set(parent, (remainingDeg.get(parent) ?? 1) - 1);
      changed = true;
    }
  }
  // Reverse so leaves stripped later (parents closer to core) are processed
  // first by positionLeaves' byParent grouping. Otherwise a leaf whose parent
  // is itself a leaf (a chain extending out from the core) would try to look
  // up a parent position that hasn't been set yet.
  leaves.reverse();
  const coreIds = nodeIds.filter((id) => !stripped.has(id));
  const coreSet = new Set(coreIds);
  const coreEdges = dedupedEdges.filter(
    ([u, v]) => coreSet.has(u) && coreSet.has(v),
  );
  const leafDirections = buildLeafDirections(leaves, edges);

  // Step 2: If the core decomposes into multiple biconnected blocks, lay
  // each out independently and stitch them at their shared cut vertices —
  // pendant blocks stay external to the main embedding instead of getting
  // pulled into the augmented outer face.
  const { blocks, articulationPoints } = biconnectedComponents(coreIds, coreEdges);
  if (blocks.length > 1) {
    return await layoutMultiBlock(blocks, articulationPoints, leaves, leafDirections);
  }

  // Step 3: If core is too small (e.g. star graph), use KK directly
  if (coreIds.length <= 2) {
    const positions = new Map<string, { col: number; row: number }>();
    const hub = coreIds[0] ?? nodeIds[0];
    positions.set(hub, { col: 0, row: 0 });
    if (coreIds[1]) positions.set(coreIds[1], { col: 2, row: 0 });
    kamadaKawai(positions, coreIds.length > 0 ? coreIds : [hub], coreEdges);
    placeLeaves(positions, leaves, [], [], new Set(coreIds), coreEdges, leafDirections);
    return { positions, nonPlanarEdges: [] };
  }

  // Step 4: Classify edges — planar vs non-planar
  const coreClassification = classifyEdges(coreIds, coreEdges);

  // Non-planar component → KK on all edges (Tutte only guarantees crossing-free
  // for 3-connected planar graphs; running it on non-planar produces garbage).
  if (coreClassification.nonPlanarEdges.length > 0) {
    const positions = new Map<string, { col: number; row: number }>();
    // Initialize on a circle so KK has a good starting point
    const R = Math.max(2, coreIds.length * 0.5);
    for (let i = 0; i < coreIds.length; i++) {
      const angle = (2 * Math.PI * i) / coreIds.length - Math.PI / 2;
      positions.set(coreIds[i], { col: R * Math.cos(angle), row: R * Math.sin(angle) });
    }
    kamadaKawai(positions, coreIds, coreEdges);
    placeLeaves(positions, leaves, [], [], new Set(coreIds), coreEdges, leafDirections);
    return { positions, nonPlanarEdges: coreClassification.nonPlanarEdges };
  }

  // Planar component → Tutte pipeline
  // Save faces from the ORIGINAL classified graph (before augmentation adds dummy edges).
  // These are the real architectural faces — augmentation subdivides them.
  const originalFaces = allFaces(coreClassification.embedding);
  augmentBiconnected(
    coreIds,
    coreClassification.planarEdges,
    coreClassification.embedding,
  );
  // Also save post-augmentation faces (before triangulation)
  const preTrFaces = allFaces(coreClassification.embedding);
  triangulate(coreClassification.embedding);

  // Step 5: Tutte's embedding on the triangulated (3-connected) core graph.
  // Use the pre-triangulation outer face (actual graph boundary) — after
  // triangulation all faces are triangles so "largest face" is meaningless.
  //
  // Pick the largest face (most boundary room); among equally-large faces,
  // prefer the one whose busiest vertex is least busy. Tutte places outer-face
  // vertices on the boundary circle and pulls everything else to its neighbour
  // centroid, so keeping high-degree hubs OFF the boundary lets them settle in
  // the centre — which reads far clearer than a hub stranded on the rim.
  // Degree is measured on the FULL graph (realAdj, leaves included) so a hub's
  // pendant spokes count toward keeping it interior.
  const boundaryMaxDeg = (f: string[]): number => {
    let m = 0;
    for (const v of f) m = Math.max(m, realAdj.get(v)?.size ?? 0);
    return m;
  };
  const preTrOuter = preTrFaces.reduce(
    (best, f) =>
      f.length !== best.length
        ? f.length > best.length
          ? f
          : best
        : boundaryMaxDeg(f) < boundaryMaxDeg(best)
          ? f
          : best,
    preTrFaces[0] ?? [],
  );
  const { positions, outerContour } = tuttePlace(
    coreClassification.embedding,
    preTrOuter,
  );

  // Step 6: Center. (No global scaling here — the old min-edge rescale is
  // exactly what blew whole diagrams up to fix one cramped spot. Room-making
  // is hierarchicalBalloon's job below; it works locally.)
  {
    let cx = 0,
      cy = 0;
    for (const p of positions.values()) {
      cx += p.col;
      cy += p.row;
    }
    cx /= positions.size;
    cy /= positions.size;
    for (const p of positions.values()) {
      p.col = p.col - cx;
      p.row = p.row - cy;
    }
  }

  // Step 7: assign leaves to faces, seed them crossing-free near their
  // parents, then let hierarchical minimal ballooning make room for
  // everything — cramped regions expand into their parents' slack (contour
  // last), inhabitants relax into the granted room.
  const assignments = assignLeavesToFaces(
    leaves,
    originalFaces,
    preTrOuter,
    outerContour,
  );
  seedLeaves(
    positions,
    leaves,
    assignments,
    originalFaces,
    coreClassification.planarEdges,
  );

  let outerFi = -1;
  for (let fi = 0; fi < originalFaces.length; fi++) {
    if (originalFaces[fi].length < 3) continue;
    if (outerFi === -1 || originalFaces[fi].length > originalFaces[outerFi].length)
      outerFi = fi;
  }
  const leafFaceMap = new Map<string, number>();
  const leafParentMap = new Map<string, string>();
  for (const l of leaves) {
    leafFaceMap.set(l.id, assignments.get(l.id) ?? -1);
    leafParentMap.set(l.id, l.parent);
  }
  const allIds = [...coreIds, ...leaves.map((l) => l.id)];
  const allEdges: EdgePair[] = [
    ...coreClassification.planarEdges,
    ...leaves.map((l) => [l.id, l.parent] as EdgePair),
  ];
  hierarchicalBalloon(
    positions,
    allIds,
    allEdges,
    originalFaces,
    outerFi,
    leafFaceMap,
    leafParentMap,
  );

  backstopScale(positions, allIds, allEdges);

  return {
    positions,
    nonPlanarEdges: coreClassification.nonPlanarEdges,
  };
}

/**
 * Layout a potentially disconnected graph.
 * Each connected component is laid out separately, then packed in a row.
 */
export async function layoutGraph(
  nodeIds: string[],
  edges: EdgePair[],
): Promise<PlanarLayoutResult> {
  if (nodeIds.length === 0)
    return { positions: new Map(), nonPlanarEdges: [] };

  const dedupedEdges = dedupeEdges(edges);
  const components = connectedComponents(nodeIds, dedupedEdges);

  if (components.length === 1) {
    return await planarLayout(nodeIds, edges);
  }

  // Layout each component separately
  const allPositions = new Map<string, { col: number; row: number }>();
  const allNonPlanar: EdgePair[] = [];
  let colOffset = 0;

  for (const comp of components) {
    const compSet = new Set(comp);
    const compEdges = edges.filter(
      ([u, v]) => compSet.has(u) && compSet.has(v),
    );
    const result = await planarLayout(comp, compEdges);

    let maxCol = 0;
    for (const [id, pos] of result.positions) {
      allPositions.set(id, { col: pos.col + colOffset, row: pos.row });
      maxCol = Math.max(maxCol, pos.col);
    }

    allNonPlanar.push(...result.nonPlanarEdges);
    colOffset += maxCol + 3;
  }

  return {
    positions: allPositions,
    nonPlanarEdges: allNonPlanar,
  };
}
