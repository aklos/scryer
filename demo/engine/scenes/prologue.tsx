/**
 * Beat 0 — the prologue: where the model comes from.
 *
 * Opens on the REAL `ProjectPicker` in its "no model yet" state, pushes in on
 * the violet "Generate from codebase" button, and clicks it. It hands off to the
 * map, which builds as a C4 zoom-down tour: the systems level blooms in, the
 * cursor selects the platform and drills into it (the diagram's own `fitView`
 * eases down a level), and the services level blooms in beneath it. Finally it
 * lands in the wiki on the Ledger, handing off to the next act.
 *
 * Two fidelity points:
 *  - A model generated FROM code is born in sync with it — committed === planned,
 *    so there is NO diff. We keep `committed === model` at every step (diff marks
 *    are reserved for later, when the agent *plans* a change that isn't in code).
 *  - Each level is revealed whole (one layout, one fit). Revealing nodes one at a
 *    time re-runs the planar layout and jitters the cards; the motion comes from
 *    drilling between levels, which `DiagramView` animates for us.
 */

import { ProjectPicker } from "../../../src/ProjectPicker";
import type { ModelStorage } from "../../../src/hooks/useModelStorage";
import type { ModelBuild } from "../../../src/hooks/useModelBuild";
import { paymentsModel } from "../../fixtures";
import {
  WorkspaceShell,
  IDLE_AGENT,
  IDLE_BUILD,
  type WorkspaceState,
} from "../Workspace";
import type { Scene } from "../types";

const EMPTY: ReadonlySet<string> = new Set();

/** The two levels the tour descends through. */
const SYSTEMS = ["cardholder", "merchant", "aperture", "acquiring-bank", "card-networks"];
const SERVICES = [
  "api-gateway", "auth", "ledger", "payments-db", "fraud",
  "event-bus", "webhooks", "notifications", "dashboard",
];

/** A React Flow card by node id (the diagram renders `data-id` on each node). */
const card = (id: string) => `.react-flow__node[data-id='${id}']`;

function buildingOf(phase: string): ModelBuild {
  return { ...IDLE_BUILD, building: true, active: true, phase, activity: "Reading the codebase…" };
}

/** Picker-stage storage mock: a project is open but has no model — the state
 *  that surfaces the Generate / Start-blank paths. The demo never opens a real
 *  folder, so the lifecycle methods are inert. */
const PICKER_STORAGE = {
  status: "needs-model",
  projectPath: "/demo/aperture-pay",
  model: null,
  committed: null,
  recentProjects: [],
  error: null,
  openProject: async () => {},
  closeProject: () => {},
  createBlankModel: async () => {},
  forgetRecent: () => {},
} as unknown as ModelStorage;

interface PrologueState extends WorkspaceState {
  stage: "picker" | "build";
}

const INITIAL: PrologueState = {
  stage: "picker",
  // The full model exists from the first frame — the diagram only ever renders
  // the focused level, and `pendingIds` says which of its cards are still
  // generating. (Passing the whole model is also what lets a top-level link like
  // merchant→dashboard lift to merchant→Aperture Pay.)
  model: paymentsModel,
  committed: paymentsModel,
  projectPath: "/demo/aperture-pay",
  view: "diagram",
  selected: null,
  expanded: new Set(["aperture"]),
  diagramFocus: null,
  driftScopes: [],
  newNodeIds: EMPTY,
  newRespIds: EMPTY,
  health: null,
  agent: IDLE_AGENT,
  build: IDLE_BUILD,
};

export const prologueScene: Scene<PrologueState> = {
  initial: INITIAL,
  render: (s) =>
    s.stage === "picker" ? (
      <ProjectPicker storage={PICKER_STORAGE} build={IDLE_BUILD} />
    ) : (
      <WorkspaceShell state={s} />
    ),
  run: async (d) => {
    // Fill one card: drop it from the pending set so its placeholder resolves
    // into real content. Slow enough to read as the agent working through them.
    const fill = async (ids: string[], step: number) => {
      for (const id of ids) {
        await d.wait(step);
        await d.set((s) => {
          const next = new Set(s.pendingIds ?? []);
          next.delete(id);
          return { ...s, pendingIds: next };
        });
      }
    };

    // 1. The picker: dwell, push in on Generate, click.
    await d.wait(650);
    await d.camera("generate", { zoom: 1.25, duration: 650, hold: 450 });
    await d.cursorTo("generate");
    await d.click("generate");
    await d.wait(260);

    // 2. Cut to the map at the top level: every system card lands at once as a
    //    pulsing "generating" placeholder, then the agent fills them in one by
    //    one. The model is already whole; the diagram just shows this level.
    await d.set((s) => ({
      ...s,
      stage: "build",
      view: "diagram",
      diagramFocus: null,
      selected: null,
      pendingIds: new Set(SYSTEMS),
      build: buildingOf("▶ Mapping the system boundary"),
    }));
    await d.camera("stage", { zoom: 1, duration: 400 });
    await d.wait(750); // the structure scaffolds (placeholders pulse)
    await fill(SYSTEMS, 440);
    await d.wait(500);

    // 3. Select the platform and drill in — its services scaffold as
    //    placeholders, then fill in beneath it.
    await d.cursorTo(card("aperture"));
    await d.click(card("aperture"));
    await d.set((s) => ({ ...s, selected: { kind: "node", id: "aperture" } }));
    await d.wait(650);

    await d.set((s) => ({
      ...s,
      diagramFocus: "aperture",
      selected: null,
      pendingIds: new Set(SERVICES),
      build: buildingOf("▶ Generating services"),
    }));
    await d.wait(850); // services scaffold
    await fill(SERVICES, 360);
    await d.wait(550);

    // 4. Build done. Land on the Ledger card, then navigate to its wiki page by
    //    clicking the real Wiki toggle (not a state flip) — hands off to Act 2.
    await d.cursorTo(card("ledger"));
    await d.click(card("ledger"));
    await d.set((s) => ({
      ...s,
      model: paymentsModel,
      committed: paymentsModel,
      build: IDLE_BUILD,
      pendingIds: new Set(),
      selected: { kind: "node", id: "ledger" },
    }));
    await d.wait(750);

    await d.cursorTo("[data-cam='view-wiki']");
    await d.click("[data-cam='view-wiki']");
    await d.set((s) => ({
      ...s,
      view: "wiki",
      selected: { kind: "node", id: "ledger" },
      expanded: new Set(["aperture"]),
    }));
    await d.wait(1100);
  },
};
