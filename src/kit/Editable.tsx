import { useEffect, useRef, useState } from "react";
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
