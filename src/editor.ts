/**
 * Editor callbacks — the contract between Surface/EntryCard/GroupOverlay and
 * App. Pure intents; App owns the model and applies them via the helpers in
 * viewmodel.ts.
 */

import type { Cell, Kind, ModelProperty, Responsibility } from "./viewmodel";

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
      deprecated?: boolean;
      relocated?: boolean;
      responsibilities?: Responsibility[];
      properties?: ModelProperty[];
    },
  ) => void;
  deleteNode: (nodeId: string) => void;
  addNode: (init: {
    kind: Kind;
    parentId?: string;
    cell?: Cell;
    groupId?: string;
  }) => string;

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
    /** Parent surface for placement. Members start empty. */
    parentNodeId: string | null;
    cell?: Cell;
  }) => string;

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

  // --- properties (model-kind nodes only) ---
  addProperty: (nodeId: string) => void;
  updateProperty: (
    nodeId: string,
    index: number,
    patch: Partial<ModelProperty>,
  ) => void;
  removeProperty: (nodeId: string, index: number) => void;
}
