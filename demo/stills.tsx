/**
 * Untreated stills — lifted pages rendered plain on the fixtures, for
 * eyeballing styling in `shoot.mjs` (`#session`, `#review`, `#page`). Not part
 * of the trailer timeline.
 */

import type { ReactNode } from "react";
import { ChangesPage, NeedsReviewPage, SessionPage, buildReviewIndex } from "../src/pages";
import { NodePage } from "../src/pages/node/NodePage";
import { planDiff } from "../src/entities/model/planDiff";
import { elementKey } from "../src/entities/model/ledger";
import type { Editor } from "../src/entities/model/editor";
import type { SessionLog } from "../src/features/session-log/useSessionLog";
import type { ScryModel } from "../src/entities/model/viewmodel";
import { committedModel, driftModel, driftScopes, healthReport, newRespIds, paymentsModel } from "./fixtures";

const noop = () => {};
const EMPTY = new Set<string>();
// Every editor method is a no-op so action buttons render.
const editor = new Proxy({}, { get: () => noop }) as unknown as Editor;

const pendingModel: ScryModel = (() => {
  const m = JSON.parse(JSON.stringify(driftModel)) as ScryModel;
  const wh = m.nodes.find((n) => n.id === "webhooks")!;
  const r = wh.responsibilities!.find((x) => x.id === "r-wh-2")!;
  r.vagrant = true;
  r.lastTouchedAt = 1_700_000_500;
  const ledger = m.nodes.find((n) => n.id === "ledger")!;
  const stale = ledger.responsibilities!.find((x) => x.id === "r-ledger-2")!;
  stale.staleProposal = "**While** settlement is unconfirmed, **hold** the captured funds in a pending-settlement account";
  m.changes = [
    { id: "chg-1", rationale: "Refund support for captured payments", createdAt: 1_700_000_000 },
    { id: "chg-2", rationale: "Harden webhook retries", createdAt: 1_700_000_100 },
  ];
  m.changeMap = { [elementKey("responsibility", "webhooks", "r-wh-2")]: "chg-2" };
  m.sourceMap = {
    ...(m.sourceMap ?? {}),
    "r-ledger-2": [{ pattern: "ledger/src/escrow.rs", symbol: "hold_in_escrow" }],
    "r-wh-2": [{ pattern: "webhooks/retry.go", symbol: "Retry" }],
    "r-fraud-1": [{ pattern: "fraud/scoring/score.py", symbol: "score" }],
    "r-fraud-vagrant": [{ pattern: "fraud/scoring/cache.py", symbol: "cached_score" }],
  };
  m.testMap = {
    ...(m.testMap ?? {}),
    "r-fraud-1": [{ pattern: "fraud/tests/test_score.py", symbol: "test_scores_within_window" }],
    "r-ledger-1": [{ pattern: "ledger/tests/posting.rs", symbol: "balanced_double_entry" }],
  };
  return m;
})();

// One session: a delivered build ask, an open one, a descoped one, an
// unbroken follow-up prompt, and a file nobody asked for.
const sessionLog: SessionLog = {
  sessions: [
    { session: "7f3a9c2e1b", updatedAt: 1_700_000_700, firstPrompt: "Add refunds for captured payments" },
    { session: "2c81d0aa94", updatedAt: 1_699_990_000, firstPrompt: "Harden webhook retries" },
  ],
  selectedId: "7f3a9c2e1b",
  select: noop,
  badge: 2,
  live: true,
  view: {
    session: "7f3a9c2e1b",
    startedAt: 1_700_000_000,
    updatedAt: 1_700_000_700,
    prompts: [
      { id: "p1", text: "Add refunds for captured payments. Partial refunds too, and keep the ledger balanced — every refund needs its reversing entry." },
      { id: "p2", text: "Also, why does the webhook retry twice?" },
    ],
    unfiled: ["p2"],
    asks: [
      { id: "a1", prompt: "p1", text: "Refund a captured payment", kind: "build", claims: ["r-ledger-1"], status: "delivered" },
      { id: "a2", prompt: "p1", text: "Partial refunds", kind: "build", claims: [], status: "open", missing: ["no claim covers a partial amount"] },
      { id: "a3", prompt: "p1", text: "Refund to a different card", kind: "build", claims: [], status: "descoped", reason: "the processor only refunds to the original instrument" },
    ],
    touched: [
      { file: "ledger/src/escrow.rs", claims: [["r-ledger-2", "hold the captured funds"]] },
      { file: "webhooks/retry.go", claims: [] },
    ],
    untraced: ["webhooks/retry.go"],
    modelEdits: ["resp:r-ledger-1", "node:ledger", "resp:r-gone"],
  },
};

const SessionStill = () => (
  <div className="flex h-screen w-screen bg-[var(--surface)]">
    <SessionPage model={pendingModel} log={sessionLog} onSelectNode={noop} onSelectGroup={noop} />
  </div>
);

const ReviewStill = () => (
  <div className="flex h-screen w-screen bg-[var(--surface)]">
    <NeedsReviewPage
      model={driftModel}
      report={healthReport}
      driftScopes={driftScopes}
      newNodeIds={EMPTY}
      newRespIds={newRespIds}
      editor={editor}
      onSelectNode={noop}
      onClearAllNew={noop}
    />
  </div>
);

const PageStill = () => (
  <div className="flex h-screen w-screen bg-[var(--surface)]">
    <NodePage
      testVerdicts={{}}
      probeResults={{}}
      preview={{ status: "error", url: null, components: null, error: null }}
      model={paymentsModel}
      committed={committedModel}
      selected={{ kind: "node", id: "ledger" }}
      report={healthReport}
      projectPath={null}
      editor={editor}
      onSelectNode={noop}
      onSelectGroup={noop}
      variationState={null}
      changeLog={[]}
      history={[]}
      driftScopes={[]}
    />
  </div>
);

void buildReviewIndex;

const ChangesStill = () => (
  <div className="flex h-screen w-screen bg-[var(--surface)]">
    <ChangesPage
      planDiff={planDiff(committedModel, pendingModel)}
      model={pendingModel}
      committed={committedModel}
      changeLog={[]}
      onSelectNode={noop}
      activeChange="chg-1"
      onSetActiveChange={noop}
      onCloseChange={() => Promise.reject(new Error("chg-2 still has 1 tagged entry — fold or revert it"))}
    />
  </div>
);

export const stills: Record<string, () => ReactNode> = {
  changes: () => <ChangesStill />,
  session: () => <SessionStill />,
  review: () => <ReviewStill />,
  page: () => <PageStill />,
};
