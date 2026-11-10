/**
 * The relationship edge for the diagram — a port of the pre-pivot canvas edge:
 * a dashed chord ending at its arrowhead and a label/method pill. A reverse
 * pair splits into parallel lanes, code-tier edges inset to their dots' rims,
 * same-ring edges in a styled level bow outward, a long box-tier link follows
 * its lane, a quiet edge (one the drawing already implies) draws as faint
 * round beads with an open arrowhead, a label sits beside its line (on it
 * only when nothing beside is free), its letters haloed so no line strikes
 * them, or, crowded, waits as a mark until its edge is lit, and edges light or fade with the
 * selection. An edge carrying structural violations is drawn red, its
 * reason always written on it. A click on an edge or its label lights it,
 * its label in full.
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
  /** Conforms to the level's style (a legal step between its layers, a
   *  tooling link): the drawing already says it, so it draws fainter and
   *  dotted, still, its label only when lit, so the links that say something
   *  new stand out. */
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
  /** Beside an upright run: which edge of the label meets the line, and
   *  where, so its words start at the line. */
  labelHug?: { side: "left" | "right"; x: number };
  /** Even the label's best spot covers another edge, a card or a label: a
   *  mark stands in for it until the edge is lit. */
  labelCrowded?: boolean;
  /** A click on the label (or its mark) lights the edge, as one on the line. */
  onLabelClick?: () => void;
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

/** The beads a quiet edge draws in: zero-length dashes, round-capped, so
 *  each is a dot the stroke's width across. */
const QUIET_DASH = "0 4.5";

export interface EdgeLook {
  opacity: number;
  width: number;
  /** The label shows: as words, or as its mark while crowded. */
  label: boolean;
  /** A crowded label at rest: a mark where its words go. */
  mark: boolean;
  dash?: string;
  flowing: boolean;
  /** Quiet edges take the fainter tone and an open arrowhead. */
  tone: "plain" | "quiet";
  arrow: "filled" | "open";
}

/** How strongly an edge draws: lit edges (at a selected end) in full; a
 *  quiet one as faint round beads, still, with an open arrowhead and no
 *  label; the rest of a selection's level faded further. The dot tier draws
 *  solid, still curves; the box tiers a dashed line flowing towards the
 *  target. */
