/**
 * Shot 5 — proven, one by one. The four refund claims on Ledger at 2.5×; as
 * `cargo test` reports each, its row turns from planned to proven and the
 * test chip ticks up. Props: how many have reported.
 */

import { ClaimRows, TestChip, claim, nodeId } from "./app";
import { Fragment, Headline, Stage, Terminal } from "./stage";

const FOUR = [
  { text: "When a merchant refunds part of a capture, posts the refund against it", test: "refunds_part_of_a_capture" },
  { text: "When a refund exceeds the captured amount, rejects it", test: "rejects_over_refund" },
  { text: "Tracks the refundable remainder of each capture", test: "tracks_refundable_remainder" },
  { text: "Refunds in the currency of the capture", test: "refunds_in_capture_currency" },
];

/** Tested claims before the run: the four still to prove are the gap. */
const BEFORE = 64;

export function Shot5({ reported }: { reported: number }) {
  return (
    <Stage light={{ x: 960, y: 480 }}>
      <Headline at={{ x: 120, y: 84 }}>Four claims. Four tests. Proven.</Headline>
      <Fragment at={{ x: 120, y: 232 }} width={700} height={180} scale={2.4}>
        <div className="dark shot-grade shot-claims h-full bg-[var(--surface)] px-4 pt-2.5 text-[var(--text)]">
          <div className="flex items-center justify-between pb-1.5 pl-1">
            <span className="text-[13px] font-semibold text-[var(--text)]">Ledger</span>
            <span style={{ display: "inline-flex", transform: "scale(1.2)", transformOrigin: "right center" }}>
              <TestChip covered={BEFORE + reported} testable={BEFORE + FOUR.length} />
            </span>
          </div>
          <ClaimRows
            host={nodeId("Ledger")}
            rows={FOUR.map((f, i) => ({ claim: claim(f.text), state: i < reported ? "proven" : "planned" }))}
          />
        </div>
      </Fragment>
      <Terminal
        at={{ x: 136, y: 740 }}
        lines={[
          { kind: "run", text: "cargo test --test refunds" },
          { kind: "result", items: FOUR.slice(0, 2).map((f, i) => ({ name: f.test, ok: i < reported ? true : undefined })) },
          { kind: "result", items: FOUR.slice(2).map((f, i) => ({ name: f.test, ok: i + 2 < reported ? true : undefined })) },
        ]}
      />
    </Stage>
  );
}
