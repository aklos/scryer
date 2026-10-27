/**
 * The reel's base, with the container map left to the app's own layout.
 * demo/fixture.json pins the containers at coordinates from an older
 * layout, and the externals it never pins now land on them; unpinned, the
 * map lays itself out cleanly.
 */

import type { Fixture } from "../../types";
import { BASE } from "../live/world";

const CONTAINERS = ["Dashboard", "API Gateway", "Fraud", "Ledger", "Webhooks", "Payments DB"];

export function unpinned(fx: Fixture): Fixture {
  const layout = { ...fx.layout };
  for (const name of CONTAINERS) delete layout[name];
  return { ...fx, layout };
}

export const BASE2 = unpinned(BASE);

/** On a map the tree is hidden, so the diagram has the whole window and
 *  sits centred in it; pages keep the tree. Rendered beside the app while
 *  a map is up. */
export function NoTree() {
  return <style>{`[data-live] .shrink-0.flex-col.h-full.border-r { display: none !important; }`}</style>;
}

/** On a map, with the tree hidden, the app fits the diagram to the whole
 *  window itself; this camera frames that window in the upper stage. */
export const MAP = { x: 800, y: 470, s: 1.25 };
