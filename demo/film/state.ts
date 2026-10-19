/**
 * The film is a pure function of the frame: `state(frame)` folds the ordered,
 * frame-stamped event list up to `frame` into a FilmState, and the
 * composition renders that. Durative events (typing, a camera move, a cursor
 * glide, a test run filling in) are evaluated at the frame, so any frame can
 * be rendered on its own — scrubbed in Studio or rendered in parallel.
 *
 * The fold plays over a base: demo/fixture.json when a derivation from a real
 * project wrote it, else fixture.ts (the bundler picks, see
 * remotion.config.ts). The script names claims by their wording and nodes by
 * their names, never by id or position, so either base plays the same script.
 */

import { Easing, interpolate } from "remotion";
import base from "@film/base";
import { NOW, REFUND_STUBS, REFUND_VIOLATION } from "../fixture";
import { resolveNode, statusOf, systemOf, treeWith } from "../shapes";
import type {
  ClaimState,
  DerivedClaim,
  DerivedViolation,
  Fixture,
  ProbeMark,
  SessionEntry,
  SessionLog,
} from "../types";

// --- Terminal ------------------------------------------------------------------

export interface DiffRow {
  op: "+" | "-" | " ";
  text: string;
}

/** One test in a run line: `pass` / `fail`, or still running. */
export interface TestResult {
  name: string;
  result: "pass" | "fail";
}

export type TermLine =
  | { kind: "user"; text: string }
  | { kind: "say"; text: string }
  | { kind: "tool"; tool: string; target?: string; arg?: string; status: "run" | "ok" }
  | { kind: "note"; text: string }
  | { kind: "diff"; file: string; rows: DiffRow[] }
  /** A dotted test run: the first `filled` tests show their ✓/✗, the rest a dot. */
  | { kind: "test"; command: string; tests: TestResult[]; filled: number };

export interface TerminalState {
  /** Shown in the tab, e.g. "~/aperture-pay". */
  cwd: string;
  /** The user's request, typed in char by char. */
  input: string;
  /** The agent is working: the caret rests and a running tool spins. */
  running: boolean;
  lines: TermLine[];
  /** The frame the terminal appeared, for its slide-in. */
  since: number;
}

// --- Film state --------------------------------------------------------------------

/** A node is named as a reader would (`Ledger`, `Ledger/domain`); the fold
 *  hands the app its id. A diagram's `containers` level is the system's. */
export type View =
  | { kind: "landing" }
  | { kind: "node"; node: string }
  | { kind: "diagram"; level: "context" | "containers" | "component"; focus?: string };

/** Zoom `scale` about the point (`x`, `y`) of the scryer window. */
export interface Camera {
  x: number;
  y: number;
  scale: number;
}

export interface Caption {
  text: string;
  /** `headline`: the sliding bottom headline. `title`: the centred brand card. */
  style: "headline" | "title";
  sub?: string;
  since: number;
}

/** Where the pointer aims: the centre of the first element `target` selects
 *  in the mounted app, else the fixed point (film pixels). */
export interface CursorPoint {
  x: number;
  y: number;
  target?: string;
}

export interface CursorState {
  /** The glide in progress, `t` from 0 to 1 (eased). */
  from: CursorPoint;
  to: CursorPoint;
  t: number;
  visible: boolean;
  /** The frame of the last click, for its ripple. */
  clickAt?: number;
}

export interface FilmState {
  frame: number;
  /** What the app sees through the invoke stub. */
  fixture: Fixture;
  /** How many fixture changes are applied — the stub tells the app to
   *  re-read when this moves. */
  version: number;
  /** With its node resolved to an id. */
  view: View;
  /** The concern lens, e.g. `persistence`. */
  lens?: string;
  camera: Camera;
  caption: Caption | null;
  cursor: CursorState;
  terminals: [TerminalState, TerminalState?];
}

// --- Events ------------------------------------------------------------------------

