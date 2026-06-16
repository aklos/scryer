/**
 * The relationship edge for the diagram — a faithful port of the pre-pivot
 * canvas edge (`RelationshipEdge`), adapted to the read-only v0.3 diagram.
 *
 * Kept from main: the animated dashed line, the clipped arrowhead, the
 * label/method pill, and the hover midpoint dot. Dropped: orthogonal route
 * geometry, bidirectional pair-shift, mention/dim/highlight states, and the old
 * endpoint-status coloring — the change-mark palette lives on the nodes now, so
 * edges read as neutral chrome.
 */

import {
  BaseEdge,
  EdgeLabelRenderer,
  type EdgeProps,
  type Edge as RFEdge,
} from "@xyflow/react";
import { getThemedHex } from "../theme";

export interface EdgeData extends Record<string, unknown> {
  label?: string;
  method?: string;
  /** Code tier: endpoints are dot centers — inset to the dot rim. */
  dot?: boolean;
}

/** Dot radius (h-3 w-3 = 12px) plus a small gap, so the line/arrow kisses the
 *  rim instead of overlapping the disc. */
const DOT_INSET = 9;
export type RFRelEdge = RFEdge<EdgeData, "rel">;

export function RelationshipEdge({
  id,
  sourceX,
  sourceY,
  targetX,
  targetY,
  data,
  selected,
}: EdgeProps<RFRelEdge>) {
  const label = data?.label;
  const method = data?.method;
  // Neutral chrome — the diff/change-mark palette is carried by the nodes.
  const baseColor = getThemedHex("slate", "400");
  const selColor =
    getComputedStyle(document.documentElement).getPropertyValue("--selection-color").trim() ||
    "#18181b";
  const edgeColor = selected ? selColor : baseColor;

  // For dot-tier edges the endpoints are dot centers; pull both in by the dot
  // radius so the line starts/ends at the rim and the arrow lands on it.
  const dx = targetX - sourceX, dy = targetY - sourceY;
  const len = Math.hypot(dx, dy) || 1;
  const inset = data?.dot ? Math.min(DOT_INSET, len / 2 - 1) : 0;
  const ux = dx / len, uy = dy / len;
  const sx = sourceX + ux * inset, sy = sourceY + uy * inset;
  const tx = targetX - ux * inset, ty = targetY - uy * inset;
  const edgePath = `M ${sx} ${sy} L ${tx} ${ty}`;
  const labelX = (sx + tx) / 2;
  const labelY = (sy + ty) / 2;
  const arrowAngle = Math.atan2(ty - sy, tx - sx);

  // Arrowhead polygon from explicit geometry.
  const arrowSize = 8;
  const ax1 = tx - arrowSize * Math.cos(arrowAngle - Math.PI / 6);
  const ay1 = ty - arrowSize * Math.sin(arrowAngle - Math.PI / 6);
  const ax2 = tx - arrowSize * Math.cos(arrowAngle + Math.PI / 6);
  const ay2 = ty - arrowSize * Math.sin(arrowAngle + Math.PI / 6);

  // Clip the dashed line where the arrowhead begins so they don't overlap.
  const clipId = `clip-${id}`;
  const perpXDir = -Math.sin(arrowAngle);
  const perpYDir = Math.cos(arrowAngle);
  const bx = (ax1 + ax2) / 2;
  const by = (ay1 + ay2) / 2;
  const far = 10000;

  const edgeOpacity = selected ? 1 : 0.7;

  return (
    <>
      <defs>
        <clipPath id={clipId}>
          <path
            d={`M ${bx + perpXDir * far} ${by + perpYDir * far} L ${bx - perpXDir * far} ${by - perpYDir * far} L ${bx - perpXDir * far - Math.cos(arrowAngle) * far} ${by - perpYDir * far - Math.sin(arrowAngle) * far} L ${bx + perpXDir * far - Math.cos(arrowAngle) * far} ${by + perpYDir * far - Math.sin(arrowAngle) * far} Z`}
          />
        </clipPath>
      </defs>
      {/* Wider invisible hit area for easier clicking. */}
      <path d={edgePath} fill="none" stroke="transparent" strokeWidth={20} className="react-flow__edge-interaction" />
      <g opacity={edgeOpacity}>
        <g clipPath={`url(#${clipId})`}>
          <BaseEdge
            id={id}
            path={edgePath}
            style={{
              stroke: edgeColor,
              strokeWidth: selected ? 2.5 : 1.5,
              strokeDasharray: "6 3",
              animation: "dash-flow 0.5s linear infinite",
            }}
          />
        </g>
        <polygon points={`${tx},${ty} ${ax1},${ay1} ${ax2},${ay2}`} fill={edgeColor} />
      </g>
      {/* Midpoint dot — visible on edge hover. */}
      <circle cx={labelX} cy={labelY} r={4} fill={edgeColor} className="edge-handle-dot" />
      {(label || method) && (
        <EdgeLabelRenderer>
          <div
            style={{
              position: "absolute",
              transform: `translate(-50%, -50%) translate(${labelX}px,${labelY}px)`,
              zIndex: 1,
              pointerEvents: "all",
            }}
            className="flex flex-col items-center"
          >
            {label && (
              <div className="whitespace-nowrap rounded bg-[var(--surface-active)]/80 px-1.5 py-0.5 text-[10px] text-[var(--text)]">
                {label}
              </div>
            )}
            {method && (
              <div className="whitespace-nowrap text-[9px] text-[var(--text-tertiary)]">[{method}]</div>
            )}
          </div>
        </EdgeLabelRenderer>
      )}
    </>
  );
}
