import type { Node, Edge } from "@xyflow/react";

export type C4Kind = "person" | "system" | "container" | "component" | "operation" | "process" | "model";

export type C4Shape = "rectangle" | "person" | "cylinder" | "pipe" | "trapezoid" | "bucket" | "hexagon";

export type Status = "proposed" | "implemented" | "verified" | "vagrant";

export interface ContractImage {
  filename: string;
  mimeType: string;
  data: string; // base64
}

export type ContractItem = { text: string; passed?: boolean; url?: string; image?: ContractImage } | string;

export interface Contract {
  expect: ContractItem[];
  ask: ContractItem[];
  never: ContractItem[];
}

/** Extract text from a ContractItem (handles both plain string and object format) */
export function contractText(item: ContractItem): string {
  return typeof item === "string" ? item : item.text;
}

/** Extract passed state from a ContractItem */
export function contractPassed(item: ContractItem): boolean | undefined {
  return typeof item === "string" ? undefined : item.passed;
}

/** Extract url from a ContractItem */
export function contractUrl(item: ContractItem): string | undefined {
  return typeof item === "string" ? undefined : item.url;
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
  statusReason?: string;
  contract?: Contract;
  notes?: string[];
  properties?: ModelProperty[];
  _reference?: boolean;
  _relationships?: { direction: "in" | "out"; label: string; method?: string }[];
  _operations?: { id: string; name: string }[];
  [key: string]: unknown;
};

export type C4EdgeData = {
  label: string;
  method?: string;
  _route?: { x: number; y: number }[];
  [key: string]: unknown;
};

export type C4Node = Node<C4NodeData>;
export type C4Edge = Edge<C4EdgeData>;

export type SourceLocation = {
  pattern: string;
  line?: number;
  endLine?: number;
  command?: string;
};

export interface ModelProperty {
  label: string;
  description: string;
}

export interface Group {
  id: string;
  name: string;
  description?: string;
  memberIds: string[];
  parentGroupId?: string;
  contract?: Contract;
}

export interface FlowBranch {
  condition: string;
  steps: FlowStep[];
}

export interface FlowStep {
  id: string;
  description?: string;
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

// AI coding tool integration state
export type AiToolsState = {
  claude: boolean;
  codex: boolean;
  claudeMcpEnabled: boolean;
  codexMcpEnabled: boolean;
  claudeReadApproved: boolean;
};
