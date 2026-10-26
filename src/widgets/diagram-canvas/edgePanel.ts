/** Flip one layer's description open or closed (its ⓘ was clicked). */
export function toggleOpen(open: ReadonlySet<number>, i: number): ReadonlySet<number> {
  const next = new Set(open);
  if (next.has(i)) next.delete(i);
  else next.add(i);
  return next;
}
