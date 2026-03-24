import type { InputHTMLAttributes } from "react";

type InputVariant = "title" | "inline" | "bordered";
type FocusColor = "blue" | "indigo";

const focusClasses: Record<FocusColor, string> = {
  blue: "focus:border-blue-400",
  indigo: "focus:border-indigo-400",
};

const variants: Record<InputVariant, string> = {
  title:
    "w-full rounded border border-[var(--border)] bg-[var(--surface-raised)] text-xs font-medium text-[var(--text)] outline-none placeholder:text-[var(--text-ghost)] px-2 py-1 transition-colors focus:ring-1 focus:ring-blue-400/30",
  inline:
    "w-0 flex-1 text-right rounded border border-[var(--border)] bg-[var(--surface-raised)] text-xs text-[var(--text-secondary)] outline-none px-1.5 py-0.5 transition-colors focus:ring-1 focus:ring-blue-400/30",
  bordered:
    "mt-0.5 w-full rounded border border-[var(--border-strong)] bg-[var(--surface-raised)] px-2 py-1.5 text-xs text-[var(--text)] outline-none focus:ring-1 focus:ring-blue-400/30",
};

export function Input({
  variant = "bordered",
  focusColor = "blue",
  className,
  ...props
}: {
  variant?: InputVariant;
  focusColor?: FocusColor;
} & Omit<InputHTMLAttributes<HTMLInputElement>, "color">) {
  return (
    <input
      {...props}
      className={`${variants[variant]} ${focusClasses[focusColor]} ${className ?? ""}`}
    />
  );
}
