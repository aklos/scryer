/**
 * Shared building blocks for the node/group pages and the tree, following
 * Wikipedia's pattern language:
 *
 *  - PageSection — underlined section heading with a per-section [edit] link.
 *  - Banner — the "ambox" maintenance notice pinned to the top of a page
 *    (stale claims, drift, undescribed behaviour), with verdict actions inline.
 *  - WikiLink — inline cross-reference. Blue = the target exists (has built
 *    content); red = the target is plan-only (proposed/empty), Wikipedia's
 *    redlink convention. Status words elsewhere keep the lifecycle hues.
 *  - StatusTag — the status word, always rendered (observability over
 *    minimalism: implemented is shown, not implied by silence).
 */

import { useState, type ComponentType, type Dispatch, type ReactNode, type SetStateAction } from "react";
import type { LucideProps } from "lucide-react";
import { ArrowDownLeft, ArrowUpRight } from "lucide-react";
import type { Status } from "./statusColors";
import { STATUS_COLORS } from "./statusColors";
import type { Node } from "./viewmodel";
import { effectiveNodeStatus, isNodeEmpty } from "./rollup";
import { Button } from "./ui";

/** Statuses a person sets by hand. `changed`/`vagrant`/`relocated` are machine
 *  states, surfaced but never picked directly. */
export const USER_STATUSES: Status[] = ["proposed", "implemented", "verified"];

/** The status pill, tinted with the status's own hue. Implemented is the
 *  steady state and stays silent — the pill only appears when there's
 *  something to say (proposed/changed/verified), so pages don't read as a
 *  wall of status badges. */
export function StatusTag({ status }: { status: Status | null | undefined }) {
  if (!status || status === "implemented") return null;
  const c = STATUS_COLORS[status];
  return <span className={`shrink-0 ${c.pill}`}>{c.label}</span>;
}

const EMPTY_HINT =
  "Empty — no responsibilities or properties. Give it a business responsibility or remove the node.";

/** The `empty` flag (see rollup.isNodeEmpty) — a symbol with no semantic
 *  content. An attention state, not a lifecycle one, so it carries the drift
 *  orange instead of fading into the chrome. */
export function EmptyFlag({ className = "" }: { className?: string }) {
  return (
    <span
      title={EMPTY_HINT}
      className={`inline-flex shrink-0 items-center rounded-full border border-dashed border-orange-400/70 bg-orange-500/5 px-2 py-px text-2xs font-medium text-orange-700 dark:text-orange-300 ${className}`}
    >
      empty
    </span>
  );
}

/** The tree-row counterpart to {@link EmptyFlag}: a hollow ring where a status
 *  dot would sit. */
export function EmptyDot({ className = "" }: { className?: string }) {
  return (
    <span
      title={EMPTY_HINT}
      className={`inline-block h-1.5 w-1.5 shrink-0 rounded-full border border-orange-500 dark:border-orange-400 ${className}`}
    />
  );
}

/**
 * Shared keyboard semantics for the uncontrolled edit fields (commit-on-blur):
 * Enter commits (blurs), Escape reverts to `revertTo` then blurs (the commit
 * handler sees the original value — a no-op). In `multiline` fields plain
 * Enter keeps inserting newlines and Ctrl/Cmd+Enter commits instead.
 */
export function fieldKeys(
  revertTo: string,
  opts?: { multiline?: boolean },
): (e: React.KeyboardEvent<HTMLInputElement | HTMLTextAreaElement>) => void {
  return (e) => {
    if (e.key === "Escape") {
      e.preventDefault();
      e.currentTarget.value = revertTo;
      e.currentTarget.blur();
    } else if (e.key === "Enter") {
      if (opts?.multiline && !(e.metaKey || e.ctrlKey)) return; // newline
      if (!opts?.multiline && e.shiftKey) return;
      e.preventDefault();
      e.currentTarget.blur();
    }
  };
}

/** A bracketed [edit]/[done] control — Wikipedia's section-edit affordance.
 *  Neutral chrome, quiet until hovered. */
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
      className="shrink-0 rounded px-0.5 text-2xs font-medium text-[var(--text-tertiary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text)] hover:underline cursor-pointer"
    >
      [{label ?? (editing ? "done" : "edit")}]
    </button>
  );
}

