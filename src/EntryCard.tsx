import { useContext, useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import {
  ArrowRight,
  ArrowLeft,
  ChevronDown,
  ChevronRight,
  Pencil,
  Plus,
  Trash2,
  X,
} from "lucide-react";
import type {
  Kind,
  ModelProperty,
  NodeView,
  Responsibility,
} from "./viewmodel";
import type { Span } from "./pack";
import {
  cardSpan,
  CARD_HEADER_H,
  RESP_LINE_H,
  RESP_PAD,
  LINK_ROW_H,
  LINK_PAD,
} from "./pack";
import { STATUS_COLORS } from "./statusColors";
import type { Status } from "./statusColors";
import { GridContext } from "./gridcontext";
import { ModelContext } from "./modelcontext";
import { useZoom, ZoomContext } from "./PanZoom";
import { findNode } from "./references";
import { effectiveRespStatus } from "./rollup";
import { tokenIcon } from "./tokens";
import { ConfirmPopover } from "./ConfirmPopover";
import { IconPicker, lookupIcon } from "./IconPicker";
import type { Editor } from "./editor";

function LinkLine({
  direction,
  links,
  onLinkClick,
}: {
  direction: "out" | "in";
  links: { partnerId: string; label: string; method?: string }[];
  onLinkClick?: (partnerId: string) => void;
}) {
  const model = useContext(ModelContext);
  const zoom = useZoom();
  const Arrow = direction === "out" ? ArrowRight : ArrowLeft;
  return (
    <div
      className="flex items-center flex-wrap"
      style={{
        gap: 6 * zoom,
        minHeight: LINK_ROW_H * zoom,
        fontSize: 10 * zoom,
        lineHeight: `${15 * zoom}px`,
      }}
    >
      <Arrow
        className="shrink-0 text-[var(--text-ghost)]"
        style={{ width: 10 * zoom, height: 10 * zoom }}
      />
      {links.map((l, i) => {
        const partner = findNode(model, l.partnerId);
        const name = partner?.name ?? l.partnerId;
        return (
          <span key={l.partnerId} className="inline shrink-0">
            {i > 0 && (
              <span
                className="text-[var(--text-ghost)]"
                style={{ marginRight: 4 * zoom }}
              >
                ·
              </span>
            )}
            <span
              data-no-pickup={onLinkClick ? "" : undefined}
              role={onLinkClick ? "button" : undefined}
              tabIndex={onLinkClick ? 0 : undefined}
              title={(() => {
                const parts: string[] = [name];
                if (l.label) parts.push(l.label);
                if (l.method) parts.push(`[${l.method}]`);
                return parts.join(" — ");
              })()}
              onClick={
                onLinkClick
                  ? (e) => {
                      e.stopPropagation();
                      onLinkClick(l.partnerId);
                    }
                  : undefined
              }
              className={`font-medium text-[var(--text-secondary)] ${
                onLinkClick ? "cursor-pointer hover:text-[var(--text)] hover:underline" : ""
              }`}
            >
              {name}
            </span>
          </span>
        );
      })}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Top-level card: view rendering in the grid + a modal portal for editing.
// ---------------------------------------------------------------------------

export function EntryCardView(props: {
  node: NodeView;
  span: Span;
  onNavigate?: (nodeId: string) => void;
  onLinkClick?: (partnerId: string) => void;
  pickupId?: string;
  ghost?: boolean;
  lifted?: boolean;
  editor?: Editor;
}) {
  const zoom = useZoom();
  const [editing, setEditing] = useState(false);
  const [sourceRect, setSourceRect] = useState<DOMRect | null>(null);
  const cardRef = useRef<HTMLDivElement>(null);
  const canEdit = !!props.editor && !props.lifted && !props.ghost;

  const startEdit = () => {
    if (cardRef.current) {
      setSourceRect(cardRef.current.getBoundingClientRect());
    }
    setEditing(true);
  };

  return (
    <>
      <ViewCard
        {...props}
        cardRef={cardRef}
        onStartEdit={canEdit ? startEdit : undefined}
      />
      {editing && canEdit &&
        createPortal(
          <EditModal
            node={props.node}
            editor={props.editor!}
            sourceRect={sourceRect}
            sourceZoom={zoom}
            onExit={() => setEditing(false)}
          />,
          document.body,
        )}
    </>
  );
}

// ---------------------------------------------------------------------------
// View mode — non-interactive aside from drag, Open navigation, and the
// pencil that enters edit mode.
// ---------------------------------------------------------------------------

function ViewCard({
  node,
  onNavigate,
  pickupId,
  ghost = false,
  lifted = false,
  onStartEdit,
  cardRef,
}: {
  node: NodeView;
  span: Span;
  onNavigate?: (nodeId: string) => void;
  onLinkClick?: (partnerId: string) => void;
  pickupId?: string;
  ghost?: boolean;
  lifted?: boolean;
  editor?: Editor;
  onStartEdit?: () => void;
  cardRef?: React.Ref<HTMLDivElement>;
}) {
  const zoom = useZoom();
  const isPerson = node.kind === "person";
  const isExternal = node.external === true;
  const isCodeKind = node.kind === "operation" || node.kind === "model";
  const isModelKind = node.kind === "model";
  const navigable = node.kind !== "person" && !isCodeKind && !isExternal;

  const isDeprecated = node.deprecated === true;
  const isRelocated = node.relocated === true;

  const shellBase = lifted
    ? "shadow-2xl border-solid border-[var(--border-strong)]"
    : ghost
      ? "opacity-40 border-dashed border-blue-400"
      : isDeprecated
        ? "shadow-sm border-dashed border-red-400/60"
        : isRelocated
          ? "shadow-sm border-dashed border-violet-400/60"
          : isExternal
            ? "shadow-sm border-dashed border-[var(--border-strong)]"
            : "shadow-md border-solid border-[var(--border)]";

  const incoming = node._incomingLinks;
  const outgoing = node._outgoingLinks;
  const hasLinks =
    outgoing.length > 0 || (!isModelKind && incoming.length > 0);
  const nameClass = isCodeKind
    ? "font-mono font-semibold text-[var(--text)]"
    : "font-semibold text-[var(--text)]";

  const OverrideIcon = lookupIcon(node.icon);
  const Icon = OverrideIcon ?? tokenIcon(node.id);

  const properties = node.properties ?? [];
  const responsibilities = node.responsibilities;

  // Double-click the body to enter edit mode (matches the user's "no
  // interaction with the card except drag" preference — clicks do nothing,
  // double-click is the gesture for editing).
  const handleDoubleClick = onStartEdit
    ? (e: React.MouseEvent) => {
        e.stopPropagation();
        onStartEdit();
      }
    : undefined;

  return (
    <div
      ref={cardRef}
      className={`group/card relative flex h-full w-full flex-col overflow-hidden bg-[var(--surface-raised)] ${shellBase}`}
      style={{
        borderRadius: 12 * zoom,
        borderWidth: (ghost ? 2 : 1) * zoom,
        fontSize: 12 * zoom,
        opacity: isDeprecated && !lifted && !ghost ? 0.55 : undefined,
      }}
      onDoubleClick={handleDoubleClick}
    >
      {/* identity row */}
      <div
        data-pickup={pickupId}
        className={`flex shrink-0 items-center border-b border-[var(--border-subtle)] ${
          pickupId && !ghost
            ? "cursor-grab hover:bg-[var(--surface-hover)]"
            : ""
        }`}
        style={{
          gap: 8 * zoom,
          height: CARD_HEADER_H * zoom,
          padding: `0 ${12 * zoom}px`,
        }}
      >
        <Icon
          className="shrink-0 text-[var(--text-muted)]"
          style={{ width: 14 * zoom, height: 14 * zoom }}
        />
        {(isPerson || isExternal) && (
          <span
            className="shrink-0 font-semibold uppercase text-[var(--text-ghost)]"
            style={{
              fontSize: 9 * zoom,
              letterSpacing: 0.08 * zoom + "em",
              padding: `${1 * zoom}px ${5 * zoom}px`,
              border: `${zoom}px solid var(--border-subtle)`,
              borderRadius: 4 * zoom,
            }}
          >
            {isPerson ? "Person" : "External"}
          </span>
        )}
        <span className="flex-1 truncate" style={{ fontSize: 13 * zoom }}>
          <span className={nameClass}>{node.name || "Untitled"}</span>
          {node.kind === "operation" && <span className={nameClass}>()</span>}
          {!isCodeKind && node.technology && (
            <span
              className="italic text-[var(--text-muted)]"
              style={{ fontSize: 11 * zoom, marginLeft: 6 * zoom }}
            >
              {node.technology}
            </span>
          )}
        </span>
        {isDeprecated && (
          <span
            className="shrink-0 font-semibold uppercase text-red-400/80"
            style={{
              fontSize: 9.5 * zoom,
              letterSpacing: 0.14 * zoom + "em",
            }}
          >
            Deprecated
          </span>
        )}
        {isRelocated && (
          <span
            className="shrink-0 font-semibold uppercase text-violet-400/80"
            style={{
              fontSize: 9.5 * zoom,
              letterSpacing: 0.14 * zoom + "em",
            }}
          >
            Relocated
          </span>
        )}
        {isExternal && !isDeprecated && !isRelocated && (
          <span
            className="shrink-0 font-semibold uppercase text-[var(--text-ghost)]"
            style={{
              fontSize: 9.5 * zoom,
              letterSpacing: 0.14 * zoom + "em",
            }}
          >
            External
          </span>
        )}
        {onStartEdit && (
          <button
            type="button"
            data-no-pickup
            onClick={(e) => {
              e.stopPropagation();
              onStartEdit();
            }}
            aria-label={`Edit ${node.name}`}
            className="flex shrink-0 cursor-pointer items-center opacity-0 group-hover/card:opacity-100 text-[var(--text-ghost)] hover:text-[var(--text-secondary)] transition-opacity"
            style={{ padding: `${2 * zoom}px` }}
          >
            <Pencil style={{ width: 12 * zoom, height: 12 * zoom }} />
          </button>
        )}
        {navigable && (
          <button
            type="button"
            data-no-pickup
            onClick={(e) => {
              e.stopPropagation();
              onNavigate?.(node.id);
            }}
            aria-label={`${node._childCount} inside ${node.name}`}
            className="flex shrink-0 cursor-pointer items-center font-medium text-[var(--text-muted)] hover:text-[var(--text-secondary)]"
            style={{
              gap: 2 * zoom,
              padding: `${2 * zoom}px ${4 * zoom}px`,
              fontSize: 11 * zoom,
              borderRadius: 4 * zoom,
            }}
          >
            {node._childCount > 0 && node._childCount}
            <ChevronRight style={{ width: 12 * zoom, height: 12 * zoom }} />
          </button>
        )}
      </div>

      {/* description */}
      {node.description && (
        <div
          className="shrink-0 text-[var(--text-muted)]"
          style={{
            padding: `${6 * zoom}px ${12 * zoom}px`,
            fontSize: 11 * zoom,
            lineHeight: `${16 * zoom}px`,
          }}
        >
          {node.description}
        </div>
      )}

      {/* model properties */}
      {isModelKind && properties.length > 0 && (
        <div
          className="border-t border-[var(--border-subtle)]"
          style={{ padding: `${(RESP_PAD / 2) * zoom}px ${12 * zoom}px` }}
        >
          {properties.map((p, i) => (
            <div
              key={i}
              style={{ minHeight: RESP_LINE_H * zoom, lineHeight: `${16 * zoom}px`, paddingTop: 4 * zoom, paddingBottom: 4 * zoom }}
            >
              <span
                className="font-mono font-medium text-[var(--text-secondary)]"
                style={{ fontSize: 12 * zoom }}
              >
                {p.label}
              </span>
              {p.description && (
                <>
                  {" "}
                  <span
                    className="text-[var(--text-muted)]"
                    style={{ fontSize: 11 * zoom }}
                  >
                    {p.description}
                  </span>
                </>
              )}
            </div>
          ))}
        </div>
      )}

      {/* responsibilities */}
      {!isModelKind && responsibilities.length > 0 && (
        <div
          className="border-t border-[var(--border-subtle)]"
          style={{ padding: `${(RESP_PAD / 2) * zoom}px ${12 * zoom}px` }}
        >
          {responsibilities.map((r) => {
            const eff = isExternal || isPerson ? undefined : effectiveRespStatus(node, r);
            const colors = eff ? STATUS_COLORS[eff] : null;
            const rules = r.implementationRules ?? [];
            return (
              <div
                key={r.id}
                className="flex items-start"
                style={{ gap: 8 * zoom, minHeight: RESP_LINE_H * zoom }}
              >
                <span
                  className={`shrink-0 rounded-full ${
                    colors ? colors.dot : "bg-[var(--text-ghost)]"
                  }`}
                  style={{
                    width: 6 * zoom,
                    height: 6 * zoom,
                    marginTop: 5 * zoom,
                  }}
                  title={colors?.label}
                  aria-label={colors?.label}
                />
                <div className="flex-1" style={{ minWidth: 0 }}>
                  <span
                    className="text-[var(--text-secondary)]"
                    style={{
                      fontSize: 11 * zoom,
                      lineHeight: `${16 * zoom}px`,
                    }}
                  >
                    {r.statement}
                  </span>
                  {rules.map((rule, ri) => (
                    <div
                      key={ri}
                      className="text-[var(--text-ghost)]"
                      style={{
                        fontSize: 10 * zoom,
                        lineHeight: `${14 * zoom}px`,
                      }}
                    >
                      {rule}
                    </div>
                  ))}
                </div>
              </div>
            );
          })}
        </div>
      )}

      {/* links */}
      {hasLinks && (
        <div
          className="shrink-0 border-t border-[var(--border-subtle)]"
          style={{ padding: `${(LINK_PAD / 2) * zoom}px ${12 * zoom}px` }}
        >
          {outgoing.length > 0 && (
            <LinkLine
              direction="out"
              links={outgoing.map((l) => ({
                partnerId: l.dst,
                label: l.label,
                method: l.method,
              }))}
            />
          )}
          {!isModelKind && incoming.length > 0 && (
            <LinkLine
              direction="in"
              links={incoming.map((l) => ({
                partnerId: l.src,
                label: l.label,
                method: l.method,
              }))}
            />
          )}
        </div>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Edit mode — full-screen modal portal. Pops out of the grid; sized to its
// own content, not the cell. Live-buffered draft state, single commit on
// Confirm.
// ---------------------------------------------------------------------------

interface Draft {
  name: string;
  kind: Kind;
  description: string;
  technology: string;
  icon?: string;
  deprecated: boolean;
  relocated: boolean;
  responsibilities: Responsibility[];
  properties: ModelProperty[];
}

function initialDraft(node: NodeView): Draft {
  return {
    name: node.name ?? "",
    kind: node.kind,
    description: node.description ?? "",
    technology: node.technology ?? "",
    icon: node.icon,
    deprecated: node.deprecated === true,
    relocated: node.relocated === true,
    responsibilities: (node.responsibilities ?? []).map((r) => ({ ...r })),
    properties: (node.properties ?? []).map((p) => ({ ...p })),
  };
}

// Duration of the open/close FLIP animation. Both ms (for setTimeout) and the
// CSS transition need to agree.
export const FLIP_MS = 240;

export const FIELD_CLASS =
  "w-full bg-[var(--surface-canvas)] border border-[var(--border)] " +
  "text-[var(--text)] placeholder:text-[var(--text-muted)] " +
  "outline-none focus:border-[var(--text-muted)] " +
  "focus:ring-2 focus:ring-[var(--text-ghost)]/30 transition-colors";

export const LABEL_CLASS =
  "block text-[10.5px] font-semibold uppercase tracking-wider " +
  "text-[var(--text-muted)]";

function EditModal({
  node,
  editor,
  sourceRect,
  sourceZoom,
  onExit,
}: {
  node: NodeView;
  editor: Editor;
  sourceRect: DOMRect | null;
  sourceZoom: number;
  onExit: () => void;
}) {
  const [draft, setDraft] = useState<Draft>(() => initialDraft(node));
  const [iconAnchor, setIconAnchor] = useState<DOMRect | null>(null);
  const [deleteRect, setDeleteRect] = useState<DOMRect | null>(null);
  const [backdropOn, setBackdropOn] = useState(false);
  const [closing, setClosing] = useState(false);
  const iconBtnRef = useRef<HTMLButtonElement | null>(null);
  const deleteBtnRef = useRef<HTMLButtonElement | null>(null);
  const modalRef = useRef<HTMLDivElement>(null);

  const isCodeKind = draft.kind === "operation" || draft.kind === "model";
  const isModelKind = draft.kind === "model";
  const isPerson = draft.kind === "person";

  const OverrideIcon = lookupIcon(draft.icon);
  const Icon = OverrideIcon ?? tokenIcon(node.id);

  const validKinds: Kind[] = node.parentId
    ? node.kind === "operation" || node.kind === "model"
      ? ["operation", "model"]
      : [node.kind]
    : ["person", "system"];

  const commitDraft = () => {
    editor.updateNode(node.id, {
      name: draft.name.trim(),
      kind: draft.kind !== node.kind ? draft.kind : undefined,
      description: draft.description.trim() || undefined,
      technology: !isCodeKind
        ? draft.technology.trim() || undefined
        : undefined,
      icon: draft.icon,
      deprecated: draft.deprecated || undefined,
      relocated: draft.relocated || undefined,
      responsibilities: isModelKind ? undefined : draft.responsibilities,
      properties: isModelKind ? draft.properties : undefined,
    });
  };

  // FLIP open: position modal at sourceRect synchronously before paint so
  // there's no one-frame flash at center, then on the next frame transition
  // to its natural centered position. We use transform so we don't disturb
  // the modal's own layout (auto height / flex).
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
    // Two RAFs so the browser has fully applied the initial styles before
    // we kick off the transition.
    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        if (!modalRef.current) return;
        modalRef.current.style.transition = `transform ${FLIP_MS}ms cubic-bezier(0.16, 1, 0.3, 1), opacity ${FLIP_MS}ms ease-out`;
        modalRef.current.style.transform = "translate(0, 0) scale(1)";
        modalRef.current.style.opacity = "1";
      });
    });
  }, [sourceRect]);

  // Backdrop fade-in (after first paint).
  useEffect(() => {
    requestAnimationFrame(() => setBackdropOn(true));
  }, []);

  // Animated exit: FLIP back to source rect, then unmount.
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
      sourceRect.left + sourceRect.width / 2 - (target.left + target.width / 2);
    const dy =
      sourceRect.top + sourceRect.height / 2 - (target.top + target.height / 2);
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

  // Esc closes (cancel).
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

      {/* source-card highlight — re-render the card at its original
          position so the user can see what they're editing through the
          backdrop, dimmed (but less so than everything else) with a
          glow ring to mark it as the active subject. */}
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
            borderRadius: 12 * sourceZoom,
            boxShadow:
              "0 0 0 2px var(--text-secondary), 0 0 0 6px rgba(255,255,255,0.05), 0 0 36px 4px rgba(255,255,255,0.12)",
          }}
        >
          <ZoomContext.Provider value={sourceZoom}>
            <EntryCardView node={node} span={cardSpan(node)} lifted />
          </ZoomContext.Provider>
        </div>
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
        {/* header — icon + name + tech */}
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
            <Icon style={{ width: 18, height: 18 }} />
          </button>
          <span
            className={`flex-1 truncate ${isCodeKind ? "font-mono" : ""} font-semibold text-[var(--text)]`}
            style={{ fontSize: 16 }}
          >
            {draft.name || node.name || "New"}
          </span>
        </div>

        {/* body — labeled fields */}
        <div
          className="flex-1 overflow-auto"
          style={{ padding: "16px 16px 8px" }}
        >
          <FieldGroup>
            <label className={LABEL_CLASS} htmlFor={`edit-name-${node.id}`}>
              Name
            </label>
            <input
              id={`edit-name-${node.id}`}
              type="text"
              autoFocus
              value={draft.name}
              onChange={(e) => setDraft({ ...draft, name: e.target.value })}
              onKeyDown={(e) => {
                if (e.key === "Enter") confirm();
              }}
              placeholder="Untitled"
              className={`${FIELD_CLASS} ${isCodeKind ? "font-mono" : ""}`}
              style={{
                fontSize: 13,
                padding: "8px 12px",
                borderRadius: 6,
                marginTop: 6,
              }}
            />
          </FieldGroup>

          {validKinds.length > 1 && (
            <FieldGroup>
              <label className={LABEL_CLASS}>Kind</label>
              <div className="flex" style={{ gap: 6, marginTop: 6 }}>
                {validKinds.map((k) => (
                  <button
                    key={k}
                    type="button"
                    onClick={() => setDraft({ ...draft, kind: k })}
                    className={`capitalize ${FIELD_CLASS} text-center`}
                    style={{
                      flex: 1,
                      fontSize: 12,
                      padding: "6px 10px",
                      borderRadius: 6,
                      background: draft.kind === k ? "var(--surface-hover)" : undefined,
                      borderColor: draft.kind === k ? "var(--text-muted)" : undefined,
                    }}
                  >
                    {k}
                  </button>
                ))}
              </div>
            </FieldGroup>
          )}

          {!isCodeKind && !isPerson && (
            <FieldGroup>
              <label
                className={LABEL_CLASS}
                htmlFor={`edit-tech-${node.id}`}
              >
                Technology
              </label>
              <input
                id={`edit-tech-${node.id}`}
                type="text"
                value={draft.technology}
                onChange={(e) =>
                  setDraft({ ...draft, technology: e.target.value })
                }
                onKeyDown={(e) => {
                  if (e.key === "Enter") confirm();
                }}
                placeholder="e.g. PostgreSQL, Next.js"
                className={FIELD_CLASS}
                style={{
                  fontSize: 13,
                  padding: "8px 12px",
                  borderRadius: 6,
                  marginTop: 6,
                }}
              />
            </FieldGroup>
          )}

          <FieldGroup>
            <label className={LABEL_CLASS} htmlFor={`edit-desc-${node.id}`}>
              Description
            </label>
            <textarea
              id={`edit-desc-${node.id}`}
              value={draft.description}
              onChange={(e) =>
                setDraft({ ...draft, description: e.target.value.slice(0, 200) })
              }
              onKeyDown={(e) => {
                if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) confirm();
              }}
              maxLength={200}
              placeholder="What does this do?"
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
            <label className={LABEL_CLASS}>
              {isModelKind ? "Properties" : "Responsibilities"}
            </label>
            <div style={{ marginTop: 6 }}>
              {isModelKind ? (
                <PropertiesEditor
                  value={draft.properties}
                  onChange={(properties) => setDraft({ ...draft, properties })}
                  onCommit={confirm}
                />
              ) : (
                <ResponsibilitiesEditor
                  value={draft.responsibilities}
                  onChange={(responsibilities) =>
                    setDraft({ ...draft, responsibilities })
                  }
                  onCommit={confirm}
                  nodeId={node.id}
                  nodeKind={node.kind}
                  onMoveFrom={(fromNodeId, respId) => {
                    commitDraft();
                    editor.moveResponsibility(fromNodeId, node.id, respId);
                    onExit();
                  }}
                />
              )}
            </div>
          </FieldGroup>

          {/* refactoring flags */}
          {node.external !== true && (
            <FieldGroup>
              <label className={LABEL_CLASS}>Flags</label>
              <div className="flex flex-col" style={{ gap: 6, marginTop: 6 }}>
                <label className="flex items-center cursor-pointer" style={{ gap: 8 }}>
                  <input
                    type="checkbox"
                    checked={draft.deprecated}
                    onChange={(e) =>
                      setDraft({ ...draft, deprecated: e.target.checked })
                    }
                    className="accent-red-500"
                  />
                  <span className="text-[var(--text-secondary)]" style={{ fontSize: 13 }}>
                    Deprecated
                  </span>
                  <span className="text-[var(--text-ghost)]" style={{ fontSize: 11 }}>
                    — planned for removal
                  </span>
                </label>
                <label className="flex items-center cursor-pointer" style={{ gap: 8 }}>
                  <input
                    type="checkbox"
                    checked={draft.relocated}
                    onChange={(e) =>
                      setDraft({ ...draft, relocated: e.target.checked })
                    }
                    className="accent-violet-500"
                  />
                  <span className="text-[var(--text-secondary)]" style={{ fontSize: 13 }}>
                    Relocated
                  </span>
                  <span className="text-[var(--text-ghost)]" style={{ fontSize: 11 }}>
                    — reparented, code needs to move
                  </span>
                </label>
              </div>
            </FieldGroup>
          )}
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
            Delete
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
            label={`Delete "${node.name || "Untitled"}"?`}
            onConfirm={() => {
              setDeleteRect(null);
              beginClose(() => editor.deleteNode(node.id));
            }}
            onCancel={() => setDeleteRect(null)}
          />
        )}
      </div>
    </div>
  );
}

