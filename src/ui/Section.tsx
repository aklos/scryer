import { useState, type ReactNode } from "react";
import { ChevronRight } from "lucide-react";

/** Collapsible disclosure section with rotating chevron. */
export function Section({ title, count, defaultOpen = true, children }: {
  title: string;
  count?: number;
  defaultOpen?: boolean;
  children: ReactNode;
}) {
  const [open, setOpen] = useState(defaultOpen);
  return (
    <div className="pt-1 first:pt-0">
      <button
        type="button"
        onClick={() => setOpen(!open)}
        className="flex w-full items-center gap-1 py-1 text-[11px] font-medium uppercase tracking-wider text-zinc-500 hover:text-zinc-600 dark:text-zinc-500 dark:hover:text-zinc-300 cursor-pointer select-none"
      >
        <ChevronRight className={`h-3 w-3 shrink-0 transition-transform ${open ? "rotate-90" : ""}`} />
        <span>{title}</span>
        {count != null && count > 0 && <span className="text-zinc-300 dark:text-zinc-600">{count}</span>}
      </button>
      {open && <div className="flex flex-col gap-2.5 pt-1 pb-1">{children}</div>}
    </div>
  );
}
