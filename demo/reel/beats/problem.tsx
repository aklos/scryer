/**
 * The problem, before any of the app: sessions editing in parallel faster
 * than anyone reads, the summary that comes back as an essay, and new code
 * added beside the old instead of replacing it.
 */

import { Caption, DARK, Stage, Statement, mix, p, s } from "../kit";
import { Terminal, type TLine } from "../Terminal";

export const PROBLEM = s(16.5);
const T = DARK;

const A = s(2.4);
const B = s(6.8);
const C = s(11.4);

const EDITS = [
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

/** A session's stream of edits: a tool line every few frames from `at`. */
function stream(at: number, seed: number, every: number): TLine[] {
  return Array.from({ length: 26 }, (_, i) => {
    const file = EDITS[(i * 5 + seed) % EDITS.length];
    const tool = i % 4 === 3 ? "Bash" : i % 3 === 0 ? "Write" : "Edit";
    return {
      at: at + i * every,
      kind: "tool" as const,
      name: tool,
      arg: tool === "Bash" ? (i % 2 ? "cargo test -p ledger" : "pnpm test") : file,
    };
  });
}

const SESSIONS = [
  { title: "session 1 — add partial refunds", lines: stream(A + 4, 0, 5) },
  { title: "session 2 — hold disputed funds", lines: stream(A + 10, 3, 6) },
  { title: "session 3 — retry failed webhooks", lines: stream(A + 7, 7, 5) },
];

const ESSAY: TLine[] = [
  { at: B + s(0.3), kind: "say", text: "Done! Here's a summary of what I changed." },
  {
    at: B + s(0.5),
    kind: "say",
    text: "I implemented partial refunds across the ledger and the gateway. The main change is in ledger.rs, where I added refund_partial_v2 to track the remaining amount, while keeping the original refund method in place for backwards compatibility with existing callers.",
  },
  {
    at: B + s(0.7),
    kind: "say",
    text: "To make testing easier I added an in-memory entry store under domain/, and updated the facade so new callers can opt into the v2 path. The gateway now forwards an optional amount field.",
  },
  {
    at: B + s(0.9),
    kind: "say",
    text: "While I was there, I also refactored the webhook retry logic, since refunds emit events, and tightened some types in the dashboard's refund form. Two tests are skipped for now because they depend on the settlement file format.",
  },
  {
    at: B + s(1.1),
    kind: "say",
    text: "The old refund path should be removed in a follow-up once all callers are migrated. Let me know if you'd like me to add a feature flag, more tests, or documentation for the new endpoint.",
  },
];

const DIFF: TLine[] = [
  { at: C + s(0.3), kind: "tool", name: "Edit", arg: "crates/ledger/src/application/ledger.rs" },
  {
    at: C + s(0.6),
    kind: "diff",
    rows: [
      { op: " ", text: "pub fn refund(&mut self, capture: &str, amount: Money)" },
      { op: " ", text: "    -> Result<RefundId, LedgerError> {" },
      { op: " ", text: "    // …still called from facade.rs" },
      { op: " ", text: "}" },
      { op: "+", text: "" },
      { op: "+", text: "// TODO: migrate callers, then remove refund()" },
      { op: "+", text: "pub fn refund_partial_v2(&mut self, capture: &str," },
      { op: "+", text: "    amount: Money, remaining: Option<Money>)" },
      { op: "+", text: "    -> Result<RefundId, LedgerError> {" },
    ],
  },
  { at: C + s(1.6), kind: "tool", name: "Write", arg: "crates/ledger/src/domain/memory.rs" },
  { at: C + s(1.9), kind: "diff", rows: [{ op: "+", text: "use crate::application::store::EntryStore;" }] },
];

export function Problem({ f }: { f: number }) {
  const fanOut = p(f, B - s(0.4), s(0.8), "move");
  const essayIn = p(f, B, s(0.8));
  const essayOut = p(f, C - s(0.3), s(0.6), "move");
  const diffIn = p(f, C, s(0.8));
  return (
    <Stage theme={T}>
      {f < B + s(0.6) &&
        SESSIONS.map((se, i) => {
          const t = p(f, A + i * 5, s(0.9));
          const lines = se.lines.filter((l) => f >= l.at).length;
          return (
            <Terminal
              key={i}
              f={f}
              theme={T}
              title={se.title}
              lines={se.lines}
              width={540}
              height={800}
              scroll={Math.max(0, lines - 18) * 31}
              style={{
                left: 140 + i * 580,
                top: 220,
                opacity: t * (1 - fanOut),
                transform: `translateY(${(1 - t) * 60 + fanOut * 40}px)`,
                filter: fanOut > 0 ? `blur(${fanOut * 8}px)` : undefined,
              }}
            />
          );
        })}
      {f >= B && f < C + s(0.4) && (
        <Terminal
          f={f}
          theme={T}
          title="session 1 — add partial refunds"
          lines={ESSAY}
          width={1100}
          height={800}
          scroll={mix(0, 260, p(f, B + s(1.6), s(3.0), "move"))}
          style={{
            left: 410,
            top: 220,
            opacity: essayIn * (1 - essayOut),
            transform: `translateY(${(1 - essayIn) * 40}px)`,
            filter: essayOut > 0 ? `blur(${essayOut * 8}px)` : undefined,
          }}
        />
      )}
      {f >= C && (
        <Terminal
          f={f}
          theme={T}
          title="session 1 — add partial refunds"
          lines={DIFF}
          width={1100}
          height={640}
          style={{ left: 410, top: 260, opacity: diffIn, transform: `translateY(${(1 - diffIn) * 40}px)` }}
        />
      )}
      <Statement f={f} theme={T} index="" text="Agents write faster than you can read." />
      <Caption f={f} theme={T} at={A + s(0.4)} until={B - s(0.2)}>
        Sessions run in parallel, each touching dozens of files.
      </Caption>
      <Caption f={f} theme={T} at={B + s(0.2)} until={C - s(0.2)}>
        What comes back is an essay. It doesn't give you a picture of the system.
      </Caption>
      <Caption f={f} theme={T} at={C + s(0.2)} until={PROBLEM}>
        And new code gets added next to the old instead of replacing it.
      </Caption>
    </Stage>
  );
}
