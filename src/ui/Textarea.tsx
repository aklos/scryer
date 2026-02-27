import type { TextareaHTMLAttributes, Ref } from "react";

const base =
  "w-full rounded-md border border-zinc-200 dark:border-transparent bg-zinc-100/60 dark:bg-zinc-800/60 focus:bg-zinc-100 dark:focus:bg-zinc-700/60 px-2.5 py-2 text-xs text-zinc-700 dark:text-zinc-200 leading-relaxed outline-none resize-none transition-colors placeholder:text-zinc-400 dark:placeholder:text-zinc-500";

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
