/**
 * The page every beat is set on (plans/demo-brief.md, "Visual language"):
 * a near-black page on a 12-column grid with 96 px margins, off-white type
 * — a grotesk lead line, captions in a column, mono labels for figure
 * numbers and file names, hairline rules — and the app's real components,
 * in its dark theme, set flat as figures. Scryer's blue is the one accent;
 * green and red are the app's own. Dark is a page, not a mood: no
 * gradients, no glow, no shadows.
 */

import type { CSSProperties, ReactNode } from "react";
import { AbsoluteFill, Img, staticFile } from "remotion";

export const W = 1920;
export const H = 1080;

/** The grid: 12 columns between 96 px margins, 24 px gutters. */
export const MARGIN = 96;
const GUTTER = 24;
const COLUMN = (W - 2 * MARGIN - 11 * GUTTER) / 12;
/** The x of column `i` (0–11), and the width of `n` columns. */
export const col = (i: number) => MARGIN + i * (COLUMN + GUTTER);
export const span = (n: number) => n * COLUMN + (n - 1) * GUTTER;

export const INK = {
  page: "#0C0D10",
  type: "#ECE9E2",
  soft: "rgba(236, 233, 226, 0.56)",
  faint: "rgba(236, 233, 226, 0.36)",
  rule: "rgba(236, 233, 226, 0.14)",
  panel: "#14161A",
  accent: "var(--accent)",
  ok: "var(--color-emerald-400)",
  bad: "var(--color-red-400)",
};

export const SANS = '"Inter Variable", ui-sans-serif, system-ui, sans-serif';
export const GROTESK = '"Space Grotesk Variable", "Inter Variable", sans-serif';
export const MONO = '"JetBrains Mono Variable", ui-monospace, monospace';

/** A small editorial label: mono, 22 px, uppercase, tracked. */
export function Label({ at, children, align = "left", style }: { at: { x: number; y: number }; children: ReactNode; align?: "left" | "right"; style?: CSSProperties }) {
  return (
    <div
      style={{
        position: "absolute",
        top: at.y,
        ...(align === "left" ? { left: at.x } : { right: W - at.x }),
        fontFamily: MONO,
        fontSize: 22,
        letterSpacing: "0.1em",
        textTransform: "uppercase",
        color: INK.soft,
        whiteSpace: "nowrap",
        ...style,
      }}
    >
      {children}
    </div>
  );
}

/** A horizontal hairline rule across `n` columns from column `from`. */
export function Rule({ y, from = 0, n = 12, style }: { y: number; from?: number; n?: number; style?: CSSProperties }) {
  return <div style={{ position: "absolute", left: col(from), top: y, width: span(n), height: 1, background: INK.rule, ...style }} />;
}

/**
 * The page: near-black, with a running head — the logo and SCRYER on the
 * left, the beat's number and subject on the right — over a hairline rule.
 */
export function Page({ beat, subject, children }: { beat: string; subject: string; children: ReactNode }) {
  return (
    <AbsoluteFill style={{ background: INK.page, fontFamily: SANS, color: INK.type, overflow: "hidden" }}>
      <Img src={staticFile("logo.png")} style={{ position: "absolute", left: MARGIN, top: 42, width: 32, height: 32 }} />
      <Label at={{ x: MARGIN + 48, y: 46 }} style={{ color: INK.type }}>
        Scryer
      </Label>
      <Label at={{ x: W - MARGIN, y: 46 }} align="right">
        {beat} — {subject}
      </Label>
      <Rule y={96} />
      {children}
    </AbsoluteFill>
  );
}

/** A beat's lead line: the grotesk at 64 px. */
export function Lead({ at, width, children, size = 64, style }: { at: { x: number; y: number }; width: number; children: ReactNode; size?: number; style?: CSSProperties }) {
  return (
    <div
      style={{
        position: "absolute",
        left: at.x,
        top: at.y,
        width,
        fontFamily: GROTESK,
        fontSize: size,
        fontWeight: 500,
        letterSpacing: "-0.035em",
        lineHeight: 1.06,
        color: INK.type,
        textWrap: "balance",
        ...style,
      }}
    >
      {children}
    </div>
  );
}

