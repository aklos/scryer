import ELK from "elkjs/lib/elk.bundled.js";
import type { C4Node, C4Edge, Group } from "./types";

const NODE_W = 180;
const NODE_H = 160;

const elk = new ELK();

// The 8 compass directions: E, SE, S, SW, W, NW, N, NE
const COMPASS_8 = Array.from({ length: 8 }, (_, i) => (i * Math.PI) / 4);

/**
 * Post-process: snap neighbors of hub nodes to the 8 compass directions.
 *
 * Processes hubs in order of decreasing degree. Once a node has been
 * repositioned by one hub, it's locked — later hubs skip it.
 *
 * When `groupedNodes` is provided, a grouped node will only be repositioned
 * by a hub in the same group — this prevents scattering group members.
 */
function spreadAroundHubs(
  nodes: C4Node[],
  edges: C4Edge[],
  groupedNodes?: Map<string, string>,
): C4Node[] {
  const adj = new Map<string, string[]>();
  const nodeIds = new Set(nodes.map((n) => n.id));
  for (const e of edges) {
    if (!nodeIds.has(e.source) || !nodeIds.has(e.target)) continue;
    if (!adj.has(e.source)) adj.set(e.source, []);
    if (!adj.has(e.target)) adj.set(e.target, []);
    adj.get(e.source)!.push(e.target);
    adj.get(e.target)!.push(e.source);
  }

  const posMap = new Map(nodes.map((n) => [n.id, { ...n.position }]));
  const nw = (n: C4Node) => n.measured?.width ?? NODE_W;
  const nh = (n: C4Node) => n.measured?.height ?? NODE_H;
  const nodeMap = new Map(nodes.map((n) => [n.id, n]));

  // Hub threshold: only nodes with above-median degree, minimum 3.
  // Low-degree nodes (like a person with 2 edges) should float freely.
  const degrees = nodes.map((n) => adj.get(n.id)?.length ?? 0).filter((d) => d > 0);
  degrees.sort((a, b) => a - b);
  const median = degrees.length > 0 ? degrees[Math.floor(degrees.length / 2)] : 0;
  const hubThreshold = Math.max(3, median);

  // Sort hubs by degree (highest first)
  const hubs = nodes
    .filter((n) => (adj.get(n.id)?.length ?? 0) >= hubThreshold)
    .sort((a, b) => (adj.get(b.id)?.length ?? 0) - (adj.get(a.id)?.length ?? 0));

  // Track repositioned nodes — don't move them again
  const placed = new Set<string>();

  for (const node of hubs) {
    if (placed.has(node.id)) continue;

    // Skip grouped hubs entirely — ELK's compound layout already positioned
    // group members well. Compass-snapping would override those positions.
    if (groupedNodes?.has(node.id)) continue;

    const neighbors = adj.get(node.id)!;
    const hubPos = posMap.get(node.id)!;
    const cx = hubPos.x + nw(node) / 2;
    const cy = hubPos.y + nh(node) / 2;

    // Only process neighbors that haven't been placed yet.
    // Skip grouped neighbors — don't pull them out of their cluster.
    const freeNeighbors = neighbors.filter((nid) => {
      if (placed.has(nid)) return false;
      if (groupedNodes?.has(nid)) return false;
      return true;
    });

    if (freeNeighbors.length === 0) continue;

    // More neighbors → more spacing needed so labels don't overlap.
    // Use freeNeighbors count (not total degree) — when groups filter out
    // cross-group neighbors, spacing should reflect only the nodes being spread.
    const minDist = 200 + freeNeighbors.length * 40;

    // Compute current angle + distance for each free neighbor
    const items: { id: string; angle: number; dist: number }[] = [];
    for (const nid of freeNeighbors) {
      const nn = nodeMap.get(nid)!;
      const nPos = posMap.get(nid)!;
      const dx = (nPos.x + nw(nn) / 2) - cx;
      const dy = (nPos.y + nh(nn) / 2) - cy;
      items.push({
        id: nid,
        angle: Math.atan2(dy, dx),
        dist: Math.max(Math.sqrt(dx * dx + dy * dy), minDist),
      });
    }

    // Figure out which compass slots are blocked by already-placed neighbors
    const taken = new Set<number>();
    for (const nid of neighbors) {
      if (!placed.has(nid)) continue;
      const nn = nodeMap.get(nid)!;
      const nPos = posMap.get(nid)!;
      const dx = (nPos.x + nw(nn) / 2) - cx;
      const dy = (nPos.y + nh(nn) / 2) - cy;
      const a = ((Math.atan2(dy, dx) % (2 * Math.PI)) + 2 * Math.PI) % (2 * Math.PI);
      let bestSlot = 0;
      let bestDist = Infinity;
      for (let s = 0; s < COMPASS_8.length; s++) {
        let d = Math.abs(COMPASS_8[s] - a);
        if (d > Math.PI) d = 2 * Math.PI - d;
        if (d < bestDist) { bestDist = d; bestSlot = s; }
      }
      taken.add(bestSlot);
    }

    // Greedy: sort by angle, assign nearest free compass slot
    items.sort((a, b) => a.angle - b.angle);

    for (const item of items) {
      const a = ((item.angle % (2 * Math.PI)) + 2 * Math.PI) % (2 * Math.PI);

      let bestSlot = -1;
      let bestDist = Infinity;
      for (let s = 0; s < COMPASS_8.length; s++) {
        if (taken.has(s)) continue;
        let d = Math.abs(COMPASS_8[s] - a);
        if (d > Math.PI) d = 2 * Math.PI - d;
        if (d < bestDist) { bestDist = d; bestSlot = s; }
      }

      const finalAngle = bestSlot >= 0 ? COMPASS_8[bestSlot] : item.angle;
      if (bestSlot >= 0) taken.add(bestSlot);

      const nn = nodeMap.get(item.id)!;
      let nx = cx + Math.cos(finalAngle) * item.dist - nw(nn) / 2;
      let ny = cy + Math.sin(finalAngle) * item.dist - nh(nn) / 2;
      // For cardinal directions, align to hub's grid line so snap doesn't misalign
      const cosA = Math.cos(finalAngle), sinA = Math.sin(finalAngle);
      if (Math.abs(cosA) < 0.01) nx = hubPos.x + nw(node) / 2 - nw(nn) / 2;
      if (Math.abs(sinA) < 0.01) ny = hubPos.y + nh(node) / 2 - nh(nn) / 2;
      posMap.set(item.id, { x: nx, y: ny });
      placed.add(item.id);
    }
  }

  return nodes.map((n) => {
    const pos = posMap.get(n.id);
    return pos ? { ...n, position: pos } : n;
  });
}