type At = { at: number };
export type FilmEvent = At &
  (
    | { kind: "view"; view: View }
    | { kind: "lens"; concern?: string }
    | { kind: "camera"; to: Camera; dur: number }
    | { kind: "caption"; text: string | null; style?: Caption["style"]; sub?: string }
    | {
        kind: "cursor";
        x: number;
        y: number;
        /** A selector in the app, or a claim by its wording (its row). */
        target?: string;
        claim?: string;
        dur: number;
        click?: boolean;
        hide?: boolean;
      }
    | { kind: "term.show"; term: 0 | 1; cwd: string }
    | { kind: "term.type"; term: 0 | 1; text: string; dur: number }
    | { kind: "term.submit"; term: 0 | 1 }
    | { kind: "term.line"; term: 0 | 1; line: TermLine }
    | { kind: "term.ok"; term: 0 | 1 }
    | { kind: "term.test"; term: 0 | 1; command: string; tests: TestResult[]; dur: number }
    | { kind: "term.idle"; term: 0 | 1 }
    /** Claims to add; a base that already holds one by its wording gives
     *  its own file, line and id. `nodeId` is a node name. */
    | { kind: "claims.add"; claims: DerivedClaim[] }
    | { kind: "claims.remove"; claims: string[] }
    | { kind: "claims.state"; claims: string[]; state: ClaimState }
    | { kind: "probe"; claim: string; mark?: ProbeMark }
    | { kind: "violation"; violation?: DerivedViolation }
    | { kind: "session"; session: string; entry: SessionEntry }
    | { kind: "end" }
  );

const HOME: Camera = { x: 570, y: 500, scale: 1 };
const ease = Easing.bezier(0.45, 0, 0.2, 1);

