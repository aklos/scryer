import type { TextareaHTMLAttributes, Ref } from "react";

const base =
  "w-full rounded-md border border-[var(--border)] bg-[var(--surface-raised)] px-2.5 py-2 text-xs text-[var(--text-secondary)] leading-relaxed outline-none resize-none transition-colors placeholder:text-[var(--text-muted)] focus:ring-1 focus:ring-blue-400/30";

export function Textarea({
  className,
  ref,
  ...props
}: TextareaHTMLAttributes<HTMLTextAreaElement> & { ref?: Ref<HTMLTextAreaElement> }) {
  return (
    <textarea
      ref={ref}
      {...props}
      className={`${base} ${className ?? ""}`}
    />
  );
}
