/**
 * Change marks — the single-letter vocabulary the UI uses to show how the plan
 * (`planned`) diverges from the committed model, plus the two drift flags. One
 * letter summarizes a row in the tree gutter; the node page and status bar
 * spell the same changes out in full.
 *
 *   A  added       — new in the plan (green)
 *   M  modified    — fields reworded, or own content added/removed (amber)
 *   D  deleted     — dropped from the plan (red)
 *   R  relocated   — re-parented / re-pointed (blue)
 *   Q  undescribed — a vagrant claim: code does it, the model didn't say so (violet, drift)
 *   X  stale       — a committed claim the code regressed from (orange, drift)
 *
 * A/M/D/R are PLAN marks (the model→code work queue, `diff(committed,planned)`);
 * Q/X are DRIFT marks (model↔code mismatch, carried as flags on claims). A row
 * can have both; the plan mark wins for the single glanceable letter.
 */

import type { Change, ElementChange, ModelDiff } from "./planDiff";
import type { Group, Node } from "./viewmodel";

export type Mark = "A" | "M" | "D" | "R" | "Q" | "X";

/** The change categories the whole UI colours by. Both axes of marking draw
 *  from this: the element marks (A/M/D/R/Q/X, the glanceable tree/map badge) and
 *  the per-change diff glyphs (+ ~ − → ? !, the rendered diff on the node and
 *  changes pages). One hue per category so a letter and its glyph never
 *  disagree. Hues follow the mockup palette: add green, reword amber, delete
 *  red, relocate blue, vagrant violet, stale orange. */
export type ChangeKind = "add" | "reword" | "delete" | "relocate" | "vagrant" | "stale";

export const CHANGE_COLOR: Record<ChangeKind, string> = {
  add: "text-emerald-600 dark:text-emerald-400",
  reword: "text-amber-600 dark:text-amber-400",
  delete: "text-red-600 dark:text-red-400",
  relocate: "text-blue-600 dark:text-blue-400",
  vagrant: "text-violet-600 dark:text-violet-400",
  stale: "text-orange-600 dark:text-orange-400",
};

/** Which category each element mark belongs to — the bridge between the
 *  one-letter badge and the shared palette. */
export const MARK_KIND: Record<Mark, ChangeKind> = {
  A: "add",
  M: "reword",
  D: "delete",
  R: "relocate",
  Q: "vagrant",
  X: "stale",
};

/** Per-mark hue + label. The hue is the category colour, kept in lockstep with
 *  the diff glyphs via {@link CHANGE_COLOR}. */
export const MARK_META: Record<Mark, { color: string; label: string }> = {
  A: { color: CHANGE_COLOR.add, label: "Added" },
  M: { color: CHANGE_COLOR.reword, label: "Modified" },
  D: { color: CHANGE_COLOR.delete, label: "Deleted" },
  R: { color: CHANGE_COLOR.relocate, label: "Relocated" },
  Q: { color: CHANGE_COLOR.vagrant, label: "Undescribed in the model (drift)" },
  X: { color: CHANGE_COLOR.stale, label: "Stale — code regressed (drift)" },
};

/** The plan diff, indexed for per-element lookup: each node/group's own changes,
 *  and the responsibility/property changes grouped under their owning id. */
export interface DiffIndex {
  nodeOwn: Map<string, Change[]>;
  groupOwn: Map<string, Change[]>;
  byOwner: Map<string, ElementChange[]>;
}

export function indexDiff(diff: ModelDiff): DiffIndex {
  const nodeOwn = new Map<string, Change[]>();
  const groupOwn = new Map<string, Change[]>();
  const byOwner = new Map<string, ElementChange[]>();
  for (const ec of diff.changes) {
    if (ec.kind === "node") nodeOwn.set(ec.id, ec.changes);
    else if (ec.kind === "group") groupOwn.set(ec.id, ec.changes);
    else if (ec.ownerId) {
      const arr = byOwner.get(ec.ownerId);
      if (arr) arr.push(ec);
      else byOwner.set(ec.ownerId, [ec]);
    }
  }
  return { nodeOwn, groupOwn, byOwner };
}

