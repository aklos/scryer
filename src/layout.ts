import {
  forceSimulation,
  forceLink,
  forceManyBody,
  forceX,
  forceY,
  type SimulationNodeDatum,
  type SimulationLinkDatum,
} from "d3-force";
import type { C4Node, C4Edge, Group } from "./types";

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

// ── Force-directed layout ───────────────────────────────────────────

interface SimNode extends SimulationNodeDatum {
  id: string;
  width: number;
  height: number;
  radius: number;
  pinned: boolean;
  groupId?: string;
  originX?: number;
  originY?: number;
}

interface SimLink extends SimulationLinkDatum<SimNode> {
  source: SimNode | string;
  target: SimNode | string;
}

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

/**
 * Post-process: quantize simulation output onto a coarse grid.
 *
 * The simulation finds good relative topology but produces organic positions.
 * This pass snaps each node to the nearest cell in a coarse grid, resolving
 * collisions by bumping nodes to adjacent free cells. Pinned nodes are
 * skipped. The grid cell size adapts to node dimensions.
 *
 * Then, for each edge, if two connected nodes are "almost" aligned on one
 * axis (within one grid step), they're snapped to share that coordinate —
 * producing clean vertical/horizontal edges.
 */
/**
 * Check if placing a grouped node at (col, row) would put it adjacent to
 * a node from a different group. Returns true if there's a conflict.
 */
function findNearbyGroupConflict(
  col: number, row: number, groupId: string,
  occupied: Map<string, SimNode>,
  cellKey: (c: number, r: number) => string,
): boolean {
  // Check the cell itself and immediate neighbors for other-group nodes
  for (let dc = -1; dc <= 1; dc++) {
    for (let dr = -1; dr <= 1; dr++) {
      const neighbor = occupied.get(cellKey(col + dc, row + dr));
      if (neighbor && neighbor.groupId && neighbor.groupId !== groupId) {
        return true;
      }
    }
  }
  return false;
}

/**
 * Post-quantize pass: if two group bounding boxes overlap, shift the
 * smaller group away from the larger one along the axis of least overlap.
 */
function separateGroups(simNodes: SimNode[], cellW: number, cellH: number): void {
  // Collect groups
  const groups = new Map<string, SimNode[]>();
  for (const n of simNodes) {
    if (!n.groupId) continue;
    if (!groups.has(n.groupId)) groups.set(n.groupId, []);
    groups.get(n.groupId)!.push(n);
  }

  const groupIds = [...groups.keys()];
  const PAD = 40; // padding around group boxes

  for (let i = 0; i < groupIds.length; i++) {
    for (let j = i + 1; j < groupIds.length; j++) {
      const membersA = groups.get(groupIds[i])!;
      const membersB = groups.get(groupIds[j])!;

      // Compute bounding boxes
      const boxA = groupBBox(membersA, PAD);
      const boxB = groupBBox(membersB, PAD);

      // Check overlap
      const overlapX = Math.min(boxA.maxX, boxB.maxX) - Math.max(boxA.minX, boxB.minX);
      const overlapY = Math.min(boxA.maxY, boxB.maxY) - Math.max(boxA.minY, boxB.minY);

      if (overlapX <= 0 || overlapY <= 0) continue; // no overlap

      // Shift the smaller group along the axis of least overlap
      const mover = membersA.length <= membersB.length ? membersA : membersB;
      const moverBox = mover === membersA ? boxA : boxB;
      const anchorBox = mover === membersA ? boxB : boxA;

      if (overlapX < overlapY) {
        // Shift horizontally
        const shift = overlapX + cellW * 0.5;
        const dir = (moverBox.minX + moverBox.maxX) > (anchorBox.minX + anchorBox.maxX) ? 1 : -1;
        for (const n of mover) n.x! += shift * dir;
      } else {
        // Shift vertically
        const shift = overlapY + cellH * 0.5;
        const dir = (moverBox.minY + moverBox.maxY) > (anchorBox.minY + anchorBox.maxY) ? 1 : -1;
        for (const n of mover) n.y! += shift * dir;
      }
    }
  }
}

