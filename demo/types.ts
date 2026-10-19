/**
 * The fixture's shapes. The model and the claim status are the app's own
 * types; what the app has no TS type for — the derived tree's claim list as
 * the backend holds it, structure.toml, the session log — is mirrored here
 * from the Rust serde (camelCase), fields the film never reads left out.
 */

import type { ScryModel } from "../src/entities/model/viewmodel";
import type { DerivedStatus } from "../src/entities/model/derived";

export type { Kind, Link, Node, ScryModel } from "../src/entities/model/viewmodel";
export type {
  ClaimState,
  DerivedClaimStatus as ClaimStatus,
  DerivedCounts,
  DerivedStatus,
  ProbeMark,
} from "../src/entities/model/derived";

export type ClaimLevel = "system" | "container" | "component";

/** One behaviour test read as a claim (Rust `DerivedClaim`). */
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

/** One style violation (Rust `DerivedViolation`). */
export interface DerivedViolation {
  nodeId: string;
  file: string;
  kind: string;
  detail: string;
}

/** The derived tree (Rust `DerivedTree`). */
export interface DerivedTree {
  fingerprint: number;
  committed: ScryModel;
  planned: ScryModel;
  claims: DerivedClaim[];
  fileNodes: Record<string, string>;
  darkFiles: string[];
  styleViolations: DerivedViolation[];
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
