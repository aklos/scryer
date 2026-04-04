/**
 * Two-phase group layout decomposition.
 *
 * Phase 1: Layout each group's internal subgraph independently.
 * Phase 2: Build a meta-graph (groups as super-nodes + ungrouped nodes),
 *          layout the meta-graph, compose final positions.
 */

import type { C4Node, C4Edge, Group } from "../types";
import { layoutGraph, dedupeEdges, type EdgePair } from "./planar";

const NODE_W = 180;
const NODE_H = 160;
const CELL_W = NODE_W + 60; // 240
const CELL_H = NODE_H + 40; // 200
const SNAP = 20;

const snap = (v: number) => Math.round(v / SNAP) * SNAP;

export interface LayoutResult {
  positions: Map<string, { x: number; y: number }>;
  nonPlanarEdgeIds: Set<string>;  // original edge IDs that need routing
  /** Pre-computed face expansion routes. Key: "edgeId", value: waypoints in pixels */
  faceRoutes: Map<string, { x: number; y: number }[]>;
}

/**
 * Full layout: group decomposition + Tutte embedding.
 */
export async function fullLayout(
  nodes: C4Node[],
  edges: C4Edge[],
  groups: Group[],
): Promise<LayoutResult> {
  const nodeIds = nodes.map((n) => n.id);
  const nodeSet = new Set(nodeIds);

  const edgePairs: EdgePair[] = dedupeEdges(
    edges
      .filter((e) => nodeSet.has(e.source) && nodeSet.has(e.target))
      .map((e) => [e.source, e.target] as EdgePair),
  );

  // Build node-to-group mapping (only groups with 2+ visible members)
  const nodeToGroup = new Map<string, string>();
  const activeGroups: Group[] = [];
  for (const g of groups) {
    const visibleMembers = g.memberIds.filter((id) => nodeSet.has(id));
    if (visibleMembers.length >= 2) {
      activeGroups.push({ ...g, memberIds: visibleMembers });
      for (const id of visibleMembers) {
        nodeToGroup.set(id, g.id);
      }
    }
  }

  // No active groups — layout directly
  if (activeGroups.length === 0) {
    const result = await layoutGraph(nodeIds, edgePairs);
    const positions = new Map<string, { x: number; y: number }>();
    for (const [id, pos] of result.positions) {
      positions.set(id, { x: snap(pos.col * CELL_W), y: snap(pos.row * CELL_H) });
    }
    // Map non-planar edge pairs to original edge IDs
    const npIds = mapNonPlanarToEdgeIds(result.nonPlanarEdges, edges);
    // Convert face routes from cell units to pixels, keyed by edge ID
    const pixelRoutes = new Map<string, { x: number; y: number }[]>();
    for (const [key, route] of result.faceRoutes) {
      const [u, v] = key.split("\0");
      const edge = edges.find((e) => (e.source === u && e.target === v) || (e.source === v && e.target === u));
      console.log(`[FaceRoute] key=${u}->${v} edge=${edge?.id ?? "NOT FOUND"} route=${route.length} pts`);
      if (edge) {
        pixelRoutes.set(edge.id, route.map((p) => ({ x: snap(p.col * CELL_W) + NODE_W / 2, y: snap(p.row * CELL_H) + NODE_H / 2 })));
      }
    }
    console.log(`[FaceRoute] ${pixelRoutes.size} pixel routes created`);
    return { positions, nonPlanarEdgeIds: npIds, faceRoutes: pixelRoutes };
  }

  // ── Phase 1: Layout each group's internal subgraph ──

  const groupInternalPos = new Map<string, Map<string, { x: number; y: number }>>();
  const groupPixelSize = new Map<string, { w: number; h: number }>();
  const allNonPlanarIds = new Set<string>();

  for (const g of activeGroups) {
    const memberSet = new Set(g.memberIds);
    const internalEdges = edgePairs.filter(([u, v]) => memberSet.has(u) && memberSet.has(v));
    const result = await layoutGraph(g.memberIds, internalEdges);

    const pixelPos = new Map<string, { x: number; y: number }>();
    let maxX = 0, maxY = 0;
    for (const [id, pos] of result.positions) {
      const px = pos.col * CELL_W;
      const py = pos.row * CELL_H;
      pixelPos.set(id, { x: px, y: py });
      maxX = Math.max(maxX, px + NODE_W);
      maxY = Math.max(maxY, py + NODE_H);
    }

    groupInternalPos.set(g.id, pixelPos);
    groupPixelSize.set(g.id, { w: maxX, h: maxY });

    // Map internal non-planar edges to original edge IDs
    for (const id of mapNonPlanarToEdgeIds(result.nonPlanarEdges, edges)) {
      allNonPlanarIds.add(id);
    }
  }

  // ── Phase 2: Build and layout meta-graph ──

  const metaNodeIds: string[] = [];
  for (const g of activeGroups) metaNodeIds.push(`group:${g.id}`);

  const ungroupedIds: string[] = [];
  for (const id of nodeIds) {
    if (!nodeToGroup.has(id)) {
      ungroupedIds.push(id);
      metaNodeIds.push(id);
    }
  }

  // Build meta-edges, tracking which original edges map to each meta-edge
  const metaEdgeSet = new Set<string>();
  const metaEdges: EdgePair[] = [];
  const metaToOriginal = new Map<string, string[]>(); // "uMeta\0vMeta" → original edge IDs

  for (const [u, v] of edgePairs) {
    const uMeta = nodeToGroup.has(u) ? `group:${nodeToGroup.get(u)}` : u;
    const vMeta = nodeToGroup.has(v) ? `group:${nodeToGroup.get(v)}` : v;
    if (uMeta === vMeta) continue;
    const key = uMeta < vMeta ? `${uMeta}\0${vMeta}` : `${vMeta}\0${uMeta}`;
    if (!metaEdgeSet.has(key)) {
      metaEdgeSet.add(key);
      metaEdges.push([uMeta, vMeta]);
      metaToOriginal.set(key, []);
    }
    // Find matching original edge IDs
    for (const e of edges) {
      if ((e.source === u && e.target === v) || (e.source === v && e.target === u)) {
        metaToOriginal.get(key)!.push(e.id);
      }
    }
  }

  const metaResult = await layoutGraph(metaNodeIds, metaEdges);

  // Map non-planar meta-edges back to original edge IDs
  for (const [npU, npV] of metaResult.nonPlanarEdges) {
    const key = npU < npV ? `${npU}\0${npV}` : `${npV}\0${npU}`;
    const origIds = metaToOriginal.get(key);
    if (origIds) {
      for (const id of origIds) allNonPlanarIds.add(id);
    }
  }

  // ── Phase 3: Compose positions with proportional sizing ──

  // Compute per-meta-node sizes for proportional spacing
  const metaNodeSizes = new Map<string, { w: number; h: number }>();
  for (const g of activeGroups) {
    const size = groupPixelSize.get(g.id) ?? { w: NODE_W, h: NODE_H };
    // Add padding for group label and border
    metaNodeSizes.set(`group:${g.id}`, { w: size.w + 60, h: size.h + 60 });
  }
  for (const id of ungroupedIds) {
    metaNodeSizes.set(id, { w: NODE_W + 40, h: NODE_H + 40 });
  }

  // Build cumulative column/row offsets based on actual sizes at each grid position
  // First, find the grid bounds from meta positions
  const metaPositions = new Map<string, { col: number; row: number }>();
  for (const id of metaNodeIds) {
    const pos = metaResult.positions.get(id);
    if (pos) metaPositions.set(id, pos);
  }

  // Compute max width per column and max height per row
  const colWidths = new Map<number, number>();
  const rowHeights = new Map<number, number>();
  for (const [id, pos] of metaPositions) {
    const size = metaNodeSizes.get(id) ?? { w: CELL_W, h: CELL_H };
    const col = Math.round(pos.col);
    const row = Math.round(pos.row);
    colWidths.set(col, Math.max(colWidths.get(col) ?? 0, size.w));
    rowHeights.set(row, Math.max(rowHeights.get(row) ?? 0, size.h));
  }

  // Build cumulative offsets
  const sortedCols = [...colWidths.keys()].sort((a, b) => a - b);
  const sortedRows = [...rowHeights.keys()].sort((a, b) => a - b);
  const colOffset = new Map<number, number>();
  const rowOffset = new Map<number, number>();
  {
    let x = 0;
    for (const col of sortedCols) {
      colOffset.set(col, x);
      x += (colWidths.get(col) ?? CELL_W) + CELL_W * 0.5; // gap between columns
    }
    let y = 0;
    for (const row of sortedRows) {
      rowOffset.set(row, y);
      y += (rowHeights.get(row) ?? CELL_H) + CELL_H * 0.5; // gap between rows
    }
  }

  const getBasePos = (metaPos: { col: number; row: number }) => {
    const col = Math.round(metaPos.col);
    const row = Math.round(metaPos.row);
    return {
      x: colOffset.get(col) ?? metaPos.col * CELL_W,
      y: rowOffset.get(row) ?? metaPos.row * CELL_H,
    };
  };

  const finalPositions = new Map<string, { x: number; y: number }>();

  for (const g of activeGroups) {
    const metaPos = metaResult.positions.get(`group:${g.id}`);
    if (!metaPos) continue;
    const internalPos = groupInternalPos.get(g.id)!;
    const base = getBasePos(metaPos);
    for (const [nodeId, iPos] of internalPos) {
      finalPositions.set(nodeId, {
        x: snap(base.x + iPos.x),
        y: snap(base.y + iPos.y + 40), // extra top padding for group label
      });
    }
  }

  for (const id of ungroupedIds) {
    const metaPos = metaResult.positions.get(id);
    if (!metaPos) continue;
    const base = getBasePos(metaPos);
    finalPositions.set(id, {
      x: snap(base.x),
      y: snap(base.y),
    });
  }

  return { positions: finalPositions, nonPlanarEdgeIds: allNonPlanarIds, faceRoutes: new Map() };
}

/** Map non-planar edge pairs [nodeA, nodeB] to original edge IDs. */
function mapNonPlanarToEdgeIds(nonPlanarEdges: EdgePair[], edges: C4Edge[]): Set<string> {
  const ids = new Set<string>();
  for (const [u, v] of nonPlanarEdges) {
    for (const e of edges) {
      if ((e.source === u && e.target === v) || (e.source === v && e.target === u)) {
        ids.add(e.id);
      }
    }
  }
  return ids;
}