/** Cohen-Sutherland: does segment (x1,y1)→(x2,y2) intersect rect (rx,ry,rw,rh)? */
function segmentIntersectsRect(
  x1: number, y1: number, x2: number, y2: number,
  rx: number, ry: number, rw: number, rh: number,
): boolean {
  const xmin = rx, xmax = rx + rw, ymin = ry, ymax = ry + rh;
  const code = (x: number, y: number) => {
    let c = 0;
    if (x < xmin) c |= 1; else if (x > xmax) c |= 2;
    if (y < ymin) c |= 4; else if (y > ymax) c |= 8;
    return c;
  };
  let c1 = code(x1, y1), c2 = code(x2, y2);
  let sx = x1, sy = y1, ex = x2, ey = y2;
  for (let i = 0; i < 20; i++) {
    if ((c1 | c2) === 0) return true;
    if ((c1 & c2) !== 0) return false;
    const c = c1 !== 0 ? c1 : c2;
    let x = 0, y = 0;
    if (c & 8) { x = sx + (ex - sx) * (ymax - sy) / (ey - sy); y = ymax; }
    else if (c & 4) { x = sx + (ex - sx) * (ymin - sy) / (ey - sy); y = ymin; }
    else if (c & 2) { y = sy + (ey - sy) * (xmax - sx) / (ex - sx); x = xmax; }
    else if (c & 1) { y = sy + (ey - sy) * (xmin - sx) / (ex - sx); x = xmin; }
    if (c === c1) { sx = x; sy = y; c1 = code(sx, sy); }
    else { ex = x; ey = y; c2 = code(ex, ey); }
  }
  return false;
}

/** How far (radians) an angle is from its nearest compass direction. */
function compassDeviation(dx: number, dy: number): number {
  const a = ((Math.atan2(dy, dx) % (2 * Math.PI)) + 2 * Math.PI) % (2 * Math.PI);
  let best = Infinity;
  for (const c of COMPASS_8) {
    let d = Math.abs(c - a);
    if (d > Math.PI) d = 2 * Math.PI - d;
    if (d < best) best = d;
  }
  return best;
}

/**
 * After spreadAroundHubs: find floaters (nodes not placed as hubs or spokes),
 * then reposition them and slide connected spokes along their radials until
 * as many floater edges as possible align to compass directions.
 */
