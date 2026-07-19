/**
 * Observability report — mirrors the Rust types behind the `get_model_health`
 * Tauri command (`scryer_core::health`, `scryer_extract::anchors`,
 * `scryer_core::build_edges`). Everything here is derived and read-only: the
 * UI renders it, never writes it.
 */

import type { ScryModel, Link } from "./viewmodel";

/** Health counters over one scope — a node's own content, or a whole subtree. */
export interface HealthCounts {
  responsibilities: number;
  properties: number;
  /** Responsibilities flagged vagrant (undescribed behaviour awaiting adopt/reject). */
  vagrant: number;
  /** Responsibilities flagged stale (drift verdict awaiting a decision). */
  stale: number;
  /** Claims expected to read through to code (any committed claim on a leaf). */
  anchorable: number;
  /** Of those, how many actually have a source anchor. */
  anchored: number;
  /** anchorable − anchored — the lens's blind spots. */
  unmapped: number;
  /** Claims carrying a backing test (a verify entry). A separate dimension
   *  from `anchored` — implemented vs. demonstrated — and not gated on
   *  leafness (a structural claim backed by an integration test counts). */
  verified: number;
  /** Claims in a When/While/If form on code-backed hosts — a concrete trigger,
   *  state, or failure a test can demonstrate mechanically. Classified
   *  deterministically from the leading keyword (rule 21). */
  testable: number;
  /** Of the testable claims, how many carry no verify entry — demonstrable
   *  claims nothing demonstrates. */
  untested: number;
  /** Unix seconds of the most recent truth-bearing edit in scope. */
  lastTouchedAt?: number;
}

/** How much of a node's owned code region the lens actually reaches. */
export interface BoundaryCoverage {
  totalFiles: number;
  anchoredFiles: number;
  /** Files in the boundary no anchor reads into — code the lens cannot see. */
  darkFiles: string[];
}

export interface NodeHealth {
  own: HealthCounts;
  subtree: HealthCounts;
  boundary?: BoundaryCoverage;
}

export interface ModelHealth {
  nodes: Record<string, NodeHealth>;
  totals: HealthCounts;
  /** Architecture nodes (symbols exempt) that no relationship link names as
   *  source or target — edgeless on every diagram, easy to miss. Node ids,
   *  sorted. Absent/empty when there is nothing to flag. */
  disconnected?: string[];
}

/** Fingerprint observation: one source anchor whose code no longer matches
 *  what the model last reconciled against. Scoping for a re-check, never a
 *  verdict. */
export type AnchorState = "changed" | "broken" | "fileMissing";

export interface AnchorObservation {
  /** sourceMap key — responsibility id, or node id for a data-shape anchor. */
  key: string;
  hostId: string;
  hostName: string;
  file: string;
  symbol?: string;
  state: AnchorState;
}

/** Evidence rating of one declared model link: how many import edges cross
 *  from the src subtree into the dst subtree. 0 = asserted-only. */
export interface LinkAudit {
  linkId: string;
  edgeCount: number;
}

/** A candidate link: sibling nodes the code connects but no declared link
 *  covers. `count` = underlying symbol→symbol import edges. */
export interface DerivedEdge {
  src: string;
  dst: string;
  count: number;
}

/** One leaf code edge, both ends resolved to their host node + the symbol that
 *  anchored them, deduped with a `count`. The per-symbol detail behind the
 *  aggregate `linkAudit` / `unmodeled` counts — what a declared link expands
 *  into, and what a node's "implied connections" are attributed from. */
export interface ResolvedEdge {
  srcNode: string;
  srcSymbol: string;
  dstNode: string;
  dstSymbol: string;
  count: number;
}

export interface DerivedGraph {
  linkAudit: LinkAudit[];
  unmodeled: DerivedEdge[];
  resolvedEdges: ResolvedEdge[];
}

/** A node's build completeness — how much of its AUTHORED subtree (committed +
 *  planned) reads through to real code. Distinct from HealthCounts (a lens over
 *  the committed model): completeness is defined from greenfield onward, the
 *  denominator being intent. The unit is the anchorable primitive — a node's
 *  boundary box (counted only when its glob owns a real file), a leaf
 *  responsibility, or a data shape. A structural node's own responsibilities are
 *  not primitives (they discharge through the subtree). */
