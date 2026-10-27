/**
 * Your coding agent's terminal, beside the app: unbranded, because it stands
 * for whichever agent you run. Its lines are the agent's requests, its tool
 * calls (scryer's among them, over MCP) and what they returned, appearing at
 * the frames the beat gives them. A pure render of the frame.
 */

import type { CSSProperties, ReactNode } from "react";
import { GREEN, MONO, RED, SANS, p, typed, type Theme } from "./kit";

export type TLine =
  | { at: number; kind: "user"; text: string; cps?: number }
  | { at: number; kind: "tool"; name: string; arg?: string; done?: number }
  | { at: number; kind: "out"; text: ReactNode; tone?: "ok" | "bad" | "dim" }
  | { at: number; kind: "say"; text: string }
  | { at: number; kind: "diff"; rows: { op: "+" | "-" | " "; text: string }[] }
  | { at: number; kind: "tests"; names: string[]; every: number; fail?: boolean };

export function Terminal({
  f,
  theme,
  title,
  lines,
  width,
  height,
  style,
  scroll = 0,
}: {
  f: number;
  theme: Theme;
  title: string;
  lines: TLine[];
  width: number;
  height: number;
  style?: CSSProperties;
  /** How far the body has scrolled up, in px. */
  scroll?: number;
}) {
  const dark = theme.dark;
  const ink = dark ? "#E6E8EB" : "#1A1C20";
  const dim = dark ? "#7D838D" : "#6B7079";
  const bg = dark ? "#0E1013" : "#FFFFFF";
  const edge = dark ? "rgba(255,255,255,0.08)" : "rgba(0,0,0,0.08)";
  const shown = lines.filter((l) => f >= l.at);
  return (
    <div
      style={{
        position: "absolute",
        width,
        height,
        borderRadius: 14,
        background: bg,
        boxShadow: theme.shadow,
        overflow: "hidden",
        fontFamily: MONO,
        fontSize: 20,
        lineHeight: 1.55,
        color: ink,
        ...style,
      }}
    >
      <div style={{ height: 44, display: "flex", alignItems: "center", gap: 10, padding: "0 18px", borderBottom: `1px solid ${edge}`, color: dim, fontSize: 15 }}>
        <span style={{ display: "flex", gap: 7 }}>
          {[0, 1, 2].map((i) => (
            <span key={i} style={{ width: 11, height: 11, borderRadius: 99, background: dark ? "#2A2E35" : "#E2E4E8" }} />
          ))}
        </span>
        <span style={{ marginLeft: 8 }}>{title}</span>
      </div>
      <div style={{ position: "relative", height: height - 45, overflow: "hidden" }}>
        <div style={{ padding: "18px 22px", transform: `translateY(${-scroll}px)` }}>
          {shown.map((l, i) => (
            <div key={i} style={{ opacity: p(f, l.at, 6), marginBottom: l.kind === "user" || l.kind === "say" ? 14 : 4 }}>
              <Line f={f} line={l} ink={ink} dim={dim} dark={dark} />
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

function Line({ f, line, ink, dim, dark }: { f: number; line: TLine; ink: string; dim: string; dark: boolean }) {
  switch (line.kind) {
    case "user":
      return (
        <div style={{ display: "flex", gap: 12, padding: "10px 14px", borderRadius: 8, background: dark ? "#181B20" : "#F1F2F4" }}>
          <span style={{ color: dim }}>❯</span>
          <span style={{ color: ink }}>{typed(line.text, f, line.at, line.cps ?? 34)}</span>
        </div>
      );
    case "tool": {
      const done = line.done === undefined || f >= line.done;
      return (
        <div style={{ display: "flex", gap: 12, whiteSpace: "pre" }}>
          <span style={{ color: done ? GREEN : dim }}>{done ? "●" : "○"}</span>
          <span>
            <span style={{ fontWeight: 600, color: ink }}>{line.name}</span>
            {line.arg ? <span style={{ color: dim }}>({line.arg})</span> : null}
          </span>
        </div>
      );
    }
    case "out":
      return (
        <div style={{ display: "flex", gap: 12, paddingLeft: 4, whiteSpace: "pre", color: line.tone === "ok" ? GREEN : line.tone === "bad" ? RED : dim }}>
          <span style={{ color: dim }}>⎿</span>
          <span>{line.text}</span>
        </div>
      );
    case "say":
      return (
        <div style={{ display: "flex", gap: 12, fontFamily: SANS, fontSize: 21, lineHeight: 1.45, color: ink }}>
          <span style={{ color: ink, fontFamily: MONO }}>●</span>
          <span>{line.text}</span>
        </div>
      );
    case "diff":
      return (
        <div style={{ margin: "4px 0 6px 26px", borderRadius: 6, overflow: "hidden", fontSize: 18 }}>
          {line.rows.map((r, i) => (
            <div
              key={i}
              style={{
                display: "flex",
                gap: 12,
                padding: "1px 12px",
                whiteSpace: "pre",
                background: r.op === "+" ? "rgba(34,160,107,0.14)" : r.op === "-" ? "rgba(229,72,77,0.14)" : "transparent",
                color: r.op === " " ? dim : ink,
              }}
            >
              <span style={{ color: r.op === "+" ? GREEN : r.op === "-" ? RED : dim }}>{r.op}</span>
              {r.text}
            </div>
          ))}
        </div>
      );
    case "tests":
      return (
        <div style={{ paddingLeft: 26, display: "flex", flexDirection: "column" }}>
          {line.names.map((n, i) => {
            const fin = f >= line.at + (i + 1) * line.every;
            const bad = line.fail && fin;
            return (
              <div key={n} style={{ whiteSpace: "pre", color: fin ? (bad ? RED : GREEN) : dim, opacity: f >= line.at + i * line.every ? 1 : 0 }}>
                test {n} ... {fin ? (bad ? "FAILED" : "ok") : ""}
              </div>
            );
          })}
        </div>
      );
  }
}
