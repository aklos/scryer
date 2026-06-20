/**
 * Shared building blocks for the node/group pages and the tree, following
 * Wikipedia's pattern language:
 *
 *  - PageSection — underlined section heading with a per-section [edit] link.
 *  - Banner — the "ambox" maintenance notice pinned to the top of a page
 *    (stale claims, drift, undescribed behaviour), with verdict actions inline.
 *  - WikiLink — inline cross-reference. A plain blue link to a real page; in
 *    prose (WikiText) an unresolvable target reads red, the only redlink case.
 */

import {
  createContext,
  useContext,
  useEffect,
  useRef,
  useState,
  type ComponentType,
  type Dispatch,
  type ReactNode,
  type SetStateAction,
} from "react";
import { createPortal } from "react-dom";
import type { LucideProps } from "lucide-react";
import { ArrowDownLeft, ArrowUpRight } from "lucide-react";
import type { Node } from "./viewmodel";

/** A section's edit controls (Cancel/Done) render into the section header's
 *  action lane via this slot — the mockup puts them in the `.h2row`, not the
 *  form footer. PageSection provides the slot element while editing; the form's
 *  {@link SectionEditor} portals its buttons into it (falling back to its own
 *  footer when there's no surrounding PageSection, e.g. the lede). */
const SectionActionsContext = createContext<HTMLElement | null>(null);

const EMPTY_HINT =
  "Empty — no responsibilities or properties. Give it a business responsibility or remove the node.";

// Field length caps, in characters. The description cap mirrors the backend
// validator (scryer-core `DESCRIPTION_MAX_CHARS`); the name cap is a UI-only
// limit on human-authored titles (symbol names are exempt — they're bound to
// real code identifiers).
export const DESCRIPTION_MAX = 200;
export const NAME_MAX = 40;

/** Coerce text to a valid source identifier: drop anything that isn't
 *  `[A-Za-z0-9_]`, and a leading character that can't start one. Mirrors the
 *  backend's `is_valid_identifier` (which allows an uppercase start). */
export function sanitizeIdentifier(text: string): string {
  const cleaned = text.replace(/[^A-Za-z0-9_]/g, "");
  return cleaned.replace(/^[0-9]+/, "");
}

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

/** One button system for the page (the mockup's `.btn`): bordered, sentence
 *  case, color = role. Set off the mono content in its own lane. */
const BTN_BASE =
  "pointer-events-auto inline-flex items-center gap-1 rounded-[5px] border px-2.5 py-0.5 text-[11px] transition-colors whitespace-nowrap";
export const BTN = `${BTN_BASE} border-[var(--border-strong)] bg-[var(--surface-hover)] text-[var(--text-secondary)] hover:bg-[var(--surface-active)] hover:text-[var(--text)]`;
export const BTN_GO = `${BTN_BASE} border-emerald-500/45 bg-emerald-500/10 text-emerald-600 hover:bg-emerald-500/20 dark:text-emerald-400`;
export const BTN_DANGER = `${BTN_BASE} border-red-500/45 bg-red-500/10 text-red-600 hover:bg-red-500/20 dark:text-red-400`;
/** Spawns an AI agent (a billable, possibly long-running fill). Violet is the
 *  agent signal throughout — the powerline's launch readout and activity pole —
 *  so every button that launches one carries it, warning the user before the
 *  click what kind of action this is. */
export const BTN_AGENT = `${BTN_BASE} border-violet-500/45 bg-violet-500/10 text-violet-600 hover:bg-violet-500/20 dark:text-violet-400`;

/** Per-row edit controls (the mockup's `.ctl`): floated over the row's right
 *  edge, top-aligned to the first line, with a gradient fade so they read over
 *  the text and take NO layout space — the field keeps its full read-mode width
 *  and nothing reflows on edit. The gradient is `pointer-events-none` so a click
 *  just past the text falls through to the field (caret lands at the end); the
 *  buttons re-enable pointer events themselves. Hidden until row hover; pair
 *  with a `relative` row and a `group/erow` ancestor. */
export const CTL =
  "pointer-events-none invisible absolute right-0 top-0 z-10 flex h-6 items-center gap-1.5 pl-9 [background-image:linear-gradient(90deg,transparent,color-mix(in_srgb,var(--text)_4%,var(--surface-canvas))_28px)] group-hover/erow:visible";