export function FieldGroup({ children }: { children: React.ReactNode }) {
  return <div style={{ marginBottom: 14 }}>{children}</div>;
}

/**
 * Valid manual status transitions. `relocated` and `changed` are automatic
 * (set by move operations and edits respectively). Can't demote to `proposed`.
 */
function manualStatusOptions(current: Status | undefined): Status[] {
  const s = current ?? "proposed";
  switch (s) {
    case "proposed":
      return ["implemented", "verified"];
    case "implemented":
      return ["verified", "vagrant"];
    case "changed":
      return ["verified", "vagrant"];
    case "verified":
      return ["vagrant"];
    case "relocated":
      return [];
    case "vagrant":
      return [];
    default:
      return [];
  }
}

function StatusPicker({
  anchorRect,
  current,
  onPick,
  onClose,
}: {
  anchorRect: DOMRect;
  current: Status;
  onPick: (s: Status) => void;
  onClose: () => void;
}) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const options = manualStatusOptions(current);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        onClose();
      }
    };
    const onDown = (e: PointerEvent) => {
      const el = containerRef.current;
      if (el && !el.contains(e.target as Node)) onClose();
    };
    const t = setTimeout(() => window.addEventListener("pointerdown", onDown, true), 0);
    window.addEventListener("keydown", onKey);
    return () => {
      clearTimeout(t);
      window.removeEventListener("pointerdown", onDown, true);
      window.removeEventListener("keydown", onKey);
    };
  }, [onClose]);

  if (options.length === 0) return null;

  const W = 150;
  const left = Math.min(anchorRect.left, window.innerWidth - W - 8);
  const top = Math.min(anchorRect.bottom + 4, window.innerHeight - 160);

  return createPortal(
    <div
      ref={containerRef}
      data-no-pickup
      onPointerDown={(e) => e.stopPropagation()}
      onWheel={(e) => e.stopPropagation()}
      style={{ position: "fixed", left, top, width: W, zIndex: 1200 }}
      className="rounded border border-[var(--border-overlay)] bg-[var(--surface-overlay)] backdrop-blur-md shadow-xl py-1"
    >
      {options.map((s) => {
        const c = STATUS_COLORS[s];
        return (
          <button
            key={s}
            type="button"
            onClick={() => {
              onPick(s);
              onClose();
            }}
            className="flex w-full items-center gap-2 px-3 py-1.5 text-xs hover:bg-[var(--surface-hover)]"
          >
            <span
              className={`block h-2 w-2 shrink-0 rounded-full ${c.dot}`}
            />
            <span className="text-[var(--text-secondary)]">
              {c.label}
            </span>
          </button>
        );
      })}
    </div>,
    document.body,
  );
}