export interface Completeness {
  /** Anchored primitives in the subtree. */
  anchored: number;
  /** Authored primitives in the subtree — the denominator. */
  total: number;
  /** Leaf primitives (responsibilities + data shapes) in the subtree. When 0 the
   *  node is unmeasured (a bare box) and `pct` is absent. */
  leafTotal: number;
  /** Rounded 0–100 percent, or absent ("—") when there is nothing to measure. */
  pct?: number;
}

/** The glanceable form of a node's completeness: `label` is the % ("42%"), or
 *  "—" when unmeasured (a bare box with no leaf primitives). `grounded` = the
 *  node has at least one anchored primitive (its box or a leaf reads through to
 *  real code) — the anchorage signal. Returns null when there is nothing to say
 *  (no primitives at all), so callers can skip the badge entirely. */
export interface CompletenessBadge {
  label: string;
  grounded: boolean;
  measured: boolean;
}

export function completenessBadge(
  c: Completeness | undefined,
): CompletenessBadge | null {
  if (!c || c.total === 0) return null;
  const grounded = c.anchored > 0;
  if (c.pct === undefined) return { label: "—", grounded, measured: false };
  return { label: `${c.pct}%`, grounded, measured: true };
}

export interface ModelHealthReport {
  health: ModelHealth;
  /** Per-node build completeness, keyed by node id. */
  completeness: Record<string, Completeness>;
  /** Anchors whose code changed/broke since the last reconcile. */
  anchors: AnchorObservation[];
  /** Anchors silently healed this pass (symbol moved, content unchanged). */
  reanchored: number;
  derived: DerivedGraph;
}

export const ANCHOR_STATE_LABEL: Record<AnchorState, string> = {
  changed: "code changed",
  broken: "symbol gone",
  fileMissing: "file gone",
};

/** Per-claim state of the BACKING TEST's fingerprint, from the verify-namespaced
 *  anchor observations (`verify:{respId}`). A claim absent here has an intact
 *  (or not-yet-fingerprinted) test link. broken/fileMissing outrank changed. */
export function verifyStatesOf(
  report: ModelHealthReport | null,
): Record<string, AnchorState> {
  const out: Record<string, AnchorState> = {};
  for (const o of report?.anchors ?? []) {
    if (!o.key.startsWith("verify:")) continue;
    const id = o.key.slice("verify:".length);
    if (out[id] === undefined || o.state !== "changed") out[id] = o.state;
  }
  return out;
}

/** Fold anchor observations that share host + file + symbol + state into one
 *  row each. The key omits `key` (the responsibility/node id) precisely because
 *  that's the only thing that differs between the duplicates we're collapsing. */
export function collapseAnchors(observations: AnchorObservation[]): AnchorObservation[] {
  const byKey = new Map<string, AnchorObservation>();
  for (const a of observations) {
    const k = `${a.hostId}\0${a.file}\0${a.symbol ?? ""}\0${a.state}`;
    if (!byKey.has(k)) byKey.set(k, a);
  }
  return [...byKey.values()];
}

/** linkId → import-edge count, for annotating connections. */
export function linkEvidence(report: ModelHealthReport | null): Record<string, number> {
  const out: Record<string, number> = {};
  for (const a of report?.derived.linkAudit ?? []) out[a.linkId] = a.edgeCount;
  return out;
}

// --- Dark code: files under a boundary that no claim reads into ---------------

/** One boundary node's blind spots — the files it owns that no anchor in its
 *  subtree reaches. */
export interface DarkBoundary {
  nodeId: string;
  files: string[];
}

/** Every dark file in the model, grouped by the boundary node that owns it.
 *  Boundary ownership is most-specific (see `BoundaryOwnership` in core), so each
 *  file is dark under exactly one node — the groups partition the dark set with
 *  no double-counting, and `total` is the project-wide dark-file count. Sorted by
 *  dark-file count, descending. */
