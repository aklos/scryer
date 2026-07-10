/**
 * The C4 card node for the diagram's architecture tiers — a faithful port of
 * the pre-pivot canvas card (`C4Node`), adapted to the read-only v0.3 diagram.
 *
 * Kept from main: the 180×160 shape-backed card, the person silhouette, status-
 * colored stroke, external dashed frame, the drill affordance on select, and
 * the has-children indicator. Dropped: contracts, advisor hints, member chips,
 * reference nodes, and all drag-to-edit — editing lives in the tree and page.
 */

import type { NodeProps, Node as RFNode } from "@xyflow/react";
import type { DiagramNode } from "../diagramLayout";
import type { Mark } from "../changeMarks";
import type { Completeness } from "../health";
import { CompletenessPie } from "../CompletenessPie";
import { NodeHandles } from "./NodeHandles";
import { ShapeBackground, resolveShape, getContentInsets } from "../shapes";

export interface CardData extends Record<string, unknown> {
  node: DiagramNode;
  selected: boolean;
  /** Change mark (plan ?? drift) for this node, or null when unchanged. */
  mark?: Mark | null;
  /** A selection exists elsewhere and this node isn't a neighbour — fade it. */
  dimmed?: boolean;
  /** Structure is laid out but the agent hasn't generated the semantics yet —
   *  render the card with the indigo "working on this" treatment (matching the
   *  tree's active-node spinner) and cross-fade the content in once it lands. */
  pending?: boolean;
  /** Build completeness for this node — drives the corner % + anchorage badge. */
  completeness?: Completeness;
}
export type RFCard = RFNode<CardData, "card">;

/** The one "generating" effect: the same animated barber-pole the powerline uses
 *  for agent activity, filling the card UNTIL its content lands, then cross-fading
 *  out. Conforms to the card exactly because it's a second `ShapeBackground` whose
 *  fill is the barber pattern (`url(#barber-gen)`, defined by the demo) — so it
 *  follows the silhouette, the container tab, the component side-tabs, etc.
 *  Rendered only for cards the demo marks pending; `undefined` (the product
 *  default) skips it entirely. */
function GeneratingFill({
  shape,
  kind,
  external,
  pending,
}: {
  shape: Parameters<typeof ShapeBackground>[0]["shape"];
  kind: DiagramNode["kind"];
  external: boolean;
  pending?: boolean;
}) {
  if (pending === undefined) return null;
  return (
    <div
      className="pointer-events-none absolute inset-0 transition-opacity duration-300"
      style={{ opacity: pending ? 1 : 0 }}
    >
      <ShapeBackground
        shape={shape}
        kind={kind}
        external={external}
        fillClass="fill-barber-gen"
      />
    </div>
  );
}

/** Card outline stroke per change mark — same palette as the tree gutter and
 *  the dots: A green, M/R amber (plan edits), D red, Q/X orange (drift). */
const MARK_STROKE: Record<Mark, string> = {
  A: "stroke-emerald-500/70 dark:stroke-emerald-400/50",
  M: "stroke-amber-500/70 dark:stroke-amber-400/50",
  D: "stroke-red-500/70 dark:stroke-red-400/50",
  R: "stroke-amber-500/70 dark:stroke-amber-400/50",
  Q: "stroke-orange-500/70 dark:stroke-orange-400/50",
  X: "stroke-orange-500/70 dark:stroke-orange-400/50",
};

/** The shared completeness pie, positioned on the card corner. */
function CompletenessDot({ c }: { c: Completeness }) {
  const measured = c.pct !== undefined;
  return (
    <div
      className="pointer-events-none absolute bottom-1.5 left-2 z-10"
      title={
        measured
          ? `${c.pct}% of this node's claims read through to code`
          : "No leaf claims yet — nothing to measure"
      }
    >
      <CompletenessPie c={c} />
    </div>
  );
}

function isExpandable(kind: DiagramNode["kind"]): boolean {
  return kind === "system" || kind === "container" || kind === "component";
}

