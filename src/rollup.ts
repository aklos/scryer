/**
 * Effective status — the conformance roll-up across surfaces.
 *
 * A leaf card's responsibilities carry their own status. A card with a child
 * surface gets each responsibility's status rolled up from the inner cards
 * that fulfill it (`entry.fulfills === responsibility.id`); recurse, and the
 * whole tree's status is grounded in its leaves.
 *
 * Externals are out of scope: their work isn't ours to ship. They don't
 * contribute to a parent's roll-up and their own status is undefined.
 * Responsibilities on externals (read as expectations of what we'll get from
 * them) have no status either.
 */

import type { Surface, Entry, Responsibility } from "./viewmodel";
import type { Status } from "./statusColors";
import { rollupStatus } from "./statusColors";

type Surfaces = Record<string, Surface>;

/** Every card on a surface (flat list — no group recursion needed). */
export function allCards(surface: Surface): Entry[] {
  return surface.entries;
}

/** A status fallback for in-scope entries whose status couldn't be derived. */
const UNSET: Status = "planned";

export function effectiveRespStatus(
  surfaces: Surfaces,
  entry: Entry,
  resp: Responsibility,
): Status {
  const own = resp.status ?? UNSET;
  if (!entry.childSurfaceId) return own;
  const child = surfaces[entry.childSurfaceId];
  if (!child) return own;
  const fulfillers = allCards(child).filter(
    (c) => c.fulfills === resp.id && !c.external,
  );
  if (fulfillers.length === 0) return own;
  return rollupStatus(fulfillers.map((c) => effectiveEntryStatus(surfaces, c) ?? UNSET));
}

/**
 * Roll a card up to a single status. Returns `null` for externals — they
 * don't carry status, and callers should render them in a neutral hue.
 */
export function effectiveEntryStatus(
  surfaces: Surfaces,
  entry: Entry,
): Status | null {
  if (entry.external) return null;
  return rollupStatus(
    entry.responsibilities.map((r) => effectiveRespStatus(surfaces, entry, r)),
  );
}
