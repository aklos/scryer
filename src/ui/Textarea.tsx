import { useCallback, type TextareaHTMLAttributes, type Ref } from "react";

type TextareaVariant = "default" | "inline";

const VARIANTS: Record<TextareaVariant, string> = {
  // Matches Input's field treatment; grows with its content instead of
  // scrolling inside a fixed box.
  default:
    "w-full rounded-md border border-[var(--border)] bg-[var(--surface-raised)] px-2.5 py-1.5 text-sm text-[var(--text)] leading-relaxed outline-none resize-none transition-colors placeholder:text-[var(--text-ghost)] focus:border-[var(--border-strong)] focus:ring-1 focus:ring-[var(--border-strong)]",
  // Borderless, transparent — for in-place wiki edit rows where the field
  // must sit flush in the mono content lane with no chrome until focus.
  inline:
    "-mx-1 w-full resize-none rounded border-0 bg-transparent px-1 py-0 text-[var(--text)] outline-none transition-colors placeholder:text-[var(--text-ghost)] hover:bg-[color-mix(in_srgb,var(--text)_5%,transparent)] focus:bg-[var(--surface-active)] focus:hover:bg-[var(--surface-active)]",
};

export function Textarea({
  variant = "default",
  className,
  ref,
  ...props
}: TextareaHTMLAttributes<HTMLTextAreaElement> & {
  variant?: TextareaVariant;
  ref?: Ref<HTMLTextAreaElement>;
}) {
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
      className={`${VARIANTS[variant]} ${className ?? ""}`}
    />
  );
}
