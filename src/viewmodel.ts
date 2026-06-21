/**
 * On-disk schema (v0.3) + derived view types.
 *
 * `ScryModel`, `Node`, `Link`, `Group`, `Responsibility`, `Source`,
 * `SchemaProperty`, `Kind`, `RenderState` mirror the Rust types in
 * `crates/scryer-core/src/lib.rs` exactly — what gets read from
 * `{project}/.scryer/model.scry` IS the in-memory model.
 */

export const SCRY_VERSION = "0.3" as const;

/** One boundary-owning node whose code changed since the last reconcile, as
 *  returned by the `get_drift_status` Tauri command (mirrors the Rust
 *  `scryer_core::drift::DriftScope`). Drives the per-node drift banner. */
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

// --- Responsibilities & code-level data --------------------------------------

export interface Responsibility {
  id: string;
  /** Verb-led business statement of accountability. No mechanism words. */
  statement: string;
  /** Discovered in code with no upstream commitment (drift). The user adopts
   *  it (clear the flag) or rejects it (delete it). */
  vagrant?: boolean;
  /** Drift observation: the semantic check judged the code no longer
   *  discharges this claim. A flag awaiting a verdict (re-implement, reword,
   *  or drop) — the status is the prescription and stays untouched. */
  stale?: boolean;
  /** Drift's proposed correction for a stale claim: the statement that would
   *  match what the code now does, set when the behaviour diverged rather than
   *  vanished. Surfaced as an accept-or-edit reword next to re-implement/drop;
   *  a localized hint that never enters the plan diff. Cleared with `stale`. */
  staleProposal?: string;
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
  /** Drift adoption marker, the property-level twin of {@link Responsibility.vagrant}:
   *  `flag_drift` found a declared field no property described. Renders as a drift
   *  mark (Q), not a plan add (A); awaits adopt/reject. Mirrors Rust `SchemaProperty.vagrant`. */
  vagrant?: boolean;
  /** Drift regression marker, the twin of {@link Responsibility.stale}: the field
   *  backing this property is gone or changed. Awaits drop/reimplement. */
  stale?: boolean;
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

// --- Appearance (the look of a UI component) ---------------------------------

/** The render-artifact lifecycle of a visual component's look — its own axis,
 *  independent of the model→code plan diff. Mirrors the Rust `RenderState`. */
export type RenderState = "proposed" | "implemented" | "changed";

export interface Appearance {
  status?: RenderState;
  distPath?: string;
  builtAt?: number;
  sourceHash?: string;
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
  /** Drift adoption marker: this node was MINTED by a drift check to home
   *  code-discovered behaviour no existing node described — it lives in the PLAN
   *  only, awaiting a verdict. Like a vagrant responsibility ("code already does
   *  this, adopt?"), NOT planned intent ("implement this!"): it reads as a drift
   *  mark (Q), not a plan add (A). Mirrors Rust `Node.vagrant`. */
  vagrant?: boolean;
  /** Drift regression marker (mirror of `vagrant`): this whole node's backing
   *  code is GONE but the model still asserts it. Set on the PLAN node by a drift
   *  check when a deleted file/folder leaves the node codeless; reads as a drift
   *  mark (X). The user re-implements the subtree or drops it. Mirrors Rust
   *  `Node.stale`. */
  stale?: boolean;
  responsibilities?: Responsibility[];
  /** Field declarations, when this symbol defines a data shape. */
  properties?: SchemaProperty[];
  /** Optional lucide-react icon name override (frontend-only). */
  icon?: string;
  visual?: boolean;
  appearance?: Appearance;
  /** User-authored freeform notes — self-context and traversal aids, distinct
   *  from `description` (what the node IS) and a responsibility's `directives`.
   *  No spec/conformance role. Supports `[[node-id]]` wikilinks. User-only:
   *  hidden from the agent's write tools. Mirrors Rust `Node.notes`. */
  notes?: string;
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

/** The anchors visible in the working view. Code-side mapping has a single
 *  home: the committed model owns every committed element's anchor; the plan
 *  overlays anchors only for the elements it *adds* (not yet committed, so they
 *  live in the draft until they fold in). Merging committed under the plan gives
 *  the effective map for display without the draft having to mirror committed —
 *  see the dedup invariant. Display-only: never written back to the plan. */
export function effectiveSourceMap(
  committed: ScryModel | null,
  working: ScryModel,
): Record<string, SourceLocation[]> {
  return { ...(committed?.sourceMap ?? {}), ...(working.sourceMap ?? {}) };
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

/** Whether a responsibility's truth-bearing content differs — used both for
 *  lastTouchedAt stamping and for highlighting external (agent) edits. */
export function respTruthChanged(a: Responsibility, b: Responsibility): boolean {
  return (
    a.statement !== b.statement ||
    a.vagrant !== b.vagrant ||
    a.stale !== b.stale ||
    !sameDirectives(a.directives, b.directives)
  );
}

function propTruthChanged(a: SchemaProperty, b: SchemaProperty): boolean {
  return a.label !== b.label || a.description !== b.description;
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

/** Rewrite `[[old]]` / `[[old|label]]` wikilink targets to the new name in one
 *  text. Returns the input unchanged when nothing matches. */
function rewriteWikilinkText(text: string, oldName: string, newName: string): string {
  const target = oldName.trim().toLowerCase();
  return text.replace(
    /\[\[([^\][|]+?)(\|[^\][]+?)?\]\]/g,
    (whole, name: string, label: string | undefined) =>
      name.trim().toLowerCase() === target ? `[[${newName}${label ?? ""}]]` : whole,
  );
}

/** After a node rename, repoint every `[[Old Name]]` prose mention (node and
 *  group descriptions, responsibility statements, directives, node notes) at the
 *  new name so wikilinks never dangle. */
export function rewriteWikilinks(
  model: ScryModel,
  oldName: string,
  newName: string,
): ScryModel {
  if (!oldName.trim() || !newName.trim() || oldName === newName) return model;
  const fix = (t: string) => rewriteWikilinkText(t, oldName, newName);
  const fixResps = (resps: Responsibility[] | undefined) =>
    resps?.map((r) => ({
      ...r,
      statement: fix(r.statement),
      directives: r.directives?.map(fix),
    }));
  return {
    ...model,
    nodes: model.nodes.map((n) => ({
      ...n,
      description: n.description ? fix(n.description) : n.description,
      responsibilities: fixResps(n.responsibilities),
      notes: n.notes ? fix(n.notes) : n.notes,
    })),
    groups: model.groups.map((g) => ({
      ...g,
      description: g.description ? fix(g.description) : g.description,
      responsibilities: fixResps(g.responsibilities),
    })),
  };
}

export function updateNode(
  model: ScryModel,
  nodeId: string,
  patch: Partial<Node>,
): ScryModel {
  const prev = model.nodes.find((n) => n.id === nodeId);
  const next = {
    ...model,
    nodes: model.nodes.map((n) => (n.id === nodeId ? { ...n, ...patch } : n)),
  };
  // A rename repoints prose mentions everywhere — wikilinks must not dangle.
  if (prev && patch.name !== undefined && patch.name !== prev.name) {
    return rewriteWikilinks(next, prev.name, patch.name);
  }
  return next;
}

/**
 * Re-parent a node — its whole subtree moves with it. Mirrors the MCP
 * `move_nodes` validation: the new parent must satisfy the kind hierarchy
 * (system→container→component→symbol; null only for top-level systems/
 * persons), must not be external, and must not sit inside the moved node's own
 * subtree. The node leaves any group at its old level (groups organize
 * siblings). Returns the model unchanged when the move is invalid.
 */
export function moveNode(
  model: ScryModel,
  nodeId: string,
  newParentId: string | null,
): ScryModel {
  const node = model.nodes.find((n) => n.id === nodeId);
  if (!node) return model;

  if (newParentId === null) {
    if (node.kind !== "system" && node.kind !== "person") return model;
  } else {
    const parent = model.nodes.find((n) => n.id === newParentId);
    if (!parent || parent.external) return model;
    const valid =
      (parent.kind === "system" && node.kind === "container") ||
      (parent.kind === "container" && node.kind === "component") ||
      (parent.kind === "component" && node.kind === "symbol");
    if (!valid) return model;
    // The new parent must not be the node itself or inside its subtree.
    let cur: string | undefined = newParentId;
    const seen = new Set<string>();
    while (cur) {
      if (cur === nodeId) return model;
      if (seen.has(cur)) break;
      seen.add(cur);
      cur = model.nodes.find((n) => n.id === cur)?.parentId;
    }
  }

  return {
    ...model,
    nodes: model.nodes.map((n) =>
      n.id === nodeId ? { ...n, parentId: newParentId ?? undefined } : n,
    ),
    groups: model.groups.map((g) =>
      g.memberIds.includes(nodeId)
        ? { ...g, memberIds: g.memberIds.filter((m) => m !== nodeId) }
        : g,
    ),
  };
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

/** Add a new node. Returns `{model, id}`. */
export function addNode(
  model: ScryModel,
  init: {
    kind: Kind;
    name: string;
    parentId?: string;
    groupId?: string;
    external?: boolean;
  },
): { model: ScryModel; id: string } {
  const id = nextNodeId(model);
  const node: Node = {
    id,
    kind: init.kind,
    name: init.name,
    parentId: init.parentId,
    external: init.external || undefined,
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

// --- Links ---------------------------------------------------------------------

/**
 * Declare a link — links are a C4 model primitive, directed and same-level.
 * Minting one from an unmodeled import-evidence candidate is the main UI path.
 * Duplicate (src,dst) pairs are a no-op (returns the existing link's id).
 */
export function addLink(
  model: ScryModel,
  src: string,
  dst: string,
  label: string = "",
): { model: ScryModel; id: string } {
  const existing = model.links.find((l) => l.src === src && l.dst === dst);
  if (existing) return { model, id: existing.id };
  const id = nextLinkId(model);
  return {
    model: { ...model, links: [...model.links, { id, src, dst, label }] },
    id,
  };
}

export function removeLink(model: ScryModel, linkId: string): ScryModel {
  return { ...model, links: model.links.filter((l) => l.id !== linkId) };
}

/** Patch a declared link's label and/or protocol. An empty `method` clears it
 *  back to undefined (the field is optional). */
export function updateLink(
  model: ScryModel,
  linkId: string,
  patch: { label?: string; method?: string },
): ScryModel {
  return {
    ...model,
    links: model.links.map((l) =>
      l.id === linkId
        ? {
            ...l,
            ...(patch.label !== undefined ? { label: patch.label } : {}),
            ...(patch.method !== undefined
              ? { method: patch.method || undefined }
              : {}),
          }
        : l,
    ),
  };
}

// --- Add / remove groups -----------------------------------------------------

/** Add a new group. Members start empty. */
export function addGroup(
  model: ScryModel,
  init: {
    name: string;
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
  // A new claim is a plan until code backs it — the diff shows it as `added`.
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
 * Move a responsibility from one node to another. The claim keeps its id and is
 * reparented onto the destination node; the plan diff matches it by id and
 * renders the move as `moved` (R). No ghost/locked copy at the source — the diff
 * is the record of the relocation, so the claim's lifecycle is unchanged.
 */
export function moveResponsibility(
  model: ScryModel,
  fromNodeId: string,
  toNodeId: string,
  respId: string,
): ScryModel {
  const sourceResps = getResponsibilities(model, "node", fromNodeId);
  const resp = sourceResps.find((r) => r.id === respId);
  if (!resp) return model;

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
    [...getResponsibilities(next, "node", toNodeId), resp],
  );
  return next;
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
        properties: [
          ...existing,
          { label, description: description ?? "" },
        ],
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

