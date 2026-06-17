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
  /** Code tier: source/target dot radius (px), so the line insets to each rim. */
  sourceR?: number;
  targetR?: number;
  /** Incident to the selected node — kept lit while the rest of the graph dims. */
  highlighted?: boolean;
  /** A selection exists elsewhere — this edge isn't part of it, so it fades. */
  dimmed?: boolean;
}

/** Gap between a dot's rim and where the line/arrow starts, so it kisses the
 *  rim instead of overlapping the disc. Added to each end's own radius. */
const DOT_GAP = 3;
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

  // For dot-tier edges the endpoints are dot centers; pull each end in by its
  // own dot radius so the line starts/ends at the rim and the arrow lands on it.
  const dx = targetX - sourceX, dy = targetY - sourceY;
  const len = Math.hypot(dx, dy) || 1;
  const ux = dx / len, uy = dy / len;
  const sInset = data?.dot ? Math.min((data.sourceR ?? 6) + DOT_GAP, len / 2 - 1) : 0;
  const tInset = data?.dot ? Math.min((data.targetR ?? 6) + DOT_GAP, len / 2 - 1) : 0;
  const sx = sourceX + ux * sInset, sy = sourceY + uy * sInset;
  const tx = targetX - ux * tInset, ty = targetY - uy * tInset;

  const isDot = !!data?.dot;
  // Dot tier: bow the edge into a quadratic arc (gitnexus's curved edges) so
  // crossing/parallel spokes separate instead of stacking as straight lines. A
  // consistent perpendicular offset makes A→B and B→A bow to opposite sides.
  const CURVE = 0.07; // arc height as a fraction of the chord — a gentle bow,
  // just enough to split A→B / B→A so cycles stay visible without reading busy
  const cx = (sx + tx) / 2 - (ty - sy) * CURVE;
  const cy = (sy + ty) / 2 + (tx - sx) * CURVE;
  const edgePath = isDot
    ? `M ${sx} ${sy} Q ${cx} ${cy} ${tx} ${ty}`
    : `M ${sx} ${sy} L ${tx} ${ty}`;
  // Quadratic midpoint (t=0.5) for the label/hover dot; end tangent for the arrow.
  const labelX = isDot ? 0.25 * sx + 0.5 * cx + 0.25 * tx : (sx + tx) / 2;
  const labelY = isDot ? 0.25 * sy + 0.5 * cy + 0.25 * ty : (sy + ty) / 2;
  const arrowAngle = isDot
    ? Math.atan2(ty - cy, tx - cx)
    : Math.atan2(ty - sy, tx - sx);

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

  // Subgraph highlight: edges touching the selection stay lit, the rest fade.
  // Dot-tier edges sit fainter at rest so the constellation reads as nodes, not lines.
  const edgeOpacity = selected || data?.highlighted ? 1 : data?.dimmed ? 0.18 : isDot ? 0.5 : 0.7;

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
              strokeWidth: selected ? 2.5 : isDot ? 1 : 1.5,
              // Dot tier: thin, solid, static curves (sigma-style). Arch tier keeps
              // the animated dashed C4 line.
              strokeDasharray: isDot ? undefined : "6 3",
              animation: isDot ? undefined : "dash-flow 0.5s linear infinite",
            }}
          />
        </g>
        <polygon points={`${tx},${ty} ${ax1},${ay1} ${ax2},${ay2}`} fill={edgeColor} />
      </g>
      {/* Midpoint dot — visible on edge hover. */}
      <circle cx={labelX} cy={labelY} r={4} fill={edgeColor} className="edge-handle-dot" />
      {(label || method) && (!isDot || selected || data?.highlighted) && (
        <EdgeLabelRenderer>
          <div
            style={{
              position: "absolute",
              transform: `translate(-50%, -50%) translate(${labelX}px,${labelY}px)`,
              zIndex: 1,
              pointerEvents: "all",
              ...(data?.dimmed ? { opacity: 0.18 } : {}),
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
