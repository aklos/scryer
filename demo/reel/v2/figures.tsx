/**
 * Reel v2's figures that are not the app: the agent's terminal as a large
 * chrome-less fragment, and the cards' motifs for catch-up (a C4 zoom that
 * never ends), checks (layers and the imports between them) and the problem
 * (tool calls pouring past faster than they can be read).
 */

import type { CSSProperties } from "react";
import { ACCENT, FPS, GREEN, MONO, RED, W, H, p, s } from "../kit";
import { BG, DIM, INK } from "./dressing";

/** Present from `a` to `b`, fading over `d` at each end. */
export function win(f: number, a: number, b: number, d = s(0.5)): number {
  return p(f, a, d) * (1 - p(f, b - d, d, "move"));
}

/** A figure's style while present: it settles in from a little smaller. */
export function shown(t: number): CSSProperties {
  return { opacity: t, transform: `scale(${0.985 + 0.015 * t})`, filter: t < 0.98 ? `blur(${(1 - t) * 6}px)` : undefined };
}

export type TLine =
  | { at: number; kind: "user"; text: string }
  | { at: number; kind: "tool"; name: string; arg?: string }
  | { at: number; kind: "out"; text: string; tone?: "ok" | "bad" }
  | { at: number; kind: "diff"; rows: { op: "+" | "-"; text: string }[] }
  | { at: number; kind: "say"; text: string };

/**
 * The agent's terminal, large: a prompt the user types, tool calls, their
 * output, a diff. Only what has happened by `f` is shown, and only the last
 * `keep` lines, so it reads as a fragment rather than a window.
 */
export function Term({ f, lines, size = 34, keep = 7, style }: { f: number; lines: TLine[]; size?: number; keep?: number; style?: CSSProperties }) {
  const seen = lines.filter((l) => f >= l.at).slice(-keep);
  const lh = 1.55;
  return (
    <div style={{ fontFamily: MONO, fontSize: size, lineHeight: lh, color: INK, whiteSpace: "pre", ...style }}>
      {seen.map((l, i) => {
        const t = p(f, l.at, s(0.35));
        const st: CSSProperties = { opacity: t, transform: `translateY(${(1 - t) * 10}px)` };
        switch (l.kind) {
          case "user": {
            const typed = l.text.slice(0, Math.max(0, Math.floor(((f - l.at) / FPS) * 34)));
            return (
              <div key={i} style={{ ...st, marginBottom: size * 0.5 }}>
                <span style={{ color: ACCENT }}>{"› "}</span>
                {typed}
                {typed.length < l.text.length ? <span style={{ opacity: 0.7 }}>▍</span> : null}
              </div>
            );
          }
          case "tool":
            return (
              <div key={i} style={st}>
                <span style={{ color: GREEN }}>{"● "}</span>
                <span style={{ fontWeight: 600 }}>{l.name}</span>
                {l.arg ? <span style={{ color: DIM }}>{` ${l.arg}`}</span> : null}
              </div>
            );
          case "out":
            return (
              <div key={i} style={{ ...st, color: l.tone === "ok" ? GREEN : l.tone === "bad" ? RED : DIM }}>
                <span style={{ color: DIM }}>{"  ⎿ "}</span>
                {l.text}
              </div>
            );
          case "diff":
            return (
              <div key={i} style={{ ...st, margin: `${size * 0.15}px 0 ${size * 0.15}px ${size * 1.2}px` }}>
                {l.rows.map((r, j) => (
                  <div key={j} style={{ background: r.op === "+" ? "rgba(34,160,107,0.16)" : "rgba(229,72,77,0.16)", color: r.op === "+" ? "#7EE0B0" : "#F59A9D", padding: `0 ${size * 0.4}px` }}>
                    {`${r.op} ${r.text}`}
                  </div>
                ))}
              </div>
            );
          case "say":
            return (
              <div key={i} style={{ ...st, whiteSpace: "normal", marginTop: size * 0.4 }}>
                {l.text}
              </div>
            );
        }
      })}
    </div>
  );
}

/**
 * Catch-up: a C4 zoom that never ends. Each level is a box holding three of
 * the next, and the camera keeps falling into the middle one — system,
 * container, component, code — each named in its corner.
 */
