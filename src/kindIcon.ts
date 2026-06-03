/**
 * Kind iconography + type tag — the C4 abstraction read at a glance.
 *
 * The card icon used to be `tokenIcon(node.id)` — a hash of the id into a random
 * glyph, which carried no type meaning (a Person and an external system got
 * unrelated icons). Type is now SEEN: a distinct silhouette per C4 kind, with the
 * bracketed type tag as the quiet textual fallback underneath. A user-set
 * `node.icon` override still wins (resolved by the caller via `lookupIcon`).
 */

import type { ComponentType } from "react";
import type { LucideProps } from "lucide-react";
import { Box, Boxes, Cloud, Code, Component, Database, Layers, User } from "lucide-react";
import type { Node } from "./viewmodel";
import { isDataShape } from "./viewmodel";

/** The minimal node shape these helpers read — works for both `Node` and the
 *  derived `NodeView` (which extends it). */
type IconNode = Pick<Node, "kind" | "external" | "technology" | "properties" | "responsibilities">;

/** The kind silhouette for a node. Person → bust, external → cloud, data-shape
 *  symbol → database, code symbol → braces, else the C4 box family by altitude. */
export function kindIcon(node: IconNode): ComponentType<LucideProps> {
  if (node.kind === "person") return User;          // an actor — human, regardless of external
  if (node.external) return Cloud;                  // the world: a third-party service
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