function groupBBox(members: SimNode[], pad: number) {
  let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
  for (const m of members) {
    minX = Math.min(minX, m.x! - m.width / 2);
    minY = Math.min(minY, m.y! - m.height / 2);
    maxX = Math.max(maxX, m.x! + m.width / 2);
    maxY = Math.max(maxY, m.y! + m.height / 2);
  }
  return { minX: minX - pad, minY: minY - pad, maxX: maxX + pad, maxY: maxY + pad };
}

function quantizeToGrid(simNodes: SimNode[], simLinks: SimLink[]): void {
  const freeNodes = simNodes.filter((n) => !n.pinned);
  if (freeNodes.length === 0) return;

  // Grid cell size: square cells so edges between aligned nodes are
  // straight horizontal/vertical or clean 45° diagonals.
  const maxW = Math.max(...simNodes.map((n) => n.width));
  const maxH = Math.max(...simNodes.map((n) => n.height));
  const cellSize = Math.max(maxW + 160, maxH + 120);
  const cellW = cellSize;
  const cellH = cellSize;

  // Snap each free node to nearest grid cell
  // Track occupied cells to avoid collisions
  const occupied = new Map<string, SimNode>();
  const cellKey = (col: number, row: number) => `${col},${row}`;

  // First, mark pinned nodes' grid cells as occupied
  for (const n of simNodes) {
    if (!n.pinned) continue;
    const col = Math.round(n.x! / cellW);
    const row = Math.round(n.y! / cellH);
    occupied.set(cellKey(col, row), n);
  }

  // Sort free nodes by degree (highest first) so hubs get their preferred spot
  const degreeMap = new Map<string, number>();
  for (const n of simNodes) degreeMap.set(n.id, 0);
  for (const link of simLinks) {
    const s = (link.source as SimNode).id;
    const t = (link.target as SimNode).id;
    degreeMap.set(s, (degreeMap.get(s) ?? 0) + 1);
    degreeMap.set(t, (degreeMap.get(t) ?? 0) + 1);
  }
  freeNodes.sort((a, b) => (degreeMap.get(b.id) ?? 0) - (degreeMap.get(a.id) ?? 0));

  for (const n of freeNodes) {
    const idealCol = Math.round(n.x! / cellW);
    const idealRow = Math.round(n.y! / cellH);

    // Spiral search for nearest free cell
    let placed = false;
    for (let radius = 0; radius <= 10 && !placed; radius++) {
      for (let dc = -radius; dc <= radius && !placed; dc++) {
        for (let dr = -radius; dr <= radius && !placed; dr++) {
          if (Math.abs(dc) !== radius && Math.abs(dr) !== radius) continue; // only perimeter
          const col = idealCol + dc;
          const row = idealRow + dr;
          const key = cellKey(col, row);
          if (occupied.has(key)) continue;

          // If this node is in a group, avoid cells occupied by other groups
          if (n.groupId) {
            const occupant = findNearbyGroupConflict(col, row, n.groupId, occupied, cellKey);
            if (occupant) continue;
          }

          n.x = col * cellW;
          n.y = row * cellH;
          occupied.set(key, n);
          placed = true;
        }
      }
    }

    // Fallback: if group-aware placement failed, place without group check
    if (!placed) {
      for (let radius = 0; radius <= 10 && !placed; radius++) {
        for (let dc = -radius; dc <= radius && !placed; dc++) {
          for (let dr = -radius; dr <= radius && !placed; dr++) {
            if (Math.abs(dc) !== radius && Math.abs(dr) !== radius) continue;
            const col = idealCol + dc;
            const row = idealRow + dr;
            const key = cellKey(col, row);
            if (!occupied.has(key)) {
              n.x = col * cellW;
              n.y = row * cellH;
              occupied.set(key, n);
              placed = true;
            }
          }
        }
      }
    }
  }

  // Post-pass: separate overlapping group bounding boxes
  separateGroups(simNodes, cellW, cellH);

  // Axis-align pass: for each edge, if endpoints are within 1 grid step
  // on one axis, snap them to share that coordinate
  for (const link of simLinks) {
    const a = link.source as SimNode;
    const b = link.target as SimNode;
    if (a.pinned && b.pinned) continue;

    const dx = Math.abs(a.x! - b.x!);
    const dy = Math.abs(a.y! - b.y!);

    // Nearly vertically aligned — snap x
    if (dx <= cellW * 0.6 && dy > cellH * 0.5) {
      const midX = a.pinned ? a.x! : b.pinned ? b.x! : (a.x! + b.x!) / 2;
      if (!a.pinned) a.x = midX;
      if (!b.pinned) b.x = midX;
    }
    // Nearly horizontally aligned — snap y
    if (dy <= cellH * 0.6 && dx > cellW * 0.5) {
      const midY = a.pinned ? a.y! : b.pinned ? b.y! : (a.y! + b.y!) / 2;
      if (!a.pinned) a.y = midY;
      if (!b.pinned) b.y = midY;
    }
  }

  // Edge-through-node resolution: if any edge passes through a non-endpoint
  // node's bounding box, shift that node to the nearest free cell that clears it.
  // Rebuild occupied map after axis-align may have shifted positions.
  occupied.clear();
  for (const n of simNodes) {
    const col = Math.round(n.x! / cellW);
    const row = Math.round(n.y! / cellH);
    occupied.set(cellKey(col, row), n);
  }

  const margin = 20;
  for (let pass = 0; pass < 3; pass++) {
    let moved = false;
    for (const n of freeNodes) {
      // Check if any edge passes through this node
      let blocked = false;
      for (const link of simLinks) {
        const src = link.source as SimNode;
        const tgt = link.target as SimNode;
        if (src.id === n.id || tgt.id === n.id) continue;
        if (segmentIntersectsRect(
          src.x!, src.y!, tgt.x!, tgt.y!,
          n.x! - n.width / 2 - margin, n.y! - n.height / 2 - margin,
          n.width + margin * 2, n.height + margin * 2,
        )) {
          blocked = true;
          break;
        }
      }
      if (!blocked) continue;

      // Find nearest free cell that doesn't intersect any edge
      const curCol = Math.round(n.x! / cellW);
      const curRow = Math.round(n.y! / cellH);
      // Remove from current cell
      occupied.delete(cellKey(curCol, curRow));

      let bestCol = curCol, bestRow = curRow, bestDist = Infinity;
      for (let radius = 1; radius <= 6; radius++) {
        for (let dc = -radius; dc <= radius; dc++) {
          for (let dr = -radius; dr <= radius; dr++) {
            if (Math.abs(dc) !== radius && Math.abs(dr) !== radius) continue;
            const col = curCol + dc;
            const row = curRow + dr;
            if (occupied.has(cellKey(col, row))) continue;

            // Check this candidate doesn't sit on any edge
            const cx = col * cellW;
            const cy = row * cellH;
            let clear = true;
            for (const link of simLinks) {
              const src = link.source as SimNode;
              const tgt = link.target as SimNode;
              if (src.id === n.id || tgt.id === n.id) continue;
              if (segmentIntersectsRect(
                src.x!, src.y!, tgt.x!, tgt.y!,
                cx - n.width / 2 - margin, cy - n.height / 2 - margin,
                n.width + margin * 2, n.height + margin * 2,
              )) {
                clear = false;
                break;
              }
            }
            if (!clear) continue;

            const dist = dc * dc + dr * dr;
            if (dist < bestDist) {
              bestDist = dist;
              bestCol = col;
              bestRow = row;
            }
          }
        }
        if (bestDist < Infinity) break; // found one at this radius
      }

      if (bestDist < Infinity) {
        n.x = bestCol * cellW;
        n.y = bestRow * cellH;
        occupied.set(cellKey(bestCol, bestRow), n);
        moved = true;
      } else {
        // Couldn't find a clear cell, put back
        occupied.set(cellKey(curCol, curRow), n);
      }
    }
    if (!moved) break;
  }
}

