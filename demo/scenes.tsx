/**
 * Scene registry + timeline for the trailer.
 *
 * Each scene lifts a real app component, renders it on the curated fixtures,
 * and wraps it in the kinetic treatment (push-in + sliding headline). The cold
 * open and close are custom title/end cards. `timeline` is the ordered beat
 * list with per-beat durations the runner plays through.
 */

import type { ReactNode } from "react";
import { DiagramView } from "../src/DiagramView";
import { NodePage } from "../src/NodePage";
import { NeedsReviewPage } from "../src/SpecialPages";
import { Powerline } from "../src/Powerline";
import { buildReviewIndex } from "../src/SpecialPages";
import { EMPTY_DIFF } from "../src/planDiff";
import type { AgentSession } from "../src/hooks/useAgentSession";
import type { ModelBuild } from "../src/hooks/useModelBuild";
import type { ResolvedLaunch } from "../src/SettingsPanel";
import { Treated } from "./treatment";
import {
  paymentsModel,
  committedModel,
  driftModel,
  driftScopes,
  newRespIds,
  healthReport,
} from "./fixtures";

const noop = () => {};
const EMPTY = new Set<string>();

export interface Scene {
  id: string;
  /** Hold time on screen, ms. */
  duration: number;
  render: () => ReactNode;
}

// --- Powerline supporting fixtures ------------------------------------------

const agentSession: AgentSession = {
  running: true,
  label: "Filling Ledger Service",
  lastTool: "add_responsibility",
  activity: 'add_responsibility · "Hold funds in escrow until settlement"',
  startFixture: noop,
  startVariation: noop,
  cancel: noop,
};

const modelBuild: ModelBuild = {
  building: false,
  checking: false,
  active: false,
  phase: "Ledger Service",
  activeNodeIds: new Set(["ledger"]),
  activity: null,
  start: async () => {},
  checkDrift: async () => {},
  cancel: noop,
};

const launch: ResolvedLaunch = { agent: "claudeCode", model: "claude-opus-4-8", effort: "high" };

// --- Lifted-component scene bodies -------------------------------------------

const Diagram = () => (
  <DiagramView
    model={paymentsModel}
    planDiff={EMPTY_DIFF}
    report={healthReport}
    focusId="aperture"
    selectedId={null}
    onFocus={noop}
    onSelectNode={noop}
  />
);

const NodeBody = () => (
  <NodePage
    model={paymentsModel}
    committed={committedModel}
    selected={{ kind: "node", id: "ledger" }}
    report={healthReport}
    projectPath={null}
    editor={undefined}
    onSelectNode={noop}
    onSelectGroup={noop}
    variationState={null}
    newRespIds={EMPTY}
    onClearNewResp={noop}
    changeLog={[]}
    history={[]}
    driftScopes={[]}
  />
);

const DriftBody = () => (
  <NeedsReviewPage
    model={driftModel}
    report={healthReport}
    driftScopes={driftScopes}
    newNodeIds={EMPTY}
    newRespIds={newRespIds}
    editor={undefined}
    onSelectNode={noop}
    onClearAllNew={noop}
  />
);

const PowerlineBody = () => {
  const reviewIndex = buildReviewIndex(driftModel, healthReport, driftScopes, EMPTY, newRespIds);
  return (
    <div className="flex h-screen w-screen flex-col justify-end bg-[var(--surface-canvas)]">
      <Powerline
        model={driftModel}
        agent={agentSession}
        build={modelBuild}
        reviewIndex={reviewIndex}
        health={healthReport}
        launch={launch}
        onOpenSpecial={noop}
        onOpenSettings={noop}
      />
    </div>
  );
};

// --- Title / end cards -------------------------------------------------------

const Brand = () => (
  <span className="kbrand">
    <img className="klogo" src="/logo.png" alt="" />
    scryer
  </span>
);

const ColdOpen = () => (
  <div className="kscene">
    <div className="kbackdrop">
      <Diagram />
    </div>
    <div className="kcenter">
      <Brand />
      <p className="khook">
        Your agent writes the code.
        <br />
        What keeps the plan honest?
      </p>
    </div>
  </div>
);

const Close = () => (
  <div className="kscene">
    <div className="kcenter">
      <Brand />
      <p className="ktag">MDD for AI agents</p>
      <p className="kurl">github.com/aklos/scryer</p>
    </div>
  </div>
);

// --- Timeline ----------------------------------------------------------------
// Ordered: what it is → who fills it → how you watch it. Durations are the hold
// time per beat (ms) and are the main knob for total length (~60s here).

export const timeline: Scene[] = [
  { id: "cold", duration: 5500, render: () => <ColdOpen /> },
  {
    id: "node",
    duration: 10000,
    render: () => (
      <Treated headline="A spec you and your agent build from." origin="50% 16%" zoom={[1.0, 1.05]}>
        <NodeBody />
      </Treated>
    ),
  },
  {
    id: "powerline",
    duration: 8000,
    render: () => (
      <Treated headline="Your agent edits the model through MCP." placement="center" zoom={[1.0, 1.0]} scrim={false}>
        <PowerlineBody />
      </Treated>
    ),
  },
  {
    id: "diagram",
    duration: 9000,
    render: () => (
      <Treated headline="The whole architecture, at a glance.">
        <Diagram />
      </Treated>
    ),
  },
  {
    id: "drift",
    duration: 10000,
    render: () => (
      <Treated headline="Know the moment code and intent diverge." origin="50% 16%" zoom={[1.0, 1.05]}>
        <DriftBody />
      </Treated>
    ),
  },
  { id: "close", duration: 6500, render: () => <Close /> },
];

export const sceneById = (id: string | null): Scene =>
  timeline.find((s) => s.id === id) ?? timeline[0];
