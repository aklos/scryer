/**
 * Layout core — a single flat grid with explicit group regions.
 *
 * Every entry occupies a whole-cell footprint at a stored `cell`. Groups are
 * explicit rectangles (stored `cell` + `size`) on the same grid. Placement is
 * strict: a footprint may only land where every cell it covers is free of other
 * entries and respects group boundaries. Nothing is ever pushed or reflowed.
 *
 * The surface grid auto-grows: its rendered size is the content bounding box
 * plus a `MARGIN` ring of empty cells.
 *
 * `hydrateCells` seeds a model that has no entry cells.
 */

import type { Surface, Group, Entry, Cell } from "./viewmodel";

export const CELL_W = 160;
export const CELL_H = 160;
export const ITEM_INSET = 14;
export const MARGIN = 4;

export const MAX_CARD_W = 4;
export const MAX_CARD_H = 8;

export const GROUP_SNAP = 2;

export const CELL_FIT_SLACK = 8;
export const CARD_HEADER_H = 30;
export const CARD_META_H = 20;
export const RESP_LINE_H = 24;
export const RESP_PAD = 16;
export const LINK_ROW_H = 28;
export const LINK_PAD = 14;

export const GROUP_LABEL_H = 28;
export const GROUP_PAD = 0;

export interface Span {
  w: number;
  h: number;
}

export interface Rect {
  row: number;
  col: number;
  w: number;
  h: number;
}

// --- footprints --------------------------------------------------------------

export function cardHeightPx(entry: Entry): number {
  const metaLines = entry.description ? 1 : 0;
  const n = entry.responsibilities.length;
  const cols = n >= 3 ? 2 : 1;
  const respRows = Math.max(1, Math.ceil(n / cols));
  const outgoing = entry.links?.length ?? 0;
  // Always reserve 2 link rows (outgoing + incoming) when outgoing exist,
  // and 1 row for any non-external entry since it may have incoming links.
  const linkRows = outgoing > 0 ? 2 : entry.external ? 0 : 1;
  return (
    CARD_HEADER_H +
    metaLines * CARD_META_H +
    (n > 0 ? RESP_PAD + respRows * RESP_LINE_H : 0) +
    (linkRows > 0 ? linkRows * LINK_ROW_H + LINK_PAD : 0)
  );
}

export function cardSpan(entry: Entry): Span {
  const w = Math.min(MAX_CARD_W, (entry.responsibilities.length >= 3 ? 2 : 1) * 2);
  const h = Math.ceil((cardHeightPx(entry) + ITEM_INSET * 2 - CELL_FIT_SLACK) / CELL_H);
  return { w: Math.max(2, w), h: Math.min(MAX_CARD_H, Math.max(1, h)) };
}

export function entryRect(entry: Entry): Rect {
  const sp = cardSpan(entry);
  const cell = entry.cell ?? { row: 0, col: 0 };
  return { row: cell.row, col: cell.col, w: sp.w, h: sp.h };
}

export function rectsOverlap(a: Rect, b: Rect): boolean {
  return (
    a.col < b.col + b.w &&
    b.col < a.col + a.w &&
    a.row < b.row + b.h &&
    b.row < a.row + a.h
  );
}

function rectContains(outer: Rect, inner: Rect): boolean {
  return (
    inner.col >= outer.col &&
    inner.row >= outer.row &&
    inner.col + inner.w <= outer.col + outer.w &&
    inner.row + inner.h <= outer.row + outer.h
  );
}

// --- grid entries & group helpers --------------------------------------------

/** Grid entries — everything except persons (which live on the perimeter).
 * Externals are in-grid, styled as out-of-scope by the card renderer. */
export function gridEntries(surface: Surface): Entry[] {
  return surface.entries.filter((e) => e.kind !== "person");
}

/** The stored region rect for a group. */
export function groupRect(group: Group): Rect {
  return { row: group.cell.row, col: group.cell.col, w: group.size.cols, h: group.size.rows };
}

