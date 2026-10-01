/**
 * Which red edge has its violation reasons open. Only a click on a red edge
 * opens one; clicking it again, or clicking anything else, closes it.
 */
export function nextOpenEdge(
  current: string | null,
  clicked: { id: string; violations?: readonly string[] },
): string | null {
  if (!clicked.violations?.length) return null;
  return current === clicked.id ? null : clicked.id;
}

/** Flip one layer's description open or closed (its ⓘ was clicked). */
export function toggleOpen(open: ReadonlySet<number>, i: number): ReadonlySet<number> {
  const next = new Set(open);
  if (next.has(i)) next.delete(i);
  else next.add(i);
  return next;
}
