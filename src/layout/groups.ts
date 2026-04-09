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
    const npIds = mapNonPlanarToEdgeIds(result.nonPlanarEdges, edges);
    return { positions, nonPlanarEdgeIds: npIds };
  }

  // ── Phase 1: Layout each group's internal subgraph ──

  const groupInternalPos = new Map<string, Map<string, { x: number; y: number }>>();
  const groupPixelSize = new Map<string, { w: number; h: number }>();
  const allNonPlanarIds = new Set<string>();

  const nodeById = new Map(nodes.map((n) => [n.id, n]));

  for (const g of activeGroups) {
    const memberSet = new Set(g.memberIds);
    const internalEdges = edgePairs.filter(([u, v]) => memberSet.has(u) && memberSet.has(v));

    const pixelPos = new Map<string, { x: number; y: number }>();
    let maxX = 0, maxY = 0;

    if (internalEdges.length === 0) {
      // No internal edges — pack members side by side with minimal gap
      let x = 0;
      for (const id of g.memberIds) {
        const w = nodeById.get(id)?.measured?.width ?? NODE_W;
        pixelPos.set(id, { x, y: 0 });
        x += w + 40;
        maxX = Math.max(maxX, x - 40);
        maxY = Math.max(maxY, NODE_H);
      }
    } else {
      // Connected members — use normal layout
      const result = await layoutGraph(g.memberIds, internalEdges);

      for (const [id, pos] of result.positions) {
        const px = pos.col * CELL_W;
        const py = pos.row * CELL_H;
        pixelPos.set(id, { x: px, y: py });
        maxX = Math.max(maxX, px + NODE_W);
        maxY = Math.max(maxY, py + NODE_H);
      }

      for (const id of mapNonPlanarToEdgeIds(result.nonPlanarEdges, edges)) {
        allNonPlanarIds.add(id);
      }
    }

    groupInternalPos.set(g.id, pixelPos);
    groupPixelSize.set(g.id, { w: maxX, h: maxY });
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

  // ── Phase 3: Compose positions with overlap resolution ──
  //
  // Convert continuous Tutte/KK positions to pixels, then iteratively push
  // apart any overlapping meta-node pairs. This avoids the cumulative column
  // approach which over-allocates space when a wide group shares a column
  // with smaller nodes.

  const metaNodeSizes = new Map<string, { w: number; h: number }>();
  for (const g of activeGroups) {
    const size = groupPixelSize.get(g.id) ?? { w: NODE_W, h: NODE_H };
    metaNodeSizes.set(`group:${g.id}`, { w: size.w + 60, h: size.h + 60 });
  }
  for (const id of ungroupedIds) {
    metaNodeSizes.set(id, { w: NODE_W + 40, h: NODE_H + 40 });
  }

  // Start from continuous positions scaled to pixel space
  const metaPx = new Map<string, number>();
  const metaPy = new Map<string, number>();
  for (const id of metaNodeIds) {
    const pos = metaResult.positions.get(id);
    if (pos) {
      metaPx.set(id, pos.col * CELL_W);
      metaPy.set(id, pos.row * CELL_H);
    }
  }

  // Iteratively resolve rectangular overlaps
  const META_GAP = 60;
  const metaIds = [...metaPx.keys()];
  for (let iter = 0; iter < 30; iter++) {
    let moved = false;
    for (let i = 0; i < metaIds.length; i++) {
      for (let j = i + 1; j < metaIds.length; j++) {
        const idA = metaIds[i], idB = metaIds[j];
        const sA = metaNodeSizes.get(idA) ?? { w: CELL_W, h: CELL_H };
        const sB = metaNodeSizes.get(idB) ?? { w: CELL_W, h: CELL_H };
        const dx = metaPx.get(idB)! - metaPx.get(idA)!;
        const dy = metaPy.get(idB)! - metaPy.get(idA)!;
        const overlapX = (sA.w + sB.w) / 2 + META_GAP - Math.abs(dx);
        const overlapY = (sA.h + sB.h) / 2 + META_GAP - Math.abs(dy);
        if (overlapX > 0 && overlapY > 0) {
          // Push apart on the axis with less overlap (minimal disruption)
          if (overlapX < overlapY) {
            const push = overlapX / 2;
            const sx = dx >= 0 ? 1 : -1;
            metaPx.set(idA, metaPx.get(idA)! - sx * push);
            metaPx.set(idB, metaPx.get(idB)! + sx * push);
          } else {
            const push = overlapY / 2;
            const sy = dy >= 0 ? 1 : -1;
            metaPy.set(idA, metaPy.get(idA)! - sy * push);
            metaPy.set(idB, metaPy.get(idB)! + sy * push);
          }
          moved = true;
        }
      }
    }
    if (!moved) break;
  }

  const finalPositions = new Map<string, { x: number; y: number }>();

  for (const g of activeGroups) {
    const metaId = `group:${g.id}`;
    if (!metaPx.has(metaId)) continue;
    const bx = metaPx.get(metaId)!;
    const by = metaPy.get(metaId)!;
    const internalPos = groupInternalPos.get(g.id)!;
    for (const [nodeId, iPos] of internalPos) {
      finalPositions.set(nodeId, {
        x: snap(bx + iPos.x),
        y: snap(by + iPos.y + 40),
      });
    }
  }

  for (const id of ungroupedIds) {
    if (!metaPx.has(id)) continue;
    finalPositions.set(id, {
      x: snap(metaPx.get(id)!),
      y: snap(metaPy.get(id)!),
    });
  }

  return { positions: finalPositions, nonPlanarEdgeIds: allNonPlanarIds };
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
