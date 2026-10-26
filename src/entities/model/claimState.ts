/**
 * The claim-state vocabulary every surface colours by: a claim is proven
 * (its test passed on the code as it is), unverified (wire name `stale`: it passed, but the code or
 * test changed since, or it never ran), failing, or todo (a todo test: work
 * not done yet, or behaviour that exists with no test body written). The
 * wire name for todo is `planned`; every surface shows CLAIM_LABEL.
 * One hue per state, so a dot on the map, a letter in the tree and a row on
 * a page never disagree.
 */

import type { ClaimState, DerivedCounts } from "./derived";

export const CLAIM_STATES: readonly ClaimState[] = ["proven", "stale", "failing", "planned"];

/** Text hue per state (light 700 / dark 300). */
export const CLAIM_TONE: Record<ClaimState, string> = {
  proven: "text-emerald-700 dark:text-emerald-300",
  planned: "text-sky-700 dark:text-sky-300",
  stale: "text-amber-700 dark:text-amber-300",
  failing: "text-red-700 dark:text-red-300",
};

/** Fill per state, for bars and dots. */
export const CLAIM_FILL: Record<ClaimState, string> = {
  proven: "bg-emerald-500",
  planned: "bg-sky-500",
  stale: "bg-amber-500",
  failing: "bg-red-500",
};

export const CLAIM_LABEL: Record<ClaimState, string> = {
  proven: "proven",
  planned: "todo",
  stale: "unverified",
  failing: "failing",
};

/** One letter per state, where words don't fit: the tree's gutter, a map
 *  card's foot. */
export const CLAIM_LETTER: Record<ClaimState, string> = {
  proven: "P",
  planned: "T",
  stale: "U",
  failing: "F",
};

/** The state that needs a reader's attention, if any: a failing claim first,
 *  then a todo one. Proven and stale are the resting states and mark
 *  nothing. */
export function attentionState(c: DerivedCounts | undefined): "failing" | "planned" | null {
  if (!c) return null;
  if (c.failing > 0) return "failing";
  if (c.planned > 0) return "planned";
  return null;
}

/** The one state a node reads as, from its subtree's counts: the most
 *  urgent its claims hold — failing, then todo, then unverified, then
 *  proven. Null for a subtree with no claims. */
export function overallState(c: DerivedCounts | undefined): ClaimState | null {
  if (!c) return null;
  for (const s of ["failing", "planned", "stale", "proven"] as const) if (c[s] > 0) return s;
  return null;
}

/** A node's direct children, each counted once in the state it reads as:
 *  the counts sum to `total` less the children with no claims. */
export interface ChildTally {
  total: number;
  byState: Record<ClaimState, number>;
}

export function childTally(children: (DerivedCounts | undefined)[]): ChildTally {
  const byState: Record<ClaimState, number> = { proven: 0, stale: 0, failing: 0, planned: 0 };
  for (const c of children) {
    const s = overallState(c);
    if (s) byState[s]++;
  }
  return { total: children.length, byState };
}

/** "3 failing · 12 unverified" — the non-zero states in a fixed order. */
export function describeCounts(c: DerivedCounts): string {
  const parts = CLAIM_STATES.filter((s) => c[s] > 0).map((s) => `${c[s]} ${CLAIM_LABEL[s]}`);
  return parts.length ? parts.join(" · ") : "no claims";
}
