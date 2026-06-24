/**
 * Act 1 — the agent plans the refund.
 *
 * The two-window desktop: a generic agent CLI tiled beside the real scryer
 * shell. The user types a request for a refund *plan*; the agent orients, then
 * writes the plan into the model through the scryer MCP tools (`search_model`,
 * `update_nodes`). As each `update_nodes` write lands, the matching service
 * lights up in the tree — the plan appearing in scryer, before a line of code.
 *
 * Fidelity notes:
 *  - The agent only PLANS here. `committed` holds at the as-built model; `model`
 *    grows. Their diff is the plan — no code is written in this act.
 *  - Adding a responsibility to an existing service reads as `M` (modified) in
 *    the tree gutter — `A`/green is for whole new nodes. The new claim itself
 *    shows as green "Added" on the node page (where the user reads it next, 1c).
 */

import { memo } from "react";
import { Terminal, type TerminalState, type TermLine } from "../Terminal";
import { WorkspaceShell, IDLE_AGENT, IDLE_BUILD, type WorkspaceState } from "../Workspace";
import { paymentsModel } from "../../fixtures";
import type { ScryModel, DriftScope, Responsibility } from "../../../src/viewmodel";
import type { AgentSession } from "../../../src/hooks/useAgentSession";
import type { ModelBuild } from "../../../src/hooks/useModelBuild";
import type { Editor } from "../../../src/editor";
import type { ChangeRevision } from "../../../src/hooks/useModelStorage";
import type { Scene } from "../types";
import type { Director } from "../director";
import "../scenes/refund.css";

const EMPTY: ReadonlySet<string> = new Set();
const noop = () => {};

// The shell never reacts to clicks in this act (the agent drives it through
// `set`), so a single frozen actions object keeps the memoized shell from
// re-rendering on every keystroke of the typed request.
const NO_ACTIONS = {};
const ShellMemo = memo(WorkspaceShell);

/** The request the user types, asking for a PLAN (not an implementation). Kept
 *  short enough to stay on one line of the input box (so the box never grows
 *  under the camera while it's being typed). */
const REQUEST = "plan refund support — money back on a captured payment";

const SAY =
  "Refunds aren't modelled yet. I'll plan them across the services they touch — the ledger, the webhook dispatcher, and notifications. Writing it into the model now.";

/** The plan is in the model — and the agent, helpfully, will not shut up about
 *  it. A wall of over-explanation (the joke) before the human takes over. */
const ESSAY =
  "There — the plan is in the model. Allow me to walk you through my reasoning in full. " +
  "A refund is, fundamentally, the controlled reversal of a previously captured payment, and " +
  "so it cannot be modelled as a single action but as a coordinated change across three distinct " +
  "services, each with its own responsibility boundary. First, the Ledger Service: I have added a " +
  "responsibility to post the refund as a reversing double-entry against the original capture, which " +
  "preserves the invariant that every credit has a matching debit — without it the books would not " +
  "balance and downstream reconciliation would silently diverge. Second, the Webhook Dispatcher: " +
  "merchants integrate against our event stream, so it must deliver refund.created and refund.settled " +
  "events to the merchant's endpoint, mirroring the existing payment-event contract so that no new " +
  "integration work is forced upon them. Third, the Notification Service: the cardholder deserves to " +
  "know their money is on the way, so I have added a responsibility to email a refund confirmation. " +
  "Together these three claims constitute a complete, traceable specification of the feature — each one " +
  "a promise the code must later keep. Note that I have deliberately written no code yet: the model " +
  "leads, the implementation follows. I trust this is satisfactory.";

/** The refund claim each service gains, in the order the agent writes them.
 *  These are the args shown in the terminal AND the responsibilities that bloom
 *  into the model. */
const WRITES: { id: string; resp: { id: string; statement: string } }[] = [
  {
    id: "ledger",
    resp: { id: "r-ledger-refund", statement: "Post a refund as a reversing double-entry against the original capture" },
  },
  {
    id: "webhooks",
    resp: { id: "r-wh-refund", statement: "Deliver refund.created and refund.settled events to the merchant's endpoint" },
  },
  {
    id: "notifications",
    resp: { id: "r-notif-refund", statement: "Email a refund confirmation to the cardholder" },
  },
];

