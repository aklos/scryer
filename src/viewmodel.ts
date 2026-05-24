/**
 * On-disk schema (v0.3) + derived view types.
 *
 * `ScryModel`, `Node`, `Link`, `Group`, `Responsibility`, `Source`,
 * `SchemaProperty`, `Kind`, `Status` mirror the Rust types in
 * `crates/scryer-core/src/lib.rs` exactly — what gets read from
 * `{project}/.scryer/model.scry` IS the in-memory model.
 *
 * `SurfaceView`, `NodeView`, `GroupView` are computed on the fly from
 * `(model, parentNodeId)` for rendering. They are not persisted.
 */

import type { Status } from "./statusColors";

export type { Status };

export const SCRY_VERSION = "0.3" as const;

// --- Core enums --------------------------------------------------------------

export type Kind =
  | "person"
  | "system"
  | "container"
  | "component"
  | "symbol"
  | "schema";

export type Altitude = "system" | "container" | "component" | "code";

// --- Layout ------------------------------------------------------------------

export interface Cell {
  row: number;
  col: number;
}

export interface GroupSize {
  cols: number;
  rows: number;
}

// --- Responsibilities & code-level data --------------------------------------

export interface Responsibility {
  id: string;
  /** Verb-led business statement of accountability. No mechanism words. */
  statement: string;
  status?: Status;
  locked?: boolean;
  /** Source side: node ID the responsibility was moved to. */
  relocatedTo?: string;
  /** Destination side: node ID the responsibility came from. */
  relocatedFrom?: string;
  /** Optional prescriptive HOW-constraints ("must"/"never" rules) — not part of conformance. */
  directives?: string[];
}

export interface SchemaProperty {
  label: string;
  description?: string;
  status?: Status;
}

export interface Source {
  pattern: string;
  comment?: string;
}

export interface SourceLocation {
  pattern: string;
  /** Durable anchor: identifier resolved to a line range on demand. */
  symbol?: string;
  line?: number;
  endLine?: number;
  command?: string;
}

// --- Nodes & links -----------------------------------------------------------

export interface Node {
  id: string;
  kind: Kind;
  name: string;
  parentId?: string;
  external?: boolean;
  /** What this node IS as software ("Payload 3.0", "PostgreSQL 16", "S3 Bucket"). */
  technology?: string;
  /** 1–2 sentence prose about what this node is. */
  description?: string;
  responsibilities?: Responsibility[];
  /** Model-kind nodes only. */
  properties?: SchemaProperty[];
  cell?: Cell;
  /** Optional lucide-react icon name override (frontend-only). */
  icon?: string;
  deprecated?: boolean;
  relocated?: boolean;
  locked?: boolean;
  relocatedTo?: string;
  relocatedFrom?: string;
}

export interface Link {
  id: string;
  src: string;
  dst: string;
  label: string;
  method?: string;
}

export interface Group {
  id: string;
  name: string;
  description?: string;
  memberIds: string[];
  parentGroupId?: string;
  parentNodeId?: string | null;
  responsibilities?: Responsibility[];
  cell?: Cell;
  size?: GroupSize;
  /** Optional lucide-react icon name override (frontend-only). */
  icon?: string;
}

export interface ScryModel {
  version: typeof SCRY_VERSION;
  nodes: Node[];
  links: Link[];
  groups: Group[];
  /** Keyed by **responsibility id** → line-precise locations (conformance
   *  numerator). Agent-produced and regenerable; never hand-authored. */
  sourceMap?: Record<string, SourceLocation[]>;
  /** Keyed by **node id** → boundary globs (coverage denominator + extraction
   *  scope). Agent-produced and regenerable; never hand-authored. */
  boundaries?: Record<string, Source[]>;
}

export function emptyModel(): ScryModel {
  return {
    version: SCRY_VERSION,
    nodes: [],
    links: [],
    groups: [],
    sourceMap: {},
    boundaries: {},
  };
}

// --- Derived view types (NOT persisted) --------------------------------------

/**
 * A Node enriched with rendering metadata. `_groupId`, `_outgoingLinks`,
 * `_incomingLinks` are derived from the model on every view computation.
 */
export interface NodeView extends Node {
  _groupId?: string;
  _outgoingLinks: Link[];
  _incomingLinks: Link[];
  _childCount: number;
  /** Convenience alias for legacy renderers that read `links` (= outgoing). */
  readonly links: Link[];
  /** Required form of `responsibilities` for renderers. */
  responsibilities: Responsibility[];
}

