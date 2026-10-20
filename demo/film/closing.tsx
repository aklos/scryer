/**
 * Beats 7–11: a concern across the services, a green test that would
 * notice, many agents on one substrate, since you last looked, and the back
 * cover.
 */

import { Img, staticFile } from "remotion";
import { ClaimRows, claim, fixture, nodeId, type Row } from "../shots/app";
import { LandingFigure, ago, landingClaim, session } from "../shots/landing";
import { C, Caption, col, Figure, GROTESK, INK, Label, Lead, MARGIN, MONO, Page, Rule, span, Terminal, W, type TermLine } from "../shots/stage";
import { NOW } from "../story";
import { fadeOut, prog, revealed, s, settle } from "./motion";

const LEDGER = nodeId("Ledger");

// --- 7. Scan across a concern -------------------------------------------------------

export const CONCERN = s(5.2);

/** Ten persistence claims across the five services, with a few of their
 *  neighbours: what the lens leaves lit, and what it dims. (The page lists
 *  a concern's claims together, so the untagged ones follow.) */
const LENS_ROWS: { text: string; tagged: boolean }[] = [
  { text: "When the service restarts, keeps every posted entry", tagged: true },
  { text: "Writes a posting's entries as one journal record", tagged: true },
  { text: "Posts every money movement as balanced entries", tagged: false },
  { text: "Records every accepted request before answering it", tagged: true },
  { text: "Keeps each idempotency key for twenty-four hours", tagged: true },
  { text: "Records every decision with the score behind it", tagged: true },
  { text: "Caches a card's features for an hour", tagged: true },
  { text: "Queues each event durably before sending it", tagged: true },
  { text: "When the sender crashes, delivers the outbox on restart", tagged: true },
  { text: "Keeps a merchant's filters across visits", tagged: true },
  { text: "Caches each response in session storage", tagged: true },
  { text: "Pays each merchant's settled balance out once a day", tagged: false },
  { text: "Shows the next payout's amount and date", tagged: false },
];

export function Concern({ f }: { f: number }) {
  // The lens is on persistence; the claims it does not hold dim under it.
  const lens = prog(f, s(1.2), s(0.6));
  const rows: Row[] = LENS_ROWS.filter((r) => fixture.tree.claims.some((c) => c.description === r.text)).map((r, i) => ({
    claim: claim(r.text),
    state: "proven",
    css: `margin-top: ${i === 0 ? 0 : 4}px !important; opacity: ${r.tagged ? 1 : 1 - 0.75 * lens} !important;`,
  }));
  return (
    <Page beat="07" subject="Scan across a concern">
      <Lead at={{ x: col(0), y: 140 }} width={span(4)} size={56} style={settle(f, 0)}>
        A trailing <span style={{ fontFamily: MONO, fontSize: "0.82em", letterSpacing: "-0.03em" }}>[persistence]</span> tag marks a concern.
      </Lead>
      <Caption at={{ x: col(0), y: 470 }} width={span(4)} style={settle(f, s(0.4))}>
        Scryer lifts the tag with the claim. Turn the lens on and every persistence claim stays lit, across all five services.
      </Caption>
      <Figure at={{ x: col(4), y: 176 }} width={span(8) / 1.9} height={824 / 1.9} scale={1.9} label="Fig. 8 — Claims, persistence lens" style={settle(f, s(0.3))}>
        <div className="shot-grade shot-claims h-full bg-[var(--surface)] px-4 pt-3 text-[var(--text)]">
          <div className="mb-2.5 flex items-center gap-2 pl-1 text-[11px] text-[var(--text-muted)]">
            <span>Concern lens</span>
            <span
              className="rounded border px-1.5 py-0.5 font-mono"
              style={{ borderColor: "var(--accent)", color: "var(--accent)" }}
            >
              persistence · 10
            </span>
          </div>
          <ClaimRows host={LEDGER} rows={rows} />
        </div>
      </Figure>
    </Page>
  );
}

// --- 8. A green test that would notice ----------------------------------------------

export const PROBE = s(9);

const WORKTREE = "~/.scryer/probes/aperture-pay-af330fd3f692dafd";

