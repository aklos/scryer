/**
 * The real app shell, demo-owned.
 *
 * The product's `Workspace` (src/App.tsx) is welded to disk-backed hooks
 * (`useModelStorage`, `useAgentSession`, `useModelHealth`, …) and isn't
 * exported, so we can't lift it directly. Instead we recompose the SAME real
 * child components — `TopBar`, `ModelTree`, `DiagramView`, `NodePage`,
 * `SpecialPages`, `Powerline` — around a plain state object the film director
 * can drive. Same pixels as the product; full control for the camera script.
 *
 * `WorkspaceShell` is a pure state→UI render (mirrors `Workspace`'s JSX).
 * `WorkspaceState` is everything a scene mutates over a take: the model and its
 * committed base (their diff paints the tree gutter), the current view +
 * selection, and the agent/build status the Powerline reflects.
 */

import { useMemo } from "react";
import { TopBar, type WorkspaceView } from "../../src/TopBar";
import { ModelTree } from "../../src/ModelTree";
import { DiagramView } from "../../src/DiagramView";
import { NodePage, type Selected } from "../../src/NodePage";
import type { Editor } from "../../src/editor";
import {
  buildReviewIndex,
  DarkCodePage,
  NeedsReviewPage,
  RecentChangesPage,
  UnmappedClaimsPage,
} from "../../src/SpecialPages";
import { Powerline } from "../../src/Powerline";
import { planDiff } from "../../src/planDiff";
import type { ScryModel, DriftScope } from "../../src/viewmodel";
import type { ModelHealthReport } from "../../src/health";
import type { ModelBuild } from "../../src/hooks/useModelBuild";
import type { AgentSession } from "../../src/hooks/useAgentSession";
import type { ChangeRevision } from "../../src/hooks/useModelStorage";
import type { ResolvedLaunch } from "../../src/SettingsPanel";

const EMPTY_IDS: ReadonlySet<string> = new Set();
const noop = () => {};

/** The animated barber-pole used as a diagram card's "generating" fill — a
 *  hidden SVG `<pattern>` that `DiagramCard`'s pending overlay references via
 *  `fill: url(#barber-gen)`. Self-animating (SMIL shifts the stripes), matching
 *  the powerline's violet barber. Rendered once per shell. */
function BarberPattern() {
  return (
    <svg width="0" height="0" aria-hidden="true" className="absolute">
      <defs>
        <pattern
          id="barber-gen"
          patternUnits="userSpaceOnUse"
          width="25.456"
          height="40"
          patternTransform="rotate(45)"
        >
          <rect width="25.456" height="40" fill="transparent" />
          <rect width="12.728" height="40" fill="var(--color-violet-500)" fillOpacity="0.32" />
          <animate attributeName="x" values="0;25.456" dur="0.7s" repeatCount="indefinite" />
        </pattern>
      </defs>
    </svg>
  );
}

/** Idle agent/build defaults — overridden per beat for the build + drift acts. */
export const IDLE_AGENT: AgentSession = {
  running: false,
  label: "",
  lastTool: null,
  activity: null,
  startFixture: noop,
  startVariation: noop,
  cancel: noop,
};

export const IDLE_BUILD: ModelBuild = {
  building: false,
  checking: false,
  active: false,
  phase: null,
  activeNodeIds: EMPTY_IDS,
  activity: null,
  start: async () => {},
  checkDrift: async () => {},
  cancel: noop,
};

export const DEMO_LAUNCH: ResolvedLaunch = {
  agent: "claudeCode",
  model: "claude-opus-4-8",
  effort: "high",
};

/** The whole filmable state of the shell. A scene seeds it, then mutates it. */
export interface WorkspaceState {
  model: ScryModel;
  committed: ScryModel | null;
  projectPath: string | null;
  view: WorkspaceView;
  selected: Selected | null;
  expanded: ReadonlySet<string>;
  diagramFocus: string | null;
  driftScopes: DriftScope[];
  newNodeIds: ReadonlySet<string>;
  newRespIds: ReadonlySet<string>;
  health: ModelHealthReport | null;
  agent: AgentSession;
  build: ModelBuild;
  /** Diagram nodes whose semantics are still "generating" (build sequence). */
  pendingIds?: ReadonlySet<string>;
  /** This session's edit journal — what the Recent changes page shows (the whole
   *  plan, agent's writes + the human's edits). Empty until a scene populates it. */
  changeLog?: readonly ChangeRevision[];
}

/** Navigation/intent callbacks. A scene can wire these to real clicks, or just
 *  drive state directly through the director's `set`. */
