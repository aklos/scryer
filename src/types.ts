import type { Node, Edge } from "@xyflow/react";

export type C4Kind = "person" | "system" | "container" | "component" | "operation" | "process" | "model";

export type C4Shape = "rectangle" | "person" | "cylinder" | "pipe" | "trapezoid" | "bucket" | "hexagon";

export type Status = "implemented" | "proposed" | "changed" | "deprecated";

export interface Contract {
  expect: string[];
  ask: string[];
  never: string[];
}

export interface Attachment {
  id: string;
  filename: string;
  mimeType: string;
  data: string; // base64
}

export type C4NodeData = {
  name: string;
  description: string;
  kind: C4Kind;
  technology?: string;
  external?: boolean;
  expanded?: boolean;
  shape?: C4Shape;
  sources?: { pattern: string; comment: string }[];
  status?: Status;
  contract?: Contract;
  accepts?: string[];
  decisions?: string;
  properties?: ModelProperty[];
  attachments?: Attachment[];
  _reference?: boolean;
  _relationships?: { direction: "in" | "out"; label: string; method?: string }[];
  _operations?: { id: string; name: string }[];
  [key: string]: unknown;
};

export type C4EdgeData = {
  label: string;
  method?: string;
  [key: string]: unknown;
};

export type C4Node = Node<C4NodeData>;
export type C4Edge = Edge<C4EdgeData>;

export type SourceLocation = {
  file: string;
  line?: number;
  endLine?: number;
};

export interface ModelProperty {
  label: string;
  description: string;
}

export type GroupKind = "deployment" | "package";

export interface Group {
  id: string;
  kind: GroupKind;
  name: string;
  description?: string;
  memberIds: string[];
}

export interface FlowBranch {
  condition: string;
  steps: FlowStep[];
}

export interface FlowStep {
  id: string;
  description?: string;
  processIds?: string[];
  branches?: FlowBranch[];
}

export interface FlowTransition {
  source: string;
  target: string;
  label?: string;
}

export interface Flow {
  id: string;
  name: string;
  description?: string;
  steps: FlowStep[];
  transitions?: FlowTransition[];
}

export interface C4ModelData {
  nodes: C4Node[];
  edges: C4Edge[];
  startingLevel?: StartingLevel;
  sourceMap?: Record<string, SourceLocation[]>;
  projectPath?: string;
  refPositions?: Record<string, { x: number; y: number }>;
  groups?: Group[];
  contract?: Contract;
  flows?: Flow[];
}

export type StartingLevel = "system" | "container" | "component";

// Advisor hints
export interface Hint {
  nodeId: string;
  message: string;
  severity: "info" | "warning";
  action?: HintAction;
}

export type HintAction =
  | { type: "setShape"; shape: string }
  | { type: "setExternal"; value: boolean };
