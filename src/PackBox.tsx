/**
 * Renders the flat inventory grid — a fixed lattice of `cols x rows` chunky
 * cells. Entries sit at their stored `cell`. Groups are painted as tinted
 * background rectangles with a label and resize handle.
 *
 * When an item is being aimed, `highlight` paints the footprint preview:
 * green when valid, red when occupied or out of bounds.
 */

import { useContext } from "react";
import type { ReactNode } from "react";
import type { Altitude, Entry, Group, Surface } from "./viewmodel";
import {
  CELL_W,
  CELL_H,
  ITEM_INSET,
  cardSpan,
  groupRect,
  groupDepth,
  type Rect,
} from "./pack";
import { GridContext } from "./gridcontext";
import { useZoom } from "./PanZoom";
import type { SurfaceContext, ContextEntry } from "./references";
import { PerimeterNode, PERSON_W } from "./PerimeterNode";

export interface GridHighlight {
  rect: Rect;
  valid: boolean;
  inset?: boolean;
}

/** Logical pixel dimensions — multiplied by zoom at render time. */
export const LABEL_H = 20;
const RESIZE_SIZE = 24;

function GroupOverlay({ group, surface }: { group: Group; surface: Surface }) {
  const { beginResize, beginGroupDrag, heldId, resizingId, resizeRejected } = useContext(GridContext);
  const zoom = useZoom();
  const region = groupRect(group);
  const depth = groupDepth(surface, group);
  const ghost = heldId === group.id;
  const resizing = resizingId === group.id;
  const rejection = resizing ? resizeRejected : null;

  const left = region.col * CELL_W * zoom;
  const top = region.row * CELL_H * zoom;
  const width = region.w * CELL_W * zoom;
  const height = region.h * CELL_H * zoom;
  const labelH = LABEL_H * zoom;
  const resizeSize = RESIZE_SIZE * zoom;
  const opacity = ghost ? 0.4 : 1;
  const dragZ = 11 + depth;
  const resizeZ = 11 + depth;

  return (
    <>
      {/* group boundary — tinted fill + visible border */}
      <div
        style={{
          position: "absolute",
          left, top, width, height,
          zIndex: resizing ? 10 : depth,
          opacity,
          border: `${1.5 * zoom}px solid ${resizing ? "var(--text-secondary)" : "var(--text-muted)"}`,
          backgroundColor: `color-mix(in srgb, black ${14 + depth * 8}%, transparent)`,
          borderRadius: 12 * zoom,
          pointerEvents: "none",
          transition: "border-color 0.15s",
        }}
      />
      {/* legend label — straddles the top border like a fieldset legend */}
      <div
        data-group-pickup={group.id}
        onPointerDown={(e) => {
          e.stopPropagation();
          beginGroupDrag(group.id, e);
        }}
        className="flex cursor-grab items-center hover:border-[var(--border-strong)] hover:text-[var(--text-secondary)]"
        style={{
          position: "absolute",
          left: left + 8 * zoom,
          top: top - labelH / 2,
          height: labelH,
          padding: `0 ${8 * zoom}px`,
          gap: 6 * zoom,
          zIndex: dragZ,
          opacity,
          backgroundColor: "var(--surface-canvas)",
          borderRadius: 5 * zoom,
          border: `${1.5 * zoom}px solid var(--text-muted)`,
        }}
      >
        <svg width={4 * zoom} height={8 * zoom} viewBox="0 0 4 8" className="shrink-0 opacity-30">
          <circle cx="1" cy="1" r="0.75" fill="var(--text-muted)" />
          <circle cx="3" cy="1" r="0.75" fill="var(--text-muted)" />
          <circle cx="1" cy="4" r="0.75" fill="var(--text-muted)" />
          <circle cx="3" cy="4" r="0.75" fill="var(--text-muted)" />
          <circle cx="1" cy="7" r="0.75" fill="var(--text-muted)" />
          <circle cx="3" cy="7" r="0.75" fill="var(--text-muted)" />
        </svg>
        <span
          className="font-semibold uppercase text-[var(--text-muted)] whitespace-nowrap"
          style={{ fontSize: 10 * zoom, letterSpacing: 0.1 * zoom + "em" }}
        >
          {group.name}
        </span>
      </div>
      {/* rejected-edge highlights */}
      {rejection?.right && (
        <div
          aria-hidden
          style={{
            position: "absolute",
            left: left + width - 2 * zoom,
            top,
            width: 3 * zoom,
            height,
            zIndex: 10,
            borderRadius: `0 ${12 * zoom}px ${12 * zoom}px 0`,
            backgroundColor: "var(--color-red-500)",
            opacity: 0.7,
            pointerEvents: "none",
            transition: "opacity 0.15s",
          }}
        />
      )}
      {rejection?.bottom && (
        <div
          aria-hidden
          style={{
            position: "absolute",
            left,
            top: top + height - 2 * zoom,
            width,
            height: 3 * zoom,
            zIndex: 10,
            borderRadius: `0 0 ${12 * zoom}px ${12 * zoom}px`,
            backgroundColor: "var(--color-red-500)",
            opacity: 0.7,
            pointerEvents: "none",
            transition: "opacity 0.15s",
          }}
        />
      )}
      {/* resize handle */}
      <div
        data-no-pickup
        onPointerDown={(e) => {
          e.stopPropagation();
          beginResize(group.id, e);
        }}
        className="cursor-nwse-resize"
        style={{
          position: "absolute",
          left: left + width - resizeSize,
          top: top + height - resizeSize,
          width: resizeSize,
          height: resizeSize,
          zIndex: resizeZ,
          opacity,
          background: "linear-gradient(135deg, transparent 50%, var(--border-strong) 50%)",
          borderRadius: `0 0 ${12 * zoom}px 0`,
        }}
      />
    </>
  );
}

