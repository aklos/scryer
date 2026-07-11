/**
 * Shared building blocks for the node/group pages and the tree, following
 * Wikipedia's pattern language:
 *
 *  - PageSection — underlined section heading with a per-section [edit] link.
 *  - WikiLink — inline cross-reference. A plain blue link to a real page.
 */

import {
  createContext,
  Fragment,
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
import { ArrowDownLeft, ArrowUpRight, Bot } from "lucide-react";
import { wordDiff } from "./wordDiff";

/** A section's edit controls (Cancel/Done) render into the section header's
 *  action lane via this slot — the mockup puts them in the `.h2row`, not the
 *  form footer. PageSection provides the slot element while editing; the form's
 *  {@link SectionEditor} portals its buttons into it (falling back to its own
 *  footer when there's no surrounding PageSection, e.g. the lede). */
const SectionActionsContext = createContext<HTMLElement | null>(null);

const EMPTY_HINT =
  "Empty — no responsibilities or properties. Give it a business responsibility or remove the node.";

/** The one page content column every full-page surface shares: article (900) +
 *  gap (32) + rail (300) + horizontal padding (56), centered at wide widths so
 *  pages compose instead of hugging the tree and stranding a dead right region.
 *  Headers span their pane (border/surface are chrome) but wrap their content
 *  in this; special pages cap their inner prose narrower inside it. */
export const PAGE_COL = "mx-auto w-full max-w-[1288px] px-7";

// Field length caps, in characters. The description and technology caps mirror
// the backend validator (scryer-core `DESCRIPTION_MAX_CHARS` /
// `TECHNOLOGY_MAX_CHARS`); the name cap is a UI-only limit on human-authored
// titles (symbol names are exempt — they're bound to real code identifiers).
export const DESCRIPTION_MAX = 200;
export const TECHNOLOGY_MAX = 80;
export const NAME_MAX = 40;

/** Inline word-level diff: unchanged text plain, added words highlighted, removed
 *  words struck through. Used for reworded claims, descriptions, and link labels. */
export function WordDiffText({ from, to }: { from: string; to: string }) {
  const segs = wordDiff(from, to);
  return (
    <>
      {segs.map((s, i) => {
        // A substitution (removed word immediately followed by its replacement)
        // glues the two together — "forin" — because the tokenizer keeps the
        // surrounding spaces in the unchanged runs, leaving none between the
        // changed words. Reinsert that gap so the strike and the pill read apart.
        const sep = i > 0 && segs[i - 1].kind !== "equal" && s.kind !== "equal";
        return (
          <Fragment key={i}>
            {sep && " "}
            {s.kind === "equal" ? (
              <span>{s.text}</span>
            ) : s.kind === "added" ? (
              <span className="rounded-xs bg-emerald-500/15 px-px text-emerald-700 dark:text-emerald-300">
                {s.text}
              </span>
            ) : (
              <del className="text-red-700 decoration-red-400/60 dark:text-red-400/90">
                {s.text}
              </del>
            )}
          </Fragment>
        );
      })}
    </>
  );
}

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

/** The eyebrow — the one spec for every micro uppercase section label. Size,
 *  weight, and tracking are fixed here so the idiom can't drift per file.
 *  EYEBROW_BASE carries the type spec; EYEBROW adds the default (tertiary)
 *  ink. Dimmed variants pair the base with their own ink token. */
export const EYEBROW_BASE = "text-2xs font-semibold uppercase tracking-[0.07em]";
export const EYEBROW = `${EYEBROW_BASE} text-[var(--text-tertiary)]`;

/** One button system for the page (the mockup's `.btn`): bordered, sentence
 *  case, color = role. Set off the mono content in its own lane. */
const BTN_BASE =
  "pointer-events-auto inline-flex items-center gap-1 rounded border px-2.5 py-0.5 text-2xs transition-colors whitespace-nowrap";
export const BTN = `${BTN_BASE} border-[var(--border-strong)] bg-[var(--surface-hover)] text-[var(--text-secondary)] hover:bg-[var(--surface-active)] hover:text-[var(--text)]`;
export const BTN_GO = `${BTN_BASE} border-emerald-500/45 bg-emerald-500/10 text-emerald-700 hover:bg-emerald-500/20 dark:text-emerald-400`;
export const BTN_DANGER = `${BTN_BASE} border-red-500/45 bg-red-500/10 text-red-700 hover:bg-red-500/20 dark:text-red-400`;
/** Spawns an AI agent (a billable, possibly long-running fill). Violet is the
 *  agent signal throughout — the powerline's launch readout and activity pole —
 *  so every button that launches one carries it, warning the user before the
 *  click what kind of action this is. */
export const BTN_AGENT = `${BTN_BASE} border-violet-500/45 bg-violet-500/10 text-violet-600 hover:bg-violet-500/20 dark:text-violet-400`;

/** Chromeless icon button — window/panel dismissals, small toggles. Neutral
 *  ink that lifts on hover; no border so it sits quietly in header rows. */
export const BTN_ICON =
  "rounded p-0.5 text-[var(--text-tertiary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text)]";

/** Inline text link — the wikilink blue ({@link WikiLink}'s palette) for plain
 *  navigational text buttons that sit inside prose or data rows. */
export const LINK = "text-blue-700 hover:underline dark:text-blue-400";

/** The agent's mark — the Bot icon, violet per the color contract (violet =
 *  the agent). Em-sized so it scales with the surrounding type, and the
 *  default ink can be overridden via `className` (pass `""` to inherit, e.g.
 *  inside an already-violet button). */
export function AgentMark({
  className = "text-violet-500 dark:text-violet-400",
}: {
  className?: string;
}) {
  return (
    <Bot
      aria-hidden
      className={`inline-block h-[1.2em] w-[1.2em] shrink-0 select-none align-text-bottom ${className}`}
    />
  );
}

/** Per-row edit controls (the mockup's `.ctl`): floated over the row's right
 *  edge, top-aligned to the first line, with a gradient fade so they read over
 *  the text and take NO layout space — the field keeps its full read-mode width
 *  and nothing reflows on edit. The gradient is `pointer-events-none` so a click
 *  just past the text falls through to the field (caret lands at the end); the
 *  buttons re-enable pointer events themselves. Hidden until row hover; pair
 *  with a `relative` row and a `group/erow` ancestor. */
export const CTL =
  "pointer-events-none invisible absolute right-0 top-0 z-10 flex h-6 items-center gap-1.5 pl-9 [background-image:linear-gradient(90deg,transparent,var(--surface-tint)_28px)] group-hover/erow:visible";

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
  onEnter,
  onEscape,
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
  /** Fired on blur when the text differs from `initial` — for fields whose
   *  parent commits the draft. An untouched field never fires it. */
  onCommit?: (text: string) => void;
  /** Enter, instead of the default blur — e.g. the title editor's Done. */
  onEnter?: () => void;
  /** Escape while the field is UNCHANGED — e.g. the title editor's Cancel.
   *  (A changed field's first Escape reverts it to `initial` instead; with no
   *  handler, an unchanged field's Escape bubbles to the section editor.) */
  onEscape?: () => void;
  className?: string;
}) {
  const ref = useRef<HTMLSpanElement>(null);
  // Live length + focus, only to drive the near-cap counter. The field itself
  // stays uncontrolled — re-renders never touch its textContent.
  const [live, setLive] = useState(() => ({
    len: [...initial].length,
    focused: false,
  }));
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
  // Field text changed (by typing or by the Escape revert): sync the counter
  // and report to the parent's draft.
  const report = (text: string) => {
    setLive((s) => ({ ...s, len: [...text].length }));
    onInput?.(text);
  };
  // The counter only appears while focused and within 20 chars of the cap, so
  // it never adds noise to reading or to comfortable typing.
  const nearCap =
    maxLength != null && live.focused && live.len >= maxLength - 20;
  return (
    <>
      <span
        ref={ref}
        role="textbox"
        contentEditable
        suppressContentEditableWarning
        data-placeholder={placeholder}
        onInput={(e) => report(enforce(e.currentTarget))}
        onFocus={() => setLive((s) => ({ ...s, focused: true }))}
        onBlur={(e) => {
          setLive((s) => ({ ...s, focused: false }));
          const text = enforce(e.currentTarget);
          if (text !== initial) onCommit?.(text);
        }}
        onKeyDown={(e) => {
          // Plain text only — Enter commits (via onEnter or blur) rather than
          // injecting <div>/<br> markup. Escape is layered: a changed field
          // reverts to `initial` (and stays focused); an unchanged one exits via
          // onEscape, or bubbles so an enclosing SectionEditor can close.
          if (e.key === "Enter") {
            e.preventDefault();
            if (onEnter) onEnter();
            else e.currentTarget.blur();
          } else if (e.key === "Escape") {
            const el = e.currentTarget;
            if ((el.textContent ?? "") !== initial) {
              e.stopPropagation();
              el.textContent = initial;
              report(initial);
              caretToEnd(el);
            } else if (onEscape) {
              e.stopPropagation();
              onEscape();
            }
          }
        }}
        className={`-mx-1 cursor-text whitespace-pre-wrap break-words rounded px-1 caret-[var(--accent)] outline-none transition-colors focus:bg-[var(--surface-field)] focus:text-[var(--text)] focus:ring-1 focus:ring-[var(--accent)] empty:before:text-[var(--text-muted)] empty:before:content-[attr(data-placeholder)] ${className}`}
      />
      {nearCap && (
        <span
          aria-hidden
          className={`ml-1.5 select-none font-mono text-2xs tabular-nums ${
            live.len >= maxLength ? "text-red-500" : "text-[var(--text-muted)]"
          }`}
        >
          {live.len}/{maxLength}
        </span>
      )}
    </>
  );
}

