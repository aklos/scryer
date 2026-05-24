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

export function effectiveNodeStatus(node: Node): Status | null {
  if (node.external) return null;
  if (node.kind === "schema") {
    const properties = node.properties ?? [];
    if (properties.length === 0) return UNSET;
    return rollupStatus(properties.map((p) => p.status ?? UNSET));
  }
  const responsibilities = node.responsibilities ?? [];
  if (responsibilities.length === 0) return UNSET;
  return rollupStatus(responsibilities.map((r) => r.status ?? UNSET));
}

export function effectiveRespStatus(
  _node: Node,
  resp: { status?: Status },
): Status {
  return resp.status ?? UNSET;
}
