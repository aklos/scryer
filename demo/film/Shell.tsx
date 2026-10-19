/**
 * Scryer itself, mounted in the film: the real `src/app/App.tsx`, its Tauri
 * layer swapped for demo/stub by the bundler. Each frame brings the app to
 * the film's state through that stub — the fixture it serves, the change
 * events a watcher would send, the navigation — and holds the frame until the
 * app has re-read and drawn. Nothing here renders product UI of its own.
 */

import { useLayoutEffect, useState } from "react";
import { continueRender, delayRender, staticFile } from "remotion";
import App from "../../src/app/App";
import "../../src/app/index.css";
import { applyTheme, loadTheme } from "../../src/shared/theme/theme";
import { NOW } from "../fixture";
import { PROJECT, sync, waitForApp, type Navigate } from "../stub/backend";
import type { Fixture } from "../types";
import type { View } from "./state";
import "./shell.css";

export interface ShellProps {
  fixture: Fixture;
  /** How many fixture changes are applied; the app re-reads when it moves. */
  version: number;
  /** With its node ids resolved. */
  view: View;
  lens?: string;
  frame: number;
}

/** The app's navigation for a film view. */
function navigation(view: View, lens: string | undefined): Navigate {
  const base = { lens: lens ?? null };
  switch (view.kind) {
    case "landing":
      return { ...base, view: "wiki", page: "landing" };
    case "node":
      return { ...base, view: "wiki", page: view.node };
    case "diagram":
      return { ...base, view: "diagram", diagramFocus: view.level === "context" ? null : (view.focus ?? null) };
  }
}

/** Set the app up as the desktop would have it: dark, and opened before at a
 *  fixed time so "since you last looked" reads the same in every render tab. */
function prepare() {
  applyTheme({ ...loadTheme(), colorMode: "dark" });
  try {
    localStorage.setItem(`scryer:last-opened:${PROJECT}`, String(NOW - 3 * 3600));
    localStorage.setItem("scryer:view", "wiki");
  } catch {
    /* no storage: the landing window just reads "first time here" */
  }
}

export function Shell({ fixture, version, view, lens, frame }: ShellProps) {
  const [ready] = useState(() => {
    prepare();
    return true;
  });
  // Serve this frame's fixture before the app asks for anything.
  const nav = navigation(view, lens);
  useLayoutEffect(() => {
    const first = !mounted;
    mounted = true;
    const moved = sync(fixture, version, nav);
    if (!moved && !first) return;
    const handle = delayRender(`app catches up at frame ${frame}`);
    // A diagram lays out and fits its view on its own clock; a page only
    // re-reads. The first frame of a render tab mounts the whole app.
    const minMs = first ? 1200 : nav.view === "diagram" ? 900 : 250;
    void waitForApp(minMs).then(() => {
      // The app links its logo at the site root; Remotion serves public/ under /public/.
      document.querySelectorAll<HTMLImageElement>('img[src="/logo.png"]').forEach((img) => {
        img.src = staticFile("logo.png");
      });
      continueRender(handle);
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [frame]);
  return <div data-film="">{ready && <App />}</div>;
}

let mounted = false;