/** A wiki-style section: an underlined heading with an optional count and
 *  [edit] toggle, then the section body. */
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
    <section className="flow-root pt-6">
      <div className="mb-3 flex items-baseline gap-2 border-b border-[var(--border)] pb-1.5">
        <h2 className="text-base font-semibold text-[var(--text)]">{title}</h2>
        {count != null && count > 0 && (
          <span className="text-xs tabular-nums text-[var(--text-muted)]">
            ({count})
          </span>
        )}
        {right}
        <span className="flex-1" />
        {/* The [edit] affordance disappears while editing — the form's own
            footer (Done / Cancel) is the single exit. */}
        {editable && onToggleEdit && !editing && (
          <EditLink editing={false} onClick={onToggleEdit} />
        )}
      </div>
      {children}
    </section>
  );
}

export function Empty({ children }: { children: ReactNode }) {
  return (
    <div className="text-xs leading-relaxed text-[var(--text-muted)]">
      {children}
    </div>
  );
}

// --- transactional section editor ---------------------------------------------

/**
 * The one editing shell every section shares. Snapshots `initial` into a local
 * draft (deep copy — the model is JSON-safe), renders the form via
 * `children(draft, setDraft)`, and owns the single exit: Done commits the
 * draft through `onCommit`, Cancel or Esc discards it. Nothing reaches the
 * model until Done.
 */
export function SectionEditor<T>({
  initial,
  onCommit,
  onClose,
  footerExtra,
  children,
}: {
  initial: T;
  onCommit: (draft: T) => void;
  onClose: () => void;
  /** Optional extra footer controls (e.g. an "add row" button), right-aligned. */
  footerExtra?: (setDraft: Dispatch<SetStateAction<T>>) => ReactNode;
  children: (draft: T, setDraft: Dispatch<SetStateAction<T>>) => ReactNode;
}) {
  const [draft, setDraft] = useState<T>(() => structuredClone(initial));
  return (
    <div
      className="flex flex-col gap-2"
      onKeyDown={(e) => {
        if (e.key === "Escape") {
          e.stopPropagation();
          onClose();
        }
      }}
    >
      {children(draft, setDraft)}
      <div className="flex items-center gap-2">
        <Button
          variant="primary"
          size="sm"
          onClick={() => {
            onCommit(draft);
            onClose();
          }}
        >
          Done
        </Button>
        <Button variant="ghost" size="sm" onClick={onClose}>
          Cancel
        </Button>
        <span className="flex-1" />
        {footerExtra?.(setDraft)}
      </div>
    </div>
  );
}

// --- maintenance banner (Wikipedia ambox) ------------------------------------

export type BannerTone = "info" | "warning" | "danger";

const BANNER_TONE: Record<BannerTone, { bar: string; bg: string }> = {
  // Indigo = the agent / informational; orange = drift/attention; red = vagrant.
  info: { bar: "border-l-indigo-400", bg: "bg-indigo-500/[0.06]" },
  warning: { bar: "border-l-orange-400", bg: "bg-orange-500/[0.06]" },
  danger: { bar: "border-l-red-400", bg: "bg-red-500/[0.06]" },
};

/** A maintenance notice pinned to the top of a page — Wikipedia's ambox.
 *  States the problem in a sentence and carries its verdict actions inline. */
export function Banner({
  tone,
  icon,
  children,
  actions,
}: {
  tone: BannerTone;
  icon?: ReactNode;
  children: ReactNode;
  actions?: ReactNode;
}) {
  const t = BANNER_TONE[tone];
  return (
    <div
      className={`flex items-start gap-2.5 border border-[var(--border)] border-l-4 ${t.bar} ${t.bg} rounded-r-md px-3.5 py-2.5`}
    >
      {icon && <span className="mt-px shrink-0 text-[var(--text-tertiary)]">{icon}</span>}
      <div className="min-w-0 flex-1 text-xs leading-relaxed text-[var(--text-secondary)]">
        {children}
      </div>
      {actions && <div className="flex shrink-0 items-center gap-2">{actions}</div>}
    </div>
  );
}

// --- wiki links ---------------------------------------------------------------

/** Redlink test: the target is plan-only — nothing built behind it yet. */
export function isRedLink(node: Node): boolean {
  if (node.external || node.kind === "person") return false;
  return isNodeEmpty(node) || effectiveNodeStatus(node) === "proposed";
}

