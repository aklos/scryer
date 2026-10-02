/**
 * Untreated stills — lifted pages rendered plain on the fixtures, for
 * eyeballing styling in `shoot.mjs` (`#session`, `#changes`, `#page`). Not part
 * of the trailer timeline.
 */

import type { ReactNode } from "react";
import { ChangesPage, SessionPage } from "../src/pages";
import { NodePage } from "../src/pages/node/NodePage";
import { planDiff } from "../src/entities/model/planDiff";
import type { Editor } from "../src/entities/model/editor";
import type { SessionLog } from "../src/features/session-log/useSessionLog";
import type { ScryModel } from "../src/entities/model/viewmodel";
import { committedModel, driftModel, healthReport, paymentsModel } from "./fixtures";

const noop = () => {};
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

// One session: files touched and the claims they reached, one plan entry
// left unbuilt with its progress note, and the plan elements it wrote.
const sessionLog: SessionLog = {
  sessions: [
    { session: "7f3a9c2e1b", updatedAt: 1_700_000_700 },
    { session: "2c81d0aa94", updatedAt: 1_699_990_000 },
  ],
  selectedId: "7f3a9c2e1b",
  select: noop,
  badge: 1,
  live: true,
  view: {
    session: "7f3a9c2e1b",
    startedAt: 1_700_000_000,
    updatedAt: 1_700_000_700,
    touched: [
      { file: "ledger/src/escrow.rs", claims: [["r-ledger-2", "hold the captured funds"]] },
      { file: "webhooks/retry.go", claims: [] },
    ],
    unfolded: [{ key: "resp:r-ledger-1", label: "Refund a captured payment", note: "full refunds built; partial amounts left" }],
    modelEdits: ["resp:r-ledger-1", "node:ledger", "resp:r-gone"],
  },
};

const SessionStill = () => (
  <div className="flex h-screen w-screen bg-[var(--surface)]">
    <SessionPage model={pendingModel} log={sessionLog} onSelectNode={noop} onSelectGroup={noop} />
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
      changeLog={[]}
      history={[]}
      driftScopes={[]}
    />
  </div>
);

const ChangesStill = () => (
  <div className="flex h-screen w-screen bg-[var(--surface)]">
    <ChangesPage
      planDiff={planDiff(committedModel, pendingModel)}
      model={pendingModel}
      committed={committedModel}
      changeLog={[]}
      onSelectNode={noop}
    />
  </div>
);

export const stills: Record<string, () => ReactNode> = {
  changes: () => <ChangesStill />,
  session: () => <SessionStill />,
  page: () => <PageStill />,
};
