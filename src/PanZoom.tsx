/**
 * Hand-rolled continuous pan/zoom viewport.
 *
 * An inner container gets `transform: translate() scale()`. The wheel zooms
 * (anchored at the cursor); a drag on the *empty background* pans — a press
 * that lands on an item (a card, a group header, a resize handle) is left for
 * that item's own drag, so pressing a card moves the card, not the canvas.
 *
 * `ZoomContext` exposes the current zoom (so consumers can correct pointer
 * deltas); `PanContext` exposes `panBy`, used to edge-pan while carrying an
 * item toward the viewport border.
 */

import {
  createContext,
  useCallback,
  useContext,
  useLayoutEffect,
  useRef,
  useState,
} from "react";
import type { ReactNode, WheelEvent, PointerEvent } from "react";
import { CELL_W, CELL_H, GROUP_SNAP_W } from "./pack";

const MIN_ZOOM = 0.3;
const MAX_ZOOM = 2.5;

/** Elements whose press should move the element, not pan the canvas. */
const ITEM_SELECTOR = "[data-pickup],[data-group-pickup],[data-resize],[data-no-pickup]";

/** Current viewport zoom factor. 1 = unscaled. */
export const ZoomContext = createContext(1);
export const useZoom = () => useContext(ZoomContext);

/** Imperative pan — shift the view by a screen-pixel delta. */
export const PanContext = createContext<(dx: number, dy: number) => void>(
  () => {},
);
export const usePan = () => useContext(PanContext);

interface ViewState {
  x: number;
  y: number;
  zoom: number;
}

const INITIAL: ViewState = { x: 0, y: 0, zoom: 1 };

function wrapMod(v: number, m: number) {
  return m > 0 ? ((v % m) + m) % m : 0;
}

