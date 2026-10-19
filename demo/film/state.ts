/**
 * The film is a pure function of the frame: `state(frame)` folds the ordered,
 * frame-stamped event list up to `frame` into a FilmState, and the
 * composition renders that. Durative events (typing, a camera move, a cursor
 * glide, a test run filling in) are evaluated at the frame, so any frame can
 * be rendered on its own — scrubbed in Studio or rendered in parallel.
 */

import { Easing, interpolate } from "remotion";
import {
  BASE_CLAIMS,
  BASE_PROBES,
  BASE_STATES,
  layout,
  sessions as baseSessions,
  statusOf,
  structure,
  treeOf,
} from "../fixture";
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

export type View =
  | { kind: "landing" }
  | { kind: "tree" }
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

export interface CursorState {
  x: number;
  y: number;
  visible: boolean;
  /** The frame of the last click, for its ripple. */
  clickAt?: number;
}

export interface FilmState {
  frame: number;
  /** What the app sees through the invoke stub. */
  fixture: Fixture;
  view: View;
  selection?: string;
  /** The concern lens, e.g. `persistence`. */
  lens?: string;
  camera: Camera;
  caption: Caption | null;
  cursor: CursorState;
  terminals: [TerminalState, TerminalState?];
  /** A claim row that just changed, flashed for a moment. */
  flash?: { ids: string[]; since: number };
}

// --- Events ------------------------------------------------------------------------

type At = { at: number };
export type FilmEvent = At &
  (
    | { kind: "view"; view: View }
    | { kind: "select"; node?: string }
    | { kind: "lens"; concern?: string }
    | { kind: "camera"; to: Camera; dur: number }
    | { kind: "caption"; text: string | null; style?: Caption["style"]; sub?: string }
    | { kind: "cursor"; x: number; y: number; dur: number; click?: boolean; hide?: boolean }
    | { kind: "term.show"; term: 0 | 1; cwd: string }
    | { kind: "term.type"; term: 0 | 1; text: string; dur: number }
    | { kind: "term.submit"; term: 0 | 1 }
    | { kind: "term.line"; term: 0 | 1; line: TermLine }
    | { kind: "term.ok"; term: 0 | 1 }
    | { kind: "term.test"; term: 0 | 1; command: string; tests: TestResult[]; dur: number }
    | { kind: "term.idle"; term: 0 | 1 }
    | { kind: "claims.add"; claims: DerivedClaim[] }
    | { kind: "claims.remove"; ids: string[] }
    | { kind: "claims.state"; ids: string[]; state: ClaimState }
    | { kind: "probe"; id: string; mark?: ProbeMark }
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
  let claims: DerivedClaim[] = BASE_CLAIMS.map((c) => ({ ...c }));
  const states: Record<string, ClaimState> = { ...BASE_STATES };
  const probes: Record<string, ProbeMark> = { ...BASE_PROBES };
  let violations: DerivedViolation[] = [];
  const sessions: SessionLog[] = baseSessions.map((s) => ({ ...s, entries: [...s.entries] }));

  let view: View = { kind: "landing" };
  let selection: string | undefined;
  let lens: string | undefined;
  let tween = { from: HOME, to: HOME, at: 0, dur: 0 };
  let caption: Caption | null = null;
  let glide = { from: { x: 900, y: 600 }, to: { x: 900, y: 600 }, at: 0, dur: 0 };
  let cursor: CursorState = { x: 900, y: 600, visible: false };
  const terminals: [TerminalState, TerminalState?] = [terminal("~/aperture-pay", 0)];
  let flash: FilmState["flash"];

  for (const e of events) {
    if (e.at > frame) break;
    switch (e.kind) {
      case "view":
        view = e.view;
        break;
      case "select":
        selection = e.node;
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
        const t = ease(progress(e.at, glide.at, glide.dur));
        const now = { x: glide.from.x + (glide.to.x - glide.from.x) * t, y: glide.from.y + (glide.to.y - glide.from.y) * t };
        glide = { from: now, to: { x: e.x, y: e.y }, at: e.at, dur: e.dur };
        cursor = { ...cursor, visible: !e.hide, clickAt: e.click ? e.at + e.dur : cursor.clickAt };
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
        claims = [...claims, ...e.claims.map((c) => ({ ...c }))];
        flash = { ids: e.claims.map((c) => c.id), since: e.at };
        break;
      case "claims.remove":
        claims = claims.filter((c) => !e.ids.includes(c.id));
        break;
      case "claims.state":
        for (const id of e.ids) {
          states[id] = e.state;
          const c = claims.find((c) => c.id === id);
          if (c) c.planned = e.state === "planned" || undefined;
        }
        flash = { ids: e.ids, since: e.at };
        break;
      case "probe":
        if (e.mark) probes[e.id] = e.mark;
        else delete probes[e.id];
        flash = { ids: [e.id], since: e.at };
        break;
      case "violation":
        violations = e.violation ? [e.violation] : [];
        break;
      case "session": {
        const log = sessions.find((s) => s.id === e.session);
        if (log) log.entries.push(e.entry);
        else sessions.unshift({ id: e.session, entries: [e.entry] });
        break;
      }
      case "end":
        break;
    }
  }

  const tree = treeOf(claims, violations);
  const glideT = ease(progress(frame, glide.at, glide.dur));
  return {
    frame,
    fixture: { tree, status: statusOf(tree, states, probes), structure, sessions, layout },
    view,
    selection,
    lens,
    camera: mix(tween.from, tween.to, progress(frame, tween.at, tween.dur)),
    caption,
    cursor: {
      ...cursor,
      x: glide.from.x + (glide.to.x - glide.from.x) * glideT,
      y: glide.from.y + (glide.to.y - glide.from.y) * glideT,
    },
    terminals,
    flash,
  };
}