/**
 * An in-place editable field — a `contentEditable` span, NOT a native input.
 * This is the mockup's trick: the edit field is the same element type as the
 * read content (a span with the same font/size/line-height), so toggling
 * read↔edit is pixel-identical — no font resize, no reflow, no baseline shift.
 *
 * Uncontrolled by design: the initial text is written once on mount and never
 * fed back from state (which would yank the caret). Edits report out through
 * `onInput`; the parent's draft stays in sync for the eventual commit.
 */
/** Move the caret to the end of a contentEditable element. */
function caretToEnd(el: HTMLElement) {
  const r = document.createRange();
  r.selectNodeContents(el);
  r.collapse(false);
  const sel = window.getSelection();
  sel?.removeAllRanges();
  sel?.addRange(r);
}

export function Editable({
  initial,
  placeholder,
  autoFocus,
  maxLength,
  sanitize,
  onInput,
  onCommit,
  className = "",
}: {
  initial: string;
  placeholder?: string;
  autoFocus?: boolean;
  /** Hard cap on length, counted in Unicode scalar values (characters, not
   *  UTF-16 units) to match the backend's `chars().count()`. Input past the cap —
   *  typed or pasted — is dropped. */
  maxLength?: number;
  /** Coerce the text to an allowed shape on every input (e.g. identifier chars).
   *  Runs before `maxLength`. */
  sanitize?: (text: string) => string;
  /** Fired on every keystroke — for draft-backed fields. */
  onInput?: (text: string) => void;
  /** Fired on blur — for fields that commit straight to the model. */
  onCommit?: (text: string) => void;
  className?: string;
}) {
  const ref = useRef<HTMLSpanElement>(null);
  // Coerce the live text to the allowed shape/length, rewriting the DOM (and
  // restoring the caret) only when it actually changed so typing never janks.
  const enforce = (el: HTMLElement): string => {
    const raw = el.textContent ?? "";
    let next = sanitize ? sanitize(raw) : raw;
    if (maxLength != null && [...next].length > maxLength) {
      next = [...next].slice(0, maxLength).join("");
    }
    if (next !== raw) {
      el.textContent = next;
      caretToEnd(el);
    }
    return next;
  };
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    if (el.textContent !== initial) el.textContent = initial;
    if (autoFocus) {
      el.focus();
      caretToEnd(el);
    }
    // Mount-only: stays uncontrolled so typing never resets the caret.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  return (
    <span
      ref={ref}
      role="textbox"
      contentEditable
      suppressContentEditableWarning
      data-placeholder={placeholder}
      onInput={(e) => onInput?.(enforce(e.currentTarget))}
      onBlur={(e) => onCommit?.(enforce(e.currentTarget))}
      onKeyDown={(e) => {
        // Plain text only — Enter commits the field (blur) rather than
        // injecting <div>/<br> markup; Escape bails out.
        if (e.key === "Enter") {
          e.preventDefault();
          e.currentTarget.blur();
        }
      }}
      className={`-mx-1 cursor-text whitespace-pre-wrap break-words rounded-[4px] px-1 outline-none transition-colors focus:bg-[var(--surface-active)] empty:before:text-[var(--text-muted)] empty:before:content-[attr(data-placeholder)] ${className}`}
    />
  );
}

/** The section-edit affordance — a bordered button, sentence-case. */
export function EditLink({
  editing,
  label,
  onClick,
  className = "",
}: {
  editing?: boolean;
  label?: string;
  onClick: () => void;
  className?: string;
}) {
  return (
    <button type="button" onClick={onClick} className={`${BTN} ${className}`}>
      {label ?? (editing ? "Done" : "Edit")}
    </button>
  );
}

