/**
 * Renders the flat inventory grid — a fixed lattice of `cols x rows` chunky
 * cells. Entries sit at their stored `cell`. Groups are painted as tinted
 * background rectangles with a label and resize handle.
 *
 * When an item is being aimed, `highlight` paints the footprint preview:
 * green when valid, red when occupied or out of bounds.
 */

import {
  useContext,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from "react";
import { createPortal } from "react-dom";
import type { ReactNode } from "react";
import { Pencil, Trash2 } from "lucide-react";
import type {
  Altitude,
  GroupView,
  NodeView,
  Responsibility,
  SurfaceView,
} from "./viewmodel";
import {
  CELL_W,
  CELL_H,
  ITEM_INSET,
  groupHeaderRows,
  cardSpan,
  cardWidthCells,
  spanFromMeasuredHeight,
  rectsOverlap,
  groupRect,
  groupDepth,
  type Rect,
  type Span,
} from "./pack";
import { GridContext } from "./gridcontext";
import { useZoom } from "./PanZoom";
import type { SurfaceContext, ContextEntry } from "./references";
import { PerimeterNode, PERSON_W } from "./PerimeterNode";
import { ConfirmPopover } from "./ConfirmPopover";
import { IconPicker, lookupIcon } from "./IconPicker";
import { STATUS_COLORS } from "./statusColors";
import type { Editor } from "./editor";
import {
  FIELD_CLASS,
  FLIP_MS,
  FieldGroup,
  LABEL_CLASS,
  ResponsibilitiesEditor,
} from "./EntryCard";

export interface GridHighlight {
  rect: Rect;
  valid: boolean;
  inset?: boolean;
}

/** Logical pixel dimensions — multiplied by zoom at render time. */
export const LABEL_H = 20;
const RESIZE_SIZE = 24;
const RESP_H = 22;

function GroupOverlay({
  group,
  view,
  editor,
}: {
  group: GroupView;
  view: SurfaceView;
  editor?: Editor;
}) {
  const { beginResize, beginGroupDrag, heldId, resizingId, resizeRejected } = useContext(GridContext);
  const zoom = useZoom();
  const region = groupRect(group);
  const depth = groupDepth(view, group);
  const ghost = heldId === group.id;
  const resizing = resizingId === group.id;
  const rejection = resizing ? resizeRejected : null;

  const left = region.col * CELL_W * zoom;
  const top = region.row * CELL_H * zoom;
  const width = region.w * CELL_W * zoom;
  const height = region.h * CELL_H * zoom;
  const hdrRows = groupHeaderRows(group);
  const headerH = hdrRows * CELL_H * zoom;
  const resizeSize = RESIZE_SIZE * zoom;
  const opacity = ghost ? 0.4 : 1;
  const headerZ = 11 + depth;
  const resizeZ = 11 + depth;
  const responsibilities = group.responsibilities ?? [];
  const canEdit = !!editor;
  const headerRef = useRef<HTMLDivElement | null>(null);
  const [editing, setEditing] = useState(false);
  const [sourceRect, setSourceRect] = useState<DOMRect | null>(null);

  const startEdit = () => {
    if (headerRef.current) {
      setSourceRect(headerRef.current.getBoundingClientRect());
    }
    setEditing(true);
  };

  const GroupIcon = lookupIcon(group.icon);

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
      {/* full-row header — occupies the first row of the group */}
      <div
        ref={headerRef}
        className="group/grouphdr"
        onDoubleClick={canEdit ? (e) => { e.stopPropagation(); startEdit(); } : undefined}
        style={{
          position: "absolute",
          left,
          top,
          width,
          height: headerH,
          zIndex: headerZ,
          opacity,
          display: "flex",
          flexDirection: "column",
          borderRadius: `${12 * zoom}px ${12 * zoom}px 0 0`,
          overflow: "hidden",
          fontSize: 12 * zoom,
          backgroundColor: `color-mix(in srgb, black ${10 + depth * 6}%, transparent)`,
          borderBottom: `${1 * zoom}px solid color-mix(in srgb, var(--text-muted) 25%, transparent)`,
        }}
      >
        {/* name bar — entire bar is the drag handle */}
        <div
          data-group-pickup={group.id}
          onPointerDown={(e) => {
            e.stopPropagation();
            beginGroupDrag(group.id, e);
          }}
          className="cursor-grab active:cursor-grabbing"
          style={{
            display: "flex",
            alignItems: "center",
            flexShrink: 0,
            gap: 8 * zoom,
            height: 30 * zoom,
            padding: `0 ${12 * zoom}px`,
          }}
        >
          <svg
            width={4 * zoom}
            height={10 * zoom}
            viewBox="0 0 4 10"
            className="opacity-30 group-hover/grouphdr:opacity-60 transition-opacity"
          >
            <circle cx="1" cy="1" r="0.75" fill="var(--text-muted)" />
            <circle cx="3" cy="1" r="0.75" fill="var(--text-muted)" />
            <circle cx="1" cy="5" r="0.75" fill="var(--text-muted)" />
            <circle cx="3" cy="5" r="0.75" fill="var(--text-muted)" />
            <circle cx="1" cy="9" r="0.75" fill="var(--text-muted)" />
            <circle cx="3" cy="9" r="0.75" fill="var(--text-muted)" />
          </svg>
          {GroupIcon && (
            <GroupIcon
              className="shrink-0 text-[var(--text-muted)]"
              style={{ width: 12 * zoom, height: 12 * zoom }}
            />
          )}
          <span
            className="font-semibold uppercase text-[var(--text-muted)] truncate"
            style={{
              fontSize: 11 * zoom,
              letterSpacing: 0.1 * zoom + "em",
            }}
          >
            {group.name || "Untitled"}
          </span>
          <span className="flex-1" />
          {canEdit && (
            <button
              type="button"
              data-no-pickup
              onPointerDown={(e) => e.stopPropagation()}
              onClick={(e) => {
                e.stopPropagation();
                startEdit();
              }}
              className="opacity-0 group-hover/grouphdr:opacity-100 text-[var(--text-ghost)] hover:text-[var(--text-secondary)] transition-opacity"
              style={{ padding: 2 * zoom }}
              aria-label={`Edit group ${group.name}`}
            >
              <Pencil style={{ width: 12 * zoom, height: 12 * zoom }} />
            </button>
          )}
        </div>
        {group.description && (
          <div
            className="text-[var(--text-muted)]"
            style={{
              fontSize: 11 * zoom,
              lineHeight: 1.3,
              padding: `${4 * zoom}px ${12 * zoom}px`,
            }}
          >
            {group.description}
          </div>
        )}
        {/* responsibilities — vertical list, view-only */}
        <div
          style={{
            flex: 1,
            padding: `0 ${12 * zoom}px`,
            overflow: "hidden",
          }}
        >
          {responsibilities.map((r) => {
            const color = r.status
              ? STATUS_COLORS[r.status].dot
              : "bg-[var(--text-ghost)]";
            return (
              <div
                key={r.id}
                className="flex items-start"
                style={{ gap: 8 * zoom, minHeight: RESP_H * zoom }}
              >
                <span
                  className={`shrink-0 rounded-full ${color}`}
                  style={{ width: 6 * zoom, height: 6 * zoom, marginTop: 5 * zoom }}
                />
                <span
                  className="flex-1 truncate text-[var(--text-secondary)]"
                  style={{ fontSize: 11 * zoom }}
                >
                  {r.statement}
                </span>
              </div>
            );
          })}
        </div>
      </div>
      {editing && canEdit &&
        createPortal(
          <EditGroupModal
            group={group}
            editor={editor!}
            sourceRect={sourceRect}
            sourceZoom={zoom}
            onExit={() => setEditing(false)}
          />,
          document.body,
        )}
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

// ---------------------------------------------------------------------------
// Group edit modal — same FLIP / portal pattern as the node EditModal.
// ---------------------------------------------------------------------------

interface GroupDraft {
  name: string;
  description: string;
  icon?: string;
  responsibilities: Responsibility[];
}

function initialGroupDraft(group: GroupView): GroupDraft {
  return {
    name: group.name ?? "",
    description: group.description ?? "",
    icon: group.icon,
    responsibilities: (group.responsibilities ?? []).map((r) => ({ ...r })),
  };
}

function EditGroupModal({
  group,
  editor,
  sourceRect,
  sourceZoom,
  onExit,
}: {
  group: GroupView;
  editor: Editor;
  sourceRect: DOMRect | null;
  sourceZoom: number;
  onExit: () => void;
}) {
  const [draft, setDraft] = useState<GroupDraft>(() => initialGroupDraft(group));
  const [iconAnchor, setIconAnchor] = useState<DOMRect | null>(null);
  const [deleteRect, setDeleteRect] = useState<DOMRect | null>(null);
  const [backdropOn, setBackdropOn] = useState(false);
  const [closing, setClosing] = useState(false);
  const iconBtnRef = useRef<HTMLButtonElement | null>(null);
  const deleteBtnRef = useRef<HTMLButtonElement | null>(null);
  const modalRef = useRef<HTMLDivElement>(null);

  const OverrideIcon = lookupIcon(draft.icon);

  const commitDraft = () => {
    editor.updateGroup(group.id, {
      name: draft.name.trim(),
      description: draft.description.trim() || undefined,
      icon: draft.icon,
      responsibilities: draft.responsibilities,
    });
  };

  useLayoutEffect(() => {
    const el = modalRef.current;
    if (!el) return;
    if (!sourceRect) {
      el.style.opacity = "0";
      el.style.transform = "scale(0.96)";
    } else {
      const target = el.getBoundingClientRect();
      const dx =
        sourceRect.left + sourceRect.width / 2 -
        (target.left + target.width / 2);
      const dy =
        sourceRect.top + sourceRect.height / 2 -
        (target.top + target.height / 2);
      const sx = sourceRect.width / target.width;
      const sy = sourceRect.height / target.height;
      el.style.transition = "none";
      el.style.transformOrigin = "center center";
      el.style.transform = `translate(${dx}px, ${dy}px) scale(${sx}, ${sy})`;
      el.style.opacity = "0.4";
    }
    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        if (!modalRef.current) return;
        modalRef.current.style.transition = `transform ${FLIP_MS}ms cubic-bezier(0.16, 1, 0.3, 1), opacity ${FLIP_MS}ms ease-out`;
        modalRef.current.style.transform = "translate(0, 0) scale(1)";
        modalRef.current.style.opacity = "1";
      });
    });
  }, [sourceRect]);

  useEffect(() => {
    requestAnimationFrame(() => setBackdropOn(true));
  }, []);

  const beginClose = (afterClose?: () => void) => {
    if (closing) return;
    setClosing(true);
    setBackdropOn(false);
    const el = modalRef.current;
    if (!el || !sourceRect) {
      window.setTimeout(() => {
        afterClose?.();
        onExit();
      }, FLIP_MS);
      return;
    }
    const target = el.getBoundingClientRect();
    const dx =
      sourceRect.left + sourceRect.width / 2 -
      (target.left + target.width / 2);
    const dy =
      sourceRect.top + sourceRect.height / 2 -
      (target.top + target.height / 2);
    const sx = sourceRect.width / target.width;
    const sy = sourceRect.height / target.height;
    el.style.transition = `transform ${FLIP_MS}ms cubic-bezier(0.4, 0, 1, 1), opacity ${FLIP_MS}ms ease-in`;
    el.style.transform = `translate(${dx}px, ${dy}px) scale(${sx}, ${sy})`;
    el.style.opacity = "0";
    window.setTimeout(() => {
      afterClose?.();
      onExit();
    }, FLIP_MS);
  };

  const cancel = () => beginClose();
  const confirm = () => beginClose(commitDraft);

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        cancel();
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const memberCount = group.memberIds.length;

  return (
    <div
      className="fixed inset-0 z-[1000] flex items-center justify-center"
      onPointerDown={(e) => e.stopPropagation()}
      onWheel={(e) => e.stopPropagation()}
    >
      {/* backdrop */}
      <div
        className="absolute inset-0 bg-black/55 backdrop-blur-[3px]"
        style={{
          opacity: backdropOn ? 1 : 0,
          transition: `opacity ${FLIP_MS}ms ease-out`,
        }}
        onClick={cancel}
      />

      {/* source highlight — ring around the group header */}
      {sourceRect && (
        <div
          style={{
            position: "fixed",
            left: sourceRect.left,
            top: sourceRect.top,
            width: sourceRect.width,
            height: sourceRect.height,
            pointerEvents: "none",
            opacity: backdropOn ? 0.55 : 0,
            transition: `opacity ${FLIP_MS}ms ease-out`,
            borderRadius: `${12 * sourceZoom}px ${12 * sourceZoom}px 0 0`,
            boxShadow:
              "0 0 0 2px var(--text-secondary), 0 0 0 6px rgba(255,255,255,0.05), 0 0 36px 4px rgba(255,255,255,0.12)",
          }}
        />
      )}

      {/* modal card */}
      <div
        ref={modalRef}
        className="relative flex flex-col overflow-hidden bg-[var(--surface-raised)] border border-[var(--border)] shadow-2xl"
        style={{
          width: "min(540px, 92vw)",
          maxHeight: "85vh",
          borderRadius: 10,
          fontSize: 13,
        }}
        onPointerDown={(e) => e.stopPropagation()}
        onClick={(e) => e.stopPropagation()}
      >
        {/* header */}
        <div
          className="flex shrink-0 items-center border-b border-[var(--border-subtle)]"
          style={{ gap: 10, padding: "12px 16px" }}
        >
          <button
            ref={iconBtnRef}
            type="button"
            onClick={(e) => {
              e.stopPropagation();
              const r = iconBtnRef.current?.getBoundingClientRect();
              if (r) setIconAnchor(r);
            }}
            className="shrink-0 cursor-pointer text-[var(--text-muted)] hover:text-[var(--text-secondary)]"
            aria-label="Change icon"
          >
            {OverrideIcon ? (
              <OverrideIcon style={{ width: 18, height: 18 }} />
            ) : (
              <span
                className="block border border-dashed border-[var(--border)] rounded"
                style={{ width: 18, height: 18 }}
                title="Pick icon"
              />
            )}
          </button>
          <span
            className="flex-1 truncate font-semibold uppercase text-[var(--text)]"
            style={{ fontSize: 14, letterSpacing: "0.06em" }}
          >
            {draft.name || group.name || "Untitled group"}
          </span>
          <span
            className="shrink-0 text-[var(--text-muted)]"
            style={{ fontSize: 11 }}
          >
            {memberCount} {memberCount === 1 ? "member" : "members"}
          </span>
        </div>

        {/* body */}
        <div className="flex-1 overflow-auto" style={{ padding: "16px 16px 8px" }}>
          <FieldGroup>
            <label className={LABEL_CLASS} htmlFor={`edit-group-name-${group.id}`}>
              Name
            </label>
            <input
              id={`edit-group-name-${group.id}`}
              type="text"
              autoFocus
              value={draft.name}
              onChange={(e) => setDraft({ ...draft, name: e.target.value })}
              onKeyDown={(e) => {
                if (e.key === "Enter") confirm();
              }}
              placeholder="Untitled group"
              className={FIELD_CLASS}
              style={{
                fontSize: 13,
                padding: "8px 12px",
                borderRadius: 6,
                marginTop: 6,
              }}
            />
          </FieldGroup>

          <FieldGroup>
            <label className={LABEL_CLASS} htmlFor={`edit-group-desc-${group.id}`}>
              Description
            </label>
            <textarea
              id={`edit-group-desc-${group.id}`}
              value={draft.description}
              onChange={(e) => setDraft({ ...draft, description: e.target.value.slice(0, 200) })}
              onKeyDown={(e) => {
                if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) confirm();
              }}
              maxLength={200}
              placeholder="What does this group bundle together?"
              rows={3}
              className={`${FIELD_CLASS} resize-y`}
              style={{
                fontSize: 13,
                lineHeight: "20px",
                padding: "8px 12px",
                borderRadius: 6,
                marginTop: 6,
              }}
            />
            <div
              className="text-right text-[var(--text-ghost)]"
              style={{ fontSize: 10, marginTop: 2 }}
            >
              {draft.description.length}/200
            </div>
          </FieldGroup>

          <FieldGroup>
            <label className={LABEL_CLASS}>Responsibilities</label>
            <div style={{ marginTop: 6 }}>
              <ResponsibilitiesEditor
                value={draft.responsibilities}
                onChange={(responsibilities) =>
                  setDraft({ ...draft, responsibilities })
                }
                onCommit={confirm}
              />
            </div>
          </FieldGroup>
        </div>

        {/* footer */}
        <div
          className="shrink-0 flex items-center border-t border-[var(--border-subtle)] bg-[var(--surface-canvas)]"
          style={{ gap: 8, padding: "10px 16px" }}
        >
          <button
            ref={deleteBtnRef}
            type="button"
            onClick={() => {
              const r = deleteBtnRef.current?.getBoundingClientRect();
              if (r) setDeleteRect(r);
            }}
            className="flex cursor-pointer items-center text-[var(--text-muted)] hover:text-red-400"
            style={{
              gap: 6,
              fontSize: 12,
              padding: "6px 10px",
              borderRadius: 6,
            }}
          >
            <Trash2 style={{ width: 14, height: 14 }} />
            Delete group
          </button>
          <span className="flex-1" />
          <button
            type="button"
            onClick={cancel}
            className="cursor-pointer border border-[var(--border)] text-[var(--text-secondary)] hover:bg-[var(--surface-hover)]"
            style={{
              fontSize: 13,
              padding: "6px 14px",
              borderRadius: 6,
            }}
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={confirm}
            className="cursor-pointer font-semibold bg-[var(--text-secondary)] text-[var(--surface-canvas)] hover:bg-[var(--text)]"
            style={{
              fontSize: 13,
              padding: "6px 14px",
              borderRadius: 6,
            }}
          >
            Confirm
          </button>
        </div>

        {iconAnchor && (
          <IconPicker
            anchorRect={iconAnchor}
            current={draft.icon}
            onPick={(name) => {
              setDraft({ ...draft, icon: name });
              setIconAnchor(null);
            }}
            onClose={() => setIconAnchor(null)}
          />
        )}
        {deleteRect && (
          <ConfirmPopover
            anchorRect={deleteRect}
            label={`Delete group "${group.name || "Untitled"}"? Members stay on the canvas (they keep their positions).`}
            onConfirm={() => {
              setDeleteRect(null);
              beginClose(() => editor.deleteGroup(group.id));
            }}
            onCancel={() => setDeleteRect(null)}
          />
        )}
      </div>
    </div>
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
  view,
  highlight,
  levels = [],
  context,
  onRingClick,
  renderEntry,
  editor,
  emptyContent,
  measuredSpans,
  onMeasure,
  onFixOverlaps,
}: {
  view: SurfaceView;
  highlight?: GridHighlight | null;
  /** Levels outer-most first; the last entry is the current surface. */
  levels?: LevelInfo[];
  context?: SurfaceContext;
  onRingClick?: (ancestorIndex: number) => void;
  renderEntry: (node: NodeView) => ReactNode;
  editor?: Editor;
  emptyContent?: ReactNode;
  measuredSpans: ReadonlyMap<string, Span>;
  onMeasure?: (spans: Map<string, Span>) => void;
  onFixOverlaps?: (measuredSpans: ReadonlyMap<string, Span>) => void;
}) {
  const zoom = useZoom();
  const cellW = CELL_W * zoom;
  const cellH = CELL_H * zoom;
  const inset = ITEM_INSET * zoom;
  const entries = view.entries.filter(
    (n) => n.kind !== "person" || view.altitude === "system",
  );

  // -- Measure-after-render card sizing --
  // Cards render with h-full (filling the grid cell). After each render,
  // useLayoutEffect temporarily sets height:auto on each card, reads the
  // natural content height, and computes the correct cell span. Because
  // useLayoutEffect blocks paint, the user never sees the measuring frame.
  const wrapperRefs = useRef(new Map<string, HTMLDivElement>());

  const getSpan = (node: NodeView): Span =>
    measuredSpans.get(node.id) ?? cardSpan(node);

  const placed = entries.map((node) => ({
    node,
    span: getSpan(node),
    cell: node.cell ?? { row: 0, col: 0 },
  }));

  useLayoutEffect(() => {
    const cards: [string, HTMLElement, number][] = [];
    for (const node of entries) {
      const wrapper = wrapperRefs.current.get(node.id);
      const card = wrapper?.firstElementChild as HTMLElement | null;
      if (card) cards.push([node.id, card, cardWidthCells(node)]);
    }
    if (cards.length === 0) return;

    for (const [, card] of cards) card.style.height = "auto";

    const next = new Map<string, Span>();
    for (const [id, card, w] of cards) {
      next.set(id, spanFromMeasuredHeight(card.offsetHeight / zoom, w));
    }

    for (const [, card] of cards) card.style.height = "";

    let changed = next.size !== measuredSpans.size;
    if (!changed) {
      for (const [id, s] of next) {
        const p = measuredSpans.get(id);
        if (!p || p.w !== s.w || p.h !== s.h) { changed = true; break; }
      }
    }
    if (changed) onMeasure?.(next);
  });

  // After measurement, check if any cards now overlap and notify the parent.
  const entriesRef = useRef(entries);
  entriesRef.current = entries;
  const fixOverlapsRef = useRef(onFixOverlaps);
  fixOverlapsRef.current = onFixOverlaps;
  useEffect(() => {
    if (measuredSpans.size === 0 || !fixOverlapsRef.current) return;
    const cur = entriesRef.current;
    const items = cur
      .filter((n) => n.cell)
      .map((n) => {
        const span = measuredSpans.get(n.id) ?? cardSpan(n);
        return { row: n.cell!.row, col: n.cell!.col, w: span.w, h: span.h };
      });
    for (let i = 0; i < items.length; i++) {
      for (let j = i + 1; j < items.length; j++) {
        if (rectsOverlap(items[i], items[j])) {
          fixOverlapsRef.current(measuredSpans);
          return;
        }
      }
    }
  }, [measuredSpans]);

  const sortedGroups = [...view.groups].sort(
    (a, b) => groupDepth(view, a) - groupDepth(view, b),
  );

  // Content extent. Perimeter follows the grid container.
  let contentCols = 0,
    contentRows = 0;
  for (const { span, cell } of placed) {
    contentCols = Math.max(contentCols, cell.col + span.w);
    contentRows = Math.max(contentRows, cell.row + span.h);
  }
  for (const g of view.groups) {
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
            key={ce.node.id}
            node={ce.node}
            variant={
              ce.node.kind === "person"
                ? "person"
                : ce.node.external
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
              onClick={isClickable ? () => onRingClick(i - 1) : undefined}
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
        <GroupOverlay key={g.id} group={g} view={view} editor={editor} />
      ))}

      {/* node cards */}
      {placed.map(({ node, span, cell }) => (
        <div
          key={node.id}
          ref={(el) => {
            if (el) wrapperRefs.current.set(node.id, el);
            else wrapperRefs.current.delete(node.id);
          }}
          style={{
            gridColumn: `${cell.col + 1} / span ${span.w}`,
            gridRow: `${cell.row + 1} / span ${span.h}`,
            padding: inset,
            zIndex: 10,
          }}
        >
          {renderEntry(node)}
        </div>
      ))}

      {/* empty-level CTA */}
      {entries.length === 0 && emptyContent && (
        <div
          style={{
            position: "absolute",
            inset: 0,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            pointerEvents: "none",
            zIndex: 5,
          }}
        >
          <div style={{ pointerEvents: "auto" }}>{emptyContent}</div>
        </div>
      )}

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
