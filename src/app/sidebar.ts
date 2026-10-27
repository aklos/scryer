/**
 * Where the tree sits as the window narrows. A wide window docks it beside
 * the page, shown or hidden at the user's choice (remembered); a narrow one
 * has no room for both, so the tree leaves the row and slides over the page
 * on demand, closing again once something in it is picked.
 */

import { useCallback, useEffect, useState, useSyncExternalStore } from "react";

/** Below this window width the tree overlays the page instead of docking. */
export const NARROW_WINDOW = 800;

export type SidebarMode = "docked" | "drawer" | "hidden";

/** The tree's place, from the window's width and the two toggles. */
export function sidebarMode(narrow: boolean, docked: boolean, drawerOpen: boolean): SidebarMode {
  if (narrow) return drawerOpen ? "drawer" : "hidden";
  return docked ? "docked" : "hidden";
}

const NARROW_QUERY = `(max-width: ${NARROW_WINDOW - 0.02}px)`;

function useNarrowWindow(): boolean {
  return useSyncExternalStore(
    (onChange) => {
      const mql = window.matchMedia(NARROW_QUERY);
      mql.addEventListener("change", onChange);
      return () => mql.removeEventListener("change", onChange);
    },
    () => window.matchMedia(NARROW_QUERY).matches,
  );
}

export function useSidebar() {
  const narrow = useNarrowWindow();
  const [docked, setDocked] = useState(() => localStorage.getItem("scryer:treeOpen") !== "0");
  const [drawerOpen, setDrawerOpen] = useState(false);
  // Crossing the breakpoint either way starts the drawer closed.
  useEffect(() => setDrawerOpen(false), [narrow]);

  const toggle = useCallback(() => {
    if (narrow) {
      setDrawerOpen((o) => !o);
      return;
    }
    setDocked((d) => {
      localStorage.setItem("scryer:treeOpen", d ? "0" : "1");
      return !d;
    });
  }, [narrow]);
  const dismiss = useCallback(() => setDrawerOpen(false), []);

  const mode = sidebarMode(narrow, docked, drawerOpen);
  return { mode, open: mode !== "hidden", toggle, dismiss };
}
