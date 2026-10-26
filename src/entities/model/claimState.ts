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

/** The state that needs a reader's attention, if any: a failing claim first,
 *  then a todo one. Proven and stale are the resting states and mark
 *  nothing. */
export function attentionState(c: DerivedCounts | undefined): "failing" | "planned" | null {
  if (!c) return null;
  if (c.failing > 0) return "failing";
  if (c.planned > 0) return "planned";
  return null;
}

/** "3 failing · 12 unverified" — the non-zero states in a fixed order. */
export function describeCounts(c: DerivedCounts): string {
  const parts = CLAIM_STATES.filter((s) => c[s] > 0).map((s) => `${c[s]} ${CLAIM_LABEL[s]}`);
  return parts.length ? parts.join(" · ") : "no claims";
}
