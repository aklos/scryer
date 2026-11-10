/**
 * The notice an agent's direction raises: which session sent it and where it points, with the agent's note, and,
 * when the view didn't move, a link that goes there.
 */

import { useEffect } from "react";
import { X } from "lucide-react";
import type { Notice } from "./useDirections";

/** How long a notice stays: one the view already acted on can go sooner than one still offering its link. */
const STAYS_MS = { moved: 6000, offered: 15000 };

export function DirectionNotice({
  notice,
  nodeName,
  onGo,
  onDismiss,
}: {
  notice: Notice | null;
  nodeName: (id: string) => string;
  onGo: () => void;
  onDismiss: () => void;
}) {
  useEffect(() => {
    if (!notice) return;
    const t = setTimeout(onDismiss, notice.moved ? STAYS_MS.moved : STAYS_MS.offered);
    return () => clearTimeout(t);
  }, [notice, onDismiss]);
  if (!notice) return null;
  const d = notice.direction;
  return (
    <div
      role="status"
      data-cam="direction-notice"
      className="fixed bottom-12 left-1/2 z-50 flex max-w-[min(520px,calc(100vw-32px))] -translate-x-1/2 items-start gap-2 rounded-md border border-[var(--border-overlay)] bg-[var(--surface-overlay)] px-3 py-2 text-xs shadow-sm backdrop-blur-md animate-in slide-in-from-bottom-2"
    >
      <span className="mt-[5px] h-1.5 w-1.5 shrink-0 rounded-full bg-[var(--accent)]" />
      <span className="flex min-w-0 flex-col gap-0.5">
        <span className="truncate text-[var(--text-secondary)]">
          <span className="font-medium text-[var(--text)]">{d.sender}</span>
          {notice.moved ? " showed you " : " points to "}
          <span className="font-medium text-[var(--text)]">{nodeName(d.node)}</span>
        </span>
        {d.note && <span className="text-[var(--text-tertiary)]">{d.note}</span>}
      </span>
      {!notice.moved && (
        <button
          type="button"
          onClick={onGo}
          className="shrink-0 rounded border border-[var(--border-strong)] px-2 py-0.5 font-medium text-[var(--accent)] hover:bg-[var(--accent-soft)]"
        >
          Show
        </button>
      )}
      <button
        type="button"
        aria-label="Dismiss"
        onClick={onDismiss}
        className="shrink-0 rounded p-0.5 text-[var(--text-muted)] hover:bg-[var(--surface-hover)] hover:text-[var(--text)]"
      >
        <X className="h-3.5 w-3.5" />
      </button>
    </div>
  );
}
