/**
 * The offer to follow a session that started working here: its task, Follow and a dismiss. Bottom right, apart
 * from an agent's show notice, so the two never cover each other.
 */

import { useEffect } from "react";
import { Eye, X } from "lucide-react";
import type { SessionBrief } from "./follow";

/** How long an unanswered offer stays. */
const STAYS_MS = 30000;

export function FollowOffer({
  offer,
  onFollow,
  onDismiss,
}: {
  offer: SessionBrief | null;
  onFollow: () => void;
  onDismiss: () => void;
}) {
  useEffect(() => {
    if (!offer) return;
    const t = setTimeout(onDismiss, STAYS_MS);
    return () => clearTimeout(t);
  }, [offer, onDismiss]);
  if (!offer) return null;
  return (
    <div
      role="status"
      data-cam="follow-offer"
      className="fixed bottom-12 right-4 z-50 flex max-w-[min(420px,calc(100vw-32px))] items-center gap-2 rounded-md border border-[var(--border-overlay)] bg-[var(--surface-overlay)] px-3 py-2 text-xs shadow-sm backdrop-blur-md animate-in slide-in-from-bottom-2"
    >
      <span className="h-1.5 w-1.5 shrink-0 animate-pulse rounded-full bg-violet-600 dark:bg-violet-400" />
      <span className="min-w-0 truncate text-[var(--text-secondary)]">
        <span className="font-medium text-[var(--text)]">{offer.task ? `“${offer.task}”` : "An agent"}</span> is working
        here
      </span>
      <button
        type="button"
        onClick={onFollow}
        className="flex shrink-0 items-center gap-1 rounded border border-[var(--border-strong)] px-2 py-0.5 font-medium text-[var(--accent)] hover:bg-[var(--accent-soft)]"
      >
        <Eye className="h-3.5 w-3.5" />
        Follow
      </button>
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
