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

/** A raw marker glyph back to its change category (the durable history log
 *  stores the chars); undefined for anything unrecognised. */
export function kindOfGlyph(glyph: string): ChangeKind | undefined {
  return GLYPH_KIND[glyph];
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

/** Whole-element treatment, tuned per mode. Dark: text colour only, like a
 *  terminal diff — bright 300s pop on a dark canvas and a page of pending work
 *  stays readable instead of drowning in paint. Light: coloured text alone
 *  cannot pop (emerald's dark shades read as teal-gray at 13px), so the hue
 *  rides a quiet background wash — the GitHub-diff idiom — with deep text for
 *  contrast. */
export const DIFF_TINT: Record<"add" | "delete", string> = {
  add: "rounded-xs bg-emerald-500/10 px-0.5 decoration-clone text-emerald-800 dark:bg-transparent dark:text-emerald-300",
  delete:
    "rounded-xs bg-red-500/10 px-0.5 decoration-clone text-red-800 line-through decoration-red-400/60 dark:bg-transparent dark:text-red-300/90",
};

/** Content class for a whole diff row by category: added/deleted rows tint
 *  their text, everything else stays neutral (a reworded row's word-diff
 *  carries its own paint). One rule, every surface. */
export function diffTextClass(kind: ChangeKind | undefined): string {
  return kind === "add"
    ? DIFF_TINT.add
    : kind === "delete"
      ? DIFF_TINT.delete
      : "text-[var(--text-secondary)]";
}

/** The shared diff-row anatomy — a fixed glyph gutter beside the content. The
 *  History timeline and the Changes page render their rows through this, so a
 *  diff reads as a diff everywhere (the node page's numbered claim rows carry
 *  extra lanes but keep the same gutter/glyph/treatment vocabulary). */
export function DiffRow({
  kind,
  marker,
  className = "",
  children,
}: {
  kind?: ChangeKind;
  /** Raw marker char (history rows) — resolved through the glyph table. */
  marker?: string;
  className?: string;
  children: React.ReactNode;
}) {
  const k = kind ?? (marker ? GLYPH_KIND[marker] : undefined);
  return (
    <div className={`grid grid-cols-[16px_1fr] items-baseline gap-1 ${className}`}>
      {k ? <ChangeGlyph kind={k} /> : <span />}
      <div className="min-w-0">{children}</div>
    </div>
  );
}