function ImplRulesEditor({
  value,
  onChange,
}: {
  value: string[];
  onChange: (next: string[]) => void;
}) {
  const add = () => onChange([...value, ""]);
  const update = (idx: number, text: string) =>
    onChange(value.map((v, i) => (i === idx ? text : v)));
  const remove = (idx: number) => onChange(value.filter((_, i) => i !== idx));

  return (
    <div className="flex flex-col" style={{ gap: 4, paddingLeft: 16 }}>
      {value.map((rule, i) => (
        <div
          key={i}
          className="group/implrule flex items-center"
          style={{ gap: 6 }}
        >
          <span
            className="shrink-0 text-[var(--text-ghost)]"
            style={{ fontSize: 10 }}
          >
            ·
          </span>
          <input
            type="text"
            value={rule}
            onChange={(e) => update(i, e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Backspace" && rule === "") {
                e.preventDefault();
                remove(i);
              }
            }}
            placeholder="Implementation detail"
            className={FIELD_CLASS}
            style={{
              fontSize: 12,
              padding: "4px 8px",
              borderRadius: 5,
            }}
          />
          <button
            type="button"
            onClick={() => remove(i)}
            className="opacity-0 group-hover/implrule:opacity-100 shrink-0 text-[var(--text-ghost)] hover:text-red-400"
            aria-label="Remove detail"
          >
            <X style={{ width: 12, height: 12 }} />
          </button>
        </div>
      ))}
      <button
        type="button"
        onClick={add}
        className="self-start text-[var(--text-ghost)] hover:text-[var(--text-muted)]"
        style={{ fontSize: 11, paddingLeft: 14 }}
      >
        + Add detail
      </button>
    </div>
  );
}