export function PanZoom({
  children,
  /** Bump this to reset pan/zoom (e.g. on surface navigation). */
  resetKey,
}: {
  children: ReactNode;
  resetKey?: string | number;
}) {
  const [view, setView] = useState<ViewState>(INITIAL);
  const containerRef = useRef<HTMLDivElement>(null);
  const transformRef = useRef<HTMLDivElement>(null);
  const lastResetKey = useRef(resetKey);
  const panState = useRef<{ x: number; y: number; ox: number; oy: number } | null>(
    null,
  );
  const [gridOrigin, setGridOrigin] = useState({ x: 0, y: 0 });

  // Reset pan/zoom when the surface changes.
  if (resetKey !== lastResetKey.current) {
    lastResetKey.current = resetKey;
    // setState during render is fine here — it's a sync reset, like getDerivedState.
    setView(INITIAL);
  }

  // Measure the screen-pixel offset from the transform origin to [data-grid].
  // No /zoom — there's no transform: scale anymore, so gridEl's rect is
  // already in screen pixels relative to the (translated) transformRef.
  useLayoutEffect(() => {
    const transform = transformRef.current;
    const viewport = containerRef.current;
    if (!transform || !viewport) return;
    const gridEl = viewport.querySelector("[data-grid]") as HTMLElement | null;
    if (!gridEl) return;
    const tr = transform.getBoundingClientRect();
    const gr = gridEl.getBoundingClientRect();
    const x = gr.left - tr.left;
    const y = gr.top - tr.top;
    setGridOrigin((prev) => {
      if (Math.abs(prev.x - x) < 0.5 && Math.abs(prev.y - y) < 0.5) return prev;
      return { x, y };
    });
  });

  const panBy = useCallback((dx: number, dy: number) => {
    setView((v) => ({ ...v, x: v.x + dx, y: v.y + dy }));
  }, []);

  const onWheel = useCallback((e: WheelEvent<HTMLDivElement>) => {
    e.preventDefault();
    const rect = containerRef.current?.getBoundingClientRect();
    if (!rect) return;
    // Cursor position relative to the viewport.
    const px = e.clientX - rect.left;
    const py = e.clientY - rect.top;
    setView((v) => {
      const factor = Math.exp(-e.deltaY * 0.0015);
      const next = Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, v.zoom * factor));
      const ratio = next / v.zoom;
      // Keep the point under the cursor fixed while scaling.
      return {
        zoom: next,
        x: px - (px - v.x) * ratio,
        y: py - (py - v.y) * ratio,
      };
    });
  }, []);

  const onPointerDown = useCallback(
    (e: PointerEvent<HTMLDivElement>) => {
      if (e.button !== 0) return;
      // A press on an item is that item's drag — never a pan.
      if ((e.target as Element).closest(ITEM_SELECTOR)) return;
      panState.current = { x: e.clientX, y: e.clientY, ox: view.x, oy: view.y };
      (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    },
    [view.x, view.y],
  );

  const onPointerMove = useCallback((e: PointerEvent<HTMLDivElement>) => {
    const p = panState.current;
    if (!p) return;
    setView((v) => ({
      ...v,
      x: p.ox + (e.clientX - p.x),
      y: p.oy + (e.clientY - p.y),
    }));
  }, []);

  const endPan = useCallback((e: PointerEvent<HTMLDivElement>) => {
    if (panState.current) {
      panState.current = null;
      try {
        (e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
      } catch {
        /* pointer may already be released */
      }
    }
  }, []);

  // Grid geometry — viewport-level SVG keeps lines at constant screen-pixel width.
  // gridOrigin is already screen-pixel offset within transformRef; the actual
  // grid cell on screen is CELL_W * zoom because PackBox scales its template.
  const cellPx = CELL_H * view.zoom;
  const majorPx = CELL_W * GROUP_SNAP_W * view.zoom;
  const sOx = view.x + gridOrigin.x;
  const sOy = view.y + gridOrigin.y;
  const gxMin = wrapMod(sOx, cellPx);
  const gyMin = wrapMod(sOy, cellPx);
  const gxMaj = wrapMod(sOx, majorPx);
  const gyMaj = wrapMod(sOy, majorPx);

  // Fade minor lines when zoomed out far enough that cells are < 40px on screen.
  const minorOpacity = cellPx < 30 ? 0 : cellPx < 50 ? (cellPx - 30) / 20 : 1;

  return (
    <div
      id="panzoom-viewport"
      ref={containerRef}
      onWheel={onWheel}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={endPan}
      onPointerCancel={endPan}
      className="relative h-full w-full overflow-hidden bg-[var(--surface-canvas)] cursor-grab active:cursor-grabbing select-none"
    >
      <svg
        aria-hidden
        className="absolute inset-0 h-full w-full pointer-events-none"
        style={{ contain: "strict" }}
      >
        <defs>
          <pattern
            id="grid-minor"
            width={cellPx}
            height={cellPx}
            patternUnits="userSpaceOnUse"
          >
            <line
              x1={gxMin} y1="0" x2={gxMin} y2={cellPx}
              stroke="var(--grid-line)" strokeWidth="0.5"
            />
            <line
              x1="0" y1={gyMin} x2={cellPx} y2={gyMin}
              stroke="var(--grid-line)" strokeWidth="0.5"
            />
          </pattern>
          <pattern
            id="grid-major"
            width={majorPx}
            height={majorPx}
            patternUnits="userSpaceOnUse"
          >
            <line
              x1={gxMaj} y1="0" x2={gxMaj} y2={majorPx}
              stroke="var(--grid-line-major)" strokeWidth="1"
            />
            <line
              x1="0" y1={gyMaj} x2={majorPx} y2={gyMaj}
              stroke="var(--grid-line-major)" strokeWidth="1"
            />
          </pattern>
        </defs>
        {minorOpacity > 0 && (
          <rect
            width="100%" height="100%"
            fill="url(#grid-minor)"
            opacity={minorOpacity}
          />
        )}
        <rect width="100%" height="100%" fill="url(#grid-major)" />
      </svg>

      <ZoomContext.Provider value={view.zoom}>
        <PanContext.Provider value={panBy}>
          <div
            ref={transformRef}
            style={{
              // Pan via transform translate; zoom is *not* applied here.
              // Consumers (PackBox, EntryCard, perimeter shapes) read zoom
              // from ZoomContext and scale their pixel dimensions inline so
              // text re-renders crisply at every zoom level without reflow.
              transform: `translate(${view.x}px, ${view.y}px)`,
              transformOrigin: "0 0",
            }}
            className="absolute left-0 top-0 origin-top-left"
          >
            {children}
          </div>
        </PanContext.Provider>
      </ZoomContext.Provider>
    </div>
  );
}