/** Innermost ring sits this many logical px outside the grid edge. */
const RING_PAD = 60;
/** Minimum band height between consecutive rings (when no refs in the band). */
const RING_GAP = 40;
/** Logical padding inside a band, between ring borders and refs. */
const BAND_PAD = 16;
/** Logical gap between adjacent refs in a band. */
const REF_GAP = 16;

export interface LevelInfo {
  altitude: Altitude;
  label: string;
}

export function PackBox({
  surface,
  highlight,
  levels = [],
  context,
  onRingClick,
  renderEntry,
}: {
  surface: Surface;
  highlight?: GridHighlight | null;
  /** Levels outer-most first; the last entry is the current surface. */
  levels?: LevelInfo[];
  context?: SurfaceContext;
  onRingClick?: (ancestorIndex: number) => void;
  renderEntry: (entry: Entry) => ReactNode;
}) {
  const zoom = useZoom();
  const cellW = CELL_W * zoom;
  const cellH = CELL_H * zoom;
  const inset = ITEM_INSET * zoom;
  const entries = surface.entries.filter((e) => e.kind !== "person");

  const placed = entries.map((entry) => ({
    entry,
    span: cardSpan(entry),
    cell: entry.cell ?? { row: 0, col: 0 },
  }));

  const sortedGroups = [...surface.groups].sort(
    (a, b) => groupDepth(surface, a) - groupDepth(surface, b),
  );

  // Content extent — rebase keeps origin at (0,0) so the bottom-right corner
  // is all we need for ring sizing. Perimeter follows the grid container.
  let contentCols = 0,
    contentRows = 0;
  for (const { span, cell } of placed) {
    contentCols = Math.max(contentCols, cell.col + span.w);
    contentRows = Math.max(contentRows, cell.row + span.h);
  }
  for (const g of surface.groups) {
    contentCols = Math.max(contentCols, g.cell.col + g.size.cols);
    contentRows = Math.max(contentRows, g.cell.row + g.size.rows);
  }

  let cols = contentCols,
    rows = contentRows;
  if (highlight) {
    cols = Math.max(cols, highlight.rect.col + highlight.rect.w);
    rows = Math.max(rows, highlight.rect.row + highlight.rect.h);
  }
  cols = Math.max(1, cols);
  rows = Math.max(1, rows);

  const contentW = Math.max(1, contentCols) * cellW;
  const contentH = Math.max(1, contentRows) * cellH;

  // ---- band layout --------------------------------------------------------
  // Group context entries by origin altitude AND direction:
  //   - left lane:  incoming (callers — things that link INTO our scope)
  //   - right lane: outgoing (callees — things we link OUT to)
  // "both" defaults to left (incoming wins) so a peer with bidirectional
  // links still has a definite home.
  type Lane = "left" | "right";
  const laneFor = (ce: ContextEntry): Lane =>
    ce.direction === "outgoing" ? "right" : "left";
  const banded = new Map<Altitude, { left: ContextEntry[]; right: ContextEntry[] }>();
  if (context) {
    for (const ce of [...context.persons, ...context.externals, ...context.refs]) {
      const slot = banded.get(ce.altitude) ?? { left: [], right: [] };
      slot[laneFor(ce)].push(ce);
      banded.set(ce.altitude, slot);
    }
  }

  // Each ring has its own pad on each side. Top and bottom only get RING_GAP
  // breathing room. Left/right grow to fit their lane content (refs stack
  // vertically; lane width = single ref width + padding).
  const refLogicalW = PERSON_W;
  interface Pads { top: number; right: number; bottom: number; left: number }
  const ringPads: Pads[] = [];
  for (let i = 0; i < levels.length; i++) {
    if (i === 0) {
      ringPads.push({ top: RING_PAD, right: RING_PAD, bottom: RING_PAD, left: RING_PAD });
    } else {
      const altitude = levels[levels.length - 1 - i].altitude;
      const slot = banded.get(altitude) ?? { left: [], right: [] };
      const leftW = slot.left.length > 0 ? refLogicalW + BAND_PAD * 2 : RING_GAP;
      const rightW = slot.right.length > 0 ? refLogicalW + BAND_PAD * 2 : RING_GAP;
      const prev = ringPads[i - 1];
      ringPads.push({
        top: prev.top + RING_GAP,
        bottom: prev.bottom + RING_GAP,
        left: prev.left + leftW,
        right: prev.right + rightW,
      });
    }
  }
  const outerPad = ringPads[ringPads.length - 1] ?? { top: 0, right: 0, bottom: 0, left: 0 };
  // Outer grid margin = the outermost ring's extent on each side. Use the
  // max so the grid container reserves enough on every side (CSS margin is
  // a single shorthand — use individual margins instead).
  const marginTop = outerPad.top * zoom + 8 * zoom;
  const marginRight = outerPad.right * zoom + 8 * zoom;
  const marginBottom = outerPad.bottom * zoom + 8 * zoom;
  const marginLeft = outerPad.left * zoom + 8 * zoom;

  return (
    <div
      data-grid
      style={{
        position: "relative",
        display: "grid",
        gridTemplateColumns: `repeat(${cols}, ${cellW}px)`,
        gridTemplateRows: `repeat(${rows}, ${cellH}px)`,
        gap: 0,
        marginTop,
        marginRight,
        marginBottom,
        marginLeft,
      }}
    >
      {/* level ring overlays + their band content (perimeter refs).
          Each ring extends asymmetrically: just RING_GAP on top/bottom,
          left/right grow to hold incoming/outgoing refs respectively. */}
      {levels.map((level, i) => {
        const depth = levels.length - 1 - i;
        const p = ringPads[depth] ?? { top: 0, right: 0, bottom: 0, left: 0 };
        const padTop = p.top * zoom;
        const padRight = p.right * zoom;
        const padBottom = p.bottom * zoom;
        const padLeft = p.left * zoom;
        const isInnermost = depth === 0;
        const isClickable = !isInnermost && onRingClick;
        const slot = banded.get(level.altitude) ?? { left: [], right: [] };
        // Inner ring's pads in *this* ring's coord space.
        const inner = depth > 0
          ? ringPads[depth - 1] ?? { top: 0, right: 0, bottom: 0, left: 0 }
          : { top: 0, right: 0, bottom: 0, left: 0 };
        const innerLeft = (p.left - inner.left) * zoom; // band thickness on left
        const innerTop = (p.top - inner.top) * zoom;   // band thickness on top
        const innerH = contentH + (inner.top + inner.bottom) * zoom;
        const renderRef = (ce: ContextEntry) => (
          <PerimeterNode
            key={ce.entry.id}
            entry={ce.entry}
            variant={
              ce.entry.kind === "person"
                ? "person"
                : ce.entry.external
                  ? "external"
                  : "reference"
            }
          />
        );
        return (
          <div
            key={`ring-${i}`}
            style={{
              position: "absolute",
              left: -padLeft,
              top: -padTop,
              width: contentW + padLeft + padRight,
              height: contentH + padTop + padBottom,
              borderRadius: 24 * zoom,
              border: `${zoom}px ${isInnermost ? "solid" : "dashed"} ${
                isInnermost ? "var(--border)" : "var(--border-subtle)"
              }`,
              pointerEvents: "none",
              zIndex: -1,
            }}
          >
            <span
              data-no-pickup
              onClick={isClickable ? () => onRingClick(i) : undefined}
              className={`absolute font-semibold uppercase ${
                isInnermost
                  ? "text-[var(--text-muted)]"
                  : "text-[var(--text-ghost)] hover:text-[var(--text-muted)]"
              } ${isClickable ? "cursor-pointer" : ""}`}
              style={{
                top: 10 * zoom,
                left: 16 * zoom,
                fontSize: 12 * zoom,
                letterSpacing: 0.14 * zoom + "em",
                pointerEvents: isClickable ? "auto" : "none",
              }}
            >
              {level.label}
            </span>
            {/* left lane — incoming refs (callers) */}
            {!isInnermost && slot.left.length > 0 && (
              <div
                style={{
                  position: "absolute",
                  top: innerTop,
                  left: BAND_PAD * zoom,
                  width: innerLeft - BAND_PAD * 2 * zoom,
                  height: innerH,
                  display: "flex",
                  flexDirection: "column",
                  justifyContent: "center",
                  alignItems: "flex-end",
                  gap: REF_GAP * zoom,
                  pointerEvents: "auto",
                }}
              >
                {slot.left.map(renderRef)}
              </div>
            )}
            {/* right lane — outgoing refs (callees) */}
            {!isInnermost && slot.right.length > 0 && (
              <div
                style={{
                  position: "absolute",
                  top: innerTop,
                  right: BAND_PAD * zoom,
                  width: (p.right - inner.right) * zoom - BAND_PAD * 2 * zoom,
                  height: innerH,
                  display: "flex",
                  flexDirection: "column",
                  justifyContent: "center",
                  alignItems: "flex-start",
                  gap: REF_GAP * zoom,
                  pointerEvents: "auto",
                }}
              >
                {slot.right.map(renderRef)}
              </div>
            )}
          </div>
        );
      })}

      {/* group overlays */}
      {sortedGroups.map((g) => (
        <GroupOverlay key={g.id} group={g} surface={surface} />
      ))}

      {/* entry cards */}
      {placed.map(({ entry, span, cell }) => (
        <div
          key={entry.id}
          style={{
            gridColumn: `${cell.col + 1} / span ${span.w}`,
            gridRow: `${cell.row + 1} / span ${span.h}`,
            padding: inset,
            zIndex: 10,
          }}
        >
          {renderEntry(entry)}
        </div>
      ))}

      {/* footprint preview */}
      {highlight && (() => {
        const pad = (highlight.inset !== false ? ITEM_INSET : 0) * zoom;
        return (
          <div
            aria-hidden
            style={{
              position: "absolute",
              left: highlight.rect.col * cellW + pad,
              top: highlight.rect.row * cellH + pad,
              width: highlight.rect.w * cellW - pad * 2,
              height: highlight.rect.h * cellH - pad * 2,
              borderRadius: 12 * zoom,
              pointerEvents: "none",
              zIndex: 20,
              border: `${2 * zoom}px solid ${
                highlight.valid ? "rgb(52 211 153)" : "rgb(248 113 113)"
              }`,
              background: highlight.valid
                ? "rgb(52 211 153 / 0.15)"
                : "rgb(248 113 113 / 0.15)",
            }}
          />
        );
      })()}
    </div>
  );
}