/** Nesting depth of a group (0 = root-level). */
export function groupDepth(surface: Surface, group: Group): number {
  let depth = 0;
  let current = group;
  while (current.parentGroupId) {
    depth++;
    const parent = surface.groups.find((g) => g.id === current.parentGroupId);
    if (!parent) break;
    current = parent;
  }
  return depth;
}

/** Find the deepest group whose region contains `cell`. */
export function groupAtCell(surface: Surface, cell: Cell): Group | null {
  let best: Group | null = null;
  let bestDepth = -1;
  for (const g of surface.groups) {
    const r = groupRect(g);
    if (
      cell.col >= r.col &&
      cell.col < r.col + r.w &&
      cell.row >= r.row &&
      cell.row < r.row + r.h
    ) {
      const d = groupDepth(surface, g);
      if (d > bestDepth) {
        bestDepth = d;
        best = g;
      }
    }
  }
  return best;
}

// --- grid sizing -------------------------------------------------------------

export function gridExtent(surface: Surface): { cols: number; rows: number } {
  let cols = 0,
    rows = 0;
  for (const e of gridEntries(surface)) {
    if (!e.cell) continue;
    const sp = cardSpan(e);
    cols = Math.max(cols, e.cell.col + sp.w);
    rows = Math.max(rows, e.cell.row + sp.h);
  }
  for (const g of surface.groups) {
    cols = Math.max(cols, g.cell.col + g.size.cols);
    rows = Math.max(rows, g.cell.row + g.size.rows);
  }
  return { cols: Math.max(1, cols), rows: Math.max(1, rows) };
}

export function surfaceGrid(
  surface: Surface,
  extra?: Rect,
): { cols: number; rows: number } {
  let { cols, rows } = gridExtent(surface);
  if (extra) {
    cols = Math.max(cols, extra.col + extra.w);
    rows = Math.max(rows, extra.row + extra.h);
  }
  return { cols: cols + MARGIN, rows: rows + MARGIN };
}

// --- placement ---------------------------------------------------------------

/**
 * Can an entry be placed at `rect`? Checks no overlap with any other
 * grid entry (except `ignoreId`). Negative coords are allowed — the
 * caller rebases after placement.
 */
export function canPlace(
  surface: Surface,
  rect: Rect,
  ignoreId: string,
): boolean {
  for (const e of gridEntries(surface)) {
    if (e.id === ignoreId) continue;
    if (rectsOverlap(rect, entryRect(e))) return false;
  }
  return true;
}

/**
 * Can an entry's footprint be dropped at `rect`? Combines entry overlap check
 * with group-aware constraints:
 * - If `targetGroupId` is set, the entire footprint must be within that group.
 * - If `targetGroupId` is undefined, the footprint must not overlap any group.
 */
export function canDrop(
  surface: Surface,
  rect: Rect,
  entryId: string,
  targetGroupId: string | undefined,
): boolean {
  if (!canPlace(surface, rect, entryId)) return false;

  if (targetGroupId) {
    const group = surface.groups.find((g) => g.id === targetGroupId);
    if (!group) return false;
    return rectContains(groupRect(group), rect);
  }

  for (const g of surface.groups) {
    if (rectsOverlap(rect, groupRect(g))) return false;
  }
  return true;
}

/**
 * Shift all entries and groups so the top-left of the content is exactly at
 * (0, 0) — both directions, always. Without this the grid container keeps
 * empty rows/cols on the top and left when content drifts away from the
 * origin, and the perimeter (which anchors to the container, not the ring)
 * desynchronises from it.
 */
