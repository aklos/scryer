import type { ButtonHTMLAttributes, ReactNode } from "react";

type ButtonVariant = "primary" | "secondary" | "ghost" | "link";
type ButtonColor = "default" | "accent" | "danger";
type ButtonSize = "sm" | "md";

const base = "inline-flex items-center justify-center gap-1 cursor-pointer transition-colors disabled:opacity-50 disabled:cursor-default";

const variantClasses: Record<ButtonVariant, Record<ButtonColor, string>> = {
  primary: {
    default: "rounded-md bg-blue-500 text-white hover:bg-blue-600",
    accent: "rounded-md bg-violet-500 text-white hover:bg-violet-600",
    danger: "rounded-md bg-red-500 text-white hover:bg-red-600",
  },
  secondary: {
    default:
      "rounded-md border border-[var(--border-strong)] bg-[var(--surface-raised)] text-[var(--text-secondary)] hover:bg-[var(--surface)]",
    accent:
      "rounded-md border border-indigo-200 text-indigo-600 hover:bg-indigo-50 dark:border-indigo-700 dark:text-indigo-300 dark:hover:bg-indigo-950/40",
    danger:
      "rounded-md border border-red-200 text-red-600 hover:bg-red-50 dark:border-red-700 dark:text-red-300 dark:hover:bg-red-950/40",
  },
  ghost: {
    default:
      "rounded text-[var(--text-tertiary)] hover:bg-[var(--surface-hover)]",
    accent:
      "rounded text-indigo-500 hover:bg-indigo-100/60 dark:text-indigo-400 dark:hover:bg-indigo-950/40",
    danger:
      "rounded text-red-500 hover:bg-red-100/60 dark:text-red-400 dark:hover:bg-red-950/40",
  },
  link: {
    default:
      "w-fit self-center text-xs text-[var(--text-muted)] hover:text-[var(--text-secondary)]",
    accent:
      "w-fit self-center text-xs text-indigo-400 hover:text-indigo-600 dark:text-indigo-500 dark:hover:text-indigo-300",
    danger:
      "w-fit self-center text-xs text-red-400 hover:text-red-600 dark:text-red-500 dark:hover:text-red-300",
  },
};

const sizeClasses: Record<ButtonSize, string> = {
  sm: "px-2 py-1 text-[11px]",
  md: "px-3 py-1.5 text-xs",
};

export function Button({
  variant = "secondary",
  color = "default",
  size = "sm",
  children,
  className,
  ...props
}: {
  variant?: ButtonVariant;
  color?: ButtonColor;
  size?: ButtonSize;
  children: ReactNode;
} & Omit<ButtonHTMLAttributes<HTMLButtonElement>, "color">) {
  // Link buttons don't use size classes — they're just styled text
  const appliedSize = variant === "link" ? "" : sizeClasses[size];
  return (
    <button
      type="button"
      {...props}
      className={`${base} ${appliedSize} ${variantClasses[variant][color]} ${className ?? ""}`}
    >
      {children}
    </button>
  );
}
