import type { ReactNode } from "react";

/** Static field label + content wrapper (non-collapsible). */
export function Field({ label, trailing, children }: {
  label: string;
  trailing?: ReactNode;
  children: ReactNode;
}) {
  return (
    <div>
      <div className="sticky top-0 z-10 flex items-baseline justify-between mb-1 bg-zinc-50 dark:bg-zinc-900 py-0.5">
        <span className="text-xs text-zinc-500 dark:text-zinc-500">{label}</span>
        {trailing}
      </div>
      {children}
    </div>
  );
}
