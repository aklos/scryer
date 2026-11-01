/**
 * What changed in the model since its base — HEAD on the default branch, the
 * merge base off it: which nodes were added or modified, which hold changes
 * below them, how many were removed, and which claim each claim was before.
 *
 * A claim's id is its test file and test name, so rewording a test whose
 * name is its claim (`it("…")`) gives it a new id. Such a claim pairs with
 * its base copy when it is still the same test — same file and same
 * identifier, or same file and same line — so it reads as reworded, not as
 * one claim removed and another added. A claim moved elsewhere reads as
 * removed there and added here.
 */

import type { Claim, Node, ScryModel, SourceLocation } from "./viewmodel";

export type NodeChange = "added" | "modified";

export interface ModelChanges {
  /** Nodes added or modified since the base. */
  nodes: Map<string, NodeChange>;
  /** Nodes with a change somewhere below them: an added, modified or
   *  removed descendant. */
  holding: Set<string>;
  /** How many nodes the base had that the model no longer has. */
  removed: number;
}

export const NO_CHANGES: ModelChanges = { nodes: new Map(), holding: new Set(), removed: 0 };

type Tests = Record<string, SourceLocation[]> | undefined;

function sameTest(a: SourceLocation | undefined, b: SourceLocation | undefined): boolean {
  if (!a || !b || a.pattern !== b.pattern) return false;
  return (a.symbol != null && a.symbol === b.symbol) || (a.line != null && a.line === b.line);
}

/** `base`'s claims, each under the id of the claim in `current` it became:
 *  its own id when that survives, else the id of an unmatched claim on the
 *  same test. Diff `current` against the result by id. */
export function alignClaims(base: Claim[], current: Claim[], baseTests: Tests, tests: Tests): Claim[] {
  const currentIds = new Set(current.map((c) => c.id));
  const baseIds = new Set(base.map((c) => c.id));
  const open = current.filter((c) => !baseIds.has(c.id));
  return base.map((b) => {
    if (currentIds.has(b.id)) return b;
    const at = baseTests?.[b.id]?.[0];
    const i = open.findIndex((c) => sameTest(at, tests?.[c.id]?.[0]));
    if (i < 0) return b;
    const [match] = open.splice(i, 1);
    return { ...b, id: match.id };
  });
}

const claimsKey = (claims: Claim[]) =>
  JSON.stringify(
    [...claims].sort((a, b) => a.id.localeCompare(b.id)).map((c) => [c.id, c.statement, c.concerns ?? []]),
  );

const propsKey = (n: Node) =>
  JSON.stringify((n.properties ?? []).map((p) => [p.label, p.type ?? "", p.description ?? ""]));

const linksKey = (m: ScryModel, id: string) =>
  JSON.stringify(
    m.links
      .filter((l) => l.src === id)
      .map((l) => `${l.dst}|${l.label}|${l.method ?? ""}`)
      .sort(),
  );

/** How `model` differs from `base`, node by node. No base, no changes. */
export function modelChanges(base: ScryModel | null, model: ScryModel): ModelChanges {
  if (!base) return NO_CHANGES;
  const before = new Map(base.nodes.map((n) => [n.id, n]));
  const nodes = new Map<string, NodeChange>();
  for (const n of model.nodes) {
    const b = before.get(n.id);
    if (!b) {
      nodes.set(n.id, "added");
      continue;
    }
    const claims = n.claims ?? [];
    const differs =
      b.name !== n.name ||
      b.parentId !== n.parentId ||
      b.layer !== n.layer ||
      (b.description ?? "") !== (n.description ?? "") ||
      claimsKey(alignClaims(b.claims ?? [], claims, base.testMap, model.testMap)) !== claimsKey(claims) ||
      propsKey(b) !== propsKey(n) ||
      linksKey(base, n.id) !== linksKey(model, n.id);
    if (differs) nodes.set(n.id, "modified");
  }
  const now = new Map(model.nodes.map((n) => [n.id, n]));
  const holding = new Set<string>();
  const markUp = (id: string | undefined) => {
    for (let at = id; at && !holding.has(at); at = now.get(at)?.parentId) holding.add(at);
  };
  for (const id of nodes.keys()) markUp(now.get(id)?.parentId);
  let removed = 0;
  for (const b of base.nodes) {
    if (now.has(b.id)) continue;
    removed++;
    markUp(b.parentId);
  }
  return { nodes, holding, removed };
}
