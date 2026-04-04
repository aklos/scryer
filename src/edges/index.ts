import { createContext } from "react";
import { RelationshipEdge } from "./RelationshipEdge";

export const edgeTypes = {
  default: RelationshipEdge,
};

export const StraightEdgesContext = createContext(false);
