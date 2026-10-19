/**
 * The film's motion, as pure functions of the frame: things settle in (a
 * short rise and a fade, eased out), text sets itself, counts tick. Slow and
 * confident — nothing overshoots, nothing loops.
 */

import type { CSSProperties } from "react";
import { Easing, interpolate } from "remotion";

export const FPS = 30;
export const s = (seconds: number) => Math.round(seconds * FPS);

const settleEase = Easing.bezier(0.16, 1, 0.3, 1);
const moveEase = Easing.bezier(0.65, 0, 0.35, 1);

/** 0→1 between `at` and `at + dur` frames, eased. `move` eases both ends
 *  (for things travelling); the default eases out (for things arriving). */
export function prog(f: number, at: number, dur: number, kind: "settle" | "move" = "settle"): number {
  if (dur <= 0) return f >= at ? 1 : 0;
  const t = interpolate(f, [at, at + dur], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp" });
  return (kind === "move" ? moveEase : settleEase)(t);
}

/** Settle in: rise `dy` px and fade in over `dur` frames from `at`. */
export function settle(f: number, at: number, dur = s(0.6), dy = 16): CSSProperties {
  const p = prog(f, at, dur);
  return { opacity: p, transform: `translateY(${(1 - p) * dy}px)` };
}

/** Fade out over `dur` frames from `at`. */
export function fadeOut(f: number, at: number, dur = s(0.3)): number {
  return 1 - prog(f, at, dur);
}

/** How many characters of a line are typed by frame `f`, typing from `at`. */
export function typedChars(f: number, at: number, cps = 42): number {
  return Math.max(0, Math.floor(((f - at) / FPS) * cps));
}

/** A count ticking from `a` to `b` over `dur` frames from `at`. */
export function tick(f: number, a: number, b: number, at: number, dur: number): number {
  return Math.round(a + (b - a) * prog(f, at, dur, "move"));
}

/** How many of a list's items are out, one every `every` frames from `at`. */
export function revealed(f: number, at: number, every: number, total: number): number {
  return Math.max(0, Math.min(total, Math.floor((f - at) / every) + 1));
}
