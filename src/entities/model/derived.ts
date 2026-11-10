/**
 * The derived tree's status as `get_derived_status` and the health report
 * return it: every claim's state (proven, stale, unproven, failing, planned) and probe
 * mark, and the health counts per node and in total.
 */

export type ClaimState = "proven" | "stale" | "unproven" | "failing" | "planned";
export type ProbeMark = "crosshair" | "doubleCheck";

export interface DerivedCounts {
  claims: number;
  proven: number;
  stale: number;
  unproven: number;
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
  /** The wording the claim last passed as, when it now reads otherwise: its
   *  test was reworded since. */
  provenAs?: string;
}

export interface DerivedNodeStatus {
  own: DerivedCounts;
  subtree: DerivedCounts;
  probe?: ProbeMark;
  darkFiles?: string[];
  /** Concern → lines of this node's own files that only claims tagged with it
   *  from elsewhere run: the concern is served here, its claim stated
   *  elsewhere. Present only with current per-test coverage. */
  concernGaps?: Record<string, number>;
}

export interface DerivedStatus {
  claims: DerivedClaimStatus[];
  nodes: Record<string, DerivedNodeStatus>;
  totals: DerivedCounts;
}
