/**
 * Planar graph layout: FPP shift method → Kamada-Kawai stress minimization.
 *
 * Pipeline:
 *   1. DFS → tree edges + back edges
 *   2. Greedy planarity: build embedding incrementally, classify edges
 *   3. Augment to biconnected + triangulate
 *   4. Canonical ordering (de Fraysseix-Pach-Pollack)
 *   5. FPP shift method: crossing-free integer grid placement
 *   6. Kamada-Kawai: stress-minimize all edges (planar + non-planar)
 *
 * FPP works on any triangulated planar graph (no 3-connectivity needed).
 * Kamada-Kawai adjusts spacing proportionally to graph-theoretic distance.
 * d3-force (in layout.ts) handles final collision resolution.
 *
 * All algorithms are O(n²) or better, which is instant for n ≤ 50.
 */

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
 * Three-pass leaf placement:
 *   1. assign each leaf to one of its parent's faces (or -1 for outer-contour fan)
 *   2. expand cramped faces so each (parent, face) has room for its leaves
 *   3. position leaves in the expanded layout
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
  balloonRelax(positions, coreEdges, faces, assignments, leaves);
  positionLeaves(positions, leaves, faces, assignments, coreEdges, leafDirections);
  return assignments;
}

/** Round-robin across parent's interior faces (largest first). */
function assignLeavesToFaces(
  leaves: { id: string; parent: string }[],
  faces: string[][],
  outerFace: string[],
  outerContour: Set<string>,
): Map<string, number> {
  const assignments = new Map<string, number>();

  const outerKey = [...outerFace].sort().join(",");
  const parentFaces = new Map<string, number[]>();
  for (let fi = 0; fi < faces.length; fi++) {
    const f = faces[fi];
    if (f.length < 3) continue;
    if ([...f].sort().join(",") === outerKey) continue;
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
    const parentOnOuter = outerContour.has(parentId);
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

/**
 * "Balloon" relaxation: treat each cramped face as a balloon with outward
 * radial pressure, and each core edge as a spring holding its natural Tutte
 * length. Iterate force-directed until equilibrium.
 *
 * Unlike constrained Tutte with pinned outer face, this lets the outer
 * boundary drift outward where a cramped face pushes against it — so the
 * graph grows locally near the cramp, not uniformly. Non-cramped regions
 * stay put because edge springs resist compression/extension.
 *
 * Planarity is preserved for small-to-moderate expansions starting from a
 * planar Tutte layout; no explicit crossing check (would add on demand).
 */
function balloonRelax(
  positions: Map<string, { col: number; row: number }>,
  coreEdges: EdgePair[],
  faces: string[][],
  assignments: Map<string, number>,
  leaves: { id: string; parent: string }[],
): void {
  const leafParent = new Map<string, string>();
  for (const { id, parent } of leaves) leafParent.set(id, parent);

  // Faces that carry leaves (with their parent set)
  const parentsByFace = new Map<number, Set<string>>();
  for (const [leafId, fi] of assignments) {
    if (fi < 0) continue;
    const parent = leafParent.get(leafId);
    if (!parent) continue;
    if (!parentsByFace.has(fi)) parentsByFace.set(fi, new Set());
    parentsByFace.get(fi)!.add(parent);
  }
  if (parentsByFace.size === 0) return;

  // Natural length per edge = its current Tutte length.
  const natural = new Map<string, number>();
  const edgeKey = (u: string, v: string) => (u < v ? `${u}\0${v}` : `${v}\0${u}`);
  for (const [u, v] of coreEdges) {
    const pu = positions.get(u);
    const pv = positions.get(v);
    if (!pu || !pv) continue;
    natural.set(edgeKey(u, v), Math.hypot(pu.col - pv.col, pu.row - pv.row));
  }

  // Leaf placement parameters (must match positionLeaves):
  //   NODE_MARGIN — distance from parent to leaf along face axis.
  // Non-connected constraint:
  //   NON_EDGE_GAP — minimum distance between leaf and *any other* face
  //   vertex (which the leaf isn't connected to). Smaller than NODE_MARGIN
  //   because non-connected nodes only need to not overlap, not fit a label.
  const NODE_MARGIN = 1.8;
  const NON_EDGE_GAP = 1.6;
  const K_FACE = 0.5;
  const K_EDGE = 0.5;
  const STEP = 0.15;
  const MAX_ITER = 500;
  const CONVERGED = 0.0005;

  for (let iter = 0; iter < MAX_ITER; iter++) {
    const forces = new Map<string, [number, number]>();
    for (const id of positions.keys()) forces.set(id, [0, 0]);

    // Targeted face pressure: for each (parent, face) with leaves, predict
    // where the leaf will land (parent + NODE_MARGIN · dir→opposite-centroid),
    // then for each *other* face vertex, if that vertex is too close to the
    // predicted leaf position, push it directly away from the leaf.
    // This only pushes what's cramped, in the direction it actually needs
    // to move, without translating the rest of the graph.
    for (const [fi, parents] of parentsByFace) {
      const face = faces[fi];
      if (face.length < 3) continue;

      for (const parent of parents) {
        const pp = positions.get(parent);
        if (!pp) continue;
        const opposite = face.filter((v) => v !== parent);
        if (opposite.length === 0) continue;
        let ox = 0, oy = 0;
        for (const v of opposite) {
          const p = positions.get(v)!;
          ox += p.col; oy += p.row;
        }
        ox /= opposite.length; oy /= opposite.length;
        const dxo = ox - pp.col;
        const dyo = oy - pp.row;
        const do_ = Math.hypot(dxo, dyo);
        if (do_ < 0.001) continue;

        const leafX = pp.col + NODE_MARGIN * (dxo / do_);
        const leafY = pp.row + NODE_MARGIN * (dyo / do_);

        for (const X of opposite) {
          const pX = positions.get(X)!;
          const dx = pX.col - leafX;
          const dy = pX.row - leafY;
          const d = Math.hypot(dx, dy);
          const cramp = NON_EDGE_GAP - d;
          if (cramp <= 0) continue;
          const f = forces.get(X)!;
          f[0] += (dx / d) * cramp * K_FACE;
          f[1] += (dy / d) * cramp * K_FACE;
        }
      }
    }

    // Edge springs toward natural length.
    for (const [u, v] of coreEdges) {
      const pu = positions.get(u);
      const pv = positions.get(v);
      if (!pu || !pv) continue;
      const dx = pv.col - pu.col;
      const dy = pv.row - pu.row;
      const current = Math.hypot(dx, dy);
      if (current < 0.001) continue;
      const target = natural.get(edgeKey(u, v)) ?? current;
      const mag = ((current - target) * K_EDGE) / current;
      const fu = forces.get(u)!;
      const fv = forces.get(v)!;
      fu[0] += dx * mag;
      fu[1] += dy * mag;
      fv[0] -= dx * mag;
      fv[1] -= dy * mag;
    }

    // Apply forces
    let maxMove = 0;
    for (const [id, p] of positions) {
      const [fx, fy] = forces.get(id)!;
      const mx = fx * STEP, my = fy * STEP;
      p.col += mx;
      p.row += my;
      const m = Math.hypot(mx, my);
      if (m > maxMove) maxMove = m;
    }

    // Anchor against graph-wide drift: recenter to centroid = (0, 0) each step.
    // Without this, forces on cramped-face vertices translate the whole graph
    // in that direction because the edge springs drag non-cramp vertices along.
    let cx = 0, cy = 0;
    for (const p of positions.values()) { cx += p.col; cy += p.row; }
    cx /= positions.size; cy /= positions.size;
    for (const p of positions.values()) { p.col -= cx; p.row -= cy; }

    if (maxMove < CONVERGED) break;
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
        if (sectorPlace(positions, pp, parentLeaves, N, leafDirections)) {
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
        if (N >= 3 && coreEdges.length > 0) {
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
          const arcGap = N > 1 ? fanSpread / (N - 1) : Math.PI / 2;
          const arcDist = Math.max(
            1.8,
            MIN_CHORD / (2 * Math.sin(Math.min(arcGap, Math.PI) / 2)),
          );
          for (let i = 0; i < N; i++) {
            const t = N === 1 ? 0 : (i / (N - 1) - 0.5) * fanSpread;
            positions.set(ordered[i].id, {
              col: pp.col + arcDist * Math.cos(baseAngle + t),
              row: pp.row + arcDist * Math.sin(baseAngle + t),
            });
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

  // Barycentric relaxation: each interior vertex → average of neighbors
  for (let iter = 0; iter < 500; iter++) {
    let maxMove = 0;
    for (const id of interiorIds) {
      const neighbors = embedding.get(id) ?? [];
      if (neighbors.length === 0) continue;
      let sx = 0,
        sy = 0;
      for (const nbr of neighbors) {
        sx += px.get(nbr) ?? 0;
        sy += py.get(nbr) ?? 0;
      }
      const newX = sx / neighbors.length;
      const newY = sy / neighbors.length;
      const move =
        Math.abs(newX - (px.get(id) ?? 0)) + Math.abs(newY - (py.get(id) ?? 0));
      maxMove = Math.max(maxMove, move);
      px.set(id, newX);
      py.set(id, newY);
    }
    if (maxMove < 0.001) break; // converged
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

  // Step 1: Identify leaf nodes (degree 1 in real graph) — strip before FPP
  const realAdj = buildAdj(nodeIds, dedupedEdges);
  const leaves: { id: string; parent: string }[] = [];
  const coreIds: string[] = [];
  for (const id of nodeIds) {
    const deg = realAdj.get(id)?.size ?? 0;
    if (deg === 1) {
      leaves.push({ id, parent: [...realAdj.get(id)!][0] });
    } else {
      coreIds.push(id);
    }
  }
  const coreSet = new Set(coreIds);
  const coreEdges = dedupedEdges.filter(
    ([u, v]) => coreSet.has(u) && coreSet.has(v),
  );
  const leafDirections = buildLeafDirections(leaves, edges);

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
  const preTrOuter = preTrFaces.reduce(
    (a, b) => (a.length >= b.length ? a : b),
    [],
  );
  const { positions, outerContour } = tuttePlace(
    coreClassification.embedding,
    preTrOuter,
  );

  // Step 6: Normalize — uniform scale to compact size
  {
    // Scale so shortest edge = 2× node diagonal. Ensures no cramping.
    // Must match CELL_W/CELL_H in layout.ts (300 × 180, anisotropic).
    const NODE_CELL_W = (180 + 80) / 300;
    const NODE_CELL_H = (160 + 80) / 180;
    const minEdgeTarget =
      Math.sqrt(NODE_CELL_W * NODE_CELL_W + NODE_CELL_H * NODE_CELL_H) * 1.2;

    let minEdgeLen = Infinity;
    for (const [u, v] of coreEdges) {
      const pu = positions.get(u),
        pv = positions.get(v);
      if (pu && pv) {
        const d = Math.sqrt((pu.col - pv.col) ** 2 + (pu.row - pv.row) ** 2);
        if (d > 0.001) minEdgeLen = Math.min(minEdgeLen, d);
      }
    }
    const scale =
      minEdgeLen < Infinity && minEdgeLen < minEdgeTarget
        ? minEdgeTarget / minEdgeLen
        : 1;

    // Center and scale
    let cx = 0,
      cy = 0;
    for (const p of positions.values()) {
      cx += p.col;
      cy += p.row;
    }
    cx /= positions.size;
    cy /= positions.size;
    for (const p of positions.values()) {
      p.col = (p.col - cx) * scale;
      p.row = (p.row - cy) * scale;
    }
  }

  // Step 7: assign leaves → balloon-relax cramped faces → position leaves.
  // Balloon uses only the real (non-dummy) planar edges for its springs.
  placeLeaves(
    positions,
    leaves,
    originalFaces,
    preTrOuter,
    outerContour,
    coreClassification.planarEdges,
    leafDirections,
  );

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
