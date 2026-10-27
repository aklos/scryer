/**
 * Plan a feature as claims, then watch them get proven: the agent writes
 * five todo tests and they appear on Ledger's page as planned claims; one is
 * cut; the code lands, the tests run, and the report turns the rows proven;
 * then a probe breaks the code to see whether a green test notices.
 */

import { AppWindow, type Box, type Cam, type Rect } from "../AppWindow";
import { Caption, LIGHT, Stage, Statement, keyed, p, s } from "../kit";
import { Terminal, type TLine } from "../Terminal";
import { REFUND_STUBS, nodeId, world } from "../live/world";
import type { Navigate } from "../live/backend";

export const PLAN = s(27);
const T = LIGHT;

const K = {
  enter: s(1.9),
  ask: s(2.4),
  stubs: s(5.4),
  cut: s(10.0),
  build: s(10.6),
  ingest: s(15.0),
  probe: s(18.4),
  survived: s(20.8),
  caught: s(24.2),
};

const REFUNDS = REFUND_STUBS.slice(0, 4).map((c) => c.description);

// Each stage of the work as the app sees it: Ledger without refunds yet,
// with the five planned claims, with four, then proven, then probed.
const W = [
  world({ drop: REFUNDS }),
  world({ drop: REFUNDS, add: REFUND_STUBS.map((claim) => ({ claim })) }),
  world({ drop: REFUNDS, add: REFUND_STUBS.slice(0, 4).map((claim) => ({ claim })) }),
  world({ drop: REFUNDS, add: REFUND_STUBS.slice(0, 4).map((claim) => ({ claim, state: "proven" as const })) }),
  world({ drop: REFUNDS, add: REFUND_STUBS.slice(0, 4).map((claim, i) => ({ claim, state: "proven" as const, probe: i === 1 ? ("crosshair" as const) : undefined })) }),
  world({ drop: REFUNDS, add: REFUND_STUBS.slice(0, 4).map((claim, i) => ({ claim, state: "proven" as const, probe: i === 1 ? ("doubleCheck" as const) : undefined })) }),
];

function version(f: number): number {
  if (f >= K.caught) return 5;
  if (f >= K.survived) return 4;
  if (f >= K.ingest) return 3;
  if (f >= K.cut) return 2;
  if (f >= K.stubs) return 1;
  return 0;
}

/** The app beside the terminal. */
const FRAME: Box = { x: 780, y: 200, w: 1060, h: 840 };
const NAV: Navigate = { view: "wiki", page: nodeId("Ledger"), lens: null };

const LINES: TLine[] = [
  { at: K.ask, kind: "user", text: "add partial refunds to the ledger" },
  { at: K.ask + s(1.2), kind: "tool", name: "scryer · orient", arg: "ledger/src/domain/refund.rs", done: K.ask + s(1.7) },
  { at: K.ask + s(1.8), kind: "out", text: "Ledger › domain › refund" },
  { at: K.ask + s(2.0), kind: "out", text: "stubs → crates/ledger/tests/refunds.rs" },
  { at: K.stubs - s(0.5), kind: "tool", name: "Write", arg: "crates/ledger/tests/refunds.rs", done: K.stubs },
  {
    at: K.stubs - s(0.3),
    kind: "diff",
    rows: [
      { op: "+", text: "/// When a refund exceeds the captured amount, rejects it" },
      { op: "+", text: "#[test] #[ignore]" },
      { op: "+", text: "fn rejects_over_refund() { todo!() }" },
    ],
  },
  { at: K.stubs + s(0.5), kind: "say", text: "Planned as five todo tests. Have a look before I build." },
  { at: K.cut - s(1.4), kind: "user", text: "drop the notify one, then go", cps: 30 },
  { at: K.build, kind: "tool", name: "Edit", arg: "crates/ledger/src/domain/refund.rs" },
  { at: K.build + s(0.5), kind: "tool", name: "Edit", arg: "crates/ledger/src/application/ledger.rs" },
  { at: K.build + s(1.0), kind: "tool", name: "Bash", arg: "cargo test --test refunds" },
  { at: K.build + s(1.4), kind: "tests", names: REFUND_STUBS.slice(0, 4).map((c) => c.identifier), every: s(0.55) },
  { at: K.ingest - s(0.5), kind: "tool", name: "scryer · ingest_test_report", arg: "target/junit/cargo.xml", done: K.ingest },
  { at: K.ingest + s(0.1), kind: "out", text: "4 proven", tone: "ok" },
  { at: K.probe, kind: "tool", name: "scryer · open_probe", arg: "rejects_over_refund", done: K.probe + s(0.4) },
  { at: K.probe + s(0.5), kind: "out", text: "worktree ~/.scryer/probes/aperture-pay-3f1c" },
  {
    at: K.probe + s(0.9),
    kind: "diff",
    rows: [
      { op: "-", text: "if amount.minor > remaining.minor {" },
      { op: "+", text: "if amount.minor > remaining.minor * 2 {" },
    ],
  },
  { at: K.probe + s(1.5), kind: "tests", names: ["rejects_over_refund"], every: s(0.5) },
  { at: K.survived - s(0.3), kind: "tool", name: "scryer · close_probe", done: K.survived },
  { at: K.survived, kind: "out", text: "1 break tried, 1 survived", tone: "bad" },
  { at: K.survived + s(0.8), kind: "tool", name: "Edit", arg: "crates/ledger/tests/refunds.rs" },
  { at: K.survived + s(1.1), kind: "diff", rows: [{ op: "+", text: 'assert!(ledger.refund("cap-1", gbp(1001)).is_err());' }] },
  { at: K.caught - s(0.9), kind: "tool", name: "scryer · open_probe", arg: "rejects_over_refund", done: K.caught - s(0.5) },
  { at: K.caught - s(0.3), kind: "tool", name: "scryer · close_probe", done: K.caught },
  { at: K.caught, kind: "out", text: "1 break tried, 0 survived", tone: "ok" },
];