/** The first round: the break survives. */
const ROUND_ONE: TermLine[] = [
  { kind: "tool", name: "open_probe", arg: "rejects_over_refund" },
  { kind: "out", text: "  Probe OPEN on rejects_over_refund — work ONLY in the probe worktree:" },
  { kind: "out", text: `    ${WORKTREE}`, tone: "accent" },
  { kind: "diff", op: "-", text: "if amount.minor > remaining.minor {" },
  { kind: "diff", op: "+", text: "if amount.minor > remaining.minor * 2 {" },
  { kind: "run", text: "cargo test --test refunds rejects_over_refund" },
  { kind: "out", text: "  test rejects_over_refund ... ok" },
  { kind: "tool", name: "close_probe", arg: "rejects_over_refund" },
  { kind: "out", text: "  Probe CLOSED on rejects_over_refund — worktree reset. 1 break(s) tried, 1 survived.", tone: "bad" },
];
const ROUND_ONE_AT = [s(0.6), s(0.8), s(0.95), s(1.6), s(1.75), s(2.3), s(3.0), s(3.4), s(3.6)];

/** The second round: the assertion tightened, the break caught. */
const ROUND_TWO: TermLine[] = [
  { kind: "diff", op: "-", text: 'let _ = ledger.refund("cap-1", "ref-1", Money::gbp(1500));' },
  { kind: "diff", op: "+", text: 'assert!(ledger.refund("cap-1", "ref-1", Money::gbp(1500)).is_err());' },
  { kind: "tool", name: "open_probe", arg: "rejects_over_refund" },
  { kind: "diff", op: "+", text: "if amount.minor > remaining.minor * 2 {" },
  { kind: "out", text: "  test rejects_over_refund ... FAILED" },
  { kind: "tool", name: "close_probe", arg: "rejects_over_refund" },
  { kind: "out", text: "  Probe CLOSED on rejects_over_refund — worktree reset. 1 break(s) tried, 0 survived.", tone: "ok" },
  { kind: "out", text: "  Every break was caught — the test reads as PROBED. That is a sample, not a proof." },
];
const TWO = s(5.0);
const ROUND_TWO_AT = [TWO + s(0.3), TWO + s(0.45), TWO + s(1.0), TWO + s(1.2), TWO + s(1.8), TWO + s(2.2), TWO + s(2.4), TWO + s(2.55)];

export function Probe({ f }: { f: number }) {
  const one = f < TWO;
  const shown = (one ? ROUND_ONE_AT : ROUND_TWO_AT).filter((at) => f >= at).length;
  const crosshair = f >= ROUND_ONE_AT[8] + s(0.15);
  const doubled = f >= ROUND_TWO_AT[6] + s(0.15);
  const state = doubled ? "doubleCheck" : crosshair ? "crosshair" : "proven";
  const markAt = doubled ? ROUND_TWO_AT[6] + s(0.15) : ROUND_ONE_AT[8] + s(0.15);
  const pop = f >= markAt ? Math.sin(Math.PI * Math.min(1, (f - markAt) / s(0.35))) : 0;
  const swap = one ? fadeOut(f, TWO - s(0.3), s(0.3)) : prog(f, TWO, s(0.3));
  return (
    <Page beat="08" subject="A green test that would notice">
      <Lead at={{ x: col(0), y: 140 }} width={span(7)} style={settle(f, 0)}>
        Scryer breaks the code in a worktree and runs the one test.
      </Lead>
      <Caption at={{ x: col(8), y: 150 }} width={span(4)} style={{ ...settle(f, s(3.8)), opacity: Number(settle(f, s(3.8)).opacity) * (one ? fadeOut(f, TWO - s(0.3), s(0.3)) : 0) }}>
        It stayed green, so it never checked the guard. Your working tree is untouched.
      </Caption>
      <Caption at={{ x: col(8), y: 150 }} width={span(4)} style={settle(f, TWO + s(2.6))}>
        The agent tightens the assertion and probes again; the break is caught. Mutation reports from cargo-mutants or Stryker land the same way.
      </Caption>
      <Figure at={{ x: col(0), y: 400 }} width={576} height={44} scale={3} label="Fig. 9 — Ledger" style={settle(f, s(0.2))}>
        <div className="shot-grade shot-claims h-full bg-[var(--surface)] px-3 pt-[9px] text-[var(--text)]">
          <ClaimRows
            host={LEDGER}
            rows={[
              {
                claim: claim("When a refund exceeds the captured amount, rejects it"),
                state,
                css: `--pop: ${pop};`,
              },
            ]}
          />
        </div>
      </Figure>
      <style>{`.shot-claims li span.absolute.right-1 svg:last-child { transform: scale(calc(1 + 0.45 * var(--pop, 0))); transform-origin: center; }`}</style>
      <Terminal
        at={{ x: col(0), y: 616 }}
        width={span(12)}
        label={one ? "The agent's session" : "The agent's session, second round"}
        lines={one ? ROUND_ONE : ROUND_TWO}
        shown={shown}
        style={{ ...settle(f, s(0.4)), opacity: Number(settle(f, s(0.4)).opacity) * swap }}
      />
    </Page>
  );
}