function straightenFloaterEdges(
  nodes: C4Node[],
  edges: C4Edge[],
  groupedNodes?: Map<string, string>,
): C4Node[] {
  const nw = (n: C4Node) => n.measured?.width ?? NODE_W;
  const nh = (n: C4Node) => n.measured?.height ?? NODE_H;
  const nodeMap = new Map(nodes.map((n) => [n.id, n]));
  const nodeIds = new Set(nodes.map((n) => n.id));
  const posMap = new Map(nodes.map((n) => [n.id, { ...n.position }]));

  // Build adjacency
  const adj = new Map<string, Set<string>>();
  for (const e of edges) {
    if (!nodeIds.has(e.source) || !nodeIds.has(e.target)) continue;
    if (!adj.has(e.source)) adj.set(e.source, new Set());
    if (!adj.has(e.target)) adj.set(e.target, new Set());
    adj.get(e.source)!.add(e.target);
    adj.get(e.target)!.add(e.source);
  }
  const degree = (id: string) => adj.get(id)?.size ?? 0;

  // Identify spokes: angle to highest-degree neighbor is compass-aligned.
  // Record their hub and locked angle.
  const spokeInfo = new Map<string, { hubId: string; angle: number; dist: number }>();
  for (const n of nodes) {
    if (groupedNodes?.has(n.id)) continue;
    const neighbors = adj.get(n.id);
    if (!neighbors) continue;
    let hubId = "";
    let hubDeg = 0;
    for (const nid of neighbors) {
      const d = degree(nid);
      if (d > hubDeg && d > degree(n.id)) { hubDeg = d; hubId = nid; }
    }
    if (!hubId) continue;
    const hub = nodeMap.get(hubId)!;
    const hp = posMap.get(hubId)!;
    const hcx = hp.x + nw(hub) / 2, hcy = hp.y + nh(hub) / 2;
    const p = posMap.get(n.id)!;
    const ncx = p.x + nw(n) / 2, ncy = p.y + nh(n) / 2;
    const dx = ncx - hcx, dy = ncy - hcy;
    // Only lock single-edge nodes as spokes. Nodes with 2+ edges need
    // freedom to reposition (they become floaters instead).
    if (degree(n.id) <= 1 && compassDeviation(dx, dy) < 0.1) {
      const angle = Math.atan2(dy, dx);
      const dist = Math.sqrt(dx * dx + dy * dy);
      spokeInfo.set(n.id, { hubId, angle, dist });
    }
  }

  const hubIds = new Set([...spokeInfo.values()].map((s) => s.hubId));

  // Floaters: not a spoke, not a hub, has neighbors
  const floaters: string[] = [];
  for (const n of nodes) {
    if (spokeInfo.has(n.id) || hubIds.has(n.id)) continue;
    if (groupedNodes?.has(n.id)) continue;
    if ((adj.get(n.id)?.size ?? 0) === 0) continue;
    floaters.push(n.id);
  }
  console.log(`[straightenFloaterEdges] spokes: [${[...spokeInfo.keys()].map(id => nodeMap.get(id)?.data?.name ?? id).join(', ')}], hubs: [${[...hubIds].map(id => nodeMap.get(id)?.data?.name ?? id).join(', ')}], floaters: [${floaters.map(id => nodeMap.get(id)?.data?.name ?? id).join(', ')}]`);

  if (floaters.length === 0) return nodes;

  // Helper: center of a node given current posMap
  const center = (id: string) => {
    const n = nodeMap.get(id)!;
    const p = posMap.get(id)!;
    return { x: p.x + nw(n) / 2, y: p.y + nh(n) / 2 };
  };

  // Score: compass deviation for all edges of a node + proximity penalty
  // + edge-through-node penalty
  const MIN_GAP = 250;
  const edgeAlignmentScore = (nodeId: string) => {
    const c = center(nodeId);
    let score = 0;
    const neighbors = adj.get(nodeId) ?? [];
    for (const nid of neighbors) {
      const nc = center(nid);
      score += compassDeviation(nc.x - c.x, nc.y - c.y);
    }
    // Penalize being too close to any other node
    for (const n of nodes) {
      if (n.id === nodeId) continue;
      const nc = center(n.id);
      const dx = nc.x - c.x, dy = nc.y - c.y;
      const dist = Math.sqrt(dx * dx + dy * dy);
      if (dist < MIN_GAP) score += (MIN_GAP - dist) / MIN_GAP * 5;
    }
    // Penalize edges that pass through other nodes
    for (const nid of neighbors) {
      const nc = center(nid);
      for (const n of nodes) {
        if (n.id === nodeId || n.id === nid) continue;
        const p = posMap.get(n.id)!;
        const margin = 20;
        if (segmentIntersectsRect(c.x, c.y, nc.x, nc.y,
            p.x - margin, p.y - margin,
            nw(n) + margin * 2, nh(n) + margin * 2)) {
          score += 10;
        }
      }
    }
    // Penalize edge crossings (this node's edges crossing other edges)
    for (const nid of neighbors) {
      const nc = center(nid);
      for (const e of edges) {
        if (e.source === nodeId || e.target === nodeId) continue;
        if (e.source === nid || e.target === nid) continue;
        const ec1 = center(e.source);
        const ec2 = center(e.target);
        if (segmentsIntersect(c.x, c.y, nc.x, nc.y,
            ec1.x, ec1.y, ec2.x, ec2.y)) {
          score += 20;
        }
      }
    }
    return score;
  };

  // Phase 1: Position each floater to minimize compass deviation of its edges.
  for (const fid of floaters) {
    const fn = nodeMap.get(fid)!;
    const w = nw(fn), h = nh(fn);
    const neighbors = adj.get(fid);
    if (!neighbors) continue;

    let bestScore = edgeAlignmentScore(fid);
    let bestPos = { ...posMap.get(fid)! };

    // Candidate origins: each neighbor + centroid of all neighbors
    const origins: { x: number; y: number }[] = [];
    for (const nid of neighbors) origins.push(center(nid));
    if (origins.length >= 2) {
      const cx = origins.reduce((s, o) => s + o.x, 0) / origins.length;
      const cy = origins.reduce((s, o) => s + o.y, 0) / origins.length;
      origins.push({ x: cx, y: cy });
    }

    for (const origin of origins) {
      for (const angle of COMPASS_8) {
        for (const dist of [300, 400, 550, 700]) {
          const tx = origin.x + Math.cos(angle) * dist - w / 2;
          const ty = origin.y + Math.sin(angle) * dist - h / 2;
          posMap.set(fid, { x: tx, y: ty });
          const score = edgeAlignmentScore(fid);
          if (score < bestScore) {
            bestScore = score;
            bestPos = { x: tx, y: ty };
          }
        }
      }
    }
    posMap.set(fid, bestPos);
  }

  // Phase 2: Slide spokes connected to floaters along their locked radial.
  // Narrow range (0.9–1.5x) to preserve hub-spoke spacing.
  for (const [spokeId, info] of spokeInfo) {
    const neighbors = adj.get(spokeId);
    if (!neighbors) continue;
    let connectsFloater = false;
    for (const fid of floaters) {
      if (neighbors.has(fid)) { connectsFloater = true; break; }
    }
    if (!connectsFloater) continue;

    const sn = nodeMap.get(spokeId)!;
    const hub = nodeMap.get(info.hubId)!;
    const hp = posMap.get(info.hubId)!;
    const hcx = hp.x + nw(hub) / 2, hcy = hp.y + nh(hub) / 2;

    const scoreSpoke = () => {
      let s = edgeAlignmentScore(spokeId);
      for (const fid of floaters) {
        if (neighbors.has(fid)) s += edgeAlignmentScore(fid);
      }
      // Penalize when any edge passes through or near this spoke
      const sp = posMap.get(spokeId)!;
      const sw = nw(sn), sh = nh(sn);
      const margin = 120;
      for (const e of edges) {
        if (e.source === spokeId || e.target === spokeId) continue;
        const sc = center(e.source);
        const tc = center(e.target);
        if (segmentIntersectsRect(sc.x, sc.y, tc.x, tc.y,
            sp.x - margin, sp.y - margin,
            sw + margin * 2, sh + margin * 2)) {
          s += 10;
        }
      }
      return s;
    };

    let bestScore = scoreSpoke();
    let bestPos = { ...posMap.get(spokeId)! };

    for (let scale = 0.9; scale <= 1.8; scale += 0.05) {
      const d = info.dist * scale;
      const nx = hcx + Math.cos(info.angle) * d - nw(sn) / 2;
      const ny = hcy + Math.sin(info.angle) * d - nh(sn) / 2;
      posMap.set(spokeId, { x: nx, y: ny });
      const score = scoreSpoke();
      if (score < bestScore) {
        bestScore = score;
        bestPos = { x: nx, y: ny };
      }
    }
    posMap.set(spokeId, bestPos);
  }

  return nodes.map((n) => {
    const pos = posMap.get(n.id);
    return pos ? { ...n, position: pos } : n;
  });
}

