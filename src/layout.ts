import {
  forceSimulation,
  forceLink,
  forceManyBody,
  forceX,
  forceY,
  type SimulationNodeDatum,
  type SimulationLinkDatum,
} from "d3-force";
import type { C4Node, C4Edge } from "./types";
import { layoutGraph } from "./layout/planar";

const NODE_W = 180;
const NODE_H = 160;
const GRID_SNAP = 20;

// ── Grid layout (code-level nodes) ──────────────────────────────────

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

// ── Incremental layout (d3-force for adding new nodes) ─────────────

interface SimNode extends SimulationNodeDatum {
  id: string;
  width: number;
  height: number;
  pinned: boolean;
}

interface SimLink extends SimulationLinkDatum<SimNode> {
  source: SimNode | string;
  target: SimNode | string;
}

/** Rectangular collision force for d3-force. */
function forceRectCollide(padding = 40) {
  let nodes: SimNode[] = [];

  function force() {
    for (let i = 0; i < nodes.length; i++) {
      for (let j = i + 1; j < nodes.length; j++) {
        const a = nodes[i], b = nodes[j];
        const dx = b.x! - a.x!;
        const dy = b.y! - a.y!;
        const overlapX = (a.width + b.width) / 2 + padding - Math.abs(dx);
        const overlapY = (a.height + b.height) / 2 + padding - Math.abs(dy);

        if (overlapX > 0 && overlapY > 0) {
          if (overlapX < overlapY) {
            const shift = overlapX / 2;
            const sx = dx > 0 ? shift : -shift;
            if (!a.pinned) a.x! -= sx;
            if (!b.pinned) b.x! += sx;
            if (a.pinned) b.x! += sx;
            if (b.pinned) a.x! -= sx;
          } else {
            const shift = overlapY / 2;
            const sy = dy > 0 ? shift : -shift;
            if (!a.pinned) a.y! -= sy;
            if (!b.pinned) b.y! += sy;
            if (a.pinned) b.y! += sy;
            if (b.pinned) a.y! -= sy;
          }
        }
      }
    }
  }

  force.initialize = (n: SimNode[]) => { nodes = n; };
  return force;
}

/** Incremental layout: d3-force with pinned existing nodes. */
async function incrementalLayout(
  nodes: C4Node[],
  edges: C4Edge[],
): Promise<C4Node[]> {
  if (nodes.length === 0) return nodes;

  const nodeIds = new Set(nodes.map((n) => n.id));
  const filteredEdges = edges.filter((e) => nodeIds.has(e.source) && nodeIds.has(e.target));

  const simNodes: SimNode[] = nodes.map((n) => {
    const w = n.measured?.width ?? NODE_W;
    const h = NODE_H;
    const pinned = !n.data._needsLayout;
    const cx = n.position.x + w / 2;
    const cy = n.position.y + h / 2;
    return {
      id: n.id,
      x: cx,
      y: cy,
      width: w,
      height: h,
      pinned,
      ...(pinned ? { fx: cx, fy: cy } : {}),
    };
  });

  const simNodeMap = new Map(simNodes.map((n) => [n.id, n]));
  const simLinks: SimLink[] = filteredEdges
    .map((e) => ({ source: e.source, target: e.target }))
    .filter((link, i, arr) => {
      for (let j = 0; j < i; j++) {
        if (
          (arr[j].source === link.source && arr[j].target === link.target) ||
          (arr[j].source === link.target && arr[j].target === link.source)
        ) return false;
      }
      return true;
    });

  const freeNodes = simNodes.filter((n) => !n.pinned);
  if (freeNodes.length === 0) return nodes;

  // Place unpositioned free nodes near their connected pinned neighbors
  for (const free of freeNodes) {
    if (free.x !== 0 || free.y !== 0) continue;
    const connectedPinned: SimNode[] = [];
    for (const link of simLinks) {
      const srcId = typeof link.source === "string" ? link.source : link.source.id;
      const tgtId = typeof link.target === "string" ? link.target : link.target.id;
      if (srcId === free.id) {
        const other = simNodeMap.get(tgtId);
        if (other?.pinned) connectedPinned.push(other);
      } else if (tgtId === free.id) {
        const other = simNodeMap.get(srcId);
        if (other?.pinned) connectedPinned.push(other);
      }
    }
    if (connectedPinned.length > 0) {
      let cx = 0, cy = 0;
      for (const n of connectedPinned) { cx += n.x!; cy += n.y!; }
      cx /= connectedPinned.length;
      cy /= connectedPinned.length;
      free.x = cx + (Math.random() - 0.5) * 200;
      free.y = cy + (Math.random() - 0.5) * 200;
    } else {
      let cx = 0, cy = 0, count = 0;
      for (const n of simNodes) {
        if (n === free) continue;
        cx += n.x!; cy += n.y!; count++;
      }
      if (count > 0) {
        free.x = cx / count + (Math.random() - 0.5) * 300;
        free.y = cy / count + (Math.random() - 0.5) * 300;
      }
    }
  }

  const desiredDistance = Math.max(300, 220 + nodes.length * 12);

  const simulation = forceSimulation<SimNode>(simNodes)
    .force("link", forceLink<SimNode, SimLink>(simLinks)
      .id((d) => d.id)
      .distance(desiredDistance)
      .strength(0.3),
    )
    .force("charge", forceManyBody<SimNode>()
      .strength(-1200 - nodes.length * 30)
      .distanceMax(desiredDistance * 3),
    )
    .force("collide", forceRectCollide(60))
    .force("originX", forceX<SimNode>().x((d) => d.x ?? 0).strength(0.02))
    .force("originY", forceY<SimNode>().y((d) => d.y ?? 0).strength(0.02))
    .alphaDecay(0.008)
    .velocityDecay(0.35)
    .stop();

  for (let i = 0; i < 300; i++) simulation.tick();

  return nodes.map((n) => {
    const sim = simNodeMap.get(n.id);
    if (!sim || sim.pinned) return n;
    return {
      ...n,
      position: {
        x: Math.round((sim.x! - sim.width / 2) / GRID_SNAP) * GRID_SNAP,
        y: Math.round((sim.y! - sim.height / 2) / GRID_SNAP) * GRID_SNAP,
      },
    };
  });
}

