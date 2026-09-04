import { FileClock } from "lucide-react";
import type { ScryModel, Node } from "../viewmodel";
import type { Editor } from "../editor";
import { DiffRow, diffAnchorClass, diffTextClass, kindOfGlyph } from "../diffkit";
import { EVENT_META, type HistoryEvent, relativeTime } from "../history";
import { StatementText } from "../markup";
import { ClaimSource } from "../SourceSection";
import {
  BTN,
  BTN_GO,
  Editable,
  EditLink,
  AgentMark,
  PAGE_COL,
} from "../pagekit";

// --- header -----------------------------------------------------------------

/** Root-first ancestor chain (excluding the node itself). */
export function ancestorChain(model: ScryModel, startParentId: string | undefined | null) {
  const chain: Node[] = [];
  const seen = new Set<string>();
  let cur = startParentId ? model.nodes.find((n) => n.id === startParentId) : undefined;
  while (cur && !seen.has(cur.id)) {
    seen.add(cur.id);
    chain.unshift(cur);
    cur = cur.parentId ? model.nodes.find((n) => n.id === cur!.parentId) : undefined;
  }
  return chain;
}

export function Crumbs({
  chain,
  onSelectNode,
}: {
  chain: Node[];
  onSelectNode: (id: string) => void;
}) {
  if (chain.length === 0) return null;
  // Inherits the header crumb row's font (mono) / size (11px) / color.
  return (
    <nav className="flex min-w-0 items-center gap-1">
      {chain.map((n, i) => (
        <span key={n.id} className="flex min-w-0 items-center gap-1">
          {i > 0 && <span className="text-[var(--text-ghost)]">/</span>}
          <button
            type="button"
            onClick={() => onSelectNode(n.id)}
            className="max-w-[200px] truncate hover:text-[var(--text-secondary)] hover:underline"
          >
            {n.name || "Untitled"}
          </button>
        </span>
      ))}
    </nav>
  );
}

export function PageHeader({
  crumbs,
  actions,
  name,
  typeLine,
  tabs,
  editor,
  editingName,
  onToggleName,
  onNameInput,
  onDone,
  onCancel,
  nameMaxLength,
  nameSanitize,
}: {
  crumbs: React.ReactNode;
  /** Page-level actions, right-aligned on the crumb line. */
  actions?: React.ReactNode;
  name: string;
  /** The line under the title: kind icon, type word, technology, status. */
  typeLine: React.ReactNode;
  /** Article tabs (Overview · History), rendered as the header's last row. */
  tabs?: React.ReactNode;
  editor: Editor | undefined;
  editingName: boolean;
  onToggleName: () => void;
  /** Per-keystroke draft update. Nothing reaches the model until {@link onDone}. */
  onNameInput: (v: string) => void;
  /** Commit the title/type-line draft and close — the header's Done. */
  onDone: () => void;
  /** Discard the draft and close. Omit to hide the Cancel button. */
  onCancel?: () => void;
  /** Hard character cap on the title (omitted for symbol names, which are
   *  identifier-shaped rather than length-bound). */
  nameMaxLength?: number;
  /** Coerce the title's shape on input (symbol names → source identifiers). */
  nameSanitize?: (text: string) => string;
}) {
  return (
    // The header spans the pane (its rule and surface are chrome), but its
    // content shares the page's bounded column (PAGE_COL) so title, gauges,
    // article and rail all hang on the same grid at any window width.
    <header className="shrink-0 border-b border-[var(--border)] pt-[13px]">
      <div className={PAGE_COL}>
      <div className="flex min-h-[15px] items-center gap-1 font-mono text-xs text-[var(--text-tertiary)]">
        {crumbs}
        <span className="flex-1" />
        {actions}
      </div>
      <div className="mt-[5px] flex items-start gap-4">
        {editingName ? (
          // The title edits in place as a contentEditable span (same metrics as
          // the h1, no reflow). It edits a DRAFT — like every section, nothing
          // reaches the model until Done; Cancel and navigation discard. It's
          // `inline-block` so it grows with the text rather than spanning the
          // header; the buttons are pinned right by a flex spacer. Edit mode
          // stays open across fields — so you can edit the type line too.
          <div className="flex min-w-0 flex-1 items-baseline gap-2">
            <Editable
              initial={name}
              autoFocus
              maxLength={nameMaxLength}
              sanitize={nameSanitize}
              placeholder="Untitled"
              onInput={onNameInput}
              onEnter={onDone}
              onEscape={onCancel}
              className="inline-block max-w-full text-xl font-semibold leading-tight text-[var(--text)]"
            />
            <span className="flex-1" />
            <span className="flex shrink-0 items-center gap-2">
              {onCancel && (
                <button type="button" onClick={onCancel} className={BTN}>
                  Cancel
                </button>
              )}
              <button type="button" onClick={onDone} className={BTN_GO}>
                Done
              </button>
            </span>
          </div>
        ) : (
          <div className="flex min-w-0 flex-1 items-baseline gap-3">
            <h1 className="min-w-0 flex-1 truncate text-xl font-semibold leading-tight text-[var(--text)]">
              {name || "Untitled"}
            </h1>
            {editor && <EditLink onClick={onToggleName} />}
          </div>
        )}
      </div>
      <div className="mt-[3px] flex items-center gap-2 text-xs text-[var(--text-tertiary)]">
        {typeLine}
      </div>
      {tabs}
      </div>
    </header>
  );
}

/** A maintenance notice (ambox) — a full-width banner stacked at the top of the
 *  article body: neutral chrome on an inset surface, the icon alone carrying
 *  the semantic hue (the toast recipe), inline actions right-aligned. The wiki
 *  hatnote's job without the tinted-callout look. */
