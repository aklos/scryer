/**
 * Pure SVG geometry for each shape. All functions take (w, h) content box.
 *
 * Every node has a sharp base rect (w x h) that is always filled.
 * Shape decorations extend OUTSIDE that rect — never inside it.
 */

/** Sharp-cornered base rectangle. Used as the filled content area for every node. */
export function baseRectPath(w: number, h: number): string {
  return `M0,0 H${w} V${h} H0 Z`;
}

/** Rectangle shape — same as base rect, nothing extra. */
export function rectanglePath(w: number, h: number): string {
  return baseRectPath(w, h);
}

/** Person: single unified path — rect body + shoulders + head arc, all one outline. */
export function personPath(w: number, h: number): string {
  const cx = w / 2;
  const neckHalf = 12;
  const neckY = -10;
  const headR = 16;
  const neckRight = cx + neckHalf;
  const neckLeft = cx - neckHalf;

  return (
    `M0,${h}` +
    ` H${w}` +
    ` V0` +
    ` C${w},${neckY * 0.5} ${neckRight + 8},${neckY} ${neckRight},${neckY}` +
    ` A${headR},${headR} 0 1,0 ${neckLeft},${neckY}` +
    ` C${neckLeft - 8},${neckY} 0,${neckY * 0.5} 0,0` +
    ` Z`
  );
}

export interface CylinderParts {
  bodyPath: string;
  topCapPath: string;
  capRy: number;
}

/** Cylinder: elliptical caps extend above and below the base rect. */
export function cylinderParts(w: number, h: number): CylinderParts {
  const capRy = 16;
  const rx = w / 2;

  // Body: straight sides, elliptical top and bottom edges extending outside
  const bodyPath =
    `M0,0` +
    ` V${h}` +
    ` A${rx},${capRy} 0 0,0 ${w},${h}` +
    ` V0` +
    ` A${rx},${capRy} 0 0,0 0,0 Z`;

  // Top cap: full ellipse at y=0 (visible "lid")
  const topCapPath =
    `M0,0` +
    ` A${rx},${capRy} 0 0,1 ${w},0` +
    ` A${rx},${capRy} 0 0,1 0,0 Z`;

  return { bodyPath, topCapPath, capRy };
}

export interface PipeParts {
  bodyPath: string;
  rightCapPath: string;
  capRx: number;
}

/** Pipe: elliptical caps extend left and right of the base rect. */
export function pipeParts(w: number, h: number): PipeParts {
  const capRx = 16;
  const ry = h / 2;

  // Body: straight top/bottom, elliptical left and right edges extending outside
  const bodyPath =
    `M0,0` +
    ` H${w}` +
    ` A${capRx},${ry} 0 0,1 ${w},${h}` +
    ` H0` +
    ` A${capRx},${ry} 0 0,1 0,0 Z`;

  // Right cap: full ellipse at x=w (visible "lid")
  const rightCapPath =
    `M${w},0` +
    ` A${capRx},${ry} 0 0,1 ${w},${h}` +
    ` A${capRx},${ry} 0 0,1 ${w},0 Z`;

  return { bodyPath, rightCapPath, capRx };
}

/** Trapezoid: angled sides extend outside the base rect at the bottom. */
export function trapezoidPath(w: number, h: number): string {
  const extend = 24;
  return (
    `M0,0 H${w}` +
    ` L${w + extend},${h}` +
    ` H${-extend}` +
    ` Z`
  );
}

export interface BucketParts {
  bodyPath: string;
  bottomCapPath: string;
  capRy: number;
}

/** Bucket: wider at top (trapezoid sides narrowing down) + elliptical bottom cap. */
export function bucketParts(w: number, h: number): BucketParts {
  const capRy = 16;
  const extend = 24;

  // Body: wide top, sides narrow toward base rect bottom, elliptical bottom edge
  const bodyPath =
    `M${-extend},0 H${w + extend}` +
    ` L${w},${h}` +
    ` A${w / 2},${capRy} 0 0,1 0,${h}` +
    ` Z`;

  // Bottom cap: full ellipse at y=h (visible rounded bottom)
  const bottomCapPath =
    `M0,${h}` +
    ` A${w / 2},${capRy} 0 0,0 ${w},${h}` +
    ` A${w / 2},${capRy} 0 0,0 0,${h} Z`;

  return { bodyPath, bottomCapPath, capRy };
}

/** Hexagon: pointed sides extend outside the base rect at the midpoint. */
export function hexagonPath(w: number, h: number): string {
  const extend = 24;
  return (
    `M0,0 H${w}` +
    ` L${w + extend},${h / 2}` +
    ` L${w},${h}` +
    ` H0` +
    ` L${-extend},${h / 2} Z`
  );
}
