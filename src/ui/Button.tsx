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
      "rounded-md border border-zinc-200 bg-white text-zinc-600 hover:bg-zinc-50 dark:border-zinc-600 dark:bg-zinc-800 dark:text-zinc-300 dark:hover:bg-zinc-700",
    accent:
      "rounded-md border border-indigo-200 text-indigo-600 hover:bg-indigo-50 dark:border-indigo-700 dark:text-indigo-300 dark:hover:bg-indigo-950/40",
    danger:
      "rounded-md border border-red-200 text-red-600 hover:bg-red-50 dark:border-red-700 dark:text-red-300 dark:hover:bg-red-950/40",
  },
  ghost: {
    default:
      "rounded text-zinc-500 hover:bg-zinc-200/60 dark:text-zinc-400 dark:hover:bg-zinc-700/60",
    accent:
      "rounded text-indigo-500 hover:bg-indigo-100/60 dark:text-indigo-400 dark:hover:bg-indigo-950/40",
    danger:
      "rounded text-red-500 hover:bg-red-100/60 dark:text-red-400 dark:hover:bg-red-950/40",
  },
  link: {
    default:
      "text-xs text-zinc-400 hover:text-zinc-600 dark:text-zinc-500 dark:hover:text-zinc-300",
    accent:
      "text-xs text-indigo-400 hover:text-indigo-600 dark:text-indigo-500 dark:hover:text-indigo-300",
    danger:
      "text-xs text-red-400 hover:text-red-600 dark:text-red-500 dark:hover:text-red-300",
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
