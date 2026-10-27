/**
 * Run many agents on one codebase. Four sessions beside the container map,
 * each ringing the containers it is editing in its own colour; where two
 * meet in Ledger, the second is told when it orients there. Nothing locks.
 * Then the page that gathers what every session changed since you last
 * looked.
 */

import { AppWindow, WIDE, type Box, type Cam, type Rect } from "../AppWindow";
import { Caption, Crescent, DARK, MONO, Stage, Statement, keyed, p, s } from "../kit";
import { BASE, nodeId } from "../live/world";
import type { Navigate } from "../live/backend";

export const AGENTS = s(17);
const T = DARK;

const K = { enter: s(1.9), lines: s(2.8), told: s(6.6), landing: s(10.6) };

/** The container map, between the two columns of sessions. */
const FRAME: Box = { x: 490, y: 200, w: 940, h: 840 };
const MAP: Cam = { x: 1060, y: 470, s: 1.6, rx: 0, ry: 0 };

const HUES = ["#3B82F6", "#38BDF8", "#A78BFA", "#F0B44C"];

const SESSIONS = [
  { id: "c3d8e1f7", task: "add partial refunds to the ledger", at: [70, 250], to: ["Ledger", "API Gateway"], tools: ["Edit ledger/src/application/ledger.rs", "Edit api-gateway/src/lib.rs"] },
  { id: "e90c44ab", task: "export payouts as CSV", at: [70, 650], to: ["Dashboard"], tools: ["Edit dashboard/src/features/export.ts", "Bash pnpm test"] },
  { id: "7be2d510", task: "retry failed ledger webhooks first", at: [1470, 250], to: ["Webhooks"], tools: ["Edit webhooks/src/shell/delivery.rs", "Bash cargo test -p webhooks"] },
  { id: "a41f9c02", task: "hold disputed funds back from payouts", at: [1470, 650], to: ["Ledger"], tools: ["scryer · orient ledger/src/application/ledger.rs"] },
] as const;

const NAMES = ["API Gateway", "Ledger", "Webhooks", "Dashboard"] as const;
type Name = (typeof NAMES)[number];
const TARGETS = Object.fromEntries(NAMES.map((n) => [n, { sel: `.react-flow__node[data-id="${nodeId(n)}"]` }])) as Record<Name, { sel: string }>;

/** When session `i` starts working in its containers. */
const working = (i: number) => K.lines + i * s(0.6);

const CARD_W = 380;

export function Agents({ f }: { f: number }) {
  const enter = p(f, K.enter, s(1.0));
  const toLanding = p(f, K.landing, s(0.4), "move");
  // The frame dips out while the sessions clear, and comes back wide on the page.
  const swap = K.landing + s(0.4);
  const dip = p(f, K.landing, s(0.35)) * (1 - p(f, swap + 1, s(0.5)));
  const nav: Navigate = f < swap ? { view: "diagram", diagramFocus: nodeId("Aperture Pay"), lens: null } : { view: "wiki", page: "landing", lens: null };
  const cam = keyed<Cam>(f, MAP, [
    { at: swap, dur: 1, x: 900, y: 420, s: 1.1 },
  ]);
  const fr = keyed<Box>(f, FRAME, [{ at: swap, dur: 1, ...WIDE }]);
  const told = p(f, K.told, s(0.6));
  const rings = (at: Partial<Record<Name, Rect>>) => {
    const seen: Record<string, number> = {};
    return SESSIONS.flatMap((se, i) =>
      se.to.map((name) => {
        const r = at[name as Name];
        if (!r) return null;
        const k = (seen[name] = (seen[name] ?? -1) + 1);
        const t = p(f, working(i) + k * s(0.4), s(0.5));
        const pad = 6 + k * 7;
        const pulse = 0.75 + 0.25 * Math.sin((f - working(i)) / 9);
        return (
          <div key={`${se.id}-${name}`} style={{ opacity: t * (1 - toLanding) }}>
            <div style={{ position: "absolute", left: r.x - pad, top: r.y - pad, width: r.w + pad * 2, height: r.h + pad * 2, borderRadius: 10 + pad / 2, border: `2.5px solid ${HUES[i]}`, opacity: pulse }} />
            <div
              style={{
                position: "absolute",
                left: r.x + r.w + pad - 78,
                top: r.y - pad - 22 - k * 22,
                padding: "2px 7px",
                borderRadius: 5,
                background: HUES[i],
                color: "#08090B",
                fontFamily: MONO,
                fontSize: 11,
                fontWeight: 600,
              }}
            >
              {se.id}
            </div>
          </div>
        );
      }),
    );
  };
  return (
    <Stage theme={T}>
      <AppWindow f={f} theme={T} fixture={BASE} version={0} nav={nav} cam={cam} frame={fr} opacity={enter * (1 - dip)} targets={TARGETS} overlay={f < K.landing + s(1.0) ? rings : undefined} />
      {f < K.landing + s(1.0) && (
        <div style={{ position: "absolute", inset: 0, opacity: 1 - toLanding }}>
          {SESSIONS.map((se, i) => {
            const t = p(f, K.enter + s(0.3) + i * 4, s(0.7));
            const alert = se.id === "a41f9c02" ? told : 0;
            return (
              <div
                key={se.id}
                style={{
                  position: "absolute",
                  left: se.at[0],
                  top: se.at[1],
                  width: CARD_W,
                  boxSizing: "border-box",
                  padding: "18px 20px",
                  borderRadius: 14,
                  background: T.panel,
                  border: `1px solid ${f >= working(i) ? HUES[i] : "transparent"}`,
                  boxShadow: T.shadow,
                  opacity: t,
                  transform: `translateY(${(1 - t) * 30}px)`,
                  fontFamily: MONO,
                  color: T.text,
                }}
              >
                <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
                  <svg width={26} height={26}>
                    <Crescent cx={13} cy={13} r={12} fill={HUES[i]} />
                  </svg>
                  <span style={{ fontSize: 19 }}>{se.id}</span>
                </div>
                <div style={{ marginTop: 8, fontSize: 18, color: T.dim, lineHeight: 1.4 }}>{se.task}</div>
                <div style={{ marginTop: 12, display: "flex", flexDirection: "column", gap: 4 }}>
                  {se.tools.map((tool, k) => (
                    <div key={k} style={{ fontSize: 16, color: T.dim, opacity: p(f, working(i) + k * s(1.2), 8), whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>
                      ● {tool}
                    </div>
                  ))}
                </div>
                {alert > 0 && (
                  <div style={{ marginTop: 12, padding: "10px 12px", borderRadius: 8, background: "rgba(59,130,246,0.14)", color: "#BFD7FF", fontSize: 15, lineHeight: 1.45, opacity: alert }}>
                    Also edited by live session c3d8e1f7 (add partial refunds to the ledger)
                  </div>
                )}
              </div>
            );
          })}
        </div>
      )}
      <Statement f={f} theme={T} index="05" text="Run many agents on one codebase." />
      <Caption f={f} theme={T} at={K.lines} until={K.landing - s(0.2)}>
        Every session's edits are recorded. An agent opening a file is told who else is in it. Nothing locks.
      </Caption>
      <Caption f={f} theme={T} at={K.landing} until={AGENTS}>
        And one page shows what every session changed since you last looked.
      </Caption>
    </Stage>
  );
}
