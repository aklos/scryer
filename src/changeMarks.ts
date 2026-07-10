/**
 * Change marks — the single-letter vocabulary the UI uses to show how the plan
 * (`planned`) diverges from the committed model, plus the two drift flags. One
 * letter summarizes a row in the tree gutter; the node page and status bar
 * spell the same changes out in full.
 *
 *   A  added       — new in the plan (green)
 *   M  modified    — fields reworded, or own content added/removed (amber)
 *   D  deleted     — dropped from the plan (red)
 *   R  relocated   — re-parented / re-pointed (amber — a structural edit; the → glyph carries "move")
 *   Q  undescribed — a vagrant claim: code does it, the model didn't say so (orange, drift)
 *   X  stale       — a committed claim the code regressed from (orange, drift)
 *
 * A/M/D/R are PLAN marks (the model→code work queue, `diff(committed,planned)`);
 * Q/X are DRIFT marks (model↔code mismatch, carried as flags on claims). A row
 * can have both; the plan mark wins for the single glanceable letter.
 */

import type { Change, ElementChange, ModelDiff } from "./planDiff";
import type { Group, Node, ScryModel } from "./viewmodel";

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

export const CHANGE_COLOR: Record<ChangeKind, string> = {
  add: "text-emerald-600 dark:text-emerald-400",
  modified: "text-amber-600 dark:text-amber-400",
  delete: "text-red-600 dark:text-red-400",
  relocate: "text-amber-600 dark:text-amber-400",
  vagrant: "text-orange-600 dark:text-orange-400",
  stale: "text-orange-600 dark:text-orange-400",
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

/** A node's drift mark alone (no diff needed): the node's own vagrant/stale
 *  flag, or any flagged claim/property it holds. */
export function nodeDrift(node: Node): Mark | null {
  if (node.vagrant || (node.responsibilities ?? []).some((r) => r.vagrant) || (node.properties ?? []).some((p) => p.vagrant))
    return "Q";
  if (node.stale || (node.responsibilities ?? []).some((r) => r.stale) || (node.properties ?? []).some((p) => p.stale))
    return "X";
  return null;
}

/** A group's drift mark alone. */
export function groupDrift(group: Group): Mark | null {
  return driftOf(group.responsibilities);
}

/** The single glanceable letter for a row — the plan mark if any, else drift. */
export function resolveMark(marks: { plan: Mark | null; drift: Mark | null }): Mark | null {
  return marks.plan ?? marks.drift;
}

export interface MarkPair {
  plan: Mark | null;
  drift: Mark | null;
}

// --- the shared "what does the plan change" computation ----------------------

/** A changed link attributed to its carrying node (the src side — the side
 *  that performs the relationship), with the dst kept for rendering. */
export interface LinkChange {
  ec: ElementChange;
  /** Current target node id — planned, or the committed copy for a dropped link. */
  dst: string | null;
}

/** One carrier of plan change — a node or group with its own changes, the
 *  changes to content it owns, and the link changes it performs. THE single
 *  definition of what the Changes lens counts, the tree gutter marks, and the
 *  Changes page lists — one computation, so the surfaces cannot disagree.
 *  Carriers the plan no longer holds (deleted nodes/groups) are included;
 *  they have no tree row of their own, so the tree surfaces them as roll-ups
 *  on the surviving branch. */
export interface PlanEntry {
  kind: "node" | "group";
  id: string;
  mark: Mark;
  /** The element's own field / structural changes (rewords, move, members…). */
  own: Change[];
  /** Owned responsibility/property changes (drift-flagged ones filtered out —
   *  vagrant content is the drift axis, not a planned edit). */
  children: ElementChange[];
  /** Outgoing link changes — relationships this element is the source of. */
  links: LinkChange[];
}

export function collectPlanEntries(
  diff: ModelDiff,
  model: ScryModel,
  committed: ScryModel | null,
): PlanEntry[] {
  const idx = indexDiff(diff);
  const isGroup = new Set(model.groups.map((g) => g.id));
  for (const g of committed?.groups ?? []) isGroup.add(g.id);
  const nodeById = new Map(model.nodes.map((n) => [n.id, n] as const));

  // A changed link belongs to its source node — the side that performs the
  // relationship — so it reads under that node, not rootless.
  const linkById = new Map(
    [...(committed?.links ?? []), ...model.links].map((l) => [l.id, l] as const),
  );
  const linksBySrc = new Map<string, LinkChange[]>();
  for (const ec of diff.changes) {
    if (ec.kind !== "link") continue;
    const link = linkById.get(ec.id);
    const host = link?.src ?? link?.dst; // dropped-link fallback: either end
    if (!host) continue;
    const lc: LinkChange = { ec, dst: link?.dst ?? null };
    const arr = linksBySrc.get(host);
    if (arr) arr.push(lc);
    else linksBySrc.set(host, [lc]);
  }

  // Every node/group that carries a change — its own, content it owns (a
  // reworded claim surfaces its host node), or a link it performs.
  const nodeIds = new Set<string>(idx.nodeOwn.keys());
  const groupIds = new Set<string>(idx.groupOwn.keys());
  for (const ownerId of idx.byOwner.keys())
    (isGroup.has(ownerId) ? groupIds : nodeIds).add(ownerId);
  for (const host of linksBySrc.keys()) if (!isGroup.has(host)) nodeIds.add(host);

  const entries: PlanEntry[] = [];
  const push = (kind: "node" | "group", id: string) => {
    const node = kind === "node" ? nodeById.get(id) : undefined;
    // The same drift filter as the gutter's nodeMarks: vagrant content (and a
    // vagrant node's own plan-only "added") is code-first review material, not
    // a planned edit — without this the page listed what the gutter didn't.
    const vagrantIds = new Set<string>();
    const vagrantProps = new Set<string>();
    for (const r of node?.responsibilities ?? []) if (r.vagrant) vagrantIds.add(r.id);
    for (const p of node?.properties ?? []) if (p.vagrant) vagrantProps.add(p.label);
    const own = node?.vagrant
      ? undefined
      : (kind === "node" ? idx.nodeOwn : idx.groupOwn).get(id);
    const children = (idx.byOwner.get(id) ?? []).filter(
      (ec) =>
        !(ec.kind === "responsibility" && vagrantIds.has(ec.id)) &&
        !(ec.kind === "property" && vagrantProps.has(ec.id)),
    );
    const links = kind === "node" ? (linksBySrc.get(id) ?? []) : [];
    const childChanges = [
      ...children.flatMap((c) => c.changes),
      ...links.flatMap((l) => l.ec.changes),
    ];
    const mark = classifyPlan(own, childChanges);
    if (!mark) return;
    entries.push({ kind, id, mark, own: own ?? [], children, links });
  };
  for (const id of nodeIds) push("node", id);
  for (const id of groupIds) push("group", id);
  return entries;
}

// --- subtree roll-up ----------------------------------------------------------

/** Priority when several descendant marks collapse into one rolled-up letter:
 *  a hidden deletion outranks an addition outranks structure outranks rewords;
 *  undescribed behaviour outranks staleness. */
const PLAN_ROLLUP: Mark[] = ["D", "A", "R", "M"];
const DRIFT_ROLLUP: Mark[] = ["Q", "X"];

/** Merge mark pairs by roll-up priority. */
export function combineMarks(pairs: (MarkPair | undefined)[]): MarkPair {
  const plan = new Set<Mark>();
  const drift = new Set<Mark>();
  for (const p of pairs) {
    if (p?.plan) plan.add(p.plan);
    if (p?.drift) drift.add(p.drift);
  }
  return {
    plan: PLAN_ROLLUP.find((m) => plan.has(m)) ?? null,
    drift: DRIFT_ROLLUP.find((m) => drift.has(m)) ?? null,
  };
}

/** Subtree roll-up: every node's DESCENDANT marks (plan entries + drift flags)
 *  bubbled onto each ancestor, so a collapsed branch still shows what it hides
 *  (the README's "rolled-up plan and drift marks"). Deleted descendants
 *  attribute through their committed parent — the "D" a dropped node cannot
 *  show itself (it has no row) surfaces on the surviving branch. Groups bubble
 *  through their anchoring level's node. */
export function rollupMarks(
  model: ScryModel,
  committed: ScryModel | null,
  entries: PlanEntry[],
): Map<string, MarkPair> {
  const plannedParent = new Map(model.nodes.map((n) => [n.id, n.parentId ?? null] as const));
  const committedParent = new Map(
    (committed?.nodes ?? []).map((n) => [n.id, n.parentId ?? null] as const),
  );
  // has() before get(): a planned ROOT's parent is a legitimate null, which
  // must not fall through to the committed map.
  const parentOf = (id: string): string | null =>
    plannedParent.has(id)
      ? (plannedParent.get(id) ?? null)
      : (committedParent.get(id) ?? null);

  const groupById = new Map(
    [...model.groups, ...(committed?.groups ?? [])].map((g) => [g.id, g] as const),
  );
  const groupAnchor = (id: string): string | null => {
    const g = groupById.get(id);
    if (!g) return null;
    return (
      g.parentNodeId ??
      model.nodes.find((n) => n.id === g.memberIds[0])?.parentId ??
      null
    );
  };

  const acc = new Map<string, { plan: Set<Mark>; drift: Set<Mark> }>();
  const bubble = (start: string | null, axis: "plan" | "drift", mark: Mark) => {
    let cur = start;
    const seen = new Set<string>();
    while (cur != null && !seen.has(cur)) {
      seen.add(cur);
      let slot = acc.get(cur);
      if (!slot) {
        slot = { plan: new Set(), drift: new Set() };
        acc.set(cur, slot);
      }
      slot[axis].add(mark);
      cur = parentOf(cur);
    }
  };

  for (const e of entries) {
    const start = e.kind === "group" ? groupAnchor(e.id) : parentOf(e.id);
    bubble(start, "plan", e.mark);
  }
  for (const n of model.nodes) {
    const d = nodeDrift(n);
    if (d) bubble(n.parentId ?? null, "drift", d);
  }
  for (const g of model.groups) {
    const d = groupDrift(g);
    if (d) bubble(groupAnchor(g.id), "drift", d);
  }

  return new Map(
    [...acc].map(([id, { plan, drift }]) => [
      id,
      {
        plan: PLAN_ROLLUP.find((m) => plan.has(m)) ?? null,
        drift: DRIFT_ROLLUP.find((m) => drift.has(m)) ?? null,
      },
    ]),
  );
}
