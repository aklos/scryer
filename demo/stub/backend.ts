/**
 * The desktop backend, played by the film. The app is mounted unchanged; the
 * bundler swaps its four Tauri modules for the files beside this one, and
 * every command it invokes is answered here from the film's current state:
 * the derived tree's two layers, the health report, verdicts, probe marks,
 * the landing view, settings. When the state moves, the change events the
 * real watcher would send go out, and the app re-reads exactly as it does on
 * a desktop.
 *
 * Navigation travels the same event layer: a sticky `navigate` the app
 * listens for (the desktop backend never sends it). Sticky events reach a
 * listener that subscribes late, so an app mounted mid-film — a parallel
 * render tab starting at frame 1800 — catches up to the frame's state.
 */

import { NOW, REFUND_VIOLATION_TARGET } from "../fixture";
import type { DerivedClaim, Fixture, SessionLog } from "../types";
import type { ModelHealthReport, NodeHealth, HealthCounts, ClaimTestStatus, ClaimProbeStatus } from "../../src/entities/model/health";
import { classifyTestable } from "./ears";

/** The project path the film opens. */
export const PROJECT = "/demo/aperture-pay";
const MODEL_REF = `project:${PROJECT}`;

// --- The event layer --------------------------------------------------------------

type Handler = (event: { event: string; id: number; payload: unknown }) => void;
const handlers = new Map<string, Set<Handler>>();
const sticky = new Map<string, unknown>();

export function listen(name: string, handler: Handler): Promise<() => void> {
  const set = handlers.get(name) ?? new Set<Handler>();
  handlers.set(name, set);
  set.add(handler);
  if (sticky.has(name)) {
    // After the subscribing commit's effects, like a real event would land.
    const payload = sticky.get(name);
    queueMicrotask(() => set.has(handler) && handler({ event: name, id: 0, payload }));
  }
  return Promise.resolve(() => set.delete(handler));
}

export function emit(name: string, payload: unknown, keep = false) {
  if (keep) sticky.set(name, payload);
  for (const h of handlers.get(name) ?? []) h({ event: name, id: 0, payload });
}

// --- In-flight tracking, so a frame waits for the app to settle -----------------

let inFlight = 0;
export function busy(): boolean {
  return inFlight > 0;
}

// --- State ------------------------------------------------------------------------

let current: Fixture | null = null;

export function setFixture(f: Fixture) {
  current = f;
}

function fixture(): Fixture {
  if (!current) throw new Error("the film has not set a fixture yet");
  return current;
}

// --- Payloads ---------------------------------------------------------------------

const zero = (): HealthCounts => ({
  responsibilities: 0,
  properties: 0,
  vagrant: 0,
  driftStale: 0,
  anchorable: 0,
  anchored: 0,
  unmapped: 0,
  tested: 0,
  testedOther: 0,
  testable: 0,
  untested: 0,
});

/** The health report the app's surfaces read: claim counts per subtree, the
 *  dark files per container, the structural report, and the derived status. */
function health(f: Fixture): ModelHealthReport {
  const model = f.tree.planned;
  const parent = new Map(model.nodes.map((n) => [n.id, n.parentId]));
  const nodes: Record<string, NodeHealth> = {};
  const at = (id: string) => (nodes[id] ??= { own: zero(), subtree: zero() });
  for (const n of model.nodes) at(n.id);
  for (const c of f.tree.claims) {
    const testable = classifyTestable(c.description);
    const bump = (h: HealthCounts) => {
      h.responsibilities += 1;
      h.anchorable += 1;
      h.anchored += 1;
      if (testable) {
        h.testable += 1;
        h.tested += 1;
      } else {
        h.testedOther += 1;
      }
    };
    bump(at(c.nodeId).own);
    for (let id: string | undefined = c.nodeId; id; id = parent.get(id)) bump(at(id).subtree);
  }
  for (const file of f.tree.darkFiles) {
    const container = file.split("/")[0];
    const b = (at(container).boundary ??= { totalFiles: 12, anchoredFiles: 12, darkFiles: [] });
    b.darkFiles.push(file);
    b.anchoredFiles -= 1;
  }
  const violations = f.tree.styleViolations.map((v) => ({
    kind: v.kind as ModelHealthReport["structural"]["violations"][number]["kind"],
    node: v.nodeId,
    other: REFUND_VIOLATION_TARGET,
    file: v.file,
    container: v.nodeId.split(".")[0],
    detail: v.detail,
  }));
  return {
    health: { nodes, totals: at("sys").subtree },
    completeness: {},
    derived: { linkAudit: [], unmodeled: [], resolvedEdges: [] },
    structural: {
      violations,
      layerViolations: violations.length,
      isolationViolations: 0,
      externalViolations: 0,
      misplaced: 0,
      unstyled: 0,
      layerless: 0,
      cycles: 0,
      forbiddenLinks: 0,
      roleViolations: 0,
      dataViolations: 0,
    },
    derivedStatus: f.status,
  };
}

