import { useContext, useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import {
  ArrowLeft,
  ChevronDown,
  ChevronRight,
  Pencil,
  Plus,
  Share2,
  Trash2,
  X,
} from "lucide-react";
import type {
  Kind,
  SchemaProperty,
  NodeView,
  Responsibility,
} from "./viewmodel";
import { isDataShape } from "./viewmodel";
import type { Span } from "./pack";
import { cardSpan } from "./pack";
import { STATUS_COLORS } from "./statusColors";
import type { Status } from "./statusColors";
import { GridContext } from "./gridcontext";
import { ModelContext, VisibleScopeContext } from "./modelcontext";
import { useZoom, ZoomContext } from "./PanZoom";
import { effectiveRespStatus } from "./rollup";
import { rowSkin } from "./fossil";
import { kindIcon, typeTag } from "./kindIcon";
import { ConfirmPopover } from "./ConfirmPopover";
import { IconPicker, lookupIcon } from "./IconPicker";
import type { Editor } from "./editor";

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
  cardSelected?: boolean;
  selectedRespId?: string | null;
  buildActive?: boolean;
  cardNew?: boolean;
  newRespIds?: ReadonlySet<string>;
  dimmed?: boolean;
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
  cardSelected = false,
  selectedRespId = null,
  buildActive = false,
  cardNew = false,
  newRespIds,
  dimmed = false,
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
  cardSelected?: boolean;
  selectedRespId?: string | null;
  buildActive?: boolean;
  cardNew?: boolean;
  newRespIds?: ReadonlySet<string>;
  /** Pushed back (faded) because another node's connections are being traced. */
  dimmed?: boolean;
}) {
  const zoom = useZoom();
  const isPerson = node.kind === "person";
  const isExternal = node.external === true;
  const isCodeKind = node.kind === "symbol";
  const dataShape = isDataShape(node);
  // Boundary IS the recede axis: persons + externals are "the world" and sit
  // back on a duller surface; everything in-scope reads at full strength.
  const isWorld = isPerson || isExternal;
  const navigable = node.kind !== "person" && !isCodeKind && !isExternal;

  const isDeprecated = node.deprecated === true;
  const isRelocated = node.relocated === true;

  // The card is a NEUTRAL container — it frames the metadata and holds the
  // responsibility tiles, but carries no lifecycle/age material itself. The
  // geology (lifecycle edge + age patina) lives entirely on the tiles below
  // (`rowSkin`). `now` is shared so every tile ages on one clock; Date.now() at
  // render is intentional — the patina deepens over real calendar time.
  const now = Math.floor(Date.now() / 1000);

  // Resting material of the card as a raised "act-on" object: a 1px top-lit
  // bevel + (in-scope only) a soft contact shadow; the drag clone lifts higher.
  // The interaction ring (selected blue / AI-new indigo) layers on top.
  const ringLayer = cardSelected
    ? `0 0 0 ${2 * zoom}px var(--accent-blue)`
    : cardNew
      ? `0 0 0 ${2 * zoom}px var(--accent-indigo)`
      : null;
  const restShadow = lifted
    ? `inset 0 ${zoom}px 0 0 var(--bevel-hi), 0 ${10 * zoom}px ${30 * zoom}px ${-8 * zoom}px rgba(var(--card-shadow), 0.5)`
    : isWorld
      ? `inset 0 ${zoom}px 0 0 var(--bevel-hi)`
      : `inset 0 ${zoom}px 0 0 var(--bevel-hi), 0 ${zoom}px ${2 * zoom}px rgba(var(--card-shadow), var(--card-shadow-a))`;
  const composedBoxShadow =
    [ringLayer, restShadow].filter(Boolean).join(", ") || undefined;

  // Shell = border + surface. Refactor flags keep their dashed accent border;
  // otherwise the border is solid and the *surface* (raised vs receded) tells
  // in-scope from world apart — not a dashed edge.
  const shellBase = lifted
    ? "border-solid border-[var(--border-strong)]"
    : ghost
      ? "opacity-40 border-dashed border-[color:var(--accent-blue)]"
      : isDeprecated
        ? "border-dashed border-[color:var(--accent-red)]"
        : isRelocated
          ? "border-dashed border-[color:var(--accent-violet)]"
          : isWorld
            ? "border-solid border-[var(--border-subtle)]"
            : "border-solid border-[var(--border)]";
  const cardBg = isWorld ? "var(--surface-world)" : "var(--surface-raised)";

  const incoming = node._incomingLinks;
  const outgoing = node._outgoingLinks;
  // Resting connection scent: one aggregated degree, shown as a header chip.
  // The relationships themselves draw on demand (ConnectionsOverlay on select).
  // Count only partners present on THIS surface (the same `visibleScope` the
  // overlay draws against) — a link whose other end lives a level away has no
  // line here, so counting it would promise a connection the surface can't show.
  // Self-loops excluded, matching the overlay.
  const visibleScope = useContext(VisibleScopeContext);
  const onSurface = (partner: string) =>
    partner !== node.id && (!visibleScope || visibleScope.has(partner));
  const degree =
    outgoing.filter((l) => onSurface(l.dst)).length +
    (dataShape ? 0 : incoming.filter((l) => onSurface(l.src)).length);
  const nameClass = isCodeKind
    ? "font-mono font-semibold text-[var(--text)]"
    : "font-semibold text-[var(--text)]";

  const tag = typeTag(node);
  const OverrideIcon = lookupIcon(node.icon);
  const Icon = OverrideIcon ?? kindIcon(node);

  const properties = node.properties ?? [];
  const responsibilities = node.responsibilities ?? [];

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
      data-select-node={!lifted && !ghost ? node.id : undefined}
      data-conn-node={!lifted && !ghost ? node.id : undefined}
      className={`group/card relative flex h-full w-full flex-col overflow-hidden ${shellBase} ${buildActive ? "scryer-building" : ""}`}
      style={{
        backgroundColor: cardBg,
        borderRadius: 12 * zoom,
        borderWidth: (ghost ? 2 : 1) * zoom,
        fontSize: 12 * zoom,
        transition: "opacity 0.16s ease, filter 0.16s ease",
        filter: dimmed ? "saturate(0.2)" : undefined,
        opacity: dimmed
          ? 0.32
          : isDeprecated && !lifted && !ghost
            ? 0.55
            : undefined,
        // `scryer-building` animates box-shadow off this width; it overrides the
        // box-shadow below while the AI is generating this node. The neutral
        // container only carries the interaction ring (selected blue / AI-new
        // indigo); the shell class supplies the resting shadow/border otherwise.
        ["--ring-w" as string]: `${2.5 * zoom}px`,
        boxShadow: composedBoxShadow,
      }}
      onDoubleClick={handleDoubleClick}
    >
      {/* identity row — a lidded title bar: kind stamp, name, type-tag, count + drill */}
      <div
        data-pickup={pickupId}
        className={`flex shrink-0 items-center border-b border-[var(--border-subtle)] ${
          pickupId && !ghost
            ? "cursor-grab hover:bg-[var(--surface-hover)]"
            : ""
        }`}
        style={{
          gap: 9 * zoom,
          padding: `${10 * zoom}px ${11 * zoom}px`,
        }}
      >
        {/* kind stamp — type read as a silhouette, not a label */}
        <span
          className="flex shrink-0 items-center justify-center text-[var(--text-secondary)]"
          style={{
            width: 26 * zoom,
            height: 26 * zoom,
            borderRadius: 7 * zoom,
            backgroundColor: "var(--surface-hover)",
            boxShadow: `inset 0 0 0 ${zoom}px var(--border-subtle)`,
          }}
        >
          <Icon style={{ width: 15 * zoom, height: 15 * zoom }} />
        </span>
        {/* name + type-tag (the textual fallback for the silhouette) */}
        <span className="min-w-0 flex-1">
          <span
            className={`block truncate ${nameClass}`}
            style={{ fontSize: 14.5 * zoom, lineHeight: 1.2 }}
          >
            {node.name || "Untitled"}
            {node.kind === "symbol" && "()"}
          </span>
          <span
            className="block truncate"
            style={{ fontSize: 11 * zoom, marginTop: 1 * zoom }}
          >
            <span className="font-medium text-[var(--text-secondary)]">{tag.type}</span>
            {tag.tech && <span className="text-[var(--text-muted)]"> · {tag.tech}</span>}
          </span>
        </span>
        {isDeprecated && (
          <span
            className="shrink-0 font-semibold uppercase text-[color:var(--accent-red)]"
            style={{ fontSize: 9.5 * zoom, letterSpacing: 0.14 * zoom + "em" }}
          >
            Deprecated
          </span>
        )}
        {isRelocated && (
          <span
            className="shrink-0 font-semibold uppercase text-[color:var(--accent-violet)]"
            style={{ fontSize: 9.5 * zoom, letterSpacing: 0.14 * zoom + "em" }}
          >
            Relocated
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
        {/* resting connection scent — one aggregated degree in a fixed slot */}
        {degree > 0 && (
          <span
            className="flex shrink-0 items-center font-semibold text-[var(--text-muted)]"
            style={{ gap: 4 * zoom, fontSize: 11 * zoom }}
            title={`${degree} connection${degree === 1 ? "" : "s"}`}
            aria-label={`${degree} connections`}
          >
            <Share2 style={{ width: 12 * zoom, height: 12 * zoom }} />
            {degree}
          </span>
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
          className="shrink-0 text-[var(--text-secondary)]"
          style={{
            padding: `${8 * zoom}px ${12 * zoom}px ${3 * zoom}px`,
            fontSize: 12 * zoom,
            lineHeight: `${17 * zoom}px`,
          }}
        >
          {node.description}
        </div>
      )}

      {/* data-shape properties — seated in a sunken read-well; status = the lip */}
      {properties.length > 0 && (
        <div
          className="flex flex-col"
          style={{
            margin: `0 ${7 * zoom}px ${8 * zoom}px`,
            marginTop: node.description ? 6 * zoom : 8 * zoom,
            padding: 3 * zoom,
            borderRadius: 7 * zoom,
            backgroundColor: "var(--well)",
            boxShadow: `inset 0 ${zoom}px ${2 * zoom}px rgba(0,0,0,0.18), inset 0 0 0 ${zoom}px rgba(0,0,0,0.1)`,
          }}
        >
          {properties.map((p, i) => {
            const eff = p.status ?? "proposed";
            const psk = rowSkin(eff, p.lastTouchedAt, false, now, zoom);
            // Hairline floored at 1 device px so the divider doesn't vanish to
            // sub-pixel when zoomed out (it still scales up past 1× when zoomed in).
            const sep = i > 0 ? `inset 0 ${Math.max(1, zoom)}px 0 0 var(--row-sep)` : null;
            return (
              <div
                key={i}
                style={{
                  padding: `${7 * zoom}px ${10 * zoom}px ${7 * zoom}px ${11 * zoom}px`,
                  borderRadius: 5 * zoom,
                  lineHeight: `${17 * zoom}px`,
                  boxShadow: [psk.edge, sep].filter(Boolean).join(", ") || undefined,
                }}
              >
                <span
                  className="font-mono font-medium"
                  style={{ fontSize: 12 * zoom, color: psk.color, fontStyle: psk.italic ? "italic" : undefined }}
                >
                  {p.label}
                </span>
                {p.description && (
                  <span className="text-[var(--text-muted)]" style={{ fontSize: 11 * zoom }}>
                    {" "}
                    {p.description}
                  </span>
                )}
              </div>
            );
          })}
        </div>
      )}

      {/* responsibilities — the hero, seated in a sunken read-well */}
      {responsibilities.length > 0 && (
        <div
          className="flex flex-col"
          style={{
            margin: `0 ${7 * zoom}px ${8 * zoom}px`,
            marginTop:
              node.description || properties.length > 0 ? 6 * zoom : 8 * zoom,
            padding: 3 * zoom,
            borderRadius: 7 * zoom,
            backgroundColor: "var(--well)",
            boxShadow: `inset 0 ${zoom}px ${2 * zoom}px rgba(0,0,0,0.18), inset 0 0 0 ${zoom}px rgba(0,0,0,0.1)`,
          }}
        >
          {responsibilities.map((r, i) => {
            // Person/external lines carry no truth-state → neutral; in-scope rows
            // wear the geology (lifecycle lip + age patina) via `rowSkin`.
            const eff = isWorld ? undefined : effectiveRespStatus(node, r);
            const respSelected = selectedRespId === r.id;
            // Per-row "new" tint. newRespIds is filtered at ingestion to exclude
            // rows on a still-new node (the card's ring already covers those), so
            // this fires only for rows added to an already-reviewed card. The
            // !cardNew guard is a defensive backstop for that invariant.
            const respNew =
              !respSelected && !cardNew && (newRespIds?.has(r.id) ?? false);
            const rsk = isWorld
              ? null
              : rowSkin(eff, r.lastTouchedAt, r.vagrant === true, now, zoom);
            const rules = r.directives ?? [];
            // Hairline floored at 1 device px so the divider doesn't vanish to
            // sub-pixel when zoomed out (it still scales up past 1× when zoomed in).
            const sep = i > 0 ? `inset 0 ${Math.max(1, zoom)}px 0 0 var(--row-sep)` : null;
            return (
              <div
                key={r.id}
                data-select-resp={!lifted && !ghost ? r.id : undefined}
                className={`flex items-start ${!lifted && !ghost ? "cursor-pointer" : ""}`}
                style={{
                  gap: 8 * zoom,
                  padding: `${7 * zoom}px ${10 * zoom}px ${7 * zoom}px ${11 * zoom}px`,
                  borderRadius: 5 * zoom,
                  backgroundColor: respSelected
                    ? "color-mix(in srgb, var(--accent-blue) 18%, transparent)"
                    : respNew
                      ? "color-mix(in srgb, var(--accent-indigo) 15%, transparent)"
                      : undefined,
                  boxShadow: [rsk?.edge, sep].filter(Boolean).join(", ") || undefined,
                }}
              >
                {/* list bullet — neutral marker; status stays on the left lip */}
                <span
                  aria-hidden
                  className="shrink-0 rounded-full bg-[var(--text-muted)]"
                  style={{ width: 4 * zoom, height: 4 * zoom, marginTop: 6.5 * zoom }}
                />
                <div style={{ minWidth: 0, flex: 1 }}>
                  <span
                    style={{
                      display: "block",
                      color: isWorld ? "var(--text-secondary)" : rsk?.color ?? "var(--text)",
                      fontStyle: rsk?.italic ? "italic" : undefined,
                      fontSize: 13 * zoom,
                      lineHeight: `${17 * zoom}px`,
                    }}
                  >
                    {r.statement}
                  </span>
                  {rules.map((rule, ri) => (
                    <span
                      key={ri}
                      className="block text-[var(--text-muted)]"
                      style={{ fontSize: 10.5 * zoom, lineHeight: `${14 * zoom}px` }}
                    >
                      {rule}
                    </span>
                  ))}
                </div>
              </div>
            );
          })}
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
  properties: SchemaProperty[];
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

  const isCodeKind = draft.kind === "symbol";
  const isPerson = draft.kind === "person";

  const OverrideIcon = lookupIcon(draft.icon);
  const Icon = OverrideIcon ?? kindIcon(node);

  const validKinds: Kind[] = node.parentId
    ? node.kind === "symbol"
      ? ["symbol"]
      : [node.kind]
    : ["person", "system"];

  const commitDraft = () => {
    const patch: Parameters<typeof editor.updateNode>[1] = {
      name: draft.name.trim(),
      description: draft.description.trim() || undefined,
      technology: !isCodeKind
        ? draft.technology.trim() || undefined
        : undefined,
      icon: draft.icon,
      deprecated: draft.deprecated || undefined,
      relocated: draft.relocated || undefined,
      responsibilities: draft.responsibilities,
      properties: isCodeKind ? draft.properties : undefined,
    };
    // Only include `kind` when it actually changed — `kind` is required, and
    // spreading `kind: undefined` into the node would clobber it.
    if (draft.kind !== node.kind) patch.kind = draft.kind;
    editor.updateNode(node.id, patch);
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
            boxShadow: `0 0 0 ${2 * sourceZoom}px var(--text-secondary), 0 ${8 * sourceZoom}px ${24 * sourceZoom}px rgba(var(--focus-halo), var(--focus-halo-a))`,
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

          {isCodeKind && (
            <FieldGroup>
              <label className={LABEL_CLASS}>Properties</label>
              <div style={{ marginTop: 6 }}>
                <PropertiesEditor
                  value={draft.properties}
                  onChange={(properties) => setDraft({ ...draft, properties })}
                  onCommit={confirm}
                />
              </div>
            </FieldGroup>
          )}

          <FieldGroup>
            <label className={LABEL_CLASS}>Responsibilities</label>
            <div style={{ marginTop: 6 }}>
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

function DirectivesEditor({
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
        (patch.statement !== undefined || patch.directives !== undefined) &&
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
        const hasRules = (r.directives?.length ?? 0) > 0;
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
              <DirectivesEditor
                value={r.directives ?? []}
                onChange={(rules) =>
                  update(i, { directives: rules.length > 0 ? rules : undefined })
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
  value: SchemaProperty[];
  onChange: (next: SchemaProperty[]) => void;
  onCommit: () => void;
}) {
  const update = (idx: number, patch: Partial<SchemaProperty>) =>
    onChange(value.map((p, i) => {
      if (i !== idx) return p;
      const next = { ...p, ...patch };
      if (
        !patch.status &&
        (patch.label !== undefined || patch.description !== undefined) &&
        (p.status === "implemented" || p.status === "verified")
      ) {
        next.status = "changed";
      }
      return next;
    }));
  const remove = (idx: number) => onChange(value.filter((_, i) => i !== idx));
  const add = () => onChange([...value, { label: "", description: "" }]);

  const [pickerFor, setPickerFor] = useState<{ idx: number; rect: DOMRect } | null>(null);

  return (
    <div className="flex flex-col" style={{ gap: 6 }}>
      {value.map((p, i) => {
        const colors = p.status ? STATUS_COLORS[p.status] : null;
        const hasTransitions = manualStatusOptions(p.status).length > 0;
        return (
        <div
          key={i}
          className="group/propedit flex items-center"
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
                className={`block rounded-full ${colors ? colors.dot : "bg-[var(--text-ghost)]"} cursor-pointer hover:ring-2 hover:ring-[var(--border)]`}
                style={{ width: 8, height: 8 }}
              />
            </button>
          ) : (
            <span
              className={`shrink-0 block rounded-full ${colors ? colors.dot : "bg-[var(--text-ghost)]"}`}
              style={{ width: 8, height: 8 }}
              title={colors?.label ?? "proposed"}
            />
          )}
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
        );
      })}
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

// ---------------------------------------------------------------------------
// Public wrapper used by PackBox — keeps the same signature.
// ---------------------------------------------------------------------------

export function EntryCard({
  node,
  onNavigate,
  onLinkClick,
  editor,
  cardSelected,
  selectedRespId,
  buildActive,
  cardNew,
  newRespIds,
  dimmed,
}: {
  node: NodeView;
  onNavigate?: (nodeId: string) => void;
  onLinkClick?: (partnerId: string) => void;
  editor?: Editor;
  cardSelected?: boolean;
  selectedRespId?: string | null;
  buildActive?: boolean;
  cardNew?: boolean;
  newRespIds?: ReadonlySet<string>;
  dimmed?: boolean;
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
      cardSelected={cardSelected}
      selectedRespId={selectedRespId}
      buildActive={buildActive}
      cardNew={cardNew}
      newRespIds={newRespIds}
      dimmed={dimmed}
    />
  );
}
