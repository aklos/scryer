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
      responsibilities?: Responsibility[];
      properties?: SchemaProperty[];
      /** User-authored freeform notes (gutter). `""`/undefined clears it. */
      notes?: string;
      /** User-authored node-level directives (HOW-constraints). Replaces the
       *  whole array; `[]`/undefined clears it. */
      directives?: string[];
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
  /** Patch a link's label and/or protocol. An empty `method` clears it. */
  updateLink: (linkId: string, patch: { label?: string; method?: string }) => void;
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
  /** Rename a concern everywhere — the registry entry and every responsibility
   *  tagged with it, in one step (the registry entry IS the concept). */
  renameConcern: (from: string, to: string) => void;
  /** Adopt a code-discovered (vagrant) responsibility: clear its flag AND fold it
   *  straight into the committed model. Unlike other edits this commits, because
   *  the claim is anchored to code that already exists — there is nothing to
   *  implement. Backend-driven; the file watcher refreshes both layers. */
  adoptResponsibility: (respId: string) => void;
  /** Reject a code-discovered (vagrant) responsibility: the behaviour should not
   *  be in the model. Backend folds it (and any minted host chain) into the
   *  committed model then drops it from the plan, turning it into a deletion work
   *  item (`toDelete`) anchored to the code to remove — so the rejection sticks
   *  and drift stops re-proposing it. Backend-driven; the watcher refreshes. */
  rejectResponsibility: (respId: string) => void;
  /** Verdict on a STALE responsibility (the model asserts it, the code stopped
   *  doing it) — the take-model mirror of adopt/reject. `drop`: the code is right
   *  (removed on purpose) → delete the claim from both layers. `reimplement`: the
   *  model is right (code regressed) → remove from committed so it reads as an
   *  `Added` to-do the agent rebuilds. Backend-driven; the watcher refreshes. */
  dropResponsibility: (respId: string) => void;
  reimplementResponsibility: (respId: string) => void;
  /** Accept (or edit) drift's proposed reword for a STALE responsibility: the
   *  code diverged rather than vanished, so the new wording — already true of the
   *  code — lands in both layers and the stale flag clears, with no work item.
   *  Backend-driven; the watcher refreshes. */
  rewordResponsibility: (respId: string, statement: string) => void;
  /** Verdict on a STALE node — the whole subtree's backing code is gone. The
   *  node-level mirror of drop/reimplement: `dropNode` removes the node and every
   *  descendant from both layers; `reimplementNode` keeps the subtree in the plan
   *  as a rebuild to-do while removing it from the committed model. */
  dropNode: (nodeId: string) => void;
  reimplementNode: (nodeId: string) => void;
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
  /** Verdict on a code-discovered (vagrant) PROPERTY — the property-level twin of
   *  {@link adoptResponsibility}/{@link rejectResponsibility}. `adopt`: the field
   *  already exists, so fold it into the committed model. `reject`: fold then drop
   *  from the plan as a deletion work item. Addressed by (node, label) since
   *  properties have no id. Backend-driven; the watcher refreshes both layers. */
  adoptProperty: (nodeId: string, label: string) => void;
  rejectProperty: (nodeId: string, label: string) => void;
  /** Verdict on a STALE property (its backing field is gone or changed) — the
   *  take-model mirror. `drop`: the code is right → remove from both layers.
   *  `reimplement`: the model is right → remove from committed so it reads as an
   *  `Added` to-do. Backend-driven; the watcher refreshes. */
  dropProperty: (nodeId: string, label: string) => void;
  reimplementProperty: (nodeId: string, label: string) => void;
}
