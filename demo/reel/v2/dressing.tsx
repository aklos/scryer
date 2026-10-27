/**
 * Reel v2's set dressing and layout. A beat opens on its card: the motif
 * drawn from its subject, composed with the headline at full size. Then the
 * card clears and the beat's figure takes the upper stage, running off its
 * edges and fading into the dark, while the headline holds in the lower
 * third on its own ground. Nothing is drawn over the app's text.
 */

import type { ReactNode } from "react";
import { ACCENT, GREEN, MONO, SANS, W, H, mix, p, s } from "../kit";

export const BG = "#07080A";
export const INK = "#F2F3F5";
export const DIM = "#A9AFB9";

/** Side margin of the type. */
export const M = 120;

/** The figure's ground: the upper stage, bleeding off three edges. */
export const FIGURE = { x: 0, y: 0, w: W, h: 860 };

export function Stage2({ children }: { children: ReactNode }) {
  return <div style={{ position: "absolute", inset: 0, overflow: "hidden", background: BG, color: INK, fontFamily: SANS }}>{children}</div>;
}

/** The app a step brighter and crisper than its own dark theme, so its
 *  panels and text read against the near-black stage. */
export function Lift({ children }: { children: ReactNode }) {
  return <div style={{ position: "absolute", inset: 0, filter: "brightness(1.55) contrast(1.06)" }}>{children}</div>;
}

/** A full-stage SVG layer. */
export function Field({ children, opacity = 1 }: { children: ReactNode; opacity?: number }) {
  return (
    <svg width={W} height={H} viewBox={`0 0 ${W} ${H}`} style={{ position: "absolute", inset: 0, opacity }}>
      {children}
    </svg>
  );
}

/** The figure's edges dissolving into the stage: top, sides, and a long
 *  fall into the lower third, so the headline has clean ground. */
export function Fades() {
  return (
    <div style={{ position: "absolute", inset: 0, zIndex: 4, pointerEvents: "none" }}>
      <div style={{ position: "absolute", left: 0, right: 0, top: 0, height: 56, background: `linear-gradient(${BG}, transparent)` }} />
      <div style={{ position: "absolute", left: 0, right: 0, top: 660, height: 170, background: `linear-gradient(transparent, ${BG})` }} />
      <div style={{ position: "absolute", left: 0, right: 0, top: 828, bottom: 0, background: BG }} />
      <div style={{ position: "absolute", left: 0, top: 0, bottom: 0, width: 110, background: `linear-gradient(90deg, ${BG}, transparent)` }} />
      <div style={{ position: "absolute", right: 0, top: 0, bottom: 0, width: 110, background: `linear-gradient(270deg, ${BG}, transparent)` }} />
    </div>
  );
}

/** Opacity and blur of a card: in from 0, gone by `until`. */
export function cardStyle(f: number, until: number) {
  const out = p(f, until, s(0.7), "move");
  return { opacity: p(f, 0, s(0.8)) * (1 - out), filter: out > 0.01 ? `blur(${out * 10}px)` : undefined };
}

/** The headline on the card: each line rises in, the next a beat later. */
export function CardTitle({ f, lines, size = 112, align = "center", at = s(0.5) }: { f: number; lines: string[]; size?: number; align?: "center" | "left"; at?: number }) {
  return (
    <div style={{ fontFamily: SANS, fontSize: size, fontWeight: 580, letterSpacing: "-0.045em", lineHeight: 1.04, color: INK, textAlign: align, whiteSpace: "nowrap" }}>
      {lines.map((line, i) => {
        const t = p(f, at + i * s(0.25), s(0.9));
        return (
          <div key={i} style={{ opacity: t, transform: `translateY(${(1 - t) * 18}px)` }}>
            {line}
          </div>
        );
      })}
    </div>
  );
}

export type Sub = { at: number; text: string };

/**
 * The headline held in the lower third from `at`, under the figure, and
 * under it the subheading for whichever part of the beat is on screen:
 * each one replaces the last as the beat moves on.
 */
