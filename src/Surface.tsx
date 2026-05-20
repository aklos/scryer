/**
 * A navigable surface — one flat inventory grid, ringed by perimeter
 * collaborators (people, external systems, and reference context).
 *
 * Move an entry by dragging it: press a card to drag, or click to pick it onto
 * the cursor then click again to place. Press Esc or right-click to cancel.
 * The canvas pans from empty background. Nothing is ever pushed or reflowed —
 * an occupied drop is simply refused.
 *
 * Groups are auto-fit overlays — their region derives from the bounding box of
 * their member entries. Drag a group's label to move all members together.
 */

import { useCallback, useContext, useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import type { Surface as SurfaceModel, Entry } from "./viewmodel";
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
  GROUP_SNAP,
  cardSpan,
  groupRect,
  groupAtCell,
  canDrop,
  placeEntry,
  moveGroup,
  resizeGroup,
  type Rect,
} from "./pack";

const ALTITUDE_LABEL: Record<SurfaceModel["altitude"], string> = {
  system: "System Context",
  container: "Container View",
  component: "Component View",
};

const CLICK_SLOP = 5;
const EDGE_BAND = 64;
const EDGE_SPEED = 16;

/** Find an entry on this surface by id. */
function locate(surface: SurfaceModel, id: string): Entry | null {
  return surface.entries.find((e) => e.id === id) ?? null;
}

/** The pickable entry card directly under a point. */
function pickableAt(surface: SurfaceModel, x: number, y: number): Entry | null {
  const els = document.elementsFromPoint(x, y);
  if ((els[0] as HTMLElement | undefined)?.closest("[data-no-pickup]")) {
    return null;
  }
  const el = els.find((e) => e.hasAttribute("data-pickup")) as
    | HTMLElement
    | undefined;
  if (!el) return null;
  const id = el.getAttribute("data-pickup")!;
  // If it's a group label, return null (group dragging handled separately)
  if (el.hasAttribute("data-group-pickup")) return null;
  return locate(surface, id);
}

/**
 * The footprint a held entry would occupy on the flat grid, whether the drop
 * is legal, and which group (if any) the entry would join.
 */
function hoverFor(
  surface: SurfaceModel,
  entry: Entry,
  zoom: number,
  x: number,
  y: number,
): HoverState | null {
  const gridEl = document.querySelector("[data-grid]") as HTMLElement | null;
  if (!gridEl) return null;
  const gr = gridEl.getBoundingClientRect();
  const sp = cardSpan(entry);
  const col = Math.round((x - gr.left) / (CELL_W * zoom) - sp.w / 2);
  const row = Math.round((y - gr.top) / (CELL_H * zoom) - sp.h / 2);
  const rect: Rect = { row, col, w: sp.w, h: sp.h };
  const targetGroup = groupAtCell(surface, { row, col });
  const targetGroupId = targetGroup?.id;
  const valid = canDrop(surface, rect, entry.id, targetGroupId);
  return { rect, valid, targetGroupId };
}

function hoverForGroup(
  surface: SurfaceModel,
  groupId: string,
  anchorRow: number,
  anchorCol: number,
  zoom: number,
  x: number,
  y: number,
): HoverState | null {
  const gridEl = document.querySelector("[data-grid]") as HTMLElement | null;
  if (!gridEl) return null;
  const group = surface.groups.find((g) => g.id === groupId);
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
  const valid = moveGroup(surface, groupId, dRow, dCol) !== null;
  return { rect, valid };
}

// --- held state: either an entry or a group being dragged --------------------

type HeldItem =
  | { kind: "entry"; entry: Entry }
  | { kind: "group"; groupId: string; anchorRow: number; anchorCol: number };

