/**
 * Navigable surface — one flat inventory grid, ringed by perimeter
 * collaborators (people, externals, refs).
 *
 * Receives a *derived* SurfaceView (computed by App from the model) plus
 * intent callbacks. Mutations don't happen here: we ask App to place a node,
 * move a group, or resize a group; App applies the change to the model and
 * the next render gives us a fresh view.
 */

import { useCallback, useContext, useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import type { Cell, Kind, NodeView, SurfaceView } from "./viewmodel";
import type { AgentSession } from "./hooks/useAgentSession";
import { childKindFor } from "./viewmodel";
import { EntryCard, EntryCardView } from "./EntryCard";
import { PackBox, LABEL_H } from "./PackBox";
import { GridContext } from "./gridcontext";
import type { HoverState } from "./gridcontext";
import { useZoom, usePan } from "./PanZoom";
import { ModelContext, VisibleScopeContext } from "./modelcontext";
import { surfaceContext } from "./references";
import {
  CELL_W,
  CELL_H,
  ITEM_INSET,
  GROUP_SNAP_W,
  GROUP_SNAP_H,
  cardSpan,
  groupRect,
  groupAtCell,
  canDrop,
  canMoveGroup,
  clampGroupSize,
  type Rect,
  type Span,
} from "./pack";
import { ContextMenu } from "./ContextMenu";
import type { Editor } from "./editor";

const ALTITUDE_LABEL: Record<SurfaceView["altitude"], string> = {
  system: "System Context",
  container: "Container View",
  component: "Component View",
  code: "Code",
};

const CLICK_SLOP = 5;
const EDGE_BAND = 64;
const EDGE_SPEED = 16;

/** Find a node on this surface by id. */
function locate(view: SurfaceView, id: string): NodeView | null {
  return view.entries.find((n) => n.id === id) ?? null;
}

/** The pickable node card directly under a point. */
function pickableAt(view: SurfaceView, x: number, y: number): NodeView | null {
  const els = document.elementsFromPoint(x, y);
  if ((els[0] as HTMLElement | undefined)?.closest("[data-no-pickup]")) {
    return null;
  }
  const el = els.find((e) => e.hasAttribute("data-pickup")) as
    | HTMLElement
    | undefined;
  if (!el) return null;
  const id = el.getAttribute("data-pickup")!;
  if (el.hasAttribute("data-group-pickup")) return null;
  return locate(view, id);
}

function hoverForNode(
  view: SurfaceView,
  node: NodeView,
  zoom: number,
  x: number,
  y: number,
  measured?: ReadonlyMap<string, Span>,
): HoverState | null {
  const gridEl = document.querySelector("[data-grid]") as HTMLElement | null;
  if (!gridEl) return null;
  const gr = gridEl.getBoundingClientRect();
  const sp = measured?.get(node.id) ?? cardSpan(node);
  const col = Math.round((x - gr.left) / (CELL_W * zoom) - sp.w / 2);
  const row = Math.round((y - gr.top) / (CELL_H * zoom) - sp.h / 2);
  const rect: Rect = { row, col, w: sp.w, h: sp.h };
  const targetGroup = groupAtCell(view, { row, col });
  const targetGroupId = targetGroup?.id;
  const valid = canDrop(view, rect, node.id, targetGroupId, measured);
  return { rect, valid, targetGroupId };
}

function hoverForGroup(
  view: SurfaceView,
  groupId: string,
  anchorRow: number,
  anchorCol: number,
  zoom: number,
  x: number,
  y: number,
  measured?: ReadonlyMap<string, Span>,
): HoverState | null {
  const gridEl = document.querySelector("[data-grid]") as HTMLElement | null;
  if (!gridEl) return null;
  const group = view.groups.find((g) => g.id === groupId);
  if (!group) return null;
  const gr = gridEl.getBoundingClientRect();
  const region = groupRect(group);
  const curCol = Math.round((x - gr.left) / (CELL_W * zoom));
  const curRow = Math.round((y - gr.top) / (CELL_H * zoom));
  const dRow = curRow - anchorRow;
  const dCol = curCol - anchorCol;
  const rect: Rect = {
    row: region.row + dRow,
    col: region.col + dCol,
    w: region.w,
    h: region.h,
  };
  const valid = canMoveGroup(view, groupId, dRow, dCol, measured) !== undefined;
  return { rect, valid };
}

type HeldItem =
  | { kind: "node"; node: NodeView }
  | { kind: "group"; groupId: string; anchorRow: number; anchorCol: number };

function EmptyLevelCta({
  parentNodeId,
  projectPath,
  modelRef,
  zoom,
  agent,
}: {
  parentNodeId: string;
  projectPath: string;
  modelRef: string;
  zoom: number;
  agent: AgentSession;
}) {
  const model = useContext(ModelContext);
  const parentNode = useMemo(
    () => model.nodes.find((n) => n.id === parentNodeId),
    [model, parentNodeId],
  );

  if (!parentNode) return null;

  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        gap: 8 * zoom,
        padding: `${16 * zoom}px ${24 * zoom}px`,
        textAlign: "center",
      }}
    >
      <span
        style={{
          fontSize: 11 * zoom,
          color: "var(--text-ghost)",
        }}
      >
        No children yet
      </span>
      <button
        type="button"
        onClick={() =>
          agent.startFill(
            projectPath,
            modelRef,
            parentNodeId,
            parentNode.name || "node",
          )
        }
        disabled={agent.running}
        style={{
          padding: `${6 * zoom}px ${16 * zoom}px`,
          borderRadius: 6 * zoom,
          border: "none",
          backgroundColor: agent.running
            ? "var(--color-blue-800)"
            : "var(--color-blue-600)",
          fontSize: 11 * zoom,
          fontWeight: 500,
          color: "#fff",
          cursor: agent.running ? "default" : "pointer",
          opacity: agent.running ? 0.5 : 1,
        }}
      >
        Fill with AI
      </button>
      <span
        style={{
          fontSize: 10 * zoom,
          color: "var(--text-ghost)",
        }}
      >
        or right-click to add manually
      </span>
    </div>
  );
}

