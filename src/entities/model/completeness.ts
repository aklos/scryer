/**
 * Build completeness — how much of a node's authored subtree reads through to
 * real code, as the health report carries it per node, and the glanceable
 * badge it collapses to.
 */

/** A node's build completeness — how much of its AUTHORED subtree (committed +
 *  planned) reads through to real code. Distinct from HealthCounts (a lens over
 *  the committed model): completeness is defined from greenfield onward, the
 *  denominator being intent. The unit is the anchorable primitive — a node's
 *  boundary box (counted only when its glob owns a real file), a leaf
 *  responsibility, or a data shape. A structural node's own responsibilities are
 *  not primitives (they discharge through the subtree). */
export interface Completeness {
  /** Anchored primitives in the subtree. */
  anchored: number;
  /** Authored primitives in the subtree — the denominator. */
  total: number;
  /** Leaf primitives (responsibilities + data shapes) in the subtree. When 0 the
   *  node is unmeasured (a bare box) and `pct` is absent. */
  leafTotal: number;
  /** Rounded 0–100 percent, or absent ("—") when there is nothing to measure. */
  pct?: number;
}

/** The glanceable form of a node's completeness: `label` is the % ("42%"), or
 *  "—" when unmeasured (a bare box with no leaf primitives). `grounded` = the
 *  node has at least one anchored primitive (its box or a leaf reads through to
 *  real code) — the anchorage signal. Returns null when there is nothing to say
 *  (no primitives at all), so callers can skip the badge entirely. */
export interface CompletenessBadge {
  label: string;
  grounded: boolean;
  measured: boolean;
}

export function completenessBadge(
  c: Completeness | undefined,
): CompletenessBadge | null {
  if (!c || c.total === 0) return null;
  const grounded = c.anchored > 0;
  if (c.pct === undefined) return { label: "—", grounded, measured: false };
  return { label: `${c.pct}%`, grounded, measured: true };
}
