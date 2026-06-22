/**
 * The lowest layer of the engine: a single rAF-driven tween and a cancellable
 * wait. Everything animated (camera, cursor) drives `style.transform` straight
 * off these — no React re-render per frame, so motion stays at display rate.
 */

import type { Easing } from "./easing";

export interface TweenHandle {
  /** Resolves when the tween finishes — or immediately if cancelled. */
  done: Promise<void>;
  cancel(): void;
}

/** Run `onUpdate(eased)` each frame for `duration` ms, then resolve. */
export function tween(
  duration: number,
  easing: Easing,
  onUpdate: (eased: number) => void,
): TweenHandle {
  let raf = 0;
  let cancelled = false;
  let resolve!: () => void;
  const done = new Promise<void>((r) => (resolve = r));

  if (duration <= 0) {
    onUpdate(1);
    resolve();
    return { done, cancel() {} };
  }

  const start = performance.now();
  const step = (now: number) => {
    if (cancelled) return;
    const t = Math.min(1, (now - start) / duration);
    onUpdate(easing(t));
    if (t < 1) raf = requestAnimationFrame(step);
    else resolve();
  };
  raf = requestAnimationFrame(step);

  return {
    done,
    cancel() {
      if (cancelled) return;
      cancelled = true;
      cancelAnimationFrame(raf);
      resolve();
    },
  };
}

/** A cancellable hold. */
export function wait(ms: number): TweenHandle {
  let resolve!: () => void;
  const done = new Promise<void>((r) => (resolve = r));
  const id = window.setTimeout(resolve, ms);
  return {
    done,
    cancel() {
      clearTimeout(id);
      resolve();
    },
  };
}
