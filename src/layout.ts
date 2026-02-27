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

  // Sort hubs by degree (highest first)
  const hubs = nodes
    .filter((n) => (adj.get(n.id)?.length ?? 0) >= 2)
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
      posMap.set(item.id, {
        x: cx + Math.cos(finalAngle) * item.dist - nw(nn) / 2,
        y: cy + Math.sin(finalAngle) * item.dist - nh(nn) / 2,
      });
      placed.add(item.id);
    }
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
 * Run ELK stress layout + compass snapping + crossing reduction.
 *
 * When `groups` are provided, group members become children of a synthetic
 * compound node in the ELK graph so they are laid out together. After layout,
 * child positions are converted from parent-relative to absolute.
 */
export async function autoLayout(
  nodes: C4Node[],
  edges: C4Edge[],
  groups?: Group[],
): Promise<C4Node[]> {
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
  return uncrossEdges(snapped, filteredEdges, groupMap);
}