/**
 * Custom force: resolve edge crossings by moving the most mobile node.
 *
 * For each crossing, pick the lowest-degree non-pinned endpoint and push
 * it away from the crossing point. The push direction is away from the
 * other edge's midpoint — this encourages the node to route around the
 * obstruction rather than just jittering perpendicular to its own edge.
 */
function forceCrossingPenalty(links: SimLink[], strength = 0.5) {
  let nodes: SimNode[] = [];
  let degreeMap: Map<string, number>;

  function force(alpha: number) {
    const s = strength * alpha;

    for (let i = 0; i < links.length; i++) {
      const a = links[i].source as SimNode;
      const b = links[i].target as SimNode;
      if (a.x == null || b.x == null) continue;

      for (let j = i + 1; j < links.length; j++) {
        const c = links[j].source as SimNode;
        const d = links[j].target as SimNode;
        if (c.x == null || d.x == null) continue;

        // Skip edges sharing a node
        if (a.id === c.id || a.id === d.id || b.id === c.id || b.id === d.id) continue;

        if (!segmentsIntersect(a.x!, a.y!, b.x!, b.y!, c.x!, c.y!, d.x!, d.y!)) continue;

        // Found a crossing — find the most mobile endpoint (lowest degree, not pinned)
        const candidates = [a, b, c, d].filter((n) => !n.pinned);
        if (candidates.length === 0) continue;
        candidates.sort((x, y) => (degreeMap.get(x.id) ?? 0) - (degreeMap.get(y.id) ?? 0));

        // Move the most mobile node away from the other edge's midpoint
        const mover = candidates[0];
        // Determine which edge the mover belongs to, and use the other edge's midpoint
        const isOnEdge1 = mover.id === a.id || mover.id === b.id;
        const otherMidX = isOnEdge1 ? (c.x! + d.x!) / 2 : (a.x! + b.x!) / 2;
        const otherMidY = isOnEdge1 ? (c.y! + d.y!) / 2 : (a.y! + b.y!) / 2;

        // Push away from the other edge's midpoint
        const dx = mover.x! - otherMidX;
        const dy = mover.y! - otherMidY;
        const dist = Math.sqrt(dx * dx + dy * dy) || 1;

        const pushStrength = s * 80;
        mover.vx! += (dx / dist) * pushStrength;
        mover.vy! += (dy / dist) * pushStrength;

        // Also give a smaller push to the second-most-mobile node on the other edge
        if (candidates.length >= 2) {
          const second = candidates.find((n) =>
            isOnEdge1 ? (n.id === c.id || n.id === d.id) : (n.id === a.id || n.id === b.id),
          );
          if (second) {
            const dx2 = second.x! - (isOnEdge1 ? (a.x! + b.x!) / 2 : (c.x! + d.x!) / 2);
            const dy2 = second.y! - (isOnEdge1 ? (a.y! + b.y!) / 2 : (c.y! + d.y!) / 2);
            const dist2 = Math.sqrt(dx2 * dx2 + dy2 * dy2) || 1;
            second.vx! += (dx2 / dist2) * pushStrength * 0.4;
            second.vy! += (dy2 / dist2) * pushStrength * 0.4;
          }
        }
      }
    }
  }

  force.initialize = (n: SimNode[]) => {
    nodes = n;
    // Pre-compute degree map
    degreeMap = new Map(nodes.map((n) => [n.id, 0]));
    for (const link of links) {
      const sId = typeof link.source === "string" ? link.source : (link.source as SimNode).id;
      const tId = typeof link.target === "string" ? link.target : (link.target as SimNode).id;
      degreeMap.set(sId, (degreeMap.get(sId) ?? 0) + 1);
      degreeMap.set(tId, (degreeMap.get(tId) ?? 0) + 1);
    }
  };
  return force;
}