/** The constraint the human pins to the Ledger refund claim by hand. */
const CONSTRAINT = "A refund can never exceed the captured amount";

const NODE_NAME: Record<string, string> = {
  ledger: "Ledger Service",
  webhooks: "Webhook Dispatcher",
  notifications: "Notification Service",
};

/** This session's edit journal — the whole plan, newest first: the human's hand
 *  edit (the constraint) over the agent's three refund-claim writes. Drives the
 *  Recent changes page — the blast radius in one place. Fixed timestamps keep the
 *  take deterministic. */
const PLAN_AT = 1_718_900_000_000;
const PLAN_LOG: readonly ChangeRevision[] = [
  {
    at: PLAN_AT + 52_000,
    by: "user",
    items: [
      {
        op: "changed",
        what: "claim",
        label: WRITES[0].resp.statement,
        context: NODE_NAME.ledger,
        nodeId: "ledger",
        fields: [{ field: "directive", from: "—", to: CONSTRAINT }],
      },
    ],
  },
  {
    at: PLAN_AT,
    by: "agent",
    items: WRITES.map((w) => ({
      op: "added" as const,
      what: "claim" as const,
      label: w.resp.statement,
      context: NODE_NAME[w.id],
      nodeId: w.id,
    })),
  },
];

interface Edit {
  file: string;
  rows: { op: "+" | "-" | " "; text: string }[];
}

/** The code edits the agent streams while implementing the plan. The first four
 *  realise the planned refund claims across the services. The next two are
 *  genuine refund plumbing (index captures as refundable, add the migration).
 *  The LAST — fees.rs — silently waives the processing fee on refunds, a policy
 *  no claim ever asked for, which scryer's drift watch then catches. */
const EDITS: Edit[] = [
  {
    file: "ledger/src/refund.rs",
    rows: [
      { op: "+", text: "/// Post a refund as a reversing entry against the capture." },
      { op: "+", text: "pub fn post_refund(tx: &Tx, cap: &Entry, amount: Money) -> Result<RefundId> {" },
      { op: "+", text: "    if amount > cap.amount { return Err(Error::ExceedsCapture); }" },
      { op: "+", text: "    ledger.debit(ESCROW, amount)?;" },
      { op: "+", text: "    ledger.credit(tx.account, amount)?;" },
      { op: "+", text: "    Ok(RefundId::new())" },
      { op: "+", text: "}" },
    ],
  },
  {
    file: "ledger/src/posting.rs",
    rows: [
      { op: " ", text: "pub fn post_entry(tx: &Tx, entry: Entry) -> Result<()> {" },
      { op: " ", text: "    assert_non_negative(&entry)?;" },
      { op: "+", text: "    if entry.kind == Kind::Refund {" },
      { op: "+", text: "        return refund::post_refund(tx, &entry.cap, entry.amount).map(|_| ());" },
      { op: "+", text: "    }" },
      { op: " ", text: "    ledger.append(tx, entry)" },
      { op: " ", text: "}" },
    ],
  },
  {
    file: "webhooks/dispatch.go",
    rows: [
      { op: "+", text: "// Deliver refund.created and refund.settled to the merchant." },
      { op: "+", text: "func (d *Dispatcher) EmitRefund(ev RefundEvent) error {" },
      { op: "+", text: '    return d.deliver(ev.Endpoint, "refund."+ev.Phase, ev.Payload)' },
      { op: "+", text: "}" },
    ],
  },
  {
    file: "notifications/refund.ts",
    rows: [
      { op: "+", text: "// Email a refund confirmation to the cardholder." },
      { op: "+", text: "export async function sendRefundConfirmation(r: Refund) {" },
      { op: "+", text: "  await mailer.send(r.cardholder.email, templates.refund(r));" },
      { op: "+", text: "}" },
    ],
  },
  // --- genuine refund plumbing the claims imply (not separate claims): index
  //     existing captures as refundable, and add the column to store the link. --
  {
    file: "ledger/src/escrow.rs",
    rows: [
      { op: " ", text: "    ledger.credit(ESCROW, amount)?;" },
      { op: "+", text: "    refund::index_capture(id, tx.id, amount); // captures are now refundable" },
      { op: " ", text: "    Ok(id)" },
    ],
  },
  {
    file: "ledger/migrations/0008_refund.sql",
    rows: [
      { op: "+", text: "ALTER TABLE entries ADD COLUMN refund_of UUID NULL;" },
      { op: "+", text: "CREATE INDEX idx_entries_refund_of ON entries(refund_of);" },
    ],
  },
  // --- the overreach: the agent quietly waives the processing fee on every
  //     refund — a real behaviour change no claim asked for (the drift bait). ----
  {
    file: "ledger/src/fees.rs",
    rows: [
      { op: " ", text: "pub fn fee(entry: &Entry) -> Money {" },
      { op: "+", text: "    if entry.kind == Kind::Refund { return Money::ZERO; } // waive fees on refunds" },
      { op: " ", text: "    entry.amount * FEE_BPS / 10_000" },
      { op: " ", text: "}" },
    ],
  },
];