/** A Group enriched with required layout fields (defaulted if absent on disk). */
export interface GroupView extends Group {
  cell: Cell;
  size: GroupSize;
}

/** The set of nodes + groups visible at one navigation depth. */
export interface SurfaceView {
  /** Parent node id, or `null` at the root (top-level systems / persons). */
  parentId: string | null;
  altitude: Altitude;
  entries: NodeView[];
  groups: GroupView[];
}

// --- View derivation ---------------------------------------------------------

const ALTITUDE_FOR_PARENT: Record<Kind | "root", Altitude> = {
  root: "system",
  system: "container",
  container: "component",
  component: "code",
  // symbol / schema don't have children — no SurfaceView altitude.
  person: "system",
  symbol: "code",
  schema: "code",
};

export function altitudeFor(parentKind: Kind | "root"): Altitude {
  return ALTITUDE_FOR_PARENT[parentKind];
}

/** Child kind for a parent kind (used when adding a new node). */
export function childKindFor(parentKind: Kind | "root"): Kind {
  switch (parentKind) {
    case "root":
      return "system";
    case "system":
      return "container";
    case "container":
      return "component";
    case "component":
      return "symbol";
    default:
      return "component";
  }
}

/** Build a SurfaceView for the children of `parentId` (or top-level if null). */
export function deriveSurfaceView(
  model: ScryModel,
  parentId: string | null,
): SurfaceView {
  const parentKind: Kind | "root" = parentId
    ? (model.nodes.find((n) => n.id === parentId)?.kind ?? "root")
    : "root";

  const visibleNodes = model.nodes.filter(
    (n) => (n.parentId ?? null) === parentId,
  );

  // Inverse of Group.memberIds → group id by node id
  const nodeGroup = new Map<string, string>();
  for (const g of model.groups) {
    for (const m of g.memberIds) nodeGroup.set(m, g.id);
  }

  // Precompute incoming/outgoing by node id (cheap for moderate models)
  const out = new Map<string, Link[]>();
  const inc = new Map<string, Link[]>();
  for (const l of model.links) {
    (out.get(l.src) ?? out.set(l.src, []).get(l.src)!).push(l);
    (inc.get(l.dst) ?? inc.set(l.dst, []).get(l.dst)!).push(l);
  }

  // Precompute direct child count per node
  const childCount = new Map<string, number>();
  for (const n of model.nodes) {
    if (n.parentId) {
      childCount.set(n.parentId, (childCount.get(n.parentId) ?? 0) + 1);
    }
  }

  const entries: NodeView[] = visibleNodes.map((n) => {
    const outgoing = out.get(n.id) ?? [];
    const incoming = inc.get(n.id) ?? [];
    return {
      ...n,
      responsibilities: n.responsibilities ?? [],
      _groupId: nodeGroup.get(n.id),
      _outgoingLinks: outgoing,
      _incomingLinks: incoming,
      _childCount: childCount.get(n.id) ?? 0,
      links: outgoing,
    };
  });

  // Groups visible here = groups with members at this depth, or empty groups
  // explicitly placed on this surface via parentNodeId.
  const visibleNodeIds = new Set(visibleNodes.map((n) => n.id));
  const groups: GroupView[] = model.groups
    .filter(
      (g) =>
        g.memberIds.some((m) => visibleNodeIds.has(m)) ||
        (g.memberIds.length === 0 &&
          (g.parentNodeId ?? null) === parentId),
    )
    .map((g) => deriveGroupView(g, visibleNodes));

  return {
    parentId,
    altitude: altitudeFor(parentKind),
    entries,
    groups,
  };
}

/**
 * A Group on disk may have no `cell`/`size`. For rendering, default both from
 * the bounding box of its current members; if members lack positions, anchor
 * at (0,0) with a 1×1 size and let the renderer auto-grow.
 */
