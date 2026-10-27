/**
 * Plan features as claims: the agent writes the plan as todo tests, they
 * appear on Ledger's page as planned claims, one is cut, the tests run and
 * the report turns the rest proven. Then a probe breaks the code to see
 * whether a green test notices, and the test is tightened until it does.
 */

import { AppWindow, type Cam, type Rect } from "../../AppWindow";
import { DARK, GREEN, keyed, mix, p, s } from "../../kit";
import { BASE, REFUND_STUBS, nodeId, world } from "../../live/world";
import type { Navigate } from "../../live/backend";
import { CardTitle, ClaimColumn, Dock, Field, Lift, PANEL_LEFT, PanelFrame, Stage2, cardStyle } from "../dressing";
import { Term, win, type TLine } from "../figures";

export const PLAN2 = s(27);

// One thing on screen at a time: a terminal step covers the app panel, then
// lifts away to show what it changed.
const K = { clear: s(3.0), ask: s(3.3), rows: s(9.4), cut: s(11.2), cutDone: s(12.9), run: s(14.0), proven: s(15.9), probe: s(17.9), caught: s(23.8) };

const KEPT = REFUND_STUBS.slice(0, 4);
const DROP = KEPT.map((c) => c.description);
const W = [
  world({ drop: DROP, add: REFUND_STUBS.map((claim) => ({ claim })) }),
  world({ drop: DROP, add: KEPT.map((claim) => ({ claim })) }),
  world({ drop: DROP, add: KEPT.map((claim) => ({ claim, state: "proven" as const })) }),
  world({ drop: DROP, add: KEPT.map((claim, i) => ({ claim, state: "proven" as const, probe: i === 1 ? ("doubleCheck" as const) : undefined })) }),
];

function version(f: number): number {
  if (f >= K.caught) return 3;
  if (f >= K.proven) return 2;
  if (f >= K.cutDone) return 1;
  return 0;
}

const NAV: Navigate = { view: "wiki", page: nodeId("Ledger"), lens: null };
const CLAIMS = [...BASE.tree.claims.map((c) => c.description), ...REFUND_STUBS.map((c) => c.description)].filter((d) => d.length < 64);

/** Ledger's own claims come first on its page; the refund rows follow. */
const FIRST = 3;

const ASK: TLine[] = [
  { at: K.ask + s(0.3), kind: "user", text: "add partial refunds to the ledger" },
  { at: K.ask + s(1.8), kind: "tool", name: "scryer · orient", arg: "ledger/src/domain/refund.rs" },
  { at: K.ask + s(2.3), kind: "out", text: "stubs → crates/ledger/tests/refunds.rs" },
  { at: K.ask + s(3.2), kind: "tool", name: "Write", arg: "crates/ledger/tests/refunds.rs" },
  {
    at: K.ask + s(3.5),
    kind: "diff",
    rows: [
      { op: "+", text: "/// When a refund exceeds the captured amount, rejects it" },
      { op: "+", text: "#[test] #[ignore]" },
      { op: "+", text: "fn rejects_over_refund() { todo!() }" },
    ],
  },
];

const CUT: TLine[] = [{ at: K.cut + s(0.3), kind: "user", text: "drop the notify one, then go" }];

const RUN: TLine[] = [
  { at: K.run + s(0.2), kind: "tool", name: "Bash", arg: "cargo test --test refunds" },
  { at: K.run + s(0.7), kind: "tool", name: "scryer · ingest_test_report", arg: "target/junit/cargo.xml" },
  { at: K.run + s(1.1), kind: "out", text: "4 proven", tone: "ok" },
];

const PROBE: TLine[] = [
  { at: K.probe + s(0.3), kind: "tool", name: "scryer · open_probe", arg: "rejects_over_refund" },
  { at: K.probe + s(0.8), kind: "out", text: "worktree ~/.scryer/probes/aperture-pay-3f1c" },
  { at: K.probe + s(1.3), kind: "diff", rows: [{ op: "-", text: "if amount > remaining {" }, { op: "+", text: "if amount > remaining * 2 {" }] },
  { at: K.probe + s(2.4), kind: "out", text: "1 break tried, 1 survived", tone: "bad" },
  { at: K.probe + s(3.4), kind: "tool", name: "Edit", arg: "crates/ledger/tests/refunds.rs" },
  { at: K.probe + s(3.7), kind: "diff", rows: [{ op: "+", text: 'assert!(ledger.refund("cap-1", gbp(1001)).is_err());' }] },
  { at: K.probe + s(5.0), kind: "out", text: "1 break tried, 0 survived", tone: "ok" },
];

type Name = "rows" | "r0" | "r1" | "r2" | "r3";

