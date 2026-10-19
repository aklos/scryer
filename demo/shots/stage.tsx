/**
 * The film's visual identity, shared by every shot: a near-black stage, one
 * soft light behind the fragment, the fragment itself (a real component of
 * the app at 2–3× on a fixed-size container, clipped in a rounded card),
 * headline type on its own ground, and a chromeless terminal fragment.
 *
 * Everything takes its state as props; motion comes later from the frame.
 */

import type { CSSProperties, ReactNode } from "react";
import { AbsoluteFill } from "remotion";

export const W = 1920;
export const H = 1080;

/** The stage's colours. */
export const INK = {
  ground: "#0B0D12",
  card: "#111318",
  hairline: "rgba(255, 255, 255, 0.09)",
  fg: "#F2F4F8",
  sub: "rgba(242, 244, 248, 0.6)",
  dim: "rgba(242, 244, 248, 0.38)",
  ok: "#4ADE80",
  bad: "#F87171",
};

export const SANS = '"Inter Variable", ui-sans-serif, system-ui, sans-serif';
export const MONO = '"JetBrains Mono Variable", ui-monospace, monospace';

/** The ground, with the light centred on `light` (frame px). */
export function Stage({ light = { x: W / 2, y: H * 0.62 }, children }: { light?: { x: number; y: number }; children: ReactNode }) {
  return (
    <AbsoluteFill
      style={{
        background: `radial-gradient(900px 620px at ${light.x}px ${light.y}px, rgba(110, 130, 255, 0.13), rgba(110, 130, 255, 0) 70%), ${INK.ground}`,
        fontFamily: SANS,
        color: INK.fg,
        overflow: "hidden",
      }}
    >
      {children}
    </AbsoluteFill>
  );
}

/**
 * A fragment of the app: `width`×`height` in the app's own pixels, drawn at
 * `scale`, its top-left at `at` in the frame. The component inside lays out
 * at native size; the transform scales it, so text stays vector-sharp.
 */
export function Fragment({
  at,
  width,
  height,
  scale,
  bare,
  style,
  children,
}: {
  at: { x: number; y: number };
  width: number;
  height: number;
  scale: number;
  /** No card: the fragment sits straight on the stage. */
  bare?: boolean;
  style?: CSSProperties;
  children: ReactNode;
}) {
  return (
    <div
      style={{
        position: "absolute",
        left: at.x,
        top: at.y,
        width: width * scale,
        height: height * scale,
        borderRadius: bare ? 0 : 24,
        border: bare ? "none" : `1px solid ${INK.hairline}`,
        background: bare ? "transparent" : INK.card,
        boxShadow: bare ? "none" : "0 40px 120px rgba(0, 0, 0, 0.55), 0 8px 24px rgba(0, 0, 0, 0.35)",
        overflow: "hidden",
        ...style,
      }}
    >
      <div style={{ width, height, transform: `scale(${scale})`, transformOrigin: "0 0" }}>{children}</div>
    </div>
  );
}

/** A headline on its own ground: 72 px, mixed case, with an optional 36 px sub-line. */
export function Headline({ at, children, sub, width = 1400 }: { at: { x: number; y: number }; children: ReactNode; sub?: ReactNode; width?: number }) {
  return (
    <div style={{ position: "absolute", left: at.x, top: at.y, width }}>
      <div style={{ fontSize: 72, fontWeight: 600, letterSpacing: "-0.025em", lineHeight: 1.08, color: INK.fg }}>{children}</div>
      {sub && <div style={{ marginTop: 18, fontSize: 36, fontWeight: 400, letterSpacing: "-0.01em", color: INK.sub }}>{sub}</div>}
    </div>
  );
}

/** One line of a terminal fragment. */
export type TermLine =
  | { kind: "prompt"; text: string }
  /** A tool call: done (a quiet tick) or still running. */
  | { kind: "tool"; name: string; arg?: string; running?: boolean }
  | { kind: "run"; text: string }
  | { kind: "result"; items: { name: string; ok?: boolean }[] }
  | { kind: "diff"; op: "+" | "-"; text: string }
  | { kind: "note"; text: string };

/**
 * A terminal as 3–6 lines of 32 px mono straight on the stage: no window, no
 * title bar, no robot. Prompt lines white, tool lines muted with the tool's
 * name a step brighter, diffs on a two-colour gutter.
 */
export function Terminal({ at, lines, width = 1600 }: { at: { x: number; y: number }; lines: TermLine[]; width?: number }) {
  return (
    <div style={{ position: "absolute", left: at.x, top: at.y, width, fontFamily: MONO, fontSize: 32, lineHeight: 1.6, letterSpacing: "-0.01em" }}>
      {lines.map((l, i) => (
        <div key={i} style={{ whiteSpace: "pre", display: "flex", alignItems: "baseline", gap: 20 }}>
          <TermRow line={l} />
        </div>
      ))}
    </div>
  );
}

function TermRow({ line }: { line: TermLine }) {
  switch (line.kind) {
    case "prompt":
      return (
        <>
          <span style={{ color: INK.dim }}>›</span>
          <span style={{ color: INK.fg }}>{line.text}</span>
        </>
      );
    case "tool":
      return (
        <>
          <span style={{ color: line.running ? INK.dim : INK.ok, opacity: 0.8 }}>{line.running ? "▸" : "✓"}</span>
          <span style={{ color: "rgba(242,244,248,0.78)" }}>{line.name}</span>
          {line.arg && <span style={{ color: INK.dim }}>{line.arg}</span>}
        </>
      );
    case "run":
      return (
        <>
          <span style={{ color: INK.dim }}>$</span>
          <span style={{ color: "rgba(242,244,248,0.78)" }}>{line.text}</span>
        </>
      );
    case "result":
      return (
        <span style={{ display: "flex", gap: 40, paddingLeft: 40 }}>
          {line.items.map((t) => (
            <span key={t.name} style={{ color: t.ok === undefined ? INK.dim : t.ok ? INK.fg : INK.bad, opacity: t.ok === undefined ? 0.5 : 1 }}>
              <span style={{ color: t.ok === undefined ? INK.dim : t.ok ? INK.ok : INK.bad, marginRight: 14 }}>
                {t.ok === undefined ? "·" : t.ok ? "✓" : "✗"}
              </span>
              {t.name}
            </span>
          ))}
        </span>
      );
    case "diff":
      return (
        <span
          style={{
            display: "block",
            width: "100%",
            paddingLeft: 24,
            borderLeft: `4px solid ${line.op === "+" ? INK.ok : INK.bad}`,
            background: line.op === "+" ? "rgba(74, 222, 128, 0.08)" : "rgba(248, 113, 113, 0.08)",
            color: line.op === "+" ? "#BBF7D0" : "#FECACA",
          }}
        >
          {line.op} {line.text}
        </span>
      );
    case "note":
      return <span style={{ color: INK.sub }}>{line.text}</span>;
  }
}