export function MapZoom({ f, clear }: { f: number; clear?: { x: number; y: number; w: number; h: number } }) {
  const K = 2.6;
  const LEVELS = [
    ["Aperture Pay", "Cardholder", "Acquiring Bank"],
    ["Ledger", "API Gateway", "Fraud"],
    ["domain", "application", "infrastructure"],
    ["refund.rs", "money.rs", "account.rs"],
  ];
  const total = (f / 30) * 0.22;
  const base = Math.floor(total);
  const phase = total - base;
  const names = (level: number) => LEVELS[((level % 4) + 4) % 4];
  const boxes: { x: number; y: number; w: number; h: number; label: string; main: boolean }[] = [];
  const cx = W / 2;
  const cy = 470;
  // From the outermost level still in frame inward; each holds the next
  // level's middle box (the one the camera falls into) and its two siblings.
  for (let i = -1; i < 5; i++) {
    const scale = Math.pow(K, -(i + phase)) * 3.2;
    const w = 1500 * scale;
    const h = 760 * scale;
    if (w < 40 || w > 9000) continue;
    boxes.push({ x: cx - w / 2, y: cy - h / 2, w, h, label: names(base + i)[0], main: true });
    const cw = w / K;
    const ch = h / K;
    const gap = cw * 0.12;
    for (const side of [-1, 1]) {
      boxes.push({ x: cx - cw / 2 + side * (cw + gap), y: cy - ch / 2, w: cw * 0.86, h: ch, label: names(base + i + 1)[side < 0 ? 1 : 2], main: false });
    }
  }
  return (
    <g>
      {boxes.map((b, k) => {
        // Brightest at the middle of its life, gone when tiny or past the frame.
        const size = b.w / 1500;
        const o = Math.min(1, Math.max(0, (size - 0.04) / 0.18)) * Math.min(1, Math.max(0, (3.4 - size) / 1.6));
        if (o <= 0.01) return null;
        const r = Math.max(4, Math.min(22, b.w * 0.012));
        const fs = Math.max(12, Math.min(30, b.w * 0.016));
        return (
          <g key={k} opacity={o}>
            <rect x={b.x} y={b.y} width={b.w} height={b.h} rx={r} fill="none" stroke={b.main ? "#FFFFFF" : "#FFFFFF"} strokeOpacity={b.main ? 0.42 : 0.24} strokeWidth={1.5} strokeDasharray={b.main ? undefined : "6 6"} />
            {fs > 13 ? (
              <text x={b.x + fs * 0.9} y={b.y + fs * 1.6} fontFamily={MONO} fontSize={fs} fill={b.main ? ACCENT : "#FFFFFF"} fillOpacity={b.main ? 0.95 : 0.5}>
                {b.label}
              </text>
            ) : null}
          </g>
        );
      })}
      {clear ? <rect x={clear.x} y={clear.y} width={clear.w} height={clear.h} rx={24} fill={BG} fillOpacity={0.88} /> : null}
    </g>
  );
}

/**
 * Checks: a container's layers as bands, the imports between them as edges
 * pointing inward. One import points the wrong way: it draws red, carries
 * its finding, and is gone when the file moves.
 */
