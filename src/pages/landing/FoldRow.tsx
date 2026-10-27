/**
 * One line of the landing that opens in place: the summary stays a single
 * row, and what is behind it shows only when asked for. A row with nothing
 * behind it has no chevron and does not toggle.
 */

import { useState, type ReactNode } from "react";
import { ChevronRight } from "lucide-react";

export function FoldRow({ summary, children }: { summary: ReactNode; children?: ReactNode }) {
  const [open, setOpen] = useState(false);
  const folds = children != null && children !== false;
  return (
    <li className="flex flex-col">
      <button
        type="button"
        aria-expanded={folds ? open : undefined}
        disabled={!folds}
        onClick={() => setOpen((v) => !v)}
        className="flex w-full min-w-0 items-center gap-2 rounded px-1 py-1 text-left text-sm enabled:hover:bg-[var(--surface-hover)]"
      >
        <ChevronRight
          className={`h-3.5 w-3.5 shrink-0 text-[var(--text-ghost)] transition-transform ${open ? "rotate-90" : ""} ${folds ? "" : "invisible"}`}
        />
        {summary}
      </button>
      {folds && open && <div className="mb-2 ml-6 mt-0.5 flex flex-col gap-1">{children}</div>}
    </li>
  );
}
