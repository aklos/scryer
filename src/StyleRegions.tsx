/**
 * The drawing behind a styled level — the bands, rings, columns or hexagon
 * that tell a first-time reader what kind of thing they are looking at
 * before they read a card. Rendered inside React Flow's viewport (so it pans
 * and zooms with the cards) and painted underneath them.
 */

import { ViewportPortal } from "@xyflow/react";
import type { LayerRegion } from "./layout/styled";

const HEX_LABEL_INSET = 10;

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

export function StyleRegions({ regions }: { regions: LayerRegion[] }) {
  if (regions.length === 0) return null;
  const b = bounds(regions);
  return (
    <ViewportPortal>
      <div
        className="pointer-events-none absolute"
        style={{ transform: `translate(${b.x}px, ${b.y}px)`, width: b.w, height: b.h, zIndex: -1 }}
      >
        <svg width={b.w} height={b.h} viewBox={`${b.x} ${b.y} ${b.w} ${b.h}`} className="overflow-visible">
          {regions.map((r, i) => {
            const label = r.layer.toUpperCase();
            const common = {
              fill: "var(--surface-tint)",
              fillOpacity: 0.35,
              stroke: "var(--border)",
              strokeWidth: 1,
              strokeDasharray: "4 4",
            };
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
                  </text>
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
                  </text>
                </g>
              );
            }
            return (
              <g key={i}>
                <polygon points={hexPoints(r.cx, r.cy, r.r)} {...common} strokeDasharray={undefined} />
                <text
                  x={r.cx}
                  y={r.cy - (r.r * Math.sin(Math.PI / 3)) + 16 - HEX_LABEL_INSET + 10}
                  textAnchor="middle"
                  fontSize={10}
                  letterSpacing={1.5}
                  fill="var(--text-ghost)"
                  fontWeight={600}
                >
                  {label}
                </text>
              </g>
            );
          })}
        </svg>
      </div>
    </ViewportPortal>
  );
}