// --- 9. Many agents, one substrate ---------------------------------------------------

export const AGENTS = s(7);

const FIRST = "c3d8e1f7";
const SECOND = "7be2d510";

export function Agents({ f }: { f: number }) {
  const left: TermLine[] = [
    { kind: "prompt", text: "add partial refunds to the ledger" },
    { kind: "tool", name: "write", arg: "crates/ledger/src/application/ledger.rs" },
    { kind: "run", text: "cargo test --test refunds" },
  ];
  const right: TermLine[] = [
    { kind: "prompt", text: "retry failed ledger webhooks first" },
    { kind: "tool", name: "orient", arg: "crates/ledger/src/application/ledger.rs" },
    { kind: "out", text: '  "alsoEditing": "Also edited by live', tone: "accent" },
    { kind: "out", text: `  session ${FIRST} (add partial refunds`, tone: "accent" },
    { kind: "out", text: '  to the ledger)"', tone: "accent" },
  ];
  return (
    <Page beat="09" subject="Many agents, one substrate">
      <Lead at={{ x: col(0), y: 140 }} width={span(7)} style={settle(f, 0)}>
        Two agents, two tasks. Nothing locks.
      </Lead>
      <Caption at={{ x: col(8), y: 150 }} width={span(4)} style={settle(f, s(2.6))}>
        Session hooks record which files each agent touched; <C>orient</C> tells the second agent that the first is already there.
      </Caption>
      <Terminal at={{ x: col(0), y: 400 }} width={span(6)} label={`Session ${FIRST}`} lines={left} shown={revealed(f, s(0.4), s(0.35), 3)} style={settle(f, s(0.2))} />
      <Terminal
        at={{ x: col(6), y: 400 }}
        width={span(6)}
        label={`Session ${SECOND}`}
        lines={right}
        shown={f < s(1.6) ? 0 : f < s(2.1) ? 1 : f < s(2.5) ? 2 : 5}
        style={settle(f, s(1.2))}
      />
      <Figure at={{ x: col(0), y: 790 }} width={span(12) / 2} height={104} scale={2} label="The landing page" style={settle(f, s(3.6))}>
        <div style={{ marginTop: -142 }}>
          <LandingFigure
            since={ago(180)}
            landing={{
              sessions: [
                session({ session: `${SECOND}-webhooks`, title: "retry failed ledger webhooks first", startedAt: ago(14), endedAt: ago(11) }),
                session({ session: `${FIRST}-refunds`, title: "add partial refunds to the ledger", startedAt: ago(40), endedAt: ago(12) }),
              ],
              shared: [{ nodeId: LEDGER, sessions: [`${FIRST}-refunds`, `${SECOND}-webhooks`] }],
            }}
          />
        </div>
      </Figure>
    </Page>
  );
}

// --- 10. Since you last looked ---------------------------------------------------------

export const SINCE = s(6);

