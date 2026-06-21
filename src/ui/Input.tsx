import type { InputHTMLAttributes } from "react";

type InputVariant = "title" | "inline" | "bordered" | "ghost";

// One consistent field treatment: raised surface, visible border, neutral
// focus ring (the focused border darkens instead of borrowing a status hue).
const base =
  "w-full rounded-md border border-[var(--border)] bg-[var(--surface-raised)] text-[var(--text)] outline-none transition-colors placeholder:text-[var(--text-ghost)] focus:border-[var(--border-strong)] focus:ring-1 focus:ring-[var(--border-strong)]";

const variants: Record<InputVariant, string> = {
  /** Page-title rename field — matches the h1 metrics so nothing jumps. */
  title: `${base} px-2 py-1 text-xl font-semibold leading-tight`,
  /** Compact field for tight rows (meta lines). */
  inline: `${base} px-2 py-1 text-xs`,
  /** The default form field. */
  bordered: `${base} px-2.5 py-1.5 text-sm`,
  /** Borderless, transparent — for in-place wiki edit rows (directives etc.). */
  ghost:
    "-mx-1 w-full rounded border-0 bg-transparent px-1 text-[var(--text)] outline-none transition-colors placeholder:text-[var(--text-ghost)] hover:bg-[color-mix(in_srgb,var(--text)_5%,transparent)] focus:bg-[var(--surface-active)] focus:hover:bg-[var(--surface-active)]",
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
