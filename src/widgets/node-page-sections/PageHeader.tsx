/**
 * A page's header: breadcrumb (`Crumbs` over `ancestorChain`), the
 * title with in-place rename, the type line, and the page's action buttons.
 */

import type { ScryModel, Node } from "../../entities/model/viewmodel";
import type { Editor } from "../../entities/model/editor";
import {
  BTN,
  BTN_GO,
  Editable,
  EditLink,
  PAGE_COL,
} from "../../shared/ui/pagekit";

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
      </div>
    </header>
  );
}

/** A maintenance notice (ambox) — a full-width banner stacked at the top of the
 *  article body: a translucent wash and border in the tone's accent, the icon
 *  in full hue, inline actions right-aligned. */
export function Ambox({
  tone,
  icon,
  children,
  actions,
}: {
  /** `note` is neutral: something to decide, not something wrong. */
  tone: "warning" | "danger" | "info" | "note";
  icon: React.ReactNode;
  children: React.ReactNode;
  actions?: React.ReactNode;
}) {
  const iconTone =
    tone === "danger"
      ? "text-red-600 dark:text-red-400"
      : tone === "info"
        ? "text-violet-600 dark:text-violet-400"
        : tone === "note"
          ? "text-[var(--text-tertiary)]"
          : "text-orange-600 dark:text-orange-400";
  const wash =
    tone === "danger"
      ? "border-red-500/30 bg-red-500/10"
      : tone === "info"
        ? "border-violet-500/30 bg-violet-500/10"
        : tone === "note"
          ? "border-[var(--border-strong)] bg-[var(--surface-tint)]"
          : "border-orange-500/30 bg-orange-500/10";
  return (
    <div className={`flex items-center gap-2.5 rounded-md border ${wash} px-3 py-2 text-xs text-[var(--text-secondary)]`}>
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