export function rebase(surface: Surface): Surface {
  let minRow = Infinity,
    minCol = Infinity;
  for (const e of gridEntries(surface)) {
    if (!e.cell) continue;
    minRow = Math.min(minRow, e.cell.row);
    minCol = Math.min(minCol, e.cell.col);
  }
  for (const g of surface.groups) {
    minRow = Math.min(minRow, g.cell.row);
    minCol = Math.min(minCol, g.cell.col);
  }
  if (!Number.isFinite(minRow) || !Number.isFinite(minCol)) return surface;
  if (minRow === 0 && minCol === 0) return surface;
  const dRow = -minRow;
  const dCol = -minCol;
  return {
    ...surface,
    entries: surface.entries.map((e) =>
      e.cell
        ? { ...e, cell: { row: e.cell.row + dRow, col: e.cell.col + dCol } }
        : e,
    ),
    groups: surface.groups.map((g) => ({
      ...g,
      cell: { row: g.cell.row + dRow, col: g.cell.col + dCol },
    })),
  };
}

/** Move an entry to a new cell, optionally changing its group. Returns null if invalid. */
export function placeEntry(
  surface: Surface,
  entryId: string,
  cell: Cell,
  newGroupId?: string | null,
): Surface | null {
  const entry = gridEntries(surface).find((e) => e.id === entryId);
  if (!entry) return null;

  const sp = cardSpan(entry);
  const rect: Rect = { row: cell.row, col: cell.col, w: sp.w, h: sp.h };

  const gid = newGroupId !== undefined ? (newGroupId ?? undefined) : entry.groupId;
  if (!canDrop(surface, rect, entryId, gid)) return null;

  return rebase({
    ...surface,
    entries: surface.entries.map((e) =>
      e.id === entryId ? { ...e, cell, groupId: gid } : e,
    ),
  });
}

// --- group move --------------------------------------------------------------

/**
 * Move a group and all its member entries by a cell delta. Returns null if any
 * placement is invalid.
 */
/** All descendant group IDs (children, grandchildren, etc). */
function descendantGroupIds(surface: Surface, groupId: string): Set<string> {
  const ids = new Set<string>();
  const stack = [groupId];
  while (stack.length) {
    const id = stack.pop()!;
    for (const g of surface.groups) {
      if (g.parentGroupId === id && !ids.has(g.id)) {
        ids.add(g.id);
        stack.push(g.id);
      }
    }
  }
  return ids;
}

export function moveGroup(
  surface: Surface,
  groupId: string,
  dRow: number,
  dCol: number,
): Surface | null {
  const group = surface.groups.find((g) => g.id === groupId);
  if (!group) return null;

  const newGroupCell: Cell = {
    row: group.cell.row + dRow,
    col: group.cell.col + dCol,
  };
  const newGroupRect: Rect = {
    row: newGroupCell.row,
    col: newGroupCell.col,
    w: group.size.cols,
    h: group.size.rows,
  };

  const descendants = descendantGroupIds(surface, groupId);

  // Check group-vs-group: allow full containment (nesting), reject partial overlap
  for (const g of surface.groups) {
    if (g.id === groupId || descendants.has(g.id)) continue;
    const other = groupRect(g);
    if (!rectsOverlap(newGroupRect, other)) continue;
    if (rectContains(other, newGroupRect)) continue;
    if (rectContains(newGroupRect, other)) continue;
    return null;
  }

  // Determine new parent: smallest non-descendant group that fully contains us
  let newParent: Group | null = null;
  let bestArea = Infinity;
  for (const g of surface.groups) {
    if (g.id === groupId || descendants.has(g.id)) continue;
    const r = groupRect(g);
    if (rectContains(r, newGroupRect)) {
      const area = r.w * r.h;
      if (area < bestArea) {
        bestArea = area;
        newParent = g;
      }
    }
  }

  // Collect all entries that move with this group (own members + descendant members)
  const movingGroupIds = new Set([groupId, ...descendants]);
  const movingEntryIds = new Set(
    gridEntries(surface)
      .filter((e) => e.groupId && movingGroupIds.has(e.groupId))
      .map((e) => e.id),
  );

  // Check moved entries don't overlap non-moving entries
  for (const e of gridEntries(surface)) {
    if (!movingEntryIds.has(e.id)) continue;
    const c = e.cell ?? { row: 0, col: 0 };
    const sp = cardSpan(e);
    const newRect: Rect = {
      row: c.row + dRow,
      col: c.col + dCol,
      w: sp.w,
      h: sp.h,
    };
    for (const other of gridEntries(surface)) {
      if (movingEntryIds.has(other.id)) continue;
      if (rectsOverlap(newRect, entryRect(other))) return null;
    }
  }

  return rebase({
    ...surface,
    groups: surface.groups.map((g) => {
      if (g.id === groupId) {
        return {
          ...g,
          cell: newGroupCell,
          parentGroupId: newParent?.id,
        };
      }
      if (descendants.has(g.id)) {
        return {
          ...g,
          cell: { row: g.cell.row + dRow, col: g.cell.col + dCol },
        };
      }
      return g;
    }),
    entries: surface.entries.map((e) => {
      if (!movingEntryIds.has(e.id)) return e;
      const c = e.cell ?? { row: 0, col: 0 };
      return { ...e, cell: { row: c.row + dRow, col: c.col + dCol } };
    }),
  });
}

