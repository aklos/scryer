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
  /** Pre-computed routes for face expansion edges (L-shaped bends). Key: "source\0target" */
  faceRoutes: Map<string, { col: number; row: number }[]>;
}

// ── Graph utilities ────────────────────────────────────────────────────

function buildAdj(nodeIds: string[], edges: EdgePair[]): Map<string, Set<string>> {
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
export function connectedComponents(nodeIds: string[], edges: EdgePair[]): string[][] {
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
  order: string[];           // DFS pre-order
  depth: Map<string, number>;
  treeEdges: EdgePair[];
  backEdges: EdgePair[];     // (descendant, ancestor)
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
function walkFace(embedding: Embedding, startU: string, startV: string): string[] {
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
): { planarEdges: EdgePair[]; nonPlanarEdges: EdgePair[]; embedding: Embedding } {
  if (nodeIds.length <= 2) {
    return { planarEdges: edges, nonPlanarEdges: [], embedding: treeEmbedding(nodeIds, edges) };
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
function triangulate(
  embedding: Embedding,
): EdgePair[] {
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
  const LEAF_DIST = 1.5;

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
      angles.push(Math.atan2(nbrPos.row - parentPos.row, nbrPos.col - parentPos.col));
    }

    if (angles.length === 0) {
      positions.set(id, { col: parentPos.col, row: parentPos.row - LEAF_DIST });
      continue;
    }

    angles.sort((a, b) => a - b);

    if (outerContour.has(parent)) {
      // Parent is on outer boundary — place leaf OUTSIDE in widest angular gap
      let bestGap = 0, bestAngle = 0;
      for (let i = 0; i < angles.length; i++) {
        const next = i + 1 < angles.length ? angles[i + 1] : angles[0] + 2 * Math.PI;
        const gap = next - angles[i];
        if (gap > bestGap) { bestGap = gap; bestAngle = angles[i] + gap / 2; }
      }
      positions.set(id, {
        col: parentPos.col + LEAF_DIST * Math.cos(bestAngle),
        row: parentPos.row + LEAF_DIST * Math.sin(bestAngle),
      });
    } else {
      // Parent is INSIDE a face — place leaf inside the same face.
      // The "inside" direction is toward the centroid of the graph
      // (away from the outer boundary). Pick the angular gap that points
      // most toward the graph centroid.
      let cx = 0, cy = 0, count = 0;
      for (const p of positions.values()) { cx += p.col; cy += p.row; count++; }
      cx /= count; cy /= count;
      const toCentroidAngle = Math.atan2(cy - parentPos.row, cx - parentPos.col);

      // Find gap closest to the centroid direction (stay inside the face)
      let bestAngle = angles[0];
      let bestDist = Infinity;
      for (let i = 0; i < angles.length; i++) {
        const next = i + 1 < angles.length ? angles[i + 1] : angles[0] + 2 * Math.PI;
        const gap = next - angles[i];
        if (gap < 0.1) continue; // skip tiny gaps
        const midAngle = angles[i] + gap / 2;
        // Angular distance to centroid direction
        let diff = Math.abs(midAngle - toCentroidAngle);
        if (diff > Math.PI) diff = 2 * Math.PI - diff;
        if (diff < bestDist) { bestDist = diff; bestAngle = midAngle; }
      }
      positions.set(id, {
        col: parentPos.col + LEAF_DIST * 0.7 * Math.cos(bestAngle),
        row: parentPos.row + LEAF_DIST * 0.7 * Math.sin(bestAngle),
      });
    }
  }
}

// ── FPP shift method placement ───────────────────────────────────────

/**
 * Integrated FPP: vertex selection + placement in one pass.
 *
 * Instead of a pre-computed canonical ordering, selects the next vertex
 * based on the ACTUAL contour at each step. Picks the unplaced vertex
 * with the smallest span on the contour (fewest covered nodes), which
 * preserves contour vertices for future placements.
 *
 * Grid size: at most (2n-4) × (n-2).
 */
function fppPlace(
  embedding: Embedding,
): { positions: Map<string, { col: number; row: number }>; outerContour: Set<string> } {
  const allIds = [...embedding.keys()];
  const n = allIds.length;
  const pos = new Map<string, { col: number; row: number }>();

  if (n === 0) return { positions: pos, outerContour: new Set<string>() };
  if (n === 1) { pos.set(allIds[0], { col: 0, row: 0 }); return { positions: pos, outerContour: new Set(allIds) }; }

  // Build full adjacency from embedding (symmetric)
  const adj = new Map<string, Set<string>>();
  for (const id of allIds) adj.set(id, new Set());
  for (const [u, nbrs] of embedding) {
    for (const v of nbrs) {
      adj.get(u)!.add(v);
      adj.get(v)!.add(u);
    }
  }

  // Find an outer face edge — pick from the largest face
  const faces = allFaces(embedding);
  let outerFace = faces[0] ?? allIds.slice(0, 3);
  for (const f of faces) {
    if (f.length > outerFace.length) outerFace = f;
  }
  const v1 = outerFace[0];
  const v2 = outerFace[1] ?? allIds[1];

  if (n === 2) {
    pos.set(v1, { col: 0, row: 0 });
    pos.set(v2, { col: 2, row: 0 });
    return { positions: pos, outerContour: new Set([v1, v2]) };
  }

  // Place v1 and v2 on baseline
  pos.set(v1, { col: 0, row: 0 });
  pos.set(v2, { col: 2, row: 0 });
  const placed = new Set([v1, v2]);

  // Contour: left-to-right boundary of placed vertices
  let contour: string[] = [v1, v2];

  // Covered vertices tracking for shift propagation
  const coveredBy = new Map<string, string[]>();
  coveredBy.set(v1, []);
  coveredBy.set(v2, []);

  const shiftVertex = (id: string, dx: number) => {
    pos.get(id)!.col += dx;
    for (const cv of coveredBy.get(id) ?? []) {
      pos.get(cv)!.col += dx;
    }
  };

  while (placed.size < n) {
    // Find unplaced vertex with ≥2 contour neighbors, preferring smallest span
    let bestV: string | null = null;
    let bestP = -1, bestQ = -1;
    let bestSpan = Infinity;

    for (const v of allIds) {
      if (placed.has(v)) continue;
      const nbrs = adj.get(v)!;

      let pIdx = -1, qIdx = -1;
      for (let i = 0; i < contour.length; i++) {
        if (nbrs.has(contour[i])) {
          if (pIdx === -1) pIdx = i;
          qIdx = i;
        }
      }

      if (pIdx !== -1 && qIdx !== -1 && pIdx !== qIdx) {
        // FPP requires ALL contour vertices between wp and wq to be
        // neighbors of v. If any intermediate vertex is NOT a neighbor,
        // covering it would be invalid and produce crossings.
        let allIntermediate = true;
        for (let j = pIdx + 1; j < qIdx; j++) {
          if (!nbrs.has(contour[j])) { allIntermediate = false; break; }
        }
        if (!allIntermediate) continue;

        const span = qIdx - pIdx;
        // Count how many of v's neighbors are still unplaced — fewer means
        // v is "more ready" and should go first (its contour window may close)
        let unplacedNbrs = 0;
        for (const nb of nbrs) if (!placed.has(nb)) unplacedNbrs++;

        // Primary: fewest unplaced neighbors (most "ready" — window may close).
        // Secondary: smallest span.
        const prevUnplaced = bestV ? [...adj.get(bestV)!].filter((nb) => !placed.has(nb)).length : Infinity;
        if (unplacedNbrs < prevUnplaced || (unplacedNbrs === prevUnplaced && span < bestSpan)) {
          bestV = v;
          bestP = pIdx;
          bestQ = qIdx;
          bestSpan = span;
        }
      }
    }

    if (!bestV) {
      // No vertex with ≥2 contour neighbors. Log what's stuck.
      const unplaced = allIds.filter((v) => !placed.has(v));
      for (const v of unplaced) {
        const nbrs = [...adj.get(v)!];
        const onContour = nbrs.filter((nb) => contour.includes(nb));
        console.warn(`[FPP] stuck: ${v} neighbors=[${nbrs.join(",")}] onContour=[${onContour.join(",")}] contour=[${contour.join(",")}]`);
      }
      // Fallback: place remaining at fallback positions
      for (const v of unplaced) {
        const maxCol = Math.max(...[...pos.values()].map((p) => p.col));
        pos.set(v, { col: maxCol + 2, row: 0 });
        placed.add(v);
      }
      break;
    }

    console.log(`[FPP] place ${bestV}: span=${bestSpan} contour=[${contour.join(",")}] wp=${contour[bestP]} wq=${contour[bestQ]}`);

    // FPP shift + place
    const pIdx = bestP;
    const qIdx = bestQ;

    for (let i = pIdx + 1; i < qIdx; i++) shiftVertex(contour[i], 1);
    for (let i = qIdx; i < contour.length; i++) shiftVertex(contour[i], 2);

    const posP = pos.get(contour[pIdx])!;
    const posQ = pos.get(contour[qIdx])!;
    const xk = (posQ.row - posP.row + posQ.col + posP.col) / 2;
    const yk = posP.row + (xk - posP.col);

    pos.set(bestV, { col: xk, row: yk });
    placed.add(bestV);

    // Cover vertices between pIdx and qIdx
    const newCovered: string[] = [];
    for (let i = pIdx + 1; i < qIdx; i++) {
      newCovered.push(contour[i]);
      for (const cv of coveredBy.get(contour[i]) ?? []) newCovered.push(cv);
      coveredBy.delete(contour[i]);
    }
    coveredBy.set(bestV, newCovered);

    // Update contour
    contour = [...contour.slice(0, pIdx + 1), bestV, ...contour.slice(qIdx)];
  }

  return { positions: pos, outerContour: new Set(contour ?? []) };
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
  const dist: number[][] = Array.from({ length: n }, () => new Array(n).fill(Infinity));
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
      const ui = idx.get(u), vi = idx.get(v);
      if (ui === undefined || vi === undefined) continue;
      const key = ui < vi ? `${ui},${vi}` : `${vi},${ui}`;
      if (!edgeSet.has(key)) { edgeSet.add(key); kkEdges.push([ui, vi]); }
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
        const d1x = x[bi] - x[ai], d1y = y[bi] - y[ai];
        const d2x = x[di] - x[ci], d2y = y[di] - y[ci];
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
      let dEdx = 0, dEdy = 0;
      for (let i = 0; i < n; i++) {
        if (i === m || k[m][i] === 0) continue;
        const dx = x[m] - x[i];
        const dy = y[m] - y[i];
        const d = Math.sqrt(dx * dx + dy * dy);
        if (d < 0.001) continue;
        dEdx += k[m][i] * (dx - l[m][i] * dx / d);
        dEdy += k[m][i] * (dy - l[m][i] * dy / d);
      }
      const delta = Math.sqrt(dEdx * dEdx + dEdy * dEdy);
      if (delta > maxDelta) { maxDelta = delta; maxM = m; }
    }

    if (maxDelta < EPSILON || maxM === -1) break;

    const m = maxM;
    const savedX = x[m], savedY = y[m];

    for (let inner = 0; inner < MAX_INNER; inner++) {
      let dEdx = 0, dEdy = 0;
      let d2Edx2 = 0, d2Edy2 = 0, d2Edxdy = 0;

      for (let i = 0; i < n; i++) {
        if (i === m || k[m][i] === 0) continue;
        const dx = x[m] - x[i];
        const dy = y[m] - y[i];
        const d = Math.sqrt(dx * dx + dy * dy);
        if (d < 0.001) continue;
        const d3 = d * d * d;

        dEdx += k[m][i] * (dx - l[m][i] * dx / d);
        dEdy += k[m][i] * (dy - l[m][i] * dy / d);
        d2Edx2 += k[m][i] * (1 - l[m][i] * dy * dy / d3);
        d2Edy2 += k[m][i] * (1 - l[m][i] * dx * dx / d3);
        d2Edxdy += k[m][i] * (l[m][i] * dx * dy / d3);
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

// ── Face expansion ────────────────────────────────────────────────────

/**
 * Expand cramped triangular faces into rectangles.
 *
 * For each face with interior nodes:
 * 1. Pick the edge closest to vertical/horizontal as the "spine"
 * 2. Route the other 2 edges with right-angle bends to form a rectangle
 * 3. Push boundary super nodes (vertex + connected outside subgraph) outward
 * 4. Spread interior nodes within the expanded rectangle
 */
function expandCrampedFaces(
  positions: Map<string, { col: number; row: number }>,
  originalFaces: string[][],
  outerContour: Set<string>,
  leaves: { id: string; parent: string }[],
  adj: Map<string, Set<string>>,
): Map<string, { col: number; row: number }[]> {
  const routes = new Map<string, { col: number; row: number }[]>();
  // Node footprint in cell units
  const NODE_W = (180 + 80) / 240;
  const NODE_H = (160 + 80) / 200;

  // Pre-triangulation faces — the real graph faces (polygons, not just triangles)
  const faces = originalFaces;

  // Identify interior (covered) core nodes — not on outer contour
  // Core IDs are all unique node IDs that appear in any face
  const coreIdSet = new Set<string>();
  for (const face of faces) for (const v of face) coreIdSet.add(v);
  const coreIds = [...coreIdSet];
  const interiorCoreIds = coreIds.filter((id) => !outerContour.has(id));

  console.log(`[FaceExpand] coreIds=${coreIds.length} interiorCoreIds=${interiorCoreIds.length} faces=${faces.length}`);
  console.log(`[FaceExpand] outerContour: [${[...outerContour].join(",")}]`);
  console.log(`[FaceExpand] interior nodes: [${interiorCoreIds.join(",")}]`);

  // For each interior core node, find which face contains it (point-in-triangle)
  // Point-in-polygon using ray casting
  function pointInPolygon(
    px: number, py: number, verts: { col: number; row: number }[],
  ): boolean {
    let inside = false;
    for (let i = 0, j = verts.length - 1; i < verts.length; j = i++) {
      const xi = verts[i].col, yi = verts[i].row;
      const xj = verts[j].col, yj = verts[j].row;
      if ((yi > py) !== (yj > py) && px < (xj - xi) * (py - yi) / (yj - yi) + xi) {
        inside = !inside;
      }
    }
    return inside;
  }

  // Map: face index → interior node IDs (core + their leaves)
  const faceInterior = new Map<number, string[]>();

  // Log face geometry
  for (let fi = 0; fi < faces.length; fi++) {
    const face = faces[fi];
    const verts = face.map((v) => {
      const p = positions.get(v);
      return p ? `${v}(${p.col.toFixed(1)},${p.row.toFixed(1)})` : `${v}(?)`;
    });
    console.log(`[FaceExpand] face ${fi}: [${verts.join(", ")}]`);
  }
  for (const id of interiorCoreIds) {
    const p = positions.get(id);
    console.log(`[FaceExpand] interior ${id}: ${p ? `(${p.col.toFixed(1)},${p.row.toFixed(1)})` : "no pos"}`);
  }

  for (const id of interiorCoreIds) {
    const p = positions.get(id);
    if (!p) continue;
    for (let fi = 0; fi < faces.length; fi++) {
      const face = faces[fi];
      if (face.length < 3) continue;
      // Skip faces that have this node as a vertex (it's ON the face, not inside)
      if (face.includes(id)) continue;
      const faceVerts = face.map((v) => positions.get(v)).filter((v): v is { col: number; row: number } => !!v);
      if (faceVerts.length < 3) continue;
      const isInside = pointInPolygon(p.col, p.row, faceVerts);
      console.log(`[FaceExpand] test ${id}(${p.col.toFixed(1)},${p.row.toFixed(1)}) in face ${fi} [${face.join(",")}]: ${isInside}`);
      if (isInside) {
        if (!faceInterior.has(fi)) faceInterior.set(fi, []);
        faceInterior.get(fi)!.push(id);
        // Also add this node's leaves
        for (const leaf of leaves) {
          if (leaf.parent === id) faceInterior.get(fi)!.push(leaf.id);
        }
        break;
      }
    }
  }

  // Expand each face that has interior nodes
  for (const [fi, interiorIds] of faceInterior) {
    const face = faces[fi];
    if (face.length < 3) continue;

    const verts = face.slice(0, 3); // triangle vertices
    const pa = positions.get(verts[0])!, pb = positions.get(verts[1])!, pc = positions.get(verts[2])!;
    if (!pa || !pb || !pc) continue;

    // Find the edge closest to vertical or horizontal — this becomes the spine
    const edgeCandidates = [
      { a: 0, b: 1, dx: Math.abs(pb.col - pa.col), dy: Math.abs(pb.row - pa.row) },
      { a: 1, b: 2, dx: Math.abs(pc.col - pb.col), dy: Math.abs(pc.row - pb.row) },
      { a: 0, b: 2, dx: Math.abs(pc.col - pa.col), dy: Math.abs(pc.row - pa.row) },
    ];
    // Score: how close to axis-aligned (lower = more aligned)
    const scored = edgeCandidates.map((e) => ({
      ...e,
      score: Math.min(e.dx, e.dy) / (Math.max(e.dx, e.dy) || 1),
    }));
    scored.sort((a, b) => a.score - b.score);
    const spine = scored[0]; // most axis-aligned edge

    // The third vertex (not on spine) is the one we need to "rectangularize"
    const spineVerts = [verts[spine.a], verts[spine.b]];
    const oppositeVert = verts.find((v) => !spineVerts.includes(v))!;
    const spA = positions.get(spineVerts[0])!;
    const spB = positions.get(spineVerts[1])!;
    const opP = positions.get(oppositeVert)!;

    // Compute how much area is needed — grid of nodes with padding
    const nodesInFace = interiorIds.length;
    const gridCols = Math.max(1, Math.ceil(Math.sqrt(nodesInFace)));
    const gridRows = Math.max(1, Math.ceil(nodesInFace / gridCols));
    const neededWidth = gridCols * NODE_W * 1.5;
    const neededHeight = gridRows * NODE_H * 1.5;

    // Current face dimensions
    // Distance from opposite vertex to spine (height of triangle)
    const spineDx = spB.col - spA.col, spineDy = spB.row - spA.row;
    const spineNorm = Math.sqrt(spineDx * spineDx + spineDy * spineDy) || 1;
    const perpDist = Math.abs((opP.col - spA.col) * (-spineDy / spineNorm) + (opP.row - spA.row) * (spineDx / spineNorm));

    // Expansion factor: triangle faces have ~50% usable area of a rectangle,
    // so we need roughly 2x the perpendicular distance to fit the same nodes.
    const neededPerp = Math.max(neededWidth, neededHeight) * 1.6;
    const expansionFactor = Math.max(1, neededPerp / Math.max(perpDist, 0.1));

    if (expansionFactor <= 1.1) continue; // face is big enough

    // Push opposite vertex away from spine center
    const spineCx = (spA.col + spB.col) / 2;
    const spineCy = (spA.row + spB.row) / 2;
    const pushDx = opP.col - spineCx;
    const pushDy = opP.row - spineCy;
    const pushLen = Math.sqrt(pushDx * pushDx + pushDy * pushDy) || 1;

    const displacement = (expansionFactor - 1) * perpDist;
    const dx = (pushDx / pushLen) * displacement;
    const dy = (pushDy / pushLen) * displacement;

    // Move opposite vertex as a super node: vertex + everything connected
    // on the outside (away from this face)
    const superNodeIds = new Set<string>();
    // BFS from opposite vertex through nodes NOT in this face's interior
    const faceInteriorSet = new Set(interiorIds);
    const spineSet = new Set(spineVerts);
    const queue = [oppositeVert];
    superNodeIds.add(oppositeVert);
    while (queue.length > 0) {
      const u = queue.shift()!;
      for (const nbr of adj.get(u) ?? []) {
        if (superNodeIds.has(nbr) || faceInteriorSet.has(nbr) || spineSet.has(nbr)) continue;
        if (!positions.has(nbr)) continue;
        superNodeIds.add(nbr);
        queue.push(nbr);
      }
      // Also include leaves of this node
      for (const leaf of leaves) {
        if (leaf.parent === u && !superNodeIds.has(leaf.id)) {
          superNodeIds.add(leaf.id);
        }
      }
    }

    // Apply displacement to entire super node
    for (const id of superNodeIds) {
      const p = positions.get(id);
      if (p) {
        p.col += dx;
        p.row += dy;
      }
    }

    // Spread interior nodes within the expanded face
    const faceCx = (spA.col + spB.col + opP.col + dx) / 3;
    const faceCy = (spA.row + spB.row + opP.row + dy) / 3;
    for (let i = 0; i < interiorIds.length; i++) {
      const p = positions.get(interiorIds[i]);
      if (!p) continue;
      const gr = Math.floor(i / gridCols);
      const gc = i % gridCols;
      p.col = faceCx + (gc - (gridCols - 1) / 2) * NODE_W * 1.5;
      p.row = faceCy + (gr - (gridRows - 1) / 2) * NODE_H * 1.5;
    }

    // Compute L-shaped waypoints for the 2 non-spine edges to form rectangle.
    // opP was already displaced by the super node move above — use it directly.
    const corner1 = { col: spA.col, row: opP.row };
    const corner2 = { col: spB.col, row: opP.row };

    // Only store bend points — edge renderer connects from handle to corner to handle
    const key1a = `${spineVerts[0]}\0${oppositeVert}`;
    const key1b = `${oppositeVert}\0${spineVerts[0]}`;
    routes.set(key1a, [corner1]);
    routes.set(key1b, [corner1]);

    const key2a = `${spineVerts[1]}\0${oppositeVert}`;
    const key2b = `${oppositeVert}\0${spineVerts[1]}`;
    routes.set(key2a, [corner2]);
    routes.set(key2b, [corner2]);
  }

  return routes;
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
  if (nodeIds.length === 0) return { positions: new Map(), nonPlanarEdges: [], faceRoutes: new Map() };
  if (nodeIds.length === 1) {
    return {
      positions: new Map([[nodeIds[0], { col: 0, row: 0 }]]),
      nonPlanarEdges: [], faceRoutes: new Map(),
    };
  }
  if (nodeIds.length === 2) {
    return {
      positions: new Map([
        [nodeIds[0], { col: 0, row: 0 }],
        [nodeIds[1], { col: 1, row: 0 }],
      ]),
      nonPlanarEdges: [], faceRoutes: new Map(),
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
  const coreEdges = dedupedEdges.filter(([u, v]) => coreSet.has(u) && coreSet.has(v));

  // Step 3: If core is too small (e.g. star graph), use KK directly
  if (coreIds.length <= 2) {
    const positions = new Map<string, { col: number; row: number }>();
    const hub = coreIds[0] ?? nodeIds[0];
    positions.set(hub, { col: 0, row: 0 });
    if (coreIds[1]) positions.set(coreIds[1], { col: 2, row: 0 });
    kamadaKawai(positions, coreIds.length > 0 ? coreIds : [hub], coreEdges);
    placeLeaves(positions, leaves, realAdj, new Set(coreIds));
    return { positions, nonPlanarEdges: [], faceRoutes: new Map() };
  }

  // Step 4: Classify, augment, triangulate the CORE graph
  const coreClassification = classifyEdges(coreIds, coreEdges);
  augmentBiconnected(coreIds, coreClassification.planarEdges, coreClassification.embedding);
  // Save faces BEFORE triangulation — these are the real graph faces (polygons, not all triangles).
  // Triangulation subdivides them; interior nodes end up inside these original faces.
  const preTrFaces = allFaces(coreClassification.embedding);
  triangulate(coreClassification.embedding);

  // Step 5: FPP on core graph
  const { positions, outerContour } = fppPlace(coreClassification.embedding);

  // Step 6: Normalize — uniform scale to compact size
  {
    let totalLen = 0, edgeCount = 0;
    for (const [u, v] of coreEdges) {
      const pu = positions.get(u), pv = positions.get(v);
      if (pu && pv) {
        totalLen += Math.sqrt((pu.col - pv.col) ** 2 + (pu.row - pv.row) ** 2);
        edgeCount++;
      }
    }
    const meanLen = edgeCount > 0 ? totalLen / edgeCount : 1;
    // More nodes need more room — faces contain multiple nodes that need space
    const targetLen = 2.0 + positions.size * 0.15;
    if (meanLen > 0.01) {
      const scale = targetLen / meanLen;
      let cx = 0, cy = 0;
      for (const p of positions.values()) { cx += p.col; cy += p.row; }
      cx /= positions.size; cy /= positions.size;
      for (const p of positions.values()) {
        p.col = (p.col - cx) * scale;
        p.row = (p.row - cy) * scale;
      }
    }
  }

  // Step 7: Place leaf nodes — inside parent's face if parent is interior,
  // outside (widest gap) if parent is on outer contour
  placeLeaves(positions, leaves, realAdj, outerContour);

  // Step 8: Expand cramped faces — route 2 triangle edges to form rectangles,
  // push boundary super nodes outward to fit interior nodes
  const faceRoutes = expandCrampedFaces(positions, preTrFaces, outerContour, leaves, realAdj);
  console.log(`[FaceExpand] ${faceRoutes.size} edges routed`);

  return { positions, nonPlanarEdges: coreClassification.nonPlanarEdges, faceRoutes };
}

/**
 * Layout a potentially disconnected graph.
 * Each connected component is laid out separately, then packed in a row.
 */
export async function layoutGraph(
  nodeIds: string[],
  edges: EdgePair[],
): Promise<PlanarLayoutResult> {
  if (nodeIds.length === 0) return { positions: new Map(), nonPlanarEdges: [], faceRoutes: new Map() };

  const dedupedEdges = dedupeEdges(edges);
  const components = connectedComponents(nodeIds, dedupedEdges);

  if (components.length === 1) {
    return await planarLayout(nodeIds, dedupedEdges);
  }

  // Layout each component separately
  const allPositions = new Map<string, { col: number; row: number }>();
  const allNonPlanar: EdgePair[] = [];
  const allFaceRoutes = new Map<string, { col: number; row: number }[]>();
  let colOffset = 0;

  for (const comp of components) {
    const compSet = new Set(comp);
    const compEdges = dedupedEdges.filter(([u, v]) => compSet.has(u) && compSet.has(v));
    const result = await planarLayout(comp, compEdges);

    let maxCol = 0;
    for (const [id, pos] of result.positions) {
      allPositions.set(id, { col: pos.col + colOffset, row: pos.row });
      maxCol = Math.max(maxCol, pos.col);
    }

    allNonPlanar.push(...result.nonPlanarEdges);
    for (const [key, route] of result.faceRoutes) {
      allFaceRoutes.set(key, route.map((p) => ({ col: p.col + colOffset, row: p.row })));
    }
    colOffset += maxCol + 3;
  }

  return { positions: allPositions, nonPlanarEdges: allNonPlanar, faceRoutes: allFaceRoutes };
}
