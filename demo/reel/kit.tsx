/**
 * The reel's kit: timing, the two themes the beats alternate between, and
 * the few things drawn around the app — the statement that opens a beat and
 * then stays docked as its reference point, the caption under it, the
 * camera's keyframes, the pointer. The app itself is never drawn here.
 */

import type { CSSProperties, ReactNode } from "react";
import { Easing, interpolate } from "remotion";

export const FPS = 30;
export const W = 1920;
export const H = 1080;
export const s = (seconds: number) => Math.round(seconds * FPS);

export const SANS = '"Geist Variable", system-ui, sans-serif';
export const MONO = '"Geist Mono Variable", ui-monospace, monospace';

export type Theme = {
  dark: boolean;
  bg: string;
  /** The stage's backdrop: a soft light from above on `bg`. */
  wash: string;
  text: string;
  dim: string;
  faint: string;
  line: string;
  panel: string;
  shadow: string;
};

export const DARK: Theme = {
  dark: true,
  bg: "#08090B",
  wash: "radial-gradient(120% 80% at 50% 0%, #15181E 0%, #08090B 70%)",
  text: "#F2F3F5",
  dim: "#9399A3",
  faint: "#5C626D",
  line: "rgba(255,255,255,0.10)",
  panel: "#111317",
  shadow: "0 40px 100px rgba(0,0,0,0.7), 0 0 0 1px rgba(255,255,255,0.11)",
};

/** The light beats: a grey stage, so the white app and terminal read as
 *  objects on it. */
export const LIGHT: Theme = {
  dark: false,
  bg: "#D6DAE0",
  wash: "radial-gradient(120% 80% at 50% 0%, #E8EAEE 0%, #D3D7DD 70%)",
  text: "#111318",
  dim: "#4C525C",
  faint: "#7D838E",
  line: "rgba(16,24,40,0.12)",
  panel: "#FFFFFF",
  shadow: "0 30px 80px rgba(16,24,40,0.20), 0 6px 20px rgba(16,24,40,0.10), 0 0 0 1px rgba(16,24,40,0.12)",
};

export const ACCENT = "#3B82F6";
export const GREEN = "#22A06B";
export const RED = "#E5484D";

const outEase = Easing.bezier(0.16, 1, 0.3, 1);
const inOutEase = Easing.bezier(0.65, 0, 0.35, 1);

/** 0→1 between frame `at` and `at + dur`, eased out (arrivals) or at both
 *  ends (`move`, for travel). */