/**
 * Custom force: keep grouped nodes clustered using a bounding-box approach.
 *
 * Instead of pulling all members toward centroid (which squishes them),
 * this computes the group's bounding box and only pulls in members that
 * are too far from the group center — beyond a "slack" radius derived
 * from the group's ideal spread. Members within the slack zone are left
 * alone, so collision and charge forces handle their internal spacing.
 */
function forceGroupCohesion(strength = 0.3) {
  let nodes: SimNode[] = [];

  function force(alpha: number) {
    const s = strength * alpha;

    // Collect groups
    const groups = new Map<string, SimNode[]>();
    for (const n of nodes) {
      if (!n.groupId) continue;
      if (!groups.has(n.groupId)) groups.set(n.groupId, []);
      groups.get(n.groupId)!.push(n);
    }

    for (const members of groups.values()) {
      if (members.length < 2) continue;

      // Compute centroid
      let cx = 0, cy = 0;
      for (const m of members) { cx += m.x!; cy += m.y!; }
      cx /= members.length;
      cy /= members.length;

      // Ideal spread: enough room for members side by side with padding.
      // Members within this radius from centroid are left alone.
      const avgSize = members.reduce((s, m) => s + Math.max(m.width, m.height), 0) / members.length;
      const slack = avgSize * Math.sqrt(members.length) * 0.8;

      for (const m of members) {
        if (m.pinned) continue;
        const dx = m.x! - cx;
        const dy = m.y! - cy;
        const dist = Math.sqrt(dx * dx + dy * dy);

        // Only pull if the member is beyond the slack zone
        if (dist > slack) {
          const excess = dist - slack;
          const nx = dx / dist;
          const ny = dy / dist;
          m.vx! -= nx * excess * s;
          m.vy! -= ny * excess * s;
        }
      }
    }
  }

  force.initialize = (n: SimNode[]) => { nodes = n; };
  return force;
}

