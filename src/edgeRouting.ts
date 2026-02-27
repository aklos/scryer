/**
 * Edge handle assignment — picks the shortest-distance handle pair.
 */

interface EdgeInput {
  id: string;
  source: string;
  target: string;
}
interface NodeInput {
  id: string;
  position: { x: number; y: number };
  measured?: { width?: number; height?: number };
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  [key: string]: any;
}

const NODE_W = 180;
const NODE_H = 160;

type HandleId = "top" | "bottom" | "left" | "right" | "top-left" | "top-right" | "bottom-left" | "bottom-right";

const HANDLES: HandleId[] = ["top", "bottom", "left", "right", "top-left", "top-right", "bottom-left", "bottom-right"];

function getHandlePositions(node: NodeInput): Record<HandleId, { x: number; y: number }> {
  const styleW = node.style?.width;
  const styleH = node.style?.height;
  const w = node.measured?.width ?? (typeof styleW === "number" ? styleW : NODE_W);
  const h = node.measured?.height ?? (typeof styleH === "number" ? styleH : NODE_H);
  const { x, y } = node.position;
  return {
    top:          { x: x + w / 2, y },
    bottom:       { x: x + w / 2, y: y + h },
    left:         { x,            y: y + h / 2 },
    right:        { x: x + w,     y: y + h / 2 },
    "top-left":    { x,           y },
    "top-right":   { x: x + w,    y },
    "bottom-left": { x,           y: y + h },
    "bottom-right":{ x: x + w,    y: y + h },
  };
}

// Penalty added per existing edge on a handle (squared-pixel units).
// Small enough to only break ties — never overrides the natural shortest path.
const CONGESTION_PENALTY = 40 ** 2;

// Extra cost for corner handles so they only win when nodes are truly diagonal.
// Without this, corners are geometrically closer for slightly-off-center nodes
// and the router flickers between side and corner handles.
const CORNER_PENALTY = 80 ** 2;

const isCorner = (h: HandleId) => h.includes("-");

export function assignAllHandles(
  nodes: NodeInput[],
  edges: EdgeInput[],
): Map<string, { sourceHandle: string; targetHandle: string }> {
  const nodeMap = new Map(nodes.map((n) => [n.id, n]));
  const result = new Map<string, { sourceHandle: string; targetHandle: string }>();

  // Track how many edges already use each handle on each node
  const usage = new Map<string, number>();
  const getUsage = (nodeId: string, h: HandleId) => usage.get(`${nodeId}:${h}`) ?? 0;
  const addUsage = (nodeId: string, h: HandleId) => {
    const k = `${nodeId}:${h}`;
    usage.set(k, (usage.get(k) ?? 0) + 1);
  };

  for (const e of edges) {
    const src = nodeMap.get(e.source);
    const tgt = nodeMap.get(e.target);
    if (!src || !tgt) continue;

    const srcHandles = getHandlePositions(src);
    const tgtHandles = getHandlePositions(tgt);

    let bestSrc: HandleId = "right";
    let bestTgt: HandleId = "left";
    let bestCost = Infinity;

    for (const sh of HANDLES) {
      const sp = srcHandles[sh];
      const srcPenalty = getUsage(e.source, sh) * CONGESTION_PENALTY;
      const srcCorner = isCorner(sh) ? CORNER_PENALTY : 0;
      for (const th of HANDLES) {
        const tp = tgtHandles[th];
        const tgtPenalty = getUsage(e.target, th) * CONGESTION_PENALTY;
        const tgtCorner = isCorner(th) ? CORNER_PENALTY : 0;
        const cost = (sp.x - tp.x) ** 2 + (sp.y - tp.y) ** 2
          + srcPenalty + tgtPenalty + srcCorner + tgtCorner;
        if (cost < bestCost) {
          bestCost = cost;
          bestSrc = sh;
          bestTgt = th;
        }
      }
    }

    result.set(e.id, { sourceHandle: bestSrc, targetHandle: bestTgt });
    addUsage(e.source, bestSrc);
    addUsage(e.target, bestTgt);
  }
  return result;
}
