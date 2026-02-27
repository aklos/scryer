import type { ReactNode } from "react";

/** Horizontal key-value row: muted label left, control right. */
export function KVRow({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-2">
      <span className="text-xs text-zinc-500 dark:text-zinc-500 shrink-0">{label}</span>
      {children}
    </div>
  );
}