function MoveFromButton({
  nodeId,
  nodeKind,
  onMoveFrom,
}: {
  nodeId: string;
  nodeKind: Kind;
  onMoveFrom: (fromNodeId: string, respId: string) => void;
}) {
  const model = useContext(ModelContext);
  const [open, setOpen] = useState(false);
  const [anchorRect, setAnchorRect] = useState<DOMRect | null>(null);
  const btnRef = useRef<HTMLButtonElement | null>(null);

  const sameLevelNodes = model.nodes.filter(
    (n) => n.kind === nodeKind && n.id !== nodeId,
  );
  const nodesWithResps = sameLevelNodes.filter(
    (n) => (n.responsibilities ?? []).some((r) => !r.locked),
  );

  if (nodesWithResps.length === 0) return null;

  return (
    <>
      <button
        ref={btnRef}
        type="button"
        onClick={() => {
          const r = btnRef.current?.getBoundingClientRect();
          if (r) setAnchorRect(r);
          setOpen(true);
        }}
        className="flex items-center text-[var(--text-muted)] hover:text-[var(--text-secondary)] border border-dashed border-[var(--border)] hover:border-[var(--text-muted)]"
        style={{
          gap: 6,
          fontSize: 12,
          padding: "6px 10px",
          borderRadius: 6,
        }}
      >
        <ArrowLeft style={{ width: 12, height: 12 }} />
        Move from...
      </button>
      {open && anchorRect && (
        <MoveFromPicker
          anchorRect={anchorRect}
          nodes={nodesWithResps}
          onPick={(fromNodeId, respId) => {
            onMoveFrom(fromNodeId, respId);
            setOpen(false);
          }}
          onClose={() => setOpen(false)}
        />
      )}
    </>
  );
}

