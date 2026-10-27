/**
 * Scryer itself, mounted in the reel: the real `src/app/App.tsx`, its Tauri
 * layer swapped for this directory's stub by the bundler. Each frame brings
 * the app to the beat's state through that stub — the fixture it serves, the
 * change events a watcher would send, the navigation — and holds the frame
 * until the app has re-read and drawn. Nothing here renders product UI.
 *
 * One app is mounted at a time: a beat mounts its own and the stub forgets
 * the last beat's state when it does.
 */

import { useLayoutEffect, useRef, useState } from "react";
import { continueRender, delayRender, staticFile } from "remotion";
import App from "../../../src/app/App";
import "../../../src/app/index.css";
import { applyColorMode, applyTheme, loadTheme } from "../../../src/shared/theme/theme";
import { NOW } from "../../story";
import type { Fixture } from "../../types";
import { PROJECT, resetSync, sync, waitForApp, type Navigate } from "./backend";
import "./live.css";

/** Set the app up as the desktop would have it, opened before at a fixed
 *  time so "since you last looked" reads the same in every render tab. The
 *  tabs share one localStorage and the app records the wall clock as
 *  last-opened when it opens the project, so that key is pinned. */
function prepare(dark: boolean) {
  applyTheme({ ...loadTheme(), colorMode: dark ? "dark" : "light" });
  try {
    const key = `scryer:last-opened:${PROJECT}`;
    const set = Storage.prototype.setItem;
    set.call(localStorage, key, String(NOW - 3 * 3600));
    Storage.prototype.setItem = function (this: Storage, k: string, v: string) {
      if (k !== key) set.call(this, k, v);
    };
    localStorage.setItem("scryer:treeSymbols", "0");
    localStorage.setItem("scryer:treeWidth", "330");
  } catch {
    /* no storage: the landing window just reads "first time here" */
  }
}

export function Live({ fixture, version, nav, dark, frame }: { fixture: Fixture; version: number; nav: Navigate; dark: boolean; frame: number }) {
  const mounted = useRef(false);
  const [ready] = useState(() => {
    prepare(dark);
    resetSync();
    return true;
  });
  useLayoutEffect(() => {
    applyColorMode(dark ? "dark" : "light");
    const first = !mounted.current;
    mounted.current = true;
    const moved = sync(fixture, version, nav);
    if (!moved && !first) return;
    const handle = delayRender(`app catches up at frame ${frame}`);
    // A diagram lays out and fits its view on its own clock; a page only
    // re-reads. The first frame mounts the whole app.
    const minMs = first ? 3000 : nav.view === "diagram" ? 2200 : 300;
    void waitForApp(minMs).then(() => {
      // The app links its logo at the site root; Remotion serves public/ under /public/.
      document.querySelectorAll<HTMLImageElement>('img[src="/logo.png"]').forEach((img) => {
        img.src = staticFile("logo.png");
      });
      continueRender(handle);
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [frame]);
  return <div data-live="">{ready && <App />}</div>;
}
