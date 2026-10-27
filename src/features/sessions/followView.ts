/**
 * Which view a followed session's change belongs in: the map when it
 * changed the shape around a node (added it, moved it, linked or unlinked
 * it), the wiki when it changed what the node says (its claims or its
 * description). Read off the tree before and after, at the one node the
 * session is working in, so other sessions' changes elsewhere never switch
 * the view.
 */

import type { ScryModel } from "../../entities/model/viewmodel";
import type { SessionStep } from "./useSessions";

export type FollowView = "diagram" | "wiki";

/** The view that shows how `next` differs from `prev` at `nodeId`, the map
 *  first when both changed; null when neither did. */
export function followView(prev: ScryModel, next: ScryModel, nodeId: string): FollowView | null {
  const before = prev.nodes.find((n) => n.id === nodeId);
  const after = next.nodes.find((n) => n.id === nodeId);
  if (!after) return null;
  const links = (m: ScryModel) =>
    m.links
      .filter((l) => l.src === nodeId || l.dst === nodeId)
      .map((l) => `${l.src}>${l.dst}`)
      .sort()
      .join(" ");
  if (!before || before.parentId !== after.parentId || links(prev) !== links(next)) return "diagram";
  const says = (n: typeof after) =>
    JSON.stringify([n.description ?? "", (n.responsibilities ?? []).map((r) => [r.id, r.statement])]);
  return says(before) !== says(after) ? "wiki" : null;
}

/** A step that is about a claim belongs in the wiki. */
export function stepView(step: SessionStep | undefined): FollowView | null {
  return step?.kind === "todo" || step?.kind === "verdict" ? "wiki" : null;
}
