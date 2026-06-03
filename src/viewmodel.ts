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

/** One boundary-owning node whose code changed since the last reconcile, as
 *  returned by the `get_drift_status` Tauri command (mirrors the Rust
 *  `scryer_core::drift::DriftScope`). Drives the SyncBar drift panel. */
export interface DriftScope {
  nodeId: string;
  nodeName: string;
  /** Project-relative files under this node's boundary that changed. */
  changedFiles: string[];
}

// --- Core enums --------------------------------------------------------------

export type Kind =
  | "person"
  | "system"
  | "container"
  | "component"
  | "symbol";

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
  /** Discovered in code with no upstream commitment (drift). The user adopts
   *  it (clear the flag) or rejects it (delete it). */
  vagrant?: boolean;
  /** Source side: node ID the responsibility was moved to. */
  relocatedTo?: string;
  /** Destination side: node ID the responsibility came from. */
  relocatedFrom?: string;
  /** Optional prescriptive HOW-constraints ("must"/"never" rules) — not part of conformance. */
  directives?: string[];
  /** Unix seconds of the last truth-bearing edit. Drives the canvas
   *  fossilization patina (fresh → settled → stone). Stamped automatically by
   *  the Rust write path (agent edits) and the mutation helpers below (canvas
   *  edits); never hand-authored. */
  lastTouchedAt?: number;
}

export interface SchemaProperty {
  label: string;
  description?: string;
  status?: Status;
  /** Unix seconds of the last truth-bearing edit — see {@link Responsibility.lastTouchedAt}. */
  lastTouchedAt?: number;
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
  /** Field declarations, when this symbol defines a data shape. */
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
  // symbols don't have children — no SurfaceView altitude.
  person: "system",
  symbol: "code",
};

/**
 * A symbol that defines a data type — it declares fields and discharges no
 * behavior. Renders with the table affordance and (like the former `schema`
 * kind) hides its incoming links, which are typically too numerous to be
 * useful. A symbol carrying both properties and responsibilities is not a pure
 * data shape and renders as a normal code node.
 */
export function isDataShape(node: {
  properties?: SchemaProperty[];
  responsibilities?: Responsibility[];
}): boolean {
  return (
    (node.properties?.length ?? 0) > 0 &&
    (node.responsibilities?.length ?? 0) === 0
  );
}

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

/** Unix seconds — the canvas-side fossilization clock. Mirrors Rust's
 *  `drift::now_secs()`. */
function nowSecs(): number {
  return Math.floor(Date.now() / 1000);
}

function sameDirectives(a?: string[], b?: string[]): boolean {
  const x = a ?? [];
  const y = b ?? [];
  if (x.length !== y.length) return false;
  for (let i = 0; i < x.length; i++) if (x[i] !== y[i]) return false;
  return true;
}

function respTruthChanged(a: Responsibility, b: Responsibility): boolean {
  return (
    a.statement !== b.statement ||
    a.status !== b.status ||
    a.vagrant !== b.vagrant ||
    a.locked !== b.locked ||
    a.relocatedTo !== b.relocatedTo ||
    a.relocatedFrom !== b.relocatedFrom ||
    !sameDirectives(a.directives, b.directives)
  );
}

function propTruthChanged(a: SchemaProperty, b: SchemaProperty): boolean {
  return (
    a.label !== b.label || a.description !== b.description || a.status !== b.status
  );
}

/**
 * Stamp `lastTouchedAt` on every responsibility/property whose truth-bearing
 * content is new or changed relative to `prev`, carrying the prior date forward
 * otherwise. The canvas-side mirror of Rust's `stamp_touches`: it runs at the
 * single write chokepoint (`updateModel`) so EVERY canvas edit — granular,
 * EditModal bulk-commit, or the auto-"changed" transition — is dated, while a
 * layout-only change (a card drag, a group resize) re-dates nothing because no
 * truth field moves. Responsibilities are matched per host by id, properties per
 * node by label, exactly like the Rust side.
 */
export function stampTouches(prev: ScryModel, next: ScryModel): ScryModel {
  const now = nowSecs();
  const priorNodeResp = new Map<string, Map<string, Responsibility>>();
  const priorNodeProp = new Map<string, Map<string, SchemaProperty>>();
  for (const n of prev.nodes) {
    priorNodeResp.set(n.id, new Map((n.responsibilities ?? []).map((r) => [r.id, r])));
    priorNodeProp.set(n.id, new Map((n.properties ?? []).map((p) => [p.label, p])));
  }
  const priorGroupResp = new Map<string, Map<string, Responsibility>>();
  for (const g of prev.groups)
    priorGroupResp.set(g.id, new Map((g.responsibilities ?? []).map((r) => [r.id, r])));

  const dateResp = (
    r: Responsibility,
    host: Map<string, Responsibility> | undefined,
  ): Responsibility => {
    const pv = host?.get(r.id);
    const lastTouchedAt = pv && !respTruthChanged(pv, r) ? pv.lastTouchedAt : now;
    return r.lastTouchedAt === lastTouchedAt ? r : { ...r, lastTouchedAt };
  };
  const dateProp = (
    p: SchemaProperty,
    host: Map<string, SchemaProperty> | undefined,
  ): SchemaProperty => {
    const pv = host?.get(p.label);
    const lastTouchedAt = pv && !propTruthChanged(pv, p) ? pv.lastTouchedAt : now;
    return p.lastTouchedAt === lastTouchedAt ? p : { ...p, lastTouchedAt };
  };

  return {
    ...next,
    nodes: next.nodes.map((n) => {
      const hr = priorNodeResp.get(n.id);
      const hp = priorNodeProp.get(n.id);
      return {
        ...n,
        responsibilities: n.responsibilities?.map((r) => dateResp(r, hr)),
        properties: n.properties?.map((p) => dateProp(p, hp)),
      };
    }),
    groups: next.groups.map((g) => {
      const hr = priorGroupResp.get(g.id);
      return { ...g, responsibilities: g.responsibilities?.map((r) => dateResp(r, hr)) };
    }),
  };
}

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

