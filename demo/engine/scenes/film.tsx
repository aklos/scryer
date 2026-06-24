/**
 * The full trailer in one continuous take: the prologue (where the model comes
 * from) flowing straight into the refund act — no cut, no second clip.
 *
 * It opens held on the finished scryer chrome (the Ledger, tree + wiki) — this
 * is the frame GitHub uses as the video poster. It then rewinds to the empty
 * project picker and plays the model-generation prologue (Generate → the C4 map
 * builds → drill into the platform → land back on the Ledger), and at the seam
 * the agent terminal *launches* in beside scryer to form the two-window desktop
 * the refund act runs in. From there it hands straight to `runRefund`.
 *
 * Everything lives in the one refund desktop coordinate space (`refund.css`):
 * scryer is the SAME window throughout, so nothing reflows when the terminal
 * arrives — the hand-off is seamless.
 */

import { memo } from "react";
import { ProjectPicker } from "../../../src/ProjectPicker";
import type { ModelStorage } from "../../../src/hooks/useModelStorage";
import type { ModelBuild } from "../../../src/hooks/useModelBuild";
import { Terminal } from "../Terminal";
import {
  WorkspaceShell,
  IDLE_AGENT,
  IDLE_BUILD,
  type WorkspaceState,
} from "../Workspace";
import { paymentsModel } from "../../fixtures";
import { runRefund, demoEditor, type RefundState } from "./refund";
import type { Director } from "../director";
import type { Scene } from "../types";
import "./refund.css";
import "./film.css";

const EMPTY: ReadonlySet<string> = new Set();
const NO_ACTIONS = {};
const ShellMemo = memo(WorkspaceShell);

/** The two C4 levels the generation tour descends through (mirrors the prologue). */
const SYSTEMS = ["cardholder", "merchant", "aperture", "acquiring-bank", "card-networks"];
const SERVICES = [
  "api-gateway", "auth", "ledger", "payments-db", "fraud",
  "event-bus", "webhooks", "notifications", "dashboard",
];

/** A React Flow card by node id. */
const card = (id: string) => `.react-flow__node[data-id='${id}']`;

function buildingOf(phase: string): ModelBuild {
  return { ...IDLE_BUILD, building: true, active: true, phase, activity: "Reading the codebase…" };
}

/** Picker-stage storage mock: a project is open but has no model yet — the state
 *  that surfaces the Generate / Start-blank paths. Inert lifecycle (the demo
 *  never opens a real folder). */
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

interface FilmState extends RefundState {
  /** The scryer window shows the empty picker, or the live workspace shell. */
  stage: "picker" | "shell";
  /** The agent terminal has launched in beside scryer (the seam). */
  launched: boolean;
}

/** The destination the prologue builds toward (and the poster frame): the Ledger,
 *  tree expanded, wiki open, model born in sync with code (committed === model). */
const LEDGER_SHELL: WorkspaceState = {
  model: paymentsModel,
  committed: paymentsModel,
  projectPath: "/demo/aperture-pay",
  view: "wiki",
  selected: { kind: "node", id: "ledger" },
  expanded: new Set(["aperture"]),
  diagramFocus: "aperture",
  driftScopes: [],
  newNodeIds: EMPTY,
  newRespIds: EMPTY,
  health: null,
  agent: IDLE_AGENT,
  build: IDLE_BUILD,
};

const INITIAL: FilmState = {
  stage: "shell",
  launched: false,
  term: { cwd: "~/aperture-pay", input: "", running: false, lines: [] },
  shell: LEDGER_SHELL,
};