/** Check if two line segments (p1→p2) and (p3→p4) intersect. */
function segmentsIntersect(
  p1x: number, p1y: number, p2x: number, p2y: number,
  p3x: number, p3y: number, p4x: number, p4y: number,
): boolean {
  const d1x = p2x - p1x, d1y = p2y - p1y;
  const d2x = p4x - p3x, d2y = p4y - p3y;
  const cross = d1x * d2y - d1y * d2x;
  if (Math.abs(cross) < 1e-10) return false; // parallel
  const t = ((p3x - p1x) * d2y - (p3y - p1y) * d2x) / cross;
  const u = ((p3x - p1x) * d1y - (p3y - p1y) * d1x) / cross;
  return t > 0.01 && t < 0.99 && u > 0.01 && u < 0.99;
}

/** Count how many edge pairs cross given current positions. */
function countCrossings(
  edges: C4Edge[],
  centers: Map<string, { x: number; y: number }>,
): number {
  let count = 0;
  for (let i = 0; i < edges.length; i++) {
    const a = centers.get(edges[i].source);
    const b = centers.get(edges[i].target);
    if (!a || !b) continue;
    for (let j = i + 1; j < edges.length; j++) {
      // Skip edges that share a node — they meet at a point, not a crossing
      if (edges[i].source === edges[j].source || edges[i].source === edges[j].target ||
          edges[i].target === edges[j].source || edges[i].target === edges[j].target) continue;
      const c = centers.get(edges[j].source);
      const d = centers.get(edges[j].target);
      if (!c || !d) continue;
      if (segmentsIntersect(a.x, a.y, b.x, b.y, c.x, c.y, d.x, d.y)) count++;
    }
  }
  return count;
}

