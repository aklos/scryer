/**
 * The drawing behind a styled level — the bands, rings, columns or hexagon
 * that tell a first-time reader what kind of thing they are looking at
 * before they read a card. Rendered inside React Flow's viewport (so it pans
 * and zooms with the cards) and painted underneath them.
 */

import { useEffect, useState } from "react";
import { ViewportPortal } from "@xyflow/react";
import { Info } from "lucide-react";
import type { LayerRegion } from "./styled";
import { LAYER_NOTE, clickedOff, toggleOpen } from "./edgePanel";


function hexPoints(cx: number, cy: number, r: number): string {
  // Flat-topped hexagon, circumradius r.
  const pts: string[] = [];
  for (let i = 0; i < 6; i++) {
    const a = (Math.PI / 3) * i;
    pts.push(`${(cx + r * Math.cos(a)).toFixed(1)},${(cy + r * Math.sin(a)).toFixed(1)}`);
  }
  return pts.join(" ");
}

function bounds(regions: LayerRegion[]): { x: number; y: number; w: number; h: number } {
  let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
  for (const r of regions) {
    if (r.shape === "rect") {
      x0 = Math.min(x0, r.x); y0 = Math.min(y0, r.y);
      x1 = Math.max(x1, r.x + r.w); y1 = Math.max(y1, r.y + r.h);
    } else {
      x0 = Math.min(x0, r.cx - r.r); y0 = Math.min(y0, r.cy - r.r);
      x1 = Math.max(x1, r.cx + r.r); y1 = Math.max(y1, r.cy + r.r);
    }
  }
  if (!Number.isFinite(x0)) return { x: 0, y: 0, w: 0, h: 0 };
  const pad = 40;
  return { x: x0 - pad, y: y0 - pad, w: x1 - x0 + 2 * pad, h: y1 - y0 + 2 * pad };
}

/** The frame that holds the whole drawing — the cards' bounds widened to
 *  every region behind them, label room included — so fitting the view
 *  never cuts a ring or a band off at the edge. */
export function drawingBounds(
  cards: { x: number; y: number; width: number; height: number },
  regions: LayerRegion[],
): { x: number; y: number; width: number; height: number } {
  if (regions.length === 0) return cards;
  const r = bounds(regions);
  const x = Math.min(cards.x, r.x), y = Math.min(cards.y, r.y);
  return {
    x,
    y,
    width: Math.max(cards.x + cards.width, r.x + r.w) - x,
    height: Math.max(cards.y + cards.height, r.y + r.h) - y,
  };
}

/** Where a region's label sits (flow coordinates: the left end, or the
 *  middle, of its first line) and how it reads. */
interface LabelSpot {
  x: number;
  y: number;
  anchor: "middle" | "start";
  size: number;
  boundary: boolean;
}

function labelSpots(regions: LayerRegion[]): LabelSpot[] {
  let hexSeen = 0;
  let innerR = 0;
  return regions.map((r) => {
    if (r.shape === "rect")
      return r.boundary
        ? { x: r.x + 14, y: r.y + 8, anchor: "start", size: 13, boundary: true }
        : { x: r.x + 14, y: r.y + 7, anchor: "start", size: 10, boundary: false };
    if (r.shape === "ring") return { x: r.cx, y: r.cy - r.r + 7, anchor: "middle", size: 10, boundary: false };
    // Hexagons: the ring cards sit on the top and bottom edges, so the
    // innermost label goes to its bottom edge (below the centre card) and
    // outer labels to the left vertex, reading inward. A ring with no cards
    // is a thin band with no room at its vertex: its label lies along its
    // bottom edge too. A bottom label starts just right of the vertical
    // axis, where the links from cards above and below the hexagon run.
    const inner = hexSeen === 0;
    const thin = !inner && r.r - innerR < 80;
    hexSeen++;
    if (inner) innerR = r.r;
    return inner || thin
      ? { x: r.cx + 14, y: r.cy + r.r * 0.866 - 29, anchor: "start", size: 10, boundary: false }
      : { x: r.cx - r.r * 0.92, y: r.cy - 15, anchor: "start", size: 10, boundary: false };
  });
}

/**
 * The regions' labels, each with an info button that opens the layer's
 * description beneath it. Drawn above the cards and edges, so nothing runs
 * over a label or an open description, and a click reaches the button.
 */
