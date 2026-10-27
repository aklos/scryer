/**
 * Keep the codebase coherent. A container's declared style is checked
 * against the real import graph: a file placed in domain that imports from
 * application is flagged on the map, and the finding clears only when the
 * file moves. A claim's verdict is computed from the test report and the
 * code under it: edit the code and it goes stale until the test runs again.
 */

import { AppWindow, type Cam, type Rect } from "../../AppWindow";
import { DARK, GREEN, SANS, keyed, mix, p, s } from "../../kit";
import { MISPLACED, nodeId, world } from "../../live/world";

/** The finding as the map labels it, without the link's internal id. */
const FINDING = { ...MISPLACED, detail: MISPLACED.detail.replace(/^declared link [^:]+:/, "declared link:") };
import type { Navigate } from "../../live/backend";
import { Dock, Field, INK, Lift, M, PANEL_LEFT, PanelFrame, Stage2, cardStyle } from "../dressing";
import { ImportGraph, Term, win, type TLine } from "../figures";
import { NoTree } from "../world";

/** The Ledger map in the panel. */
const MAP = { x: 800, y: 480, s: 1.0 };

export const CHECKS2 = s(23);

const K = { clear: s(3.4), write: s(3.7), flagged: s(6.4), move: s(10.4), cleared: s(11.4), edit: s(13.6), stale: s(15.6), rerun: s(18.4), proven: s(19.8) };

const MONEY = "If two amounts differ in currency, then refuses to add them";

const W = [world({ violations: [] }), world({ violations: [FINDING] }), world({ violations: [], states: { [MONEY]: "stale" } })];

function at(f: number): { w: number; v: number; nav: Navigate } {
  const ledgerMap: Navigate = { view: "diagram", diagramFocus: nodeId("Ledger"), lens: null };
  const money: Navigate = { view: "wiki", page: nodeId("Ledger/domain/money"), lens: null };
  if (f < K.cleared) return { w: 1, v: 1, nav: ledgerMap };
  if (f < K.edit) return { w: 0, v: 2, nav: ledgerMap };
  if (f < K.proven) return { w: 2, v: 3, nav: money };
  return { w: 0, v: 4, nav: money };
}

const WRITE: TLine[] = [
  { at: K.write + s(0.3), kind: "tool", name: "Write", arg: "crates/ledger/src/domain/memory.rs" },
  { at: K.write + s(0.8), kind: "diff", rows: [{ op: "+", text: "use crate::application::store::EntryStore;" }] },
  { at: K.write + s(1.6), kind: "out", text: "domain → application: illegal in hexagonal", tone: "bad" },
];

const SIDE: TLine[] = [
  { at: K.move, kind: "tool", name: "git mv", arg: "memory.rs → infrastructure/" },
  { at: K.cleared, kind: "out", text: "0 findings", tone: "ok" },
  { at: K.edit, kind: "tool", name: "Edit", arg: "domain/money.rs" },
  { at: K.rerun, kind: "tool", name: "scryer · ingest_test_report", arg: "cargo.xml" },
  { at: K.proven, kind: "out", text: "1 re-proven", tone: "ok" },
];

export function Checks2({ f }: { f: number }) {
  const st = at(f);
  const term = win(f, K.write, K.flagged + s(0.2), s(0.6));
  const app = win(f, K.flagged, CHECKS2 + s(1), s(0.6));
  const side = win(f, K.move, CHECKS2 + s(1), s(0.4));
  const cam = (m: Partial<Record<"rows", Rect>>): Cam => {
    const view = MAP;
    const r = m.rows;
    // The whole row, its verdict lane at the right end included.
    const z = r ? Math.min(1.8, (PANEL_LEFT.w - 60) / (r.w + 60)) : 1.6;
    const rows = r ? { x: r.cx, y: r.cy + 60, s: z } : { x: 900, y: 400, s: z };
    // In on domain and the application layer it reaches into, where the map draws the finding; out again once it clears.
    return keyed<Cam>(f, { ...view, rx: 0, ry: 0 }, [
      { at: K.flagged + s(0.6), dur: s(1.6), x: 650, y: 380, s: 1.7 },
      { at: K.move, dur: s(1.0), ...view },
      { at: K.edit, dur: 1, ...rows },
    ]);
  };
  return (
    <Stage2>
      {/* The layers stay as the beat's ground beside the panel; the wrong import clears with the app's finding. */}
      <Field opacity={1 - 0.3 * p(f, K.clear, s(1.0), "move")}>
        <ImportGraph f={f} x0={mix(1220, 1400, p(f, K.clear, s(1.2), "move"))} x1={1800} fixAt={K.cleared} stillAt={K.clear} />
      </Field>
      <div style={{ position: "absolute", left: M, top: 420, zIndex: 6, ...cardStyle(f, K.clear) }}>
        <div style={{ fontFamily: SANS, fontSize: 112, fontWeight: 580, letterSpacing: "-0.045em", lineHeight: 1.02, color: INK, opacity: p(f, s(0.4), s(0.8)) }}>
          Keep the codebase
          <br />
          coherent.
        </div>
      </div>
      {term > 0 ? (
        <PanelFrame at={PANEL_LEFT} opacity={term}>
          <Term f={f} lines={WRITE} size={30} style={{ position: "absolute", left: 56, top: 64 }} />
        </PanelFrame>
      ) : null}
      {app > 0 ? (
        <div style={{ position: "absolute", inset: 0, opacity: app, transform: `translateY(${(1 - app) * 24}px)` }}>
          <Lift>
            <AppWindow
              f={f}
              theme={DARK}
              fixture={W[st.w]}
              version={st.v}
              nav={st.nav}
              cam={cam}
              frame={PANEL_LEFT}
              targets={{ rows: { sel: '[id^="resp-"]', all: true } }}
              overlay={(m) => (
                <>
                  {/* The verdict: amber while stale, green once the test runs again. */}
                  {f >= K.stale - s(0.4) && m.rows ? (
                    <div
                      style={{
                        position: "absolute",
                        left: m.rows.x + m.rows.w - 66,
                        top: m.rows.y - 2,
                        width: 58,
                        height: 30,
                        borderRadius: 16,
                        border: `2.5px solid ${f >= K.proven ? GREEN : "#F0B44C"}`,
                        opacity: p(f, f >= K.proven ? K.proven : K.stale - s(0.4), s(0.5)),
                        transform: `scale(${mix(1.5, 1, p(f, f >= K.proven ? K.proven : K.stale - s(0.4), s(0.5)))})`,
                      }}
                    />
                  ) : null}
                </>
              )}
            />
            <NoTree />
          </Lift>
        </div>
      ) : null}
      {side > 0 ? <Term f={f} lines={SIDE} size={30} keep={2} style={{ position: "absolute", right: 120, top: 872, zIndex: 6, textAlign: "right", opacity: side }} /> : null}
      <Dock
        f={f}
        at={K.flagged + s(0.4)}
        text="Keep the codebase coherent."
        subs={[
          { at: K.flagged, text: "Every import is checked against its layer." },
          { at: K.move, text: "It clears when the file moves." },
          { at: K.edit, text: "Change the code under a claim and it goes stale." },
          { at: K.rerun, text: "Run the test again and it's proven again." },
        ]}
      />
    </Stage2>
  );
}
