import { useContext } from "react";
import { ArrowRight, ArrowLeft, ChevronRight } from "lucide-react";
import type { Entry } from "./viewmodel";
import type { Span } from "./pack";
import {
  cardSpan,
  CARD_HEADER_H,
  CARD_META_H,
  RESP_LINE_H,
  RESP_PAD,
  LINK_ROW_H,
  LINK_PAD,
} from "./pack";
import { STATUS_COLORS } from "./statusColors";
import { GridContext } from "./gridcontext";
import { ModelContext, VisibleScopeContext } from "./modelcontext";
import { useZoom } from "./PanZoom";
import { findCard, incomingLinks } from "./references";
import { effectiveRespStatus } from "./rollup";
import { tokenIcon } from "./tokens";

function StatusDot({
  status,
  size,
}: {
  status: string | null;
  size: number;
}) {
  const zoom = useZoom();
  const s = size * zoom;
  const colors =
    status ? STATUS_COLORS[status as keyof typeof STATUS_COLORS] : null;
  return (
    <span
      className={`shrink-0 rounded-full ${colors ? colors.dot : "bg-[var(--text-ghost)]"}`}
      style={{ width: s, height: s }}
      aria-label={colors?.label}
    />
  );
}

function LinkLine({
  direction,
  links,
  onLinkClick,
}: {
  direction: "out" | "in";
  links: { partnerId: string; label: string }[];
  onLinkClick?: (partnerId: string) => void;
}) {
  const surfaces = useContext(ModelContext);
  const zoom = useZoom();
  const Arrow = direction === "out" ? ArrowRight : ArrowLeft;

  return (
    <div
      className="flex items-center truncate"
      style={{
        gap: 6 * zoom,
        height: LINK_ROW_H * zoom,
        fontSize: 10 * zoom,
      }}
    >
      <Arrow
        className="shrink-0 text-[var(--text-ghost)]"
        style={{ width: 10 * zoom, height: 10 * zoom }}
      />
      {links.map((l, i) => {
        const partner = findCard(surfaces, l.partnerId);
        const name = partner?.title ?? l.partnerId;
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
              data-no-pickup
              role="button"
              tabIndex={0}
              title={l.label ? `${name} — ${l.label}` : name}
              onClick={(e) => {
                e.stopPropagation();
                onLinkClick?.(l.partnerId);
              }}
              className="cursor-pointer font-medium text-[var(--text-secondary)] hover:text-[var(--text)] hover:underline"
            >
              {name}
            </span>
          </span>
        );
      })}
    </div>
  );
}

