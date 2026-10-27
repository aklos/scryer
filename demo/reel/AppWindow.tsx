/**
 * The real app in a fixed frame on the reel's stage, seen through a camera:
 * the camera names the window point it centres in the frame, its zoom, and a
 * small tilt of the frame. The app always fills the frame — the camera is
 * held from zooming out past it or panning off the window's edge — so
 * nothing slides under the header or beside the terminal. Targets (a tree row, a map card, a claim row, a tab) are measured in
 * the live app once it has settled for the frame, in window pixels, so the
 * pointer and any mark laid over the app land on the real UI.
 */

import { useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { continueRender, delayRender } from "remotion";
import type { Fixture } from "../types";
import { appSettled, type Navigate } from "./live/backend";
import { Live } from "./live/Live";
import type { Theme } from "./kit";

export const WIN_W = 1600;
export const WIN_H = 1000;

export type Cam = { x: number; y: number; s: number; rx: number; ry: number };

/** A frame on the stage, in screen pixels. */
export type Box = { x: number; y: number; w: number; h: number };

/** The window's shape, centred under the header. */
export const WIDE: Box = { x: 288, y: 200, w: 1344, h: 840 };

/** The camera as it is drawn: zoom no wider than the frame, centre no
 *  closer to the window's edge than half the frame. */
export function held(fr: Box, c: Cam): Cam {
  const z = Math.max(c.s, fr.w / WIN_W, fr.h / WIN_H);
  const hw = fr.w / 2 / z;
  const hh = fr.h / 2 / z;
  return { ...c, s: z, x: Math.min(WIN_W - hw, Math.max(hw, c.x)), y: Math.min(WIN_H - hh, Math.max(hh, c.y)) };
}

/** Where a window point lands on screen. */
export function onScreen(fr: Box, c: Cam, [x, y]: [number, number]): [number, number] {
  const h = held(fr, c);
  return [fr.x + fr.w / 2 + (x - h.x) * h.s, fr.y + fr.h / 2 + (y - h.y) * h.s];
}

export type Rect = { x: number; y: number; w: number; h: number; cx: number; cy: number };

/** How a target is found: by CSS selector and/or exact text (or text it
 *  contains), within a region of the window. */
export type Target = {
  sel?: string;
  text?: string;
  contains?: string;
  region?: "tree" | "main" | "top" | "status";
  nth?: number;
  /** The box around every match rather than one. */
  all?: boolean;
};

const TREE_W = 330;

function inRegion(r: Rect, region: Target["region"]): boolean {
  switch (region) {
    case "tree":
      return r.cx < TREE_W && r.cy > 40 && r.cy < WIN_H - 30;
    case "main":
      return r.cx > TREE_W && r.cy > 40 && r.cy < WIN_H - 30;
    case "top":
      return r.cy < 40;
    case "status":
      return r.cy > WIN_H - 30;
    default:
      return true;
  }
}

function find(root: HTMLElement, t: Target): Rect | null {
  const box = root.getBoundingClientRect();
  const pool: Element[] = t.sel
    ? [...root.querySelectorAll(t.sel)]
    : [...root.querySelectorAll("span, div, button, a, li, h1, h2, p, text")].filter((e) => e.childElementCount === 0);
  const hits: Rect[] = [];
  for (const el of pool) {
    const text = el.textContent?.trim() ?? "";
    if (t.text !== undefined && text !== t.text) continue;
    if (t.contains !== undefined && !text.includes(t.contains)) continue;
    const r = el.getBoundingClientRect();
    if (r.width === 0 && r.height === 0) continue;
    const rect = { x: r.left - box.left, y: r.top - box.top, w: r.width, h: r.height, cx: r.left - box.left + r.width / 2, cy: r.top - box.top + r.height / 2 };
    if (!inRegion(rect, t.region)) continue;
    hits.push(rect);
  }
  if (t.all && hits.length) {
    const x0 = Math.min(...hits.map((h) => h.x));
    const y0 = Math.min(...hits.map((h) => h.y));
    const x1 = Math.max(...hits.map((h) => h.x + h.w));
    const y1 = Math.max(...hits.map((h) => h.y + h.h));
    return { x: x0, y: y0, w: x1 - x0, h: y1 - y0, cx: (x0 + x1) / 2, cy: (y0 + y1) / 2 };
  }
  return hits[t.nth ?? 0] ?? null;
}

export function AppWindow<K extends string>({
  f,
  theme,
  fixture,
  version,
  nav,
  cam,
  frame: fr = WIDE,
  opacity = 1,
  targets,
  overlay,
  bleed = false,
}: {
  f: number;
  theme: Theme;
  fixture: Fixture;
  version: number;
  nav: Navigate;
  /** The camera, or a camera from what was measured this frame. */
  cam: Cam | ((at: Partial<Record<K, Rect>>) => Cam);
  /** The frame the app is seen in. */
  frame?: Box;
  opacity?: number;
  targets?: Record<K, Target>;
  /** Drawn over the app, in window pixels, with what was measured. */
  overlay?: (at: Partial<Record<K, Rect>>) => ReactNode;
  /** No frame: square edges, no shadow, so the app runs off the stage. */
  bleed?: boolean;
}) {
  const win = useRef<HTMLDivElement>(null);
  const [at, setAt] = useState<Partial<Record<K, Rect>>>({});
  const key = targets ? JSON.stringify(targets) : "";
  useLayoutEffect(() => {
    if (!targets) return;
    const handle = delayRender("measure targets");
    void appSettled().then(() => {
      const el = win.current;
      const out: Partial<Record<K, Rect>> = {};
      if (el) {
        // Measure the window flat: the camera's transform comes off for the
        // read and goes straight back.
        const saved = el.style.transform;
        el.style.transform = "none";
        const root = el.querySelector<HTMLElement>("[data-live]") ?? el;
        for (const [name, t] of Object.entries(targets) as [K, Target][]) {
          const r = find(root, t);
          if (r) out[name] = r;
        }
        el.style.transform = saved;
      }
      setAt(out);
      continueRender(handle);
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [f, key]);

  const c = held(fr, typeof cam === "function" ? cam(at) : cam);
  return (
    <div
      style={{
        position: "absolute",
        left: fr.x,
        top: fr.y,
        width: fr.w,
        height: fr.h,
        borderRadius: bleed ? 0 : 14,
        overflow: "hidden",
        boxShadow: bleed ? "none" : theme.shadow,
        background: theme.panel,
        // It rises a little as it fades in.
        transform: `translateY(${(1 - opacity) * 28}px)${c.rx || c.ry ? ` perspective(2600px) rotateX(${c.rx}deg) rotateY(${c.ry}deg)` : ""}`,
        opacity,
      }}
    >
      <div
        ref={win}
        style={{
          position: "absolute",
          left: fr.w / 2,
          top: fr.h / 2,
          width: WIN_W,
          height: WIN_H,
          transformOrigin: "0 0",
          transform: `scale(${c.s}) translate(${-c.x}px, ${-c.y}px)`,
        }}
      >
        <Live fixture={fixture} version={version} nav={nav} dark={theme.dark} frame={f} />
        {overlay ? <div style={{ position: "absolute", inset: 0, pointerEvents: "none" }}>{overlay(at)}</div> : null}
      </div>
    </div>
  );
}

/** A pointer's glide through measured targets: where it is at `f`, and the
 *  frame of its last click. `path` steps are [frame it arrives, target name
 *  or window point, click on arrival]. */
export function glide<K extends string>(f: number, at: Partial<Record<K, Rect>>, path: { at: number; to: K | [number, number]; click?: boolean; dur?: number }[]) {
  const pos = (to: K | [number, number]) => {
    if (Array.isArray(to)) return { x: to[0], y: to[1] };
    const r = at[to];
    return r ? { x: r.cx, y: r.cy } : null;
  };
  let cur = pos(path[0].to);
  let clickAt: number | undefined;
  for (let i = 1; i < path.length; i++) {
    const step = path[i];
    const dur = step.dur ?? 22;
    const start = step.at - dur;
    if (f < start) break;
    const next = pos(step.to);
    if (!cur || !next) {
      cur = next ?? cur;
      continue;
    }
    const t = Math.min(1, (f - start) / dur);
    const e = t < 0.5 ? 4 * t * t * t : 1 - Math.pow(-2 * t + 2, 3) / 2;
    cur = { x: cur.x + (next.x - cur.x) * e, y: cur.y + (next.y - cur.y) * e };
    if (step.click && f >= step.at) clickAt = step.at;
  }
  return { ...(cur ?? { x: 0, y: 0 }), clickAt, ready: cur !== null };
}