/** Generate all permutations of an array (for small arrays only). */
function permutations<T>(arr: T[]): T[][] {
  if (arr.length <= 1) return [arr];
  const result: T[][] = [];
  for (let i = 0; i < arr.length; i++) {
    const rest = [...arr.slice(0, i), ...arr.slice(i + 1)];
    for (const perm of permutations(rest)) {
      result.push([arr[i], ...perm]);
    }
  }
  return result;
}

/**
 * Find crossing edges, collect the involved nodes into clusters,
 * then brute-force all permutations of each cluster's positions
 * to find the arrangement with fewest crossings.
 *
 * Clusters are capped at 6 nodes (720 permutations) — larger ones
 * are left as-is since brute force gets expensive.
 */
function uncrossEdges(
  nodes: C4Node[],
  edges: C4Edge[],
  groupedNodes?: Map<string, string>,
): C4Node[] {
  const nw = (n: C4Node) => n.measured?.width ?? NODE_W;
  const nh = (n: C4Node) => n.measured?.height ?? NODE_H;
  const posMap = new Map(nodes.map((n) => [n.id, { ...n.position }]));

  const centers = new Map<string, { x: number; y: number }>();
  const updateCenters = () => {
    for (const n of nodes) {
      const p = posMap.get(n.id)!;
      centers.set(n.id, { x: p.x + nw(n) / 2, y: p.y + nh(n) / 2 });
    }
  };
  updateCenters();

  if (countCrossings(edges, centers) === 0) {
    return nodes;
  }

  // Collect nodes involved in any crossing into connected clusters
  // Two nodes are in the same cluster if they're endpoints of crossing edges
  // that share a node (transitively connected through crossings)
  const crossingNodes = new Set<string>();
  const links = new Map<string, Set<string>>(); // adjacency between crossing-involved nodes

  for (let i = 0; i < edges.length; i++) {
    const a = centers.get(edges[i].source);
    const b = centers.get(edges[i].target);
    if (!a || !b) continue;
    for (let j = i + 1; j < edges.length; j++) {
      if (edges[i].source === edges[j].source || edges[i].source === edges[j].target ||
          edges[i].target === edges[j].source || edges[i].target === edges[j].target) continue;
      const c = centers.get(edges[j].source);
      const d = centers.get(edges[j].target);
      if (!c || !d) continue;
      if (!segmentsIntersect(a.x, a.y, b.x, b.y, c.x, c.y, d.x, d.y)) continue;

      // All 4 endpoints are part of this crossing cluster
      const ids = [edges[i].source, edges[i].target, edges[j].source, edges[j].target];
      for (const id of ids) {
        crossingNodes.add(id);
        if (!links.has(id)) links.set(id, new Set());
      }
      // Link nodes together — but only if they share the same group
      // (or are both ungrouped). This prevents position swaps that would
      // scatter group members across the canvas.
      for (const id1 of ids) {
        for (const id2 of ids) {
          if (id1 === id2) continue;
          if (groupedNodes) {
            const g1 = groupedNodes.get(id1);
            const g2 = groupedNodes.get(id2);
            if (g1 !== g2) continue; // different groups or one grouped/one not
          }
          links.get(id1)!.add(id2);
        }
      }
    }
  }

  if (crossingNodes.size === 0) return nodes;

  // BFS to find connected clusters
  const visited = new Set<string>();
  const clusters: string[][] = [];
  for (const start of crossingNodes) {
    if (visited.has(start)) continue;
    const cluster: string[] = [];
    const queue = [start];
    visited.add(start);
    while (queue.length > 0) {
      const id = queue.shift()!;
      cluster.push(id);
      for (const neighbor of links.get(id) ?? []) {
        if (!visited.has(neighbor)) {
          visited.add(neighbor);
          queue.push(neighbor);
        }
      }
    }
    clusters.push(cluster);
  }

  // For each small-enough cluster, try all permutations of positions
  for (const cluster of clusters) {
    if (cluster.length < 2 || cluster.length > 6) continue;

    const positions = cluster.map((id) => ({ ...posMap.get(id)! }));
    const perms = permutations(cluster);

    let bestCrossings = countCrossings(edges, centers);
    let bestPerm = cluster; // identity

    for (const perm of perms) {
      // Assign positions[i] to perm[i]
      for (let k = 0; k < perm.length; k++) {
        posMap.set(perm[k], positions[k]);
      }
      updateCenters();
      const c = countCrossings(edges, centers);
      if (c < bestCrossings) {
        bestCrossings = c;
        bestPerm = [...perm];
      }
    }

    // Apply best permutation
    for (let k = 0; k < bestPerm.length; k++) {
      posMap.set(bestPerm[k], positions[k]);
    }
    updateCenters();
  }

  return nodes.map((n) => {
    const pos = posMap.get(n.id);
    return pos ? { ...n, position: pos } : n;
  });
}