/** A wiki-style section: a tiny uppercase eyebrow heading (with an optional
 *  count) on a rule, a hover-revealed [Edit] toggle, then the section body. */
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
  // While editing, the header exposes a slot the form's SectionEditor portals
  // its Cancel/Done into (the mockup's `.editbtns` live in the `.h2row`).
  const [actionsSlot, setActionsSlot] = useState<HTMLElement | null>(null);
  return (
    <SectionActionsContext.Provider value={actionsSlot}>
      <section
        // `py-2` + compensating margins (mt 26→18, -mb-2) bleed the edit-mode
        // bg 8px above/below the content — the vertical analog of `-mx-3 px-3` —
        // without moving content or changing inter-section spacing.
        className={`group/sec mt-[18px] -mb-2 flow-root -mx-3 rounded-md px-3 py-2 ${
          editing
            ? "bg-[color-mix(in_srgb,var(--text)_4%,transparent)] shadow-[inset_2px_0_0_0_var(--border-strong)]"
            : ""
        }`}
      >
        <div className="mb-2 flex items-end justify-between gap-2 border-b border-[var(--border)] pb-[5px]">
          <h2 className="text-[11px] font-semibold uppercase tracking-[0.07em] text-[var(--text-tertiary)]">
            {title}
            {count != null && count > 0 && (
              <span className="ml-1.5 font-mono text-[11px] font-normal normal-case tracking-normal text-[var(--text-ghost)]">
                {count}
              </span>
            )}
          </h2>
          <div className="flex items-center gap-2 pb-px">
            {right}
            {editing ? (
              // Filled by the form via the actions context (Cancel / Done).
              <span ref={setActionsSlot} className="flex items-center gap-2" />
            ) : (
              editable &&
              onToggleEdit && (
                <EditLink
                  editing={false}
                  onClick={onToggleEdit}
                  className="invisible group-hover/sec:visible"
                />
              )
            )}
          </div>
        </div>
        {children}
      </section>
    </SectionActionsContext.Provider>
  );
}

export function Empty({ children }: { children: ReactNode }) {
  return (
    <div className="font-mono text-xs leading-relaxed text-[var(--text-muted)]">
      {children}
    </div>
  );
}

/** A neutral segmented toggle in the wiki design's chrome — a bordered track
 *  whose active segment fills with the selection surface, like the [edit]
 *  buttons and selected rows on the node pages. The shared ui SegmentedControl
 *  still carries a solid-zinc active fill (off the neutral interaction
 *  contract), so the wiki pages use this instead. */
export function SegField<T extends string | number | undefined>({
  options,
  value,
  disabled,
  onChange,
}: {
  options: { value: T; label: ReactNode }[];
  value: T;
  disabled?: boolean;
  onChange: (value: T) => void;
}) {
  return (
    <div className="flex overflow-hidden rounded-[5px] border border-[var(--border-strong)]">
      {options.map((opt, i) => {
        const active = value === opt.value;
        return (
          <button
            key={String(opt.value ?? "__none__")}
            type="button"
            disabled={disabled}
            onClick={() => onChange(opt.value)}
            className={`flex-1 px-2 py-1 text-[11px] transition-colors disabled:opacity-50 ${
              i > 0 ? "border-l border-[var(--border)]" : ""
            } ${
              active
                ? "bg-[var(--surface-active)] font-medium text-[var(--text)]"
                : "bg-[var(--surface-raised)] text-[var(--text-tertiary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text-secondary)]"
            }`}
          >
            {opt.label}
          </button>
        );
      })}
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
  const slot = useContext(SectionActionsContext);
  const buttons = (
    <>
      <button type="button" onClick={onClose} className={BTN}>
        Cancel
      </button>
      <button
        type="button"
        onClick={() => {
          onCommit(draft);
          onClose();
        }}
        className={BTN_GO}
      >
        Done
      </button>
    </>
  );
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
      {/* Cancel/Done ride in the section header (mockup `.editbtns`); only when
          there's no surrounding PageSection do they fall back to the footer. */}
      {slot && createPortal(buttons, slot)}
      {children(draft, setDraft)}
      {(footerExtra || !slot) && (
        <div className="mt-2 flex items-center gap-2 border-t border-[var(--border-subtle)] pt-2">
          {footerExtra?.(setDraft)}
          {!slot && (
            <>
              <span className="flex-1" />
              {buttons}
            </>
          )}
        </div>
      )}
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

/**
 * An inline cross-reference to another node/group's page. A plain blue wikilink:
 * every target is a real, navigable page, so link colour no longer tries to
 * encode plan-vs-built (that reads on the target's own page). `muted` dims a
 * link that isn't declared (a code-suggested candidate).
 */
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
  const color = muted ? "text-[var(--text-muted)]" : "text-blue-700 dark:text-blue-400";
  return (
    <button
      type="button"
      onClick={onClick}
      className={`group/wl inline-flex max-w-full items-center gap-1.5 rounded px-1 py-0.5 text-left transition-colors hover:bg-[var(--surface-hover)] ${color}`}
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
 * hand-typed form. A resolved target is a plain blue link; an unresolvable one
 * is red and unclickable (the dangling case: the node was deleted, or the prose
 * names something the model doesn't have).
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
      parts.push(
        <button
          key={m.index}
          type="button"
          onClick={() => onSelectNode(target.id)}
          title={target.name}
          className="inline rounded-sm text-left text-blue-700 hover:underline dark:text-blue-400"
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
