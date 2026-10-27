/** Open one layer's description (its ⓘ was clicked), closing any other; a
 *  second click on the same ⓘ closes it. */
export function toggleOpen(open: ReadonlySet<number>, i: number): ReadonlySet<number> {
  return open.has(i) ? new Set() : new Set([i]);
}

/** What marks a layer's ⓘ and its open description: a press inside one is
 *  not a click off. */
export const LAYER_NOTE = "data-layer-note";

/** Whether a press landed off every layer's ⓘ and description, so an open
 *  description should close. */
export function clickedOff(target: { closest(selector: string): unknown } | null): boolean {
  return !target?.closest(`[${LAYER_NOTE}]`);
}