export function Plan({ f }: { f: number }) {
  const enter = p(f, K.enter, s(1.0));
  // The terminal scrolls as it fills.
  const scroll = keyed<{ y: number }>(f, { y: 0 }, [
    { at: K.build + s(0.6), dur: s(1.0), y: 260 },
    { at: K.probe - s(0.4), dur: s(1.0), y: 600 },
    { at: K.survived + s(0.6), dur: s(1.0), y: 760 },
  ]).y;
  const cam = (at: Partial<Record<"rows" | "row2" | "status", Rect>>): Cam => {
    const rows = at.rows;
    const row2 = at.row2;
    // Rows' left edge just inside the frame's.
    const inset = FRAME.w / 2 - 36;
    const sRows = rows ? Math.min(1.25, 760 / rows.h) : 1.1;
    const onRows = rows ? { x: rows.x + inset / sRows, y: rows.cy, s: sRows } : { x: 900, y: 450, s: 1.1 };
    // The probed row whole, its verdict lane at the right end included.
    const sRow = row2 ? Math.min(1.3, (FRAME.w - 60) / row2.w) : 1.1;
    const onRow = row2 ? { x: row2.x + inset / sRow, y: row2.cy, s: sRow } : onRows;
    return keyed<Cam>(f, { x: 900, y: 460, s: 0.7, rx: 0, ry: 0 }, [
      { at: K.enter, dur: s(1.3), x: 900, y: 460, s: 0.84, rx: 0, ry: 0 },
      { at: K.stubs, dur: s(1.0), ...onRows },
      { at: K.ingest + s(0.4), dur: s(0.9), x: 420, y: 960, s: 1.5 },
      { at: K.ingest + s(2.0), dur: s(0.9), ...onRows },
      { at: K.survived - s(0.6), dur: s(1.0), ...onRow },
    ]);
  };
  return (
    <Stage theme={T}>
      <AppWindow
        f={f}
        theme={T}
        fixture={W[version(f)]}
        version={version(f)}
        nav={NAV}
        cam={cam}
        frame={FRAME}
        opacity={enter}
        targets={{
          rows: { sel: '[id^="resp-"]', all: true },
          row2: { sel: "#resp-c-ref-2" },
          status: { sel: ".pl" },
        }}
        overlay={(at) => {
          const row = at.row2;
          if (!row || f < K.survived) return null;
          const caught = f >= K.caught;
          const pulse = p(f, caught ? K.caught : K.survived, s(0.5));
          const fade = 1 - p(f, (caught ? K.caught : K.survived) + s(1.6), s(0.5));
          return (
            <div
              style={{
                position: "absolute",
                left: row.x + row.w - 76,
                top: row.y + 2,
                width: 56,
                height: 30,
                borderRadius: 16,
                border: `2.5px solid ${caught ? "#22A06B" : "#E5484D"}`,
                transform: `scale(${1.6 - 0.6 * pulse})`,
                opacity: pulse * fade,
              }}
            />
          );
        }}
      />
      <Terminal
        f={f}
        theme={T}
        title="~/aperture-pay — your agent"
        lines={LINES}
        width={660}
        height={840}
        scroll={scroll}
        style={{ left: 80, top: 200, opacity: enter, transform: `translateY(${(1 - enter) * 40}px)` }}
      />
      <Statement f={f} theme={T} index="02" text="Plan features as claims, then watch them get proven." />
      <Caption f={f} theme={T} at={K.stubs - s(0.4)} until={K.cut - s(1.6)}>
        The plan is a list of claims: one sentence each, each one a todo test in the repo.
      </Caption>
      <Caption f={f} theme={T} at={K.cut - s(1.4)} until={K.build + s(1.0)}>
        Cut what you don't want before any code is written.
      </Caption>
      <Caption f={f} theme={T} at={K.build + s(1.2)} until={K.probe - s(0.2)}>
        Rows turn proven when the test report comes in. Nothing is marked by hand.
      </Caption>
      <Caption f={f} theme={T} at={K.probe} until={PLAN}>
        A probe breaks the code on purpose. If the test stays green, it gets tightened.
      </Caption>
    </Stage>
  );
}