export function Surface({
  surface,
  ancestorAltitudes = [],
  onChange,
  onNavigate,
  onBack,
}: {
  surface: SurfaceModel;
  ancestorAltitudes?: SurfaceModel["altitude"][];
  onChange: (next: SurfaceModel) => void;
  onNavigate: (childSurfaceId: string) => void;
  onBack?: (ancestorIndex: number) => void;
}) {
  const zoom = useZoom();
  const panBy = usePan();
  const surfaces = useContext(ModelContext);
  const context = surfaceContext(surfaces, surface);

  const [held, setHeld] = useState<HeldItem | null>(null);
  const [hover, setHover] = useState<HoverState | null>(null);
  const [resizingId, setResizingId] = useState<string | null>(null);
  const [resizeRejected, setResizeRejected] = useState<{ right: boolean; bottom: boolean } | null>(null);
  const rejectTimer = useRef<ReturnType<typeof setTimeout>>(undefined);

  const heldRef = useRef(held);
  const surfaceRef = useRef(surface);
  const zoomRef = useRef(zoom);
  const onChangeRef = useRef(onChange);
  const downRef = useRef<{
    x: number;
    y: number;
    candidateEntry: Entry | null;
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
  surfaceRef.current = surface;
  zoomRef.current = zoom;
  onChangeRef.current = onChange;

  const endHold = useCallback(() => {
    heldRef.current = null;
    document.body.classList.remove("dragging");
    setHeld(null);
    setHover(null);
  }, []);

  const cancelHeld = useCallback(() => {
    endHold();
  }, [endHold]);

  const startHoldEntry = useCallback(
    (entry: Entry, x: number, y: number) => {
      document.body.classList.add("dragging");
      const h: HeldItem = { kind: "entry", entry };
      heldRef.current = h;
      pointerRef.current = { x, y };
      setHeld(h);
      setHover(hoverFor(surfaceRef.current, entry, zoomRef.current, x, y));
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
      setHover(hoverForGroup(surfaceRef.current, groupId, anchorRow, anchorCol, zoomRef.current, x, y));
    },
    [],
  );

  const tryPlaceEntry = useCallback(
    (entry: Entry, x: number, y: number): boolean => {
      const hv = hoverFor(surfaceRef.current, entry, zoomRef.current, x, y);
      if (hv?.valid) {
        const ns = placeEntry(
          surfaceRef.current,
          entry.id,
          { row: hv.rect.row, col: hv.rect.col },
          hv.targetGroupId ?? null,
        );
        if (ns) onChangeRef.current(ns);
        return true;
      }
      return false;
    },
    [],
  );

  const tryPlaceGroup = useCallback(
    (groupId: string, anchorRow: number, anchorCol: number, x: number, y: number): boolean => {
      const hv = hoverForGroup(surfaceRef.current, groupId, anchorRow, anchorCol, zoomRef.current, x, y);
      if (!hv?.valid) return false;
      const group = surfaceRef.current.groups.find((g) => g.id === groupId);
      if (!group) return false;
      const dRow = hv.rect.row - group.cell.row;
      const dCol = hv.rect.col - group.cell.col;
      if (dRow === 0 && dCol === 0) return true;
      const ns = moveGroup(surfaceRef.current, groupId, dRow, dCol);
      if (ns) {
        onChangeRef.current(ns);
        return true;
      }
      return false;
    },
    [],
  );

  const beginResize = useCallback(
    (groupId: string, e: { clientX: number; clientY: number }) => {
      const group = surfaceRef.current.groups.find((g) => g.id === groupId);
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
        candidateEntry: null,
        candidateGroupId: groupId,
      };
    },
    [],
  );

  useEffect(() => {
    const onDown = (e: PointerEvent) => {
      if (heldRef.current || resizeRef.current) return;
      if ((e.target as Element).closest("[data-no-pickup]")) return;
      const entry = pickableAt(surfaceRef.current, e.clientX, e.clientY);
      if (!entry) return;
      document.body.classList.add("dragging");
      downRef.current = {
        x: e.clientX,
        y: e.clientY,
        candidateEntry: entry,
        candidateGroupId: null,
      };
    };

    const onMove = (e: PointerEvent) => {
      pointerRef.current = { x: e.clientX, y: e.clientY };

      // Handle group resize
      const resize = resizeRef.current;
      if (resize) {
        const z = zoomRef.current;
        const groupPx = CELL_W * GROUP_SNAP;
        const groupPy = CELL_H * GROUP_SNAP;
        const desired = {
          cols:
            resize.startSize.cols +
            Math.round((e.clientX - resize.sx) / (groupPx * z)) * GROUP_SNAP,
          rows:
            resize.startSize.rows +
            Math.round((e.clientY - resize.sy) / (groupPy * z)) * GROUP_SNAP,
        };
        const clamped = { cols: Math.max(GROUP_SNAP, desired.cols), rows: Math.max(GROUP_SNAP, desired.rows) };
        const cur = surfaceRef.current;
        const ns = resizeGroup(cur, resize.groupId, clamped);
        if (ns) {
          onChangeRef.current(ns);
          if (resizeRejected) setResizeRejected(null);
        } else {
          const group = cur.groups.find((g) => g.id === resize.groupId);
          if (group) {
            const colsChanged = clamped.cols !== group.size.cols;
            const rowsChanged = clamped.rows !== group.size.rows;
            if (colsChanged || rowsChanged) {
              const rightBlocked = colsChanged && !resizeGroup(cur, resize.groupId, { cols: clamped.cols, rows: group.size.rows });
              const bottomBlocked = rowsChanged && !resizeGroup(cur, resize.groupId, { cols: group.size.cols, rows: clamped.rows });
              setResizeRejected({ right: !!rightBlocked, bottom: !!bottomBlocked });
              clearTimeout(rejectTimer.current);
              rejectTimer.current = setTimeout(() => setResizeRejected(null), 400);
            }
          }
        }
        return;
      }

      const h = heldRef.current;
      if (h) {
        if (h.kind === "entry") {
          setHover(
            hoverFor(
              surfaceRef.current,
              h.entry,
              zoomRef.current,
              e.clientX,
              e.clientY,
            ),
          );
        } else if (h.kind === "group") {
          setHover(
            hoverForGroup(
              surfaceRef.current,
              h.groupId,
              h.anchorRow,
              h.anchorCol,
              zoomRef.current,
              e.clientX,
              e.clientY,
            ),
          );
        }
        return;
      }

      const down = downRef.current;
      if (!down) return;
      const moved = Math.hypot(e.clientX - down.x, e.clientY - down.y);
      if (moved >= CLICK_SLOP) {
        if (down.candidateEntry) {
          startHoldEntry(down.candidateEntry, e.clientX, e.clientY);
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

      if (h.kind === "entry") {
        tryPlaceEntry(h.entry, e.clientX, e.clientY);
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
      }
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
    startHoldEntry,
    startHoldGroup,
    tryPlaceEntry,
    tryPlaceGroup,
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
          if (held.kind === "entry") {
            setHover(
              hoverFor(surfaceRef.current, held.entry, zoomRef.current, p.x, p.y),
            );
          } else if (held.kind === "group") {
            setHover(
              hoverForGroup(surfaceRef.current, held.groupId, held.anchorRow, held.anchorCol, zoomRef.current, p.x, p.y),
            );
          }
        }
      }
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [held, panBy]);

  const handleLinkClick = useCallback((partnerId: string) => {
    const vp = document.getElementById("panzoom-viewport");
    const target = vp?.querySelector(`[data-pickup="${partnerId}"]`) as HTMLElement | null;
    if (!vp || !target) return;
    const vr = vp.getBoundingClientRect();
    const tr = target.getBoundingClientRect();
    panBy(
      vr.left + vr.width / 2 - (tr.left + tr.width / 2),
      vr.top + vr.height / 2 - (tr.top + tr.height / 2),
    );
  }, [panBy]);

  // Everything visible on this surface — own entries + perimeter context.
  // Incoming-link pills filter to this scope so deeper-down sources don't
  // bleed through (e.g. a container's link to an external shouldn't show as
  // an incoming pill on that external at the system context level).
  const visibleScope = new Set<string>([
    ...surface.entries.map((e) => e.id),
    ...context.persons.map((p) => p.entry.id),
    ...context.externals.map((x) => x.entry.id),
    ...context.refs.map((r) => r.entry.id),
  ]);

  const heldEntry = held?.kind === "entry" ? held.entry : null;
  const heldSpan = heldEntry ? cardSpan(heldEntry) : null;
  const boardHighlight = hover && held
    ? { rect: hover.rect, valid: hover.valid, inset: held.kind === "entry" }
    : null;

  const heldId =
    held?.kind === "entry"
      ? held.entry.id
      : held?.kind === "group"
        ? held.groupId
        : null;

  // Level metadata for ring rendering — outer-most first. Each level carries
  // its altitude (so PackBox can place context entries by origin altitude)
  // and a human label.
  const levels = [
    ...ancestorAltitudes.map((a) => ({ altitude: a, label: ALTITUDE_LABEL[a] })),
    { altitude: surface.altitude, label: ALTITUDE_LABEL[surface.altitude] },
  ];

  return (
    <GridContext.Provider value={{ beginResize, beginGroupDrag, heldId, resizingId, resizeRejected, hover }}>
      <VisibleScopeContext.Provider value={visibleScope}>
      <div style={{ margin: 80 * zoom, position: "relative" }}>
        <PackBox
          surface={surface}
          highlight={boardHighlight}
          levels={levels}
          context={context}
          onRingClick={onBack}
          renderEntry={(entry) => (
            <EntryCard entry={entry} onNavigate={onNavigate} onLinkClick={handleLinkClick} />
          )}
        />
      </div>

      {/* Held entry clone — snapped to the highlight rect so the silhouette
          always overlays the drop indicator (no cursor-vs-snap desync). The
          clone uses screen-pixel sizes (logical × zoom) and EntryCardView
          reads zoom from context, so the clone is crisp at every zoom. */}
      {heldEntry &&
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
                left: gr.left + (hover.rect.col * CELL_W + ITEM_INSET) * zoom,
                top: gr.top + (hover.rect.row * CELL_H + ITEM_INSET) * zoom,
                width: (heldSpan.w * CELL_W - ITEM_INSET * 2) * zoom,
                height: (heldSpan.h * CELL_H - ITEM_INSET * 2) * zoom,
                pointerEvents: "none",
                opacity: 0.85,
                zIndex: 1000,
              }}
            >
              <EntryCardView entry={heldEntry} span={heldSpan} lifted />
            </div>,
            document.body,
          );
        })()}

      {/* Held group clone — snapped to the highlight rect for the same reason */}
      {held?.kind === "group" && hover && (() => {
        const g = surface.groups.find((x) => x.id === held.groupId);
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
                backgroundColor: "color-mix(in srgb, black 3%, transparent)",
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
                style={{ fontSize: 10 * zoom, letterSpacing: 0.12 * zoom + "em" }}
              >
                {g.name}
              </span>
            </div>
          </div>,
          document.body,
        );
      })()}
      </VisibleScopeContext.Provider>
    </GridContext.Provider>
  );
}