export function darkBoundaries(report: ModelHealthReport | null): {
  groups: DarkBoundary[];
  total: number;
} {
  const groups: DarkBoundary[] = [];
  let total = 0;
  for (const [nodeId, h] of Object.entries(report?.health.nodes ?? {})) {
    const files = h.boundary?.darkFiles ?? [];
    if (files.length === 0) continue;
    groups.push({ nodeId, files });
    total += files.length;
  }
  groups.sort((a, b) => b.files.length - a.files.length);
  return { groups, total };
}

// --- Implied connections: the per-symbol detail behind the aggregates --------

/** One derived (code-only) connection of a node, with the peer rolled up to its
 *  nearest architectural (non-symbol) node and the underlying leaf edges summed.
 *  `dir` is from this node's point of view: `out` = this node's code reaches the
 *  peer, `in` = the peer's code reaches this node. */
export interface ImpliedConn {
  /** Peer rolled up to its nearest non-symbol ancestor — the architectural node
   *  the relationship reads against ("Health Engine", not a bare symbol). */
  peerId: string;
  dir: "out" | "in";
  /** Underlying leaf import edges, summed across the peer's subtree. */
  count: number;
}

/** One underlying code path of a declared link — a single `(node, symbol)` →
 *  `(node, symbol)` edge inside the src→dst subtrees the link spans. */
export interface LinkPath {
  srcId: string;
  srcSymbol: string;
  dstId: string;
  dstSymbol: string;
  count: number;
}

/** Walk from `nodeId` up the parent chain; true if `ancestorId` is on it
 *  (inclusive of the node itself). Cycle-guarded. */
function isWithin(
  parentOf: Map<string, string | undefined>,
  nodeId: string,
  ancestorId: string,
): boolean {
  let cur: string | undefined = nodeId;
  const seen = new Set<string>();
  while (cur && !seen.has(cur)) {
    if (cur === ancestorId) return true;
    seen.add(cur);
    cur = parentOf.get(cur);
  }
  return false;
}

/** One resolved leaf code edge that crosses this node's subtree boundary,
 *  tagged with the direction (from this node's POV) and the peer rolled up to
 *  its arch node. The shared substrate for `impliedFor` (aggregate counts) and
 *  `impliedPaths` (the ladder behind one row) — both walk these, so a row's
 *  `×count` always equals the sum of its expanded leaves. */
export interface ImpliedLeaf {
  edge: ResolvedEdge;
  dir: "out" | "in";
  /** Peer rolled up to its nearest non-symbol ancestor. */
  peerId: string;
}

/** Every leaf code edge that crosses out of `nodeId`'s subtree to a peer
 *  outside it, with the peer rolled up to its nearest non-symbol ancestor.
 *  Edges already covered by a **declared** link from/to this node (the peer
 *  sitting inside that link's subtree) are dropped — those belong to the
 *  declared link and show as its expansion, not here. Same-parent siblings and
 *  containment are excluded — only genuine cross-LEVEL reach. */
export function impliedLeaves(
  report: ModelHealthReport | null,
  model: ScryModel,
  nodeId: string,
): ImpliedLeaf[] {
  const edges = report?.derived.resolvedEdges ?? [];
  if (edges.length === 0) return [];
  const parentOf = new Map(model.nodes.map((n) => [n.id, n.parentId] as const));
  const kindOf = new Map(model.nodes.map((n) => [n.id, n.kind] as const));
  const inSelf = (id: string) => isWithin(parentOf, id, nodeId);
  // Roll a node up to its nearest non-symbol ancestor (inclusive) — the level a
  // relationship reads at. Cycle-guarded; falls back to the node itself.
  const arch = (id: string): string => {
    let cur: string | undefined = id;
    const seen = new Set<string>();
    while (cur && !seen.has(cur)) {
      if (kindOf.get(cur) !== "symbol") return cur;
      seen.add(cur);
      cur = parentOf.get(cur);
    }
    return id;
  };
  // Peer subtrees this node already declares a link into / out of — covered.
  const declaredOut = model.links.filter((l) => l.src === nodeId).map((l) => l.dst);
  const declaredIn = model.links.filter((l) => l.dst === nodeId).map((l) => l.src);
  const covered = (peerLeaf: string, roots: string[]) =>
    roots.some((r) => isWithin(parentOf, peerLeaf, r));

  const selfParent = parentOf.get(nodeId);
  // Roll the peer leaf up to its arch node, or null if this isn't a genuine
  // cross-LEVEL reach: skip containment (one node inside the other) and
  // same-parent siblings — a sibling code-connection is a candidate *same-level
  // link* ("Suggested by the code"), not an implied relationship.
  const peerOf = (peerLeaf: string): string | null => {
    const peerId = arch(peerLeaf);
    if (isWithin(parentOf, peerId, nodeId) || isWithin(parentOf, nodeId, peerId)) return null;
    if (parentOf.get(peerId) === selfParent) return null;
    return peerId;
  };

  const leaves: ImpliedLeaf[] = [];
  for (const edge of edges) {
    const srcIn = inSelf(edge.srcNode);
    const dstIn = inSelf(edge.dstNode);
    if (srcIn === dstIn) continue; // internal to the subtree, or unrelated
    if (srcIn && !covered(edge.dstNode, declaredOut)) {
      const peerId = peerOf(edge.dstNode);
      if (peerId) leaves.push({ edge, dir: "out", peerId });
    } else if (dstIn && !covered(edge.srcNode, declaredIn)) {
      const peerId = peerOf(edge.srcNode);
      if (peerId) leaves.push({ edge, dir: "in", peerId });
    }
  }
  return leaves;
}