// --- group resize ------------------------------------------------------------

/** Bounding box of all member entries within a group. */
function memberExtent(surface: Surface, groupId: string): Rect | null {
  const members = gridEntries(surface).filter((e) => e.groupId === groupId);
  if (members.length === 0) return null;
  let minR = Infinity,
    minC = Infinity,
    maxR = 0,
    maxC = 0;
  for (const m of members) {
    const r = entryRect(m);
    minR = Math.min(minR, r.row);
    minC = Math.min(minC, r.col);
    maxR = Math.max(maxR, r.row + r.h);
    maxC = Math.max(maxC, r.col + r.w);
  }
  return { row: minR, col: minC, w: maxC - minC, h: maxR - minR };
}

/**
 * Resize a group to `size`. Rejects if the new size doesn't contain all
 * members or overlaps other groups / ungrouped entries.
 */
export function resizeGroup(
  surface: Surface,
  groupId: string,
  size: { cols: number; rows: number },
): Surface | null {
  const snapped = {
    cols: Math.max(GROUP_SNAP, Math.round(size.cols / GROUP_SNAP) * GROUP_SNAP),
    rows: Math.max(GROUP_SNAP, Math.round(size.rows / GROUP_SNAP) * GROUP_SNAP),
  };
  const group = surface.groups.find((g) => g.id === groupId);
  if (!group) return null;

  const newRect: Rect = {
    row: group.cell.row,
    col: group.cell.col,
    w: snapped.cols,
    h: snapped.rows,
  };

  // Must still contain all members and child groups
  const ext = memberExtent(surface, groupId);
  if (ext && !rectContains(newRect, ext)) return null;
  const descendants = descendantGroupIds(surface, groupId);
  for (const gid of descendants) {
    const child = surface.groups.find((g) => g.id === gid);
    if (child && !rectContains(newRect, groupRect(child))) return null;
  }

  // No partial overlap with non-descendant groups (full containment OK)
  for (const g of surface.groups) {
    if (g.id === groupId || descendants.has(g.id)) continue;
    const other = groupRect(g);
    if (!rectsOverlap(newRect, other)) continue;
    if (rectContains(other, newRect)) continue;
    if (rectContains(newRect, other)) continue;
    return null;
  }

  // Must not overlap entries outside this group's hierarchy
  const movingGroupIds = new Set([groupId, ...descendants]);
  for (const e of gridEntries(surface)) {
    if (e.groupId && movingGroupIds.has(e.groupId)) continue;
    if (rectsOverlap(newRect, entryRect(e))) return null;
  }

  return rebase({
    ...surface,
    groups: surface.groups.map((g) =>
      g.id === groupId ? { ...g, size: snapped } : g,
    ),
  });
}

// --- hydration ---------------------------------------------------------------