export function Since({ f }: { f: number }) {
  const day = 24 * 60;
  return (
    <Page beat="10" subject="Since you last looked">
      <Lead at={{ x: col(0), y: 140 }} width={span(4)} size={56} style={settle(f, 0)}>
        A week later, this is what you open.
      </Lead>
      <Caption at={{ x: col(0), y: 400 }} width={span(4)} style={settle(f, s(0.5))}>
        What turned proven, what is still todo, what went stale. A verdict goes stale when the test, or the code it exercises, changes under it.
      </Caption>
      <style>
        {[1, 2, 3]
          .map((k) => {
            const p = prog(f, s(0.9) + (k - 1) * s(0.55), s(0.6));
            return `.film-since section:nth-of-type(${k}) { opacity: ${p}; transform: translateY(${(1 - p) * 10}px); }`;
          })
          .join("\n")}
      </style>
      <Figure at={{ x: col(4), y: 176 }} width={span(8) / 2} height={412} scale={2} label="Fig. 10 — Since you last looked" style={settle(f, s(0.2))}>
        <div style={{ marginTop: -150, height: 412 + 150 }}>
        <LandingFigure
          className="film-since"
          since={NOW - 7 * day * 60}
          landing={{
            sessions: [
              session({
                session: `${FIRST}-refunds`,
                title: "add partial refunds to the ledger",
                startedAt: ago(6 * day + 40),
                endedAt: ago(6 * day + 12),
                newlyProven: [
                  "When a merchant refunds part of a capture, posts the refund against it",
                  "When a refund exceeds the captured amount, rejects it",
                  "Tracks the refundable remainder of each capture",
                  "Refunds in the currency of the capture",
                ].map((t) => landingClaim(t, "proven")),
              }),
              session({
                session: "a41f9c02-disputes",
                title: "hold disputed funds back from payouts",
                startedAt: ago(day + 95),
                endedAt: ago(day + 60),
                stillPlanned: [landingClaim("While a dispute is open, holds the disputed amount back from payouts", "planned")],
                newlyStaleOrFailing: [landingClaim("Nets fees out of each payout", "stale")],
              }),
              session({
                session: `${SECOND}-webhooks`,
                title: "retry failed ledger webhooks first",
                startedAt: ago(5 * day + 14),
                endedAt: ago(5 * day + 11),
                stillPlanned: [landingClaim("When a ledger event fails to send, retries it before newer events", "planned")],
              }),
            ],
            shared: [],
          }}
        />
        </div>
      </Figure>
    </Page>
  );
}

// --- 11. Back cover ------------------------------------------------------------------

export const BACK = s(4.2);

export function Back({ f }: { f: number }) {
  const column = (x: number, title: string, lines: [string, string][], at: number, keyWidth: number) => (
    <div style={{ position: "absolute", left: x, top: 640, width: span(6) - 24, ...settle(f, at) }}>
      <Label at={{ x: 0, y: 0 }}>{title}</Label>
      {lines.map(([a, b], i) => (
        <div key={a} style={{ position: "absolute", top: 56 + i * 48, left: 0, display: "flex", fontFamily: MONO, fontSize: 24, letterSpacing: "-0.02em", width: span(6) - 24, whiteSpace: "nowrap" }}>
          <span style={{ width: keyWidth, flexShrink: 0, color: INK.type }}>{a}</span>
          <span style={{ color: INK.soft }}>{b}</span>
        </div>
      ))}
    </div>
  );
  return (
    <Page beat="11" subject="Scryer">
      <div style={{ position: "absolute", left: col(0), top: 220, display: "flex", alignItems: "center", gap: 40, ...settle(f, 0, s(0.9)) }}>
        <Img src={staticFile("logo.png")} style={{ width: 128, height: 128 }} />
        <span style={{ fontFamily: GROTESK, fontSize: 128, fontWeight: 500, letterSpacing: "-0.045em", color: INK.type }}>Scryer</span>
      </div>
      <div style={{ position: "absolute", left: col(0), top: 420, fontFamily: MONO, fontSize: 40, letterSpacing: "-0.02em", color: INK.accent, ...settle(f, s(0.5)) }}>
        github.com/aklos/scryer
      </div>
      <Rule y={590} style={{ opacity: prog(f, s(0.9), s(0.6)) }} />
      {column(
        col(0),
        "Languages",
        [
          ["Import graph", "Rust, TypeScript, Python, Go, Clojure"],
          ["Heuristics", "Java, Ruby, C and C++, C#, PHP"],
          ["Claims", "Rust, Vitest and Jest, pytest, .NET"],
        ],
        s(1.1),
        210,
      )}
      {column(
        col(6),
        "Written into your repo",
        [
          [".scryer/structure.toml", "committed"],
          [".scryer/layout", "committed"],
          ["caches, sessions", "gitignored"],
        ],
        s(1.4),
        400,
      )}
      <Label at={{ x: W - MARGIN, y: 1006 }} align="right" style={settle(f, s(1.8))}>
        Tests are the spec.
      </Label>
    </Page>
  );
}
