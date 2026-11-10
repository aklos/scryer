/**
 * Which move takes the view from where it is to where an agent pointed: into a card, out to the card that held
 * the level, a hop between places neither holds, or a morph between a node's card and its page.
 */

import type { ScryModel } from "../../entities/model/viewmodel";

export type View = "diagram" | "wiki";

/** Where the view is, or is going. */
export interface Place {
  view: View;
  /** The map level: whose children it shows, null for the top. */
  focus: string | null;
  /** The node whose wiki page is open; null on any other page. */
  page: string | null;
}

export type MoveKind = "none" | "push" | "pull" | "hop" | "morph";

export interface Move {
  kind: MoveKind;
  /** The card the move goes through: entered (push), returned to (pull), or one with the page (morph). */
  via?: string;
}

function parentOf(model: ScryModel, id: string): string | null {
  return model.nodes.find((n) => n.id === id)?.parentId ?? null;
}

/** `id` and its ancestors, nearest first. */
function chain(model: ScryModel, id: string | null): string[] {
  const out: string[] = [];
  const seen = new Set<string>();
  for (let cur = id; cur && !seen.has(cur); cur = parentOf(model, cur)) {
    seen.add(cur);
    out.push(cur);
  }
  return out;
}

/** Whether level `outer` holds `inner` (strictly); the top level (null) holds every node. */
function holds(model: ScryModel, outer: string | null, inner: string | null): boolean {
  if (inner === null || outer === inner) return false;
  return outer === null || chain(model, inner).includes(outer);
}

/** The node on level `level` that is `id` or holds it. */
function onLevel(model: ScryModel, level: string | null, id: string): string | undefined {
  return chain(model, id).find((n) => parentOf(model, n) === level);
}

/** The place showing `node` in `view`: the map frames it among its siblings, the wiki opens its page. */
export function placeOf(model: ScryModel, node: string, view: View): Place {
  return { view, focus: parentOf(model, node), page: node };
}

export function movement(model: ScryModel, from: Place, to: Place): Move {
  if (from.view === "diagram" && to.view === "diagram") {
    if (from.focus === to.focus) return { kind: "none" };
    if (holds(model, from.focus, to.focus)) return { kind: "push", via: onLevel(model, from.focus, to.focus!) };
    if (holds(model, to.focus, from.focus)) return { kind: "pull", via: onLevel(model, to.focus, from.focus!) };
    return { kind: "hop" };
  }
  if (from.view === "wiki" && to.view === "wiki") {
    if (!from.page || !to.page) return { kind: "hop" };
    if (from.page === to.page) return { kind: "none" };
    if (holds(model, from.page, to.page)) return { kind: "push" };
    if (holds(model, to.page, from.page)) return { kind: "pull" };
    return { kind: "hop" };
  }
  if (from.view === "diagram" && to.page) {
    // The card that opens into the page: the node itself, or the card on this level that holds it.
    const via = onLevel(model, from.focus, to.page);
    return via ? { kind: "morph", via } : { kind: "hop" };
  }
  // Wiki to map: the page folds into its own card, which the new level always shows.
  return to.page ? { kind: "morph", via: to.page } : { kind: "hop" };
}