export function Dock({ f, at, until, text, subs = [] }: { f: number; at: number; until?: number; text: string; subs?: Sub[] }) {
  const out = until === undefined ? 0 : p(f, until, s(0.5), "move");
  const t = p(f, at, s(0.8)) * (1 - out);
  if (t <= 0 && f > at) return null;
  return (
    <div style={{ position: "absolute", left: M, top: 836, zIndex: 6 }}>
      <div style={{ fontFamily: SANS, fontSize: 72, fontWeight: 580, letterSpacing: "-0.04em", lineHeight: 1, color: INK, whiteSpace: "nowrap", opacity: t, transform: `translateY(${(1 - t) * 16}px)` }}>{text}</div>
      <div style={{ position: "relative", marginTop: 22, height: 48 }}>
        {subs.map((sub, i) => {
          const next = subs[i + 1]?.at;
          const gone = next === undefined ? 0 : p(f, next - s(0.35), s(0.35), "move");
          const u = p(f, Math.max(sub.at, at + s(0.4)), s(0.6)) * (1 - gone) * (1 - out);
          if (u <= 0) return null;
          return (
            <div key={i} style={{ position: "absolute", left: 0, top: 0, fontFamily: SANS, fontSize: 40, fontWeight: 440, letterSpacing: "-0.02em", color: DIM, whiteSpace: "nowrap", opacity: u, transform: `translateY(${(1 - u) * 10}px)` }}>
              {sub.text}
            </div>
          );
        })}
      </div>
    </div>
  );
}

/** Flat-top hexagon of radius `r` round (cx, cy), as a path. */
export function hex(cx: number, cy: number, r: number): string {
  const pts = Array.from({ length: 6 }, (_, i) => {
    const a = (Math.PI / 3) * i;
    return `${cx + r * Math.cos(a)},${cy + r * Math.sin(a)}`;
  });
  return `M${pts.join("L")}Z`;
}

/**
 * Structure: the rings of a hexagonal container round the stage's centre,
 * each layer named along its edge, and chevrons on the spokes travelling
 * inward — dependencies point to the core.
 */
export function HexRings({ f, cx = W / 2, cy = H / 2, core = 540 }: { f: number; cx?: number; cy?: number; core?: number }) {
  const STEP = 120;
  const rings = Array.from({ length: 9 }, (_, i) => core + i * STEP);
  const LAYERS = ["domain", "application", "infrastructure"];
  const drawn = (i: number) => p(f, s(0.1) + i * 4, s(1.2));
  return (
    <g>
      {rings.map((r, i) => (
        <path
          key={r}
          d={hex(cx, cy, r)}
          fill="none"
          stroke={i === 0 ? ACCENT : "#FFFFFF"}
          strokeOpacity={(i === 0 ? 0.95 : Math.max(0.1, 0.38 - i * 0.04)) * drawn(i)}
          strokeWidth={i === 0 ? 2.5 : 1.5}
        />
      ))}
      {/* Spokes from each vertex outward, chevrons moving in along them. */}
      {Array.from({ length: 6 }, (_, k) => {
        const a = (Math.PI / 3) * k;
        const ux = Math.cos(a);
        const uy = Math.sin(a);
        const reach = core + STEP * 8;
        const chevrons = Array.from({ length: 6 }, (_, j) => {
          const d = core + 30 + ((j * 150 - (f / 30) * 40) % 900 + 900) % 900;
          const x = cx + ux * d;
          const y = cy + uy * d;
          const ang = (a * 180) / Math.PI + 180;
          return <path key={j} d="M8 -7 L0 0 L8 7" transform={`translate(${x} ${y}) rotate(${ang})`} fill="none" stroke="#FFFFFF" strokeOpacity={0.45 * drawn(2)} strokeWidth={1.75} />;
        });
        return (
          <g key={k}>
            <line x1={cx + ux * core} y1={cy + uy * core} x2={cx + ux * reach} y2={cy + uy * reach} stroke="#FFFFFF" strokeOpacity={0.16 * drawn(2)} />
            {chevrons}
          </g>
        );
      })}
      {/* Each layer named on its upper-right edge, reading up the edge. */}
      {LAYERS.map((name, i) => {
        const r = rings[i];
        const x = cx + 0.75 * r + 0.866 * 34;
        const y = cy - 0.433 * r - 0.5 * 34;
        return (
          <text key={name} x={x} y={y} transform={`rotate(-60 ${x} ${y})`} textAnchor="middle" fontFamily={MONO} fontSize={22} letterSpacing="0.1em" fill={i === 0 ? ACCENT : "#FFFFFF"} fillOpacity={(i === 0 ? 1 : 0.72) * drawn(i)}>
            {name}
          </text>
        );
      })}
    </g>
  );
}

