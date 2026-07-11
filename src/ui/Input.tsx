import type { InputHTMLAttributes } from "react";

type InputVariant = "title" | "inline" | "bordered" | "ghost";

// One consistent field treatment: the dedicated field surface, visible border,
// and the accent on focus — a live field is "where you are", so it carries the
// one interaction hue instead of a barely-darker gray.
const base =
  "w-full rounded-md border border-[var(--border)] bg-[var(--surface-field)] text-[var(--text)] outline-none transition-colors placeholder:text-[var(--text-ghost)] focus:border-[var(--accent)] focus:ring-1 focus:ring-[var(--accent)]";

const variants: Record<InputVariant, string> = {
  /** Page-title rename field — matches the h1 metrics so nothing jumps. */
  title: `${base} px-2 py-1 text-xl font-semibold leading-tight`,
  /** Compact field for tight rows (meta lines). */
  inline: `${base} px-2 py-1 text-xs`,
  /** The default form field. */
  bordered: `${base} px-2.5 py-1.5 text-sm`,
  /** Borderless, transparent — for in-place wiki edit rows (directives etc.). */
  ghost:
    "-mx-1 w-full rounded border-0 bg-transparent px-1 text-[var(--text)] outline-none transition-colors placeholder:text-[var(--text-ghost)] hover:bg-[color-mix(in_srgb,var(--text)_5%,transparent)] focus:bg-[var(--surface-field)] focus:hover:bg-[var(--surface-field)] focus:ring-1 focus:ring-[var(--accent)]",
};

export function Input({
  variant = "bordered",
  className,
  ...props
}: {
  variant?: InputVariant;
} & Omit<InputHTMLAttributes<HTMLInputElement>, "color">) {
  return <input {...props} className={`${variants[variant]} ${className ?? ""}`} />;
}
