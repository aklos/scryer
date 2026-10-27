/**
 * The strip along the window's bottom while you follow an agent session:
 * which session, what it did last, and its latest steps on asking. The wiki
 * and map go where it edits; Stop hands navigation back.
 */

import { useState } from "react";
import { ChevronUp, Eye, X } from "lucide-react";
import { stepLine, type LoggedSession } from "../../features/sessions/useSessions";

/** How many of the latest steps the strip opens to. */
const OPEN_STEPS = 8;

export function FollowBar({
  session,
  title,
  names,
  onStop,
}: {
  /** The followed session; null until its first read lands. */
  session: LoggedSession | null;
  title: string;
  names: Map<string, string>;
  onStop: () => void;
}) {
  const [open, setOpen] = useState(false);
  const name = (id: string) => names.get(id) ?? id;
  const newest = [...(session?.steps ?? [])].reverse();
  const time = (at: number) => new Date(at * 1000).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  return (
    <div className="shrink-0 border-t border-violet-500/40 bg-[var(--surface)]">
      {open && newest.length > 1 && (
        <ul className="flex flex-col border-b border-[var(--border)] px-3 py-1">
          {newest.slice(1, OPEN_STEPS + 1).reverse().map((step, i) => (
            <li key={`${step.at}-${i}`} className="flex items-baseline gap-2 text-xs text-[var(--text-secondary)]">
              <span className="shrink-0 font-mono text-2xs tabular-nums text-[var(--text-ghost)]">{time(step.at)}</span>
              <span className="min-w-0 flex-1 truncate">{stepLine(step, name)}</span>
            </li>
          ))}
        </ul>
      )}
      <div className="flex min-w-0 items-center gap-2 px-3 py-1 text-xs">
        <span className="h-2 w-2 shrink-0 animate-pulse rounded-full bg-violet-600 dark:bg-violet-500" />
        <Eye className="h-3.5 w-3.5 shrink-0 text-violet-600 dark:text-violet-400" />
        <span className="min-w-0 max-w-[40%] shrink truncate font-medium text-[var(--text)]" title={title}>
          Following {title}
        </span>
        <button
          type="button"
          disabled={newest.length < 2}
          onClick={() => setOpen((v) => !v)}
          title={open ? "Hide its earlier steps" : "Show its earlier steps"}
          className="flex min-w-0 flex-1 items-baseline gap-2 rounded px-1 text-left enabled:hover:bg-[var(--surface-hover)]"
        >
          {newest[0] ? (
            <>
              <span className="shrink-0 font-mono text-2xs tabular-nums text-[var(--text-ghost)]">{time(newest[0].at)}</span>
              <span className="min-w-0 flex-1 truncate text-[var(--text-secondary)]">{stepLine(newest[0], name)}</span>
            </>
          ) : (
            <span className="text-[var(--text-muted)]">{session ? "waiting for its first edit" : "Loading…"}</span>
          )}
          {newest.length > 1 && (
            <ChevronUp className={`h-3.5 w-3.5 shrink-0 self-center text-[var(--text-ghost)] ${open ? "" : "rotate-180"}`} />
          )}
        </button>
        <button
          type="button"
          onClick={onStop}
          title="Stop following"
          aria-label="Stop following"
          className="flex shrink-0 items-center gap-1 rounded-md px-2 py-0.5 text-[var(--text-secondary)] hover:bg-[var(--surface-hover)]"
        >
          <X className="h-3.5 w-3.5" />
          Stop
        </button>
      </div>
    </div>
  );
}