/** The section-edit affordance — a bordered button, sentence-case. */
export function EditLink({
  label,
  onClick,
  className = "",
}: {
  label?: string;
  onClick: () => void;
  className?: string;
}) {
  return (
    <button type="button" onClick={onClick} className={`${BTN} ${className}`}>
      {label ?? "Edit"}
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
        data-section={title}
        // `py-2` + compensating margins (mt 26→18, -mb-2) bleed the edit-mode
        // bg 8px above/below the content — the vertical analog of `-mx-3 px-3` —
        // without moving content or changing inter-section spacing.
        className={`group/sec mt-[18px] -mb-2 flow-root -mx-3 rounded-md px-3 py-2 ${
          editing
            ? "bg-[var(--surface-inset)] ring-1 ring-[color-mix(in_srgb,var(--accent)_35%,transparent)]"
            : ""
        }`}
      >
        <div className="mb-2 flex items-end justify-between gap-2 border-b border-[var(--border)] pb-[5px]">
          <h2 className={EYEBROW}>
            {title}
            {count != null && count > 0 && (
              <span className="ml-1.5 font-mono text-2xs font-normal normal-case tracking-normal text-[var(--text-ghost)]">
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
    <div className="flex overflow-hidden rounded border border-[var(--border-strong)]">
      {options.map((opt, i) => {
        const active = value === opt.value;
        return (
          <button
            key={String(opt.value ?? "__none__")}
            type="button"
            disabled={disabled}
            onClick={() => onChange(opt.value)}
            className={`flex-1 px-2 py-1 text-2xs transition-colors disabled:opacity-50 ${
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
      <button type="button" data-act="cancel" onClick={onClose} className={BTN}>
        Cancel
      </button>
      <button
        type="button"
        data-act="commit"
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
