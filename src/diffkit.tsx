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
import { ANCHOR_CALM } from "./markup";

export { WordDiffText } from "./pagekit";
export type { ChangeKind } from "./changeMarks";

/** The glyph each change category shows. `!` (stale), `?` (vagrant) and `≈`
 *  (amended after sign-off) are drift markers; the rest are plan changes. */
const KIND_GLYPH: Record<ChangeKind, string> = {
  add: "+",
  modified: "~",
  delete: "−",
  relocate: "→",
  vagrant: "?",
  stale: "!",
  amendment: "≈",
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
  "≈": "amendment",
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

/** Statement-markup anchor tone inside each tint: one step brighter (dark) /
 *  deeper (light) within the row's own hue — the tinted twin of the calm
 *  rows' secondary→text lift (`ANCHOR_CALM` in markup.tsx). */
export const DIFF_ANCHOR: Record<"add" | "delete", string> = {
  add: "text-emerald-950 dark:text-emerald-100",
  delete: "text-red-950 dark:text-red-100",
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

/** The statement-markup anchor tone that goes with {@link diffTextClass} —
 *  tinted rows lift within their own hue, everything else takes the calm
 *  secondary→text lift. Paired so a row's body and its bold anchors are never
 *  chosen apart. */
export function diffAnchorClass(kind: ChangeKind | undefined): string {
  return kind === "add"
    ? DIFF_ANCHOR.add
    : kind === "delete"
      ? DIFF_ANCHOR.delete
      : ANCHOR_CALM;
}

/** How one element diverges from its committed copy — the shared vocabulary
 *  behind the claim, property, and link diff views. `relocated` and `vagrant`
 *  only occur where the caller opts in (claims). */
export type ElementDiffKind =
  | "added"
  | "reworded"
  | "deleted"
  | "relocated"
  | "vagrant"
  | "unchanged";

/** Element diff kinds map onto the shared change categories so the marker
 *  glyph, its hue, and the whole-element tint all come from the one diff kit. */
export const CHANGE_OF: Record<Exclude<ElementDiffKind, "unchanged">, ChangeKind> = {
  added: "add",
  reworded: "modified",
  deleted: "delete",
  relocated: "relocate",
  vagrant: "vagrant",
};

export interface ElementDiff<T> {
  item: T;
  kind: ElementDiffKind;
  /** The committed copy, for word-diffing a reworded element. */
  prev?: T;
  /** Display number — null for vagrant/deleted rows (outside the numbered list). */
  index: number | null;
  /** For `relocated`: the display name of the host that now holds this element. */
  movedTo?: string;
}

/** The one classifier behind every element diff view: planned elements in
 *  order (each tagged added / reworded / vagrant / unchanged against the
 *  committed copy by `key`), then any committed elements the plan dropped —
 *  `deleted`, or `relocated` when `relocatedTo` finds the key living on
 *  another host in the plan. Claims key by id, properties by exact label,
 *  links by id — same keying as planDiff.ts and diff.rs, so no surface ever
 *  reads "clean" while the tree and get_pending say modified. */
export function buildElementDiff<T>(
  planned: readonly T[],
  committed: readonly T[],
  opts: {
    key: (t: T) => string;
    /** Whether a surviving element was reworded relative to its committed copy. */
    changed: (prev: T, next: T) => boolean;
    /** Code-discovered elements (the "?" drift kind) — never numbered. */
    vagrant?: (t: T) => boolean;
    /** For dropped elements: the plan host now holding this key, if any. */
    relocatedTo?: (t: T) => string | undefined;
  },
): ElementDiff<T>[] {
  const prevByKey = new Map(committed.map((t) => [opts.key(t), t]));
  const liveKeys = new Set(planned.map((t) => opts.key(t)));
  const rows: ElementDiff<T>[] = [];
  let n = 0;
  for (const t of planned) {
    const prev = prevByKey.get(opts.key(t));
    let kind: ElementDiffKind;
    if (opts.vagrant?.(t)) kind = "vagrant";
    else if (!prev) kind = "added";
    else if (opts.changed(prev, t)) kind = "reworded";
    else kind = "unchanged";
    // Vagrant elements aren't part of the numbered contract yet (they await a
    // verdict); everything else takes the next sequence number.
    rows.push({ item: t, kind, prev, index: kind === "vagrant" ? null : ++n });
  }
  for (const t of committed) {
    if (liveKeys.has(opts.key(t))) continue;
    // Present on some other host in the plan → relocated (context only, never
    // restorable); present nowhere → genuinely deleted (restorable).
    const movedTo = opts.relocatedTo?.(t);
    rows.push(
      movedTo
        ? { item: t, kind: "relocated", index: null, movedTo }
        : { item: t, kind: "deleted", index: null },
    );
  }
  return rows;
}

/** The verdict strip under a flagged element — a muted hint ending in an
 *  em-dash, then the action buttons. One idiom for every adopt/reject,
 *  re-implement/drop, and restore prompt. */
export function VerdictBar({
  hint,
  tone,
  className = "",
  children,
}: {
  hint: React.ReactNode;
  /** "drift" dresses the bar as a drift annotation (orange rule + hint ink). */
  tone?: "drift";
  className?: string;
  children: React.ReactNode;
}) {
  return (
    <div
      className={`mt-1.5 flex flex-wrap items-center gap-2 text-2xs ${
        tone === "drift" ? DRIFT_RULE : ""
      } ${className}`}
    >
      <span className={tone === "drift" ? DRIFT_HINT : "text-[var(--text-tertiary)]"}>
        {hint} —
      </span>
      {children}
    </div>
  );
}

/** Drift-annotation dress — the orange left rule and hint ink that mark a block
 *  as drift's note about the content above it (orange = drift, per the color
 *  contract), keeping the annotation visually subordinate to the claim it
 *  hangs off. Shared by {@link VerdictBar}'s drift tone and the stale-claim
 *  proposal blocks. */
export const DRIFT_RULE = "border-l-2 border-orange-500/30 pl-3 dark:border-orange-400/30";
export const DRIFT_HINT = "text-orange-700/80 dark:text-orange-400/80";

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

