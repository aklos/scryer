/**
 * The whole ScryModel, in context — so a card can resolve cross-surface
 * references (e.g. mention pills pointing at nodes that aren't visible at
 * this depth). Provided by App.
 */

import { createContext } from "react";
import type { ScryModel } from "./viewmodel";
import { emptyModel } from "./viewmodel";

export const ModelContext = createContext<ScryModel>(emptyModel());

/**
 * The set of node ids visible on the *current* surface — own grid entries
 * plus everything in its perimeter (persons, externals, refs). Provided by
 * Surface. Used to scope incoming-link pills so a card doesn't show
 * relationships originating from cards that aren't even on this view.
 */
export const VisibleScopeContext = createContext<ReadonlySet<string> | null>(
  null,
);
