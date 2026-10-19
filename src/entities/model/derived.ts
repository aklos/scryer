/**
 * The derived tree's status as `get_derived_status` and the health report
 * return it: every claim's state (proven, stale, failing, planned) and probe
 * mark, and the health counts per node and in total.
 */

export type ClaimState = "proven" | "stale" | "failing" | "planned";
export type ProbeMark = "crosshair" | "doubleCheck";

export interface DerivedCounts {
  claims: number;
  proven: number;
  stale: number;
  failing: number;
  planned: number;
  untestedComponents: number;
  survivingMutants: number;
  styleViolations: number;
  darkFiles: number;
}

export interface DerivedClaimStatus {
  respId: string;
  nodeId: string;
  state: ClaimState;
  probe?: ProbeMark;
  recordedAt?: number;
}

export interface DerivedNodeStatus {
  own: DerivedCounts;
  subtree: DerivedCounts;
  probe?: ProbeMark;
  darkFiles?: string[];
}

export interface DerivedStatus {
  claims: DerivedClaimStatus[];
  nodes: Record<string, DerivedNodeStatus>;
  totals: DerivedCounts;
}