function MoveFromPicker({
  anchorRect,
  nodes,
  onPick,
  onClose,
}: {
  anchorRect: DOMRect;
  nodes: { id: string; name: string; responsibilities?: Responsibility[] }[];
  onPick: (nodeId: string, respId: string) => void;
  onClose: () => void;
}) {
  const containerRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        onClose();
      }
    };
    const onDown = (e: PointerEvent) => {
      const el = containerRef.current;
      if (el && !el.contains(e.target as Node)) onClose();
    };
    const t = setTimeout(() => window.addEventListener("pointerdown", onDown, true), 0);
    window.addEventListener("keydown", onKey);
    return () => {
      clearTimeout(t);
      window.removeEventListener("pointerdown", onDown, true);
      window.removeEventListener("keydown", onKey);
    };
  }, [onClose]);

  const W = 320;
  const left = Math.min(anchorRect.left, window.innerWidth - W - 8);
  const top = Math.min(anchorRect.bottom + 4, window.innerHeight - 300);

  return createPortal(
    <div
      ref={containerRef}
      data-no-pickup
      onPointerDown={(e) => e.stopPropagation()}
      onWheel={(e) => e.stopPropagation()}
      style={{ position: "fixed", left, top, width: W, maxHeight: 280, zIndex: 1200 }}
      className="overflow-auto rounded border border-[var(--border-overlay)] bg-[var(--surface-overlay)] backdrop-blur-md shadow-xl py-1"
    >
      {nodes.map((n) => {
        const resps = (n.responsibilities ?? []).filter((r) => !r.locked);
        if (resps.length === 0) return null;
        return (
          <div key={n.id}>
            <div
              className="px-3 py-1 text-[var(--text-ghost)] font-semibold uppercase"
              style={{ fontSize: 10, letterSpacing: "0.05em" }}
            >
              {n.name || "Untitled"}
            </div>
            {resps.map((r) => {
              const colors = r.status ? STATUS_COLORS[r.status] : null;
              return (
                <button
                  key={r.id}
                  type="button"
                  onClick={() => onPick(n.id, r.id)}
                  className="flex w-full items-center gap-2 px-3 py-1.5 text-xs hover:bg-[var(--surface-hover)] text-left"
                >
                  <span
                    className={`block h-2 w-2 shrink-0 rounded-full ${
                      colors ? colors.dot : "bg-[var(--text-ghost)]"
                    }`}
                  />
                  <span className="text-[var(--text-secondary)] truncate">
                    {r.statement || "(empty)"}
                  </span>
                </button>
              );
            })}
          </div>
        );
      })}
    </div>,
    document.body,
  );
}

