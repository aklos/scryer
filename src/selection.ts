/**
 * Canvas selection. Two grains, nested: selecting a responsibility implies its
 * owning node is the context (`nodeId`), with the responsibility as the focus.
 * Drives the read-side inspector panel.
 */
export type Selection =
  | { kind: "node"; nodeId: string }
  | { kind: "responsibility"; nodeId: string; respId: string }
  | null;