export function edgeLook(data: EdgeData | undefined, selected = false): EdgeLook {
  const lit = !!(selected || data?.highlighted);
  const red = (data?.violations?.length ?? 0) > 0;
  const isDot = !!data?.dot;
  const hushed = !!data?.quiet && !lit && !red;
  const opacity = lit ? 1 : data?.dimmed ? (data?.quiet ? 0.14 : 0.18) : red ? 0.95 : isDot ? 0.5 : hushed ? 0.9 : 0.85;
  const width = selected ? 2.5 : isDot ? 1 : hushed ? 1.7 : 1.5;
  const label = (!isDot && !data?.quiet) || labelInFull(data, selected);
  return {
    opacity,
    width,
    label,
    mark: label && !!data?.labelCrowded && !labelInFull(data, selected),
    dash: isDot ? undefined : hushed ? QUIET_DASH : "6 3",
    flowing: !isDot && !hushed,
    tone: hushed ? "quiet" : "plain",
    arrow: hushed ? "open" : "filled",
  };
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
  const look = edgeLook(data, selected);
  const baseColor = red ? getThemedHex("red", "500") : look.tone === "quiet" ? "var(--map-edge-quiet)" : "var(--map-edge)";
  const selColor =
    getComputedStyle(document.documentElement).getPropertyValue("--selection-color").trim() ||
    "#18181b";
  const edgeColor = selected ? selColor : baseColor;
  const full = labelInFull(data, selected);

  // A port's shift runs along its side: across a top or bottom, down a left or right.
  const along = (pos: string, shift = 0) => (pos === "top" || pos === "bottom" ? [shift, 0] : [0, shift]);
  const [sdx, sdy] = along(sourcePosition, data?.sourceShift);
  const [tdx, tdy] = along(targetPosition, data?.targetShift);
  // The line stops where the arrowhead begins, so the two don't overlap.
  const arrowSize = 8;
  const { sx, sy, tx, ty, ux, uy, path: edgePath, midX: labelX, midY: labelY, arrowAngle } = edgeShape(
    sourceX + sdx,
    sourceY + sdy,
    targetX + tdx,
    targetY + tdy,
    { ...data, trimEnd: arrowSize * Math.cos(Math.PI / 6) },
  );
  // Parallel (reverse-pair) edges share one chord, so a midpoint pill can't be
  // told apart from its twin. Unless the level placed it, place each label by
  // (a) sliding it along the chord toward its own source — the pair lands at
  // ~22%/78% and clears ALONG the line at any angle — and (b) nudging it to its
  // lane's OUTER side so the pill hugs its own line.
  let labelDivX = labelX, labelDivY = labelY;
  const hug = data?.labelAt ? data.labelHug : undefined;
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
  const ax1 = tx - arrowSize * Math.cos(arrowAngle - Math.PI / 6);
  const ay1 = ty - arrowSize * Math.sin(arrowAngle - Math.PI / 6);
  const ax2 = tx - arrowSize * Math.cos(arrowAngle + Math.PI / 6);
  const ay2 = ty - arrowSize * Math.sin(arrowAngle + Math.PI / 6);


  return (
    <>
      <g opacity={look.opacity}>
        <BaseEdge
          id={id}
          path={edgePath}
          style={{
            stroke: edgeColor,
            strokeWidth: look.width,
            strokeDasharray: look.dash,
            strokeLinecap: look.dash === QUIET_DASH ? "round" : undefined,
            animation: look.flowing ? "dash-flow 0.5s linear infinite" : undefined,
          }}
        />
        {look.arrow === "open" ? (
          <polyline
            points={`${ax1},${ay1} ${tx},${ty} ${ax2},${ay2}`}
            fill="none"
            stroke={edgeColor}
            strokeWidth={1.2}
            strokeLinecap="round"
            strokeLinejoin="round"
          />
        ) : (
          <polygon points={`${tx},${ty} ${ax1},${ay1} ${ax2},${ay2}`} fill={edgeColor} />
        )}
      </g>
      {(label || method) && look.label && (
        <EdgeLabelRenderer>
          <div
            style={{
              position: "absolute",
              transform: hug
                ? `translate(${hug.side === "left" ? "0" : "-100%"}, -50%) translate(${hug.x}px,${labelDivY}px)`
                : `translate(-50%, -50%) translate(${labelDivX}px,${labelDivY}px)`,
              zIndex: red ? 3 : full ? 2 : 1,
              pointerEvents: data?.onLabelClick ? "all" : "none",
              cursor: data?.onLabelClick ? "pointer" : undefined,
              // At rest the words are muted by colour, not by opacity: a
              // translucent label lets the edge under it strike the text.
              ...(data?.dimmed ? { opacity: 0.18 } : {}),
            }}
            className={`nodrag nopan flex flex-col ${hug ? (hug.side === "left" ? "items-start" : "items-end") : "items-center"}`}
            onClick={(ev) => {
              ev.stopPropagation();
              data?.onLabelClick?.();
            }}
          >
            {label && look.mark ? (
              // Crowded: its words would cover something, so a mark holds
              // the place until the edge is lit.
              <div
                data-label-mark=""
                aria-label={label}
                className="h-[7px] w-[7px] rounded-full border border-[var(--map-edge)] bg-[var(--surface-canvas)]"
              />
            ) : (
              label && (
                // Wrapped to a narrow column: a sentence-long label on one
                // line runs across its neighbours' pills where edges converge.
                // At rest, bare words, their letters haloed in the canvas
                // colour: any line under them (its own, another edge, a
                // boundary) shows between the words instead of striking them
                // or stopping at a box. Beside an upright run, the words
                // start at the line.
                <div
                  className={`w-max max-w-[150px] rounded border px-1.5 py-0.5 text-[10px] leading-tight ${
                    hug ? (hug.side === "left" ? "text-left" : "text-right") : "text-center"
                  } ${
                    red
                      ? "border-red-500/50 bg-[var(--surface-raised)] text-red-600 dark:text-red-300"
                      : full
                        ? "border-[var(--map-card-stroke)] bg-[var(--surface-raised)] text-[var(--text-secondary)]"
                        : "edge-label-halo border-transparent text-[var(--text-muted)]"
                  }`}
                >
                  {label}
                </div>
              )
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