/**
 * Custom force: push non-group nodes out of group bounding boxes.
 * Computes each group's bounding rect (with padding) and applies a
 * repulsive force to any non-member node inside it.
 */
function forceGroupExclusion(padding = 60, strength = 0.8) {
  let nodes: SimNode[] = [];

  function force(alpha: number) {
    const s = strength * alpha;

    // Collect groups and compute bounding boxes
    const groups = new Map<string, SimNode[]>();
    for (const n of nodes) {
      if (!n.groupId) continue;
      if (!groups.has(n.groupId)) groups.set(n.groupId, []);
      groups.get(n.groupId)!.push(n);
    }

    const boxes: { groupId: string; minX: number; minY: number; maxX: number; maxY: number }[] = [];
    for (const [groupId, members] of groups) {
      if (members.length < 2) continue;
      let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
      for (const m of members) {
        minX = Math.min(minX, m.x! - m.width / 2);
        minY = Math.min(minY, m.y! - m.height / 2);
        maxX = Math.max(maxX, m.x! + m.width / 2);
        maxY = Math.max(maxY, m.y! + m.height / 2);
      }
      boxes.push({
        groupId,
        minX: minX - padding,
        minY: minY - padding,
        maxX: maxX + padding,
        maxY: maxY + padding,
      });
    }

    // Push non-members out of group boxes
    for (const n of nodes) {
      if (n.pinned) continue;
      for (const box of boxes) {
        if (n.groupId === box.groupId) continue; // member of this group — skip

        // Check if node center is inside the box
        if (n.x! < box.minX || n.x! > box.maxX || n.y! < box.minY || n.y! > box.maxY) continue;

        // Push toward nearest edge of the box
        const distLeft = n.x! - box.minX;
        const distRight = box.maxX - n.x!;
        const distTop = n.y! - box.minY;
        const distBottom = box.maxY - n.y!;
        const minDist = Math.min(distLeft, distRight, distTop, distBottom);

        const push = (minDist + 50) * s;
        if (minDist === distLeft) n.vx! -= push;
        else if (minDist === distRight) n.vx! += push;
        else if (minDist === distTop) n.vy! -= push;
        else n.vy! += push;
      }
    }
  }

  force.initialize = (n: SimNode[]) => { nodes = n; };
  return force;
}

/**
 * Custom force: prevent edges from passing through nodes.
 * For each edge, check if it passes through any non-endpoint node's
 * bounding box, and push that node out of the way.
 */
