/**
 * The plan diff — `diff(model, planned)` — ported from `scryer-core/src/diff.rs`.
 *
 * Scryer holds two persisted layers: `model` (the committed source of truth,
 * `model.scry`) and `planned` (the draft the canvas and agent edit). The diff
 * between them IS the plan: per element, how `planned` (`to`) diverges from the
 * committed `model` (`from`) — Added / Deleted / Moved / Repointed / Reworded /
 * MembersChanged. This replaces the old per-element `status` lifecycle: a claim
 * the code doesn't back yet is simply `Added` in the plan; a reworded one is
 * `Reworded`; the live diff needs no stored baseline (no `changedFrom`).
 *
 * Computed client-side so the diff updates live as you edit, with no async
 * round-trip. The JSON shape mirrors the Rust `ModelDiff` exactly (the agent
 * consumes the same diff via `get_pending`), so the two never disagree.
 * Identity is carried by stable ids, so a reparent reads as `Moved` and a
 * relabel as `Reworded`, never as a spurious delete-plus-add.
 */

import type { Group, Responsibility, ScryModel } from "./viewmodel";

export type ElementKind = "node" | "link" | "responsibility" | "property" | "group";

/** A single divergence of `planned` from `model` for one element. Several can
 *  stack on one element (e.g. a responsibility both moved and reworded). The
 *  `type` tag and field names match the Rust `Change` serialization. */
export type Change =
  | { type: "added" }
  | { type: "deleted" }
  /** Same id, different owner — a node's parent, or the node/group a
   *  responsibility lives in. `from`/`to` are owner ids (null = root). */
  | { type: "moved"; from: string | null; to: string | null }
  /** A link's endpoints changed. Endpoints that didn't move have equal
   *  from/to; the UI shows whichever differs. */
  | { type: "repointed"; srcFrom: string; srcTo: string; dstFrom: string; dstTo: string }
  /** A truth-bearing text field changed. */
  | { type: "reworded"; field: string; from: string; to: string }
  /** A group's membership changed (member node ids added / removed). */
  | { type: "membersChanged"; added: string[]; removed: string[] };

/** Every divergence recorded against one element. */
export interface ElementChange {
  kind: ElementKind;
  id: string;
  /** Owning node/group id for a responsibility or property; absent for nodes,
   *  links, and groups. */
  ownerId?: string;
  /** Human-facing label (the element's name / statement). */
  label: string;
  changes: Change[];
}

/** The full set of element changes taking `model` to `planned`. */
export interface ModelDiff {
  changes: ElementChange[];
}

export const EMPTY_DIFF: ModelDiff = { changes: [] };

export function isEmptyDiff(d: ModelDiff): boolean {
  return d.changes.length === 0;
}

/** Push a `reworded` change when two strings differ. */
function reword(changes: Change[], field: string, from: string, to: string) {
  if (from !== to) changes.push({ type: "reworded", field, from, to });
}

/** A responsibility together with the id of the node or group it lives in. */
interface OwnedResp {
  ownerId: string;
  resp: Responsibility;
}

/** Index every responsibility by its (globally unique) id, recording its owner
 *  (node or group), so moving one between owners reads as `moved`, not
 *  delete-plus-add. */
function indexResponsibilities(model: ScryModel): Map<string, OwnedResp> {
  const map = new Map<string, OwnedResp>();
  for (const node of model.nodes)
    for (const r of node.responsibilities ?? []) map.set(r.id, { ownerId: node.id, resp: r });
  for (const group of model.groups)
    for (const r of group.responsibilities ?? []) map.set(r.id, { ownerId: group.id, resp: r });
  return map;
}

/** A group's anchor — the parent group (nesting) or, failing that, the
 *  anchoring node level. Mirrors `group_owner` in diff.rs. */
function groupOwner(g: Group): string | null {
  return g.parentGroupId ?? g.parentNodeId ?? null;
}

/** Compute how `planned` (`to`) diverges from the committed `model` (`from`). */
export function planDiff(from: ScryModel, to: ScryModel): ModelDiff {
  const out: ModelDiff = { changes: [] };
  diffNodes(from, to, out);
  diffLinks(from, to, out);
  diffResponsibilities(from, to, out);
  diffProperties(from, to, out);
  diffGroups(from, to, out);
  return out;
}

function diffNodes(from: ScryModel, to: ScryModel, out: ModelDiff) {
  const fromBy = new Map(from.nodes.map((n) => [n.id, n] as const));
  const toBy = new Map(to.nodes.map((n) => [n.id, n] as const));

  for (const [id, n] of toBy) {
    const prev = fromBy.get(id);
    if (!prev) {
      out.changes.push({ kind: "node", id, label: n.name, changes: [{ type: "added" }] });
      continue;
    }
    const changes: Change[] = [];
    if ((prev.parentId ?? null) !== (n.parentId ?? null))
      changes.push({ type: "moved", from: prev.parentId ?? null, to: n.parentId ?? null });
    reword(changes, "name", prev.name, n.name);
    reword(changes, "technology", prev.technology ?? "", n.technology ?? "");
    reword(changes, "description", prev.description ?? "", n.description ?? "");
    reword(changes, "directives", (prev.directives ?? []).join("\n"), (n.directives ?? []).join("\n"));
    // A visual change is a planned change like any other: we don't diff the
    // fixture contents (the accepted fixture is the basis), so any appearance
    // delta surfaces as one reworded "appearance" claim pointing at the fixture.
    if (JSON.stringify(prev.appearance ?? null) !== JSON.stringify(n.appearance ?? null))
      changes.push({
        type: "reworded",
        field: "appearance",
        from: prev.appearance?.distPath ?? "",
        to: n.appearance?.distPath ?? "",
      });
    if (changes.length) out.changes.push({ kind: "node", id, label: n.name, changes });
  }
  for (const [id, n] of fromBy)
    if (!toBy.has(id))
      out.changes.push({ kind: "node", id, label: n.name, changes: [{ type: "deleted" }] });
}