/** A caption: 34 px, 1.38 line height, set in a column. */
export function Caption({ at, width, children, style }: { at: { x: number; y: number }; width: number; children: ReactNode; style?: CSSProperties }) {
  return (
    <div
      style={{
        position: "absolute",
        left: at.x,
        top: at.y,
        width,
        fontSize: 34,
        lineHeight: 1.38,
        letterSpacing: "-0.01em",
        color: INK.soft,
        textWrap: "pretty",
        ...style,
      }}
    >
      {children}
    </div>
  );
}

/** Inline code in running text. */
export const C = ({ children }: { children: ReactNode }) => (
  <span style={{ fontFamily: MONO, fontSize: "0.88em", color: INK.type, letterSpacing: "-0.02em" }}>{children}</span>
);

/** The frame a figure sits in: flat on the page, hairline, 4 px radius. */
export const FIGURE: CSSProperties = {
  border: `1px solid ${INK.rule}`,
  borderRadius: 4,
  overflow: "hidden",
};

/**
 * A figure of the app: `width`×`height` in the app's own pixels, drawn at
 * `scale`, its top-left at `at`, its mono label set above it. The component
 * lays out at native size; the transform scales it, so text stays sharp.
 */
export function Figure({
  at,
  width,
  height,
  scale,
  label,
  style,
  children,
}: {
  at: { x: number; y: number };
  width: number;
  height: number;
  scale: number;
  label?: ReactNode;
  style?: CSSProperties;
  children: ReactNode;
}) {
  return (
    <div style={{ position: "absolute", left: at.x, top: at.y - 40, ...style }}>
      {label && <Label at={{ x: 0, y: 0 }}>{label}</Label>}
      <div style={{ position: "absolute", left: 0, top: 40, width: width * scale, height: height * scale, ...FIGURE, background: "var(--surface)" }}>
        <div style={{ width, height, transform: `scale(${scale})`, transformOrigin: "0 0" }}>{children}</div>
      </div>
    </div>
  );
}

/** A file as a figure, one ReactNode per line; `dim` lines recede, `mark`
 *  takes the accent on its gutter. */
export function Code({
  at,
  width,
  lines,
  dim = [],
  mark,
  size = 30,
  label,
  style,
  lineStyle,
}: {
  at: { x: number; y: number };
  width: number;
  lines: ReactNode[];
  dim?: number[];
  mark?: number;
  size?: number;
  label?: ReactNode;
  style?: CSSProperties;
  /** Per line, extra style (a line's own motion). */
  lineStyle?: (i: number) => CSSProperties | undefined;
}) {
  return (
    <div style={{ position: "absolute", left: at.x, top: at.y - 40, width, ...style }}>
      {label && <Label at={{ x: 0, y: 0 }}>{label}</Label>}
      <div
        style={{
          position: "absolute",
          left: 0,
          top: 40,
          width,
          ...FIGURE,
          background: "var(--surface)",
          padding: "22px 0",
          fontFamily: MONO,
          fontSize: size,
          lineHeight: 1.6,
          letterSpacing: "-0.015em",
          whiteSpace: "pre",
        }}
      >
        {lines.map((line, i) => (
          <div
            key={i}
            style={{
              minHeight: "1.6em",
              padding: "0 32px",
              opacity: dim.includes(i) ? 0.38 : 1,
              ...(i === mark ? { paddingLeft: 28, borderLeft: `4px solid ${INK.accent}`, background: "var(--accent-soft)" } : {}),
              ...lineStyle?.(i),
            }}
          >
            {line}
          </div>
        ))}
      </div>
    </div>
  );
}

/** The app's code-peek token colours in dark theme (SourceSection's TOKEN_CLASS). */
export const SYNTAX = {
  keyword: "var(--color-violet-400)",
  string: "var(--color-emerald-400)",
  comment: "var(--text-ghost)",
  constant: "var(--color-orange-300)",
  fn: "var(--color-blue-400)",
  type: "var(--color-cyan-400)",
  plain: "var(--text)",
  punct: "var(--text-muted)",
};
export const tok = (color: string, s: string) => <span style={{ color }}>{s}</span>;