function RegionLabels({ regions }: { regions: LayerRegion[] }) {
  // Which layer has its description open. Nothing opens on hover: the
  // button is a button, a new drawing starts closed, and a press anywhere off
  // the description closes it.
  const [open, setOpen] = useState<ReadonlySet<number>>(new Set());
  useEffect(() => setOpen(new Set()), [regions]);
  useEffect(() => {
    if (open.size === 0) return;
    const off = (e: PointerEvent) => {
      if (clickedOff(e.target instanceof Element ? e.target : null)) setOpen(new Set());
    };
    document.addEventListener("pointerdown", off, true);
    return () => document.removeEventListener("pointerdown", off, true);
  }, [open]);
  const spots = labelSpots(regions);
  return (
    <ViewportPortal>
      {regions.map((r, i) => {
        const at = spots[i];
        const isOpen = open.has(i);
        const shift = at.anchor === "middle" ? "-50%" : "0";
        return (
          <div
            key={i}
            className="pointer-events-none absolute left-0 top-0 flex flex-col"
            style={{
              transform: `translate(${at.x}px, ${at.y}px) translateX(${shift})`,
              alignItems: at.anchor === "middle" ? "center" : "flex-start",
              zIndex: 4,
            }}
          >
            <div
              className={`flex items-center gap-1 whitespace-nowrap font-semibold leading-none ${
                at.boundary ? "text-[var(--text-tertiary)]" : "uppercase text-[var(--map-region-label)]"
              }`}
              style={{ fontSize: at.size, letterSpacing: at.boundary ? 0.2 : 1.5 }}
            >
              {r.layer}
              {r.caption && (
                <button
                  type="button"
                  {...{ [LAYER_NOTE]: "" }}
                  aria-label={`${r.layer} layer description`}
                  aria-expanded={isOpen}
                  className={`nodrag nopan pointer-events-auto flex shrink-0 items-center rounded-full p-0.5 transition-colors hover:text-[var(--text-secondary)] ${
                    isOpen ? "text-[var(--text-secondary)]" : ""
                  }`}
                  onClick={(e) => {
                    e.stopPropagation();
                    setOpen((prev) => toggleOpen(prev, i));
                  }}
                >
                  <Info className="h-3 w-3" strokeWidth={2} />
                </button>
              )}
            </div>
            {isOpen && r.caption && (
              <div
                {...{ [LAYER_NOTE]: "" }}
                onClick={(e) => e.stopPropagation()}
                className="nodrag nopan pointer-events-auto mt-1.5 w-max max-w-[260px] rounded border border-[var(--border)] bg-[var(--surface)] px-2 py-1.5 text-[11px] leading-snug text-[var(--text-secondary)] shadow-lg"
              >
                {r.caption}
              </div>
            )}
          </div>
        );
      })}
    </ViewportPortal>
  );
}

export function StyleRegions({ regions }: { regions: LayerRegion[] }) {
  if (regions.length === 0) return null;
  const b = bounds(regions);
  return (
    <>
      <ViewportPortal>
        <div
          className="pointer-events-none absolute"
          style={{ transform: `translate(${b.x}px, ${b.y}px)`, width: b.w, height: b.h, zIndex: -1 }}
        >
          <svg width={b.w} height={b.h} viewBox={`${b.x} ${b.y} ${b.w} ${b.h}`} className="overflow-visible">
            {regions.map((r, i) => {
              // A ghost band is open: no fill, a dotted edge — the world beyond
              // this level, not a layer of it. A boundary is the C4 frame round
              // the level's own node: no fill, a solid line in the map's ink —
              // dashes belong to edges and the world outside.
              const common = r.boundary
                ? { fill: "none", stroke: "var(--map-boundary)", strokeWidth: 1 }
                : r.ghost
                ? { fill: "none", stroke: "var(--map-region-line)", strokeWidth: 1, strokeDasharray: "2 6" }
                : {
                    fill: "var(--map-region)",
                    stroke: "var(--map-region-line)",
                    strokeWidth: 1,
                    strokeDasharray: "4 4",
                  };
              if (r.shape === "rect") return <rect key={i} x={r.x} y={r.y} width={r.w} height={r.h} rx={12} {...common} />;
              if (r.shape === "ring") return <circle key={i} cx={r.cx} cy={r.cy} r={r.r} {...common} />;
              return <polygon key={i} points={hexPoints(r.cx, r.cy, r.r)} {...common} strokeDasharray={undefined} />;
            })}
          </svg>
        </div>
      </ViewportPortal>
      <RegionLabels regions={regions} />
    </>
  );
}
