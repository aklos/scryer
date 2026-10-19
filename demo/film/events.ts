/**
 * The script: the six acts of plans/demo-video.md as one ordered list of
 * frame-stamped events. Each act is written in seconds from its own start;
 * durations are targets — retime an act by moving its start.
 */

import { REFUND_STUBS, STYLE_VIOLATION } from "../fixture";
import { NOW, SESSION_REFUNDS, SESSION_WEBHOOKS } from "../story";
import type { SessionEntry, SessionEvent } from "../types";
import type { Camera, FilmEvent, TermLine, TestResult } from "./state";

export const FPS = 30;

/** Act starts, in seconds. */
export const ACTS = {
  poster: 0,
  plans: 6,
  styles: 24,
  tdd: 44,
  probe: 62,
  swarm: 80,
  close: 96,
  end: 102,
} as const;

/** The poster: act 0's first held frame, the title fully in. */
export const POSTER_FRAME = Math.round(1.5 * FPS);

type Loose = FilmEvent extends infer E ? (E extends FilmEvent ? Omit<E, "at"> : never) : never;

/** Events of one act, stamped `start + t` seconds. */
function act(start: number, beats: [number, Loose][]): FilmEvent[] {
  return beats.map(([t, e]) => ({ ...e, at: Math.round((start + t) * FPS) }) as FilmEvent);
}

const cam = (x: number, y: number, scale: number, dur = 1.2): Loose => ({
  kind: "camera",
  to: { x, y, scale } satisfies Camera,
  dur: Math.round(dur * FPS),
});
const say = (text: string, term: 0 | 1 = 0): Loose => ({ kind: "term.line", term, line: { kind: "say", text } });
const tool = (name: string, target?: string, arg?: string, term: 0 | 1 = 0): Loose => ({
  kind: "term.line",
  term,
  line: { kind: "tool", tool: name, target, arg, status: "run" },
});
const ok = (term: 0 | 1 = 0): Loose => ({ kind: "term.ok", term });
const note = (text: string, term: 0 | 1 = 0): Loose => ({ kind: "term.line", term, line: { kind: "note", text } });
const diff = (file: string, rows: [" " | "+" | "-", string][], term: 0 | 1 = 0): Loose => ({
  kind: "term.line",
  term,
  line: { kind: "diff", file, rows: rows.map(([op, text]) => ({ op, text })) } satisfies TermLine,
});
const type = (text: string, secs: number, term: 0 | 1 = 0): Loose => ({
  kind: "term.type",
  term,
  text,
  dur: Math.round(secs * FPS),
});
const run = (command: string, tests: TestResult[], secs: number, term: 0 | 1 = 0): Loose => ({
  kind: "term.test",
  term,
  command,
  tests,
  dur: Math.round(secs * FPS),
});
/** Glide to the element `target` selects in the app (falling back to the
 *  point), clicking on arrival when asked. */
const cursor = (target: string, x: number, y: number, secs: number, click = false): Loose => ({
  kind: "cursor",
  target,
  x,
  y,
  dur: Math.round(secs * FPS),
  click,
});
const hideCursor: Loose = { kind: "cursor", x: 0, y: 0, dur: 0, hide: true };

/** Glide to a claim's row in the app, by the claim's wording. */
const toClaim = (claim: string, x: number, y: number, secs: number, click = false): Loose => ({
  kind: "cursor",
  claim,
  x,
  y,
  dur: Math.round(secs * FPS),
  click,
});

/** The concern lens, in the tree. */
const LENS = '[title^="Concern lens"]';
/** A line of a session's log, as its hooks write it. */
const log = (session: string, entry: SessionEvent): Loose => ({
  kind: "session",
  session,
  entry: { at: NOW, ...entry } as SessionEntry,
});
const headline = (text: string): Loose => ({ kind: "caption", text, style: "headline" });
const clear: Loose = { kind: "caption", text: null };

/** The claims, by wording: the four refund stubs built, the one deleted, the one probed. */
const BUILT = REFUND_STUBS.slice(0, 4).map((c) => c.description);
const DELETED = REFUND_STUBS[4].description;
const PROBED = REFUND_STUBS[1].description;
const refundTests = (result: TestResult["result"]): TestResult[] =>
  REFUND_STUBS.slice(0, 4).map((c) => ({ name: c.identifier, result }));