/**
 * Plan: a column of claims written down the stage, each a sentence with its
 * mark — planned rings, proven ticks — scrolling up slowly. Marks turn
 * proven as the beat runs. On the card it is centred with a clear band for
 * the headline; as a rail (`rail` 0→1) it moves to the right of the stage
 * and keeps running beside the panel.
 */
export function ClaimColumn({ f, claims, clear = [380, 700], rail = 0 }: { f: number; claims: string[]; clear?: [number, number]; rail?: number }) {
  const PITCH = mix(70, 56, rail);
  const size = mix(28, 21, rail);
  const x0 = mix(420, 1330, rail);
  const scroll = (f / 30) * 14;
  const first = Math.floor(scroll / PITCH);
  const rows = Math.ceil(H / PITCH) + 2;
  return (
    <g>
      {Array.from({ length: rows }, (_, k) => {
        const i = first + k;
        const y = i * PITCH - scroll + 60;
        const full = claims[(i * 5) % claims.length];
        const text = rail > 0.5 && full.length > 44 ? full.slice(0, 43) + "…" : full;
        // On the card the clear band fades the column out; on the rail only the edges do.
        const mid = (clear[0] + clear[1]) / 2;
        const half = ((clear[1] - clear[0]) / 2) * (1 - rail);
        const dist = Math.max(0, Math.abs(y - mid) - half);
        const band = rail > 0.99 ? 1 : Math.min(1, dist / 120);
        const o = band * Math.min(1, (820 - y) / 120) * Math.min(1, y / 80);
        if (o <= 0.01) return null;
        const proven = ((i * 37) % 11) / 11 < p(f, s(0.8), s(5));
        return (
          <g key={i} opacity={o}>
            {proven ? (
              <path d={`M${x0} ${y - size * 0.32} l${size * 0.25} ${size * 0.25} l${size * 0.46} ${-size * 0.53}`} fill="none" stroke={GREEN} strokeWidth={2.5} strokeOpacity={1} />
            ) : (
              <circle cx={x0 + size * 0.36} cy={y - size * 0.32} r={size * 0.28} fill="none" stroke="#FFFFFF" strokeOpacity={0.55} strokeWidth={1.75} />
            )}
            <text x={x0 + size * 1.3} y={y} fontFamily={MONO} fontSize={size} fill="#FFFFFF" fillOpacity={0.5}>
              {text}
            </text>
          </g>
        );
      })}
    </g>
  );
}

export { mix };

/** A detail panel on a beat's stage: the beat's environment stays visible round it. */
export type Panel = { x: number; y: number; w: number; h: number };
export const PANEL_RIGHT: Panel = { x: 660, y: 56, w: 1180, h: 724 };
export const PANEL_LEFT: Panel = { x: 80, y: 56, w: 1180, h: 724 };

/** The panel's frame, for what is not the app (a terminal): the app draws its own. */
export function PanelFrame({ at, opacity = 1, z = 3, children }: { at: Panel; opacity?: number; z?: number; children?: ReactNode }) {
  return (
    <div
      style={{
        position: "absolute",
        left: at.x,
        top: at.y,
        width: at.w,
        height: at.h,
        borderRadius: 14,
        overflow: "hidden",
        background: "#0D1015",
        boxShadow: "0 40px 100px rgba(0,0,0,0.7), 0 0 0 1px rgba(255,255,255,0.13)",
        opacity,
        transform: `translateY(${(1 - opacity) * 24}px)`,
        zIndex: z,
      }}
    >
      {children}
    </div>
  );
}
