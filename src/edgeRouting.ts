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
// Strong enough to push a second edge to an adjacent handle, so multiple
// edges arriving at the same node spread across different connection points.
const CONGESTION_PENALTY = 160 ** 2;

// Corners are only allowed when the angle between node centers falls within a
// narrow window around one of the four diagonal directions (±15° of 45/135/
// 225/315°). Outside those windows, only side handles are considered.
const CORNER_HALF_WINDOW_DEG = 15;

// Returns the source/target corner handle pair for the diagonal window the
// angle falls into, or null if it falls outside all four windows.
function diagonalCornerPair(dx: number, dy: number): [HandleId, HandleId] | null {
  const angle = (Math.atan2(dy, dx) * 180) / Math.PI; // -180..180, 0 = +x
  const within = (target: number) => {
    let d = Math.abs(angle - target);
    if (d > 180) d = 360 - d;
    return d <= CORNER_HALF_WINDOW_DEG;
  };
  if (within(45))   return ["bottom-right", "top-left"];     // target down-right of source
  if (within(135))  return ["bottom-left",  "top-right"];    // target down-left of source
  if (within(-45))  return ["top-right",    "bottom-left"];  // target up-right of source
  if (within(-135)) return ["top-left",     "bottom-right"]; // target up-left of source
  return null;
}

// Penalty for handles that fight the dominant direction between nodes.
// When one node is clearly above another, edges should flow top→bottom,
// not exit sideways. This makes vertical relationships use top/bottom handles
// and horizontal relationships use left/right handles.
const AXIS_PENALTY = 60 ** 2;

// Returns how misaligned a handle is with the direction to the other node.
// 0 = well-aligned, 1 = perpendicular to the dominant axis.
function axisMisalignment(handle: HandleId, dx: number, dy: number): number {
  const ax = Math.abs(dx);
  const ay = Math.abs(dy);
  // If nodes are close on both axes, no preference
  if (ax < 40 && ay < 40) return 0;
  const ratio = Math.max(ax, ay) === 0 ? 0 : Math.abs(ax - ay) / Math.max(ax, ay);
  // ratio is 0 when perfectly diagonal, 1 when purely vertical/horizontal
  if (ratio < 0.3) return 0; // roughly diagonal — no preference

  const vertical = ay > ax;
  if (vertical) {
    // Dominant axis is vertical — penalize left/right handles
    if (handle === "left" || handle === "right") return ratio;
  } else {
    // Dominant axis is horizontal — penalize top/bottom handles
    if (handle === "top" || handle === "bottom") return ratio;
  }
  return 0;
}

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

  // Detect bidirectional pairs: if A→B and B→A both exist, the reverse edge
  // must use the swapped handles of its counterpart so RelationshipEdge's
  // perpendicular offset renders correctly.
  const edgeKey = (src: string, tgt: string) => `${src}::${tgt}`;
  const edgeSet = new Set(edges.map((e) => edgeKey(e.source, e.target)));
  const biPairProcessed = new Map<string, { sourceHandle: HandleId; targetHandle: HandleId }>();

  // Sort edges by distance so closer edges claim optimal handles first.
  // Farther edges then spread to adjacent handles via congestion penalty.
  const sorted = [...edges].map((e) => {
    const s = nodeMap.get(e.source);
    const t = nodeMap.get(e.target);
    if (!s || !t) return { e, dist: Infinity };
    const sw = s.measured?.width ?? (typeof s.style?.width === "number" ? s.style.width : NODE_W);
    const sh = s.measured?.height ?? (typeof s.style?.height === "number" ? s.style.height : NODE_H);
    const tw = t.measured?.width ?? (typeof t.style?.width === "number" ? t.style.width : NODE_W);
    const th = t.measured?.height ?? (typeof t.style?.height === "number" ? t.style.height : NODE_H);
    const dx = (t.position.x + tw / 2) - (s.position.x + sw / 2);
    const dy = (t.position.y + th / 2) - (s.position.y + sh / 2);
    return { e, dist: dx * dx + dy * dy };
  }).sort((a, b) => a.dist - b.dist);

  for (const { e } of sorted) {
    const src = nodeMap.get(e.source);
    const tgt = nodeMap.get(e.target);
    if (!src || !tgt) continue;

    // If this is the reverse edge of an already-processed bidirectional pair,
    // force it to use the swapped handles so both edges share the same endpoints.
    const reverseKey = edgeKey(e.target, e.source);
    const isBiDirectional = edgeSet.has(reverseKey);
    const reverseHandles = biPairProcessed.get(reverseKey);
    if (isBiDirectional && reverseHandles) {
      const swappedSrc = reverseHandles.targetHandle;
      const swappedTgt = reverseHandles.sourceHandle;
      result.set(e.id, { sourceHandle: swappedSrc, targetHandle: swappedTgt });
      addUsage(e.source, swappedSrc);
      addUsage(e.target, swappedTgt);
      continue;
    }

    const srcHandles = getHandlePositions(src);
    const tgtHandles = getHandlePositions(tgt);

    let bestSrc: HandleId = "right";
    let bestTgt: HandleId = "left";
    let bestCost = Infinity;

    // Direction between node centers (for axis alignment penalty)
    const srcW = src.measured?.width ?? (typeof src.style?.width === "number" ? src.style.width : NODE_W);
    const srcH = src.measured?.height ?? (typeof src.style?.height === "number" ? src.style.height : NODE_H);
    const tgtW = tgt.measured?.width ?? (typeof tgt.style?.width === "number" ? tgt.style.width : NODE_W);
    const tgtH = tgt.measured?.height ?? (typeof tgt.style?.height === "number" ? tgt.style.height : NODE_H);
    const dx = (tgt.position.x + tgtW / 2) - (src.position.x + srcW / 2);
    const dy = (tgt.position.y + tgtH / 2) - (src.position.y + srcH / 2);

    // Decide which handles are eligible based on the angle between centers.
    // If the angle falls in one of the four diagonal windows, the matching
    // corner pair is included; otherwise only side handles are considered.
    const cornerPair = diagonalCornerPair(dx, dy);
    const sideHandles: HandleId[] = ["top", "bottom", "left", "right"];
    const srcCandidates: HandleId[] = cornerPair ? [...sideHandles, cornerPair[0]] : sideHandles;
    const tgtCandidates: HandleId[] = cornerPair ? [...sideHandles, cornerPair[1]] : sideHandles;

    for (const sh of srcCandidates) {
      const sp = srcHandles[sh];
      const srcPenalty = getUsage(e.source, sh) * CONGESTION_PENALTY;
      const srcAxis = axisMisalignment(sh, dx, dy) * AXIS_PENALTY;
      for (const th of tgtCandidates) {
        const tp = tgtHandles[th];
        const tgtPenalty = getUsage(e.target, th) * CONGESTION_PENALTY;
        const tgtAxis = axisMisalignment(th, dx, dy) * AXIS_PENALTY;
        const cost = (sp.x - tp.x) ** 2 + (sp.y - tp.y) ** 2
          + srcPenalty + tgtPenalty + srcAxis + tgtAxis;
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

    // Record for bidirectional pair detection
    if (isBiDirectional) {
      biPairProcessed.set(edgeKey(e.source, e.target), { sourceHandle: bestSrc, targetHandle: bestTgt });
    }
  }
  return result;
}
