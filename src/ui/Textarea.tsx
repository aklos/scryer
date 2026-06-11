import { useCallback, type TextareaHTMLAttributes, type Ref } from "react";

// Matches Input's field treatment; grows with its content instead of
// scrolling inside a fixed box.
const base =
  "w-full rounded-md border border-[var(--border)] bg-[var(--surface-raised)] px-2.5 py-1.5 text-sm text-[var(--text)] leading-relaxed outline-none resize-none transition-colors placeholder:text-[var(--text-ghost)] focus:border-[var(--border-strong)] focus:ring-1 focus:ring-[var(--border-strong)]";

export function Textarea({
  className,
  ref,
  ...props
}: TextareaHTMLAttributes<HTMLTextAreaElement> & { ref?: Ref<HTMLTextAreaElement> }) {
  // Auto-grow: size the box to its content on mount and on every input.
  const fit = useCallback((el: HTMLTextAreaElement | null) => {
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${el.scrollHeight + 2}px`;
  }, []);

  return (
    <textarea
      ref={(el) => {
        fit(el);
        if (typeof ref === "function") ref(el);
        else if (ref) (ref as React.RefObject<HTMLTextAreaElement | null>).current = el;
      }}
      onInput={(e) => fit(e.currentTarget)}
      {...props}
      className={`${base} ${className ?? ""}`}
    />
  );
}
