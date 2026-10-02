/**
 * Durable committed-model history — the client mirror of `scryer_core::history`.
 * The committed `model.scry` only changes through an agent operation (a fold, a
 * drift reconcile, a build, a move); each appends one {@link HistoryEvent} to
 * `.scryer/history.jsonl`, which the node page's History tab renders as a real
 * timeline. Read via the `read_history` Tauri command; reloaded whenever the
 * model changes.
 */

import type { SourceLocation } from "./viewmodel";

export type EventKind = "impl" | "drift" | "move" | "born";

export interface EventRow {
  /** Single-char marker — `+` added, `−`/`~` reworded, `!` stale, `→` moved. */
  marker: string;
  text: string;
  /** Anchored code location for `impl` rows that discharge a claim. */
  source?: SourceLocation;
}

export interface HistoryEvent {
  /** Unix seconds. */
  at: number;
  /** Who drove it — agent-only in v0.3. */
  by: string;
  /** Short driver/intent label, e.g. "fill", "build", "took code". */
  driver: string;
  kind: EventKind;
  /** The node this event is about — the History tab filters on it. */
  nodeId: string;
  rows: EventRow[];
}

/** Per-kind timeline presentation: the event's prose label and the colour of
 *  its rail dot + driver badge. Hues follow the app-wide contract — the diff
 *  glyphs inside each event carry the add/delete colour, so the event chrome
 *  only classifies the KIND: done work is quiet (neutral), drift wears its
 *  orange review hue, a move wears the amber structural hue. */
export const EVENT_META: Record<EventKind, { label: string; dot: string; badge: string }> = {
  impl: {
    label: "implemented",
    dot: "var(--text-muted)",
    badge: "text-[var(--text-secondary)] border-[var(--border-strong)]",
  },
  drift: {
    label: "drift reconciled",
    dot: "var(--color-orange-500)",
    badge: "text-orange-700 dark:text-orange-400 border-orange-500/30",
  },
  move: {
    label: "moved",
    dot: "var(--color-amber-500)",
    badge: "text-amber-700 dark:text-amber-400 border-amber-500/30",
  },
  born: {
    label: "created from code",
    dot: "var(--text-ghost)",
    badge: "text-[var(--text-tertiary)] border-[var(--border)]",
  },
};

/** A coarse "2 days ago" label from a unix-seconds timestamp. The durable log
 *  spans days/weeks, so relative phrasing reads better than a clock time. */
export function relativeTime(atSecs: number, nowSecs = Date.now() / 1000): string {
  const d = Math.max(0, Math.floor(nowSecs - atSecs));
  if (d < 60) return "just now";
  const mins = Math.floor(d / 60);
  if (mins < 60) return `${mins} min${mins === 1 ? "" : "s"} ago`;
  const hrs = Math.floor(mins / 60);
  if (hrs < 24) return `${hrs} hour${hrs === 1 ? "" : "s"} ago`;
  const days = Math.floor(hrs / 24);
  if (days < 30) return `${days} day${days === 1 ? "" : "s"} ago`;
  const months = Math.floor(days / 30);
  if (months < 12) return `${months} month${months === 1 ? "" : "s"} ago`;
  const years = Math.floor(days / 365);
  return `${years} year${years === 1 ? "" : "s"} ago`;
}