function deriveGroupView(group: Group, visibleNodes: Node[]): GroupView {
  if (group.cell && group.size) {
    return { ...group, cell: group.cell, size: group.size };
  }
  const members = visibleNodes.filter((n) => group.memberIds.includes(n.id));
  let minRow = Infinity,
    minCol = Infinity,
    maxRow = -Infinity,
    maxCol = -Infinity;
  for (const n of members) {
    if (!n.cell) continue;
    minRow = Math.min(minRow, n.cell.row);
    minCol = Math.min(minCol, n.cell.col);
    maxRow = Math.max(maxRow, n.cell.row + 1);
    maxCol = Math.max(maxCol, n.cell.col + 1);
  }
  if (!Number.isFinite(minRow)) {
    return {
      ...group,
      cell: group.cell ?? { row: 0, col: 0 },
      size: group.size ?? { cols: 1, rows: 1 },
    };
  }
  return {
    ...group,
    cell: { row: minRow, col: minCol },
    size: { cols: maxCol - minCol, rows: maxRow - minRow },
  };
}

// --- Model mutation helpers --------------------------------------------------

export function updateNode(
  model: ScryModel,
  nodeId: string,
  patch: Partial<Node>,
): ScryModel {
  return {
    ...model,
    nodes: model.nodes.map((n) => (n.id === nodeId ? { ...n, ...patch } : n)),
  };
}

export function setNodeCell(
  model: ScryModel,
  nodeId: string,
  cell: Cell,
): ScryModel {
  return updateNode(model, nodeId, { cell });
}

/** Move a node into a group (or out of any group when `groupId` is null). */
export function setNodeGroup(
  model: ScryModel,
  nodeId: string,
  groupId: string | null,
): ScryModel {
  return {
    ...model,
    groups: model.groups.map((g) => {
      const has = g.memberIds.includes(nodeId);
      if (g.id === groupId && !has) {
        return { ...g, memberIds: [...g.memberIds, nodeId] };
      }
      if (g.id !== groupId && has) {
        return { ...g, memberIds: g.memberIds.filter((m) => m !== nodeId) };
      }
      return g;
    }),
  };
}

export function updateGroup(
  model: ScryModel,
  groupId: string,
  patch: Partial<Group>,
): ScryModel {
  return {
    ...model,
    groups: model.groups.map((g) =>
      g.id === groupId ? { ...g, ...patch } : g,
    ),
  };
}

// --- ID generation -----------------------------------------------------------

function nextNumericId(prefix: string, existing: Iterable<string>): string {
  let max = 0;
  for (const id of existing) {
    if (!id.startsWith(prefix + "-")) continue;
    const n = parseInt(id.slice(prefix.length + 1), 10);
    if (!Number.isNaN(n) && n > max) max = n;
  }
  return `${prefix}-${max + 1}`;
}

export function nextNodeId(model: ScryModel): string {
  return nextNumericId(
    "node",
    model.nodes.map((n) => n.id),
  );
}

export function nextGroupId(model: ScryModel): string {
  return nextNumericId(
    "group",
    model.groups.map((g) => g.id),
  );
}

export function nextResponsibilityId(existing: Responsibility[]): string {
  return nextNumericId(
    "resp",
    existing.map((r) => r.id),
  );
}

export function nextLinkId(model: ScryModel): string {
  return nextNumericId(
    "link",
    model.links.map((l) => l.id),
  );
}

// --- Add / remove nodes ------------------------------------------------------

/** Add a new node. Returns `{model, id}`. The cell is left unset; the caller
 *  (or the layout pass) decides where to place it. */
export function addNode(
  model: ScryModel,
  init: {
    kind: Kind;
    name: string;
    parentId?: string;
    cell?: Cell;
    groupId?: string;
  },
): { model: ScryModel; id: string } {
  const id = nextNodeId(model);
  const node: Node = {
    id,
    kind: init.kind,
    name: init.name,
    parentId: init.parentId,
    cell: init.cell,
    responsibilities: [],
    properties: [],
  };
  let next: ScryModel = { ...model, nodes: [...model.nodes, node] };
  if (init.groupId) next = setNodeGroup(next, id, init.groupId);
  return { model: next, id };
}

/** Remove a node, its descendants, all attached links, source-map entries,
 *  and its membership from any groups. */