/** Anchors for the three refund claims — added when the agent implements them
 *  (mark_implemented maps each claim to the code it wrote). Without these the
 *  committed claims would read "unmapped". */
const REFUND_SOURCE_MAP = {
  "r-ledger-refund": [{ pattern: "ledger/src/refund.rs", symbol: "post_refund", line: 12, endLine: 40 }],
  "r-wh-refund": [{ pattern: "webhooks/dispatch.go", symbol: "EmitRefund", line: 88, endLine: 121 }],
  "r-notif-refund": [{ pattern: "notifications/refund.ts", symbol: "sendRefundConfirmation", line: 3, endLine: 24 }],
};

/** The one undescribed behaviour scryer's scan extracts from the overreach — an
 *  unauthorised policy the code now enforces (from the fees.rs change) that no
 *  claim ever asked for. Surfaces as a vagrant claim; the human rejects it. */
const VAGRANT: Responsibility = {
  id: "r-ledger-vagrant",
  statement: "Waive the processing fee on every refund",
  vagrant: true,
};

/** The agent's reconcile edit — reverting the fee waiver it never should have
 *  written (the code behind the rejected claim): drop the refund guard so
 *  refunds pay the fee again, same as every other entry. */
const RECONCILE_EDIT: Edit = {
  file: "ledger/src/fees.rs",
  rows: [
    { op: " ", text: "pub fn fee(entry: &Entry) -> Money {" },
    { op: "-", text: "    if entry.kind == Kind::Refund { return Money::ZERO; } // waive fees on refunds" },
    { op: " ", text: "    entry.amount * FEE_BPS / 10_000" },
    { op: " ", text: "}" },
  ],
};

/** The ledger files scryer's plan-aware drift watch flags — code the agent
 *  changed beyond the three mapped claims (the changedFiles match the plumbing +
 *  overreach EDITS above). The semantic Check then triages them: the plumbing
 *  backs the refund claims and reconciles; the fee waiver is left as a vagrant. */
const DRIFT_SCOPES: DriftScope[] = [
  {
    nodeId: "ledger",
    nodeName: "Ledger Service",
    changedFiles: [
      "ledger/src/escrow.rs",
      "ledger/migrations/0008_refund.sql",
      "ledger/src/fees.rs",
    ],
  },
];

/** Append a responsibility to one node — the bloom (`committed` stays put, so
 *  the new claim reads as a plan addition). */
function withResp(model: ScryModel, nodeId: string, resp: { id: string; statement: string }): ScryModel {
  return {
    ...model,
    nodes: model.nodes.map((n) =>
      n.id === nodeId
        ? { ...n, responsibilities: [...(n.responsibilities ?? []), resp] }
        : n,
    ),
  };
}

// The human-edit beat drives the REAL node-page editor (clicks [edit], the
// [Directive] control, types into the live field, clicks Done). The editor's
// commit lands here: `applyEdit` is wired to the director's `set` at run time so
// these intents mutate the scene model. Only the methods the beat exercises do
// real work (node/group patches on Done); the rest are inert.
let applyEdit: (fn: (s: RefundState) => RefundState) => void = () => {};

