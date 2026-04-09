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

// ── Leaf placement ────────────────────────────────────────────────────

/**
 * Place leaf nodes outside the core graph, near their parent.
 * Each leaf goes in the widest angular gap around its parent,
 * so it's on the outside rather than crammed inside a face.
 */
function placeLeaves(
  positions: Map<string, { col: number; row: number }>,
  leaves: { id: string; parent: string }[],
  adj: Map<string, Set<string>>,
  outerContour: Set<string>,
): void {
  const LEAF_DIST = 2.0;

  // Collect existing edges for crossing checks
  const posEdges: [string, string][] = [];
  for (const [u, nbrs] of adj) {
    for (const v of nbrs) {
      if (u < v && positions.has(u) && positions.has(v)) posEdges.push([u, v]);
    }
  }

  // Check if placing leaf at (lx,ly) with edge to parent crosses any existing edge
  function wouldCross(lx: number, ly: number, parentId: string): boolean {
    const pp = positions.get(parentId)!;
    for (const [eu, ev] of posEdges) {
      if (eu === parentId || ev === parentId) continue;
      const pu = positions.get(eu)!,
        pv = positions.get(ev)!;
      // Segment intersection test
      const d1x = pp.col - lx,
        d1y = pp.row - ly;
      const d2x = pv.col - pu.col,
        d2y = pv.row - pu.row;
      const cross = d1x * d2y - d1y * d2x;
      if (Math.abs(cross) < 1e-10) continue;
      const t = ((pu.col - lx) * d2y - (pu.row - ly) * d2x) / cross;
      const u = ((pu.col - lx) * d1y - (pu.row - ly) * d1x) / cross;
      if (t > 0.01 && t < 0.99 && u > 0.01 && u < 0.99) return true;
    }
    return false;
  }

  for (const { id, parent } of leaves) {
    const parentPos = positions.get(parent);
    if (!parentPos) {
      positions.set(id, { col: 0, row: 0 });
      continue;
    }

    // Compute angles to all positioned neighbors of the parent
    const angles: number[] = [];
    for (const nbr of adj.get(parent) ?? []) {
      if (nbr === id) continue;
      const nbrPos = positions.get(nbr);
      if (!nbrPos) continue;
      angles.push(
        Math.atan2(nbrPos.row - parentPos.row, nbrPos.col - parentPos.col),
      );
    }

    if (angles.length === 0) {
      positions.set(id, { col: parentPos.col, row: parentPos.row - LEAF_DIST });
      posEdges.push([id, parent]);
      continue;
    }

    angles.sort((a, b) => a - b);

    // Build outer contour polygon for inside/outside test
    const contourVerts = [...outerContour]
      .map((v) => positions.get(v))
      .filter((p): p is { col: number; row: number } => !!p);

    function isInsideContour(px: number, py: number): boolean {
      if (contourVerts.length < 3) return false;
      let inside = false;
      for (
        let i = 0, j = contourVerts.length - 1;
        i < contourVerts.length;
        j = i++
      ) {
        const xi = contourVerts[i].col,
          yi = contourVerts[i].row;
        const xj = contourVerts[j].col,
          yj = contourVerts[j].row;
        if (
          yi > py !== yj > py &&
          px < ((xj - xi) * (py - yi)) / (yj - yi) + xi
        ) {
          inside = !inside;
        }
      }
      return inside;
    }

    // Build gap list sorted by size (widest first)
    const gaps: { angle: number; size: number }[] = [];
    for (let i = 0; i < angles.length; i++) {
      const next =
        i + 1 < angles.length ? angles[i + 1] : angles[0] + 2 * Math.PI;
      const gap = next - angles[i];
      gaps.push({ angle: angles[i] + gap / 2, size: gap });
    }
    gaps.sort((a, b) => b.size - a.size);

    const dist = LEAF_DIST;

    // All leaves prefer outside the contour. Crossing check prevents bad placements.
    // First pass: outside contour + no crossings
    let placed = false;
    for (const { angle } of gaps) {
      const lx = parentPos.col + dist * Math.cos(angle);
      const ly = parentPos.row + dist * Math.sin(angle);
      if (!isInsideContour(lx, ly) && !wouldCross(lx, ly, parent)) {
        positions.set(id, { col: lx, row: ly });
        placed = true;
        break;
      }
    }

    // Second pass: any non-crossing gap (inside is OK if outside all cross)
    if (!placed) {
      for (const { angle } of gaps) {
        const lx = parentPos.col + dist * Math.cos(angle);
        const ly = parentPos.row + dist * Math.sin(angle);
        if (!wouldCross(lx, ly, parent)) {
          positions.set(id, { col: lx, row: ly });
          placed = true;
          break;
        }
      }
    }

    if (!placed) {
      // All gaps cross — use widest anyway
      const angle = gaps[0].angle;
      positions.set(id, {
        col: parentPos.col + dist * Math.cos(angle),
        row: parentPos.row + dist * Math.sin(angle),
      });
    }

    // Add this leaf's edge for subsequent crossing checks
    posEdges.push([id, parent]);
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

  // Step 3: If core is too small (e.g. star graph), use KK directly
  if (coreIds.length <= 2) {
    const positions = new Map<string, { col: number; row: number }>();
    const hub = coreIds[0] ?? nodeIds[0];
    positions.set(hub, { col: 0, row: 0 });
    if (coreIds[1]) positions.set(coreIds[1], { col: 2, row: 0 });
    kamadaKawai(positions, coreIds.length > 0 ? coreIds : [hub], coreEdges);
    placeLeaves(positions, leaves, realAdj, new Set(coreIds));
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
    placeLeaves(positions, leaves, realAdj, new Set(coreIds));
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
    const NODE_CELL_W = (180 + 80) / 240;
    const NODE_CELL_H = (160 + 80) / 200;
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

  // Step 7: Place leaf nodes — inside parent's face if parent is interior,
  // outside (widest gap) if parent is on outer contour
  placeLeaves(positions, leaves, realAdj, outerContour);

  // Step 8: Expand cramped faces — one face at a time, most cramped first.
  // Checks ALL vertex pairs within each face (not just inner nodes).
  {
    const NODE_MARGIN = Math.sqrt(((180 + 40) / 240) ** 2 + ((160 + 40) / 200) ** 2) * 0.8;
    const allFacesToCheck = [...new Set([...originalFaces, ...preTrFaces].map((f) => [...f].sort().join(",")))]
      .map((key) => {
        const sorted = key.split(",");
        // Find the original face with these vertices
        return [...originalFaces, ...preTrFaces].find((f) => [...f].sort().join(",") === key) ?? sorted;
      });

    function pip(px: number, py: number, verts: { col: number; row: number }[]): boolean {
      let inside = false;
      for (let i = 0, j = verts.length - 1; i < verts.length; j = i++) {
        const xi = verts[i].col, yi = verts[i].row, xj = verts[j].col, yj = verts[j].row;
        if ((yi > py) !== (yj > py) && px < (xj - xi) * (py - yi) / (yj - yi) + xi) inside = !inside;
      }
      return inside;
    }

    // Repeat: find the single most cramped face, expand it, stop.
    for (let round = 0; round < 5; round++) {
      let worstFace: string[] | null = null;
      let worstMinDist = Infinity;
      let worstAllNodes: string[] = [];

      // Find the outer face (largest) to skip — it's the unbounded region
      const outerFaceKey = preTrOuter.length > 0 ? [...preTrOuter].sort().join(",") : "";

      for (const face of allFacesToCheck) {
        if (face.length < 3) continue;
        // Skip the outer face — it's unbounded, expanding it blows up the graph
        if ([...face].sort().join(",") === outerFaceKey) continue;
        const faceVerts = face.map((v) => positions.get(v)).filter((p): p is { col: number; row: number } => !!p);
        if (faceVerts.length < 3) continue;
        const faceSet = new Set(face);

        // Collect face vertices + any nodes inside the face (leaves, etc.)
        const allNodes = [...face.filter((v) => positions.has(v))];
        for (const [id, p] of positions) {
          if (faceSet.has(id)) continue;
          if (pip(p.col, p.row, faceVerts)) allNodes.push(id);
        }
        if (allNodes.length < 2) continue;

        let minDist = Infinity;
        for (let i = 0; i < allNodes.length; i++) {
          const pi = positions.get(allNodes[i])!;
          for (let j = i + 1; j < allNodes.length; j++) {
            const pj = positions.get(allNodes[j])!;
            const d = Math.sqrt((pi.col - pj.col) ** 2 + (pi.row - pj.row) ** 2);
            if (d > 0.001) minDist = Math.min(minDist, d);
          }
        }

        if (minDist < NODE_MARGIN && minDist < worstMinDist) {
          worstFace = face;
          worstMinDist = minDist;
          worstAllNodes = allNodes;
        }
      }

      if (!worstFace) break;

      // Expand this one face: push vertices outward from centroid, 10% at a time
      const face = worstFace;
      const faceSet = new Set(face);

      // Build super-nodes
      const superNodes = new Map<string, Set<string>>();
      for (const v of face) {
        const sn = new Set<string>([v]);
        const queue = [v];
        while (queue.length > 0) {
          const u = queue.shift()!;
          for (const nbr of realAdj.get(u) ?? []) {
            if (sn.has(nbr) || faceSet.has(nbr)) continue;
            if (!positions.has(nbr)) continue;
            sn.add(nbr);
            queue.push(nbr);
          }
          for (const leaf of leaves) {
            if (leaf.parent === u && !sn.has(leaf.id)) sn.add(leaf.id);
          }
        }
        superNodes.set(v, sn);
      }

      for (let step = 0; step < 10; step++) {
        // Recheck min distance between all nodes in this face
        let curMin = Infinity;
        for (let i = 0; i < worstAllNodes.length; i++) {
          const pi = positions.get(worstAllNodes[i])!;
          for (let j = i + 1; j < worstAllNodes.length; j++) {
            const pj = positions.get(worstAllNodes[j])!;
            const d = Math.sqrt((pi.col - pj.col) ** 2 + (pi.row - pj.row) ** 2);
            if (d > 0.001) curMin = Math.min(curMin, d);
          }
        }
        if (curMin >= NODE_MARGIN) break;

        // Push 10% outward from centroid
        const fps = face.map((v) => positions.get(v)).filter((p): p is { col: number; row: number } => !!p);
        let fcx = 0, fcy = 0;
        for (const p of fps) { fcx += p.col; fcy += p.row; }
        fcx /= fps.length; fcy /= fps.length;

        for (const v of face) {
          const p = positions.get(v);
          if (!p) continue;
          const dx = (p.col - fcx) * 0.1;
          const dy = (p.row - fcy) * 0.1;
          for (const id of superNodes.get(v) ?? []) {
            const sp = positions.get(id);
            if (sp) { sp.col += dx; sp.row += dy; }
          }
        }
      }
    }
  }

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
    return await planarLayout(nodeIds, dedupedEdges);
  }

  // Layout each component separately
  const allPositions = new Map<string, { col: number; row: number }>();
  const allNonPlanar: EdgePair[] = [];
  let colOffset = 0;

  for (const comp of components) {
    const compSet = new Set(comp);
    const compEdges = dedupedEdges.filter(
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