function testStatuses(f: Fixture): ClaimTestStatus[] {
  return f.status.claims
    .filter((s) => s.state !== "planned")
    .map((s) => ({
      respId: s.respId,
      outcome: s.state === "failing" ? "failed" : "passed",
      cases: 1,
      stale: s.state === "stale",
      recordedAt: s.recordedAt ?? NOW,
    }));
}

function probeStatuses(f: Fixture): ClaimProbeStatus[] {
  return f.status.claims
    .filter((s) => s.probe)
    .map((s) => ({
      respId: s.respId,
      probes: s.probe === "crosshair" ? 1 : 2,
      survived: s.probe === "crosshair" ? 1 : 0,
      survivors: s.probe === "crosshair" ? ["a deliberate break went unnoticed"] : [],
      stale: false,
      recordedAt: s.recordedAt ?? NOW,
    }));
}

/** The node a file belongs to: the deepest node whose id its path names
 *  (`ledger/src/domain/x.rs` → `ledger.domain`), else its container. */
function nodeOfFile(f: Fixture, file: string): string | undefined {
  const parts = file.split("/");
  const ids = new Set(f.tree.planned.nodes.map((n) => n.id));
  for (const seg of parts.slice(1).reverse()) {
    const id = `${parts[0]}.${seg}`;
    if (ids.has(id)) return id;
  }
  return ids.has(parts[0]) ? parts[0] : undefined;
}

/** The landing view, as `domain::landing` computes it from the session logs
 *  and the derived status. A session's edits reach a claim through the files
 *  it touched and the claims its stops recorded as reached. */
function landing(f: Fixture, since: number) {
  const states = new Map(f.status.claims.map((s) => [s.respId, s]));
  const touchedBy = new Map<string, Set<string>>();
  const sessions = f.sessions
    .map((log: SessionLog) => {
      const ats = log.entries.map((e) => e.at);
      const start = Math.min(...ats);
      const end = Math.max(...ats);
      if (end < since) return null;
      const touched = new Set(log.entries.flatMap((e) => (e.event === "touch" ? [e.file] : [])));
      const reachedIds = new Set(log.entries.flatMap((e) => (e.event === "reached" ? e.ids : [])));
      const stubs = log.entries.flatMap((e) => (e.event === "stub" ? [`${e.file}#${e.test}`] : []));
      const task = log.entries.find((e) => e.event === "task");
      const row = (c: DerivedClaim) => ({
        respId: c.id,
        nodeId: c.nodeId,
        statement: c.description,
        file: c.file,
        line: c.line,
        state: states.get(c.id)!.state,
      });
      const s = {
        session: log.id,
        title: task && task.event === "task" ? task.text : `Session ${log.id.slice(0, 8)}`,
        startedAt: start,
        endedAt: end,
        newlyProven: [] as ReturnType<typeof row>[],
        newlyPlanned: [] as ReturnType<typeof row>[],
        stillPlanned: [] as ReturnType<typeof row>[],
        newlyStaleOrFailing: [] as ReturnType<typeof row>[],
        survivingMutants: 0,
        newDarkFiles: [...touched].filter((t) => f.tree.darkFiles.includes(t)),
        components: [] as string[],
      };
      for (const c of f.tree.claims) {
        const st = states.get(c.id);
        if (!st) continue;
        const stubbed = stubs.includes(`${c.file}#${c.junitName}`);
        if (!(touched.has(c.file) || reachedIds.has(c.id) || stubbed)) continue;
        const inWindow = (st.recordedAt ?? 0) >= start;
        if (st.state === "proven" && inWindow) s.newlyProven.push(row(c));
        else if (st.state === "planned" && stubbed) s.newlyPlanned.push(row(c));
        else if (st.state === "planned") s.stillPlanned.push(row(c));
        else if (st.state === "stale" || st.state === "failing") s.newlyStaleOrFailing.push(row(c));
      }
      const components = new Set([...touched].flatMap((t) => nodeOfFile(f, t) ?? []));
      s.survivingMutants = [...components].reduce((n, id) => n + (f.status.nodes[id]?.own.survivingMutants ?? 0), 0);
      for (const id of components) {
        const set = touchedBy.get(id) ?? new Set<string>();
        set.add(log.id);
        touchedBy.set(id, set);
      }
      s.components = [...components].sort();
      return s;
    })
    .filter((s) => s !== null)
    .sort((a, b) => b.endedAt - a.endedAt || a.session.localeCompare(b.session));
  // A component two sessions touched, counted at the container: two agents
  // in one crate is the overlap worth seeing.
  const byContainer = new Map<string, Set<string>>();
  for (const [id, ss] of touchedBy) {
    const c = id.split(".")[0];
    const set = byContainer.get(c) ?? new Set<string>();
    ss.forEach((s) => set.add(s));
    byContainer.set(c, set);
  }
  const shared = [...byContainer]
    .filter(([, ss]) => ss.size > 1)
    .map(([nodeId, ss]) => ({ nodeId, sessions: [...ss].sort() }));
  return { since, sessions, shared };
}

