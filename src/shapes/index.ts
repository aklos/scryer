import type { C4Kind, C4Shape } from "../types";

export { ShapeBackground } from "./ShapeBackground";
export { ShapeIcon } from "./ShapeIcon";

const KIND_DEFAULTS: Record<C4Kind, C4Shape> = {
  person: "person",
  system: "rectangle",
  container: "rectangle",
  component: "rectangle",
  operation: "rectangle",
  process: "rectangle",
  model: "rectangle",
};

export function resolveShape(kind: C4Kind, shapeOverride?: C4Shape): C4Shape {
  return shapeOverride ?? KIND_DEFAULTS[kind];
}

export function defaultShapeForKind(kind: C4Kind): C4Shape {
  return KIND_DEFAULTS[kind];
}

export interface ContentInsets {
  top: number;
  bottom: number;
  left: number;
  right: number;
}

const DEFAULT_INSETS: ContentInsets = { top: 6, bottom: 6, left: 8, right: 8 };

const INSETS: Record<C4Shape, ContentInsets> = {
  "rectangle": DEFAULT_INSETS,
  "person": DEFAULT_INSETS,
  "cylinder": DEFAULT_INSETS,
  "pipe": DEFAULT_INSETS,
  "trapezoid": DEFAULT_INSETS,
  "bucket": DEFAULT_INSETS,
  "hexagon": DEFAULT_INSETS,
};

export function getContentInsets(shape: C4Shape): ContentInsets {
  return INSETS[shape];
}

export const ALL_SHAPES: C4Shape[] = [
  "rectangle",
  "person",
  "cylinder",
  "pipe",
  "trapezoid",
  "bucket",
  "hexagon",
];

export const SHAPE_LABELS: Record<C4Shape, string> = {
  "rectangle": "Rect",
  "person": "Person",
  "cylinder": "Database",
  "pipe": "Bus",
  "trapezoid": "Trapezoid",
  "bucket": "Bucket",
  "hexagon": "Hexagon",
};