export function removeNode(model: ScryModel, nodeId: string): ScryModel {
  const remove = new Set<string>([nodeId]);
  const stack = [nodeId];
  while (stack.length) {
    const id = stack.pop()!;
    for (const n of model.nodes) {
      if (n.parentId === id && !remove.has(n.id)) {
        remove.add(n.id);
        stack.push(n.id);
      }
    }
  }
  // source_map is keyed by responsibility id or schema node id: drop entries
  // for every responsibility owned by a removed node, and for the removed nodes
  // themselves (schema declaration locations). boundaries are keyed by node id.
  const removedRespIds = new Set<string>();
  for (const n of model.nodes) {
    if (remove.has(n.id)) {
      for (const r of n.responsibilities ?? []) removedRespIds.add(r.id);
    }
  }
  const sourceMap = { ...(model.sourceMap ?? {}) };
  for (const id of removedRespIds) delete sourceMap[id];
  for (const id of remove) delete sourceMap[id];
  const boundaries = { ...(model.boundaries ?? {}) };
  for (const id of remove) delete boundaries[id];
  return {
    ...model,
    nodes: model.nodes.filter((n) => !remove.has(n.id)),
    links: model.links.filter(
      (l) => !remove.has(l.src) && !remove.has(l.dst),
    ),
    groups: model.groups.map((g) => ({
      ...g,
      memberIds: g.memberIds.filter((m) => !remove.has(m)),
    })),
    sourceMap,
    boundaries,
  };
}

// --- Add / remove groups -----------------------------------------------------

/** Add a new group at the given size (defaults to 2×2). Members start empty. */
export function addGroup(
  model: ScryModel,
  init: {
    name: string;
    cell?: Cell;
    size?: GroupSize;
    memberIds?: string[];
    parentNodeId?: string | null;
  },
): { model: ScryModel; id: string } {
  const id = nextGroupId(model);
  const group: Group = {
    id,
    name: init.name,
    memberIds: init.memberIds ?? [],
    parentNodeId: init.parentNodeId,
    cell: init.cell,
    size: init.size ?? { cols: 2, rows: 2 },
  };
  return { model: { ...model, groups: [...model.groups, group] }, id };
}

/** Remove a group. Members survive (they just lose their group association).
 *  Child groups are reparented up to their grandparent (or top-level). */
export function removeGroup(model: ScryModel, groupId: string): ScryModel {
  const target = model.groups.find((g) => g.id === groupId);
  if (!target) return model;
  return {
    ...model,
    groups: model.groups
      .filter((g) => g.id !== groupId)
      .map((g) =>
        g.parentGroupId === groupId
          ? { ...g, parentGroupId: target.parentGroupId }
          : g,
      ),
  };
}

// --- Responsibility CRUD -----------------------------------------------------

type ResponsibilityHost = "node" | "group";

function setResponsibilities(
  model: ScryModel,
  host: ResponsibilityHost,
  hostId: string,
  next: Responsibility[],
): ScryModel {
  if (host === "node") {
    return {
      ...model,
      nodes: model.nodes.map((n) =>
        n.id === hostId ? { ...n, responsibilities: next } : n,
      ),
    };
  }
  return {
    ...model,
    groups: model.groups.map((g) =>
      g.id === hostId ? { ...g, responsibilities: next } : g,
    ),
  };
}

function getResponsibilities(
  model: ScryModel,
  host: ResponsibilityHost,
  hostId: string,
): Responsibility[] {
  if (host === "node") {
    return model.nodes.find((n) => n.id === hostId)?.responsibilities ?? [];
  }
  return model.groups.find((g) => g.id === hostId)?.responsibilities ?? [];
}

export function addResponsibility(
  model: ScryModel,
  host: ResponsibilityHost,
  hostId: string,
  statement: string = "",
): { model: ScryModel; id: string } {
  const existing = getResponsibilities(model, host, hostId);
  const id = nextResponsibilityId(existing);
  const resp: Responsibility = { id, statement };
  return {
    model: setResponsibilities(model, host, hostId, [...existing, resp]),
    id,
  };
}

export function updateResponsibility(
  model: ScryModel,
  host: ResponsibilityHost,
  hostId: string,
  respId: string,
  patch: Partial<Responsibility>,
): ScryModel {
  const existing = getResponsibilities(model, host, hostId);
  return setResponsibilities(
    model,
    host,
    hostId,
    existing.map((r) => (r.id === respId ? { ...r, ...patch } : r)),
  );
}

export function removeResponsibility(
  model: ScryModel,
  host: ResponsibilityHost,
  hostId: string,
  respId: string,
): ScryModel {
  const existing = getResponsibilities(model, host, hostId);
  return setResponsibilities(
    model,
    host,
    hostId,
    existing.filter((r) => r.id !== respId),
  );
}

