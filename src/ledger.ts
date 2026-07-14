/**
 * The change ledger, canvas side — named partitions of the plan.
 *
 * Mirrors `scryer-core/src/changes.rs`: the plan carries an open-change
 * registry (`model.changes`, each with the dev's rationale) and a side-map
 * (`model.changeMap`) from element key to change id. The canvas stamps the
 * ACTIVE change onto whatever an edit touched at the `updateModel` chokepoint
 * — the mirror of the MCP server tagging each tool write — so dev-on-canvas
 * and agent-over-MCP attribute into the same ledger. Untagged elements are
 * the unfiled bucket (the zero-friction serial workflow). Keys must match the
 * Rust side byte-for-byte: they are how the two writers agree on identity.
 */

import type { ElementChange, ElementKind, ModelDiff } from "./planDiff";
import { planDiff } from "./planDiff";
import type { ScryModel } from "./viewmodel";

export interface ChangeMeta {
  /** Stable id, minted `chg-N` — same mint rule as the Rust side. */
  id: string;
  /** The dev's original sentence — why this change exists. */
  rationale: string;
  /** Unix seconds. */
  createdAt: number;
}

/** Canonical change-map key for an element — kind-prefixed, properties keyed
 *  by `(owner node, label)`. Must equal `changes::element_key` in Rust. */
export function elementKey(kind: ElementKind, ownerId: string | undefined, id: string): string {
  switch (kind) {
    case "node":
      return `node:${id}`;
    case "link":
      return `link:${id}`;
    case "group":
      return `group:${id}`;
    case "responsibility":
      return `resp:${id}`;
    case "property":
      return `prop:${ownerId ?? ""}:${id}`;
  }
}

/** The key of a diff entry — the join point between the map and the diff. */
export function keyFor(ec: ElementChange): string {
  return elementKey(ec.kind, ec.ownerId, ec.id);
}

/** Tag what an edit changed to the active change: the keys of
 *  `diff(prev, next)` — exactly what THIS edit touched, deletions included,
 *  the same computation the MCP write path uses. Last writer wins a re-tag
 *  (the collision itself is surfaced agent-side at write time). No-op when
 *  the edit touched nothing truth-bearing (a drag re-tags nothing) or the
 *  change is not in this plan's registry (it closed under us). */
export function tagEdit(prev: ScryModel, next: ScryModel, changeId: string): ScryModel {
  if (!(next.changes ?? []).some((c) => c.id === changeId)) return next;
  const keys = planDiff(prev, next).changes.map(keyFor);
  if (keys.length === 0) return next;
  const map = { ...(next.changeMap ?? {}) };
  for (const k of keys) map[k] = changeId;
  return { ...next, changeMap: map };
}

/** Which changes an aggregated plan entry participates in — the tags on its
 *  own element key and on every child/link element it carries. Keys whose
 *  entries the diff no longer holds are naturally ignored (the display-side
 *  analogue of the Rust gc invariant). Empty set = unfiled. */
export function entryChanges(
  entryKind: "node" | "group",
  entryId: string,
  parts: ElementChange[],
  changeMap: Record<string, string> | undefined,
): Set<string> {
  const out = new Set<string>();
  if (!changeMap) return out;
  const own = changeMap[elementKey(entryKind, undefined, entryId)];
  if (own) out.add(own);
  for (const ec of parts) {
    const tag = changeMap[keyFor(ec)];
    if (tag) out.add(tag);
  }
  return out;
}

/** Per-change count of live pending entries (diff-backed, stale tags don't
 *  count) — what the section headers and the powerline report. */
export function liveEntryCounts(
  diff: ModelDiff,
  changeMap: Record<string, string> | undefined,
): Map<string, number> {
  const counts = new Map<string, number>();
  if (!changeMap) return counts;
  for (const ec of diff.changes) {
    const tag = changeMap[keyFor(ec)];
    if (tag) counts.set(tag, (counts.get(tag) ?? 0) + 1);
  }
  return counts;
}