/**
 * Simple grid layout for code-level nodes (operations, processes, models).
 * Packs nodes into a compact multi-column grid — no stress layout needed
 * since code-level nodes typically have no edges. Reference nodes are
 * returned unchanged (their positions come from refPositions).
 */
export function gridLayout(nodes: C4Node[]): C4Node[] {
  const GAP_X = 40;
  const GAP_Y = 32;

  // Separate reference nodes — don't include them in the grid
  const gridNodes = nodes.filter((n) => !n.data._reference);
  const refNodes = nodes.filter((n) => n.data._reference);

  const COLS = Math.max(1, Math.min(4, Math.ceil(Math.sqrt(gridNodes.length))));

  // Compute max width per column and max height per row for alignment
  const colWidths: number[] = new Array(COLS).fill(0);
  const rowHeights: number[] = new Array(Math.ceil(gridNodes.length / COLS)).fill(0);
  for (let i = 0; i < gridNodes.length; i++) {
    const col = i % COLS;
    const row = Math.floor(i / COLS);
    const w = gridNodes[i].measured?.width ?? NODE_W;
    const h = gridNodes[i].measured?.height ?? NODE_H;
    colWidths[col] = Math.max(colWidths[col], w);
    rowHeights[row] = Math.max(rowHeights[row], h);
  }

  // Compute cumulative x/y offsets
  const colX = [0];
  for (let c = 1; c < COLS; c++) colX.push(colX[c - 1] + colWidths[c - 1] + GAP_X);
  const rowY = [0];
  for (let r = 1; r < rowHeights.length; r++) rowY.push(rowY[r - 1] + rowHeights[r - 1] + GAP_Y);

  const positioned = gridNodes.map((n, i) => ({
    ...n,
    position: {
      x: colX[i % COLS],
      y: rowY[Math.floor(i / COLS)],
    },
  }));

  // Place reference nodes in a row below the grid
  if (refNodes.length > 0) {
    const gridBottom = rowY.length > 0
      ? rowY[rowY.length - 1] + (rowHeights[rowHeights.length - 1] || NODE_H)
      : 0;
    const refY = gridBottom + GAP_Y * 3;
    let refX = 0;
    const positionedRefs = refNodes.map((n) => {
      const pos = { x: refX, y: refY };
      refX += (n.measured?.width ?? NODE_W) + GAP_X;
      return { ...n, position: pos };
    });
    return [...positioned, ...positionedRefs];
  }

  return positioned;
}

/**
 * Final deconfliction: find nodes whose edges cross other edges,
 * and reposition them to reduce crossings.
 */
