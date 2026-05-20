/**
 * Context for the pick-up-and-place interaction and group resize.
 */

import { createContext } from "react";
import type { PointerEvent } from "react";
import type { Rect } from "./pack";

export interface HoverState {
  rect: Rect;
  valid: boolean;
  targetGroupId?: string;
}

export interface GridContextValue {
  beginResize(groupId: string, e: PointerEvent): void;
  beginGroupDrag(groupId: string, e: PointerEvent): void;
  heldId: string | null;
  resizingId: string | null;
  resizeRejected: { right: boolean; bottom: boolean } | null;
  hover: HoverState | null;
}

export const GridContext = createContext<GridContextValue>({
  beginResize: () => {},
  beginGroupDrag: () => {},
  heldId: null,
  resizingId: null,
  resizeRejected: null,
  hover: null,
});