export function Plan2({ f }: { f: number }) {
  const v = version(f);
  const ask = win(f, K.ask, K.rows + s(0.2), s(0.6));
  const app = win(f, K.rows, K.probe + s(0.2), s(0.6)) + win(f, K.caught, PLAN2 + s(1), s(0.6));
  const probe = win(f, K.probe, K.caught + s(0.2), s(0.6));
  const cut = win(f, K.cut, K.cutDone - s(0.3), s(0.4));
  const run = win(f, K.run, K.proven - s(0.1), s(0.4));
  const z = 1.5;
  const cam = (at: Partial<Record<Name, Rect>>): Cam => {
    const r = at.rows;
    const on = r ? { x: r.x - 70 + PANEL_LEFT.w / 2 / z, y: r.cy + 20 } : { x: 900, y: 450 };
    return keyed<Cam>(f, { ...on, s: z - 0.08, rx: 0, ry: 0 }, [{ at: K.rows, dur: s(3), s: z }]);
  };
  const targets = {
    rows: { sel: '[id^="resp-"]', all: true },
    ...Object.fromEntries(KEPT.map((_, i) => [`r${i}`, { sel: '[id^="resp-"]', nth: FIRST + i }])),
  } as Record<Name, { sel: string; all?: boolean; nth?: number }>;
  return (
    <Stage2>
      <Field opacity={1 - 0.45 * p(f, K.clear + s(1.2), s(0.8))}>
        <ClaimColumn f={Math.min(f, K.clear + s(1.2))} claims={CLAIMS} rail={p(f, K.clear, s(1.2), "move")} />
      </Field>
      <div style={{ position: "absolute", left: 0, right: 0, top: 480, display: "flex", flexDirection: "column", alignItems: "center", zIndex: 6, ...cardStyle(f, K.clear) }}>
        <CardTitle f={f} lines={["Plan features as claims."]} />
      </div>

      {ask > 0 ? (
        <PanelFrame at={PANEL_LEFT} opacity={ask}>
          <Term f={f} lines={ASK} size={30} style={{ position: "absolute", left: 56, top: 64 }} />
        </PanelFrame>
      ) : null}

      {app > 0 ? (
        <div style={{ position: "absolute", inset: 0, opacity: Math.min(1, app), transform: `translateY(${(1 - Math.min(1, app)) * 24}px)` }}>
          <Lift>
            <AppWindow
              f={f}
              theme={DARK}
              fixture={W[v]}
              version={v}
              nav={NAV}
              cam={cam}
              frame={PANEL_LEFT}
              targets={targets}
              overlay={(at) => (
                <>
                  {/* Proven: a tick lands beside each row as the report comes in. */}
                  {f >= K.proven &&
                    KEPT.map((_, i) => {
                      const r = at[`r${i}` as Name];
                      if (!r) return null;
                      const t = p(f, K.proven + i * s(0.3), s(0.5));
                      return (
                        <svg key={i} width={34} height={34} viewBox="0 0 34 34" style={{ position: "absolute", left: r.x - 46, top: r.y + 2, opacity: t, transform: `scale(${mix(1.6, 1, t)})` }}>
                          <path d="M6 18 l8 8 l15 -18" fill="none" stroke={GREEN} strokeWidth={4} strokeLinecap="round" strokeLinejoin="round" />
                        </svg>
                      );
                    })}
                  {/* Probed: a ring round the row whose test now catches the break. */}
                  {f >= K.caught && at.r1 ? (
                    <div
                      style={{
                        position: "absolute",
                        left: at.r1.x - 60,
                        top: at.r1.y - 8,
                        width: at.r1.w + 80,
                        height: at.r1.h + 16,
                        borderRadius: 14,
                        border: `2.5px solid ${GREEN}`,
                        opacity: p(f, K.caught + s(0.4), s(0.5)),
                        transform: `scale(${mix(1.04, 1, p(f, K.caught + s(0.4), s(0.6)))})`,
                      }}
                    />
                  ) : null}
                </>
              )}
            />
          </Lift>
        </div>
      ) : null}

      {probe > 0 ? (
        <PanelFrame at={PANEL_LEFT} opacity={probe}>
          <Term f={f} lines={PROBE} size={29} keep={8} style={{ position: "absolute", left: 56, top: 56 }} />
        </PanelFrame>
      ) : null}
      {cut > 0 ? (
        <PanelFrame at={PANEL_LEFT} opacity={cut}>
          <Term f={f} lines={CUT} size={34} style={{ position: "absolute", left: 64, top: 300 }} />
        </PanelFrame>
      ) : null}
      {run > 0 ? (
        <PanelFrame at={PANEL_LEFT} opacity={run}>
          <Term f={f} lines={RUN} size={32} style={{ position: "absolute", left: 64, top: 250 }} />
        </PanelFrame>
      ) : null}
      <Dock
        f={f}
        at={K.ask + s(0.4)}
        text="Plan features as claims."
        subs={[
          { at: K.ask, text: "Your agent writes the plan as todo tests." },
          { at: K.rows, text: "Each one shows up as a planned claim." },
          { at: K.cut, text: "Cut what you don't want before any code exists." },
          { at: K.run, text: "Rows turn proven when the test report comes in." },
          { at: K.probe, text: "A probe breaks the code to see if the test notices." },
          { at: K.caught, text: "Now it does." },
        ]}
      />
    </Stage2>
  );
}
