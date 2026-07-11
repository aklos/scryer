/**
 * Observation-flag styling. The model→code plan is shown through the diff
 * (added/reworded/moved marks), not a stored status; what remains here is the
 * second axis — the drift flags `vagrant` (undescribed behaviour) and `stale`
 * (drift verdict), rendered as tinted pills via FLAG_COLORS.
 */

/** Shared pill base — states render as tinted pills (background tint + inset
 *  ring + tinted text), never as bare colored words, so state always reads as
 *  a badge distinct from body text. */
export const PILL_BASE =
  "inline-flex items-center gap-1 rounded-full px-2 py-px text-2xs font-medium ring-1 ring-inset";

/** Observation-flag hues — the second axis. Both drift flags share the orange
 *  "review" hue (vagrant = undescribed code, stale = regressed claim); the mark
 *  glyph (? vs X) tells them apart. Orange keeps drift distinct from the amber
 *  plan-edit marks — it's a different axis, not more pending plan work. */
export const FLAG_COLORS = {
  vagrant: {
    text: "text-orange-600 dark:text-orange-400",
    dot: "bg-orange-500 dark:bg-orange-400",
    pill: `${PILL_BASE} bg-orange-500/10 text-orange-700 ring-orange-500/25 dark:bg-orange-400/10 dark:text-orange-300 dark:ring-orange-400/25`,
    label: "Vagrant",
  },
  stale: {
    text: "text-orange-600 dark:text-orange-400",
    dot: "bg-orange-500 dark:bg-orange-400",
    pill: `${PILL_BASE} bg-orange-500/10 text-orange-700 ring-orange-500/25 dark:bg-orange-400/10 dark:text-orange-300 dark:ring-orange-400/25`,
    label: "Stale",
  },
} as const;

/** Verification pills — the claim's backing test. Emerald = linked and intact;
 *  red = the linked test's anchor broke (symbol or file gone). A CHANGED test
 *  reuses the drift-orange stale pill: same "re-check this" axis. */
export const VERIFY_PILLS = {
  tested: `${PILL_BASE} bg-emerald-500/10 text-emerald-700 ring-emerald-500/25 dark:bg-emerald-400/10 dark:text-emerald-300 dark:ring-emerald-400/25`,
  gone: `${PILL_BASE} bg-red-500/10 text-red-700 ring-red-500/25 dark:bg-red-400/10 dark:text-red-300 dark:ring-red-400/25`,
} as const;