export function Surface({
  view,
  parentNodeId,
  ancestorAltitudes = [],
  editor,
  onNavigate,
  onBack,
  onPlaceNode,
  onMoveGroup,
  onResizeGroup,
  onFixOverlaps,
  projectPath,
  modelRef: modelRefStr,
  agent,
}: {
  view: SurfaceView;
  parentNodeId: string | null;
  ancestorAltitudes?: SurfaceView["altitude"][];
  editor?: Editor;
  onNavigate: (nodeId: string) => void;
  onBack?: (ancestorIndex: number) => void;
  onPlaceNode: (
    nodeId: string,
    cell: Cell,
    newGroupId: string | null | undefined,
    measured: ReadonlyMap<string, Span>,
  ) => void;
  onMoveGroup: (
    groupId: string,
    dRow: number,
    dCol: number,
    measured: ReadonlyMap<string, Span>,
  ) => void;
  onResizeGroup: (
    groupId: string,
    size: { cols: number; rows: number },
    measured: ReadonlyMap<string, Span>,
  ) => void;
  onFixOverlaps?: (measuredSpans: ReadonlyMap<string, Span>) => void;
  projectPath?: string | null;
  modelRef?: string | null;
  agent?: AgentSession;
}) {
  const zoom = useZoom();
  const panBy = usePan();
  const model = useContext(ModelContext);
  const context = surfaceContext(model, view.parentId);

  const [measuredSpans, setMeasuredSpans] = useState<ReadonlyMap<string, Span>>(
    () => new Map(),
  );
  const measuredRef = useRef(measuredSpans);
  measuredRef.current = measuredSpans;

  const [held, setHeld] = useState<HeldItem | null>(null);
  const [hover, setHover] = useState<HoverState | null>(null);
  const [resizingId, setResizingId] = useState<string | null>(null);
  const [resizeRejected, setResizeRejected] = useState<{
    right: boolean;
    bottom: boolean;
  } | null>(null);
  const [menu, setMenu] = useState<{
    x: number;
    y: number;
    cell: Cell;
    groupId?: string;
  } | null>(null);
  const rejectTimer = useRef<ReturnType<typeof setTimeout>>(undefined);

  const heldRef = useRef(held);
  const viewRef = useRef(view);
  const zoomRef = useRef(zoom);
  const downRef = useRef<{
    x: number;
    y: number;
    candidateNode: NodeView | null;
    candidateGroupId: string | null;
  } | null>(null);
  const pointerRef = useRef({ x: 0, y: 0 });
  const resizeRef = useRef<{
    groupId: string;
    startSize: { cols: number; rows: number };
    sx: number;
    sy: number;
  } | null>(null);
  heldRef.current = held;
  viewRef.current = view;
  zoomRef.current = zoom;

  const endHold = useCallback(() => {
    heldRef.current = null;
    document.body.classList.remove("dragging");
    setHeld(null);
    setHover(null);
  }, []);

  const cancelHeld = useCallback(() => {
    endHold();
  }, [endHold]);

  const startHoldNode = useCallback(
    (node: NodeView, x: number, y: number) => {
      document.body.classList.add("dragging");
      const h: HeldItem = { kind: "node", node };
      heldRef.current = h;
      pointerRef.current = { x, y };
      setHeld(h);
      setHover(hoverForNode(viewRef.current, node, zoomRef.current, x, y, measuredRef.current));
    },
    [],
  );

  const startHoldGroup = useCallback(
    (groupId: string, x: number, y: number) => {
      document.body.classList.add("dragging");
      const gridEl = document.querySelector("[data-grid]") as HTMLElement | null;
      if (!gridEl) return;
      const gr = gridEl.getBoundingClientRect();
      const anchorCol = Math.round((x - gr.left) / (CELL_W * zoomRef.current));
      const anchorRow = Math.round((y - gr.top) / (CELL_H * zoomRef.current));
      const h: HeldItem = { kind: "group", groupId, anchorRow, anchorCol };
      heldRef.current = h;
      pointerRef.current = { x, y };
      setHeld(h);
      setHover(
        hoverForGroup(
          viewRef.current,
          groupId,
          anchorRow,
          anchorCol,
          zoomRef.current,
          x,
          y,
          measuredRef.current,
        ),
      );
    },
    [],
  );

  const tryPlaceNode = useCallback(
    (node: NodeView, x: number, y: number): boolean => {
      const ms = measuredRef.current;
      const hv = hoverForNode(viewRef.current, node, zoomRef.current, x, y, ms);
      if (hv?.valid) {
        onPlaceNode(
          node.id,
          { row: hv.rect.row, col: hv.rect.col },
          hv.targetGroupId ?? null,
          ms,
        );
        return true;
      }
      return false;
    },
    [onPlaceNode],
  );

  const tryPlaceGroup = useCallback(
    (
      groupId: string,
      anchorRow: number,
      anchorCol: number,
      x: number,
      y: number,
    ): boolean => {
      const hv = hoverForGroup(
        viewRef.current,
        groupId,
        anchorRow,
        anchorCol,
        zoomRef.current,
        x,
        y,
        measuredRef.current,
      );
      if (!hv?.valid) return false;
      const group = viewRef.current.groups.find((g) => g.id === groupId);
      if (!group) return false;
      const dRow = hv.rect.row - group.cell.row;
      const dCol = hv.rect.col - group.cell.col;
      if (dRow === 0 && dCol === 0) return true;
      onMoveGroup(groupId, dRow, dCol, measuredRef.current);
      return true;
    },
    [onMoveGroup],
  );

  const beginResize = useCallback(
    (groupId: string, e: { clientX: number; clientY: number }) => {
      const group = viewRef.current.groups.find((g) => g.id === groupId);
      if (!group) return;
      document.body.classList.add("dragging");
      resizeRef.current = {
        groupId,
        startSize: group.size,
        sx: e.clientX,
        sy: e.clientY,
      };
      setResizingId(groupId);
    },
    [],
  );

  const beginGroupDrag = useCallback(
    (groupId: string, e: { clientX: number; clientY: number }) => {
      document.body.classList.add("dragging");
      downRef.current = {
        x: e.clientX,
        y: e.clientY,
        candidateNode: null,
        candidateGroupId: groupId,
      };
    },
    [],
  );

  useEffect(() => {
    const onDown = (e: PointerEvent) => {
      if (heldRef.current || resizeRef.current) return;
      if ((e.target as Element).closest("[data-no-pickup]")) return;
      const node = pickableAt(viewRef.current, e.clientX, e.clientY);
      if (!node) return;
      document.body.classList.add("dragging");
      downRef.current = {
        x: e.clientX,
        y: e.clientY,
        candidateNode: node,
        candidateGroupId: null,
      };
    };

    const onMove = (e: PointerEvent) => {
      pointerRef.current = { x: e.clientX, y: e.clientY };

      const resize = resizeRef.current;
      if (resize) {
        const z = zoomRef.current;
        const groupPx = CELL_W * GROUP_SNAP_W;
        const groupPy = CELL_H * GROUP_SNAP_H;
        const desired = {
          cols:
            resize.startSize.cols +
            Math.round((e.clientX - resize.sx) / (groupPx * z)) * GROUP_SNAP_W,
          rows:
            resize.startSize.rows +
            Math.round((e.clientY - resize.sy) / (groupPy * z)) * GROUP_SNAP_H,
        };
        const clamped = {
          cols: Math.max(GROUP_SNAP_W, desired.cols),
          rows: Math.max(GROUP_SNAP_H, desired.rows),
        };
        const cur = viewRef.current;
        const ms = measuredRef.current;
        const snapped = clampGroupSize(cur, resize.groupId, clamped, ms);
        if (snapped) {
          onResizeGroup(resize.groupId, snapped, ms);
          if (resizeRejected) setResizeRejected(null);
        } else {
          const group = cur.groups.find((g) => g.id === resize.groupId);
          if (group) {
            const colsChanged = clamped.cols !== group.size.cols;
            const rowsChanged = clamped.rows !== group.size.rows;
            if (colsChanged || rowsChanged) {
              const rightBlocked =
                colsChanged &&
                !clampGroupSize(cur, resize.groupId, {
                  cols: clamped.cols,
                  rows: group.size.rows,
                }, ms);
              const bottomBlocked =
                rowsChanged &&
                !clampGroupSize(cur, resize.groupId, {
                  cols: group.size.cols,
                  rows: clamped.rows,
                }, ms);
              setResizeRejected({
                right: !!rightBlocked,
                bottom: !!bottomBlocked,
              });
              clearTimeout(rejectTimer.current);
              rejectTimer.current = setTimeout(
                () => setResizeRejected(null),
                400,
              );
            }
          }
        }
        return;
      }

      const h = heldRef.current;
      if (h) {
        const ms = measuredRef.current;
        if (h.kind === "node") {
          setHover(
            hoverForNode(viewRef.current, h.node, zoomRef.current, e.clientX, e.clientY, ms),
          );
        } else if (h.kind === "group") {
          setHover(
            hoverForGroup(
              viewRef.current,
              h.groupId,
              h.anchorRow,
              h.anchorCol,
              zoomRef.current,
              e.clientX,
              e.clientY,
              ms,
            ),
          );
        }
        return;
      }

      const down = downRef.current;
      if (!down) return;
      const moved = Math.hypot(e.clientX - down.x, e.clientY - down.y);
      if (moved >= CLICK_SLOP) {
        if (down.candidateNode) {
          startHoldNode(down.candidateNode, e.clientX, e.clientY);
        } else if (down.candidateGroupId) {
          startHoldGroup(down.candidateGroupId, e.clientX, e.clientY);
        }
        downRef.current = null;
      }
    };

    const onUp = (e: PointerEvent) => {
      if (resizeRef.current) {
        resizeRef.current = null;
        setResizingId(null);
        document.body.classList.remove("dragging");
        return;
      }
      downRef.current = null;
      const h = heldRef.current;
      if (!h) {
        document.body.classList.remove("dragging");
        return;
      }
      if (h.kind === "node") {
        tryPlaceNode(h.node, e.clientX, e.clientY);
      } else {
        tryPlaceGroup(h.groupId, h.anchorRow, h.anchorCol, e.clientX, e.clientY);
      }
      endHold();
    };

    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") cancelHeld();
    };
    const onCtx = (e: MouseEvent) => {
      if (heldRef.current) {
        e.preventDefault();
        cancelHeld();
        return;
      }
      const tgt = e.target as Element | null;
      if (!tgt) return;
      if (tgt.closest("[data-pickup]")) return;
      // Accept right-click anywhere on the canvas surface, not just inside
      // the grid. If the click didn't land directly on [data-grid], look it
      // up from the DOM so we can still compute a cell position.
      const vp = tgt.closest("#panzoom-viewport");
      if (!vp) return;
      const grid = (tgt.closest("[data-grid]") ??
        document.querySelector("[data-grid]")) as HTMLElement | null;
      if (!grid) return;
      e.preventDefault();
      const rect = grid.getBoundingClientRect();
      const z = zoomRef.current;
      const cell: Cell = {
        col: Math.floor((e.clientX - rect.left) / (CELL_W * z)),
        row: Math.floor((e.clientY - rect.top) / (CELL_H * z)),
      };
      const containingGroup = groupAtCell(viewRef.current, cell);
      setMenu({
        x: e.clientX,
        y: e.clientY,
        cell,
        groupId: containingGroup?.id,
      });
    };

    window.addEventListener("pointerdown", onDown);
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    window.addEventListener("keydown", onKey);
    window.addEventListener("contextmenu", onCtx);
    return () => {
      window.removeEventListener("pointerdown", onDown);
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("contextmenu", onCtx);
    };
  }, [
    cancelHeld,
    endHold,
    onResizeGroup,
    resizeRejected,
    startHoldGroup,
    startHoldNode,
    tryPlaceGroup,
    tryPlaceNode,
  ]);

  // Auto-pan while carrying
  useEffect(() => {
    if (!held) return;
    let raf = 0;
    const tick = () => {
      const vp = document
        .getElementById("panzoom-viewport")
        ?.getBoundingClientRect();
      const p = pointerRef.current;
      if (vp) {
        let dx = 0;
        let dy = 0;
        if (p.x < vp.left + EDGE_BAND) dx = EDGE_SPEED;
        else if (p.x > vp.right - EDGE_BAND) dx = -EDGE_SPEED;
        if (p.y < vp.top + EDGE_BAND) dy = EDGE_SPEED;
        else if (p.y > vp.bottom - EDGE_BAND) dy = -EDGE_SPEED;
        if (dx || dy) {
          panBy(dx, dy);
          const ms = measuredRef.current;
          if (held.kind === "node") {
            setHover(
              hoverForNode(viewRef.current, held.node, zoomRef.current, p.x, p.y, ms),
            );
          } else if (held.kind === "group") {
            setHover(
              hoverForGroup(
                viewRef.current,
                held.groupId,
                held.anchorRow,
                held.anchorCol,
                zoomRef.current,
                p.x,
                p.y,
                ms,
              ),
            );
          }
        }
      }
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [held, panBy]);

  const handleLinkClick = useCallback(
    (partnerId: string) => {
      const vp = document.getElementById("panzoom-viewport");
      const target = vp?.querySelector(
        `[data-pickup="${partnerId}"]`,
      ) as HTMLElement | null;
      if (!vp || !target) return;
      const vr = vp.getBoundingClientRect();
      const tr = target.getBoundingClientRect();
      panBy(
        vr.left + vr.width / 2 - (tr.left + tr.width / 2),
        vr.top + vr.height / 2 - (tr.top + tr.height / 2),
      );
    },
    [panBy],
  );

  const visibleScope = new Set<string>([
    ...view.entries.map((n) => n.id),
    ...context.persons.map((p) => p.node.id),
    ...context.externals.map((x) => x.node.id),
    ...context.refs.map((r) => r.node.id),
  ]);

  const heldNode = held?.kind === "node" ? held.node : null;
  const heldSpan = heldNode
    ? (measuredSpans.get(heldNode.id) ?? cardSpan(heldNode))
    : null;
  const boardHighlight =
    hover && held
      ? { rect: hover.rect, valid: hover.valid, inset: held.kind === "node" }
      : null;

  const heldId =
    held?.kind === "node"
      ? held.node.id
      : held?.kind === "group"
        ? held.groupId
        : null;

  const levels = [
    ...ancestorAltitudes.map((a) => ({ altitude: a, label: ALTITUDE_LABEL[a] })),
    { altitude: view.altitude, label: ALTITUDE_LABEL[view.altitude] },
  ];

  return (
    <GridContext.Provider
      value={{
        beginResize,
        beginGroupDrag,
        heldId,
        resizingId,
        resizeRejected,
        hover,
      }}
    >
      <VisibleScopeContext.Provider value={visibleScope}>
        <div style={{ margin: 80 * zoom, position: "relative" }}>
          <PackBox
            view={view}
            highlight={boardHighlight}
            levels={levels}
            context={context}
            onRingClick={onBack}
            editor={editor}
            measuredSpans={measuredSpans}
            onMeasure={setMeasuredSpans}
            onFixOverlaps={onFixOverlaps}
            renderEntry={(node) => (
              <EntryCard
                node={node}
                onNavigate={onNavigate}
                onLinkClick={handleLinkClick}
                editor={editor}
              />
            )}
            emptyContent={
              parentNodeId && projectPath && modelRefStr && agent && !agent.running ? (
                <EmptyLevelCta
                  parentNodeId={parentNodeId}
                  projectPath={projectPath}
                  modelRef={modelRefStr}
                  zoom={zoom}
                  agent={agent}
                />
              ) : undefined
            }
          />
        </div>

        {heldNode &&
          heldSpan &&
          hover &&
          (() => {
            const gridEl = document.querySelector("[data-grid]") as HTMLElement | null;
            if (!gridEl) return null;
            const gr = gridEl.getBoundingClientRect();
            return createPortal(
              <div
                style={{
                  position: "fixed",
                  left:
                    gr.left + (hover.rect.col * CELL_W + ITEM_INSET) * zoom,
                  top: gr.top + (hover.rect.row * CELL_H + ITEM_INSET) * zoom,
                  width: (heldSpan.w * CELL_W - ITEM_INSET * 2) * zoom,
                  height: (heldSpan.h * CELL_H - ITEM_INSET * 2) * zoom,
                  pointerEvents: "none",
                  opacity: 0.85,
                  zIndex: 1000,
                }}
              >
                <EntryCardView node={heldNode} span={heldSpan} lifted />
              </div>,
              document.body,
            );
          })()}

        {held?.kind === "group" &&
          hover &&
          (() => {
            const g = view.groups.find((x) => x.id === held.groupId);
            if (!g) return null;
            const gridEl = document.querySelector("[data-grid]") as HTMLElement | null;
            if (!gridEl) return null;
            const gr = gridEl.getBoundingClientRect();
            const w = g.size.cols * CELL_W * zoom;
            const h = g.size.rows * CELL_H * zoom;
            const labelH = LABEL_H * zoom;
            return createPortal(
              <div
                style={{
                  position: "fixed",
                  left: gr.left + hover.rect.col * CELL_W * zoom,
                  top: gr.top + hover.rect.row * CELL_H * zoom,
                  width: w,
                  height: h,
                  pointerEvents: "none",
                  zIndex: 1000,
                }}
              >
                <div
                  style={{
                    position: "absolute",
                    inset: 0,
                    border: `${zoom}px solid var(--border)`,
                    backgroundColor:
                      "color-mix(in srgb, black 3%, transparent)",
                    borderRadius: 12 * zoom,
                    opacity: 0.55,
                    boxShadow: "0 8px 24px rgba(0,0,0,0.2)",
                  }}
                />
                <div
                  className="flex items-center"
                  style={{
                    position: "absolute",
                    left: 8 * zoom,
                    top: -labelH / 2,
                    height: labelH,
                    padding: `0 ${8 * zoom}px`,
                    backgroundColor: "var(--surface-canvas)",
                    borderRadius: 5 * zoom,
                    border: `${zoom}px solid var(--border)`,
                  }}
                >
                  <span
                    className="font-bold uppercase text-[var(--text-ghost)]"
                    style={{
                      fontSize: 10 * zoom,
                      letterSpacing: 0.12 * zoom + "em",
                    }}
                  >
                    {g.name || "Untitled"}
                  </span>
                </div>
              </div>,
              document.body,
            );
          })()}
        {menu && editor &&
          (() => {
            // Kinds valid as children at this altitude. The root surface
            // (parentNodeId === null) allows person + system; below that the
            // C4 ladder applies. Groups are always offerable.
            const kinds: Kind[] =
              parentNodeId === null
                ? ["person", "system"]
                : view.altitude === "code"
                  ? ["operation", "model"]
                  : [childKindFor(parentInferKind(view.altitude))];
            return (
              <ContextMenu
                x={menu.x}
                y={menu.y}
                items={[
                  ...kinds.map((k) => ({
                    id: `add-${k}`,
                    label: `Add ${capitalize(k)}`,
                    onSelect: () => {
                      const id = editor.addNode({
                        kind: k,
                        parentId: parentNodeId ?? undefined,
                        cell: menu.cell,
                        groupId: menu.groupId,
                      });
                      // The new card mounts with `name` empty → InlineText
                      // displays its placeholder; clicking begins inline edit.
                      void id;
                    },
                  })),
                  {
                    id: "add-group",
                    label: "Add Group",
                    onSelect: () => {
                      editor.addGroup({
                        parentNodeId,
                        cell: menu.cell,
                      });
                    },
                  },
                ]}
                onClose={() => setMenu(null)}
              />
            );
          })()}
      </VisibleScopeContext.Provider>
    </GridContext.Provider>
  );
}

function capitalize(s: string): string {
  return s.length > 0 ? s[0].toUpperCase() + s.slice(1) : s;
}

/**
 * Given a SurfaceView's altitude, return the parent's *kind* so that
 * `childKindFor()` produces the correct child kind for this surface.
 *
 *  surface altitude = "container"  → parent is a system     → child = container
 *  surface altitude = "component"  → parent is a container  → child = component
 *  surface altitude = "code"       → parent is a component  → child = operation
 *  surface altitude = "system"     → root surface           → child = system
 */
function parentInferKind(altitude: SurfaceView["altitude"]): Kind | "root" {
  switch (altitude) {
    case "system":
      return "root";
    case "container":
      return "system";
    case "component":
      return "container";
    case "code":
      return "component";
  }
}