/** Drill request — DiagramView listens and descends a level. */
function requestExpand(nodeId: string) {
  window.dispatchEvent(new CustomEvent("diagram-expand", { detail: { nodeId } }));
}

export function DiagramCard({ id, data }: NodeProps<RFCard>) {
  const { node, selected } = data;
  const shape = resolveShape(node.kind);
  const insets = getContentInsets(shape);
  const isExternal = node.external;
  const isGhost = node.reference;
  // Ghosts live elsewhere — no drill affordance; double-click navigates instead.
  const expandable = isExpandable(node.kind) && !isExternal && !isGhost;
  // Subgraph highlight: faded when a selection elsewhere doesn't touch this node.
  const dimClass = data.dimmed ? "opacity-30" : isGhost ? "opacity-60" : "";

  // Person nodes: silhouette above, no background rect, normal text layout.
  if (node.kind === "person") {
    const longDesc = (node.description?.length ?? 0) > 80;
    // The silhouette outline — shared by the fade fill, the generating barber,
    // and the stroke so all three are always the exact same shape.
    const silhouette = "M 33,72 C 33,42 48,28 76,24 A 22,26 0 1,1 104,24 C 132,28 147,42 147,72";
    return (
      <div className={`relative h-[160px] w-[180px] transition-opacity ${data.dimmed ? "opacity-30" : ""}`}>
        <NodeHandles />
        <div
          className="absolute flex flex-col items-center justify-center overflow-visible text-[var(--text)]"
          style={{ top: longDesc ? -20 : 6, bottom: 6, left: 8, right: 8 }}
        >
          <svg
            className="pointer-events-none shrink-0 overflow-visible"
            width="200"
            height="80"
            viewBox="0 0 180 72"
            style={{ marginBottom: -34 }}
          >
            <defs>
              <linearGradient id={`person-fade-${id}`} gradientUnits="userSpaceOnUse" x1="0" y1="40" x2="0" y2="72">
                <stop offset="0%" stopColor="var(--scryer-person-fill)" stopOpacity="1" />
                <stop offset="100%" stopColor="var(--scryer-person-fill)" stopOpacity="0" />
              </linearGradient>
              <linearGradient id={`person-stroke-${id}`} gradientUnits="userSpaceOnUse" x1="0" y1="0" x2="0" y2="72">
                <stop offset="0%" stopColor={selected ? "var(--scryer-select-stroke)" : "var(--scryer-outline-stroke)"} stopOpacity={selected ? 1 : 0.8} />
                <stop offset="70%" stopColor={selected ? "var(--scryer-select-stroke)" : "var(--scryer-outline-stroke)"} stopOpacity={selected ? 0.8 : 0.3} />
                <stop offset="100%" stopColor={selected ? "var(--scryer-select-stroke)" : "var(--scryer-outline-stroke)"} stopOpacity="0" />
              </linearGradient>
            </defs>
            <path d={`${silhouette} Z`} fill={`url(#person-fade-${id})`} />
            {data.pending !== undefined && (
              <path
                d={`${silhouette} Z`}
                fill="url(#barber-gen)"
                style={{ opacity: data.pending ? 1 : 0, transition: "opacity 0.3s" }}
              />
            )}
            <path
              d={silhouette}
              fill="none"
              stroke={`url(#person-stroke-${id})`}
              strokeWidth={selected ? 2.5 : 1}
            />
          </svg>
          <div
            className="flex w-full flex-col items-center transition-opacity duration-500"
            style={{ opacity: data.pending ? 0 : 1 }}
          >
            <div className="line-clamp-2 w-full break-words text-center text-sm font-semibold leading-tight">
              {node.name || "Untitled"}
            </div>
            {node.description && (
              <div
                className="line-clamp-4 mt-2 w-full break-words text-center text-[10px] leading-snug text-[var(--text-muted)]"
                title={node.description}
              >
                {node.description}
              </div>
            )}
          </div>
        </div>
      </div>
    );
  }

  // External nodes keep their dashed outline but still show a change stroke —
  // an edited external dependency is exactly the change worth noticing.
  const markStroke = data.mark ? MARK_STROKE[data.mark] : null;
  // Completeness pie — hidden on ghosts (measured at the node's real home) and on
  // nodes with no anchorable primitives at all.
  const comp = isGhost ? undefined : data.completeness;
  const showComp = !!comp && comp.total > 0;

  return (
    <div className={`relative w-[180px] transition-opacity ${dimClass}`}>
      <div className="relative h-[160px]">
        <ShapeBackground
          shape={shape}
          fillClass={
            isGhost
              ? "fill-transparent"
              : isExternal
                ? "fill-[var(--scryer-ext-bg)]"
                : "fill-[var(--scryer-node-bg)]"
          }
          strokeClass={
            selected
              ? "stroke-[var(--text)]"
              : markStroke
                ? markStroke
                : isExternal || isGhost
                  ? "stroke-[var(--scryer-outline-stroke)]"
                  : "stroke-[var(--border)]"
          }
          strokeWidth={selected ? 2.5 : markStroke ? 2 : 1}
          strokeDasharray={isExternal ? "6 3" : undefined}
          kind={node.kind}
          external={!!isExternal}
        />
        <NodeHandles />
        <GeneratingFill shape={shape} kind={node.kind} external={!!isExternal} pending={data.pending} />

        {/* Completeness pie, bottom-left. Hidden while the card is still
            generating so it doesn't flash on empty. */}
        {showComp && !data.pending && <CompletenessDot c={comp!} />}

        {/* Drill-in — shown on select. */}
        {expandable && selected && (
          <div className="absolute right-1.5 top-1.5 z-10 flex items-center">
            <button
              className="nodrag nopan flex h-5 w-5 items-center justify-center rounded bg-[var(--surface-tint)] text-xs text-[var(--text-secondary)] transition-colors hover:bg-[var(--surface-active)]"
              title="Drill into this node"
              onClick={(e) => {
                // Stop the click from bubbling to React Flow's onNodeClick,
                // which would reselect and reframe the diagram to the parent
                // level — clobbering this drill.
                e.stopPropagation();
                requestExpand(id);
              }}
            >
              &#8599;
            </button>
          </div>
        )}

        {/* Direct-child count. */}
        {expandable && node.childCount > 0 && (
          <div className="pointer-events-none absolute bottom-1.5 right-2.5 z-10 text-[11px] font-medium tabular-nums text-[var(--text-ghost)]">
            {node.childCount}
          </div>
        )}

        {/* Content area — cross-fades in as the card finishes generating. */}
        <div
          className="absolute flex flex-col items-center justify-center overflow-hidden text-[var(--text)]"
          style={{ top: insets.top, bottom: insets.bottom, left: insets.left, right: insets.right }}
        >
          <div
            className="flex w-full flex-col items-center transition-opacity duration-500"
            style={{ opacity: data.pending ? 0 : 1 }}
          >
            {/* Per-field clamps: worst case (2+2+4 lines) still fits the 160px
                card, so a paragraph-length technology or description can never
                push the stack out of the shape (unclamped, the centered flex
                overflowed BOTH ends — beheaded title, text under the icons). */}
            <div className="line-clamp-2 w-full break-words text-center text-sm font-semibold leading-tight">
              {node.name || "Untitled"}
            </div>
            {node.technology && (
              <div
                className="line-clamp-2 mt-0.5 text-center text-[10px] tracking-wider text-[var(--text-tertiary)]"
                title={node.technology}
              >
                {node.technology}
              </div>
            )}
            {node.description && (
              <div
                className="line-clamp-4 mt-2 w-full break-words text-center text-[10px] leading-snug text-[var(--text-muted)]"
                title={node.description}
              >
                {node.description}
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
