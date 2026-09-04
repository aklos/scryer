/**
 * Untreated stills — lifted pages rendered plain on the fixtures, for
 * eyeballing styling in `shoot.mjs` (`#inbox`, `#review`, `#page`). Not part
 * of the trailer timeline.
 */

import type { ReactNode } from "react";
import { ChangesPage, InboxPage, NeedsReviewPage, buildReviewIndex } from "../src/SpecialPages";
import { NodePage } from "../src/NodePage";
import { buildInboxCards } from "../src/inbox";
import { planDiff } from "../src/planDiff";
import { elementKey } from "../src/ledger";
import type { Editor } from "../src/editor";
import type { Inbox } from "../src/hooks/useInbox";
import type { ScryModel } from "../src/viewmodel";
import { committedModel, driftModel, driftScopes, healthReport, newRespIds, paymentsModel } from "./fixtures";

const noop = () => {};
const EMPTY = new Set<string>();
// Every editor method is a no-op so action buttons render.
const editor = new Proxy({}, { get: () => noop }) as unknown as Editor;

const inboxModel: ScryModel = (() => {
  const m = JSON.parse(JSON.stringify(driftModel)) as ScryModel;
  const wh = m.nodes.find((n) => n.id === "webhooks")!;
  const r = wh.responsibilities!.find((x) => x.id === "r-wh-2")!;
  r.vagrant = true;
  r.vagrantOrigin = "amendment";
  r.approvedStatement = "**If** a delivery fails, **then** retry with exponential backoff for up to 12 hours";
  r.lastTouchedAt = 1_700_000_500;
  const ledger = m.nodes.find((n) => n.id === "ledger")!;
  const stale = ledger.responsibilities!.find((x) => x.id === "r-ledger-2")!;
  stale.staleProposal = "**While** settlement is unconfirmed, **hold** the captured funds in a pending-settlement account";
  m.changes = [
    { id: "chg-1", rationale: "Refund support for captured payments", createdAt: 1_700_000_000 },
    { id: "chg-2", rationale: "Harden webhook retries", createdAt: 1_700_000_100, signedOff: { at: 1_700_000_200, entries: {} } },
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

const cards = buildInboxCards({
  model: inboxModel,
  committed: committedModel,
  planDiff: planDiff(committedModel, inboxModel),
  verdicts: {
    "r-ledger-1": { respId: "r-ledger-1", outcome: "failed", cases: 4, stale: false, recordedAt: 1_700_000_600 },
    "r-fraud-1": { respId: "r-fraud-1", outcome: "passed", cases: 2, stale: false, recordedAt: 1_700_000_300 },
  },
  probes: {
    "r-fraud-1": { respId: "r-fraud-1", probes: 3, survived: 1, survivors: ["removed the threshold comparison"], stale: false, recordedAt: 1_700_000_400 },
  },
  refusals: [
    { respId: "r-auth-1", hostId: "auth", kind: "no-test", reason: "no attached test", at: 1_700_000_200 } as never,
  ],
  closeGate: [
    { session: "7f3a9c2e1b", file: "auth/internal/session.go", symbol: "Refresh", id: "auth", respId: "r-auth-2", at: 1_700_000_700 } as never,
  ],
  now: 1_700_001_000,
});

function InboxStill({ live }: { live: boolean }) {
  const inbox: Inbox = {
    cards,
    unread: 0,
    live,
    seen: EMPTY,
    markSeen: noop,
    dismiss: noop,
    pinnedChange: null,
    setPinnedChange: noop,
  };
  return (
    <div className="flex h-screen w-screen bg-[var(--surface)]">
      <InboxPage model={inboxModel} inbox={inbox} editor={editor} onSelectNode={noop} onSelectGroup={noop} />
    </div>
  );
}

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
      planDiff={planDiff(committedModel, inboxModel)}
      model={inboxModel}
      committed={committedModel}
      changeLog={[]}
      onSelectNode={noop}
      activeChange="chg-1"
      onSetActiveChange={noop}
      onCloseChange={() => Promise.reject(new Error("chg-2 still has 1 tagged entry — fold or revert it"))}
      onSignOffChange={noop}
    />
  </div>
);

export const stills: Record<string, () => ReactNode> = {
  changes: () => <ChangesStill />,
  inbox: () => <InboxStill live />,
  review: () => <ReviewStill />,
  page: () => <PageStill />,
};
