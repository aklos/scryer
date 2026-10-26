/**
 * The relationship edge for the diagram — a port of the pre-pivot canvas edge:
 * a dashed chord with a clipped arrowhead and a label/method pill. A reverse
 * pair splits into parallel lanes, code-tier edges inset to their dots' rims,
 * same-ring edges in a styled level bow outward, a long box-tier link follows
 * its lane, a quiet edge (one the drawing already implies) sits faint, and
 * edges light or fade with
 * the selection. An edge carrying structural violations is drawn red, its
 * reason always written on it. Edges take no clicks.
 */

import {
  BaseEdge,
  EdgeLabelRenderer,
  type EdgeProps,
  type Edge as RFEdge,
} from "@xyflow/react";
import { getThemedHex } from "../theme/theme";
import { edgeShape } from "./edgeGeometry";

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
  /** Says what the drawing already says (a legal step between a style's
   *  layers, a tooling link): drawn faint, its label only when lit, so the
   *  links that say something new stand out. */
  quiet?: boolean;
  /** A reverse edge runs between the same two nodes — offset to a parallel lane
   *  so the two straight chords don't overlap. */
  parallel?: boolean;
  /** Bow the edge outward from a centre (styled rings: a same-ring chord
   *  would otherwise cut through the layers inside). */
  bow?: { cx: number; cy: number; offset: number };
  /** Turn at these points (flow coordinates) on the way, corners rounded:
   *  out of its port, across a gap, down a lane between the cards it passes. */
  waypoints?: { x: number; y: number }[];
  /** Where along its side each end meets its card, from the side's middle
   *  (px): the edges on one side spread out rather than pile up. */
  sourceShift?: number;
  targetShift?: number;
  /** Where the label pill goes, chosen for the whole level so it hides no
   *  other edge (`placeLabels`). Absent: the line's own middle. */
  labelAt?: { x: number; y: number };
  /** Structural violations charged on this pair — drawn red, the first
   *  reason written on it, so the map exposes the code's disorder instead of
   *  tidying it away. */
  violations?: string[];
}

/** Perpendicular nudge (px) for a parallel edge's label, pushing the pill to its
 *  lane's outer side so it hugs its own line instead of straddling both lanes. */
const PARALLEL_LABEL_GAP = 12;
export type RFRelEdge = RFEdge<EdgeData, "rel">;

/** A label reads in full (its pill) while a selected end lights its edge,
 *  and always on a red edge; otherwise it recedes into faint text behind the
 *  drawing. */
export function labelInFull(data: EdgeData | undefined, selected = false): boolean {
  return !!(selected || data?.highlighted || data?.violations?.length);
}

/** What an edge says on it: a red edge its first reason (and how many more),
 *  any other its label. */
export function edgeText(data: EdgeData | undefined): string | undefined {
  const v = data?.violations ?? [];
  if (v.length) return v.length > 1 ? `${v[0]} (+${v.length - 1} more)` : v[0];
  return data?.label;
}

/** How strongly an edge draws: lit edges (at a selected end) in full; a
 *  quiet one faint and thin with no label; the rest of a selection's level
 *  faded further. */
export function edgeLook(
  data: EdgeData | undefined,
  selected = false,
): { opacity: number; width: number; label: boolean } {
  const lit = !!(selected || data?.highlighted);
  const red = (data?.violations?.length ?? 0) > 0;
  const isDot = !!data?.dot;
  const opacity = lit ? 1 : data?.dimmed ? (data?.quiet ? 0.1 : 0.18) : red ? 0.95 : isDot || data?.quiet ? 0.5 : 0.85;
  const width = selected ? 2.5 : isDot || (data?.quiet && !lit) ? 1 : 1.5;
  return { opacity, width, label: (!isDot && !data?.quiet) || labelInFull(data, selected) };
}

