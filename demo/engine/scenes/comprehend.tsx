/**
 * Beat: comprehend. The REAL NodePage and DiagramView, on the Aperture Pay
 * fixtures, inside a minimal workspace frame the scene owns (top bar + a
 * Wiki/Map toggle, since the demo lifts the two panes separately rather than
 * the whole Workspace).
 *
 * Every camera move is motivated by an interaction:
 *   1. land on the page — what this part is, what it's responsible for
 *   2. claim → real code — click a responsibility's source anchor; the inline
 *      peek opens with the actual function (the payoff: the spec is backed by
 *      code, not prose)
 *   3. wiki → map — toggle to the diagram; pull out to the whole architecture
 */

import { DiagramView } from "../../../src/DiagramView";
import { NodePage } from "../../../src/NodePage";
import { EMPTY_DIFF } from "../../../src/planDiff";
import { paymentsModel, committedModel, healthReport } from "../../fixtures";
import type { Scene } from "../types";
import "./comprehend.css";

const noop = () => {};

interface State {
  view: "wiki" | "diagram";
  focusId: string | null;
  selectedId: string | null;
}

const Wiki = () => (
  <NodePage
    model={paymentsModel}
    committed={committedModel}
    selected={{ kind: "node", id: "ledger" }}
    report={healthReport}
    projectPath="/demo/aperture-pay"
    editor={undefined}
    onSelectNode={noop}
    onSelectGroup={noop}
    variationState={null}
    changeLog={[]}
    history={[]}
    driftScopes={[]}
  />
);

const Map = ({ focusId, selectedId }: { focusId: string | null; selectedId: string | null }) => (
  <DiagramView
    model={paymentsModel}
    planDiff={EMPTY_DIFF}
    report={healthReport}
    focusId={focusId}
    selectedId={selectedId}
    onFocus={noop}
    onSelectNode={noop}
  />
);

const Chrome = (s: State) => (
  <div className="cz-workspace">
    <div className="cz-topbar">
      <div className="cz-crumb">
        Aperture Pay <span className="cz-sep">›</span> Ledger Service
      </div>
      <div className="cz-toggle" data-cam="view-toggle">
        <span className={`cz-seg${s.view === "wiki" ? " is-active" : ""}`} data-cam="seg-wiki">
          Wiki
        </span>
        <span className={`cz-seg${s.view === "diagram" ? " is-active" : ""}`} data-cam="seg-map">
          Map
        </span>
      </div>
    </div>
    <div className="cz-pane" data-cam="pane">
      {s.view === "wiki" ? <Wiki /> : <Map focusId={s.focusId} selectedId={s.selectedId} />}
    </div>
  </div>
);

const RESP = "#resp-r-ledger-2"; // the escrow claim
const ANCHOR = `${RESP} [data-cam='resp-source']`;
const PEEK = `${RESP} [data-cam='source-peek']`;
const LEDGER_CARD = ".react-flow__node[data-id='ledger']";

export const comprehendScene: Scene<State> = {
  initial: { view: "wiki", focusId: "aperture", selectedId: null },
  render: (s) => <Chrome {...s} />,
  run: async (d) => {
    // Open ALREADY inside the page, framed on the escrow claim — no flat
    // full-page establishing shot for a single-component beat.
    await d.camera(ANCHOR, { zoom: 1.5, duration: 0 });
    await d.wait(550);

    // 1. claim → real code: tap the anchor, the real peek opens, push into it,
    //    then ring the file:line anchor and call it out.
    await d.cursorTo(ANCHOR);
    await d.click(ANCHOR);
    await d.wait(460); // peek fetches + renders the span
    await d.camera(PEEK, { zoom: 1.55, duration: 620, hold: 350 });
    await d.annotate(ANCHOR, "every claim, mapped to real code");
    await d.wait(1850);
    await d.clear();

    // 2. wiki → map: pull out so the toggle is in reach, flip it.
    await d.camera("pane", { zoom: 1, duration: 560, hold: 180 });
    await d.cursorTo("[data-cam='seg-map']");
    await d.click();
    await d.set((s) => ({ ...s, view: "diagram" }));
    await d.wait(600); // diagram mounts + fits

    // 3. the whole architecture, then dive into the service we just read.
    await d.title("the whole architecture");
    await d.camera("pane", { zoom: 1, hold: 900 });
    await d.clear();
    await d.camera(LEDGER_CARD, { zoom: 1.7, duration: 680, hold: 300 });
    await d.annotate(LEDGER_CARD, "the service you just read");
    await d.wait(1500);
    await d.clear();
  },
};
