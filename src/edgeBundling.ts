/**
 * Edge bundling via cardinal magnets.
 *
 * For hub nodes (≥ 3 edges), place 4 magnet points just outside the hub
 * boundary (top, right, bottom, left). Each edge is assigned to the nearest
 * magnet based on its angle to the target. Edges sharing a magnet get a
 * _route waypoint at the magnet point, creating a trunk+fan visual.
 *
 * Rules:
 *   1. Only nodes with ≥ 3 edges are hub candidates.
 *   2. Each edge is assigned to the nearest cardinal magnet (4 quadrants).
 *   3. Magnets with ≥ 2 edges emit _route waypoints; singletons left alone.
 *   4. Magnet points sit MAGNET_OFFSET px outside the hub's bounding box.
 *   5. All edges in a magnet group are forced to the magnet's cardinal handle.
 */

const NODE_W = 180;
const NODE_H = 160;
const MAGNET_OFFSET = 80;

interface BundleNode {
  id: string;
  position: { x: number; y: number };
  measured?: { width?: number; height?: number };
}

interface BundleEdge {
  id: string;
  source: string;
  target: string;
}

export interface BundleInfo {
  route: { x: number; y: number }[];
  hubHandle: string;
  hubIsSource: boolean;
}

function nodeCenter(n: BundleNode): { x: number; y: number } {
  const w = n.measured?.width ?? NODE_W;
  const h = n.measured?.height ?? NODE_H;
  return { x: n.position.x + w / 2, y: n.position.y + h / 2 };
}

// 0=top, 1=right, 2=bottom, 3=left, -1=no magnet (in diagonal gap)
// Windows are 60° wide centered on each cardinal direction. Edges whose
// angle falls in the 30° gaps between windows route naturally without
// bundling, preventing off-axis edges from being forced through a
// nearby cardinal magnet (which cramps adjacent nodes).
function magnetIndex(angle: number): number {
  const deg = (angle * 180) / Math.PI;
  if (deg >= -120 && deg < -60) return 0;  // top
  if (deg >= -30 && deg < 30) return 1;    // right
  if (deg >= 60 && deg < 120) return 2;    // bottom
  if (deg >= 150 || deg < -150) return 3;  // left
  return -1;
}

const MAGNET_DIR: { dx: number; dy: number }[] = [
  { dx: 0, dy: -1 }, // top
  { dx: 1, dy: 0 }, // right
  { dx: 0, dy: 1 }, // bottom
  { dx: -1, dy: 0 }, // left
];

const MAGNET_HANDLE = ["top", "right", "bottom", "left"];

export function computeEdgeBundles(
  edges: BundleEdge[],
  nodes: BundleNode[],
): Map<string, BundleInfo> {
  const nodeMap = new Map(nodes.map((n) => [n.id, n]));
  const result = new Map<string, BundleInfo>();

  const nodeEdges = new Map<string, BundleEdge[]>();
  for (const e of edges) {
    if (!nodeEdges.has(e.source)) nodeEdges.set(e.source, []);
    if (!nodeEdges.has(e.target)) nodeEdges.set(e.target, []);
    nodeEdges.get(e.source)!.push(e);
    nodeEdges.get(e.target)!.push(e);
  }

  for (const [hubId, hubEdges] of nodeEdges) {
    if (hubEdges.length < 3) continue;

    const hub = nodeMap.get(hubId);
    if (!hub) continue;
    const hc = nodeCenter(hub);
    const hw = (hub.measured?.width ?? NODE_W) / 2;
    const hh = (hub.measured?.height ?? NODE_H) / 2;

    // Assign each edge to a magnet
    const buckets: { edge: BundleEdge; hubIsSource: boolean }[][] = [
      [],
      [],
      [],
      [],
    ];
    for (const e of hubEdges) {
      if (result.has(e.id)) continue;
      const hubIsSource = e.source === hubId;
      const otherId = hubIsSource ? e.target : e.source;
      const other = nodeMap.get(otherId);
      if (!other) continue;
      const oc = nodeCenter(other);
      const angle = Math.atan2(oc.y - hc.y, oc.x - hc.x);
      const mi = magnetIndex(angle);
      if (mi >= 0) buckets[mi].push({ edge: e, hubIsSource });
    }

    for (let mi = 0; mi < 4; mi++) {
      if (buckets[mi].length < 2) continue;
      const dir = MAGNET_DIR[mi];
      const mx = hc.x + dir.dx * (hw + MAGNET_OFFSET);
      const my = hc.y + dir.dy * (hh + MAGNET_OFFSET);
      const handle = MAGNET_HANDLE[mi];
      for (const { edge, hubIsSource } of buckets[mi]) {
        result.set(edge.id, {
          route: [{ x: mx, y: my }],
          hubHandle: handle,
          hubIsSource,
        });
      }
    }
  }

  return result;
}