// --- Responsibility relocation ------------------------------------------------

/**
 * Move a responsibility from one node to another.
 *
 * Transition rules:
 *  - proposed: just moves, no trace at source (no code to relocate)
 *  - implemented/verified: source keeps a locked relocated copy pointing to
 *    destination; destination gets a relocated copy pointing back to source
 *  - relocated: stays relocated at destination, source keeps locked copy
 *  - vagrant: not movable
 *
 * Deleting the destination copy should unlock the source (see unlockRelocated).
 */
export function moveResponsibility(
  model: ScryModel,
  fromNodeId: string,
  toNodeId: string,
  respId: string,
): ScryModel {
  const sourceResps = getResponsibilities(model, "node", fromNodeId);
  const resp = sourceResps.find((r) => r.id === respId);
  if (!resp || resp.locked) return model;

  const status = resp.status ?? "proposed";
  const hasCode = status === "implemented" || status === "verified" || status === "relocated";

  const destResps = getResponsibilities(model, "node", toNodeId);
  const newId = nextResponsibilityId([...sourceResps, ...destResps]);

  if (hasCode) {
    const sourceCopy: Responsibility = {
      ...resp,
      status: "relocated",
      locked: true,
      relocatedTo: toNodeId,
    };
    const destCopy: Responsibility = {
      ...resp,
      id: newId,
      status: "relocated",
      relocatedFrom: fromNodeId,
    };
    let next = setResponsibilities(
      model,
      "node",
      fromNodeId,
      sourceResps.map((r) => (r.id === respId ? sourceCopy : r)),
    );
    next = setResponsibilities(
      next,
      "node",
      toNodeId,
      [...getResponsibilities(next, "node", toNodeId), destCopy],
    );
    return next;
  }

  // proposed: just move, no trace at source
  const destCopy: Responsibility = { ...resp, id: newId };
  let next = setResponsibilities(
    model,
    "node",
    fromNodeId,
    sourceResps.filter((r) => r.id !== respId),
  );
  next = setResponsibilities(
    next,
    "node",
    toNodeId,
    [...getResponsibilities(next, "node", toNodeId), destCopy],
  );
  return next;
}

/**
 * When a relocated destination responsibility is deleted, unlock the
 * source copy and revert it to its pre-relocation status.
 */
export function unlockRelocatedSource(
  model: ScryModel,
  deletedResp: Responsibility,
): ScryModel {
  if (!deletedResp.relocatedFrom) return model;
  const sourceNodeId = deletedResp.relocatedFrom;
  const sourceResps = getResponsibilities(model, "node", sourceNodeId);
  return setResponsibilities(
    model,
    "node",
    sourceNodeId,
    sourceResps.map((r) => {
      if (r.statement === deletedResp.statement && r.locked && r.relocatedTo) {
        return {
          ...r,
          status: "implemented" as Status,
          locked: undefined,
          relocatedTo: undefined,
        };
      }
      return r;
    }),
  );
}

// --- Property CRUD (model-kind nodes) ----------------------------------------

export function addProperty(
  model: ScryModel,
  nodeId: string,
  label: string = "",
  description?: string,
): ScryModel {
  return {
    ...model,
    nodes: model.nodes.map((n) => {
      if (n.id !== nodeId) return n;
      const existing = n.properties ?? [];
      return {
        ...n,
        properties: [...existing, { label, description: description ?? "" }],
      };
    }),
  };
}

export function updateProperty(
  model: ScryModel,
  nodeId: string,
  index: number,
  patch: Partial<SchemaProperty>,
): ScryModel {
  return {
    ...model,
    nodes: model.nodes.map((n) => {
      if (n.id !== nodeId) return n;
      const existing = n.properties ?? [];
      const next = existing.map((p, i) => (i === index ? { ...p, ...patch } : p));
      return { ...n, properties: next };
    }),
  };
}

export function removeProperty(
  model: ScryModel,
  nodeId: string,
  index: number,
): ScryModel {
  return {
    ...model,
    nodes: model.nodes.map((n) => {
      if (n.id !== nodeId) return n;
      const existing = n.properties ?? [];
      return { ...n, properties: existing.filter((_, i) => i !== index) };
    }),
  };
}

