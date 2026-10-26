/**
 * Kind iconography and type tag — the C4 abstraction read at a glance.
 * `kindIcon` gives each kind a distinct silhouette (person, external system,
 * previewable component, data shape, code symbol, or the C4 box family by
 * altitude); `typeTag` is the bracketed type line under the name, its textual
 * fallback. A user-set `node.icon` override still wins, resolved by the caller
 * via `lookupIcon`.
 */

import type { ComponentType } from "react";
import type { LucideProps } from "lucide-react";
import { Box, Boxes, Cloud, Code, Component, Database, Eye, Layers, User } from "lucide-react";
import type { Node } from "./viewmodel";
import { isDataShape } from "./viewmodel";

/** The minimal node shape these helpers read. */
type IconNode = Pick<Node, "kind" | "external" | "technology" | "properties" | "responsibilities">;

/** The kind silhouette for a node. Person → bust, external → cloud, previewable
 *  symbol → eye, data-shape symbol → database, code symbol → braces, else the
 *  C4 box family by altitude. `previewable` is DERIVED by the caller from the
 *  preview sidecar's export list — it is never a fact stored on the node. */
/** A symbol's glyph by what it is — the same ones `kindIcon` gives it in the
 *  tree: plain code, a data shape, a component the sidecar can render. */
export const SYMBOL_ICON: Record<"code" | "model" | "visual", ComponentType<LucideProps>> = {
  code: Code,
  model: Database,
  visual: Eye,
};

export function kindIcon(node: IconNode, previewable = false): ComponentType<LucideProps> {
  if (node.kind === "person") return User;          // an actor — human, regardless of external
  if (node.external) return Cloud;                  // the world: a third-party service
  if (previewable) return Eye;                      // a component the sidecar can render
  if (isDataShape(node)) return Database;           // a symbol that defines a data shape
  if (node.kind === "symbol") return Code;          // code-level leaf
  switch (node.kind) {
    case "system": return Boxes;                    // a software system (a set of containers)
    case "container": return Box;                   // a deployable/runnable unit
    case "component": return Component;             // a grouping of code
    default: return Layers;
  }
}

export interface TypeTag {
  /** The C4 type word(s): "Person", "External System", "Container", … */
  type: string;
  /** The technology, when the node carries one and it's meaningful to show. */
  tech?: string;
}

/** The bracketed type-tag line shown under the name — the textual fallback for
 *  the kind silhouette. Technology is surfaced for everything that has one
 *  except persons (and symbols, whose shape is the data/code itself). */
export function typeTag(node: IconNode): TypeTag {
  const tech =
    node.kind !== "person" && node.kind !== "symbol" && node.technology
      ? node.technology
      : undefined;

  if (node.kind === "person") return { type: "Person" };
  if (node.external) return { type: "External System", tech };
  switch (node.kind) {
    case "system": return { type: "Software System", tech };
    case "container": return { type: "Container", tech };
    case "component": return { type: "Component", tech };
    case "symbol": return { type: isDataShape(node) ? "Data shape" : "Code" };
    default: return { type: "Node", tech };
  }
}