// ── Main entry point ───────────────────────────────────────────────

/**
 * Auto-layout nodes.
 *
 * - codeLevel: use gridLayout (operations/processes/models)
 * - fullRelayout: FPP-based planar layout via group decomposition
 * - incremental: d3-force with pinned existing nodes
 */
/** Result of autoLayout — positioned nodes + IDs of non-planar edges that need routing. */
export interface AutoLayoutResult {
  nodes: C4Node[];
  nonPlanarEdgeIds: Set<string>;
}

export async function autoLayout(
  nodes: C4Node[],
  edges: C4Edge[],
  codeLevel?: boolean,
  fullRelayout?: boolean,
): Promise<AutoLayoutResult> {
  if (codeLevel) return { nodes: gridLayout(nodes), nonPlanarEdgeIds: new Set() };
  if (nodes.length === 0) return { nodes, nonPlanarEdgeIds: new Set() };

  if (!fullRelayout) {
    return { nodes: await incrementalLayout(nodes, edges), nonPlanarEdgeIds: new Set() };
  }

  // Full relayout: separate isolated nodes, layout connected graph, pack isolates.
  const nodeIds = nodes.map((n) => n.id);
  const idSet = new Set(nodeIds);
  const edgePairs: [string, string][] = edges
    .filter((e) => idSet.has(e.source) && idSet.has(e.target))
    .map((e) => [e.source, e.target]);

  const connectedIds = new Set<string>();
  for (const [u, v] of edgePairs) {
    connectedIds.add(u);
    connectedIds.add(v);
  }
  const graphNodeIds = nodeIds.filter((id) => connectedIds.has(id));
  const isolatedIds = new Set(nodeIds.filter((id) => !connectedIds.has(id)));

  let positions: Map<string, { col: number; row: number }>;
  let nonPlanarEdges: [string, string][];

  if (graphNodeIds.length > 0) {
    const result = await layoutGraph(graphNodeIds, edgePairs);
    positions = result.positions;
    nonPlanarEdges = result.nonPlanarEdges;
  } else {
    positions = new Map();
    nonPlanarEdges = [];
  }

  if (isolatedIds.size > 0) {
    let maxRow = -1;
    let maxCol = 0;
    for (const [, pos] of positions) {
      maxRow = Math.max(maxRow, pos.row);
      maxCol = Math.max(maxCol, pos.col);
    }

    const isolatedArr = nodeIds.filter((id) => isolatedIds.has(id));
    const cols = Math.max(1, Math.min(maxCol + 1 || 3, Math.ceil(Math.sqrt(isolatedArr.length))));
    const startRow = maxRow + 2;

    for (let i = 0; i < isolatedArr.length; i++) {
      positions.set(isolatedArr[i], {
        col: i % cols,
        row: startRow + Math.floor(i / cols) * 2,
      });
    }
  }

  const CELL_W = 300;
  const CELL_H = 180;
  const laid = nodes.map((n) => {
    const p = positions.get(n.id);
    if (!p) return n;
    return { ...n, position: { x: p.col * CELL_W, y: p.row * CELL_H } };
  });

  const nonPlanarEdgeIds = new Set<string>();
  const pairKey = (a: string, b: string) => (a < b ? `${a}\u0000${b}` : `${b}\u0000${a}`);
  const nonPlanarKeys = new Set(nonPlanarEdges.map(([a, b]) => pairKey(a, b)));
  for (const e of edges) {
    if (nonPlanarKeys.has(pairKey(e.source, e.target))) nonPlanarEdgeIds.add(e.id);
  }

  return { nodes: laid, nonPlanarEdgeIds };
}
