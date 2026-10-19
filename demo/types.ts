/**
 * The derived shapes the app reads, as scryer-core serialises them
 * (`derived_tree`, `derived_status`, the session log, structure.toml). They
 * live here, not in `src/entities`, because the app's own TS types for them
 * land with step 4; when they do, the demo imports those and this file goes.
 * Field names follow the Rust serde (camelCase); fields the film never reads
 * are left out.
 */

export type Kind = "person" | "system" | "container" | "component" | "symbol";

export interface Responsibility {
  id: string;
  statement: string;
  concern?: string;
}

export interface Node {
  id: string;
  kind: Kind;
  name: string;
  parentId?: string;
  external?: boolean;
  technology?: string;
  description?: string;
  responsibilities: Responsibility[];
  style?: string;
  layer?: string;
}

export interface Link {
  id: string;
  src: string;
  dst: string;
  label: string;
  kind?: "depends" | "uses" | "calls";
}

export interface SourceLocation {
  pattern: string;
  symbol?: string;
  line?: number;
  endLine?: number;
}

export interface ConcernDef {
  slug: string;
  description?: string;
}

export interface ScryModel {
  version: string;
  nodes: Node[];
  links: Link[];
  testMap?: Record<string, SourceLocation[]>;
  boundaries?: Record<string, { pattern: string }[]>;
  concerns?: ConcernDef[];
}

export type ClaimLevel = "system" | "container" | "component";

/** One behaviour test read as a claim. */
export interface DerivedClaim {
  id: string;
  nodeId: string;
  level: ClaimLevel;
  file: string;
  line: number;
  identifier: string;
  /** The claim text, concern tags stripped. */
  description: string;
  concerns?: string[];
  junitName: string;
  /** A todo stub: written, not yet implemented. */
  planned?: boolean;
}

export interface DerivedViolation {
  nodeId: string;
  file: string;
  kind: string;
  detail: string;
}

export interface DerivedTree {
  fingerprint: number;
  committed: ScryModel;
  planned: ScryModel;
  claims: DerivedClaim[];
  fileNodes: Record<string, string>;
  darkFiles: string[];
  styleViolations: DerivedViolation[];
}

export type ClaimState = "proven" | "stale" | "failing" | "planned";
export type ProbeMark = "crosshair" | "doubleCheck";

export interface ClaimStatus {
  respId: string;
  nodeId: string;
  state: ClaimState;
  probe?: ProbeMark;
}

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

export interface NodeStatus {
  own: DerivedCounts;
  subtree: DerivedCounts;
  probe?: ProbeMark;
  darkFiles?: string[];
}

export interface DerivedStatus {
  claims: ClaimStatus[];
  nodes: Record<string, NodeStatus>;
  totals: DerivedCounts;
}

/** `.scryer/structure.toml`, as parsed. */
export interface Structure {
  system: { name?: string; persons: string[]; externals: string[] };
  levels: { e2e?: string[]; integration?: string[] };
  containers: Record<string, { name?: string; style?: string; layers?: string[] }>;
}

/** One line of a session log (`.scryer/sessions/<id>.jsonl`). */
export type SessionEvent =
  | { event: "task"; text: string }
  | { event: "touch"; file: string }
  | { event: "stub"; file: string; test: string }
  | { event: "reached"; ids: string[] }
  | { event: "stop" };

export type SessionEntry = { at: number } & SessionEvent;

export interface SessionLog {
  id: string;
  entries: SessionEntry[];
}

/** Diagram placements per node, as `.scryer/layout` persists them. */
export type Layout = Record<string, { x: number; y: number }>;

/** Everything the invoke stub serves. */
export interface Fixture {
  tree: DerivedTree;
  status: DerivedStatus;
  structure: Structure;
  sessions: SessionLog[];
  layout: Layout;
}