function deconflictEdges(
  nodes: C4Node[],
  edges: C4Edge[],
  groupedNodes?: Map<string, string>,
): C4Node[] {
  const nw = (n: C4Node) => n.measured?.width ?? NODE_W;
  const nh = (n: C4Node) => n.measured?.height ?? NODE_H;
  const nodeMap = new Map(nodes.map((n) => [n.id, n]));
  const posMap = new Map(nodes.map((n) => [n.id, { ...n.position }]));
  const nodeIds = new Set(nodes.map((n) => n.id));

  const adj = new Map<string, Set<string>>();
  for (const e of edges) {
    if (!nodeIds.has(e.source) || !nodeIds.has(e.target)) continue;
    if (!adj.has(e.source)) adj.set(e.source, new Set());
    if (!adj.has(e.target)) adj.set(e.target, new Set());
    adj.get(e.source)!.add(e.target);
    adj.get(e.target)!.add(e.source);
  }
  const degree = (id: string) => adj.get(id)?.size ?? 0;

  const center = (id: string) => {
    const n = nodeMap.get(id)!;
    const p = posMap.get(id)!;
    return { x: p.x + nw(n) / 2, y: p.y + nh(n) / 2 };
  };

  // Count edge crossings involving a specific node's edges
  const nodeCrossings = (nodeId: string) => {
    let count = 0;
    for (const e of edges) {
      if (e.source !== nodeId && e.target !== nodeId) continue;
      const a = center(e.source);
      const b = center(e.target);
      for (const other of edges) {
        if (other === e) continue;
        // Skip edges that share a node
        if (e.source === other.source || e.source === other.target ||
            e.target === other.source || e.target === other.target) continue;
        const c = center(other.source);
        const d = center(other.target);
        if (segmentsIntersect(a.x, a.y, b.x, b.y, c.x, c.y, d.x, d.y)) count++;
      }
    }
    return count;
  };

  // Find nodes involved in crossings, prefer moving lower-degree nodes
  const crossingNodes: { id: string; crossings: number }[] = [];
  for (const n of nodes) {
    if (groupedNodes?.has(n.id)) continue;
    const c = nodeCrossings(n.id);
    if (c > 0) crossingNodes.push({ id: n.id, crossings: c });
  }
  if (crossingNodes.length === 0) return nodes;

  // Sort: try moving lower-degree nodes first (they're more flexible)
  crossingNodes.sort((a, b) => degree(a.id) - degree(b.id));

  for (const { id: moveId } of crossingNodes) {
    // Re-check — earlier moves may have resolved this node's crossings
    if (nodeCrossings(moveId) === 0) continue;

    const mn = nodeMap.get(moveId)!;
    const w = nw(mn), h = nh(mn);
    const neighbors = adj.get(moveId);
    if (!neighbors) continue;

    // Score: edge crossings (heavy) + edge-through-node + compass + proximity
    const score = () => {
      const mc = center(moveId);
      let s = nodeCrossings(moveId) * 20;
      for (const nid of neighbors) {
        const nc = center(nid);
        s += compassDeviation(nc.x - mc.x, nc.y - mc.y) * 0.5;
        // Edge-through-node
        for (const n of nodes) {
          if (n.id === moveId || n.id === nid) continue;
          const p = posMap.get(n.id)!;
          if (segmentIntersectsRect(mc.x, mc.y, nc.x, nc.y,
              p.x - 20, p.y - 20,
              nw(n) + 40, nh(n) + 40)) {
            s += 15;
          }
        }
      }
      for (const n of nodes) {
        if (n.id === moveId) continue;
        const nc = center(n.id);
        const dx = nc.x - mc.x, dy = nc.y - mc.y;
        const dist = Math.sqrt(dx * dx + dy * dy);
        if (dist < 250) s += (250 - dist) / 250 * 3;
      }
      return s;
    };

    let bestScore = score();
    let bestPos = { ...posMap.get(moveId)! };

    // Candidate origins: each neighbor + centroid of all neighbors
    const origins: { x: number; y: number }[] = [];
    for (const nid of neighbors) origins.push(center(nid));
    if (origins.length >= 2) {
      const cx = origins.reduce((s, o) => s + o.x, 0) / origins.length;
      const cy = origins.reduce((s, o) => s + o.y, 0) / origins.length;
      origins.push({ x: cx, y: cy });
    }

    for (const origin of origins) {
      for (const angle of COMPASS_8) {
        for (const dist of [300, 400, 550, 700]) {
          const tx = origin.x + Math.cos(angle) * dist - w / 2;
          const ty = origin.y + Math.sin(angle) * dist - h / 2;
          posMap.set(moveId, { x: tx, y: ty });
          const s = score();
          if (s < bestScore) {
            bestScore = s;
            bestPos = { x: tx, y: ty };
          }
        }
      }
    }
    posMap.set(moveId, bestPos);
  }

  return nodes.map((n) => {
    const pos = posMap.get(n.id);
    return pos ? { ...n, position: pos } : n;
  });
}

/**
 * Run ELK stress layout + compass snapping + crossing reduction.
 *
 * When `groups` are provided, group members become children of a synthetic
 * compound node in the ELK graph so they are laid out together. After layout,
 * child positions are converted from parent-relative to absolute.
 *
 * When `codeLevel` is true, uses a compact grid instead — code-level nodes
 * (operations, processes, models) don't have architectural edges and should
 * be packed tight.
 */