export function ResponsibilitiesEditor({
  value,
  onChange,
  onCommit,
  nodeId,
  nodeKind,
  onMoveFrom,
}: {
  value: Responsibility[];
  onChange: (next: Responsibility[]) => void;
  onCommit: () => void;
  nodeId?: string;
  nodeKind?: Kind;
  onMoveFrom?: (fromNodeId: string, respId: string) => void;
}) {
  const hideStatus = nodeKind === "person";
  const update = (idx: number, patch: Partial<Responsibility>) =>
    onChange(value.map((r, i) => {
      if (i !== idx) return r;
      const next = { ...r, ...patch };
      if (
        !patch.status &&
        (patch.statement !== undefined || patch.implementationRules !== undefined) &&
        (r.status === "implemented" || r.status === "verified")
      ) {
        next.status = "changed";
      }
      return next;
    }));
  const remove = (idx: number) =>
    onChange(value.filter((_, i) => i !== idx));
  const add = () => {
    const id =
      typeof crypto !== "undefined" && crypto.randomUUID
        ? crypto.randomUUID()
        : `r-${Date.now()}-${Math.random()}`;
    onChange([...value, { id, statement: "" }]);
  };

  const [expandedSet, setExpandedSet] = useState<Set<string>>(new Set());
  const toggleExpanded = (id: string) =>
    setExpandedSet((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  const [pickerFor, setPickerFor] = useState<{
    idx: number;
    rect: DOMRect;
  } | null>(null);

  return (
    <div className="flex flex-col" style={{ gap: 6 }}>
      {value.map((r, i) => {
        const colors = hideStatus ? null : (r.status ? STATUS_COLORS[r.status] : null);
        const isLocked = r.locked === true;
        const hasTransitions = !hideStatus && !isLocked && manualStatusOptions(r.status).length > 0;
        const hasRules = (r.implementationRules?.length ?? 0) > 0;
        const expanded = expandedSet.has(r.id);
        return (
          <div key={r.id} className="flex flex-col" style={{ gap: 4, opacity: isLocked ? 0.5 : undefined }}>
            <div
              className="group/respedit flex items-center"
              style={{ gap: 8 }}
            >
              {hasTransitions ? (
                <button
                  type="button"
                  onClick={(e) => {
                    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
                    setPickerFor({ idx: i, rect });
                  }}
                  className="shrink-0"
                  title={colors?.label ?? "proposed"}
                >
                  <span
                    className={`block rounded-full ${
                      colors ? colors.dot : "bg-[var(--text-ghost)]"
                    } cursor-pointer hover:ring-2 hover:ring-[var(--border)]`}
                    style={{ width: 8, height: 8 }}
                  />
                </button>
              ) : (
                <span
                  className={`shrink-0 block rounded-full ${
                    colors ? colors.dot : "bg-[var(--text-ghost)]"
                  }`}
                  style={{ width: 8, height: 8 }}
                  title={colors?.label ?? "proposed"}
                />
              )}
              {isLocked ? (
                <span
                  className="flex-1 text-[var(--text-muted)]"
                  style={{ fontSize: 13, padding: "6px 10px" }}
                >
                  {r.statement}
                  {r.relocatedTo && (
                    <span className="text-[var(--text-ghost)] italic" style={{ fontSize: 11, marginLeft: 6 }}>
                      → relocated
                    </span>
                  )}
                </span>
              ) : (
                <input
                  type="text"
                  value={r.statement}
                  onChange={(e) => update(i, { statement: e.target.value })}
                  onKeyDown={(e) => {
                    if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) onCommit();
                    if (e.key === "Backspace" && r.statement === "") {
                      e.preventDefault();
                      remove(i);
                    }
                  }}
                  placeholder="Responsibility statement"
                  className={FIELD_CLASS}
                  style={{
                    fontSize: 13,
                    padding: "6px 10px",
                    borderRadius: 6,
                  }}
                />
              )}
              {!isLocked && (
                <>
                  <button
                    type="button"
                    onClick={() => toggleExpanded(r.id)}
                    className={`shrink-0 transition-colors ${
                      hasRules || expanded
                        ? "text-[var(--text-muted)]"
                        : "opacity-0 group-hover/respedit:opacity-100 text-[var(--text-ghost)]"
                    } hover:text-[var(--text-secondary)]`}
                    title="Implementation details"
                  >
                    <ChevronDown
                      style={{
                        width: 14,
                        height: 14,
                        transform: expanded ? "rotate(180deg)" : undefined,
                        transition: "transform 0.15s",
                      }}
                    />
                  </button>
                  <button
                    type="button"
                    onClick={() => remove(i)}
                    className="opacity-0 group-hover/respedit:opacity-100 shrink-0 text-[var(--text-ghost)] hover:text-red-400"
                    aria-label="Remove responsibility"
                  >
                    <X style={{ width: 14, height: 14 }} />
                  </button>
                </>
              )}
            </div>
            {expanded && !isLocked && (
              <ImplRulesEditor
                value={r.implementationRules ?? []}
                onChange={(rules) =>
                  update(i, { implementationRules: rules.length > 0 ? rules : undefined })
                }
              />
            )}
          </div>
        );
      })}
      <div className="flex items-center" style={{ gap: 6, marginTop: value.length > 0 ? 2 : 0 }}>
        <button
          type="button"
          onClick={add}
          className="flex items-center text-[var(--text-muted)] hover:text-[var(--text-secondary)] border border-dashed border-[var(--border)] hover:border-[var(--text-muted)]"
          style={{
            gap: 6,
            fontSize: 12,
            padding: "6px 10px",
            borderRadius: 6,
          }}
        >
          <Plus style={{ width: 12, height: 12 }} />
          Add
        </button>
        {onMoveFrom && nodeKind && (
          <MoveFromButton
            nodeId={nodeId!}
            nodeKind={nodeKind}
            onMoveFrom={onMoveFrom}
          />
        )}
      </div>
      {pickerFor && (
        <StatusPicker
          anchorRect={pickerFor.rect}
          current={value[pickerFor.idx]?.status ?? "proposed"}
          onPick={(s) => update(pickerFor.idx, { status: s })}
          onClose={() => setPickerFor(null)}
        />
      )}
    </div>
  );
}

function PropertiesEditor({
  value,
  onChange,
  onCommit,
}: {
  value: ModelProperty[];
  onChange: (next: ModelProperty[]) => void;
  onCommit: () => void;
}) {
  const update = (idx: number, patch: Partial<ModelProperty>) =>
    onChange(value.map((p, i) => (i === idx ? { ...p, ...patch } : p)));
  const remove = (idx: number) => onChange(value.filter((_, i) => i !== idx));
  const add = () => onChange([...value, { label: "", description: "" }]);

  return (
    <div className="flex flex-col" style={{ gap: 6 }}>
      {value.map((p, i) => (
        <div
          key={i}
          className="group/propedit flex items-center"
          style={{ gap: 8 }}
        >
          <input
            type="text"
            value={p.label}
            onChange={(e) => update(i, { label: e.target.value })}
            onKeyDown={(e) => {
              if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) onCommit();
            }}
            placeholder="field"
            className={`${FIELD_CLASS} font-mono`}
            style={{
              fontSize: 13,
              padding: "6px 10px",
              borderRadius: 6,
              width: 140,
              flexShrink: 0,
            }}
          />
          <input
            type="text"
            value={p.description ?? ""}
            onChange={(e) => update(i, { description: e.target.value })}
            onKeyDown={(e) => {
              if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) onCommit();
            }}
            placeholder="description"
            className={FIELD_CLASS}
            style={{
              fontSize: 13,
              padding: "6px 10px",
              borderRadius: 6,
            }}
          />
          <button
            type="button"
            onClick={() => remove(i)}
            className="opacity-0 group-hover/propedit:opacity-100 shrink-0 text-[var(--text-ghost)] hover:text-red-400"
            aria-label="Remove property"
          >
            <X style={{ width: 14, height: 14 }} />
          </button>
        </div>
      ))}
      <button
        type="button"
        onClick={add}
        className="flex items-center self-start text-[var(--text-muted)] hover:text-[var(--text-secondary)] border border-dashed border-[var(--border)] hover:border-[var(--text-muted)]"
        style={{
          gap: 6,
          fontSize: 12,
          padding: "6px 10px",
          borderRadius: 6,
          marginTop: value.length > 0 ? 2 : 0,
        }}
      >
        <Plus style={{ width: 12, height: 12 }} />
        Add property
      </button>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Public wrapper used by PackBox — keeps the same signature.
// ---------------------------------------------------------------------------

export function EntryCard({
  node,
  onNavigate,
  onLinkClick,
  editor,
}: {
  node: NodeView;
  onNavigate?: (nodeId: string) => void;
  onLinkClick?: (partnerId: string) => void;
  editor?: Editor;
}) {
  const { heldId } = useContext(GridContext);
  return (
    <EntryCardView
      node={node}
      span={cardSpan(node)}
      onNavigate={onNavigate}
      onLinkClick={onLinkClick}
      pickupId={node.id}
      ghost={heldId === node.id}
      editor={editor}
    />
  );
}