export function p(f: number, at: number, dur: number, kind: "out" | "move" = "out"): number {
  if (dur <= 0) return f >= at ? 1 : 0;
  const t = interpolate(f, [at, at + dur], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp" });
  return (kind === "move" ? inOutEase : outEase)(t);
}

export const mix = (a: number, b: number, t: number) => a + (b - a) * t;

/** Characters of `text` typed by frame `f`, from `at`. */
export function typed(text: string, f: number, at: number, cps = 40): string {
  return text.slice(0, Math.max(0, Math.floor(((f - at) / FPS) * cps)));
}

/** Present between `at` and `until`: rises in, blurs out. */
export function present(f: number, at: number, until: number, dur = s(0.6)): CSSProperties {
  const a = p(f, at, dur);
  const l = p(f, until, s(0.45), "move");
  const blur = (1 - a) * 6 + l * 6;
  return { opacity: a * (1 - l), transform: `translateY(${(1 - a) * 14}px)`, filter: blur > 0.05 ? `blur(${blur}px)` : undefined };
}

/** A keyframed camera or position: holds each key and travels to the next
 *  over that key's `dur` frames from its `at`. */
export type Key<T> = { at: number; dur?: number } & Partial<T>;

export function keyed<T extends Record<string, number>>(f: number, first: T, keys: Key<T>[]): T {
  let cur = { ...first };
  for (const k of keys) {
    const t = p(f, k.at, k.dur ?? s(1.0), "move");
    if (t <= 0) break;
    const next = { ...cur } as Record<string, number>;
    for (const name of Object.keys(first)) {
      const to = (k as Record<string, number | undefined>)[name];
      if (to !== undefined) next[name] = mix(cur[name] as number, to, t);
    }
    cur = next as T;
  }
  return cur;
}

/** Where the header ends: windows and terminals sit below this line. */
export const TOP = 200;
const LEFT = 96;

/**
 * A beat's reference point: one plain sentence, set large on its own when
 * the beat opens, then held as the header — small index, the sentence, and
 * the caption under it — for as long as the beat runs.
 */
export function Statement({ f, theme, index, text, dockAt = s(1.7) }: { f: number; theme: Theme; index: string; text: string; dockAt?: number }) {
  const words = text.split(" ");
  const leave = p(f, dockAt, s(0.5), "move");
  const head = p(f, dockAt + s(0.2), s(0.6));
  return (
    <>
      {leave < 1 && (
        <div style={{ position: "absolute", left: 150, top: 400, width: 1620, zIndex: 5, opacity: 1 - leave, transform: `translateY(${-leave * 60}px)`, filter: leave > 0 ? `blur(${leave * 6}px)` : undefined }}>
          {index ? <div style={{ fontFamily: MONO, fontSize: 26, letterSpacing: "0.08em", color: theme.faint, marginBottom: 18, opacity: p(f, s(0.4), s(0.5)) }}>{index}</div> : null}
          <div style={{ fontFamily: SANS, fontSize: 88, fontWeight: 560, letterSpacing: "-0.04em", lineHeight: 1.06, color: theme.text }}>
            {words.map((w, i) => {
              // From after the reel's fade between beats, so two beats' text never share the screen.
              const t = p(f, s(0.45) + i * 3, s(0.6));
              return (
                <span key={i} style={{ display: "inline-block", opacity: t, transform: `translateY(${(1 - t) * 24}px)`, marginRight: "0.24em" }}>
                  {w}
                </span>
              );
            })}
          </div>
        </div>
      )}
      {head > 0 && (
        <div style={{ position: "absolute", left: LEFT, top: 44, zIndex: 5, display: "flex", alignItems: "baseline", gap: 20, whiteSpace: "nowrap", opacity: head, transform: `translateY(${(1 - head) * 16}px)` }}>
          {index ? <span style={{ fontFamily: MONO, fontSize: 22, color: theme.faint, letterSpacing: "0.06em" }}>{index}</span> : null}
          <span style={{ fontFamily: SANS, fontSize: 46, fontWeight: 580, letterSpacing: "-0.03em", color: theme.text }}>{text}</span>
        </div>
      )}
    </>
  );
}

/** The line under the header's sentence that says what the frame shows. */
export function Caption({ f, theme, at, until, children }: { f: number; theme: Theme; at: number; until: number; children: ReactNode }) {
  if (f < at - 2 || f > until + s(0.6)) return null;
  return (
    <div
      style={{
        position: "absolute",
        left: LEFT,
        top: 116,
        width: 1728,
        whiteSpace: "nowrap",
        fontFamily: SANS,
        fontSize: 27,
        fontWeight: 440,
        letterSpacing: "-0.01em",
        color: theme.dim,
        zIndex: 5,
        ...present(f, at, until),
      }}
    >
      {children}
    </div>
  );
}

/** The pointer: white arrow, a ring where it clicks. */
export function Pointer({ x, y, clickAt, f, opacity = 1 }: { x: number; y: number; clickAt?: number; f: number; opacity?: number }) {
  const since = clickAt === undefined ? -1 : f - clickAt;
  const ring = since >= 0 && since < 16 ? since / 16 : null;
  const press = since >= 0 && since < 5 ? 0.86 : 1;
  return (
    <div style={{ position: "absolute", left: x, top: y, pointerEvents: "none", opacity, zIndex: 20 }}>
      {ring !== null && (
        <div
          style={{
            position: "absolute",
            left: -24,
            top: -24,
            width: 48,
            height: 48,
            borderRadius: 9999,
            border: `2.5px solid ${ACCENT}`,
            transform: `scale(${mix(0.4, 1.3, ring)})`,
            opacity: 1 - ring,
          }}
        />
      )}
      <svg width="28" height="32" viewBox="0 0 26 30" style={{ transform: `scale(${press})`, transformOrigin: "2px 2px", filter: "drop-shadow(0 3px 6px rgba(0,0,0,0.45))" }}>
        <path d="M2 2 L2 24 L8 18.5 L12.5 28 L16.5 26.2 L12 17 L20 17 Z" fill="#fff" stroke="#111" strokeWidth="1.6" strokeLinejoin="round" />
      </svg>
    </div>
  );
}

/** A crescent (one of Scryer's shapes), as an SVG path. */
export function Crescent({ cx, cy, r, fill, bite = 0.6 }: { cx: number; cy: number; r: number; fill: string; bite?: number }) {
  const o = r * bite;
  const x = o / 2;
  const yy = Math.sqrt(Math.max(0, r * r - x * x));
  return <path d={`M${cx + x} ${cy - yy} A${r} ${r} 0 1 0 ${cx + x} ${cy + yy} A${r} ${r} 0 0 1 ${cx + x} ${cy - yy} Z`} fill={fill} />;
}

export function Stage({ theme, children, style }: { theme: Theme; children: ReactNode; style?: CSSProperties }) {
  return <div style={{ position: "absolute", inset: 0, overflow: "hidden", backgroundImage: theme.wash, backgroundColor: theme.bg, color: theme.text, fontFamily: SANS, ...style }}>{children}</div>;
}
