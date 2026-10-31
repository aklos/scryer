/**
 * Editor callbacks — the contract between the node pages and whatever applies
 * edits. App wires in the derived editor (`features/derived-tree`), which
 * writes each edit where the code keeps it: a node's name, description and
 * decisions, a claim's wording, a new claim as a todo stub.
 */

import type { Claim } from "./viewmodel";

export interface Editor {
  updateNode: (
    nodeId: string,
    patch: {
      name?: string;
      description?: string;
      /** The decisions after the description, as markdown. `""` clears them. */
      decisions?: string;
      claims?: Claim[];
    },
  ) => void;
}