/** Classify a single PLAN mark from an element's own changes plus the changes
 *  to the content it owns. A brand-new or dropped element wins outright; then a
 *  relocation; then any reword/add/remove reads as "modified". */
export function classifyPlan(own: Change[] | undefined, childChanges: Change[]): Mark | null {
  if (own?.some((c) => c.type === "added")) return "A";
  if (own?.some((c) => c.type === "deleted")) return "D";
  const all = own ? [...own, ...childChanges] : childChanges;
  if (all.length === 0) return null;
  if (all.some((c) => c.type === "moved" || c.type === "repointed")) return "R";
  return "M";
}

/** Drift mark for a set of responsibilities: a vagrant claim is undescribed
 *  behaviour (Q), a stale one is a regression (X). */
function driftOf(resps: { vagrant?: boolean; stale?: boolean }[] | undefined): Mark | null {
  let vagrant = false;
  let stale = false;
  for (const r of resps ?? []) {
    if (r.vagrant) vagrant = true;
    if (r.stale) stale = true;
  }
  return vagrant ? "Q" : stale ? "X" : null;
}

/** The plan and drift marks for one node, computed independently so the lenses
 *  can filter on either axis. Vagrant claims feed the drift mark, not the plan
 *  mark (they're code-first, not a planned edit). */
export function nodeMarks(node: Node, idx: DiffIndex): { plan: Mark | null; drift: Mark | null } {
  // The node ITSELF can be vagrant — a rung the drift check minted to home
  // code-discovered behaviour — on top of any vagrant responsibilities it holds.
  const vagrantIds = new Set<string>();
  const vagrantPropLabels = new Set<string>();
  let vagrant = !!node.vagrant;
  // The node ITSELF can be stale — its whole backing code is gone (mirror of a
  // vagrant node) — on top of any stale responsibilities it holds.
  let stale = !!node.stale;
  for (const r of node.responsibilities ?? []) {
    if (r.vagrant) {
      vagrant = true;
      vagrantIds.add(r.id);
    }
    if (r.stale) stale = true;
  }
  // Data fields drift the same way: a vagrant property is undescribed data (Q),
  // a stale one is a regressed field (X).
  for (const p of node.properties ?? []) {
    if (p.vagrant) {
      vagrant = true;
      vagrantPropLabels.add(p.label);
    }
    if (p.stale) stale = true;
  }
  const childChanges: Change[] = [];
  for (const ec of idx.byOwner.get(node.id) ?? []) {
    if (ec.kind === "responsibility" && vagrantIds.has(ec.id)) continue; // drift, not plan
    if (ec.kind === "property" && vagrantPropLabels.has(ec.id)) continue; // drift, not plan
    childChanges.push(...ec.changes);
  }
  // A vagrant node is code-first ("adopt?"), not a planned edit — its own
  // plan-only "added" diff is drift, not a plan mark, so it reads as Q not A.
  const own = node.vagrant ? undefined : idx.nodeOwn.get(node.id);
  return {
    plan: classifyPlan(own, childChanges),
    drift: vagrant ? "Q" : stale ? "X" : null,
  };
}

export function groupMarks(group: Group, idx: DiffIndex): { plan: Mark | null; drift: Mark | null } {
  const childChanges: Change[] = [];
  for (const ec of idx.byOwner.get(group.id) ?? []) childChanges.push(...ec.changes);
  return {
    plan: classifyPlan(idx.groupOwn.get(group.id), childChanges),
    drift: driftOf(group.responsibilities),
  };
}

/** The single glanceable letter for a row — the plan mark if any, else drift. */
export function resolveMark(marks: { plan: Mark | null; drift: Mark | null }): Mark | null {
  return marks.plan ?? marks.drift;
}
