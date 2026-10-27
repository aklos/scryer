/**
 * The states the reel's app is shown in, built from the derived base
 * (aperture-pay): its claims and their verdicts, with what a beat changes
 * laid over them — claims added or removed, verdicts flipped, a violation.
 * Claims are named by their wording, nodes by their names.
 */

import base from "@film/base";
import { REFUND_STUBS, STYLE_VIOLATION } from "../../fixture";
import { statusOf, treeWith } from "../../shapes";
import type { ClaimState, DerivedClaim, DerivedViolation, Fixture, ProbeMark } from "../../types";

export const BASE = base as Fixture;

const states0: Record<string, ClaimState> = {};
const probes0: Record<string, ProbeMark> = {};
const recorded0: Record<string, number> = {};
for (const c of BASE.status.claims) {
  states0[c.respId] = c.state;
  if (c.probe) probes0[c.respId] = c.probe;
  if (c.recordedAt) recorded0[c.respId] = c.recordedAt;
}

/** A node's id by name (`Ledger`), or a `Parent/child` path. */
export function nodeId(ref: string): string {
  const [first, ...rest] = ref.split("/");
  let found = BASE.tree.planned.nodes.filter((n) => n.name === first && n.kind !== "symbol");
  for (const name of rest) {
    const ids = new Set(found.map((n) => n.id));
    found = BASE.tree.planned.nodes.filter((n) => n.name === name && n.parentId !== undefined && ids.has(n.parentId));
  }
  if (!found[0]) throw new Error(`no node "${ref}" in the reel's base`);
  return found[0].id;
}

/** A base claim's id by its wording. */
export function claimId(text: string): string {
  const c = BASE.tree.claims.find((c) => c.description === text) ?? REFUND_STUBS.find((c) => c.description === text);
  if (!c) throw new Error(`no claim "${text}" in the reel's base`);
  return c.id;
}

export interface Change {
  /** Claims left out, by wording. */
  drop?: string[];
  /** Claims added (the refund stubs), each planned or with a state. */
  add?: { claim: DerivedClaim; state?: ClaimState; probe?: ProbeMark }[];
  /** States for base claims, by wording. */
  states?: Record<string, ClaimState>;
  probes?: Record<string, ProbeMark>;
  violations?: DerivedViolation[];
}

/** The base with `change` laid over it. */
export function world(change: Change = {}): Fixture {
  const dropped = new Set(change.drop ?? []);
  const ledger = nodeId("Ledger");
  const added = (change.add ?? []).map(({ claim, state }) => ({ ...claim, nodeId: ledger, planned: !state || state === "planned" }));
  const claims = [...BASE.tree.claims.filter((c) => !dropped.has(c.description)), ...added];
  const violations = change.violations ?? BASE.tree.styleViolations;
  const tree = treeWith(BASE.tree, claims, violations);
  const states = { ...states0 };
  const probes = { ...probes0 };
  const recorded = { ...recorded0 };
  for (const [text, st] of Object.entries(change.states ?? {})) states[claimId(text)] = st;
  for (const [text, pr] of Object.entries(change.probes ?? {})) probes[claimId(text)] = pr;
  for (const a of change.add ?? []) {
    if (a.state && a.state !== "planned") {
      states[a.claim.id] = a.state;
      recorded[a.claim.id] = BASE.status.claims[0]?.recordedAt ?? 0;
    }
    if (a.probe) probes[a.claim.id] = a.probe;
  }
  return { ...BASE, tree, status: statusOf(tree, states, probes, recorded) };
}

/** The violation the style beat shows: domain/memory.rs importing application. */
export const MISPLACED: DerivedViolation = (() => {
  const v = BASE.violation ?? STYLE_VIOLATION;
  const resolve = (ref: string | undefined) => (ref ? (ref.startsWith("node-") ? ref : nodeId(ref)) : undefined);
  return { ...v, nodeId: resolve(v.nodeId)!, other: resolve(v.other) };
})();

export { REFUND_STUBS };
