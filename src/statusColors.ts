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
