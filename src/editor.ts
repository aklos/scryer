/**
 * Editor callbacks — the contract between the model tree / node pages and App.
 * Pure intents; App owns the model and applies them via the helpers in
 * viewmodel.ts.
 */

import type { Kind, SchemaProperty, Responsibility } from "./viewmodel";

export interface Editor {
  // --- nodes ---
  updateNode: (
    nodeId: string,
    patch: {
      name?: string;
      kind?: Kind;
      description?: string;
      technology?: string;
      icon?: string;
      visual?: boolean;
      relocated?: boolean;
      responsibilities?: Responsibility[];
      properties?: SchemaProperty[];
    },
  ) => void;
  deleteNode: (nodeId: string) => void;
  addNode: (init: {
    kind: Kind;
    parentId?: string;
    groupId?: string;
    external?: boolean;
  }) => string;
  /** Re-parent a node (subtree moves with it); null = top-level. Kind
   *  hierarchy validated; no-op when the move is invalid. */
  moveNode: (nodeId: string, newParentId: string | null) => void;

  // --- links ---
  /** Declare a directed link (e.g. minted from an import-evidence suggestion).
   *  Duplicate (src,dst) pairs are a no-op. Returns the link id. */
  addLink: (src: string, dst: string, label?: string) => string;
  deleteLink: (linkId: string) => void;

  // --- groups ---
  updateGroup: (
    groupId: string,
    patch: {
      name?: string;
      description?: string;
      icon?: string;
      responsibilities?: Responsibility[];
    },
  ) => void;
  deleteGroup: (groupId: string) => void;
  addGroup: (init: {
    /** Level the group lives at (its members' shared parent). */
    parentNodeId: string | null;
    /** Members to enclose on creation (moved out of any prior group). */
    memberIds?: string[];
  }) => string;
  /** Move a node into a group, or out of any group when `groupId` is null. */
  setNodeGroup: (nodeId: string, groupId: string | null) => void;

  // --- responsibilities (on either a node or a group) ---
  addResponsibility: (host: "node" | "group", hostId: string) => string;
  updateResponsibility: (
    host: "node" | "group",
    hostId: string,
    respId: string,
    patch: Partial<Responsibility>,
  ) => void;
  removeResponsibility: (
    host: "node" | "group",
    hostId: string,
    respId: string,
  ) => void;
  // --- responsibility relocation ---
  moveResponsibility: (
    fromNodeId: string,
    toNodeId: string,
    respId: string,
  ) => void;

  // --- properties (schema-kind nodes only) ---
  addProperty: (nodeId: string) => void;
  updateProperty: (
    nodeId: string,
    index: number,
    patch: Partial<SchemaProperty>,
  ) => void;
  removeProperty: (nodeId: string, index: number) => void;
}
