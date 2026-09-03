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
  /** A reverse edge runs between the same two nodes — offset to a parallel lane
   *  so the two straight chords don't overlap. */
  parallel?: boolean;
  /** Bow the edge outward from a centre (styled rings: a same-ring chord
   *  would otherwise cut through the layers inside). */
  bow?: { cx: number; cy: number; offset: number };
  /** The style's layer matrix forbids this dependency — drawn red so the map
   *  exposes the code's disorder instead of tidying it away. The string is
   *  the reason, shown on hover and beside the line when selected. */
  violation?: string;
}

/** Gap between a dot's rim and where the line/arrow starts, so it kisses the
 *  rim instead of overlapping the disc. Added to each end's own radius. */
const DOT_GAP = 3;
/** Sideways shift (px) for an edge that shares a node pair with its reverse, so
 *  the two land as separate parallel lanes instead of on top of each other. */
const PARALLEL_GAP = 5;
/** Perpendicular nudge (px) for a parallel edge's label, pushing the pill to its
 *  lane's outer side so it hugs its own line instead of straddling both lanes. */
const PARALLEL_LABEL_GAP = 12;
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
  const baseColor = data?.violation ? getThemedHex("red", "500") : getThemedHex("slate", "400");
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
  // A reverse edge between the same two nodes would land exactly on top of this
  // one now that the line is straight — shift the whole chord sideways by its
  // perpendicular. The perpendicular flips with direction, so A→B and B→A push
  // to opposite sides and read as two parallel lanes.
  const off = data?.parallel ? PARALLEL_GAP : 0;
  const ox = -uy * off, oy = ux * off;
  const sx = sourceX + ux * sInset + ox, sy = sourceY + uy * sInset + oy;
  const tx = targetX - ux * tInset + ox, ty = targetY - uy * tInset + oy;

  const isDot = !!data?.dot;
  // Straight chord rim-to-rim by default. A bowed edge (styled rings) is a
  // quadratic curve whose control point sits past the chord's midpoint, away
  // from the drawing's centre.
  let cxp = (sx + tx) / 2, cyp = (sy + ty) / 2;
  if (data?.bow) {
    const mx = cxp, my = cyp;
    let vx = mx - data.bow.cx, vy = my - data.bow.cy;
    let vl = Math.hypot(vx, vy);
    if (vl < 8) {
      // Antipodal ends: the chord runs through the centre, so bow to the
      // chord's right-hand side instead.
      vx = -uy; vy = ux; vl = 1;
    }
    // A quadratic's midpoint sits halfway to its control point, so doubling
    // displaces the curve's middle exactly `offset` px from the chord.
    cxp = mx + (vx / vl) * data.bow.offset * 2;
    cyp = my + (vy / vl) * data.bow.offset * 2;
  }
  const edgePath = data?.bow ? `M ${sx} ${sy} Q ${cxp} ${cyp} ${tx} ${ty}` : `M ${sx} ${sy} L ${tx} ${ty}`;
  // Hover handle sits at the lane midpoint (the curve's midpoint when bowed).
  const labelX = data?.bow ? 0.25 * sx + 0.5 * cxp + 0.25 * tx : (sx + tx) / 2;
  const labelY = data?.bow ? 0.25 * sy + 0.5 * cyp + 0.25 * ty : (sy + ty) / 2;
  // Parallel (reverse-pair) edges share one chord, so a midpoint pill can't be
  // told apart from its twin. Place each label by (a) sliding it along the chord
  // toward its own source — the pair lands at ~35%/65% and clears ALONG the line
  // at any angle — and (b) nudging it to its lane's OUTER side so the pill hugs
  // its own line. Both flip with direction, so the pair reads as opposite lanes.
  let labelDivX = labelX, labelDivY = labelY;
  if (data?.parallel) {
    const t = 0.35;
    const px = sx + (tx - sx) * t, py = sy + (ty - sy) * t;
    labelDivX = px + -uy * PARALLEL_LABEL_GAP;
    labelDivY = py + ux * PARALLEL_LABEL_GAP;
  }
  const arrowAngle = data?.bow ? Math.atan2(ty - cyp, tx - cxp) : Math.atan2(ty - sy, tx - sx);

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
  const edgeOpacity =
    selected || data?.highlighted ? 1 : data?.dimmed ? 0.18 : data?.violation ? 0.95 : isDot ? 0.5 : 0.7;

  return (
    <>
      <defs>
        <clipPath id={clipId}>
          <path
            d={`M ${bx + perpXDir * far} ${by + perpYDir * far} L ${bx - perpXDir * far} ${by - perpYDir * far} L ${bx - perpXDir * far - Math.cos(arrowAngle) * far} ${by - perpYDir * far - Math.sin(arrowAngle) * far} L ${bx + perpXDir * far - Math.cos(arrowAngle) * far} ${by + perpYDir * far - Math.sin(arrowAngle) * far} Z`}
          />
        </clipPath>
      </defs>
      {/* Wider invisible hit area for easier clicking — and the hover tooltip
          carrying a violation's reason. */}
      <path d={edgePath} fill="none" stroke="transparent" strokeWidth={20} className="react-flow__edge-interaction">
        {data?.violation && <title>{data.violation}</title>}
      </path>
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
      {data?.violation && (selected || data?.highlighted) && (
        <EdgeLabelRenderer>
          <div
            style={{
              position: "absolute",
              transform: `translate(-50%, -50%) translate(${labelDivX}px,${labelDivY}px)`,
              zIndex: 2,
              pointerEvents: "all",
            }}
            className="max-w-[260px] rounded border border-red-500/40 bg-[var(--surface)] px-2 py-1 text-[10px] leading-snug text-[var(--text)]"
          >
            {data.violation}
          </div>
        </EdgeLabelRenderer>
      )}
      {(label || method) && (!isDot || selected || data?.highlighted) && (
        <EdgeLabelRenderer>
          <div
            style={{
              position: "absolute",
              transform: `translate(-50%, -50%) translate(${labelDivX}px,${labelDivY}px)`,
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