/** One line of a terminal figure. */
export type TermLine =
  | { kind: "prompt"; text: string }
  /** A tool call: done (a tick) or still running; `accent` for the one the beat is about. */
  | { kind: "tool"; name: string; arg?: string; running?: boolean; accent?: boolean }
  | { kind: "run"; text: string }
  | { kind: "diff"; op: "+" | "-"; text: string }
  /** A tool's output, verbatim; `bad` for a finding, `accent` for the line the beat is about. */
  | { kind: "out"; text: string; tone?: "bad" | "dim" | "ok" | "accent" };

/**
 * A terminal as a figure: real session text in mono on a panel a step off
 * the page, with the figure hairline. No window chrome, no robot. Prompts
 * in full type, tool calls muted with the tool's name in full, output as
 * the tool printed it, diffs on the app's gutters. `shown` lines are drawn
 * (the rest wait their turn); `typed` cuts the last shown line, for typing.
 */
export function Terminal({
  at,
  lines,
  width,
  size = 28,
  label,
  shown = lines.length,
  typed,
  style,
}: {
  at: { x: number; y: number };
  lines: TermLine[];
  width: number;
  size?: number;
  label?: ReactNode;
  shown?: number;
  typed?: number;
  style?: CSSProperties;
}) {
  return (
    <div style={{ position: "absolute", left: at.x, top: at.y - 40, width, ...style }}>
      {label && <Label at={{ x: 0, y: 0 }}>{label}</Label>}
      <div
        style={{
          position: "absolute",
          left: 0,
          top: 40,
          width,
          ...FIGURE,
          background: INK.panel,
          padding: "22px 0",
          fontFamily: MONO,
          fontSize: size,
          lineHeight: 1.55,
          letterSpacing: "-0.02em",
          color: INK.type,
        }}
      >
        {lines.map((l, i) => (
          <div key={i} style={{ whiteSpace: "pre", display: "flex", alignItems: "baseline", gap: "0.6em", padding: "0 28px", visibility: i < shown ? "visible" : "hidden" }}>
            <TermRow line={i === shown - 1 && typed !== undefined ? cut(l, typed) : l} />
          </div>
        ))}
      </div>
    </div>
  );
}

function cut(l: TermLine, n: number): TermLine {
  if (l.kind === "prompt" || l.kind === "run") return { ...l, text: l.text.slice(0, n) };
  return l;
}

function TermRow({ line }: { line: TermLine }) {
  const muted = INK.faint;
  switch (line.kind) {
    case "prompt":
      return (
        <>
          <span style={{ color: muted }}>›</span>
          <span>{line.text}</span>
        </>
      );
    case "tool":
      return (
        <>
          <span style={{ color: line.running ? muted : INK.ok }}>{line.running ? "▸" : "✓"}</span>
          <span style={{ color: line.accent ? INK.accent : INK.type }}>{line.name}</span>
          {line.arg && <span style={{ color: INK.soft }}>{line.arg}</span>}
        </>
      );
    case "run":
      return (
        <>
          <span style={{ color: muted }}>$</span>
          <span>{line.text}</span>
        </>
      );
    case "diff":
      return (
        <span
          style={{
            display: "block",
            width: "calc(100% + 56px)",
            margin: "0 -28px",
            padding: "0 28px 0 24px",
            borderLeft: `4px solid ${line.op === "+" ? INK.ok : INK.bad}`,
            background: line.op === "+" ? "rgb(52 211 153 / 0.08)" : "rgb(248 113 113 / 0.08)",
          }}
        >
          <span style={{ color: line.op === "+" ? INK.ok : INK.bad }}>{line.op}</span> {line.text}
        </span>
      );
    case "out":
      return (
        <span
          style={{
            color:
              line.tone === "bad"
                ? INK.bad
                : line.tone === "ok"
                  ? INK.ok
                  : line.tone === "accent"
                    ? INK.accent
                    : line.tone === "dim"
                      ? muted
                      : INK.soft,
          }}
        >
          {line.text}
        </span>
      );
  }
}