function forceEdgeNodeRepulsion(links: SimLink[], strength = 0.2) {
  let nodes: SimNode[] = [];

  function force(alpha: number) {
    const s = strength * alpha;

    for (const link of links) {
      const src = link.source as SimNode;
      const tgt = link.target as SimNode;
      if (!src.x || !tgt.x) continue;

      for (const n of nodes) {
        if (n.id === src.id || n.id === tgt.id || n.pinned) continue;

        // Check if edge passes through node's bounding box (with margin)
        const margin = 30;
        const rx = n.x! - n.width / 2 - margin;
        const ry = n.y! - n.height / 2 - margin;
        const rw = n.width + margin * 2;
        const rh = n.height + margin * 2;

        if (!segmentIntersectsRect(src.x!, src.y!, tgt.x!, tgt.y!, rx, ry, rw, rh)) continue;

        // Push node perpendicular to the edge
        const edgeDx = tgt.x! - src.x!;
        const edgeDy = tgt.y! - src.y!;
        const len = Math.sqrt(edgeDx * edgeDx + edgeDy * edgeDy) || 1;
        const px = -edgeDy / len;
        const py = edgeDx / len;

        // Pick direction that moves node away from edge midpoint
        const mx = (src.x! + tgt.x!) / 2;
        const my = (src.y! + tgt.y!) / 2;
        const dot = (n.x! - mx) * px + (n.y! - my) * py;
        const sign = dot >= 0 ? 1 : -1;

        n.vx! += px * s * sign * 60;
        n.vy! += py * s * sign * 60;
      }
    }
  }

  force.initialize = (n: SimNode[]) => { nodes = n; };
  return force;
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

/**
 * Rectangular collision force. d3's forceCollide uses circles — this
 * version uses actual node width/height with padding.
 */
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
          // Resolve along the axis with smaller overlap
          if (overlapX < overlapY) {
            const shift = overlapX / 2;
            const sx = dx > 0 ? shift : -shift;
            if (!a.pinned) a.x! -= sx;
            if (!b.pinned) b.x! += sx;
            // If one is pinned, the other takes the full shift
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

/**
 * Run force-directed layout on architecture-level nodes.
 *
 * Supports incremental layout: nodes with existing positions are pinned
 * unless `fullRelayout` is true. Only `_needsLayout` nodes (or all nodes
 * during full relayout) participate freely in the simulation.
 *
 * When `groups` are provided, a cohesion force keeps group members clustered.
 */
export async function autoLayout(
  nodes: C4Node[],
  edges: C4Edge[],
  groups?: Group[],
  codeLevel?: boolean,
  fullRelayout?: boolean,
): Promise<C4Node[]> {
  if (codeLevel) return gridLayout(nodes);
  if (nodes.length === 0) return nodes;

  const nodeIds = new Set(nodes.map((n) => n.id));
  const filteredEdges = edges.filter(
    (e) => nodeIds.has(e.source) && nodeIds.has(e.target),
  );

  // Build group membership map
  const nodeToGroup = new Map<string, string>();
  if (groups) {
    for (const g of groups) {
      const visibleMembers = g.memberIds.filter((id) => nodeIds.has(id));
      if (visibleMembers.length >= 2) {
        for (const id of visibleMembers) {
          nodeToGroup.set(id, g.id);
        }
      }
    }
  }

  // Create simulation nodes
  // d3-force uses center-based coordinates; we convert from/to top-left
  // Use base NODE_H for component nodes so member lists don't inflate the layout grid
  const simNodes: SimNode[] = nodes.map((n) => {
    const w = n.measured?.width ?? NODE_W;
    const h = (n.data as { kind?: string }).kind === "component" ? NODE_H : (n.measured?.height ?? NODE_H);
    const pinned = !fullRelayout && !n.data._needsLayout;

    const cx = n.position.x + w / 2;
    const cy = n.position.y + h / 2;
    return {
      id: n.id,
      x: cx,
      y: cy,
      width: w,
      height: h,
      radius: Math.sqrt(w * w + h * h) / 2,
      pinned,
      groupId: nodeToGroup.get(n.id),
      originX: cx,
      originY: cy,
      // Pin nodes by fixing their position
      ...(pinned ? { fx: cx, fy: cy } : {}),
    };
  });

  const simNodeMap = new Map(simNodes.map((n) => [n.id, n]));

  // Create simulation links
  const simLinks: SimLink[] = filteredEdges
    .map((e) => ({
      source: e.source,
      target: e.target,
    }))
    // Deduplicate (same pair in both directions)
    .filter((link, i, arr) => {
      for (let j = 0; j < i; j++) {
        if (
          (arr[j].source === link.source && arr[j].target === link.target) ||
          (arr[j].source === link.target && arr[j].target === link.source)
        ) return false;
      }
      return true;
    });

  // If all nodes are pinned, nothing to do
  const freeNodes = simNodes.filter((n) => !n.pinned);
  if (freeNodes.length === 0) return nodes;

  // Place unpositioned free nodes near their connected pinned neighbors
  // so they don't start at 0,0 and fly in from nowhere
  for (const free of freeNodes) {
    if (free.x !== 0 || free.y !== 0) continue; // already has a position hint

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
      // Place near centroid of connected pinned nodes with some jitter
      let cx = 0, cy = 0;
      for (const n of connectedPinned) { cx += n.x!; cy += n.y!; }
      cx /= connectedPinned.length;
      cy /= connectedPinned.length;
      free.x = cx + (Math.random() - 0.5) * 200;
      free.y = cy + (Math.random() - 0.5) * 200;
    } else {
      // No pinned neighbors — place near centroid of all nodes
      let cx = 0, cy = 0, count = 0;
      for (const n of simNodes) {
        if (n === free) continue;
        cx += n.x!; cy += n.y!; count++;
      }
      if (count > 0) {
        free.x = cx / count + (Math.random() - 0.5) * 300;
        free.y = cy / count + (Math.random() - 0.5) * 300;
      } else {
        free.x = (Math.random() - 0.5) * 400;
        free.y = (Math.random() - 0.5) * 400;
      }
    }
  }

  // Desired edge length scales with node count and edge density
  const edgeDensity = filteredEdges.length / Math.max(1, nodes.length);
  const desiredDistance = Math.max(300, 220 + nodes.length * 12 + edgeDensity * 20);

  // Build simulation
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
    .force("crossings", forceCrossingPenalty(simLinks, 0.5))
    .force("edgeNode", forceEdgeNodeRepulsion(simLinks, 0.2))
    .force("originX", forceX<SimNode>().x((d) => d.originX ?? 0).strength(fullRelayout ? 0.06 : 0.02))
    .force("originY", forceY<SimNode>().y((d) => d.originY ?? 0).strength(fullRelayout ? 0.06 : 0.02))
    .alphaDecay(0.008)
    .velocityDecay(0.35)
    .stop();

  // Add group forces if there are groups
  if (nodeToGroup.size > 0) {
    simulation.force("groupCohesion", forceGroupCohesion(0.3));
    simulation.force("groupExclusion", forceGroupExclusion(60, 0.8));
  }

  // Run simulation — more iterations for complex graphs with crossings to resolve
  const iterations = 500;
  for (let i = 0; i < iterations; i++) {
    simulation.tick();
  }

  // Post-process: snap to coarse grid while preserving topology
  quantizeToGrid(simNodes, simLinks);

  // Post-pass: resolve overlaps caused by actual node heights (member lists)
  // that were ignored during layout. Push nodes down when a taller node above
  // would overlap them.
  const actualHeights = new Map<string, number>();
  for (const n of nodes) {
    actualHeights.set(n.id, n.measured?.height ?? NODE_H);
  }
  const PAD = 40;
  for (let pass = 0; pass < 5; pass++) {
    let shifted = false;
    for (const a of simNodes) {
      const ah = actualHeights.get(a.id) ?? NODE_H;
      for (const b of simNodes) {
        if (a.id === b.id || b.pinned) continue;
        // Only check nodes that are below and horizontally nearby
        if (b.y! <= a.y!) continue;
        const bh = actualHeights.get(b.id) ?? NODE_H;
        const overlapX = (a.width + b.width) / 2 + PAD - Math.abs(b.x! - a.x!);
        if (overlapX <= 0) continue;
        const overlapY = (ah + bh) / 2 + PAD - (b.y! - a.y!);
        if (overlapY <= 0) continue;
        b.y! += overlapY;
        shifted = true;
      }
    }
    if (!shifted) break;
  }

  // Convert center-based coordinates back to top-left and snap to fine grid
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