export async function autoLayout(
  nodes: C4Node[],
  edges: C4Edge[],
  groups?: Group[],
  codeLevel?: boolean,
): Promise<C4Node[]> {
  if (codeLevel) return gridLayout(nodes);
  const nodeIds = new Set(nodes.map((n) => n.id));

  const filteredEdges = edges.filter(
    (e) => nodeIds.has(e.source) && nodeIds.has(e.target),
  );

  // Build membership map: nodeId → groupId (only for groups with ≥2 visible members)
  const nodeToGroup = new Map<string, string>();
  const activeGroups: Group[] = [];
  if (groups) {
    for (const g of groups) {
      const visibleMembers = g.memberIds.filter((id) => nodeIds.has(id));
      if (visibleMembers.length >= 2) {
        activeGroups.push({ ...g, memberIds: visibleMembers });
        for (const id of visibleMembers) {
          nodeToGroup.set(id, g.id);
        }
      }
    }
  }

  const GROUP_PADDING = 40;

  type ElkChild = {
    id: string;
    width: number;
    height: number;
    children?: ElkChild[];
    layoutOptions?: Record<string, string>;
  };

  // Build top-level children: ungrouped nodes + synthetic group parents
  const topChildren: ElkChild[] = [];

  // Add ungrouped nodes
  for (const n of nodes) {
    if (!nodeToGroup.has(n.id)) {
      topChildren.push({
        id: n.id,
        width: n.measured?.width ?? NODE_W,
        height: n.measured?.height ?? NODE_H,
      });
    }
  }

  // Add synthetic group parent nodes with members as children
  const nodeMap = new Map(nodes.map((n) => [n.id, n]));
  for (const g of activeGroups) {
    const children: ElkChild[] = g.memberIds.map((id) => {
      const n = nodeMap.get(id)!;
      return {
        id: n.id,
        width: n.measured?.width ?? NODE_W,
        height: n.measured?.height ?? NODE_H,
      };
    });
    topChildren.push({
      id: `__group__${g.id}`,
      width: 0,
      height: 0,
      children,
      layoutOptions: {
        "elk.algorithm": "stress",
        "elk.stress.desiredEdgeLength": "300",
        "elk.spacing.nodeNode": "150",
        "elk.padding": `[top=${GROUP_PADDING},left=${GROUP_PADDING},bottom=${GROUP_PADDING},right=${GROUP_PADDING}]`,
      },
    });
  }

  // Partition edges: intra-group edges go inside the compound node,
  // cross-group edges are "lifted" to reference synthetic parent nodes so
  // ELK's stress algorithm sees same-level connections.
  type ElkEdge = { id: string; sources: string[]; targets: string[] };
  const topEdges: ElkEdge[] = [];
  const groupEdges = new Map<string, ElkEdge[]>();
  for (const g of activeGroups) groupEdges.set(g.id, []);

  // Helper: lift a node ID to its synthetic group parent if grouped
  const liftToGroup = (id: string): string => {
    const g = nodeToGroup.get(id);
    return g ? `__group__${g}` : id;
  };

  // Deduplicate lifted top-level edges (multiple internal edges can lift to same pair)
  const seenTopEdges = new Set<string>();

  for (const e of filteredEdges) {
    const sg = nodeToGroup.get(e.source);
    const tg = nodeToGroup.get(e.target);
    if (sg && tg && sg === tg) {
      // Both endpoints in the same group — edge goes inside compound node
      groupEdges.get(sg)!.push({ id: e.id, sources: [e.source], targets: [e.target] });
    } else {
      // Cross-group or ungrouped — lift to top-level synthetic parents
      const liftedSource = liftToGroup(e.source);
      const liftedTarget = liftToGroup(e.target);
      if (liftedSource === liftedTarget) continue; // skip self-loops after lifting
      const key = `${liftedSource}->${liftedTarget}`;
      if (seenTopEdges.has(key)) continue;
      seenTopEdges.add(key);
      topEdges.push({ id: e.id, sources: [liftedSource], targets: [liftedTarget] });
    }
  }

  // Attach intra-group edges to compound nodes
  for (const child of topChildren) {
    const groupId = child.id.startsWith("__group__") ? child.id.slice("__group__".length) : null;
    if (groupId && groupEdges.has(groupId)) {
      (child as ElkChild & { edges?: ElkEdge[] }).edges = groupEdges.get(groupId);
    }
  }

  // When compound nodes are present, their computed size can be large (500+ px).
  // Scale top-level spacing so ELK doesn't pack them on top of each other.
  let topEdgeLength = 350;
  let topNodeSpacing = 200;
  if (activeGroups.length > 0) {
    // Estimate the largest compound node dimension
    let maxCompoundDim = 0;
    for (const child of topChildren) {
      if (child.children && child.children.length > 0) {
        const totalChildArea = child.children.reduce(
          (sum, c) => sum + Math.max(c.width, c.height), 0,
        );
        maxCompoundDim = Math.max(maxCompoundDim, totalChildArea);
      }
    }
    topEdgeLength = Math.max(topEdgeLength, maxCompoundDim + 150);
    topNodeSpacing = Math.max(topNodeSpacing, maxCompoundDim * 0.5);
  }

  const graph = {
    id: "root",
    layoutOptions: {
      "elk.algorithm": "stress",
      "elk.stress.desiredEdgeLength": String(topEdgeLength),
      "elk.spacing.nodeNode": String(topNodeSpacing),
    },
    children: topChildren,
    edges: topEdges,
  };

  const laid = await elk.layout(graph);

  // Extract positions — compound children need parent offset added
  const posMap = new Map<string, { x: number; y: number }>();
  for (const child of laid.children ?? []) {
    if (child.id.startsWith("__group__")) {
      // Synthetic group parent — extract child positions with offset
      const px = child.x ?? 0;
      const py = child.y ?? 0;
      for (const gc of child.children ?? []) {
        posMap.set(gc.id, { x: px + (gc.x ?? 0), y: py + (gc.y ?? 0) });
      }
    } else {
      posMap.set(child.id, { x: child.x ?? 0, y: child.y ?? 0 });
    }
  }

  const positioned = nodes.map((node) => {
    const pos = posMap.get(node.id);
    return pos ? { ...node, position: pos } : node;
  });

  const groupMap = nodeToGroup.size > 0 ? nodeToGroup : undefined;
  const snapped = spreadAroundHubs(positioned, filteredEdges, groupMap);
  const straightened = straightenFloaterEdges(snapped, filteredEdges, groupMap);
  const uncrossed = uncrossEdges(straightened, filteredEdges, groupMap);
  const deconflicted = deconflictEdges(uncrossed, filteredEdges, groupMap);

  // Snap final positions to 20px grid
  return deconflicted.map((n) => ({
    ...n,
    position: { x: Math.round(n.position.x / 20) * 20, y: Math.round(n.position.y / 20) * 20 },
  }));
}
