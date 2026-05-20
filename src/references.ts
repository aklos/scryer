/**
 * Reference resolution — the C4 "context" elements that surround the current
 * surface. A reference is a card link-connected to something inside this
 * surface's subtree but living outside it.
 *
 * The result is classified by role so the surface can route each bucket to
 * its own visual slot:
 *
 *   - `persons`     — actors (perimeter, top)
 *   - `externals`   — out-of-scope systems/containers (grid, dashed styling)
 *   - `refs`        — in-scope siblings/ancestors (perimeter, sides, dimmed)
 *
 * Persons and externals propagate across every altitude — a component that
 * talks to an external service shows that service in its own context, not
 * only on the system surface.
 */

import type { Altitude, Surface, Entry } from "./viewmodel";
import { allCards } from "./rollup";

type Surfaces = Record<string, Surface>;

/**
 * A perimeter context entry, tagged with:
 *   - `altitude`: the level of its home surface (drives which ring band it
 *     lives in).
 *   - `direction`: how it relates to our in-scope content — `incoming` if it
 *     calls into us, `outgoing` if we call out to it, `both` if both.
 *
 * The renderer uses these to place each entry: altitude picks the band,
 * direction picks the side within the band (left = incoming, right = outgoing).
 */
export type LinkDirection = "incoming" | "outgoing" | "both";

export interface ContextEntry {
  entry: Entry;
  altitude: Altitude;
  direction: LinkDirection;
}

export interface SurfaceContext {
  persons: ContextEntry[];
  externals: ContextEntry[];
  refs: ContextEntry[];
}

/** A link pointing *into* a target entry — used for rendering incoming pills
 * on cards whose source can't render its own (e.g. persons). */
export interface IncomingLink {
  from: Entry;
  label: string;
}

/** Find a card by id anywhere in the model. */
export function findCard(surfaces: Surfaces, id: string): Entry | null {
  for (const s of Object.values(surfaces)) {
    const c = allCards(s).find((c) => c.id === id);
    if (c) return c;
  }
  return null;
}

/**
 * Every card in `surface`'s subtree — the surface itself plus every surface
 * reachable through `childSurfaceId`.
 */
function subtreeCardIds(surfaces: Surfaces, surface: Surface): Set<string> {
  const ids = new Set<string>();
  const visited = new Set<string>([surface.id]);
  const stack: Surface[] = [surface];
  while (stack.length) {
    const s = stack.pop()!;
    for (const c of allCards(s)) {
      ids.add(c.id);
      if (c.childSurfaceId && !visited.has(c.childSurfaceId)) {
        const child = surfaces[c.childSurfaceId];
        if (child) {
          visited.add(c.childSurfaceId);
          stack.push(child);
        }
      }
    }
  }
  return ids;
}

/**
 * Every ancestor card of `surface` — i.e. every card whose `childSurfaceId`
 * chain leads to this surface. Used so context propagates down: anything
 * linking to a parent system still surrounds the containers / components
 * inside that system.
 */
function ancestorCardIds(surfaces: Surfaces, surface: Surface): Set<string> {
  const ids = new Set<string>();
  let currentSurfaceId: string | null = surface.id;
  // Guard against pathological cycles.
  const visited = new Set<string>([surface.id]);
  while (currentSurfaceId) {
    let parentCardId: string | null = null;
    let parentSurfaceId: string | null = null;
    outer: for (const s of Object.values(surfaces)) {
      for (const c of allCards(s)) {
        if (c.childSurfaceId === currentSurfaceId) {
          parentCardId = c.id;
          parentSurfaceId = s.id;
          break outer;
        }
      }
    }
    if (!parentCardId || !parentSurfaceId) break;
    ids.add(parentCardId);
    if (visited.has(parentSurfaceId)) break;
    visited.add(parentSurfaceId);
    currentSurfaceId = parentSurfaceId;
  }
  return ids;
}

/**
 * Classify a referenced entry. External flag wins over kind — an external
 * system reads as an external in every context.
 */