/** Every beat, ordered by frame (stable, so same-frame beats keep script order). */
export const EVENTS: FilmEvent[] = ([
  // 0. Poster / cold open — the landing page, brand, hook.
  ...act(ACTS.poster, [
    [0, { kind: "view", view: { kind: "landing" } }],
    [0, { kind: "caption", text: "Your agents wrote the code. This is what's true.", style: "title", sub: "scryer" }],
  ]),

  // 1. Plans without essays.
  ...act(ACTS.plans, [
    [0, clear],
    [0, { kind: "view", view: { kind: "node", node: "Ledger" } }],
    [0.4, type("add partial refunds to the ledger", 1.6)],
    [2.2, { kind: "term.submit", term: 0 }],
    [2.2, log(SESSION_REFUNDS, { event: "task", text: "add partial refunds to the ledger" })],
    [2.6, say("Planning it as todo tests — one claim each, nothing else.")],
    [3.0, tool("orient", "ledger")],
    [3.6, ok()],
    [3.8, tool("write", "ledger/tests/refunds.rs")],
    [4.4, { kind: "claims.add", claims: REFUND_STUBS }],
    [4.4, log(SESSION_REFUNDS, { event: "touch", file: "ledger/tests/refunds.rs" })],
    ...REFUND_STUBS.map((c): [number, Loose] => [4.4, log(SESSION_REFUNDS, { event: "stub", file: c.file, test: c.junitName })]),
    [4.4, diff("ledger/tests/refunds.rs", [
      ["+", "/// When a merchant refunds part of a capture, posts the refund against it"],
      ["+", "#[test] fn refunds_part_of_a_capture() { todo!() }"],
      ["+", "/// When a refund exceeds the captured amount, rejects it"],
      ["+", "#[test] fn rejects_over_refund() { todo!() }"],
      [" ", "… 3 more"],
    ])],
    [4.8, ok()],
    [4.8, cam(700, 500, 1.18, 1.2)],
    [8.6, cam(580, 500, 1, 0.8)],
    [5.4, say("Five todo tests. Delete any you don't want, then say go.")],
    [5.6, { kind: "term.idle", term: 0 }],
    [6.5, headline("The plan is a column of claims. Read it in ten seconds.")],
    [9.0, toClaim(BUILT[3], 700, 400, 1.0)],
    [10.4, toClaim(DELETED, 700, 440, 0.8, true)],
    [11.3, { kind: "claims.remove", claims: [DELETED] }],
    [11.8, hideCursor],
    [12.4, type("go", 0.4)],
    [13.2, { kind: "term.submit", term: 0 }],
    [13.6, say("Building the four.")],
    [15.5, cam(580, 500, 1, 1.2)],
  ]),

  // 2. C4 and architecture styles.
  ...act(ACTS.styles, [
    [0, clear],
    [0, { kind: "view", view: { kind: "diagram", level: "context" } }],
    [0, cam(580, 500, 0.9, 0.1)],
    [0.2, cam(580, 500, 1, 2.2)],
    [2.6, { kind: "view", view: { kind: "diagram", level: "containers" } }],
    [5.0, { kind: "view", view: { kind: "diagram", level: "component", focus: "Ledger" } }],
    [5.2, cam(580, 500, 1, 0.1)],
    [7.0, tool("write", "ledger/src/domain/memory.rs")],
    [7.4, diff("ledger/src/domain/memory.rs", [
      ["+", "use crate::application::store::EntryStore;"],
      ["+", "impl EntryStore for MemoryStore {"],
    ])],
    [7.6, ok()],
    [7.6, log(SESSION_REFUNDS, { event: "touch", file: "ledger/src/domain/memory.rs" })],
    [8.0, { kind: "violation", violation: STYLE_VIOLATION }],
    [9.0, headline("Styles the agent builds to, and cannot drift from.")],
    [10.6, say("domain can't depend on application — the store is an adapter; moving it out.")],
    [11.2, tool("move", "ledger/src/domain/memory.rs → infrastructure/")],
    [11.8, diff("ledger/src/infrastructure/mod.rs", [
      ["+", "pub mod memory;"],
    ])],
    [12.2, ok()],
    [13.0, { kind: "violation" }],
    [15.0, { kind: "view", view: { kind: "diagram", level: "component", focus: "Dashboard" } }],
    [15.0, cam(580, 500, 1, 1.0)],
  ]),

  // 3. TDD, EARS, concerns.
  ...act(ACTS.tdd, [
    [0, clear],
    [0, { kind: "view", view: { kind: "node", node: "Ledger" } }],
    [0.6, tool("get_test_radius", "ledger/src/domain/refund.rs")],
    [1.2, ok()],
    [1.2, cam(700, 520, 1.22, 1.0)],
    [1.6, run("cargo test --test refunds", refundTests("fail"), 1.4)],
    [3.0, { kind: "claims.state", claims: BUILT, state: "failing" }],
    [3.6, tool("write", "ledger/src/domain/refund.rs")],
    [4.0, diff("ledger/src/domain/refund.rs", [
      ["+", "if amount > capture.refundable() { return Err(OverRefund) }"],
      ["+", "ledger.post(Refund::against(capture, amount))"],
    ])],
    [4.4, ok()],
    [4.4, log(SESSION_REFUNDS, { event: "touch", file: "ledger/src/application/ledger.rs" })],
    [5.0, run("cargo test --test refunds", refundTests("pass"), 1.4)],
    [6.4, { kind: "claims.state", claims: BUILT, state: "proven" }],
    [6.8, tool("ingest_test_report", ".scryer/reports/radius.xml")],
    [7.2, ok()],
    [7.2, log(SESSION_REFUNDS, {
        event: "reached",
        ids: [
          ...BUILT,
          "Posts every money movement as balanced entries",
          "If debits and credits differ, then rejects the posting",
          "If a capture exceeds the authorised amount, then rejects it",
        ],
      })],
    [7.2, cam(580, 500, 1, 0.8)],
    [7.6, headline("Every claim is a test. Scan down a component, or across a concern.")],
    [9.6, cursor(LENS, 160, 130, 1.0, true)],
    [10.8, { kind: "lens", concern: "persistence" }],
    [11.4, hideCursor],
    [12.6, { kind: "view", view: { kind: "diagram", level: "containers" } }],
    [16.6, { kind: "lens" }],
  ]),

  // 4. Deterministic checks and probing.
  ...act(ACTS.probe, [
    [0, clear],
    [0, { kind: "view", view: { kind: "node", node: "Ledger" } }],
    [0.8, say("Probing the over-refund guard before I stop.")],
    [1.2, tool("open_probe", "rejects_over_refund")],
    [1.8, ok()],
    [2.0, cam(1100, 470, 1.3, 1.2)],
    [2.2, diff("worktree: ledger/src/domain/refund.rs", [
      ["-", "if amount > capture.refundable() {"],
      ["+", "if amount > capture.refundable() * 2 {"],
    ])],
    [2.8, run("cargo test rejects_over_refund", [{ name: "rejects_over_refund", result: "pass" }], 0.6)],
    [3.8, tool("close_probe", "rejects_over_refund", "1 break, 1 survived")],
    [4.2, ok()],
    [4.4, { kind: "probe", claim: PROBED, mark: "crosshair" }],
    [4.6, note("survived: doubling the cap went unnoticed — the test refunds the full capture only")],
    [5.6, headline("A green test that would actually notice.")],
    [8.0, say("Tightening the test: one cent over the capture must be rejected.")],
    [8.6, diff("ledger/tests/refunds.rs", [
      ["-", "assert!(refund(&l, cap, cap.amount()).is_ok());"],
      ["+", "assert!(refund(&l, cap, cap.amount() + cents(1)).is_err());"],
    ])],
    [9.6, run("cargo test rejects_over_refund", [{ name: "rejects_over_refund", result: "pass" }], 0.6)],
    [10.6, tool("open_probe", "rejects_over_refund")],
    [11.0, ok()],
    [11.4, run("cargo test rejects_over_refund", [{ name: "rejects_over_refund", result: "fail" }], 0.6)],
    [12.4, tool("close_probe", "rejects_over_refund", "1 break, 0 survived")],
    [12.8, ok()],
    [13.0, { kind: "probe", claim: PROBED, mark: "doubleCheck" }],
    [13.4, { kind: "term.idle", term: 0 }],
    [15.0, cam(580, 500, 1, 1.0)],
  ]),

  // 5. Overwatch and swarms.
  ...act(ACTS.swarm, [
    [0, clear],
    [0.3, { kind: "term.show", term: 1, cwd: "~/aperture-pay · webhooks" }],
    [1.0, type("retry failed ledger webhooks first", 1.4, 1)],
    [2.6, { kind: "term.submit", term: 1 }],
    [3.0, tool("orient", "webhooks/src/shell/delivery.rs ledger/src/application/ledger.rs", undefined, 1)],
    [3.6, ok(1)],
    [3.8, note("ledger/src/application/ledger.rs: also edited by live session c3d8e1f7 (add partial refunds…), 12 min ago", 1)],
    [4.4, log(SESSION_WEBHOOKS, { event: "touch", file: "ledger/src/application/ledger.rs" })],
    [4.6, { kind: "view", view: { kind: "landing" } }],
    [5.4, headline("Many agents, one substrate. You watch the claims, not the chat.")],
    [6.0, say("Retrying ledger events ahead of newer ones; leaving ledger.rs to the refunds session.", 1)],
    [7.0, tool("write", "webhooks/src/shell/delivery.rs", undefined, 1)],
    [7.6, ok(1)],
    [8.0, say("Refunds are in: four claims proven, the guard probed.")],
  ]),

  // 6. Close.
  ...act(ACTS.close, [
    [0, { kind: "caption", text: "Tests are the spec.", style: "title", sub: "github.com/aklos/scryer" }],
  ]),
  ...act(ACTS.end, [[0, { kind: "end" }]]),
] satisfies FilmEvent[]).sort((a, b) => a.at - b.at);

/** The film runs to the last event. */
export const FILM_FRAMES = EVENTS.reduce((end, e) => Math.max(end, e.at + ("dur" in e ? e.dur : 0)), 0) + 1;
