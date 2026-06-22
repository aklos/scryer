/**
 * Easing curves for the film engine. Two house curves: `camEase` for the
 * virtual camera (smooth in, settle out — reads as a deliberate, weighted move)
 * and `cursorEase` for the synthetic cursor (decisive, decelerating into its
 * target, the way a hand lands on a control).
 */

export type Easing = (t: number) => number

export const linear: Easing = (t) => t;
export const easeOutCubic: Easing = (t) => 1 - Math.pow(1 - t, 3);
export const easeInOutCubic: Easing = (t) =>
  t < 0.5 ? 4 * t * t * t : 1 - Math.pow(-2 * t + 2, 3) / 2;

/** Camera: weighted ease-in-out. The Raycast "deliberate glide." */
export const camEase: Easing = easeInOutCubic;
/** Cursor: quick depart, soft landing. */
export const cursorEase: Easing = easeOutCubic;

export const lerp = (a: number, b: number, t: number): number => a + (b - a) * t;
