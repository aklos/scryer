import type { ReactNode } from "react";

/** A JSON-style key label: monospace, muted, with trailing colon. */
export function JsonKey({ label, children, className = "" }: { label: string; children?: ReactNode; className?: string }) {
  return (
    <div className={`flex items-baseline gap-1.5 font-mono text-[11px] ${className}`}>
      <span className="text-[var(--text-ghost)] select-none">{label}:</span>
      {children}
    </div>
  );
}

/** Small inline JSON-style value (e.g. enum strings, kinds) — quoted, dimmed */
export function JsonString({ value }: { value: string }) {
  return <span className="font-mono text-[11px] text-[var(--text-tertiary)]">"{value}"</span>;
}

/** A top-level section header in JSON-mirror style — used for `node:`, `descendants:`, etc. */
export function SectionKey({ label, count }: { label: string; count?: number }) {
  return (
    <div className="flex items-baseline gap-1.5 px-1 py-1 font-mono text-[11px]">
      <span className="text-[var(--text-tertiary)] font-semibold tracking-wide select-none">{label}</span>
      <span className="text-[var(--text-ghost)] select-none">:</span>
      {count != null && <span className="text-[var(--text-ghost)] select-none">[{count}]</span>}
    </div>
  );
}
