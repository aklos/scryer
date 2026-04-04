/**
 * Edge crossing detection and orthogonal A* routing.
 *
 * Detects which edges cross at current node positions, then routes
 * only the crossing edges as orthogonal polylines through empty grid cells.
 * Non-crossing edges stay straight.
 */

export interface Waypoint { x: number; y: number }

interface NodeRect {
  id: string;
  x: number; y: number;
  w: number; h: number;
}

interface EdgeRef {
  id: string;
  source: string;
  target: string;
}

// ── Segment intersection ───────────────────────────────────────────

/** Check if two line segments (p1→p2) and (p3→p4) intersect. */
function segmentsIntersect(
  p1x: number, p1y: number, p2x: number, p2y: number,
  p3x: number, p3y: number, p4x: number, p4y: number,
): boolean {
  const d1x = p2x - p1x, d1y = p2y - p1y;
  const d2x = p4x - p3x, d2y = p4y - p3y;
  const cross = d1x * d2y - d1y * d2x;
  if (Math.abs(cross) < 1e-10) return false;
  const t = ((p3x - p1x) * d2y - (p3y - p1y) * d2x) / cross;
  const u = ((p3x - p1x) * d1y - (p3y - p1y) * d1x) / cross;
  return t > 0.01 && t < 0.99 && u > 0.01 && u < 0.99;
}

/** Cohen-Sutherland: does segment (x1,y1)→(x2,y2) intersect rect? */
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

// ── Crossing detection ─────────────────────────────────────────────

/**
 * Find edges that need rerouting: either crossing another edge,
 * or passing through a non-endpoint node's bounding box.
 */
export function findProblematicEdges(
  nodes: NodeRect[],
  edges: EdgeRef[],
): Set<string> {
  const nodeMap = new Map(nodes.map((n) => [n.id, n]));
  const problemIds = new Set<string>();
  const MARGIN = 10;

  // Compute edge lines
  const edgeLines = edges.map((e) => {
    const s = nodeMap.get(e.source);
    const t = nodeMap.get(e.target);
    if (!s || !t) return null;
    return {
      id: e.id, source: e.source, target: e.target,
      x1: s.x + s.w / 2, y1: s.y + s.h / 2,
      x2: t.x + t.w / 2, y2: t.y + t.h / 2,
    };
  });

  // Check edge-edge crossings
  for (let i = 0; i < edgeLines.length; i++) {
    const a = edgeLines[i];
    if (!a) continue;
    for (let j = i + 1; j < edgeLines.length; j++) {
      const b = edgeLines[j];
      if (!b) continue;
      if (a.source === b.source || a.source === b.target ||
          a.target === b.source || a.target === b.target) continue;
      if (segmentsIntersect(a.x1, a.y1, a.x2, a.y2, b.x1, b.y1, b.x2, b.y2)) {
        problemIds.add(a.id);
        problemIds.add(b.id);
      }
    }
  }

  // Check edge-through-node: does any edge pass through a non-endpoint node?
  for (const eLine of edgeLines) {
    if (!eLine) continue;
    for (const n of nodes) {
      if (n.id === eLine.source || n.id === eLine.target) continue;
      if (segmentIntersectsRect(
        eLine.x1, eLine.y1, eLine.x2, eLine.y2,
        n.x - MARGIN, n.y - MARGIN, n.w + MARGIN * 2, n.h + MARGIN * 2,
      )) {
        problemIds.add(eLine.id);
        break; // one obstruction is enough to flag this edge
      }
    }
  }

  return problemIds;
}


// ── A* orthogonal routing on a grid ────────────────────────────────

const BEND_PENALTY = 3; // extra cost for changing direction

interface GridCell { col: number; row: number }

function cellKey(col: number, row: number): string {
  return `${col},${row}`;
}

/**
 * Route a single edge orthogonally through the grid, avoiding obstacles.
 * Returns waypoints in screen coordinates, or null if no path found.
 */