export function Ambox({
  tone,
  icon,
  children,
  actions,
}: {
  tone: "warning" | "danger" | "info";
  icon: React.ReactNode;
  children: React.ReactNode;
  actions?: React.ReactNode;
}) {
  const iconTone =
    tone === "danger"
      ? "text-red-600 dark:text-red-400"
      : tone === "info"
        ? "text-violet-600 dark:text-violet-400"
        : "text-orange-600 dark:text-orange-400";
  return (
    <div className="flex items-center gap-2.5 rounded-md border border-[var(--border)] bg-[var(--surface-inset)] px-3 py-2 text-xs text-[var(--text-secondary)]">
      <span className={`shrink-0 ${iconTone}`}>{icon}</span>
      <span className="min-w-0 flex-1">{children}</span>
      {actions && <span className="flex shrink-0 items-center gap-3">{actions}</span>}
    </div>
  );
}

/** Inline text action for an {@link Ambox} — terse, underlined, no chrome. Full
 *  ink against the notice's secondary text; the hue stays in the icon. */
export const NOTICE_ACTION =
  "shrink-0 font-medium text-[var(--text)] underline-offset-2 hover:underline";

// --- tabs -------------------------------------------------------------------

/** Article tabs (Overview · History) — the mockup's `.modes` underline tabs,
 *  rendered as the last row of the page header. */
export function PageTabs({
  tab,
  onTab,
  historyCount,
}: {
  tab: "overview" | "history";
  onTab: (t: "overview" | "history") => void;
  historyCount: number;
}) {
  // No transition: animating border-color ghosts the outgoing underline.
  // Baseline flex keeps the label and its mono count on one line regardless of
  // their differing type sizes.
  const tabClass = (active: boolean) =>
    `-mb-px mr-[18px] flex items-baseline gap-1.5 border-b-2 py-1.5 text-sm ${
      active
        ? "border-[var(--text)] text-[var(--text)]"
        : "border-transparent text-[var(--text-tertiary)] hover:text-[var(--text-secondary)]"
    }`;
  return (
    <div className="mt-[11px] flex">
      <button type="button" onClick={() => onTab("overview")} className={tabClass(tab === "overview")}>
        Overview
      </button>
      <button type="button" onClick={() => onTab("history")} className={tabClass(tab === "history")}>
        History
        {historyCount > 0 && (
          <span className="font-mono text-2xs text-[var(--text-ghost)]">{historyCount}</span>
        )}
      </button>
    </div>
  );
}

/** The History tab body: this node's durable committed-model timeline — every
 *  fold, drift reconcile, move and birth, newest first. Each event reads as a
 *  rail dot coloured by kind, a driver badge, attribution, and its diff rows
 *  (with inline source peeks for the claims an `impl` discharged). */
export function NodeHistory({
  events,
  projectPath,
}: {
  events: readonly HistoryEvent[];
  projectPath: string | null;
}) {
  if (events.length === 0) {
    return (
      <div className="flex flex-col items-center gap-3 px-6 py-16 text-center">
        <FileClock className="h-6 w-6 text-[var(--text-ghost)]" />
        <p className="max-w-sm text-xs text-[var(--text-muted)]">
          No committed history yet. When the agent implements, reconciles drift, moves, or builds
          this node, it lands here.
        </p>
      </div>
    );
  }
  // Stored oldest-first (append-only); the timeline reads newest-first.
  const ordered = [...events].reverse();
  return (
    <div className="pt-5">
      {ordered.map((ev, i) => {
        const meta = EVENT_META[ev.kind];
        const last = i === ordered.length - 1;
        return (
          <div
            key={`${ev.at}-${i}`}
            className={`relative ml-1.5 border-l pl-6 pb-6 ${
              last ? "border-transparent" : "border-[var(--border)]"
            }`}
          >
            <span
              className="absolute -left-[5px] top-1 h-2.5 w-2.5 rounded-full"
              style={{ background: meta.dot, boxShadow: "0 0 0 2px var(--surface)" }}
            />
            <div className="mb-2 flex flex-wrap items-baseline gap-2">
              <span className="font-mono text-2xs tabular-nums text-[var(--text-tertiary)]">
                {relativeTime(ev.at)}
              </span>
              <span
                className={`rounded border px-1.5 py-px font-mono text-2xs uppercase tracking-[0.07em] ${meta.badge}`}
              >
                {meta.label}
              </span>
              <span className="text-xs text-[var(--text-muted)]">
                <AgentMark /> {ev.by} · {ev.driver}
              </span>
            </div>
            <div className="flex flex-col gap-1">
              {/* The same rows the Overview showed while these edits were
                  pending — same glyphs, same treatment, now as the record. */}
              {ev.rows.map((row, j) => (
                <DiffRow key={j} marker={row.marker}>
                  <span
                    className={`font-mono text-sm leading-relaxed ${diffTextClass(kindOfGlyph(row.marker))}`}
                  >
                    {/* A row's text is a claim statement as it was written, so
                        it carries the statement markup — render it, never print
                        the markers. Rows that aren't statements (a node's birth
                        summary, a reparent line) hold no markers and pass
                        through unchanged. */}
                    <StatementText
                      text={row.text}
                      anchor={diffAnchorClass(kindOfGlyph(row.marker))}
                    />
                  </span>
                  {row.source && (
                    // Bleed spec: undo DiffRow's 16px gutter + 4px gap; the
                    // peek then spans from the timeline's content edge to the
                    // column edge (never crossing the timeline rule itself).
                    <ClaimSource
                      locations={[row.source]}
                      projectPath={projectPath}
                      bleed="-ml-5"
                    />
                  )}
                </DiffRow>
              ))}
            </div>
          </div>
        );
      })}
    </div>
  );
}