export interface WorkspaceActions {
  onSelectNode?: (id: string) => void;
  /** Selecting a node ON the diagram — selects it WITHOUT reframing the level
   *  (unlike the tree's onSelectNode, which jumps the diagram to its level). */
  onSelectFromDiagram?: (id: string) => void;
  onSelectGroup?: (id: string) => void;
  onToggle?: (id: string, expand?: boolean) => void;
  onSetView?: (view: WorkspaceView) => void;
  onFocus?: (id: string | null) => void;
}

export function WorkspaceShell({
  state,
  actions = {},
  editor,
}: {
  state: WorkspaceState;
  actions?: WorkspaceActions;
  /** When set, the node page renders its edit affordances (the demo drives real
   *  clicks into them). Left undefined elsewhere, so the shell stays read-only. */
  editor?: Editor;
}) {
  const {
    model,
    committed,
    projectPath,
    view,
    selected,
    expanded,
    diagramFocus,
    driftScopes,
    newNodeIds,
    newRespIds,
    health,
    agent,
    build,
  } = state;

  // The plan = planned model minus its committed base. Drives the gutter marks
  // in the tree and the change tints on the node page / diagram.
  const diff = useMemo(
    () => (committed ? planDiff(committed, model) : { changes: [] }),
    [committed, model],
  );

  const reviewIndex = useMemo(
    () => buildReviewIndex(model, health, driftScopes, newNodeIds, newRespIds),
    [model, health, driftScopes, newNodeIds, newRespIds],
  );

  const sel = actions.onSelectNode ?? noop;
  const selDiagram = actions.onSelectFromDiagram ?? actions.onSelectNode ?? noop;
  const selGroup = actions.onSelectGroup ?? noop;
  const toggle = actions.onToggle ?? noop;
  const setView = actions.onSetView ?? noop;
  const focus = actions.onFocus ?? noop;

  return (
    <div data-cam="stage" className="flex h-full w-full min-h-0 flex-col bg-[var(--surface-canvas)]">
      <BarberPattern />
      <TopBar
        projectPath={projectPath}
        view={view}
        onSetView={setView}
        onOpenProject={noop}
        onCloseProject={noop}
        onOpenSearch={noop}
        onOpenSettings={noop}
      />
      <div className="flex min-h-0 flex-1">
        <ModelTree
          model={model}
          planDiff={diff}
          selected={selected}
          expanded={expanded}
          onSelectNode={sel}
          onSelectGroup={selGroup}
          onToggle={toggle}
          editor={undefined}
          activeNodeIds={build.active ? build.activeNodeIds : EMPTY_IDS}
        />
        {view === "diagram" ? (
          <DiagramView
            model={model}
            planDiff={diff}
            report={health}
            focusId={diagramFocus}
            selectedId={selected?.kind === "node" ? selected.id : null}
            onFocus={focus}
            onSelectNode={(id) => id && selDiagram(id)}
            pendingIds={state.pendingIds}
          />
        ) : selected?.kind === "special" ? (
          selected.id === "changes" ? (
            <RecentChangesPage changeLog={state.changeLog ?? []} onSelectNode={sel} />
          ) : selected.id === "dark" ? (
            <DarkCodePage model={model} report={health} onSelectNode={sel} />
          ) : selected.id === "unmapped" ? (
            <UnmappedClaimsPage committed={committed} report={health} onSelectNode={sel} />
          ) : (
            <NeedsReviewPage
              model={model}
              report={health}
              driftScopes={driftScopes}
              newNodeIds={newNodeIds}
              newRespIds={newRespIds}
              editor={undefined}
              onSelectNode={sel}
              onClearAllNew={noop}
            />
          )
        ) : selected?.kind === "node" || selected?.kind === "group" ? (
          <NodePage
            model={model}
            committed={committed}
            selected={selected}
            report={health}
            projectPath={projectPath}
            editor={editor}
            onCheckDrift={editor ? noop : undefined}
            onDismissDrift={editor ? noop : undefined}
            onSelectNode={sel}
            onSelectGroup={selGroup}
            variationState={null}
            changeLog={[]}
            history={[]}
            driftScopes={driftScopes}
          />
        ) : (
          <div className="flex flex-1 items-center justify-center text-xs text-[var(--text-muted)]">
            Select a node from the tree.
          </div>
        )}
      </div>
      <Powerline
        model={model}
        agent={agent}
        build={build}
        reviewIndex={reviewIndex}
        health={health}
        launch={DEMO_LAUNCH}
        onOpenSpecial={noop}
        onOpenSettings={noop}
      />
    </div>
  );
}