export const demoEditor: Editor = {
  updateNode: (nodeId, patch) =>
    applyEdit((s) => ({
      ...s,
      shell: {
        ...s.shell,
        model: {
          ...s.shell.model,
          nodes: s.shell.model.nodes.map((n) => (n.id === nodeId ? { ...n, ...patch } : n)),
        },
      },
    })),
  updateGroup: (groupId, patch) =>
    applyEdit((s) => ({
      ...s,
      shell: {
        ...s.shell,
        model: {
          ...s.shell.model,
          groups: s.shell.model.groups.map((g) => (g.id === groupId ? { ...g, ...patch } : g)),
        },
      },
    })),
  deleteNode: noop,
  addNode: () => "",
  moveNode: noop,
  addLink: () => "",
  updateLink: noop,
  deleteLink: noop,
  deleteGroup: noop,
  addGroup: () => "",
  setNodeGroup: noop,
  addResponsibility: () => "",
  updateResponsibility: noop,
  removeResponsibility: noop,
  adoptResponsibility: noop,
  // Reject a vagrant claim (drop the vagrancy): remove it from the plan and fold
  // an un-flagged copy into committed — so the diff reads it as a deletion to-do
  // (committed has it, the plan doesn't) the agent then reconciles.
  rejectResponsibility: (respId) =>
    applyEdit((s) => {
      const node = s.shell.model.nodes.find((n) =>
        n.responsibilities?.some((r) => r.id === respId),
      );
      const resp = node?.responsibilities?.find((r) => r.id === respId);
      if (!node || !resp || !s.shell.committed) return s;
      const committedResp = { ...resp, vagrant: undefined };
      return {
        ...s,
        shell: {
          ...s.shell,
          model: {
            ...s.shell.model,
            nodes: s.shell.model.nodes.map((n) =>
              n.id === node.id
                ? { ...n, responsibilities: (n.responsibilities ?? []).filter((r) => r.id !== respId) }
                : n,
            ),
          },
          committed: {
            ...s.shell.committed,
            nodes: s.shell.committed.nodes.map((n) =>
              n.id === node.id
                ? { ...n, responsibilities: [...(n.responsibilities ?? []), committedResp] }
                : n,
            ),
          },
        },
      };
    }),
  dropResponsibility: noop,
  reimplementResponsibility: noop,
  rewordResponsibility: noop,
  dropNode: noop,
  reimplementNode: noop,
  moveResponsibility: noop,
  addProperty: noop,
  updateProperty: noop,
  removeProperty: noop,
  adoptProperty: noop,
  rejectProperty: noop,
  dropProperty: noop,
  reimplementProperty: noop,
};

const RUNNING_AGENT: AgentSession = {
  running: true,
  label: "Planning refund support",
  lastTool: "update_nodes",
  activity: null,
  startFixture: noop,
  startVariation: noop,
  cancel: noop,
};

const IMPLEMENTING_AGENT: AgentSession = {
  running: true,
  label: "Implementing refund",
  lastTool: "edit_file",
  activity: null,
  startFixture: noop,
  startVariation: noop,
  cancel: noop,
};

/** A build state that lights one service indigo in the tree (the "agent working
 *  on this" highlight) as its refund claim lands — the flash on the bloom. */
function flashOn(nodeId: string): ModelBuild {
  return {
    ...IDLE_BUILD,
    building: true,
    active: true,
    activeNodeIds: new Set([nodeId]),
    phase: "Planning refund support",
    activity: "update_nodes",
  };
}

export interface RefundState {
  term: TerminalState;
  shell: WorkspaceState;
}

const INITIAL: RefundState = {
  term: {
    cwd: "~/aperture-pay",
    input: "",
    running: false,
    lines: [],
  },
  // Picks up where the prologue left off: the wiki on the Ledger, model born in
  // sync with the code (committed === model), nothing planned yet.
  shell: {
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
  },
};

export const refundScene: Scene<RefundState> = {
  initial: INITIAL,
  render: (s) => (
    <div className="rf-desktop" data-cam="desktop">
      <div className="rf-pair" data-cam="pair">
        <div className="rf-term">
          <Terminal state={s.term} />
        </div>
        <div className="rf-shell">
          <ShellMemo state={s.shell} actions={NO_ACTIONS} editor={demoEditor} />
        </div>
      </div>
      <div className="rf-work" data-cam="work" />
    </div>
  ),
  run: (d) => runRefund(d),
};

/**
 * The refund choreography, extracted so the combined `film` scene can run it
 * verbatim after the prologue + terminal launch. `skipEstablish` drops the wide
 * desktop establishing shot (the film already framed the pair at the seam).
 */
