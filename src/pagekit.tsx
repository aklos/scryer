/**
 * Shared building blocks for the node/group pages and the tree. Read-first: the
 * page shows clean content by default; each section carries an [edit] toggle that
 * swaps only that section into edit mode in place. Uses the existing theme tokens
 * — these are layout/interaction primitives, not a restyle.
 */

import type { ComponentType, ReactNode } from "react";
import type { LucideProps } from "lucide-react";
import { ArrowDownLeft, ArrowUpRight } from "lucide-react";
import type { Status } from "./statusColors";
import { STATUS_COLORS } from "./statusColors";

/** Statuses a person sets by hand. `changed`/`vagrant`/`relocated` are machine
 *  states, surfaced but never picked directly. */
export const USER_STATUSES: Status[] = ["proposed", "implemented", "verified"];

/** A small status dot — used in the tree (plan: status reflected as colored
 *  dots in the tree). Not used in page reading content. */
export function StatusDot({
  status,
  className = "",
}: {
  status: Status | null | undefined;
  className?: string;
}) {
  const colors = status ? STATUS_COLORS[status] : null;
  return (
    <span
      className={`inline-block h-1.5 w-1.5 shrink-0 rounded-full ${
        colors ? colors.dot : "bg-[var(--text-ghost)]"
      } ${className}`}
      title={colors?.label}
    />
  );
}

/** Status as a quiet word (not a dot/traffic-light), tinted with the status's
 *  own text colour. The read-mode status indicator on pages. */
export function StatusTag({ status }: { status: Status | null | undefined }) {
  if (!status || status === "implemented") return null;
  const c = STATUS_COLORS[status];
  return (
    <span className={`shrink-0 text-[11px] font-medium ${c.icon}`}>{c.label}</span>
  );
}

/** A bracketed [edit]/[done] control, Wikipedia-style (blue link). */
export function EditLink({
  editing,
  label,
  onClick,
}: {
  editing?: boolean;
  label?: string;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="shrink-0 text-[11px] text-[var(--color-blue-400)] hover:underline cursor-pointer"
    >
      [{label ?? (editing ? "done" : "edit")}]
    </button>
  );
}

/** A wiki-style section. `flow-root` makes it a block-formatting context so it
 *  shrinks beside the floated infobox (and reclaims full width below it) the way
 *  Wikipedia article sections wrap an infobox — without clipping dropdowns. */
export function PageSection({
  title,
  count,
  right,
  editable,
  editing,
  onToggleEdit,
  children,
}: {
  title: string;
  count?: number;
  right?: ReactNode;
  editable?: boolean;
  editing?: boolean;
  onToggleEdit?: () => void;
  children: ReactNode;
}) {
  return (
    <section className="flow-root px-6 pt-5 pb-4">
      <div className="mb-3 flex items-baseline gap-2 border-b border-[var(--border-subtle)] pb-1.5">
        <h2 className="text-[15px] font-semibold text-[var(--text)]">
          {title}
        </h2>
        {count != null && count > 0 && (
          <span className="text-[12px] tabular-nums text-[var(--text-muted)]">
            ({count})
          </span>
        )}
        {right}
        <span className="flex-1" />
        {editable && onToggleEdit && (
          <EditLink editing={editing} onClick={onToggleEdit} />
        )}
      </div>
      {children}
    </section>
  );
}

export function Empty({ children }: { children: ReactNode }) {
  return (
    <div className="text-[12px] leading-relaxed text-[var(--text-ghost)]">
      {children}
    </div>
  );
}

/** An inline cross-reference to another node/group's page. */
export function WikiLink({
  name,
  Icon,
  onClick,
  dir,
  muted = false,
}: {
  name: string;
  Icon?: ComponentType<LucideProps>;
  onClick: () => void;
  dir?: "in" | "out";
  muted?: boolean;
}) {
  const Arrow = dir === "out" ? ArrowUpRight : dir === "in" ? ArrowDownLeft : null;
  return (
    <button
      type="button"
      onClick={onClick}
      className={`group/wl inline-flex max-w-full items-center gap-1.5 rounded px-1 py-0.5 text-left transition-colors hover:bg-[var(--surface-hover)] cursor-pointer ${
        muted ? "text-[var(--text-muted)]" : "text-[var(--text-secondary)]"
      }`}
    >
      {Arrow && (
        <Arrow className="h-3.5 w-3.5 shrink-0 text-[var(--text-ghost)]" />
      )}
      {Icon && <Icon className="h-3.5 w-3.5 shrink-0 text-[var(--text-ghost)]" />}
      <span className="truncate text-[12.5px] group-hover/wl:text-[var(--text)] group-hover/wl:underline">
        {name || "Untitled"}
      </span>
    </button>
  );
}
