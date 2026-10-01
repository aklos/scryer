import { useEffect, useRef, useState, type ReactNode } from "react";
import { BTN } from "./tokens";

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

/** The current selection as character offsets within `el`, or null when the
 *  selection lives elsewhere. */
function selectionOffsets(el: HTMLElement): [number, number] | null {
  const sel = window.getSelection();
  if (!sel || sel.rangeCount === 0) return null;
  const range = sel.getRangeAt(0);
  if (!el.contains(range.startContainer) || !el.contains(range.endContainer)) return null;
  const pre = document.createRange();
  pre.selectNodeContents(el);
  pre.setEnd(range.startContainer, range.startOffset);
  const start = pre.toString().length;
  return [start, start + range.toString().length];
}

/** Place the caret at character offset `at` (the field is plain text, so the
 *  offset walks the text nodes directly). */
function caretToOffset(el: HTMLElement, at: number) {
  const walker = document.createTreeWalker(el, NodeFilter.SHOW_TEXT);
  let remaining = at;
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    const len = node.textContent?.length ?? 0;
    if (remaining <= len) {
      const r = document.createRange();
      r.setStart(node, remaining);
      r.collapse(true);
      const sel = window.getSelection();
      sel?.removeAllRanges();
      sel?.addRange(r);
      return;
    }
    remaining -= len;
  }
  caretToEnd(el);
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
  mirror,
  containerClassName = "",
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
  /** Live styled preview of the field's raw text, rendered CHAR-EXACT on an
   *  overlay while the text itself goes transparent — the field edits markered
   *  source with the styling visible in place. Only safe in a monospace
   *  context (the overlay must not change metrics). Enables Cmd/Ctrl+B to
   *  wrap the selection in `**` markers. */
  mirror?: (text: string) => ReactNode;
  /** Mirror mode only: classes for the positioning wrapper (the hover/focus
   *  surface), while `className` carries the text-box classes shared by both
   *  layers. */
  containerClassName?: string;
  className?: string;
}) {
  const ref = useRef<HTMLSpanElement>(null);
  // Live length + focus for the near-cap counter, plus the raw text when a
  // mirror needs re-rendering per keystroke. The field itself stays
  // uncontrolled — re-renders never touch its textContent.
  const [live, setLive] = useState(() => ({
    len: [...initial].length,
    focused: false,
    text: initial,
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
    setLive((s) => ({ ...s, len: [...text].length, text }));
    onInput?.(text);
  };
  // Mirror mode's manual override: wrap the selected text in markers. The
  // rewrite goes through textContent (the field is plain text), so the mirror,
  // the draft, and the caret all resync in one place.
  const wrapSelection = (el: HTMLElement, marker: string) => {
    const sel = selectionOffsets(el);
    if (!sel || sel[0] === sel[1]) return;
    const [start, end] = sel;
    const text = el.textContent ?? "";
    const next =
      text.slice(0, start) + marker + text.slice(start, end) + marker + text.slice(end);
    el.textContent = next;
    caretToOffset(el, end + marker.length * 2);
    report(next);
  };
  // The counter only appears while focused and within 20 chars of the cap, so
  // it never adds noise to reading or to comfortable typing.
  const nearCap =
    maxLength != null && live.focused && live.len >= maxLength - 20;
  // Mirror mode splits the field into layers: the contentEditable keeps the
  // caret, selection, and focus surface but its text goes transparent; the
  // overlay renders the same characters styled. Both share `className` (the
  // text-box classes), so their metrics — and therefore wrap points and caret
  // positions — coincide.
  const field = (
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
          if (mirror && (e.metaKey || e.ctrlKey) && e.key === "b") {
            // Marker formatting — intercepted so the browser's rich-text
            // bold never injects <b> into the plain-text field.
            e.preventDefault();
            wrapSelection(e.currentTarget, "**");
          } else if (e.key === "Enter") {
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
        className={
          mirror
            ? `block cursor-text whitespace-pre-wrap break-words px-1 caret-[var(--accent)] outline-none !text-transparent empty:before:text-[var(--text-muted)] empty:before:content-[attr(data-placeholder)] ${className}`
            : `-mx-1 cursor-text whitespace-pre-wrap break-words rounded px-1 caret-[var(--accent)] outline-none transition-colors focus:bg-[var(--surface-field)] focus:text-[var(--text)] focus:ring-1 focus:ring-[var(--accent)] empty:before:text-[var(--text-muted)] empty:before:content-[attr(data-placeholder)] ${className}`
        }
      />
  );
  return (
    <>
      {mirror ? (
        <span
          className={`relative -mx-1 block rounded transition-colors focus-within:bg-[var(--surface-field)] focus-within:ring-1 focus-within:ring-[var(--accent)] ${containerClassName}`}
        >
          {field}
          <span
            aria-hidden
            className={`pointer-events-none absolute inset-0 block select-none whitespace-pre-wrap break-words px-1 text-[var(--text)] ${className}`}
          >
            {mirror(live.text)}
          </span>
        </span>
      ) : (
        field
      )}
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
