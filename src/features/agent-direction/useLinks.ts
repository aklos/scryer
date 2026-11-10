/**
 * `scryer://show` links the developer clicked: the app host holds the newest until the window takes it (at a
 * cold start it arrives before anything listens), the window opens the link's project when another is open, and
 * the workspace then follows the link's direction. A click is the developer's own choice, so a link moves the view
 * whether or not they follow agents.
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { Direction } from "./useDirections";

/** A clicked link (mirrors Rust `links::LinkTarget`). */
export interface LinkTarget {
  project: string;
  direction: Direction;
}

const trimSlash = (p: string) => p.replace(/\/+$/, "");

/** Whether a link points into the project at `path`. */
export function linkIsFor(link: LinkTarget, path: string | null): boolean {
  return !!path && trimSlash(link.project) === trimSlash(path);
}

export function useLinks({
  projectPath,
  openProject,
}: {
  projectPath: string | null;
  openProject: (path: string) => Promise<unknown> | void;
}) {
  const [link, setLink] = useState<LinkTarget | null>(null);
  const opened = useRef(projectPath);
  opened.current = projectPath;
  const openRef = useRef(openProject);
  openRef.current = openProject;

  const take = useCallback(async () => {
    const t = await invoke<LinkTarget | null>("take_link").catch(() => null);
    if (!t) return;
    if (!linkIsFor(t, opened.current)) await openRef.current(t.project);
    setLink(t);
  }, []);

  useEffect(() => {
    void take();
    const un = listen("open-link", () => void take());
    return () => void un.then((f) => f());
  }, [take]);

  /** The workspace followed the link (or found nothing to follow). */
  const done = useCallback(() => setLink(null), []);
  return { link, done };
}
