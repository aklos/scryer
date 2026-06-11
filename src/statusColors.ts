/**
 * Status model — aligned with the v0.3 schema in `crates/scryer-core/src/lib.rs`.
 *
 * Statuses are the PRESCRIPTION — what the model says about the work, moved
 * deliberately:
 *  proposed    — planned, no code yet
 *  implemented — code exists; may still be incomplete
 *  verified    — responsibilities checked against code
 *  changed     — the spec was edited after implementation; code must catch up
 *
 * Observations about the lens — `vagrant` (undescribed behaviour), `stale`
 * (drift verdict), `relocatedTo`/`relocatedFrom` — are FLAGS, a separate axis.
 * They are never statuses and never enter this union; they render through
 * FLAG_COLORS instead.
 */

export type Status = "proposed" | "implemented" | "verified" | "changed";

/** Shared pill base — states render as tinted pills (background tint + inset
 *  ring + tinted text), never as bare colored words, so state always reads as
 *  a badge distinct from body text. */
export const PILL_BASE =
  "inline-flex items-center gap-1 rounded-full px-2 py-px text-2xs font-medium ring-1 ring-inset";

/** Lifecycle hues — each owned exclusively by its status (see the color
 *  contract in index.css). Interaction chrome must never borrow these. */
export const STATUS_COLORS: Record<
  Status,
  { text: string; dot: string; pill: string; label: string }
> = {
  proposed: {
    text: "text-blue-600 dark:text-blue-400",
    dot: "bg-blue-500 dark:bg-blue-400",
    pill: `${PILL_BASE} bg-blue-500/10 text-blue-700 ring-blue-500/25 dark:bg-blue-400/10 dark:text-blue-300 dark:ring-blue-400/25`,
    label: "Proposed",
  },
  implemented: {
    text: "text-amber-600 dark:text-amber-400",
    dot: "bg-amber-500 dark:bg-amber-400",
    pill: `${PILL_BASE} bg-amber-500/10 text-amber-700 ring-amber-500/25 dark:bg-amber-400/10 dark:text-amber-300 dark:ring-amber-400/25`,
    label: "Implemented",
  },
  verified: {
    text: "text-emerald-600 dark:text-emerald-400",
    dot: "bg-emerald-500 dark:bg-emerald-400",
    pill: `${PILL_BASE} bg-emerald-500/10 text-emerald-700 ring-emerald-500/25 dark:bg-emerald-400/10 dark:text-emerald-300 dark:ring-emerald-400/25`,
    label: "Verified",
  },
  changed: {
    text: "text-orange-600 dark:text-orange-400",
    dot: "bg-orange-500 dark:bg-orange-400",
    pill: `${PILL_BASE} bg-orange-500/10 text-orange-700 ring-orange-500/25 dark:bg-orange-400/10 dark:text-orange-300 dark:ring-orange-400/25`,
    label: "Changed",
  },
};

/** Observation-flag hues — the second axis. Red = vagrant (undescribed code),
 *  orange = stale (drift verdict, shares the drift hue), violet = relocated. */
export const FLAG_COLORS = {
  vagrant: {
    text: "text-red-600 dark:text-red-400",
    dot: "bg-red-500 dark:bg-red-400",
    pill: `${PILL_BASE} bg-red-500/10 text-red-700 ring-red-500/25 dark:bg-red-400/10 dark:text-red-300 dark:ring-red-400/25`,
    label: "Vagrant",
  },
  stale: {
    text: "text-orange-600 dark:text-orange-400",
    dot: "bg-orange-500 dark:bg-orange-400",
    pill: `${PILL_BASE} bg-orange-500/10 text-orange-700 ring-orange-500/25 dark:bg-orange-400/10 dark:text-orange-300 dark:ring-orange-400/25`,
    label: "Stale",
  },
  relocated: {
    text: "text-violet-600 dark:text-violet-400",
    dot: "bg-violet-500 dark:bg-violet-400",
    pill: `${PILL_BASE} bg-violet-500/10 text-violet-700 ring-violet-500/25 dark:bg-violet-400/10 dark:text-violet-300 dark:ring-violet-400/25`,
    label: "Relocated",
  },
} as const;

const LIFE_RANK: Record<Status, number> = {
  proposed: 0,
  changed: 1,
  implemented: 2,
  verified: 3,
};

/**
 * Roll a set of responsibility statuses up to one node status. A node is only
 * as done as its weakest responsibility. (Flags don't roll up here — they're
 * surfaced on their own axis.)
 */
export function rollupStatus(statuses: Status[]): Status {
  if (statuses.length === 0) return "proposed";
  if (statuses.some((s) => s === "changed")) return "changed";
  return statuses.reduce((worst, s) =>
    LIFE_RANK[s] < LIFE_RANK[worst] ? s : worst,
  );
}
