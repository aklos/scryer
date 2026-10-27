/**
 * Deterministic checks keep the codebase coherent. A container's declared
 * style is checked against the real import graph: a file placed in domain
 * that imports from application is flagged on the map, and the finding
 * clears only when the file moves. A claim's verdict is computed from the
 * test report and the code under it: edit the code and it goes stale until
 * the test runs again.
 */

import { AppWindow, type Cam, type Rect } from "../AppWindow";
import { Caption, LIGHT, Stage, Statement, keyed, p, s } from "../kit";
import { Terminal, type TLine } from "../Terminal";
import { MISPLACED, nodeId, world } from "../live/world";
import type { Navigate } from "../live/backend";

export const CHECKS = s(21);
const T = LIGHT;

const K = {
  enter: s(1.9),
  write: s(2.8),
  flagged: s(4.0),
  move: s(8.0),
  cleared: s(8.6),
  edit: s(11.4),
  stale: s(12.2),
  rerun: s(15.0),
  proven: s(16.4),
  sum: s(17.2),
};

const MONEY = "If two amounts differ in currency, then refuses to add them";

const W = [
  world({ violations: [] }),
  world({ violations: [MISPLACED] }),
  world({ violations: [], states: { [MONEY]: "stale" } }),
];

function at(f: number): { w: number; v: number; nav: Navigate } {
  const ledgerMap: Navigate = { view: "diagram", diagramFocus: nodeId("Ledger"), lens: null };
  const money: Navigate = { view: "wiki", page: nodeId("Ledger/domain/money"), lens: null };
  if (f < K.flagged) return { w: 0, v: 0, nav: ledgerMap };
  if (f < K.cleared) return { w: 1, v: 1, nav: ledgerMap };
  if (f < K.edit) return { w: 0, v: 2, nav: ledgerMap };
  if (f < K.stale) return { w: 0, v: 2, nav: money };
  if (f < K.proven) return { w: 2, v: 3, nav: money };
  return { w: 0, v: 4, nav: money };
}

const LINES: TLine[] = [
  { at: K.write, kind: "tool", name: "Write", arg: "crates/ledger/src/domain/memory.rs", done: K.write + s(0.5) },
  { at: K.write + s(0.6), kind: "diff", rows: [{ op: "+", text: "use crate::application::store::EntryStore;" }] },
  { at: K.flagged + s(0.4), kind: "out", text: "domain → application: illegal in hexagonal", tone: "bad" },
  { at: K.move - s(0.6), kind: "tool", name: "Bash", arg: "git mv …/domain/memory.rs …/infrastructure/", done: K.move },
  { at: K.cleared + s(0.2), kind: "out", text: "0 findings", tone: "ok" },
  { at: K.edit - s(0.4), kind: "tool", name: "Edit", arg: "crates/ledger/src/domain/money.rs", done: K.edit },
  { at: K.rerun - s(0.6), kind: "tool", name: "Bash", arg: "cargo test -p ledger", done: K.rerun },
  { at: K.rerun + s(0.2), kind: "tool", name: "scryer · ingest_test_report", arg: "target/junit/cargo.xml", done: K.proven },
  { at: K.proven + s(0.1), kind: "out", text: "1 re-proven", tone: "ok" },
];

export function Checks({ f }: { f: number }) {
  const st = at(f);
  const enter = p(f, K.enter, s(1.0));
  const cam = (m: Partial<Record<"cards" | "rows", Rect>>): Cam => {
    const rows = m.rows;
    const onRows = rows ? { x: rows.x + 500 / 1.45, y: rows.cy + 40, s: 1.45 } : { x: 900, y: 360, s: 1.4 };
    return keyed<Cam>(f, { x: 965, y: 523, s: 0.7, rx: 0, ry: 0 }, [
      { at: K.enter, dur: s(1.3), x: 965, y: 523, s: 0.98 },
      // In on domain and the application layer it reaches into.
      { at: K.flagged + s(0.2), dur: s(1.0), x: 965, y: 450, s: 1.6 },
      { at: K.cleared + s(0.8), dur: s(1.0), x: 965, y: 523, s: 1.0 },
      { at: K.stale - s(0.9), dur: s(0.9), ...onRows },
      { at: K.stale + s(1.0), dur: s(0.9), x: 520, y: 960, s: 1.55 },
      { at: K.rerun - s(0.3), dur: s(0.9), ...onRows },
      { at: K.sum, dur: s(1.0), x: 820, y: 420, s: 1.0 },
    ]);
  };
  const termIn = p(f, K.write - s(0.4), s(0.7));
  return (
    <Stage theme={T}>
      <AppWindow
        f={f}
        theme={T}
        fixture={W[st.w]}
        version={st.v}
        nav={st.nav}
        cam={cam}
        frame={{ x: 80, y: 200, w: 1100, h: 840 }}
        opacity={enter}
        targets={{ cards: { sel: ".react-flow__node", all: true }, rows: { sel: '[id^="resp-"]', all: true } }}
      />
      <Terminal
        f={f}
        theme={T}
        title="~/aperture-pay — your agent"
        lines={LINES}
        width={620}
        height={840}
        style={{ left: 1220, top: 200, opacity: termIn, transform: `translateY(${(1 - termIn) * 40}px)` }}
      />
      <Statement f={f} theme={T} index="04" text="Deterministic checks keep the codebase coherent." />
      <Caption f={f} theme={T} at={K.write} until={K.move - s(0.6)}>
        Each container declares its architecture style. Every import is checked against it.
      </Caption>
      <Caption f={f} theme={T} at={K.move - s(0.4)} until={K.edit - s(0.6)}>
        The finding stays until the code moves. No rule gets loosened to make it pass.
      </Caption>
      <Caption f={f} theme={T} at={K.edit - s(0.4)} until={K.sum - s(0.2)}>
        Change the code under a claim and its verdict goes stale until the test runs again.
      </Caption>
      <Caption f={f} theme={T} at={K.sum} until={CHECKS}>
        Planned, proven, stale: computed from tests and imports, never from what the agent says.
      </Caption>
    </Stage>
  );
}