export const filmScene: Scene<FilmState> = {
  initial: INITIAL,
  render: (s) => (
    <div className="rf-desktop" data-cam="desktop">
      <div className="rf-pair" data-cam="pair">
        <div className={`rf-term${s.launched ? " is-live" : ""}`}>
          <Terminal state={s.term} />
        </div>
        <div className="rf-shell">
          {s.stage === "picker" ? (
            <ProjectPicker storage={PICKER_STORAGE} build={IDLE_BUILD} />
          ) : (
            <ShellMemo state={s.shell} actions={NO_ACTIONS} editor={demoEditor} />
          )}
        </div>
      </div>
      <div className="rf-work" data-cam="work" />
    </div>
  ),
  run: async (d) => {
    // Frame the scryer window so it fills the frame at ~1:1 — the prologue plays
    // out entirely inside this one window box (the terminal is dark beside it).
    const frameShell = (opts: { duration?: number; hold?: number } = {}) =>
      d.camera(".rf-shell", { pad: 24, minZoom: 0.5, duration: opts.duration ?? 700, hold: opts.hold });

    // Drop a generating card's pending mark → its placeholder resolves to content.
    const fill = async (ids: string[], step: number) => {
      for (const id of ids) {
        await d.wait(step);
        await d.set((s) => {
          const next = new Set(s.shell.pendingIds ?? []);
          next.delete(id);
          return { ...s, shell: { ...s.shell, pendingIds: next } };
        });
      }
    };

    // 0. POSTER — the finished chrome (the Ledger wiki) is the frame GitHub shows
    //    as the thumbnail. In playback we only flash it: a short beat to register
    //    it as the destination, then straight to the rewind so the model
    //    generation gets going right away.
    await frameShell({ duration: 0 });
    await d.wait(350);

    // 1. Rewind to the empty picker — where the model comes from. Push in on the
    //    violet Generate button and click it.
    await d.set((s) => ({ ...s, stage: "picker" }));
    await d.wait(380);
    await d.camera("generate", { zoom: 1.15, duration: 560, hold: 300 });
    await d.cursorTo("generate");
    await d.click("generate");
    await d.wait(200);

    // 2. The map builds top-down. Every system card lands at once as a pulsing
    //    "generating" placeholder; the agent fills them in one by one. A title
    //    names what's happening — the model is being generated from the code.
    await d.set((s) => ({
      ...s,
      stage: "shell",
      shell: {
        ...s.shell,
        view: "diagram",
        diagramFocus: null,
        selected: null,
        pendingIds: new Set(SYSTEMS),
        build: buildingOf("▶ Mapping the system boundary"),
      },
    }));
    await frameShell({ duration: 450 });
    await d.wait(750);
    await fill(SYSTEMS, 380);
    // Label the beat once the map is in. annotate → hold → clear with no state
    // change in between, so the marker fades out cleanly (same shape as Act 2).
    await d.annotate(card("aperture"), "From your codebase to a living model", { place: "right" });
    await d.wait(1300);
    await d.clear();

    // 3. Drill into the platform — its services scaffold, then fill in beneath it.
    await d.cursorTo(card("aperture"));
    await d.click(card("aperture"));
    await d.set((s) => ({ ...s, shell: { ...s.shell, selected: { kind: "node", id: "aperture" } } }));
    await d.wait(650);
    await d.set((s) => ({
      ...s,
      shell: {
        ...s.shell,
        diagramFocus: "aperture",
        selected: null,
        pendingIds: new Set(SERVICES),
        build: buildingOf("▶ Generating services"),
      },
    }));
    await d.wait(850);
    await fill(SERVICES, 320);
    await d.wait(550);

    // 4. Build done. Land on the Ledger and open its wiki via the real toggle.
    await d.cursorTo(card("ledger"));
    await d.click(card("ledger"));
    await d.set((s) => ({
      ...s,
      shell: {
        ...s.shell,
        model: paymentsModel,
        committed: paymentsModel,
        build: IDLE_BUILD,
        pendingIds: new Set(),
        selected: { kind: "node", id: "ledger" },
      },
    }));
    await d.wait(700);
    await d.cursorTo("[data-cam='view-wiki']");
    await d.click("[data-cam='view-wiki']");
    await d.set((s) => ({
      ...s,
      shell: { ...s.shell, view: "wiki", selected: { kind: "node", id: "ledger" }, expanded: new Set(["aperture"]) },
    }));
    await frameShell({ duration: 600, hold: 700 });

    // 5. THE SEAM — launch the agent terminal: it eases in beside scryer while the
    //    camera pulls back to frame both windows. Scryer doesn't move (same
    //    window box), so the only motion is the terminal arriving.
    await d.set((s) => ({ ...s, launched: true }));
    await d.camera("pair", { minZoom: 0.5, duration: 1300, hold: 700 });

    // 6. Hand straight to the refund act (its own establishing shot is skipped —
    //    we're already framed on the pair).
    await runRefund(d as unknown as Director<RefundState>, { skipEstablish: true });
  },
};
