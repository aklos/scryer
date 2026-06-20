/**
 * Shape system for the diagram cards — ported from the pre-pivot canvas and
 * adapted to the v0.3 `Kind` ladder. v0.3 has no per-node shape override, so in
 * practice every kind resolves to `rectangle` except `person`; the richer
 * silhouettes remain available for future use.
 */

import type { Kind } from "../viewmodel";

export { ShapeBackground } from "./ShapeBackground";

export type C4Shape =
  | "rectangle"
  | "person"
  | "cylinder"
  | "pipe"
  | "trapezoid"
  | "bucket"
  | "hexagon";

const KIND_DEFAULTS: Record<Kind, C4Shape> = {
  person: "person",
  system: "rectangle",
  container: "rectangle",
  component: "rectangle",
  symbol: "rectangle",
};

export function resolveShape(kind: Kind, shapeOverride?: C4Shape): C4Shape {
  return shapeOverride ?? KIND_DEFAULTS[kind];
}

export interface ContentInsets {
  top: number;
  bottom: number;
  left: number;
  right: number;
}

const DEFAULT_INSETS: ContentInsets = { top: 6, bottom: 6, left: 8, right: 8 };

export function getContentInsets(_shape: C4Shape): ContentInsets {
  return DEFAULT_INSETS;
}
