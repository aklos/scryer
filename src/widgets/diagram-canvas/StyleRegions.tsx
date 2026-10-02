/**
 * The drawing behind a styled level — the bands, rings, columns or hexagon
 * that tell a first-time reader what kind of thing they are looking at
 * before they read a card. Rendered inside React Flow's viewport (so it pans
 * and zooms with the cards) and painted underneath them.
 */

import { useEffect, useState } from "react";
import { ViewportPortal } from "@xyflow/react";
import type { LayerRegion } from "./styled";
import { toggleOpen } from "./edgePanel";


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

/** A layer's description, opened by its ⓘ — wraps, unlike SVG text. */
function CaptionPanel({ x, y, anchor, text }: { x: number; y: number; anchor: "middle" | "start"; text: string }) {
  const w = 240;
  return (
    <foreignObject x={anchor === "middle" ? x - w / 2 : x} y={y + 8} width={w} height={120} className="overflow-visible">
      <div
        onClick={(e) => e.stopPropagation()}
        className={`pointer-events-auto rounded border border-[var(--border)] bg-[var(--surface)] px-2 py-1 text-[10px] leading-snug text-[var(--text-secondary)] ${
          anchor === "middle" ? "mx-auto w-fit" : "w-fit"
        }`}
      >
        {text}
      </div>
    </foreignObject>
  );
}

export function StyleRegions({ regions }: { regions: LayerRegion[] }) {
  // Which layers have their description open. Nothing opens on hover: the ⓘ
  // is a button, and a new drawing starts closed.
  const [open, setOpen] = useState<ReadonlySet<number>>(new Set());
  useEffect(() => setOpen(new Set()), [regions]);
  if (regions.length === 0) return null;
  const b = bounds(regions);
  let hexSeen = 0;
  return (
    <ViewportPortal>
      <div
        className="pointer-events-none absolute"
        style={{ transform: `translate(${b.x}px, ${b.y}px)`, width: b.w, height: b.h, zIndex: -1 }}
      >
        <svg width={b.w} height={b.h} viewBox={`${b.x} ${b.y} ${b.w} ${b.h}`} className="overflow-visible">
          {regions.map((r, i) => {
            const label = r.layer.toUpperCase();
            // A ghost band is open: no fill, a dotted edge — the world beyond
            // this level, not a layer of it.
            const common = r.ghost
              ? { fill: "none", stroke: "var(--border-subtle)", strokeWidth: 1, strokeDasharray: "2 6" }
              : {
                  fill: "var(--surface-tint)",
                  fillOpacity: 0.35,
                  stroke: "var(--border)",
                  strokeWidth: 1,
                  strokeDasharray: "4 4",
                };
            // The layer's description opens beside its label on a click of
            // the ⓘ, and closes on a second one.
            const full = r.caption ?? "";
            const isOpen = open.has(i);
            const caption = full ? (
              <tspan
                fill={isOpen ? "var(--text-secondary)" : "var(--text-ghost)"}
                fontWeight={400}
                letterSpacing={0}
                opacity={isOpen ? 1 : 0.7}
                fontSize={11}
                className="pointer-events-auto cursor-pointer"
                role="button"
                aria-label={`${r.layer} layer description`}
                onClick={(e) => {
                  e.stopPropagation();
                  setOpen((prev) => toggleOpen(prev, i));
                }}
              >
                {"  ⓘ"}
              </tspan>
            ) : null;
            const captionBelow = (x: number, y: number, anchor: "middle" | "start") =>
              isOpen && full ? <CaptionPanel x={x} y={y} anchor={anchor} text={full} /> : null;
            if (r.shape === "rect") {
              return (
                <g key={i}>
                  <rect x={r.x} y={r.y} width={r.w} height={r.h} rx={12} {...common} />
                  <text
                    x={r.x + 14}
                    y={r.y + 16}
                    fontSize={10}
                    letterSpacing={1.5}
                    fill="var(--text-ghost)"
                    fontWeight={600}
                  >
                    {label}
                    {caption}
                  </text>
                  {captionBelow(r.x + 14, r.y + 16, "start")}
                </g>
              );
            }
            if (r.shape === "ring") {
              return (
                <g key={i}>
                  <circle cx={r.cx} cy={r.cy} r={r.r} {...common} />
                  <text
                    x={r.cx}
                    y={r.cy - r.r + 16}
                    textAnchor="middle"
                    fontSize={10}
                    letterSpacing={1.5}
                    fill="var(--text-ghost)"
                    fontWeight={600}
                  >
                    {label}
                    {caption}
                  </text>
                  {captionBelow(r.cx, r.cy - r.r + 16, "middle")}
                </g>
              );
            }
            // Hexagons: the ring cards sit on the top and bottom edges, so the
            // innermost label goes to its bottom edge (below the centre card)
            // and outer labels to the left vertex, reading inward.
            const inner = hexSeen++ === 0;
            const lx = inner ? r.cx : r.cx - r.r * 0.92;
            const ly = inner ? r.cy + r.r * 0.866 - 20 : r.cy - 6;
            return (
              <g key={i}>
                <polygon points={hexPoints(r.cx, r.cy, r.r)} {...common} strokeDasharray={undefined} />
                <text
                  x={lx}
                  y={ly}
                  textAnchor={inner ? "middle" : "start"}
                  fontSize={10}
                  letterSpacing={1.5}
                  fill="var(--text-ghost)"
                  fontWeight={600}
                >
                  {label}
                  {caption}
                </text>
                {captionBelow(lx, ly, inner ? "middle" : "start")}
              </g>
            );
          })}
        </svg>
      </div>
    </ViewportPortal>
  );
}
