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
