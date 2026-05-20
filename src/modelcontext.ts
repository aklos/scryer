/**
 * The whole surface map, in context — so a card can compute its effective
 * status by looking into the child surfaces that fulfill its responsibilities
 * (see rollup.ts). Provided by App.
 */

import { createContext } from "react";
import type { Surface } from "./viewmodel";

export const ModelContext = createContext<Record<string, Surface>>({});

/**
 * The set of entry ids visible on the *current* surface — own grid entries
 * plus everything in its perimeter (persons, externals, refs). Provided by
 * Surface. Used to scope incoming-link pills so a card doesn't show
 * relationships originating from cards that aren't even on this view.
 */
export const VisibleScopeContext = createContext<ReadonlySet<string> | null>(
  null,
);