function diffLinks(from: ScryModel, to: ScryModel, out: ModelDiff) {
  const fromBy = new Map(from.links.map((l) => [l.id, l] as const));
  const toBy = new Map(to.links.map((l) => [l.id, l] as const));

  for (const [id, l] of toBy) {
    const prev = fromBy.get(id);
    if (!prev) {
      out.changes.push({ kind: "link", id, label: l.label, changes: [{ type: "added" }] });
      continue;
    }
    const changes: Change[] = [];
    if (prev.src !== l.src || prev.dst !== l.dst)
      changes.push({ type: "repointed", srcFrom: prev.src, srcTo: l.src, dstFrom: prev.dst, dstTo: l.dst });
    reword(changes, "label", prev.label, l.label);
    reword(changes, "method", prev.method ?? "", l.method ?? "");
    if (changes.length) out.changes.push({ kind: "link", id, label: l.label, changes });
  }
  for (const [id, l] of fromBy)
    if (!toBy.has(id))
      out.changes.push({ kind: "link", id, label: l.label, changes: [{ type: "deleted" }] });
}

function diffResponsibilities(from: ScryModel, to: ScryModel, out: ModelDiff) {
  const fromBy = indexResponsibilities(from);
  const toBy = indexResponsibilities(to);

  for (const [id, owned] of toBy) {
    const prev = fromBy.get(id);
    if (!prev) {
      out.changes.push({
        kind: "responsibility",
        id,
        ownerId: owned.ownerId,
        label: owned.resp.statement,
        changes: [{ type: "added" }],
      });
      continue;
    }
    const changes: Change[] = [];
    if (prev.ownerId !== owned.ownerId)
      changes.push({ type: "moved", from: prev.ownerId, to: owned.ownerId });
    reword(changes, "statement", prev.resp.statement, owned.resp.statement);
    reword(changes, "directives", (prev.resp.directives ?? []).join("\n"), (owned.resp.directives ?? []).join("\n"));
    if (changes.length)
      out.changes.push({ kind: "responsibility", id, ownerId: owned.ownerId, label: owned.resp.statement, changes });
  }
  for (const [id, owned] of fromBy)
    if (!toBy.has(id))
      out.changes.push({
        kind: "responsibility",
        id,
        ownerId: owned.ownerId,
        label: owned.resp.statement,
        changes: [{ type: "deleted" }],
      });
}

/** Properties have no id, so identity is `(owner node id, label)`. A label
 *  change reads as delete-plus-add — acceptable for plain data fields. */
function diffProperties(from: ScryModel, to: ScryModel, out: ModelDiff) {
  const index = (model: ScryModel) => {
    const map = new Map<string, { owner: string; label: string; description: string }>();
    for (const node of model.nodes)
      for (const p of node.properties ?? [])
        map.set(`${node.id}\0${p.label}`, { owner: node.id, label: p.label, description: p.description ?? "" });
    return map;
  };
  const fromBy = index(from);
  const toBy = index(to);

  for (const [key, p] of toBy) {
    const prev = fromBy.get(key);
    if (!prev) {
      out.changes.push({ kind: "property", id: p.label, ownerId: p.owner, label: p.label, changes: [{ type: "added" }] });
      continue;
    }
    const changes: Change[] = [];
    reword(changes, "description", prev.description, p.description);
    if (changes.length)
      out.changes.push({ kind: "property", id: p.label, ownerId: p.owner, label: p.label, changes });
  }
  for (const [key, p] of fromBy)
    if (!toBy.has(key))
      out.changes.push({ kind: "property", id: p.label, ownerId: p.owner, label: p.label, changes: [{ type: "deleted" }] });
}

function diffGroups(from: ScryModel, to: ScryModel, out: ModelDiff) {
  const fromBy = new Map(from.groups.map((g) => [g.id, g] as const));
  const toBy = new Map(to.groups.map((g) => [g.id, g] as const));

  for (const [id, g] of toBy) {
    const prev = fromBy.get(id);
    if (!prev) {
      out.changes.push({ kind: "group", id, label: g.name, changes: [{ type: "added" }] });
      continue;
    }
    const changes: Change[] = [];
    if (groupOwner(prev) !== groupOwner(g))
      changes.push({ type: "moved", from: groupOwner(prev), to: groupOwner(g) });
    reword(changes, "name", prev.name, g.name);
    reword(changes, "description", prev.description ?? "", g.description ?? "");
    const added = g.memberIds.filter((m) => !prev.memberIds.includes(m));
    const removed = prev.memberIds.filter((m) => !g.memberIds.includes(m));
    if (added.length || removed.length) changes.push({ type: "membersChanged", added, removed });
    if (changes.length) out.changes.push({ kind: "group", id, label: g.name, changes });
  }
  for (const [id, g] of fromBy)
    if (!toBy.has(id))
      out.changes.push({ kind: "group", id, label: g.name, changes: [{ type: "deleted" }] });
}