export function ImportGraph({ f, x0 = 980, x1 = 1800, wrongAt = s(1.3), fixAt = s(99), stillAt = s(99) }: { f: number; x0?: number; x1?: number; wrongAt?: number; fixAt?: number; stillAt?: number }) {
  const BANDS = [
    { name: "domain", y: 260, n: 4 },
    { name: "application", y: 520, n: 5 },
    { name: "infrastructure", y: 780, n: 6 },
  ];
  const node = (b: number, i: number): [number, number] => {
    const band = BANDS[b];
    const span = x1 - x0;
    return [x0 + (span * (i + 0.5)) / band.n, band.y];
  };
  // Allowed: outer depends on inner.
  const EDGES: [number, number, number, number][] = [
    [1, 0, 0, 0],
    [1, 1, 0, 1],
    [1, 2, 0, 1],
    [1, 3, 0, 2],
    [1, 4, 0, 3],
    [2, 0, 1, 0],
    [2, 1, 1, 1],
    [2, 2, 1, 1],
    [2, 3, 1, 2],
    [2, 4, 1, 3],
    [2, 5, 1, 4],
    [2, 2, 0, 1],
  ];
  const drawn = p(f, s(0.2), s(1.2));
  const wrong = p(f, wrongAt, s(0.6)) * (1 - p(f, fixAt, s(0.6), "move"));
  // It pulses on the card; beside the panel it holds still, so the app has the attention.
  const pulse = f < stillAt ? 0.6 + 0.4 * Math.sin(((f - wrongAt) / 30) * Math.PI * 2) : 0.9;
  const [wx0, wy0] = node(0, 2);
  const [wx1, wy1] = node(1, 3);
  return (
    <g>
      {BANDS.map((b) => (
        <g key={b.name} opacity={drawn}>
          <line x1={x0 - 40} y1={b.y} x2={x1 + 40} y2={b.y} stroke="#FFFFFF" strokeOpacity={0.12} strokeDasharray="2 10" />
          <text x={x0 - 40} y={b.y - 46} fontFamily={MONO} fontSize={22} letterSpacing="0.1em" fill="#FFFFFF" fillOpacity={0.6}>
            {b.name}
          </text>
        </g>
      ))}
      {EDGES.map(([fb, fi, tb, ti], k) => {
        const [ax, ay] = node(fb, fi);
        const [bx, by] = node(tb, ti);
        const t = p(f, s(0.4) + k * 2, s(0.8));
        return <line key={k} x1={ax} y1={ay} x2={ax + (bx - ax) * t} y2={ay + (by - ay) * t} stroke="#FFFFFF" strokeOpacity={0.34} strokeWidth={1.5} markerEnd="url(#ig-arrow)" />;
      })}
      {wrong > 0 ? (
        <g opacity={wrong}>
          <line x1={wx0} y1={wy0} x2={wx1} y2={wy1} stroke={RED} strokeWidth={3} strokeOpacity={pulse} markerEnd="url(#ig-arrow-red)" />
          <text x={(wx0 + wx1) / 2 - 24} y={(wy0 + wy1) / 2} textAnchor="end" fontFamily={MONO} fontSize={24} fill={RED}>
            domain → application
          </text>
        </g>
      ) : null}
      {BANDS.flatMap((b, bi) =>
        Array.from({ length: b.n }, (_, i) => {
          const [x, y] = node(bi, i);
          return <rect key={`${bi}${i}`} x={x - 14} y={y - 14} width={28} height={28} rx={6} fill={BG} stroke={bi === 0 && i === 2 && wrong > 0.5 ? RED : "#FFFFFF"} strokeOpacity={0.8 * drawn} strokeWidth={1.75} />;
        }),
      )}
      <defs>
        <marker id="ig-arrow" viewBox="0 0 10 10" refX="22" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
          <path d="M0 0 L10 5 L0 10 Z" fill="#FFFFFF" fillOpacity={0.5} />
        </marker>
        <marker id="ig-arrow-red" viewBox="0 0 10 10" refX="22" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
          <path d="M0 0 L10 5 L0 10 Z" fill={RED} />
        </marker>
      </defs>
    </g>
  );
}

const FILES = [
  "crates/ledger/src/application/ledger.rs",
  "crates/ledger/src/domain/refund.rs",
  "crates/api-gateway/src/lib.rs",
  "crates/webhooks/src/shell/outbox.rs",
  "crates/ledger/src/application/facade.rs",
  "dashboard/src/features/refund.ts",
  "crates/ledger/src/domain/payout.rs",
  "crates/webhooks/src/core/backoff.rs",
  "dashboard/src/pages/payout.ts",
  "crates/fraud/src/core/scoring.rs",
  "crates/ledger/tests/refunds.rs",
  "dashboard/src/entities/refund.ts",
];

/**
 * The problem: four sessions' tool calls pouring up the stage in columns,
 * too fast to read. The band in the middle is kept clear for the headline.
 */
export function Waterfall({ f, clear = [400, 680] }: { f: number; clear?: [number, number] }) {
  const COLS = 4;
  const PITCH = 46;
  return (
    <g>
      {Array.from({ length: COLS }, (_, c) => {
        const speed = 260 + c * 70;
        const scroll = (f / 30) * speed;
        const first = Math.floor(scroll / PITCH);
        const x = 90 + c * 450;
        return Array.from({ length: Math.ceil(H / PITCH) + 2 }, (_, k) => {
          const i = first + k;
          const y = H + 40 - (i * PITCH - scroll) - PITCH * (Math.ceil(H / PITCH) + 2);
          const yy = ((y % (H + 2 * PITCH)) + H + 2 * PITCH) % (H + 2 * PITCH) - PITCH;
          const tool = i % 4 === 3 ? "Bash" : i % 3 === 0 ? "Write" : "Edit";
          const arg = tool === "Bash" ? (i % 2 ? "cargo test -p ledger" : "pnpm test") : FILES[(i * 5 + c * 3) % FILES.length].split("/").slice(-2).join("/");
          const mid = (clear[0] + clear[1]) / 2;
          const half = (clear[1] - clear[0]) / 2;
          const dist = Math.max(0, Math.abs(yy - mid) - half);
          const o = Math.min(1, dist / 90);
          return (
            <text key={`${c}-${i}`} x={x} y={yy} fontFamily={MONO} fontSize={24} fill="#FFFFFF" fillOpacity={0.46 * o}>
              <tspan fill={GREEN} fillOpacity={0.9 * o}>
                ●{" "}
              </tspan>
              <tspan fontWeight={600}>{tool}</tspan> {arg}
            </text>
          );
        });
      })}
    </g>
  );
}
