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
import { Layers } from "lucide-react";
import type { DiagramNode } from "../diagramLayout";
import type { Mark } from "../changeMarks";
import { NodeHandles } from "./NodeHandles";
import { ShapeBackground, resolveShape, getContentInsets } from "../shapes";

export interface CardData extends Record<string, unknown> {
  node: DiagramNode;
  selected: boolean;
  /** Change mark (plan ?? drift) for this node, or null when unchanged. */
  mark?: Mark | null;
  /** A selection exists elsewhere and this node isn't a neighbour — fade it. */
  dimmed?: boolean;
}
export type RFCard = RFNode<CardData, "card">;

/** Card outline stroke per change mark — same palette as the tree gutter and
 *  the dots: A green, M amber, D red, R blue, Q violet, X orange. */
const MARK_STROKE: Record<Mark, string> = {
  A: "stroke-emerald-500/70 dark:stroke-emerald-400/50",
  M: "stroke-amber-500/70 dark:stroke-amber-400/50",
  D: "stroke-red-500/70 dark:stroke-red-400/50",
  R: "stroke-blue-500/70 dark:stroke-blue-400/50",
  Q: "stroke-violet-500/70 dark:stroke-violet-400/50",
  X: "stroke-orange-500/70 dark:stroke-orange-400/50",
};

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
            <path
              d={["M 33,72 C 33,42 48,28 76,24", "A 22,26 0 1,1 104,24", "C 132,28 147,42 147,72", "Z"].join(" ")}
              fill={`url(#person-fade-${id})`}
            />
            <path
              d={["M 33,72 C 33,42 48,28 76,24", "A 22,26 0 1,1 104,24", "C 132,28 147,42 147,72"].join(" ")}
              fill="none"
              stroke={`url(#person-stroke-${id})`}
              strokeWidth={selected ? 2.5 : 1}
            />
          </svg>
          <div className="w-full break-all text-center text-sm font-semibold leading-tight">
            {node.name || "Untitled"}
          </div>
          {node.description && (
            <div className="mt-2 w-full overflow-hidden break-words text-center text-[10px] leading-snug text-[var(--text-muted)]">
              {node.description}
            </div>
          )}
        </div>
      </div>
    );
  }

  const markStroke = data.mark && !isExternal ? MARK_STROKE[data.mark] : null;

  return (
    <div className={`relative w-[180px] transition-opacity ${dimClass}`}>
      <div className="relative h-[160px]">
        <ShapeBackground
          shape={shape}
          fillClass={isExternal || isGhost ? "fill-[var(--scryer-ext-bg)]" : "fill-[var(--scryer-node-bg)]"}
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
          strokeDasharray={isExternal || isGhost ? "6 3" : undefined}
          kind={node.kind}
          external={!!isExternal}
        />
        <NodeHandles />

        {/* Drill-in — shown on select. */}
        {expandable && selected && (
          <div className="absolute right-1.5 top-1.5 z-10 flex items-center">
            <button
              className="nodrag nopan flex h-5 w-5 cursor-pointer items-center justify-center rounded bg-[var(--surface-tint)] text-xs text-[var(--text-secondary)] transition-colors hover:bg-[var(--surface-active)]"
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

        {/* Has-children indicator. */}
        {expandable && node.hasChildren && (
          <div className="pointer-events-none absolute bottom-2 right-2.5 z-10 text-[var(--text-ghost)]">
            <Layers size={12} strokeWidth={1.5} />
          </div>
        )}

        {/* Content area. */}
        <div
          className="absolute flex flex-col items-center justify-center overflow-hidden text-[var(--text)]"
          style={{ top: insets.top, bottom: insets.bottom, left: insets.left, right: insets.right }}
        >
          <div className="w-full break-all text-center text-sm font-semibold leading-tight">
            {node.name || "Untitled"}
          </div>
          {node.technology && (
            <div className="mt-0.5 text-center text-[10px] tracking-wider text-[var(--text-tertiary)]">
              {node.technology}
            </div>
          )}
          {node.description && (
            <div className="mt-2 w-full overflow-hidden break-words text-center text-[10px] leading-snug text-[var(--text-muted)]">
              {node.description}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
