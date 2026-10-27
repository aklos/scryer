/**
 * The C4 card node for the diagram's architecture tiers — a port of the
 * pre-pivot canvas card (`C4Node`), adapted to the read-only diagram.
 *
 * It draws the 180×160 shape-backed card (or the person silhouette), the
 * external dashed frame, the C4 kind line, the claim states inside the node
 * in words along its foot (outlined red when any fails, blue when any is a
 * todo test), its style's glyph in the corner, the count of structural
 * violations inside it, and the drill affordance on select. Ghosts (nodes that live elsewhere) render
 * dimmed with no drill. Dropped from the canvas: contracts, advisor hints,
 * member chips, and all drag-to-edit.
 */

import type { NodeProps, Node as RFNode } from "@xyflow/react";
import type { DiagramNode } from "./diagramLayout";
import type { StyleViolation } from "../../entities/model/health";
import { CLAIM_LABEL, CLAIM_LETTER, CLAIM_TONE, type ChildTally } from "../../entities/model/claimState";
import { StructuralCount } from "../../entities/model/StructuralCount";
import { NodeHandles } from "../../shared/diagram/NodeHandles";
import { ShapeBackground, resolveShape, getContentInsets } from "../../entities/model/shapes";
import { builtinStyles } from "../../entities/model/styles";
import { StyleGlyph } from "../../entities/model/styles/StyleGlyph";

export interface CardData extends Record<string, unknown> {
  node: DiagramNode;
  selected: boolean;
  /** Its direct children, by the state each reads as. */
  children?: ChildTally;
  /** The state a reader should look at here: failing, then planned. */
  attention?: "failing" | "planned" | null;
  /** A selection exists elsewhere and this node isn't a neighbour — fade it. */
  dimmed?: boolean;
  /** Structure is laid out but the agent hasn't generated the semantics yet —
   *  render the card with the violet "working on this" treatment (matching the
   *  tree's active-node spinner) and cross-fade the content in once it lands. */
  pending?: boolean;
  /** Structural violations charged inside this node's subtree — counted on
   *  the card so the drill path to them is visible from any level. */
  structural?: readonly StyleViolation[];
  /** The level is drawn in a style: every card sits inside its layer's
   *  region, so the card's own layer tag would only repeat the region label. */
  styled?: boolean;
}
export type RFCard = RFNode<CardData, "card">;

/** The "generating" lifecycle marker: `data-gen` on the card root — "pending"
 *  while the agent is still filling this node, "live" once its content lands.
 *  Pure annotation; the ghost-dim and pop-in styling live in the demo's CSS
 *  (`engine.css`), so the product renders nothing extra. `undefined` (the
 *  product default) omits the attribute entirely. */
function genState(pending?: boolean): "pending" | "live" | undefined {
  return pending === undefined ? undefined : pending ? "pending" : "live";
}

/** The C4 kind word on a card, under its name. */
const KIND_WORD: Partial<Record<DiagramNode["kind"], string>> = {
  system: "Software system",
  container: "Container",
  component: "Component",
};

/** The card's foot: how many children a drill into it shows, then how
 *  many of them read as each state, the ones needing a look first, in the
 *  tree's letters: "6 · 1F 2T 3P". A child with no claims counts only in
 *  the total. */
const FOOT_ORDER = ["failing", "planned", "stale", "proven"] as const;

function ChildCounts({ t }: { t: ChildTally }) {
  const shown = FOOT_ORDER.filter((k) => t.byState[k] > 0);
  const said = [`${t.total} ${t.total === 1 ? "child" : "children"}`, ...shown.map((k) => `${t.byState[k]} ${CLAIM_LABEL[k]}`)];
  const bare = t.total - shown.reduce((n, k) => n + t.byState[k], 0);
  // The hover spells the letters out: what each child counts as, and why.
  const hover = [
    `${t.total} ${t.total === 1 ? "child" : "children"} inside, each counted once by its most urgent claim:`,
    ...shown.map((k) => `${t.byState[k]}${CLAIM_LETTER[k]} — ${CLAIM_LABEL[k]}`),
    ...(bare ? [`${bare} with no claims`] : []),
  ].join("\n");
  return (
    <div
      className="pointer-events-auto absolute bottom-1.5 left-3 right-3 z-10 flex gap-2 font-mono text-[10px] tabular-nums"
      aria-label={said.join(", ")}
      title={hover}
    >
      <span className="text-[var(--text-muted)]">{t.total}</span>
      {shown.length > 0 && <span className="-mx-1 text-[var(--text-ghost)]">·</span>}
      {shown.map((k) => (
        <span key={k} className={CLAIM_TONE[k]}>
          {t.byState[k]}
          {CLAIM_LETTER[k]}
        </span>
      ))}
    </div>
  );
}

/** The style glyph's hover: the architecture style by name, and what it
 *  asks of the code. */