function astarRoute(
  startX: number, startY: number,
  endX: number, endY: number,
  cellW: number, cellH: number,
  blocked: Set<string>,
): Waypoint[] | null {
  const startCol = Math.round(startX / cellW);
  const startRow = Math.round(startY / cellH);
  const endCol = Math.round(endX / cellW);
  const endRow = Math.round(endY / cellH);

  if (startCol === endCol && startRow === endRow) return null;

  // A* with direction tracking for bend penalty
  interface State {
    col: number;
    row: number;
    dir: number; // 0=none, 1=up, 2=right, 3=down, 4=left
    g: number;
    f: number;
  }

  const stateKey = (col: number, row: number, dir: number) => `${col},${row},${dir}`;

  const heuristic = (col: number, row: number) =>
    Math.abs(col - endCol) + Math.abs(row - endRow);

  // Priority queue (simple sorted array — fine for small grids)
  const open: State[] = [];
  const closed = new Set<string>();
  const cameFrom = new Map<string, string>(); // stateKey → parent stateKey
  const stateAt = new Map<string, State>();

  const startState: State = {
    col: startCol, row: startRow, dir: 0,
    g: 0, f: heuristic(startCol, startRow),
  };
  open.push(startState);
  const sk = stateKey(startCol, startRow, 0);
  stateAt.set(sk, startState);

  const DIRS: [number, number, number][] = [
    [0, -1, 1], // up
    [1, 0, 2],  // right
    [0, 1, 3],  // down
    [-1, 0, 4], // left
  ];

  let iterations = 0;
  const MAX_ITERATIONS = 15000; // finer grid needs more iterations

  while (open.length > 0 && iterations++ < MAX_ITERATIONS) {
    // Pop lowest f
    open.sort((a, b) => a.f - b.f);
    const current = open.shift()!;
    const curKey = stateKey(current.col, current.row, current.dir);

    if (current.col === endCol && current.row === endRow) {
      // Reconstruct path
      const path: GridCell[] = [{ col: current.col, row: current.row }];
      let traceKey = curKey;
      while (cameFrom.has(traceKey)) {
        traceKey = cameFrom.get(traceKey)!;
        const st = stateAt.get(traceKey)!;
        path.unshift({ col: st.col, row: st.row });
      }

      // Convert to waypoints, simplifying straight segments
      const waypoints: Waypoint[] = [{ x: startX, y: startY }];
      for (let i = 1; i < path.length - 1; i++) {
        const prev = path[i - 1];
        const curr = path[i];
        const next = path[i + 1];
        // Only add waypoint at direction changes
        const dx1 = curr.col - prev.col;
        const dy1 = curr.row - prev.row;
        const dx2 = next.col - curr.col;
        const dy2 = next.row - curr.row;
        if (dx1 !== dx2 || dy1 !== dy2) {
          waypoints.push({ x: curr.col * cellW, y: curr.row * cellH });
        }
      }
      waypoints.push({ x: endX, y: endY });

      return waypoints.length >= 2 ? waypoints : null;
    }

    closed.add(curKey);

    for (const [dcol, drow, dir] of DIRS) {
      const nc = current.col + dcol;
      const nr = current.row + drow;
      const nk = stateKey(nc, nr, dir);

      if (closed.has(nk)) continue;
      // Allow start and end cells even if "blocked"
      const ck = cellKey(nc, nr);
      if (blocked.has(ck) && !(nc === endCol && nr === endRow)) continue;

      const bendCost = current.dir !== 0 && current.dir !== dir ? BEND_PENALTY : 0;
      const g = current.g + 1 + bendCost;
      const f = g + heuristic(nc, nr);

      const existing = stateAt.get(nk);
      if (existing && existing.g <= g) continue;

      const state: State = { col: nc, row: nr, dir, g, f };
      stateAt.set(nk, state);
      cameFrom.set(nk, curKey);
      open.push(state);
    }
  }

  return null; // no path found
}

// ── Public API ─────────────────────────────────────────────────────

const NODE_W = 180;
const NODE_H = 160;
// Finer grid (half-cell) gives routes more room to navigate around nodes
const CELL_W = (NODE_W + 60) / 2; // 120
const CELL_H = (NODE_H + 60) / 2; // 110

/**
 * Route crossing edges as orthogonal polylines through empty grid cells.
 * Returns a map from edge ID to waypoint arrays.
 *
 * Called reactively from useVisibleNodes — runs on every position change.
 */
