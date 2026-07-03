/**
 * The diffing design language — the shared primitives every surface that
 * RENDERS A DIFF goes through, so "added / reworded / deleted / moved" looks the
 * same on the node page and the changes page. This is distinct from the element
 * MARK (the A/M/D/R badge in {@link ./changeMarks}, all the tree and map need):
 * a mark is a one-letter status, a diff is the spelled-out before → after.
 *
 * Three primitives, one palette ({@link CHANGE_COLOR}):
 *   - {@link ChangeGlyph} — the per-change marker glyph (+ ~ − → ? !) in its hue.
 *   - {@link DIFF_TINT}   — the whole-element add/delete fill, the block-level
 *                           counterpart to the per-word paint.
 *   - {@link WordDiffText} — per-word add/remove highlight for reworded text
 *                            (re-exported from pagekit, the historical home).
 */

import { CHANGE_COLOR, type ChangeKind } from "./changeMarks";

export { WordDiffText } from "./pagekit";
export type { ChangeKind } from "./changeMarks";

/** The glyph each change category shows. `!` (stale) and `?` (vagrant) are drift
 *  markers; the rest are plan changes. */
const KIND_GLYPH: Record<ChangeKind, string> = {
  add: "+",
  modified: "~",
  delete: "−",
  relocate: "→",
  vagrant: "?",
  stale: "!",
};

/** The inverse — a raw glyph back to its category. The durable history log
 *  stores these chars directly, so its rows resolve colour through here. */
const GLYPH_KIND: Record<string, ChangeKind> = {
  "+": "add",
  "~": "modified",
  "−": "delete",
  "→": "relocate",
  "?": "vagrant",
  "!": "stale",
};

/** Colour for a raw diff-marker glyph; muted for anything unrecognised. */
export function glyphColor(glyph: string): string {
  const kind = GLYPH_KIND[glyph];
  return kind ? CHANGE_COLOR[kind] : "text-[var(--text-muted)]";
}

/** The per-change marker — one glyph in its category hue. The shared vocabulary
 *  for "this line was added / reworded / deleted / moved". Sits in a fixed
 *  gutter; pass `className` to tune size (defaults to the node page's `text-xs`). */
export function ChangeGlyph({
  kind,
  className = "text-xs",
}: {
  kind: ChangeKind;
  className?: string;
}) {
  return (
    <span className={`select-none text-center font-mono font-bold ${CHANGE_COLOR[kind]} ${className}`}>
      {KIND_GLYPH[kind]}
    </span>
  );
}

/** Whole-element tint: an added element reads green-filled, a deleted one struck
 *  red. The block-level counterpart to {@link WordDiffText}'s per-word paint;
 *  applied to a statement that's entirely new or entirely dropped. */
export const DIFF_TINT: Record<"add" | "delete", string> = {
  add: "rounded-[2px] bg-emerald-500/15 px-0.5 text-emerald-700 dark:text-emerald-300 decoration-clone",
  delete:
    "rounded-[2px] bg-red-500/10 px-0.5 text-red-700/90 line-through decoration-red-400/60 decoration-clone dark:text-red-300/90",
};
