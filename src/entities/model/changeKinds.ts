/**
 * Change kinds — the single-letter vocabulary the UI uses to show how the plan
 * diverges from the committed tree, plus the two drift flags, and the one
 * palette every mark and diff glyph is coloured from.
 *
 *   A  added       — new in the plan (green)
 *   M  modified    — fields reworded, or own content added/removed (amber)
 *   D  deleted     — dropped from the plan (red)
 *   R  relocated   — re-parented / re-pointed (amber — a structural edit; the → glyph carries "move")
 *   Q  undescribed — a vagrant claim: code does it, the model didn't say so (orange, drift)
 *   X  stale       — a committed claim the code regressed from (orange, drift)
 *
 * A/M/D/R are PLAN marks (`diff(committed, planned)`); Q/X are DRIFT marks,
 * carried as flags on claims. A row can have both; the plan mark wins for the
 * single glanceable letter.
 */

export type Mark = "A" | "M" | "D" | "R" | "Q" | "X";

/** The change categories the whole UI colours by. Both axes of marking draw
 *  from this: the element marks (A/M/D/R/Q/X, the glanceable tree/map badge) and
 *  the per-change diff glyphs (+ ~ − → ? !, the rendered diff on the node and
 *  changes pages). One hue per category so a letter and its glyph never
 *  disagree. Two axes, distinct hue families: PLAN edits are the diff palette —
 *  add green, delete red, modified/relocate amber (the glyph carries the kind);
 *  DRIFT is orange for both vagrant and stale, so it reads as its own "review"
 *  axis, never mistaken for a planned edit. */
export type ChangeKind = "add" | "modified" | "delete" | "relocate" | "vagrant" | "stale";

// Light mode sits on the 700 tier — the 600s (amber especially) fall under
// ~3.5:1 on the light canvas and small mono glyphs/counts become guesswork.
// Dark mode keeps the 400s.
export const CHANGE_COLOR: Record<ChangeKind, string> = {
  add: "text-emerald-700 dark:text-emerald-400",
  modified: "text-amber-700 dark:text-amber-400",
  delete: "text-red-700 dark:text-red-400",
  relocate: "text-amber-700 dark:text-amber-400",
  vagrant: "text-orange-700 dark:text-orange-400",
  stale: "text-orange-700 dark:text-orange-400",
};

/** Which category each element mark belongs to — the bridge between the
 *  one-letter badge and the shared palette. */
export const MARK_KIND: Record<Mark, ChangeKind> = {
  A: "add",
  M: "modified",
  D: "delete",
  R: "relocate",
  Q: "vagrant",
  X: "stale",
};

/** Per-mark hue + label. The hue is the category colour, kept in lockstep with
 *  the diff glyphs via {@link CHANGE_COLOR}. */
export const MARK_META: Record<Mark, { color: string; label: string }> = {
  A: { color: CHANGE_COLOR.add, label: "Added" },
  M: { color: CHANGE_COLOR.modified, label: "Modified" },
  D: { color: CHANGE_COLOR.delete, label: "Deleted" },
  R: { color: CHANGE_COLOR.relocate, label: "Relocated" },
  Q: { color: CHANGE_COLOR.vagrant, label: "Undescribed in the model (drift)" },
  X: { color: CHANGE_COLOR.stale, label: "Stale — code regressed (drift)" },
};