function styleHover(style: string): string {
  const def = builtinStyles.find((d) => d.name === style);
  return `Architecture style: ${style}${def?.description ? `\n${def.description}` : ""}`;
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
  // Subgraph highlight: faded when a selection elsewhere doesn't touch this
  // node. A reference is not faded: its hollow dashed frame says it lives
  // elsewhere, and its name keeps full contrast.
  const dimClass = data.dimmed ? "opacity-30" : "";

  // Person nodes: silhouette above, no background rect, normal text layout.
  // On a deeper level a person is a reference: hollow, like every reference.
  // Solid, not dashed — the dash is the external systems' alone.
  if (node.kind === "person") {
    const longDesc = (node.description?.length ?? 0) > 80;
    // The silhouette outline — shared by the fade fill, the generating barber,
    // and the stroke so all three are always the exact same shape.
    const silhouette = "M 33,72 C 33,42 48,28 76,24 A 22,26 0 1,1 104,24 C 132,28 147,42 147,72";
    return (
      <div
        data-gen={genState(data.pending)}
        className={`relative h-[160px] w-[180px] transition-opacity ${dimClass}`}
      >
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
            {isGhost ? (
              // A reference is the card stroke at full strength, closed
              // along the bottom, with no fill.
              <path
                d={`${silhouette} Z`}
                fill="none"
                className={selected ? "stroke-[var(--text)]" : "stroke-[var(--map-card-stroke)]"}
                strokeWidth={selected ? 2.5 : 1.25}
              />
            ) : (
              <>
                <path d={`${silhouette} Z`} fill={`url(#person-fade-${id})`} />
                <path
                  d={silhouette}
                  fill="none"
                  stroke={`url(#person-stroke-${id})`}
                  strokeWidth={selected ? 2.5 : 1}
                />
              </>
            )}
          </svg>
          <div
            className="flex w-full flex-col items-center transition-opacity duration-500"
            style={{ opacity: data.pending ? 0 : 1 }}
          >
            <div className="line-clamp-2 w-full break-words text-center text-sm font-semibold leading-tight text-[var(--map-title)]">
              {node.name || "Untitled"}
            </div>
            {node.description && (
              <div
                className="line-clamp-4 mt-2 w-full break-words text-center text-[10px] leading-snug text-[var(--map-desc)]"
              >
                {node.description}
              </div>
            )}
          </div>
        </div>
      </div>
    );
  }

  // The outline is red only when a claim inside is failing: something is
  // broken. Todo is work, not a fault, and the foot counts it like the rest,
  // so a card with todos never reads louder than a proven one.
  const attention = data.attention;
  const kindWord = KIND_WORD[node.kind];
  const typeLine = [isExternal ? "External system" : kindWord, node.technology].filter(Boolean).join(" · ");

  return (
    <div data-gen={genState(data.pending)} className={`relative w-[180px] transition-opacity ${dimClass}`}>
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
              : attention === "failing"
                ? "stroke-red-500/80 dark:stroke-red-400/70"
                : "stroke-[var(--map-card-stroke)]"
          }
          strokeWidth={selected ? 2.5 : attention === "failing" ? 2 : 1.25}
          strokeDasharray={isExternal ? "6 3" : undefined}
          kind={node.kind}
          external={!!isExternal}
        />
        <NodeHandles />

        {/* Its children by state, along the foot. Ghosts are counted at home. */}
        {!isGhost && !data.pending && expandable && data.children && <ChildCounts t={data.children} />}

        {/* Structural violations inside, top-left. Ghosts are counted at home. */}
        {!isGhost && !data.pending && (
          <StructuralCount
            violations={data.structural}
            className="pointer-events-auto absolute left-2.5 top-1.5 z-10"
          />
        )}

        {/* The style's glyph, top-right: what shape the inside takes. Gives
            way to the drill button on select. */}
        {node.style && !isGhost && !(expandable && selected) && (
          <div
            className="absolute right-2.5 top-2 z-10 opacity-80"
            aria-label={`Drawn as ${node.style}`}
            title={styleHover(node.style)}
          >
            <StyleGlyph style={node.style} />
          </div>
        )}

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
            {/* Layer tag: the fixed word that says what role this component
                plays in its container's style. */}
            {node.layer && node.kind === "component" && (!data.styled || node.reference) && (
              <div
                className="mb-1 text-center text-[9px] uppercase tracking-[0.15em] text-[var(--text-ghost)]"
              >
                {node.layer}
              </div>
            )}
            <div className="line-clamp-2 w-full break-words text-center text-sm font-semibold leading-tight text-[var(--map-title)]">
              {node.name || "Untitled"}
            </div>
            {typeLine && (
              <div
                className="line-clamp-2 mt-0.5 text-center text-[10px] tracking-wider text-[var(--text-tertiary)]"
              >
                {typeLine}
              </div>
            )}
            {node.description && (
              <div
                className="line-clamp-4 mt-2 w-full break-words text-center text-[10px] leading-snug text-[var(--map-desc)]"
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
