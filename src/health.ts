/**
 * Observability report — mirrors the Rust types behind the `get_model_health`
 * Tauri command (`scryer_core::health`, `scryer_extract::anchors`,
 * `scryer_core::build_edges`). Everything here is derived and read-only: the
 * UI renders it, never writes it.
 */

/** Responsibility/property statuses in a scope. Same 4-status prescription as
 *  `statusColors.Status`; flags (vagrant/stale) are counted separately. */
export interface StatusCounts {
  proposed: number;
  implemented: number;
  verified: number;
  changed: number;
}

/** Health counters over one scope — a node's own content, or a whole subtree. */
export interface HealthCounts {
  responsibilities: number;
  properties: number;
  statuses: StatusCounts;
  /** Responsibilities flagged vagrant (undescribed behaviour awaiting adopt/reject). */
  vagrant: number;
  /** Responsibilities flagged stale (drift verdict awaiting a decision). */
  stale: number;
  /** Claims expected to read through to code (implemented/verified/changed on a leaf). */
  anchorable: number;
  /** Of those, how many actually have a source anchor. */
  anchored: number;
  /** anchorable − anchored — the lens's blind spots. */
  unmapped: number;
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

export interface DerivedGraph {
  linkAudit: LinkAudit[];
  unmodeled: DerivedEdge[];
}

export interface ModelHealthReport {
  health: ModelHealth;
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

/** linkId → import-edge count, for annotating connections. */
export function linkEvidence(report: ModelHealthReport | null): Record<string, number> {
  const out: Record<string, number> = {};
  for (const a of report?.derived.linkAudit ?? []) out[a.linkId] = a.edgeCount;
  return out;
}
