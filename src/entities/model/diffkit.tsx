/**
 * The diffing design language — the shared primitives every surface that
 * renders a diff of a node's claims, properties or links against the base
 * the checkout's changes are read from goes through, and the mark a changed
 * node carries in the tree and on the map, so "added / reworded / deleted" looks the same
 * on every section of the node page.
 *
 * Two primitives, one palette ({@link CHANGE_COLOR}):
 *   - {@link ChangeGlyph} — the per-change marker glyph (+ ~ −) in its hue.
 *   - {@link DIFF_TINT}   — the whole-element add/delete fill.
 */

import type { NodeChange } from "./changes";

export { WordDiffText } from "../../shared/ui/pagekit";

/** The change categories a diff colours by. */
export type ChangeKind = "add" | "modified" | "delete";

// Light mode sits on the 700 tier — the 600s (amber especially) fall under
// ~3.5:1 on the light canvas and small mono glyphs become guesswork. Dark mode
// keeps the 400s.
const CHANGE_COLOR: Record<ChangeKind, string> = {
  add: "text-emerald-700 dark:text-emerald-400",
  modified: "text-amber-700 dark:text-amber-400",
  delete: "text-red-700 dark:text-red-400",
};

const KIND_GLYPH: Record<ChangeKind, string> = {
  add: "+",
  modified: "~",
  delete: "−",
};

/** The per-change marker — one glyph in its category hue. Sits in a fixed
 *  gutter; pass `className` to tune size (defaults to the node page's `text-xs`). */
export function ChangeGlyph({
  kind,
  className = "text-xs",
  plain = false,
}: {
  kind: ChangeKind;
  className?: string;
  /** In a neutral tone, where colour already means something else (a claim
   *  row's hue is its state). */
  plain?: boolean;
}) {
  return (
    <span className={`select-none text-center font-mono font-bold ${plain ? "text-[var(--text-tertiary)]" : CHANGE_COLOR[kind]} ${className}`}>
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

/** How one element diverges from its base copy — the shared vocabulary
 *  behind the claim, property, and link diff views. */
export type ElementDiffKind = "added" | "reworded" | "deleted" | "unchanged";

/** Element diff kinds map onto the change categories so the marker glyph, its
 *  hue, and the whole-element tint all come from the one diff kit. */
export const CHANGE_OF: Record<Exclude<ElementDiffKind, "unchanged">, ChangeKind> = {
  added: "add",
  reworded: "modified",
  deleted: "delete",
};

export interface ElementDiff<T> {
  item: T;
  kind: ElementDiffKind;
  /** The base copy, for word-diffing a reworded element. */
  prev?: T;
  /** Display number — null for deleted rows (outside the numbered list). */
  index: number | null;
}

/** The one classifier behind every element diff view: planned elements in
 *  order (each tagged added / reworded / unchanged against the base copy
 *  by `key`), then any base elements since removed. Claims key by id,
 *  properties by exact label, links by id. */
export function buildElementDiff<T>(
  current: readonly T[],
  base: readonly T[],
  opts: {
    key: (t: T) => string;
    /** Whether a surviving element was reworded relative to its base copy. */
    changed: (prev: T, next: T) => boolean;
  },
): ElementDiff<T>[] {
  const prevByKey = new Map(base.map((t) => [opts.key(t), t]));
  const liveKeys = new Set(current.map((t) => opts.key(t)));
  const rows: ElementDiff<T>[] = [];
  let n = 0;
  for (const t of current) {
    const prev = prevByKey.get(opts.key(t));
    const kind: ElementDiffKind = !prev ? "added" : opts.changed(prev, t) ? "reworded" : "unchanged";
    rows.push({ item: t, kind, prev, index: ++n });
  }
  for (const t of base)
    if (!liveKeys.has(opts.key(t))) rows.push({ item: t, kind: "deleted", index: null });
  return rows;
}

/** A changed node's name colour, the source-control explorer idiom: green
 *  added, amber modified. */
export const NODE_CHANGE_TONE: Record<NodeChange, string> = {
  added: CHANGE_COLOR.add,
  modified: CHANGE_COLOR.modified,
};

const NODE_CHANGE_LETTER: Record<NodeChange, string> = { added: "A", modified: "M" };

/** A node's change since the base, as the tree and the map mark it: its
 *  letter (A added, M modified) in its hue, or, for a node that only holds
 *  changes below it, a dot. Nothing for an unchanged node. */
export function NodeChangeMark({ change, holding = false }: { change?: NodeChange; holding?: boolean }) {
  if (change)
    return (
      <span
        data-change={change}
        title={change === "added" ? "Added since the base" : "Modified since the base"}
        className={`shrink-0 select-none font-mono text-2xs font-semibold ${NODE_CHANGE_TONE[change]}`}
      >
        {NODE_CHANGE_LETTER[change]}
      </span>
    );
  if (holding)
    return (
      <span
        data-change="holding"
        title="Holds changes since the base"
        className="h-1.5 w-1.5 shrink-0 rounded-full bg-amber-600 dark:bg-amber-400"
      />
    );
  return null;
}
