import type { InputHTMLAttributes } from "react";

type InputVariant = "title" | "inline" | "bordered";
type FocusColor = "blue" | "indigo";

const focusClasses: Record<FocusColor, string> = {
  blue: "focus:border-blue-400",
  indigo: "focus:border-indigo-400",
};

const variants: Record<InputVariant, string> = {
  title:
    "w-full rounded border border-zinc-200 dark:border-transparent bg-zinc-100/60 dark:bg-zinc-800/60 focus:bg-zinc-100 dark:focus:bg-zinc-700/60 text-xs font-medium text-zinc-800 dark:text-zinc-100 outline-none placeholder:text-zinc-300 dark:placeholder:text-zinc-600 px-2 py-1 transition-colors",
  inline:
    "w-0 flex-1 text-right rounded border border-zinc-200 dark:border-transparent bg-zinc-100/60 dark:bg-zinc-800/60 focus:bg-zinc-100 dark:focus:bg-zinc-700/60 text-xs text-zinc-700 dark:text-zinc-200 outline-none px-1.5 py-0.5 transition-colors",
  bordered:
    "mt-0.5 w-full rounded border border-zinc-200 bg-white px-2 py-1.5 text-xs outline-none dark:border-zinc-600 dark:bg-zinc-700 dark:text-zinc-100",
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
