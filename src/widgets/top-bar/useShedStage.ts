/**
 * How many pieces of words a bar sheds to fit its width. Stage 0 shows
 * everything; each stage past it drops one more, in the order the bar decides.
 * The bar renders at a stage and, if its content overflows, the width it
 * needed is remembered and the next stage renders — before paint, so no
 * overflowing frame ever shows. Growing back reads those widths in reverse: a
 * stage returns once the bar is as wide as that stage needed. Measured, not
 * guessed from viewport breakpoints, because what fits depends on the project
 * name, the branch and the changes label, not the window alone.
 */

import { useLayoutEffect, useRef, useState, type RefObject } from "react";
import { flushSync } from "react-dom";

/** Overflowing by more than scrollWidth's rounding: at a fractional window
 *  width, scrollWidth rounds up past a clientWidth that rounds down. */
const overflows = (el: HTMLElement) => el.scrollWidth > el.clientWidth + 1;

/** The lowest stage the bar can return to at `width`, given the width each
 *  stage below `stage` was measured to need (undefined: never measured). */
export function stageForWidth(need: readonly (number | undefined)[], stage: number, width: number): number {
  let s = stage;
  while (s > 0 && need[s - 1] !== undefined && need[s - 1]! <= width) s--;
  return s;
}

/** The stage `ref`'s element renders at, 0..max. `contentKey` names what the
 *  bar shows; when it changes, the measured widths are stale and shedding
 *  starts over from stage 0. The element must not shrink with its content —
 *  only its own width (the window) decides how much room there is. */
export function useShedStage(ref: RefObject<HTMLElement | null>, max: number, contentKey: string): number {
  const [stage, setStage] = useState(0);
  const [key, setKey] = useState(contentKey);
  const need = useRef<number[]>([]);
  const current = useRef(stage);
  current.current = stage;

  if (key !== contentKey) {
    setKey(contentKey);
    setStage(0);
    need.current = [];
  }

  // After every render: overflowing, shed one more.
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el || stage >= max || !overflows(el)) return;
    need.current[stage] = el.scrollWidth;
    setStage(stage + 1);
  });

  // The window resized: shed if it now overflows, take words back if it is
  // as wide as a lower stage needed. flushSync lands it before paint.
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const ro = new ResizeObserver(() => {
      const s = current.current;
      let next: number;
      if (overflows(el) && s < max) {
        need.current[s] = el.scrollWidth;
        next = s + 1;
      } else {
        next = stageForWidth(need.current, s, el.clientWidth);
      }
      if (next !== s) flushSync(() => setStage(next));
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, [ref, max]);

  return stage;
}