export function RelationshipEdge({
  id,
  sourceX,
  sourceY,
  targetX,
  targetY,
  sourcePosition,
  targetPosition,
  data,
  selected,
}: EdgeProps<RFRelEdge>) {
  const label = edgeText(data);
  const method = data?.method;
  // Neutral chrome — the diff/change-mark palette is carried by the nodes.
  const violations = data?.violations ?? [];
  const red = violations.length > 0;
  const baseColor = red ? getThemedHex("red", "500") : "var(--map-edge)";
  const selColor =
    getComputedStyle(document.documentElement).getPropertyValue("--selection-color").trim() ||
    "#18181b";
  const edgeColor = selected ? selColor : baseColor;
  const full = labelInFull(data, selected);

  const isDot = !!data?.dot;
  // A port's shift runs along its side: across a top or bottom, down a left or right.
  const along = (pos: string, shift = 0) => (pos === "top" || pos === "bottom" ? [shift, 0] : [0, shift]);
  const [sdx, sdy] = along(sourcePosition, data?.sourceShift);
  const [tdx, tdy] = along(targetPosition, data?.targetShift);
  const { sx, sy, tx, ty, ux, uy, path: edgePath, midX: labelX, midY: labelY, arrowAngle } = edgeShape(
    sourceX + sdx,
    sourceY + sdy,
    targetX + tdx,
    targetY + tdy,
    data ?? {},
  );
  // Parallel (reverse-pair) edges share one chord, so a midpoint pill can't be
  // told apart from its twin. Unless the level placed it, place each label by
  // (a) sliding it along the chord toward its own source — the pair lands at
  // ~22%/78% and clears ALONG the line at any angle — and (b) nudging it to its
  // lane's OUTER side so the pill hugs its own line.
  let labelDivX = labelX, labelDivY = labelY;
  if (data?.labelAt) {
    labelDivX = data.labelAt.x;
    labelDivY = data.labelAt.y;
  } else if (data?.parallel) {
    const t = 0.22;
    const px = sx + (tx - sx) * t, py = sy + (ty - sy) * t;
    labelDivX = px + -uy * PARALLEL_LABEL_GAP;
    labelDivY = py + ux * PARALLEL_LABEL_GAP;
  }

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
  const look = edgeLook(data, selected);

  return (
    <>
      <defs>
        <clipPath id={clipId}>
          <path
            d={`M ${bx + perpXDir * far} ${by + perpYDir * far} L ${bx - perpXDir * far} ${by - perpYDir * far} L ${bx - perpXDir * far - Math.cos(arrowAngle) * far} ${by - perpYDir * far - Math.sin(arrowAngle) * far} L ${bx + perpXDir * far - Math.cos(arrowAngle) * far} ${by + perpYDir * far - Math.sin(arrowAngle) * far} Z`}
          />
        </clipPath>
      </defs>
      <g opacity={look.opacity}>
        <g clipPath={`url(#${clipId})`}>
          <BaseEdge
            id={id}
            path={edgePath}
            style={{
              stroke: edgeColor,
              strokeWidth: look.width,
              // Dot tier: thin, solid, static curves (sigma-style). Arch tier keeps
              // the animated dashed C4 line.
              strokeDasharray: isDot ? undefined : "6 3",
              animation: isDot ? undefined : "dash-flow 0.5s linear infinite",
            }}
          />
        </g>
        <polygon points={`${tx},${ty} ${ax1},${ay1} ${ax2},${ay2}`} fill={edgeColor} />
      </g>
      {(label || method) && look.label && (
        <EdgeLabelRenderer>
          <div
            style={{
              position: "absolute",
              transform: `translate(-50%, -50%) translate(${labelDivX}px,${labelDivY}px)`,
              zIndex: red ? 3 : full ? 2 : 1,
              pointerEvents: "none",
              ...(data?.dimmed ? { opacity: 0.18 } : full ? {} : { opacity: 0.45 }),
            }}
            className="flex flex-col items-center"
          >
            {label && (
              // Wrapped to a narrow column: a sentence-long label on one line
              // runs across its neighbours' pills where edges converge. At
              // rest, bare text: no pill to cover what runs behind it.
              <div
                className={`w-max max-w-[150px] rounded border px-1.5 py-0.5 text-center text-[10px] leading-tight ${
                  red
                    ? "border-red-500/50 bg-[var(--surface-raised)] text-red-600 dark:text-red-300"
                    : full
                      ? "border-[var(--map-card-stroke)] bg-[var(--surface-raised)] text-[var(--text-secondary)]"
                      : "border-transparent text-[var(--text-muted)]"
                }`}
              >
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