/**
 * An inline cross-reference to another node/group's page. Wikipedia link
 * semantics: blue when the target exists (carries built content), red when the
 * target is plan-only (proposed with nothing built, or an empty symbol) — the
 * redlink. So "what's plan vs what's real" reads in link colour everywhere.
 */
export function WikiLink({
  name,
  Icon,
  onClick,
  dir,
  red = false,
  muted = false,
}: {
  name: string;
  Icon?: ComponentType<LucideProps>;
  onClick: () => void;
  dir?: "in" | "out";
  /** Redlink: the target doesn't exist in code yet. */
  red?: boolean;
  muted?: boolean;
}) {
  const Arrow = dir === "out" ? ArrowUpRight : dir === "in" ? ArrowDownLeft : null;
  const color = muted
    ? "text-[var(--text-muted)]"
    : red
      ? "text-red-700 dark:text-red-400"
      : "text-blue-700 dark:text-blue-400";
  return (
    <button
      type="button"
      onClick={onClick}
      title={red ? `${name || "Untitled"} — planned, nothing built yet` : undefined}
      className={`group/wl inline-flex max-w-full items-center gap-1.5 rounded px-1 py-0.5 text-left transition-colors hover:bg-[var(--surface-hover)] cursor-pointer ${color}`}
    >
      {Arrow && <Arrow className="h-3.5 w-3.5 shrink-0 text-[var(--text-ghost)]" />}
      {Icon && <Icon className="h-3.5 w-3.5 shrink-0 text-[var(--text-ghost)]" />}
      <span className="truncate text-sm group-hover/wl:underline">
        {name || "Untitled"}
      </span>
    </button>
  );
}

/** `[[Target]]` / `[[Target|shown text]]` — Wikipedia's wikilink syntax. */
const WIKILINK_RE = /\[\[([^\][|]+?)(?:\|([^\][]+?))?\]\]/g;

/**
 * Prose with inline wikilinks. Descriptions, statements, and directives are
 * written as plain text; `[[node-id]]` mentions render as live links showing
 * the node's CURRENT name (ids survive renames), `[[node-id|shown text]]`
 * overrides the display. A node name as the target also resolves — the
 * hand-typed form. Blue when built, red when plan-only ({@link isRedLink}),
 * red-unclickable when the target resolves to nothing (the dangling case:
 * the node was deleted, or the prose names something the model doesn't have).
 */
export function WikiText({
  text,
  nodes,
  onSelectNode,
}: {
  text: string;
  nodes: readonly Node[];
  onSelectNode: (id: string) => void;
}) {
  const parts: ReactNode[] = [];
  const re = new RegExp(WIKILINK_RE.source, "g");
  let last = 0;
  let m: RegExpExecArray | null;
  while ((m = re.exec(text))) {
    if (m.index > last) parts.push(text.slice(last, m.index));
    const name = m[1].trim();
    const target =
      nodes.find((n) => n.id === name) ??
      nodes.find((n) => n.name.trim().toLowerCase() === name.toLowerCase());
    const shown = (m[2] ?? target?.name ?? m[1]).trim();
    if (target) {
      const red = isRedLink(target);
      parts.push(
        <button
          key={m.index}
          type="button"
          onClick={() => onSelectNode(target.id)}
          title={red ? `${target.name} — planned, nothing built yet` : target.name}
          className={`inline cursor-pointer rounded-sm text-left hover:underline ${
            red ? "text-red-700 dark:text-red-400" : "text-blue-700 dark:text-blue-400"
          }`}
        >
          {shown}
        </button>,
      );
    } else {
      parts.push(
        <span
          key={m.index}
          title={`“${name}” doesn't resolve to any node in the model`}
          className="text-red-700/80 dark:text-red-400/80"
        >
          {shown}
        </span>,
      );
    }
    last = m.index + m[0].length;
  }
  if (parts.length === 0) return <>{text}</>;
  if (last < text.length) parts.push(text.slice(last));
  return <>{parts}</>;
}

/** Scroll an element into view and flash it briefly — the citation-jump
 *  affordance between claims and their source hunks. */
export function jumpTo(elementId: string) {
  const el = document.getElementById(elementId);
  if (!el) return;
  el.scrollIntoView({ behavior: "smooth", block: "center" });
  el.classList.remove("flash-target");
  // Force a reflow so re-adding the class restarts the animation.
  void el.offsetWidth;
  el.classList.add("flash-target");
  window.setTimeout(() => el.classList.remove("flash-target"), 1400);
}