function bucketFor(entry: Entry): keyof SurfaceContext {
  if (entry.kind === "person") return "persons";
  if (entry.external) return "externals";
  return "refs";
}

/** Altitude of the surface where `entryId` lives. */
function homeAltitude(surfaces: Surfaces, entryId: string): Altitude {
  for (const s of Object.values(surfaces)) {
    if (s.entries.some((e) => e.id === entryId)) return s.altitude;
  }
  return "system";
}

/** Every link in the model that points at `entryId`. */
export function incomingLinks(
  surfaces: Surfaces,
  entryId: string,
): IncomingLink[] {
  const out: IncomingLink[] = [];
  for (const s of Object.values(surfaces)) {
    for (const c of allCards(s)) {
      if (c.id === entryId) continue;
      for (const l of c.links ?? []) {
        if (l.to === entryId) out.push({ from: c, label: l.label });
      }
    }
  }
  return out;
}

/**
 * Resolve everything outside `surface`'s subtree that is link-connected to
 * something inside it, classified by role.
 */
export function surfaceContext(
  surfaces: Surfaces,
  surface: Surface,
): SurfaceContext {
  const subtree = subtreeCardIds(surfaces, surface);
  const onSurface = new Set(allCards(surface).map((c) => c.id));
  const ancestors = ancestorCardIds(surfaces, surface);
  // A link counts as "pointing into our scope" if it targets a card on the
  // surface OR any ancestor card. The ancestor case is how perimeter context
  // propagates down: a person who uses the system also uses its containers.
  const inScope = new Set<string>([...onSurface, ...ancestors]);
  const refIds = new Set<string>();

  // Outgoing links — anything in our scope (surface entries OR ancestor
  // cards) that points outside our scope is a ref. Walking ancestors too is
  // how a deeper-level view picks up the parent's external dependencies
  // (e.g. inside a component, we see the parent container's calls to a DB).
  const scopeOutboundSources: Entry[] = [...allCards(surface)];
  for (const aid of ancestors) {
    const c = findCard(surfaces, aid);
    if (c) scopeOutboundSources.push(c);
  }
  for (const c of scopeOutboundSources) {
    for (const l of c.links ?? []) {
      if (!subtree.has(l.to) && !ancestors.has(l.to)) refIds.add(l.to);
    }
  }
  // Incoming links — cards outside our scope whose links point at a card on
  // the surface or at any ancestor.
  for (const s of Object.values(surfaces)) {
    for (const c of allCards(s)) {
      if (subtree.has(c.id) || ancestors.has(c.id)) continue;
      for (const l of c.links ?? []) if (inScope.has(l.to)) refIds.add(c.id);
    }
  }

  // Determine direction for each ref: does it link INTO our scope (incoming)
  // or do we link OUT to it (outgoing), or both? "Our scope" includes the
  // surface entries plus ancestor cards, so context propagation reflects all
  // levels properly.
  const out: SurfaceContext = { persons: [], externals: [], refs: [] };
  for (const id of refIds) {
    const card = findCard(surfaces, id);
    if (!card) continue;
    let hasIncoming = false;
    let hasOutgoing = false;
    // outgoing FROM card into our scope = incoming for us
    for (const l of card.links ?? []) {
      if (inScope.has(l.to)) {
        hasIncoming = true;
        break;
      }
    }
    // outgoing FROM our scope to card = outgoing for us
    outer: for (const sid of inScope) {
      const c = findCard(surfaces, sid);
      if (!c) continue;
      for (const l of c.links ?? []) {
        if (l.to === id) {
          hasOutgoing = true;
          break outer;
        }
      }
    }
    const direction: LinkDirection =
      hasIncoming && hasOutgoing ? "both" : hasIncoming ? "incoming" : "outgoing";
    const ce: ContextEntry = {
      entry: card,
      altitude: homeAltitude(surfaces, id),
      direction,
    };
    out[bucketFor(card)].push(ce);
  }
  return out;
}