function freeSlot(
  occupied: Rect[],
  footprint: Span,
  target: Cell,
  maxCol: number,
): Cell {
  const limit = Math.max(footprint.w, maxCol);
  const maxRow =
    Math.max(0, target.row, ...occupied.map((r) => r.row + r.h)) + footprint.h;
  let best: Cell = { row: maxRow, col: 0 };
  let bestD = Infinity;
  for (let row = 0; row <= maxRow; row++) {
    for (let col = 0; col + footprint.w <= limit; col++) {
      const rect = { row, col, w: footprint.w, h: footprint.h };
      if (occupied.some((o) => rectsOverlap(rect, o))) continue;
      const d = (row - target.row) ** 2 + (col - target.col) ** 2;
      if (d < bestD) {
        bestD = d;
        best = { row, col };
      }
    }
  }
  return best;
}

interface PackBlock {
  id: string;
  span: Span;
}

function packBlock(items: PackBlock[]): { cells: Map<string, Cell>; extent: Span } {
  const colBound = Math.max(
    2,
    Math.ceil(Math.sqrt(items.reduce((s, i) => s + i.span.w, 0))),
  );
  const occupied: Rect[] = [];
  const cells = new Map<string, Cell>();
  for (const item of items) {
    const at = freeSlot(occupied, item.span, { row: 0, col: 0 }, colBound);
    cells.set(item.id, at);
    occupied.push({ row: at.row, col: at.col, w: item.span.w, h: item.span.h });
  }
  let w = 0,
    h = 0;
  for (const r of occupied) {
    w = Math.max(w, r.col + r.w);
    h = Math.max(h, r.row + r.h);
  }
  return { cells, extent: { w: Math.max(1, w), h: Math.max(1, h) } };
}

/**
 * Seed cells for entries that lack them. Groups' members are packed into
 * blocks, then the blocks + ungrouped entries are packed onto the surface.
 * Groups get their cell and size set from the layout.
 */
export function hydrateCells(surface: Surface): Surface {
  const grid = gridEntries(surface);
  if (grid.length > 0 && grid.every((e) => e.cell)) return surface;

  const byGroup = new Map<string, Entry[]>();
  const ungrouped: Entry[] = [];
  for (const e of grid) {
    if (e.groupId) {
      const list = byGroup.get(e.groupId) ?? [];
      list.push(e);
      byGroup.set(e.groupId, list);
    } else {
      ungrouped.push(e);
    }
  }

  // Pack each group's members into a local block
  const groupLayouts = new Map<
    string,
    { cells: Map<string, Cell>; extent: Span }
  >();
  for (const [gid, members] of byGroup) {
    const items = members.map((e) => ({ id: e.id, span: cardSpan(e) }));
    groupLayouts.set(gid, packBlock(items));
  }

  const topItems: PackBlock[] = [
    ...[...groupLayouts.entries()].map(([gid, layout]) => ({
      id: gid,
      span: { w: layout.extent.w, h: layout.extent.h },
    })),
    ...ungrouped.map((e) => ({ id: e.id, span: cardSpan(e) })),
  ];
  const { cells: topCells } = packBlock(topItems);

  // Compute final entry cells and group positions
  const finalCells = new Map<string, Cell>();
  const groupPositions = new Map<string, { cell: Cell; size: { cols: number; rows: number } }>();

  for (const [gid, layout] of groupLayouts) {
    const groupPos = topCells.get(gid)!;
    const size = {
      cols: Math.ceil(layout.extent.w / GROUP_SNAP) * GROUP_SNAP,
      rows: Math.ceil(layout.extent.h / GROUP_SNAP) * GROUP_SNAP,
    };
    groupPositions.set(gid, { cell: groupPos, size });
    for (const [eid, local] of layout.cells) {
      finalCells.set(eid, {
        row: groupPos.row + local.row,
        col: groupPos.col + local.col,
      });
    }
  }
  for (const e of ungrouped) {
    const pos = topCells.get(e.id);
    if (pos) finalCells.set(e.id, pos);
  }

  return {
    ...surface,
    entries: surface.entries.map((e) =>
      finalCells.has(e.id) ? { ...e, cell: finalCells.get(e.id)! } : e,
    ),
    groups: surface.groups.map((g) => {
      const pos = groupPositions.get(g.id);
      return pos ? { ...g, cell: pos.cell, size: pos.size } : g;
    }),
  };
}
