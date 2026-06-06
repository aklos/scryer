/**
 * Inline-edit text primitive. Looks like static copy by default; click to edit
 * in place. Faint dotted underline on hover signals editability. Enter/blur
 * commits (Esc cancels). For multiline, set `multiline` and Enter inserts a
 * newline instead — Cmd/Ctrl+Enter commits.
 *
 * Style props (`fontSize`, `fontFamily`, `className`) flow through so a caller
 * can drop one of these in place of a `<span>` without restyling.
 */

import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type CSSProperties,
  type KeyboardEvent,
} from "react";

export interface InlineTextProps {
  value: string;
  onCommit: (next: string) => void;
  placeholder?: string;
  multiline?: boolean;
  /** Auto-focus into edit mode on mount (e.g. brand-new cards). */
  autoEdit?: boolean;
  className?: string;
  style?: CSSProperties;
  /** Optional title attribute (for tooltip when truncated). */
  title?: string;
  /** Render with a slight muted appearance when value is empty. */
  emptyMuted?: boolean;
  /** Prevent click from bubbling into the surface's pickup detection. */
  stopPropagation?: boolean;
}

export function InlineText({
  value,
  onCommit,
  placeholder = "",
  multiline = false,
  autoEdit = false,
  className = "",
  style,
  title,
  emptyMuted = true,
  stopPropagation = true,
}: InlineTextProps) {
  const [editing, setEditing] = useState(autoEdit);
  const [draft, setDraft] = useState(value);
  const inputRef = useRef<HTMLInputElement | HTMLTextAreaElement | null>(null);

  // Keep draft in sync if value changes from outside while not editing.
  useEffect(() => {
    if (!editing) setDraft(value);
  }, [value, editing]);

  useLayoutEffect(() => {
    if (!editing) return;
    const el = inputRef.current;
    if (!el) return;
    el.focus();
    el.select();
    if (multiline) autoresize(el as HTMLTextAreaElement);
  }, [editing, multiline]);

  const commit = useCallback(() => {
    setEditing(false);
    // An auto-edit field opened empty still commits on blur even when left empty,
    // so the parent can drop a never-filled placeholder (e.g. a blank directive).
    if (draft !== value || (autoEdit && value === "")) onCommit(draft);
  }, [draft, value, onCommit, autoEdit]);

  const cancel = useCallback(() => {
    setDraft(value);
    setEditing(false);
  }, [value]);

  const handleKey = useCallback(
    (e: KeyboardEvent<HTMLInputElement | HTMLTextAreaElement>) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        cancel();
        return;
      }
      if (e.key === "Enter") {
        if (multiline && !(e.metaKey || e.ctrlKey)) {
          // Allow newline; let the textarea handle it.
          return;
        }
        e.preventDefault();
        e.stopPropagation();
        commit();
      }
    },
    [multiline, commit, cancel],
  );

  const handleStart = useCallback(
    (e: React.MouseEvent) => {
      if (stopPropagation) e.stopPropagation();
      setEditing(true);
    },
    [stopPropagation],
  );

  // Block surface drag/pickup while editing.
  const blockBubble = useCallback(
    (e: React.MouseEvent | React.PointerEvent) => {
      e.stopPropagation();
    },
    [],
  );

  if (editing) {
    const commonProps = {
      ref: inputRef as React.Ref<HTMLInputElement & HTMLTextAreaElement>,
      value: draft,
      onChange: (
        e: React.ChangeEvent<HTMLInputElement | HTMLTextAreaElement>,
      ) => {
        setDraft(e.target.value);
        if (multiline) autoresize(e.target as HTMLTextAreaElement);
      },
      onKeyDown: handleKey,
      onBlur: commit,
      onPointerDown: blockBubble,
      onMouseDown: blockBubble,
      placeholder,
      className: `${className} bg-transparent caret-[var(--text-secondary)] placeholder:text-[var(--text-ghost)] outline-none border-b border-dotted border-[var(--text-muted)] focus:border-solid focus:border-[var(--text-secondary)] w-full resize-none`,
      // Inline color so native inputs (which don't inherit by default) are
      // readable even when the caller forgets to set a text color. Caller's
      // own `style` overrides via spread.
      style: { color: "var(--text)", ...style },
      "data-no-pickup": true,
    };
    return multiline ? (
      <textarea rows={1} {...commonProps} />
    ) : (
      <input type="text" {...commonProps} />
    );
  }

  const isEmpty = value.length === 0;
  const display = isEmpty ? placeholder : value;
  const emptyClass =
    isEmpty && emptyMuted ? "text-[var(--text-ghost)] italic" : "";
  // Use a transparent dotted underline that becomes visible on hover — purely
  // visual feedback that this text is editable.
  return (
    <span
      role="button"
      tabIndex={0}
      data-no-pickup
      title={title}
      onClick={handleStart}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          setEditing(true);
        }
      }}
      onPointerDown={blockBubble}
      className={`${className} ${emptyClass} cursor-text border-b border-dotted border-transparent hover:border-[var(--text-ghost)]`}
      style={style}
    >
      {display || " " /* keep height when empty + no placeholder */}
    </span>
  );
}

function autoresize(el: HTMLTextAreaElement) {
  el.style.height = "auto";
  el.style.height = `${el.scrollHeight}px`;
}
