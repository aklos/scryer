/**
 * Architectural styles on the frontend — the mirror of Rust `style.rs`.
 *
 * A style is the horizontal axis of the model: a fixed layer list, a legality
 * matrix over it, and the drawing the map renders it in. Every container
 * declares one; every component carries one of its layers. The frontend
 * never decides legality (the core and MCP gates do); it reads the table to
 * lay out the map, badge the tree and the page, and explain a layer.
 *
 * `builtin.json` is the checked-in copy of `Styles::builtin()`, pinned by the
 * Rust lockstep test. The health report ships the project's full table
 * (custom styles included); {@link styleTable} prefers it when present.
 */

import builtin from "./builtin.json";
import type { ModelHealthReport } from "../health";
import type { Node, ScryModel } from "../viewmodel";

export type Drawing = "hexagon" | "rows" | "rings" | "columns";
export type Isolation = "strict" | "inclusive";

export interface LayerDef {
  name: string;
  description: string;
}

/** Mirrors Rust `StyleDef` (camelCase serde). */
export interface StyleDef {
  name: string;
  description: string;
  /** Display order: outermost first for layered styles, first stage first for pipelines. */
  layers: LayerDef[];
  /** layer → the layers it may depend on. */
  matrix: Record<string, string[]>;
  isolation: Isolation;
  /** Layers a cross-container link may land on, outermost first. */
  inbound: string[];
  /** Layers that may reach out of the container (the driven side). */
  outbound?: string[];
  publicSurface: string[];
  externalBans?: Record<string, string[]>;
  path: { dirs?: Record<string, string[]>; markers?: Record<string, string[]> };
  drawing: Drawing;
}

export const builtinStyles: readonly StyleDef[] = builtin as unknown as StyleDef[];

/** The style table to render with: the report's (project-complete) when it
 *  has one, else the built-ins. Keyed by name. */
export function styleTable(report: ModelHealthReport | null): ReadonlyMap<string, StyleDef> {
  const defs = report?.styles?.length ? report.styles : builtinStyles;
  return new Map(defs.map((d) => [d.name, d]));
}

/** The style name governing a node: the nearest container-or-component at or
 *  above it that declares one (a component may override its container).
 *  Mirrors Rust `governing_style`. */
export function governingStyle(model: ScryModel, nodeId: string): string | undefined {
  const byId = new Map(model.nodes.map((n) => [n.id, n]));
  let cur = byId.get(nodeId);
  const seen = new Set<string>();
  while (cur && !seen.has(cur.id)) {
    seen.add(cur.id);
    if ((cur.kind === "container" || cur.kind === "component") && cur.style) return cur.style;
    if (cur.kind === "container") return undefined;
    cur = cur.parentId ? byId.get(cur.parentId) : undefined;
  }
  return undefined;
}

/** The layer a node carries: a component's own, a symbol's inherited from its
 *  component. Mirrors Rust `layer_of`. */
export function layerOf(model: ScryModel, nodeId: string): string | undefined {
  const byId = new Map(model.nodes.map((n) => [n.id, n]));
  const n = byId.get(nodeId);
  if (!n) return undefined;
  if (n.kind === "component") return n.layer;
  if (n.kind === "symbol" && n.parentId) {
    const p = byId.get(n.parentId);
    return p?.kind === "component" ? p.layer : undefined;
  }
  return undefined;
}

/** The style definition governing a node, if any. */
export function governingStyleDef(
  model: ScryModel,
  nodeId: string,
  table: ReadonlyMap<string, StyleDef>,
): StyleDef | undefined {
  const name = governingStyle(model, nodeId);
  return name ? table.get(name) : undefined;
}

/** "may import: application, domain" — the one line a reader needs about a
 *  layer. Empty when the style says nothing. */
export function allowedImports(def: StyleDef, layer: string): string[] {
  return def.matrix[layer] ?? [];
}

/** Display index of a layer in its style (0 = first / outermost), or -1. */
export function layerIndex(def: StyleDef, layer: string): number {
  return def.layers.findIndex((l) => l.name === layer);
}

/** The innermost layer of a style — the one everything may depend on and
 *  that depends on nothing else (last in display order for layered styles). */
export function innermostLayer(def: StyleDef): string | undefined {
  return def.layers[def.layers.length - 1]?.name;
}

/** Nodes that can carry a style: containers, and components overriding. */
export function canCarryStyle(node: Node): boolean {
  return node.kind === "container" || node.kind === "component";
}