// --- invoke -----------------------------------------------------------------------

const SETTINGS = {
  agent: "auto",
  claude: { model: "", effort: "medium" },
  codex: { model: "", effort: "medium" },
  copilot: { model: "", effort: "medium" },
  confirmLaunch: true,
};

function answer(cmd: string, args: Record<string, unknown>): unknown {
  const f = () => fixture();
  switch (cmd) {
    case "read_planned":
      return JSON.stringify(f().tree.planned);
    case "read_model":
      return JSON.stringify(f().tree.committed);
    case "get_model_health":
      return health(f());
    case "get_derived_status":
      return f().status;
    case "get_test_statuses":
      return testStatuses(f());
    case "get_probe_statuses":
      return probeStatuses(f());
    case "get_landing":
      return landing(f(), Number(args.since ?? 0));
    case "get_subagent_settings":
      return SETTINGS;
    case "detect_ai_tools":
      return { claude: true, codex: true, copilot: false };
    case "agent_integration":
    case "set_agent_integration":
      return { on: true, tools: [], gitignore: { part: "gitignore", file: ".scryer/.gitignore", state: "installed" }, notes: [] };
    case "watch_project":
    case "cancel_agent_session":
    case "set_subagent_settings":
    case "open_in_editor":
      return null;
    case "ensure_preview_server":
      throw new Error("no component previews in the film");
    default:
      throw new Error(`the film's backend does not answer ${cmd}`);
  }
}

export function invoke<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  inFlight += 1;
  return new Promise<T>((resolve, reject) => {
    // Answer on a macrotask, as the IPC bridge does, so the app's own
    // sequencing (set state, then re-read) runs the way it does on a desktop.
    setTimeout(() => {
      inFlight -= 1;
      try {
        resolve(answer(cmd, args) as T);
      } catch (e) {
        reject(e);
      }
    }, 0);
  });
}

export const MODEL_CHANGED = { name: "model-changed", payload: MODEL_REF } as const;

// --- Driving the app from the film ----------------------------------------------

/** Where the film wants the app: the event the app listens for. */
export interface Navigate {
  view: "wiki" | "diagram";
  page?: string;
  diagramFocus?: string | null;
  lens: string | null;
}

let lastNav: string | null = null;
let lastVersion = -1;

/** Bring the app to the film's state: serve `f` from now on, send the
 *  navigation when it moved, and the watcher's change events when the
 *  fixture did. True when the app has something to catch up on. */
export function sync(f: Fixture, version: number, nav: Navigate): boolean {
  setFixture(f);
  let sent = false;
  const key = JSON.stringify(nav);
  if (key !== lastNav) {
    lastNav = key;
    emit("navigate", { project: PROJECT, ...nav }, true);
    sent = true;
  }
  if (version !== lastVersion) {
    const first = lastVersion < 0;
    lastVersion = version;
    if (!first) {
      emit(MODEL_CHANGED.name, MODEL_CHANGED.payload);
      emit("test-results-changed", null);
      emit("session-changed", null);
    }
    sent = true;
  }
  return sent;
}

let settledP: Promise<void> = Promise.resolve();

/** Resolves once the app has answered everything it asked and `minMs` has
 *  passed (a diagram lays out and fits its view on its own clock). */
export function waitForApp(minMs: number): Promise<void> {
  settledP = (async () => {
    const start = performance.now();
    let quiet = 0;
    while (quiet < 4 || performance.now() - start < minMs) {
      await new Promise((r) => setTimeout(r, 16));
      quiet = busy() ? 0 : quiet + 1;
    }
  })();
  return settledP;
}

/** The last wait, for what measures the app's DOM (the cursor). */
export function appSettled(): Promise<void> {
  return settledP;
}
