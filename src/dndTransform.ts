/**
 * Tiny local replacement for `@dnd-kit/utilities`' `CSS.Transform.toString`.
 * That package is not installed and we may not add dependencies, so we inline
 * the one helper we need: serialize a sortable transform to a CSS string.
 */

export interface DndTransform {
  x: number;
  y: number;
  scaleX: number;
  scaleY: number;
}

/** Serialize a dnd-kit transform to a CSS `transform` value, or undefined. */
export function transformToCss(t: DndTransform | null | undefined): string | undefined {
  if (!t) return undefined;
  return `translate3d(${t.x}px, ${t.y}px, 0) scaleX(${t.scaleX}) scaleY(${t.scaleY})`;
}
