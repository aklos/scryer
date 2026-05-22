/**
 * Layout helpers — a single flat grid with explicit group regions.
 *
 * Pure functions only. Operates on `SurfaceView` (the per-render derived
 * shape from `viewmodel.ts`) — model mutation lives in `viewmodel.ts`.
 *
 * Every node occupies a whole-cell footprint at its stored `cell`. Groups are
 * explicit rectangles (`cell` + `size`) on the same grid. Placement is strict:
 * a footprint may only land where every cell it covers is free of other nodes
 * and respects group boundaries. Nothing is ever pushed or reflowed.
 *
 * The surface grid auto-grows: rendered size = content bbox + `MARGIN` ring.
 *
 * `hydrateCells` seeds a model that has no node cells.
 */

import {
  type Cell,
  type Group,
  type GroupSize,
  type GroupView,
  type Link,
  type Node,
  type NodeView,
  type ScryModel,
  type SurfaceView,
} from "./viewmodel";

export const CELL_W = 160;
export const CELL_H = 80;
export const ITEM_INSET = 14;
export const MARGIN = 4;

export const MAX_CARD_W = 4;
export const MAX_CARD_H = 16;

export const GROUP_SNAP_W = 1;
export const GROUP_SNAP_H = 1;

export const CARD_HEADER_H = 30;
export const CARD_META_H = 16;
export const DESC_PAD = 12;
export const RESP_LINE_H = 24;
export const IMPL_LINE_H = 18;
export const RESP_PAD = 16;
export const LINK_ROW_H = 24;
export const LINK_PAD = 10;
const CHAR_W = 6;

export const GROUP_LABEL_H = 28;
export const GROUP_PAD = 0;

const GROUP_HDR_BAR = 30;
const GROUP_HDR_LINE = 22;

export function groupHeaderRows(
  group: Pick<Group, "responsibilities">,
): number {
  const resps = group.responsibilities?.length ?? 0;
  const contentPx = GROUP_HDR_BAR + (resps + 1) * GROUP_HDR_LINE;
  return Math.max(1, Math.ceil(contentPx / CELL_H));
}

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

function nodeRespCount(node: Pick<Node, "responsibilities">): number {
  return node.responsibilities?.length ?? 0;
}

function nodeOutgoingCount(node: NodeView): number {
  return node._outgoingLinks.length;
}

export function cardHeightPx(node: NodeView, cardW = 2): number {
  const innerW = cardW * CELL_W - ITEM_INSET * 2 - 24;
  const descCpl = Math.max(1, Math.floor(innerW / (CHAR_W * 11 / 12)));
  const descLines = node.description
    ? Math.max(1, Math.ceil(node.description.length / descCpl))
    : 0;
  const n = nodeRespCount(node);
  const respCpl = Math.max(1, Math.floor((innerW - 14) / CHAR_W));
  let bodyLines = 0;
  if (node.kind === "model") {
    for (const p of node.properties ?? []) {
      const text = p.description ? `${p.label} ${p.description}` : p.label;
      bodyLines += Math.max(1, Math.ceil(text.length / respCpl));
    }
  } else {
    for (const r of node.responsibilities ?? []) {
      bodyLines += Math.max(1, Math.ceil((r.statement?.length || 1) / respCpl));
      for (const rule of r.implementationRules ?? []) {
        bodyLines += Math.max(1, Math.ceil(rule.length / respCpl)) * (IMPL_LINE_H / RESP_LINE_H);
      }
    }
    if (n > 0 && bodyLines === 0) bodyLines = 1;
  }
  const outgoing = nodeOutgoingCount(node);
  const incoming = node._incomingLinks.length;
  const linksPerRow = Math.max(1, Math.floor(innerW / 70));
  const outRows = outgoing > 0 ? Math.ceil(outgoing / linksPerRow) : 0;
  const inRows =
    node.kind === "model" || incoming === 0
      ? 0
      : Math.ceil(incoming / linksPerRow);
  const linkRows = outRows + inRows;
  const bodyH = bodyLines > 0 ? RESP_PAD + bodyLines * RESP_LINE_H : 0;
  return (
    CARD_HEADER_H +
    (descLines > 0 ? DESC_PAD + descLines * CARD_META_H : 0) +
    bodyH +
    (linkRows > 0 ? linkRows * LINK_ROW_H + LINK_PAD : 0)
  );
}