export async function runRefund(
  d: Director<RefundState>,
  opts: { skipEstablish?: boolean } = {},
): Promise<void> {
    // Route the demo editor's commits (Done in the node page) through the
    // director, so a real edit mutates the scene model and repaints.
    applyEdit = (fn) => void d.set(fn);

    // Terminal helpers — the scene drives the CLI the same way it drives scryer.
    const setTerm = (patch: Partial<TerminalState>) =>
      d.set((s) => ({ ...s, term: { ...s.term, ...patch } }));
    const pushLine = (line: TermLine) =>
      d.set((s) => ({ ...s, term: { ...s.term, lines: [...s.term.lines, line] } }));
    // One character at a time, at a readable human pace.
    const type = async (text: string) => {
      for (let i = 1; i <= text.length; i++) {
        await setTerm({ input: text.slice(0, i) });
        await d.wait(44);
      }
    };
    const resolveLastTool = (lines: TermLine[]): TermLine[] => {
      const out = lines.slice();
      const last = out[out.length - 1];
      if (last?.kind === "tool") out[out.length - 1] = { ...last, status: "ok" };
      return out;
    };

    // 0. Establish the scene: open wide on the patterned canvas, then ease in to
    //    frame BOTH windows balanced — the AI agent CLI on the left, the scryer
    //    model on the right. (The engine can zoom out below 1:1 now.)
    if (!opts.skipEstablish) {
      await d.camera("desktop", { minZoom: 0.3, duration: 0 });
      await d.camera("pair", { minZoom: 0.5, duration: 1200, hold: 600 });
    }

    // THE ENTIRE AGENT TURN PLAYS IN THIS ONE STATIC SHOT. No camera moves while
    // typing or while the edits land — so there is physically nothing to shift or
    // snap. (A tight input zoom, or a glide over to scryer for the blooms, would
    // each require moving the camera mid-turn, which is exactly what produced the
    // horizontal shift and the snap-to-scryer.)

    // 0b. Frame the relationship first: the agent CLI talks to scryer through its
    //     MCP server. The leader runs off the terminal toward scryer — the link.
    await d.annotate("terminal", "Connect your agent to Scryer through MCP", { place: "right" });
    await d.wait(2200);
    await d.clear();
    await d.wait(300);

    // 1. Type the request, right here in the both-windows shot.
    await d.cursorTo("term-input");
    await d.wait(300);
    await type(REQUEST);
    await d.wait(460);
    await pushLine({ kind: "user", text: REQUEST });
    await setTerm({ input: "", running: true });
    await d.set((s) => ({ ...s, shell: { ...s.shell, agent: RUNNING_AGENT } }));

    // 2. The agent orients (search_model), finds refunds absent, states the plan.
    await pushLine({ kind: "tool", tool: "search_model", arg: "refund", status: "run" });
    await d.wait(820);
    await d.set((s) => ({ ...s, term: { ...s.term, lines: resolveLastTool(s.term.lines) } }));
    await d.wait(440);
    await pushLine({ kind: "say", text: SAY });
    await d.wait(1100);

    // 4. Each write streams in the terminal (left) and lands in the tree (right):
    //    the target service flashes indigo as the tool runs, then settles with
    //    its new claim — the plan appearing service by service, before any code.
    for (const w of WRITES) {
      await pushLine({
        kind: "tool",
        tool: "update_nodes",
        target: w.id,
        arg: w.resp.statement,
        status: "run",
      });
      await d.set((s) => ({ ...s, shell: { ...s.shell, build: flashOn(w.id) } }));
      await d.wait(600);
      await d.set((s) => ({
        ...s,
        term: { ...s.term, lines: resolveLastTool(s.term.lines) },
        shell: { ...s.shell, model: withResp(s.shell.model, w.id, w.resp), build: IDLE_BUILD },
      }));
      await d.wait(620);
    }

    // 4b. The plan has landed — and the agent delivers an unsolicited essay about
    //     it (the joke). The terminal auto-scrolls as the wall of text streams.
    await d.wait(300);
    await pushLine({ kind: "say", text: ESSAY });
    await d.wait(2500);

    // 5. The agent's turn ends; the user takes over. The cursor crosses into
    //    scryer and points at the new Ledger claim, and the camera follows the
    //    cursor in to it — x marks the spot for the one idea of the act.
    await setTerm({ running: false });
    await d.set((s) => ({
      ...s,
      shell: { ...s.shell, agent: { ...RUNNING_AGENT, running: false, activity: null } },
    }));
    await d.wait(500);
    await d.camera("#resp-r-ledger-refund", { zoom: 1.5, duration: 1100, hold: 200 });
    await d.cursorTo("#resp-r-ledger-refund", { duration: 640 });
    // Anchor the pin to the statement TEXT (the inner inline span hugs the
    // words), not the full-width row — so it lands right after the claim, with
    // the label out in the open space beside it. The label stays up; the act
    // hands off to the next beat in a few seconds.
    await d.annotate(
      "#resp-r-ledger-refund > div > span",
      "Planned changes are diffed in the model — before code",
      { place: "right" },
    );
    await d.wait(2100);
    await d.clear();

    // 6. The human takes over and refines the plan BY HAND in scryer — driving
    //    the real node-page editor. Pull back to frame the Responsibilities
    //    section so the edit affordances are on screen, then click into them.
    const EDIT = '[data-section="Responsibilities"] [class~="group-hover/sec:visible"]';
    const ADD_DIR = '[data-erow="r-ledger-refund"] [data-act="add-directive"]';
    const DIR_FIELD = '[data-drow="r-ledger-refund"] [role="textbox"]';
    const DONE = '[data-section="Responsibilities"] [data-act="commit"]';

    await d.camera('[data-section="Responsibilities"]', { minZoom: 0.5, pad: 90, duration: 950, hold: 200 });

    // Enter edit mode (the section [edit] toggle).
    await d.cursorTo(EDIT);
    await d.click(EDIT);
    await d.wait(500);

    // Add a directive to the new Ledger refund claim, then type the constraint
    // into the live field — every keystroke flows through the real editor.
    await d.cursorTo(ADD_DIR);
    await d.click(ADD_DIR);
    await d.wait(450);
    await d.cursorTo(DIR_FIELD);
    await d.typeInto(DIR_FIELD, CONSTRAINT, { charMs: 40 });
    await d.wait(550);

    // Commit (Done) — folds the hand edit into the plan.
    await d.cursorTo(DONE);
    await d.click(DONE);
    await d.wait(600);

    // 7. Push into the result and name the beat.
    await d.camera("#resp-r-ledger-refund", { zoom: 1.4, duration: 1000, hold: 200 });
    await d.cursorTo("#resp-r-ledger-refund + li div span", { duration: 600 });
    await d.annotate(
      "#resp-r-ledger-refund + li div span",
      "Edit by hand — it lands in the plan, same as the agent",
      { place: "right" },
    );
    await d.wait(2400);

    // ========================================================================
    // ACT 2 — the blast radius. Scryer takes the full frame; the human opens
    // Recent changes (the session journal) to read the WHOLE plan in one place —
    // the agent's three writes plus the hand edit — then heads back to the
    // terminal to implement it.
    // ========================================================================
    await d.clear();

    // Scryer takes the frame on its own (terminal slides out of shot). `pad` +
    // a sub-1 floor pull the whole shell in off the edges — video-safe.
    await d.camera(".rf-shell", { minZoom: 0.7, pad: 90, duration: 1200, hold: 250 });

    // Open Recent changes from the status bar, and seed the journal it shows.
    const CHANGES = '[title^="Recent changes"]';
    await d.cursorTo(CHANGES);
    await d.click(CHANGES);
    await d.set((s) => ({
      ...s,
      shell: { ...s.shell, selected: { kind: "special", id: "changes" }, changeLog: PLAN_LOG },
    }));
    await d.wait(800);

    // Push in on the change list itself — a closer read of the plan (confirm the
    // changes by eye) — and only THEN name the beat, on the zoomed-in view.
    await d.camera("[data-changes-list]", { minZoom: 0.55, pad: 120, duration: 1100, hold: 450 });
    // Label sits in the open space UNDER the change list.
    await d.annotate(
      "[data-changes-list]",
      "The whole plan in one place — every change, across the services",
      { place: "bottom" },
    );
    await d.wait(2700);
    await d.clear();

    // Only now move to the terminal — close on the prompt box + live output (the
    // active zone), not the whole window — to implement the confirmed plan.
    await d.camera("term-active", { pad: 28, duration: 1200, hold: 500 });

    // ========================================================================
    // ACT 3 — implement the plan. The user prompts; the agent streams the code
    // edits across the services (no build viz — just the diffs going by). The
    // last one — waiving the refund fee — was never in the plan.
    // ========================================================================
    await d.cursorTo("term-input");
    await d.wait(350);
    await type("implement the plan");
    await d.wait(420);
    await pushLine({ kind: "user", text: "implement the plan" });
    await setTerm({ input: "", running: true });
    await d.set((s) => ({ ...s, shell: { ...s.shell, agent: IMPLEMENTING_AGENT } }));

    await pushLine({
      kind: "say",
      text: "On it — writing the refund across the ledger, the dispatcher, and notifications.",
    });
    await d.wait(750);

    // The edits stream by one after another — every one readable; the last (the
    // fee waiver) is the one that shouldn't be here.
    for (const e of EDITS) {
      await pushLine({ kind: "diff", file: e.file, rows: e.rows });
      await d.wait(560);
    }

    await pushLine({
      kind: "say",
      text: "Done — refund's in across all three services. I also waived the processing fee on refunds while I was in there — figured it'd be a nice touch.",
    });
    await d.wait(900);

    // The agent commits — folding the plan into the committed model.
    await pushLine({ kind: "tool", tool: "mark_implemented", arg: "refund — 3 claims", status: "run" });
    await d.wait(780);
    await d.set((s) => ({ ...s, term: { ...s.term, lines: resolveLastTool(s.term.lines) } }));
    await d.wait(450);
    await setTerm({ running: false });
    await d.set((s) => ({
      ...s,
      shell: { ...s.shell, agent: { ...IMPLEMENTING_AGENT, running: false, activity: null } },
    }));

    // ========================================================================
    // ACT 3 (cont) — commit + the drift catch (the payoff). Pull to scryer with
    // the plan still showing, fold it into committed (the plan DISAPPEARS — the
    // "Added" marks clear), then the plan-aware drift watch flags the ledger files
    // the agent touched beyond the three claims it was asked to implement.
    // ========================================================================
    await d.set((s) => ({
      ...s,
      shell: { ...s.shell, selected: { kind: "node", id: "ledger" }, view: "wiki" },
    }));
    await d.camera(".rf-shell", { minZoom: 0.7, pad: 90, duration: 1300, hold: 300 });
    await d.wait(1200); // the plan is still here — the refund claim reads "Added"

    // Commit: the agent maps each new claim to the code it wrote (no longer
    // "unmapped"), and the planned claims fold into committed — the plan clears.
    await d.set((s) => {
      const model = {
        ...s.shell.model,
        sourceMap: { ...s.shell.model.sourceMap, ...REFUND_SOURCE_MAP },
      };
      return { ...s, shell: { ...s.shell, model, committed: model } };
    });
    await d.wait(1500); // the plan disappears

    // Then drift surfaces — the ledger files the agent changed beyond the plan.
    await d.set((s) => ({ ...s, shell: { ...s.shell, driftScopes: DRIFT_SCOPES } }));
    await d.wait(900);
    await d.camera("[data-drift-banner]", { minZoom: 0.5, pad: 120, duration: 1000, hold: 300 });
    await d.cursorTo("[data-drift-banner]");
    await d.annotate(
      "[data-drift-banner]",
      "The agent changed code beyond the plan — Scryer flags it",
      { place: "above" },
    );
    await d.wait(3000);

    // ========================================================================
    // ACT 4 — reconcile (the resolution). The human runs scryer's drift check;
    // the genuine cleanup reconciles, but the scan surfaces one undescribed
    // behaviour the agent should never have written — a vagrant claim. The human
    // drops it; the terminal agent reverts the code and apologises. Clean model.
    // ========================================================================
    await d.clear();

    // Run the drift check from the banner — scryer scans the changed files.
    const CHECK = '[data-drift-banner] button';
    await d.cursorTo(CHECK);
    await d.click(CHECK);
    await d.set((s) => ({
      ...s,
      shell: {
        ...s.shell,
        build: {
          ...IDLE_BUILD,
          checking: true,
          active: true,
          phase: "Checking for drift",
          activity: "reading the changed files",
        },
      },
    }));
    await d.camera(".rf-shell", { minZoom: 0.7, pad: 90, duration: 1000, hold: 200 });
    await d.wait(1900);

    // The scan resolves: the genuine cleanup reconciles (the drift clears), but
    // one change does something no claim describes — a vagrant claim surfaces.
    await d.set((s) => ({
      ...s,
      shell: {
        ...s.shell,
        build: IDLE_BUILD,
        driftScopes: [],
        model: {
          ...s.shell.model,
          nodes: s.shell.model.nodes.map((n) =>
            n.id === "ledger"
              ? { ...n, responsibilities: [...(n.responsibilities ?? []), VAGRANT] }
              : n,
          ),
        },
      },
    }));
    await d.wait(900);

    // Frame the vagrant claim — code the agent wrote that no claim asked for.
    await d.camera("#resp-r-ledger-vagrant", { zoom: 1.35, pad: 60, duration: 1100, hold: 250 });
    await d.cursorTo("#resp-r-ledger-vagrant > div > span");
    await d.annotate(
      "#resp-r-ledger-vagrant > div > span",
      "A fee policy the agent set — that no one approved",
      { place: "right" },
    );
    await d.wait(2700);
    await d.clear();

    // The human drops the vagrancy — Reject. It becomes a deletion to-do.
    const REJECT = '#resp-r-ledger-vagrant [data-act="reject"]';
    await d.cursorTo(REJECT);
    await d.click(REJECT);
    await d.wait(950);
    await d.annotate(
      "#resp-r-ledger-vagrant > div > span",
      "Dropped — now a deletion for the agent to reconcile",
      { place: "right" },
    );
    await d.wait(2500);
    await d.clear();

    // Cut to the terminal — the human tells the agent to fix its mistake.
    await d.camera("term-active", { pad: 28, duration: 1200, hold: 350 });
    await d.cursorTo("term-input");
    await d.wait(300);
    const FIX = "reconcile the model — drop the fee waiver, I never asked for that";
    await type(FIX);
    await d.wait(420);
    await pushLine({ kind: "user", text: FIX });
    await setTerm({ input: "", running: true });
    await d.set((s) => ({
      ...s,
      shell: { ...s.shell, agent: { ...IMPLEMENTING_AGENT, label: "Reconciling" } },
    }));
    await pushLine({
      kind: "say",
      text: "You're right — waiving fees was a call I made that wasn't mine to make. Reverting it now.",
    });
    await d.wait(950);
    await pushLine({ kind: "diff", file: RECONCILE_EDIT.file, rows: RECONCILE_EDIT.rows });
    await d.wait(1100);
    await pushLine({ kind: "tool", tool: "mark_implemented", arg: "drop — refund fee waiver", status: "run" });
    await d.wait(720);
    await d.set((s) => ({ ...s, term: { ...s.term, lines: resolveLastTool(s.term.lines) } }));
    await pushLine({
      kind: "say",
      text: "Done — the code's back in line with the model. Sorry about the detour.",
    });
    await d.wait(900);
    await setTerm({ running: false });
    await d.set((s) => ({
      ...s,
      shell: { ...s.shell, agent: { ...IMPLEMENTING_AGENT, running: false, activity: null } },
    }));

    // The deletion is reconciled — drop the claim from committed too. Model and
    // code now agree: no plan, no drift, no vagrancy. Clean.
    await d.set((s) => ({
      ...s,
      shell: {
        ...s.shell,
        committed: s.shell.committed && {
          ...s.shell.committed,
          nodes: s.shell.committed.nodes.map((n) =>
            n.id === "ledger"
              ? {
                  ...n,
                  responsibilities: (n.responsibilities ?? []).filter(
                    (r) => r.id !== "r-ledger-vagrant",
                  ),
                }
              : n,
          ),
        },
      },
    }));

    // Settle on the clean model — the closer.
    await d.set((s) => ({
      ...s,
      shell: { ...s.shell, selected: { kind: "node", id: "ledger" }, view: "wiki" },
    }));
    await d.camera(".rf-shell", { minZoom: 0.7, pad: 90, duration: 1300, hold: 400 });
    await d.wait(1000);
    await d.annotate(".rf-shell h1", "Model and code, back in agreement", { place: "right" });
    await d.wait(3200);
}