export function routeCrossingEdges(
  nodes: { id: string; position: { x: number; y: number }; measured?: { width?: number; height?: number } }[],
  edges: { id: string; source: string; target: string }[],
): Map<string, Waypoint[]> {
  const routes = new Map<string, Waypoint[]>();
  if (edges.length === 0) return routes;

  // Build node rects
  const nodeRects: NodeRect[] = nodes
    .filter((n) => n.position)
    .map((n) => ({
      id: n.id,
      x: n.position.x,
      y: n.position.y,
      w: n.measured?.width ?? NODE_W,
      h: n.measured?.height ?? NODE_H,
    }));

  if (nodeRects.length < 2) return routes;

  // Find edges that cross other edges or pass through nodes
  const problemIds = findProblematicEdges(nodeRects, edges);
  if (problemIds.size === 0) return routes;

  // Compute edge lines for crossing count and cross-checking
  const nodeMap = new Map(nodeRects.map((n) => [n.id, n]));
  const edgeLines = edges.map((e) => {
    const s = nodeMap.get(e.source);
    const t = nodeMap.get(e.target);
    if (!s || !t) return null;
    return {
      id: e.id, source: e.source, target: e.target,
      x1: s.x + s.w / 2, y1: s.y + s.h / 2,
      x2: t.x + t.w / 2, y2: t.y + t.h / 2,
    };
  });

  // Sort problematic edges by crossing count (worst offenders first get best paths)
  const crossingCount = new Map<string, number>();
  for (const id of problemIds) crossingCount.set(id, 0);
  for (let i = 0; i < edgeLines.length; i++) {
    const a = edgeLines[i];
    if (!a || !problemIds.has(a.id)) continue;
    for (let j = i + 1; j < edgeLines.length; j++) {
      const b = edgeLines[j];
      if (!b) continue;
      if (a.source === b.source || a.source === b.target ||
          a.target === b.source || a.target === b.target) continue;
      if (segmentsIntersect(a.x1, a.y1, a.x2, a.y2, b.x1, b.y1, b.x2, b.y2)) {
        crossingCount.set(a.id, (crossingCount.get(a.id) ?? 0) + 1);
        if (problemIds.has(b.id)) {
          crossingCount.set(b.id, (crossingCount.get(b.id) ?? 0) + 1);
        }
      }
    }
  }
  const toRoute = [...problemIds].sort((a, b) =>
    (crossingCount.get(b) ?? 0) - (crossingCount.get(a) ?? 0),
  );

  // Build obstacle grid with margin around nodes (half-node-width padding)
  const blocked = new Set<string>();
  const MARGIN_X = NODE_W * 0.3;
  const MARGIN_Y = NODE_H * 0.3;
  for (const n of nodeRects) {
    const c1 = Math.floor((n.x - MARGIN_X) / CELL_W);
    const c2 = Math.floor((n.x + n.w + MARGIN_X) / CELL_W);
    const r1 = Math.floor((n.y - MARGIN_Y) / CELL_H);
    const r2 = Math.floor((n.y + n.h + MARGIN_Y) / CELL_H);
    for (let c = c1; c <= c2; c++) {
      for (let r = r1; r <= r2; r++) {
        blocked.add(cellKey(c, r));
      }
    }
  }

  // Route each selected edge (worst offenders first)
  for (const edgeId of toRoute) {
    const edge = edges.find((e) => e.id === edgeId);
    if (!edge) continue;

    const src = nodeMap.get(edge.source);
    const tgt = nodeMap.get(edge.target);
    if (!src || !tgt) continue;

    const srcCx = src.x + src.w / 2;
    const srcCy = src.y + src.h / 2;
    const tgtCx = tgt.x + tgt.w / 2;
    const tgtCy = tgt.y + tgt.h / 2;

    const waypoints = astarRoute(srcCx, srcCy, tgtCx, tgtCy, CELL_W, CELL_H, blocked);
    if (waypoints) {
      routes.set(edgeId, waypoints);
      // Mark routed path cells as obstacles for subsequent edges
      for (const wp of waypoints) {
        blocked.add(cellKey(Math.round(wp.x / CELL_W), Math.round(wp.y / CELL_H)));
      }
    }
  }

  return routes;
}