export function EntryCardView({
  entry,
  span: _span,
  onNavigate,
  onLinkClick,
  pickupId,
  ghost = false,
  lifted = false,
}: {
  entry: Entry;
  span: Span;
  onNavigate?: (childSurfaceId: string) => void;
  onLinkClick?: (partnerId: string) => void;
  pickupId?: string;
  ghost?: boolean;
  lifted?: boolean;
}) {
  const surfaces = useContext(ModelContext);
  const visibleScope = useContext(VisibleScopeContext);
  const zoom = useZoom();
  const isExternal = entry.external === true;
  const navigable = entry.kind !== "person" && !isExternal;

  const shellBase = lifted
    ? "shadow-2xl border-solid border-[var(--border-strong)]"
    : ghost
      ? "opacity-40 border-dashed border-blue-400"
      : isExternal
        ? "shadow-sm border-dashed border-[var(--border-strong)]"
        : "shadow-md border-solid border-[var(--border)]";

  const allIncoming = incomingLinks(surfaces, entry.id);
  const incoming = visibleScope
    ? allIncoming.filter((l) => visibleScope.has(l.from.id))
    : allIncoming;
  const outgoing = entry.links ?? [];
  const hasLinks = outgoing.length > 0 || incoming.length > 0;

  return (
    <div
      className={`flex h-full w-full flex-col overflow-hidden bg-[var(--surface-raised)] ${shellBase}`}
      style={{
        borderRadius: 12 * zoom,
        borderWidth: (ghost ? 2 : 1) * zoom,
      }}
    >
      {/* identity: status dot + title */}
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
        {(() => {
          const Icon = tokenIcon(entry.id);
          return (
            <Icon
              className="shrink-0 text-[var(--text-muted)]"
              style={{ width: 14 * zoom, height: 14 * zoom }}
            />
          );
        })()}
        <span
          className="flex-1 truncate"
          style={{ fontSize: 13 * zoom }}
        >
          <span className="font-semibold text-[var(--text)]">{entry.title}</span>
          {entry.technology && (
            <span
              className="italic text-[var(--text-muted)]"
              style={{ fontSize: 11 * zoom, marginLeft: 6 * zoom }}
            >
              {entry.technology}
            </span>
          )}
        </span>
        {isExternal && (
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
        {navigable && (
          <button
            type="button"
            data-no-pickup
            onClick={() => onNavigate?.(entry.id)}
            aria-label={`Open ${entry.title}`}
            className="flex shrink-0 cursor-pointer items-center font-medium text-[var(--text-muted)] hover:text-[var(--text-secondary)]"
            style={{
              gap: 2 * zoom,
              padding: `${2 * zoom}px ${4 * zoom}px`,
              fontSize: 11 * zoom,
              borderRadius: 4 * zoom,
            }}
          >
            Open
            <ChevronRight style={{ width: 12 * zoom, height: 12 * zoom }} />
          </button>
        )}
      </div>

      {entry.description && (
        <div
          className="shrink-0 truncate text-[var(--text-muted)]"
          style={{
            height: CARD_META_H * zoom,
            padding: `0 ${12 * zoom}px`,
            fontSize: 11 * zoom,
            lineHeight: `${CARD_META_H * zoom}px`,
          }}
        >
          {entry.description}
        </div>
      )}

      {/* responsibilities: status dots + text, 2-col grid when ≥3 */}
      {entry.responsibilities.length > 0 && (
        <div
          className="grid border-t border-[var(--border-subtle)]"
          style={{
            padding: `${(RESP_PAD / 2) * zoom}px ${12 * zoom}px`,
            gridTemplateColumns: `repeat(${entry.responsibilities.length >= 3 ? 2 : 1}, minmax(0, 1fr))`,
          }}
        >
          {entry.responsibilities.map((r) => {
            const eff = isExternal
              ? undefined
              : effectiveRespStatus(surfaces, entry, r);
            return (
              <div
                key={r.id}
                className="flex items-center"
                style={{ gap: 8 * zoom, height: RESP_LINE_H * zoom }}
              >
                <StatusDot status={eff ?? null} size={6} />
                <span
                  className="flex-1 truncate font-medium text-[var(--text-secondary)]"
                  style={{ fontSize: 12 * zoom }}
                >
                  {r.text}
                </span>
              </div>
            );
          })}
        </div>
      )}

      {/* links: grouped name lists by direction */}
      {hasLinks && (
        <div
          className="shrink-0 border-t border-[var(--border-subtle)]"
          style={{ padding: `${(LINK_PAD / 2) * zoom}px ${12 * zoom}px` }}
        >
          {outgoing.length > 0 && (
            <LinkLine
              direction="out"
              links={outgoing.map((l) => ({
                partnerId: l.to,
                label: l.label,
              }))}
              onLinkClick={onLinkClick}
            />
          )}
          {incoming.length > 0 && (
            <LinkLine
              direction="in"
              links={incoming.map((l) => ({
                partnerId: l.from.id,
                label: l.label,
              }))}
              onLinkClick={onLinkClick}
            />
          )}
        </div>
      )}
    </div>
  );
}

export function EntryCard({
  entry,
  onNavigate,
  onLinkClick,
}: {
  entry: Entry;
  onNavigate?: (childSurfaceId: string) => void;
  onLinkClick?: (partnerId: string) => void;
}) {
  const { heldId } = useContext(GridContext);
  return (
    <EntryCardView
      entry={entry}
      span={cardSpan(entry)}
      onNavigate={onNavigate}
      onLinkClick={onLinkClick}
      pickupId={entry.id}
      ghost={heldId === entry.id}
    />
  );
}
