/**
 * The derived shapes rebuilt from a claim list, for whatever base the film
 * plays on (demo/fixture.json derived from a real repo, or the hand-written
 * fixture.ts): each layer's model carries the claims as responsibilities and
 * test-map entries, and the status rolls each claim's state up its node's
 * ancestors. Pure; the film calls these each frame after applying its script.
 */

import type {
  ClaimState,
  ClaimStatus,
  DerivedClaim,
  DerivedCounts,
  DerivedStatus,
  DerivedTree,
  DerivedViolation,
  Node,
  ProbeMark,
  ScryModel,
} from "./types";

/** `model` with exactly `claims` on its nodes and in its test map. */
export function withClaims(model: ScryModel, claims: DerivedClaim[]): ScryModel {
  return {
    ...model,
    nodes: model.nodes.map((n) => ({
      ...n,
      responsibilities: claims
        .filter((c) => c.nodeId === n.id)
        .map((c) => ({ id: c.id, statement: c.description, concern: c.concerns?.[0] })),
    })),
    testMap: Object.fromEntries(claims.map((c) => [c.id, [{ pattern: c.file, symbol: c.identifier, line: c.line }]])),
  };
}

/** The tree `base` with `claims` and `violations` in place of its own. */
export function treeWith(base: DerivedTree, claims: DerivedClaim[], violations: DerivedViolation[]): DerivedTree {
  return {
    ...base,
    committed: withClaims(base.planned, claims.filter((c) => !c.planned)),
    planned: withClaims(base.planned, claims),
    claims,
    styleViolations: violations,
  };
}

export const zeroCounts = (): DerivedCounts => ({
  claims: 0,
  proven: 0,
  stale: 0,
  failing: 0,
  planned: 0,
  untestedComponents: 0,
  survivingMutants: 0,
  styleViolations: 0,
  darkFiles: 0,
});

/** Status for the tree's claims given each one's state, probe mark and when
 *  its verdict was recorded, rolled up each node's ancestors. */
export function statusOf(
  tree: DerivedTree,
  states: Record<string, ClaimState>,
  probes: Record<string, ProbeMark>,
  recorded: Record<string, number>,
): DerivedStatus {
  const claimStatus: ClaimStatus[] = tree.claims.map((c) => ({
    respId: c.id,
    nodeId: c.nodeId,
    state: c.planned ? "planned" : (states[c.id] ?? "stale"),
    probe: probes[c.id],
    recordedAt: c.planned ? undefined : recorded[c.id],
  }));
  const parent = new Map(tree.planned.nodes.map((n: Node) => [n.id, n.parentId]));
  const nodes: DerivedStatus["nodes"] = {};
  const at = (id: string) => (nodes[id] ??= { own: zeroCounts(), subtree: zeroCounts() });
  const totals = zeroCounts();
  for (const s of claimStatus) {
    const bump = (c: DerivedCounts) => {
      c.claims += 1;
      c[s.state] += 1;
      if (s.probe === "crosshair") c.survivingMutants += 1;
    };
    bump(at(s.nodeId).own);
    bump(totals);
    for (let id: string | undefined = s.nodeId; id; id = parent.get(id)) bump(at(id).subtree);
  }
  for (const v of tree.styleViolations) {
    at(v.nodeId).own.styleViolations += 1;
    totals.styleViolations += 1;
  }
  totals.darkFiles = tree.darkFiles.length;
  return { claims: claimStatus, nodes, totals };
}

/** A node by reference: its id, its name, or a `Parent/child` name path —
 *  the script names nodes the way a reader does, so derived ids can change. */
export function resolveNode(model: ScryModel, ref: string): string | undefined {
  if (model.nodes.some((n) => n.id === ref)) return ref;
  const [first, ...rest] = ref.split("/");
  let candidates = model.nodes.filter((n) => n.name === first && n.kind !== "symbol");
  for (const name of rest) {
    const ids = new Set(candidates.map((c) => c.id));
    candidates = model.nodes.filter((n) => n.name === name && n.parentId !== undefined && ids.has(n.parentId));
  }
  return candidates[0]?.id;
}

/** The system the project is: the one top-level, non-external system. */
export function systemOf(model: ScryModel): string | undefined {
  return model.nodes.find((n) => n.kind === "system" && !n.parentId && !n.external)?.id;
}