function progress(frame: number, at: number, dur: number): number {
  if (dur <= 0) return 1;
  return interpolate(frame, [at, at + dur], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp" });
}

function mix(a: Camera, b: Camera, t: number): Camera {
  const k = ease(t);
  return { x: a.x + (b.x - a.x) * k, y: a.y + (b.y - a.y) * k, scale: a.scale + (b.scale - a.scale) * k };
}

const terminal = (cwd: string, since: number): TerminalState => ({ cwd, input: "", running: false, lines: [], since });

/** Fold `events` (ordered by `at`) up to `frame`. */
export function state(events: FilmEvent[], frame: number): FilmState {
  const fixture = base as Fixture;
  const model = fixture.tree.planned;
  // What the script adds is not there at the start, whatever the base holds.
  const added = new Set(REFUND_STUBS.map((c) => c.description));
  let claims: DerivedClaim[] = fixture.tree.claims.filter((c) => !added.has(c.description)).map((c) => ({ ...c }));
  const states: Record<string, ClaimState> = {};
  const probes: Record<string, ProbeMark> = {};
  const recorded: Record<string, number> = {};
  for (const s of fixture.status.claims) {
    states[s.respId] = s.state;
    if (s.probe) probes[s.respId] = s.probe;
    if (s.recordedAt !== undefined) recorded[s.respId] = s.recordedAt;
  }
  let version = 0;
  let violations: DerivedViolation[] = fixture.tree.styleViolations.filter((v) => v.detail !== REFUND_VIOLATION.detail);
  const sessions: SessionLog[] = fixture.sessions.map((s) => ({ ...s, entries: [...s.entries] }));
  /** A claim's id by its wording. */
  const idOf = (text: string) => claims.find((c) => c.description === text)?.id ?? text;
  const node = (ref: string) => resolveNode(model, ref) ?? ref;

  let view: View = { kind: "landing" };
  let lens: string | undefined;
  let tween = { from: HOME, to: HOME, at: 0, dur: 0 };
  let caption: Caption | null = null;
  const rest: CursorPoint = { x: 900, y: 600 };
  let glide = { from: rest, to: rest, at: 0, dur: 0 };
  let visible = false;
  let clickAt: number | undefined;
  const terminals: [TerminalState, TerminalState?] = [terminal("~/aperture-pay", 0)];

  for (const e of events) {
    if (e.at > frame) break;
    if (e.kind.startsWith("claims.") || e.kind === "probe" || e.kind === "violation" || e.kind === "session") {
      version += 1;
    }
    switch (e.kind) {
      case "view":
        view = e.view;
        break;
      case "lens":
        lens = e.concern;
        break;
      case "camera": {
        const now = mix(tween.from, tween.to, progress(e.at, tween.at, tween.dur));
        tween = { from: now, to: e.to, at: e.at, dur: e.dur };
        break;
      }
      case "caption":
        caption = e.text === null ? null : { text: e.text, style: e.style ?? "headline", sub: e.sub, since: e.at };
        break;
      case "cursor": {
        // A new glide starts from where the last one was headed.
        const target = e.claim ? `#resp-${CSS.escape(idOf(e.claim))}` : e.target;
        glide = { from: glide.to, to: { x: e.x, y: e.y, target }, at: e.at, dur: e.dur };
        visible = !e.hide;
        if (e.click) clickAt = e.at + e.dur;
        break;
      }
      case "term.show":
        terminals[e.term] = terminal(e.cwd, e.at);
        break;
      case "term.type": {
        const t = terminals[e.term]!;
        const n = Math.floor(progress(frame, e.at, e.dur) * e.text.length);
        t.input = e.text.slice(0, n);
        break;
      }
      case "term.submit": {
        const t = terminals[e.term]!;
        t.lines = [...t.lines, { kind: "user", text: t.input }];
        t.input = "";
        t.running = true;
        break;
      }
      case "term.line": {
        const t = terminals[e.term]!;
        t.lines = [...t.lines, e.line];
        break;
      }
      case "term.ok": {
        const t = terminals[e.term]!;
        const i = t.lines.map((l) => l.kind === "tool" && l.status === "run").lastIndexOf(true);
        if (i >= 0) {
          t.lines = t.lines.map((l, j) => (j === i && l.kind === "tool" ? { ...l, status: "ok" } : l));
        }
        break;
      }
      case "term.test": {
        const t = terminals[e.term]!;
        const filled = Math.floor(progress(frame, e.at, e.dur) * e.tests.length);
        t.lines = [...t.lines, { kind: "test", command: e.command, tests: e.tests, filled }];
        break;
      }
      case "term.idle":
        terminals[e.term]!.running = false;
        break;
      case "claims.add":
        for (const c of e.claims) {
          const own = fixture.tree.claims.find((b) => b.description === c.description);
          claims = [...claims, { ...(own ?? c), nodeId: own?.nodeId ?? node(c.nodeId), planned: true }];
        }
        break;
      case "claims.remove": {
        const gone = new Set(e.claims.map(idOf));
        claims = claims.filter((c) => !gone.has(c.id));
        break;
      }
      case "claims.state":
        for (const id of e.claims.map(idOf)) {
          states[id] = e.state;
          recorded[id] = NOW;
          const c = claims.find((c) => c.id === id);
          if (c) c.planned = e.state === "planned" || undefined;
        }
        break;
      case "probe":
        if (e.mark) probes[idOf(e.claim)] = e.mark;
        else delete probes[idOf(e.claim)];
        break;
      case "violation":
        violations = fixture.tree.styleViolations.filter((v) => v.detail !== REFUND_VIOLATION.detail);
        if (e.violation) {
          const v = e.violation;
          violations = [...violations, { ...v, nodeId: node(v.nodeId), other: v.other && node(v.other) }];
        }
        break;
      case "session": {
        // A stop's `reached` names claims by wording in the script.
        const entry = e.entry.event === "reached" ? { ...e.entry, ids: e.entry.ids.map(idOf) } : e.entry;
        const log = sessions.find((s) => s.id === e.session);
        if (log) log.entries.push(entry);
        else sessions.unshift({ id: e.session, entries: [entry] });
        break;
      }
      case "end":
        break;
    }
  }

  const tree = treeWith(fixture.tree, claims, violations);
  const resolved: View =
    view.kind === "node"
      ? { kind: "node", node: node(view.node) }
      : view.kind === "diagram"
        ? {
            ...view,
            focus: view.level === "containers" ? systemOf(model) : view.focus === undefined ? undefined : node(view.focus),
          }
        : view;
  return {
    frame,
    fixture: { ...fixture, tree, status: statusOf(tree, states, probes, recorded), sessions },
    version,
    view: resolved,
    lens,
    camera: mix(tween.from, tween.to, progress(frame, tween.at, tween.dur)),
    caption,
    cursor: { from: glide.from, to: glide.to, t: ease(progress(frame, glide.at, glide.dur)), visible, clickAt },
    terminals,
  };
}