/** The derived connections of a node's whole **subtree** — the leaf edges of
 *  {@link impliedLeaves} aggregated by (direction, peer) with counts summed. So
 *  a component page aggregates all its symbols' cross-boundary edges, a
 *  container page all its components', a symbol page just its own: the implied
 *  connection carries to every level, and reads architecturally ("used by Health
 *  Engine", not twelve bare command symbols). Sorted by count, descending. */
export function impliedFor(
  report: ModelHealthReport | null,
  model: ScryModel,
  nodeId: string,
): ImpliedConn[] {
  const agg = new Map<string, ImpliedConn>();
  for (const { dir, peerId, edge } of impliedLeaves(report, model, nodeId)) {
    const key = `${dir}:${peerId}`;
    const cur = agg.get(key);
    if (cur) cur.count += edge.count;
    else agg.set(key, { peerId, dir, count: edge.count });
  }
  return [...agg.values()].sort((a, b) => b.count - a.count);
}

/** Expand one implied connection into the leaf code paths behind it — the same
 *  `(node, symbol) → (node, symbol)` rows a declared link's {@link pathsForLink}
 *  produces, so Implied Connections can render the identical path ladder. The
 *  edges keep their natural code direction (src→dst), which reads correctly in
 *  both directions: an `out` row shows this node's symbols reaching the peer's,
 *  an `in` row the reverse. Sorted by count. */
export function impliedPaths(
  report: ModelHealthReport | null,
  model: ScryModel,
  nodeId: string,
  conn: ImpliedConn,
): LinkPath[] {
  return impliedLeaves(report, model, nodeId)
    .filter((l) => l.dir === conn.dir && l.peerId === conn.peerId)
    .map(({ edge }) => ({
      srcId: edge.srcNode,
      srcSymbol: edge.srcSymbol,
      dstId: edge.dstNode,
      dstSymbol: edge.dstSymbol,
      count: edge.count,
    }))
    .sort((a, b) => b.count - a.count);
}

/** Expand a declared link into the leaf code paths that back it: every resolved
 *  edge whose src is inside `link.src`'s subtree and whose dst is inside
 *  `link.dst`'s subtree. Empty when the link is asserted-only. Sorted by count. */
export function pathsForLink(
  report: ModelHealthReport | null,
  model: ScryModel,
  link: Link,
): LinkPath[] {
  const edges = report?.derived.resolvedEdges ?? [];
  if (edges.length === 0) return [];
  const parentOf = new Map(model.nodes.map((n) => [n.id, n.parentId] as const));
  return edges
    .filter(
      (e) =>
        isWithin(parentOf, e.srcNode, link.src) &&
        isWithin(parentOf, e.dstNode, link.dst),
    )
    .map((e) => ({
      srcId: e.srcNode,
      srcSymbol: e.srcSymbol,
      dstId: e.dstNode,
      dstSymbol: e.dstSymbol,
      count: e.count,
    }))
    .sort((a, b) => b.count - a.count);
}
