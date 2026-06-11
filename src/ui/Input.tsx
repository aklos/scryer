import type { InputHTMLAttributes } from "react";

type InputVariant = "title" | "inline" | "bordered";

// One consistent field treatment: raised surface, visible border, neutral
// focus ring (the focused border darkens instead of borrowing a status hue).
const base =
  "w-full rounded-md border border-[var(--border)] bg-[var(--surface-raised)] text-[var(--text)] outline-none transition-colors placeholder:text-[var(--text-ghost)] focus:border-[var(--border-strong)] focus:ring-1 focus:ring-[var(--border-strong)]";

const variants: Record<InputVariant, string> = {
  /** Page-title rename field — matches the h1 metrics so nothing jumps. */
  title: `${base} px-2 py-1 text-xl font-semibold leading-tight`,
  /** Compact field for tight rows (infobox, meta lines). */
  inline: `${base} px-2 py-1 text-xs`,
  /** The default form field. */
  bordered: `${base} px-2.5 py-1.5 text-sm`,
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