export function cardWidthCells(node: NodeView): number {
  if (node.kind === "operation" || node.kind === "model") return 2;
  return Math.min(MAX_CARD_W, Math.max(2, (nodeRespCount(node) >= 3 ? 2 : 1) * 2));
}

/** Heuristic span — used for initial placement (hydrateCells), collision
 *  detection, and drag previews where no DOM is available. */
export function cardSpan(node: NodeView): Span {
  const w = cardWidthCells(node);
  const h = Math.ceil(
    (cardHeightPx(node, w) + ITEM_INSET * 2) / CELL_H,
  );
  return { w, h: Math.min(MAX_CARD_H, Math.max(2, h)) };
}

/** Convert a measured content height (logical px, unzoomed) to a cell span. */
export function spanFromMeasuredHeight(contentPx: number, w: number): Span {
  const h = Math.ceil((contentPx + ITEM_INSET * 2) / CELL_H);
  return { w, h: Math.min(MAX_CARD_H, Math.max(2, h)) };
}

export function nodeRect(
  node: NodeView,
  measured?: ReadonlyMap<string, Span>,
): Rect {
  const sp = measured?.get(node.id) ?? cardSpan(node);
  const cell = node.cell ?? { row: 0, col: 0 };
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

export function rectContains(outer: Rect, inner: Rect): boolean {
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
export function gridEntries(view: SurfaceView): NodeView[] {
  return view.entries.filter((n) => n.kind !== "person");
}

/** The stored region rect for a group. */
export function groupRect(group: GroupView): Rect {
  return {
    row: group.cell.row,
    col: group.cell.col,
    w: group.size.cols,
    h: group.size.rows,
  };
}

/** Nesting depth of a group within the visible view (0 = root). */
export function groupDepth(view: SurfaceView, group: GroupView): number {
  let depth = 0;
  let current: GroupView | undefined = group;
  while (current?.parentGroupId) {
    depth++;
    const parent = view.groups.find((g) => g.id === current!.parentGroupId);
    if (!parent) break;
    current = parent;
  }
  return depth;
}

/** Find the deepest group whose region contains `cell`. */
export function groupAtCell(view: SurfaceView, cell: Cell): GroupView | null {
  let best: GroupView | null = null;
  let bestDepth = -1;
  for (const g of view.groups) {
    const r = groupRect(g);
    if (
      cell.col >= r.col &&
      cell.col < r.col + r.w &&
      cell.row >= r.row &&
      cell.row < r.row + r.h
    ) {
      const d = groupDepth(view, g);
      if (d > bestDepth) {
        bestDepth = d;
        best = g;
      }
    }
  }
  return best;
}

// --- grid sizing -------------------------------------------------------------

export function gridExtent(
  view: SurfaceView,
  measured?: ReadonlyMap<string, Span>,
): { cols: number; rows: number } {
  let cols = 0,
    rows = 0;
  for (const n of gridEntries(view)) {
    if (!n.cell) continue;
    const sp = measured?.get(n.id) ?? cardSpan(n);
    cols = Math.max(cols, n.cell.col + sp.w);
    rows = Math.max(rows, n.cell.row + sp.h);
  }
  for (const g of view.groups) {
    cols = Math.max(cols, g.cell.col + g.size.cols);
    rows = Math.max(rows, g.cell.row + g.size.rows);
  }
  return { cols: Math.max(1, cols), rows: Math.max(1, rows) };
}

export function surfaceGrid(
  view: SurfaceView,
  extra?: Rect,
): { cols: number; rows: number } {
  let { cols, rows } = gridExtent(view);
  if (extra) {
    cols = Math.max(cols, extra.col + extra.w);
    rows = Math.max(rows, extra.row + extra.h);
  }
  return { cols: cols + MARGIN, rows: rows + MARGIN };
}

// --- validation --------------------------------------------------------------

/**
 * Can a node be placed at `rect`? Checks no overlap with any other grid node
 * (except `ignoreId`). Negative coords are allowed — the caller rebases.
 */
export function canPlace(
  view: SurfaceView,
  rect: Rect,
  ignoreId: string,
  measured?: ReadonlyMap<string, Span>,
): boolean {
  for (const n of gridEntries(view)) {
    if (n.id === ignoreId) continue;
    if (rectsOverlap(rect, nodeRect(n, measured))) return false;
  }
  return true;
}

/**
 * Combines node-overlap with group-aware constraints.
 *  - If `targetGroupId` is set, the entire footprint must be within that group.
 *  - If `targetGroupId` is undefined, the footprint must not overlap any group.
 */
export function canDrop(
  view: SurfaceView,
  rect: Rect,
  nodeId: string,
  targetGroupId: string | undefined,
  measured?: ReadonlyMap<string, Span>,
): boolean {
  if (!canPlace(view, rect, nodeId, measured)) return false;
  if (targetGroupId) {
    const group = view.groups.find((g) => g.id === targetGroupId);
    if (!group) return false;
    const gr = groupRect(group);
    const hdrRows = groupHeaderRows(group);
    const contentArea: Rect = {
      row: gr.row + hdrRows,
      col: gr.col,
      w: gr.w,
      h: gr.h - hdrRows,
    };
    return contentArea.h > 0 && rectContains(contentArea, rect);
  }
  for (const g of view.groups) {
    if (rectsOverlap(rect, groupRect(g))) return false;
  }
  return true;
}

/** All descendant group ids of `groupId` (children, grandchildren, etc). */
export function descendantGroupIds(
  view: SurfaceView,
  groupId: string,
): Set<string> {
  const ids = new Set<string>();
  const stack = [groupId];
  while (stack.length) {
    const id = stack.pop()!;
    for (const g of view.groups) {
      if (g.parentGroupId === id && !ids.has(g.id)) {
        ids.add(g.id);
        stack.push(g.id);
      }
    }
  }
  return ids;
}

/**
 * Validate moving a group by (dRow, dCol). Returns the new parent group id
 * (or null for top-level) when the move is legal; returns `undefined` when
 * it's not.
 */
export function canMoveGroup(
  view: SurfaceView,
  groupId: string,
  dRow: number,
  dCol: number,
  measured?: ReadonlyMap<string, Span>,
): { newParentId: string | null } | undefined {
  const group = view.groups.find((g) => g.id === groupId);
  if (!group) return undefined;

  const newGroupRect: Rect = {
    row: group.cell.row + dRow,
    col: group.cell.col + dCol,
    w: group.size.cols,
    h: group.size.rows,
  };
  const descendants = descendantGroupIds(view, groupId);

  for (const g of view.groups) {
    if (g.id === groupId || descendants.has(g.id)) continue;
    const other = groupRect(g);
    if (!rectsOverlap(newGroupRect, other)) continue;
    if (rectContains(other, newGroupRect)) continue;
    if (rectContains(newGroupRect, other)) continue;
    return undefined;
  }

  // Determine new parent: smallest non-descendant group fully containing us
  let newParentId: string | null = null;
  let bestArea = Infinity;
  for (const g of view.groups) {
    if (g.id === groupId || descendants.has(g.id)) continue;
    const r = groupRect(g);
    if (rectContains(r, newGroupRect)) {
      const area = r.w * r.h;
      if (area < bestArea) {
        bestArea = area;
        newParentId = g.id;
      }
    }
  }

  // Check moved member nodes don't overlap non-moving nodes
  const movingGroupIds = new Set([groupId, ...descendants]);
  const movingNodeIds = new Set(
    gridEntries(view)
      .filter((n) => n._groupId && movingGroupIds.has(n._groupId))
      .map((n) => n.id),
  );
  for (const n of gridEntries(view)) {
    if (!movingNodeIds.has(n.id)) continue;
    const c = n.cell ?? { row: 0, col: 0 };
    const sp = measured?.get(n.id) ?? cardSpan(n);
    const newRect: Rect = {
      row: c.row + dRow,
      col: c.col + dCol,
      w: sp.w,
      h: sp.h,
    };
    for (const other of gridEntries(view)) {
      if (movingNodeIds.has(other.id)) continue;
      if (rectsOverlap(newRect, nodeRect(other, measured))) return undefined;
    }
  }

  return { newParentId };
}

/** Bounding box of member nodes within a group (visible at this depth). */
function memberExtent(
  view: SurfaceView,
  groupId: string,
  measured?: ReadonlyMap<string, Span>,
): Rect | null {
  const members = gridEntries(view).filter((n) => n._groupId === groupId);
  if (members.length === 0) return null;
  let minR = Infinity,
    minC = Infinity,
    maxR = 0,
    maxC = 0;
  for (const m of members) {
    const r = nodeRect(m, measured);
    minR = Math.min(minR, r.row);
    minC = Math.min(minC, r.col);
    maxR = Math.max(maxR, r.row + r.h);
    maxC = Math.max(maxC, r.col + r.w);
  }
  return { row: minR, col: minC, w: maxC - minC, h: maxR - minR };
}

/** Snap a proposed group size and validate against members + neighbours. */
export function clampGroupSize(
  view: SurfaceView,
  groupId: string,
  size: { cols: number; rows: number },
  measured?: ReadonlyMap<string, Span>,
): { cols: number; rows: number } | undefined {
  const snapped = {
    cols: Math.max(GROUP_SNAP_W, Math.round(size.cols / GROUP_SNAP_W) * GROUP_SNAP_W),
    rows: Math.max(GROUP_SNAP_H, Math.round(size.rows / GROUP_SNAP_H) * GROUP_SNAP_H),
  };
  const group = view.groups.find((g) => g.id === groupId);
  if (!group) return undefined;
  const newRect: Rect = {
    row: group.cell.row,
    col: group.cell.col,
    w: snapped.cols,
    h: snapped.rows,
  };
  const ext = memberExtent(view, groupId, measured);
  if (ext && !rectContains(newRect, ext)) return undefined;

  const descendants = descendantGroupIds(view, groupId);
  for (const gid of descendants) {
    const child = view.groups.find((g) => g.id === gid);
    if (child && !rectContains(newRect, groupRect(child))) return undefined;
  }
  for (const g of view.groups) {
    if (g.id === groupId || descendants.has(g.id)) continue;
    const other = groupRect(g);
    if (!rectsOverlap(newRect, other)) continue;
    if (rectContains(other, newRect)) continue;
    if (rectContains(newRect, other)) continue;
    return undefined;
  }
  const movingGroupIds = new Set([groupId, ...descendants]);
  for (const n of gridEntries(view)) {
    if (n._groupId && movingGroupIds.has(n._groupId)) continue;
    if (rectsOverlap(newRect, nodeRect(n, measured))) return undefined;
  }
  return snapped;
}

// --- hydration ---------------------------------------------------------------

/**
 * Row-major first-fit. Scan rows top-down, columns left-to-right; return the
 * first cell where `footprint` doesn't overlap any rect in `occupied`. The
 * width cap (`maxCol`) bounds the column scan; rows are unbounded (we fall
 * past the content extent if nothing fits within it).
 */
function firstFit(occupied: Rect[], footprint: Span, maxCol: number): Cell {
  const limit = Math.max(footprint.w, maxCol);
  const contentRows = Math.max(0, ...occupied.map((r) => r.row + r.h));
  const rowBound = contentRows + footprint.h + 1;
  for (let row = 0; row <= rowBound; row++) {
    for (let col = 0; col + footprint.w <= limit; col++) {
      const rect = { row, col, w: footprint.w, h: footprint.h };
      if (!occupied.some((o) => rectsOverlap(rect, o))) {
        return { row, col };
      }
    }
  }
  // Shouldn't be reachable because rowBound includes one extra empty row,
  // but degrade gracefully.
  return { row: rowBound, col: 0 };
}

interface PackItem {
  id: string;
  span: Span;
  area: number;
}

const COL_CAP = 8;

function colCapFor(items: PackItem[]): number {
  const totalW = items.reduce((s, i) => s + i.span.w, 0);
  return Math.min(COL_CAP, Math.max(2, totalW));
}

/**
 * Seed cells (+ group cell/size) for any node or group that lacks them.
 * Existing positions are immutable — they're treated as occupied space that
 * new placements must avoid. Idempotent: a model whose nodes/groups all carry
 * cells is returned unchanged (by reference).
 *
 * Algorithm:
 *  for each parent surface:
 *    occupied = rects of every already-positioned node + every already-sized group
 *    new groups:
 *      pack their unpositioned members internally with first-fit (largest first)
 *      bbox(snapped to GROUP_SNAP) becomes the group's size
 *      first-fit a slot for the group rect on the parent surface
 *      members' final cells = group anchor + local cell
 *    new ungrouped nodes:
 *      first-fit one at a time (largest first), pushing each into occupied
 */
export function hydrateCells(model: ScryModel): ScryModel {
  const nodeCells = new Map<string, Cell>();
  const groupGeom = new Map<string, { cell: Cell; size: GroupSize }>();

  // Map node id → group id (inverse of Group.memberIds)
  const nodeGroup = new Map<string, string>();
  for (const g of model.groups) {
    for (const m of g.memberIds) nodeGroup.set(m, g.id);
  }

  // Precompute outgoing/incoming link maps so hydration spans match render.
  const outLinks = new Map<string, Link[]>();
  const incLinks = new Map<string, Link[]>();
  for (const l of model.links) {
    (outLinks.get(l.src) ?? outLinks.set(l.src, []).get(l.src)!).push(l);
    (incLinks.get(l.dst) ?? incLinks.set(l.dst, []).get(l.dst)!).push(l);
  }

  const parents = new Set<string | null>();
  for (const n of model.nodes) parents.add(n.parentId ?? null);

  for (const parentId of parents) {
    const siblings = model.nodes.filter(
      (n) =>
        (n.parentId ?? null) === parentId &&
        (n.kind !== "person" || parentId === null),
    );
    if (siblings.length === 0) continue;

    // ---- seed `occupied` with existing positions ----
    const occupied: Rect[] = [];
    const siblingSpan = new Map<string, Span>();
    for (const n of siblings) {
      const span = cardSpan(
        makeNodeViewForHydration(
          n,
          outLinks.get(n.id) ?? [],
          incLinks.get(n.id) ?? [],
        ),
      );
      siblingSpan.set(n.id, span);
      if (n.cell) {
        occupied.push({
          row: n.cell.row,
          col: n.cell.col,
          w: span.w,
          h: span.h,
        });
      }
    }
    // Groups whose members live at this depth.
    const groupsHere = model.groups.filter((g) =>
      g.memberIds.some((m) => siblings.some((s) => s.id === m)),
    );
    for (const g of groupsHere) {
      if (g.cell && g.size) {
        occupied.push({
          row: g.cell.row,
          col: g.cell.col,
          w: g.size.cols,
          h: g.size.rows,
        });
      }
    }

    // ---- place new groups ----
    // Compute each group's would-be size (from member spans) so we can sort
    // by area before placing.
    const newGroups: Array<{
      id: string;
      memberIds: string[];
      size: GroupSize;
      internalCells: Map<string, Cell>;
      area: number;
    }> = [];
    for (const g of groupsHere) {
      if (g.cell && g.size) continue;
      const newMembers = siblings.filter(
        (s) => g.memberIds.includes(s.id) && !s.cell,
      );
      if (newMembers.length === 0) {
        // Group has no unpositioned members; give it a minimum footprint so
        // the user has something to drag/resize. Still sort by area.
        const size: GroupSize = { cols: GROUP_SNAP_W, rows: GROUP_SNAP_H };
        newGroups.push({
          id: g.id,
          memberIds: g.memberIds,
          size,
          internalCells: new Map(),
          area: size.cols * size.rows,
        });
        continue;
      }
      // Pack members internally — largest first, row-major first-fit.
      const memberItems: PackItem[] = newMembers.map((n) => {
        const span = siblingSpan.get(n.id)!;
        return { id: n.id, span, area: span.w * span.h };
      });
      memberItems.sort((a, b) => b.area - a.area);
      const hdrRows = groupHeaderRows(g);
      const memberCap = colCapFor(memberItems);
      const localOccupied: Rect[] = [
        { row: 0, col: 0, w: 9999, h: hdrRows },
      ];
      const internalCells = new Map<string, Cell>();
      let bboxW = 1;
      let bboxH = hdrRows;
      for (const item of memberItems) {
        const at = firstFit(localOccupied, item.span, memberCap);
        internalCells.set(item.id, at);
        localOccupied.push({
          row: at.row,
          col: at.col,
          w: item.span.w,
          h: item.span.h,
        });
        bboxW = Math.max(bboxW, at.col + item.span.w);
        bboxH = Math.max(bboxH, at.row + item.span.h);
      }
      const size: GroupSize = {
        cols: Math.ceil(bboxW / GROUP_SNAP_W) * GROUP_SNAP_W,
        rows: Math.ceil(bboxH / GROUP_SNAP_H) * GROUP_SNAP_H,
      };
      newGroups.push({
        id: g.id,
        memberIds: g.memberIds,
        size,
        internalCells,
        area: size.cols * size.rows,
      });
    }

    // ---- new ungrouped nodes ----
    const newUngrouped: PackItem[] = siblings
      .filter((n) => !n.cell && !nodeGroup.has(n.id))
      .map((n) => {
        const span = siblingSpan.get(n.id)!;
        return { id: n.id, span, area: span.w * span.h };
      });

    if (newGroups.length === 0 && newUngrouped.length === 0) continue;

    // ---- merge groups + ungrouped, sort by area desc, place on parent ----
    type TopItem =
      | { kind: "group"; id: string; span: Span; area: number }
      | { kind: "node"; id: string; span: Span; area: number };
    const topItems: TopItem[] = [
      ...newGroups.map<TopItem>((g) => ({
        kind: "group",
        id: g.id,
        span: { w: g.size.cols, h: g.size.rows },
        area: g.area,
      })),
      ...newUngrouped.map<TopItem>((it) => ({
        kind: "node",
        id: it.id,
        span: it.span,
        area: it.area,
      })),
    ];
    topItems.sort((a, b) => b.area - a.area);
    const topCap = colCapFor(topItems);

    for (const item of topItems) {
      const at = firstFit(occupied, item.span, topCap);
      occupied.push({ row: at.row, col: at.col, w: item.span.w, h: item.span.h });
      if (item.kind === "node") {
        nodeCells.set(item.id, at);
      } else {
        const g = newGroups.find((x) => x.id === item.id)!;
        groupGeom.set(g.id, { cell: at, size: g.size });
        for (const [nid, local] of g.internalCells) {
          nodeCells.set(nid, {
            row: at.row + local.row,
            col: at.col + local.col,
          });
        }
      }
    }
  }

  // ---- assemble output ----
  if (nodeCells.size === 0 && groupGeom.size === 0) return model;

  return {
    ...model,
    nodes: model.nodes.map((n) =>
      n.cell || !nodeCells.has(n.id) ? n : { ...n, cell: nodeCells.get(n.id) },
    ),
    groups: model.groups.map((g) => {
      const geom = groupGeom.get(g.id);
      if (!geom) return g;
      return {
        ...g,
        cell: g.cell ?? geom.cell,
        size: g.size ?? geom.size,
      };
    }),
  };
}

// Helper used only by hydration — wraps a Node in a NodeView-shaped object so
// the shared span-calculation functions work without a full SurfaceView.
function makeNodeViewForHydration(
  n: Node,
  outgoing: Link[],
  incoming: Link[],
): NodeView {
  return {
    ...n,
    responsibilities: n.responsibilities ?? [],
    _outgoingLinks: outgoing,
    _incomingLinks: incoming,
    _childCount: 0,
    links: outgoing,
  };
}

// --- overlap correction ------------------------------------------------------

/**
 * Detect and fix overlapping cards within each parentId surface. Uses the
 * provided span map (typically from DOM measurement) to compute footprints.
 * Cards that overlap are pushed down to the first non-colliding row.
 *
 * Returns the input model unchanged (by reference) when nothing moved.
 */
export function resolveOverlaps(
  model: ScryModel,
  measuredSpans: ReadonlyMap<string, Span>,
): ScryModel {
  const corrections = new Map<string, Cell>();

  const parents = new Set<string | null>();
  for (const n of model.nodes) parents.add(n.parentId ?? null);

  // Precompute links for NodeView construction.
  const outLinks = new Map<string, Link[]>();
  const incLinks = new Map<string, Link[]>();
  for (const l of model.links) {
    (outLinks.get(l.src) ?? outLinks.set(l.src, []).get(l.src)!).push(l);
    (incLinks.get(l.dst) ?? incLinks.set(l.dst, []).get(l.dst)!).push(l);
  }

  for (const parentId of parents) {
    const siblings = model.nodes.filter(
      (n) => (n.parentId ?? null) === parentId && n.kind !== "person" && n.cell,
    );
    if (siblings.length < 2) continue;

    // Build rects using measured spans when available, heuristic otherwise.
    const rects: Array<{ id: string; rect: Rect }> = [];
    for (const n of siblings) {
      const nv = makeNodeViewForHydration(
        n,
        outLinks.get(n.id) ?? [],
        incLinks.get(n.id) ?? [],
      );
      const span = measuredSpans.get(n.id) ?? cardSpan(nv);
      rects.push({
        id: n.id,
        rect: { row: n.cell!.row, col: n.cell!.col, w: span.w, h: span.h },
      });
    }

    // Quick check: any overlaps at all?
    let hasOverlap = false;
    outer: for (let i = 0; i < rects.length; i++) {
      for (let j = i + 1; j < rects.length; j++) {
        if (rectsOverlap(rects[i].rect, rects[j].rect)) {
          hasOverlap = true;
          break outer;
        }
      }
    }
    if (!hasOverlap) continue;

    // Re-seat overlapping cards. Sort by (row, col) so earlier cards keep
    // their positions; later cards get pushed down.
    rects.sort((a, b) => a.rect.row - b.rect.row || a.rect.col - b.rect.col);
    const occupied: Rect[] = [];
    for (const entry of rects) {
      const cand = entry.rect;
      if (!occupied.some((o) => rectsOverlap(cand, o))) {
        occupied.push(cand);
        continue;
      }
      // Find first non-overlapping row at same column.
      let row = cand.row;
      while (true) {
        row++;
        const moved = { ...cand, row };
        if (!occupied.some((o) => rectsOverlap(moved, o))) {
          occupied.push(moved);
          corrections.set(entry.id, { row, col: cand.col });
          break;
        }
      }
    }
  }

  if (corrections.size === 0) return model;
  return {
    ...model,
    nodes: model.nodes.map((n) => {
      const cell = corrections.get(n.id);
      return cell ? { ...n, cell } : n;
    }),
  };
}

// --- model mutation ----------------------------------------------------------

import { deriveSurfaceView, setNodeCell, setNodeGroup } from "./viewmodel";

/**
 * Place a node at `cell` and optionally change its group membership.
 * `newGroupId === undefined` leaves the group unchanged; `null` clears it.
 * Returns the new model, or `null` if the placement is invalid.
 */
export function placeNodeInModel(
  model: ScryModel,
  nodeId: string,
  cell: Cell,
  newGroupId: string | null | undefined,
  measured?: ReadonlyMap<string, Span>,
): ScryModel | null {
  const node = model.nodes.find((n) => n.id === nodeId);
  if (!node) return null;
  const view = deriveSurfaceView(model, node.parentId ?? null);
  const viewNode = view.entries.find((n) => n.id === nodeId);
  if (!viewNode) return null;
  const sp = measured?.get(nodeId) ?? cardSpan(viewNode);
  const rect: Rect = { row: cell.row, col: cell.col, w: sp.w, h: sp.h };
  const targetGroupId =
    newGroupId === undefined
      ? viewNode._groupId
      : newGroupId === null
        ? undefined
        : newGroupId;
  if (!canDrop(view, rect, nodeId, targetGroupId, measured)) return null;

  let next = setNodeCell(model, nodeId, cell);
  if (newGroupId !== undefined) {
    next = setNodeGroup(next, nodeId, newGroupId);
  }
  return rebaseModelAt(next, node.parentId ?? null);
}

/**
 * Translate a group (and its descendants + member nodes) by (dRow, dCol).
 * Returns the new model, or `null` if any move would be invalid.
 */
export function moveGroupInModel(
  model: ScryModel,
  groupId: string,
  dRow: number,
  dCol: number,
  measured?: ReadonlyMap<string, Span>,
): ScryModel | null {
  // Resolve the depth at which this group lives — from its first member, or
  // from its explicit parentNodeId for empty groups.
  const group = model.groups.find((g) => g.id === groupId);
  if (!group) return null;
  const anyMemberId = group.memberIds[0];
  let parentId: string | null;
  if (anyMemberId) {
    const memberNode = model.nodes.find((n) => n.id === anyMemberId);
    if (!memberNode) return null;
    parentId = memberNode.parentId ?? null;
  } else {
    parentId = group.parentNodeId ?? null;
  }

  const view = deriveSurfaceView(model, parentId);
  const result = canMoveGroup(view, groupId, dRow, dCol, measured);
  if (!result) return null;
  const { newParentId } = result;

  const descendants = descendantGroupIds(view, groupId);
  const movingGroupIds = new Set([groupId, ...descendants]);
  const movingNodeIds = new Set(
    gridEntries(view)
      .filter((n) => n._groupId && movingGroupIds.has(n._groupId))
      .map((n) => n.id),
  );

  const next: ScryModel = {
    ...model,
    nodes: model.nodes.map((n) => {
      if (!movingNodeIds.has(n.id) || !n.cell) return n;
      return { ...n, cell: { row: n.cell.row + dRow, col: n.cell.col + dCol } };
    }),
    groups: model.groups.map((g) => {
      if (g.id === groupId) {
        const cell = g.cell ?? { row: 0, col: 0 };
        return {
          ...g,
          cell: { row: cell.row + dRow, col: cell.col + dCol },
          parentGroupId: newParentId ?? undefined,
        };
      }
      if (descendants.has(g.id) && g.cell) {
        return {
          ...g,
          cell: { row: g.cell.row + dRow, col: g.cell.col + dCol },
        };
      }
      return g;
    }),
  };
  return rebaseModelAt(next, parentId);
}

/**
 * Resize a group. Returns the new model, or `null` if the new size doesn't
 * contain its members / overlaps neighbours.
 */
export function resizeGroupInModel(
  model: ScryModel,
  groupId: string,
  size: { cols: number; rows: number },
  measured?: ReadonlyMap<string, Span>,
): ScryModel | null {
  const group = model.groups.find((g) => g.id === groupId);
  if (!group) return null;
  const memberId = group.memberIds[0];
  let parentId: string | null;
  if (memberId) {
    const memberNode = model.nodes.find((n) => n.id === memberId);
    if (!memberNode) return null;
    parentId = memberNode.parentId ?? null;
  } else {
    parentId = group.parentNodeId ?? null;
  }

  const view = deriveSurfaceView(model, parentId);
  const snapped = clampGroupSize(view, groupId, size, measured);
  if (!snapped) return null;

  return {
    ...model,
    groups: model.groups.map((g) => (g.id === groupId ? { ...g, size: snapped } : g)),
  };
}

/**
 * Shift all sibling nodes at `parentId` (and the groups they belong to) so the
 * top-left of the content is exactly at (0, 0). Keeps the grid container from
 * accumulating empty top/left rows as content drifts off the origin.
 */
function rebaseModelAt(model: ScryModel, parentId: string | null): ScryModel {
  const siblings = model.nodes.filter(
    (n) => (n.parentId ?? null) === parentId && n.cell,
  );
  if (siblings.length === 0) return model;
  const view = deriveSurfaceView(model, parentId);
  const siblingGroupIds = new Set(
    view.groups.map((g) => g.id),
  );

  let minRow = Infinity;
  let minCol = Infinity;
  for (const n of siblings) {
    if (!n.cell) continue;
    minRow = Math.min(minRow, n.cell.row);
    minCol = Math.min(minCol, n.cell.col);
  }
  for (const g of model.groups) {
    if (!siblingGroupIds.has(g.id) || !g.cell) continue;
    minRow = Math.min(minRow, g.cell.row);
    minCol = Math.min(minCol, g.cell.col);
  }
  if (!Number.isFinite(minRow) || !Number.isFinite(minCol)) return model;
  if (minRow === 0 && minCol === 0) return model;
  const dRow = -minRow;
  const dCol = -minCol;
  return {
    ...model,
    nodes: model.nodes.map((n) => {
      if ((n.parentId ?? null) !== parentId || !n.cell) return n;
      return {
        ...n,
        cell: { row: n.cell.row + dRow, col: n.cell.col + dCol },
      };
    }),
    groups: model.groups.map((g) => {
      if (!siblingGroupIds.has(g.id) || !g.cell) return g;
      return {
        ...g,
        cell: { row: g.cell.row + dRow, col: g.cell.col + dCol },
      };
    }),
  };
}

// --- re-exports --------------------------------------------------------------

export type { Group };
