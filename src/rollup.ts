/**
 * The `empty` flag — the one derived node signal the diff-era UI still needs.
 * (The old status roll-up is gone: the page reads as a plan↔model diff now, so
 * a single rolled-up lifecycle status no longer drives anything.)
 */

import type { Node } from "./viewmodel";

/**
 * The `empty` flag — a SYMBOL that carries no semantic content of its own: no
 * responsibilities, no properties, no rendered appearance, and not external.
 *
 * Empty is a FLAG, not a status: it sits beside the proposed→implemented→
 * verified lifecycle, never inside it. `proposed` means planned work; `empty`
 * means the node justifies nothing yet and must either gain a business
 * responsibility or be removed. Derived, never stored.
 *
 * Scoped to symbols — components/containers/systems are structural and carry
 * their meaning through their children, so an own-responsibility-less parent is
 * not "empty" in this sense.
 */
export function isNodeEmpty(node: Node): boolean {
  if (node.kind !== "symbol" || node.external) return false;
  const hasContent =
    (node.responsibilities?.length ?? 0) > 0 ||
    (node.properties?.length ?? 0) > 0 ||
    !!node.appearance?.status;
  return !hasContent;
}
