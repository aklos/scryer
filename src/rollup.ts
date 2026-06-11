/**
 * Effective status — roll a node's responsibility statuses up to one card status.
 *
 * v0.3 simplification: roll-up is local to the node (its own responsibilities).
 * The "fulfills" cross-surface mechanism from the prototype is gone — child
 * decomposition implicitly discharges parent responsibilities, but there's no
 * explicit mapping between them yet.
 *
 * Externals are out of scope: their work isn't ours to ship. They don't
 * carry status; callers should render them in a neutral hue.
 */

import type { Node } from "./viewmodel";
import type { Status } from "./statusColors";
import { rollupStatus } from "./statusColors";

const UNSET: Status = "proposed";

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

export function effectiveNodeStatus(node: Node): Status | null {
  if (node.external) return null;
  // An empty symbol has no work to show — suppress the misleading `proposed`
  // pill; the `empty` flag (isNodeEmpty) is surfaced separately in its place.
  if (isNodeEmpty(node)) return null;
  // A symbol may carry responsibilities, properties (a data shape), a visual
  // preview, or any combination. Status rolls up over whatever it carries.
  const statuses: Status[] = [
    ...(node.responsibilities ?? []).map((r) => r.status ?? UNSET),
    ...(node.properties ?? []).map((p) => p.status ?? UNSET),
  ];
  if (node.appearance?.status) statuses.push(node.appearance.status);
  if (statuses.length === 0) return UNSET;
  return rollupStatus(statuses);
}

export function effectiveRespStatus(
  _node: Node,
  resp: { status?: Status },
): Status {
  return resp.status ?? UNSET;
}
