/**
 * Scene contract. A scene is three things kept deliberately separate:
 *   - `initial` / `render` — a pure state→UI mapping (the *what's on screen*),
 *   - `run` — the choreography (the *what happens, and when*).
 * The director mutates state and drives camera + cursor; the render stays pure.
 */

import type { ReactNode } from "react";
import type { Director } from "./director";

export interface Scene<S = unknown> {
  /** Caption-free, camera-at-rest starting state. */
  initial: S;
  render: (state: S) => ReactNode;
  /** The shot list. Awaited start to finish, then the scene holds. */
  run: (d: Director<S>) => Promise<void>;
}

/** Thrown out of an awaiting script when its stage unmounts mid-take. */
export class CancelledError extends Error {
  constructor() {
    super("scene cancelled");
    this.name = "CancelledError";
  }
}
