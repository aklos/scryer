/**
 * Beats 1–4: the cover, a claim is a test, the plan is a column of claims,
 * proven one by one.
 */

import type { ReactNode } from "react";
import { ClaimRows, TestChip, claim, nodeId, placedModel, type Row } from "../shots/app";
import { Diagram } from "../shots/diagram";
import { Caption, Code, col, Figure, GROTESK, INK, Label, Lead, MARGIN, MONO, Page, Rule, span, SYNTAX as S, Terminal, tok, W } from "../shots/stage";
import { fadeOut, prog, revealed, s, settle, typedChars } from "./motion";

const LEDGER = nodeId("Ledger");

// --- 1. Cover --------------------------------------------------------------------

export const COVER = s(4.6);

export function Cover({ f }: { f: number }) {
  return (
    <Page beat="01" subject="A walkthrough">
      <div
        style={{
          position: "absolute",
          left: col(0),
          top: 196,
          width: span(5),
          fontFamily: GROTESK,
          fontSize: 66,
          fontWeight: 500,
          letterSpacing: "-0.035em",
          lineHeight: 1.06,
          color: INK.type,
          ...settle(f, s(0.5), s(0.9)),
        }}
      >
        Scryer derives a C4 model of a codebase from its packaging and its tests, and keeps it verified.
      </div>
      <Figure at={{ x: col(5) + 40, y: 196 }} width={span(7) - 40} height={760} scale={1} label="Fig. 1 — Aperture Pay, system context" style={settle(f, 0, s(0.9))}>
        <Diagram
          scenes={[{ model: placedModel, focusId: null }]}
          from={0}
          width={span(7) - 40}
          height={760}
          fit={36}
          film={{ title: 25, count: 20, label: 26, flag: 2, edge: 2 }}
        />
      </Figure>
      <Rule y={986} n={5} style={{ opacity: prog(f, s(1), s(0.6)) }} />
      <Label at={{ x: MARGIN, y: 1006 }} style={{ opacity: prog(f, s(1.1), s(0.6)) }}>
        Shot on aperture-pay, a demo payments platform
      </Label>
    </Page>
  );
}

// --- 2. A claim is a test ---------------------------------------------------------

export const CLAIM = s(5.6);

const REFUND_GUARD = "When a refund exceeds the captured amount, rejects it";

const RUST: ReactNode[] = [
  tok(INK.type, `/// ${REFUND_GUARD}`),
  tok(S.constant, "#[test]"),
  <>
    {tok(S.keyword, "fn ")}
    {tok(S.fn, "rejects_over_refund")}
    {tok(S.punct, "() {")}
  </>,
];

export function Claim({ f }: { f: number }) {
  const CODE_Y = 486;
  const ROW_Y = 806;
  // The comment's own line, then where the row's sentence sits.
  const from = { x: col(0) + 32 + 3 * 18.2, y: CODE_Y + 22, size: 30 };
  const to = { x: col(0) + 3 * 47, y: ROW_Y + 3 * 10, size: 42 };
  const lift = prog(f, s(1.6), s(1.1), "move");
  const landed = prog(f, s(2.55), s(0.35));
  const flying = f >= s(1.6) && landed < 1;
  const leader = prog(f, s(2.9), s(0.5));
  const DOC_MID = CODE_Y + 22 + 24;
  const ROW_MID = ROW_Y + 66;
  return (
    <Page beat="02" subject="A claim is a test">
      <Lead at={{ x: col(0), y: 140 }} width={span(7)} style={settle(f, 0)}>
        Scryer reads the claim straight from the test's doc comment.
      </Lead>
      <Caption at={{ x: col(8), y: 150 }} width={span(4)} style={settle(f, s(0.3))}>
        No model to maintain. An <span style={{ fontFamily: MONO, fontSize: "0.88em", color: INK.type }}>it()</span> name, a pytest docstring or an xUnit DisplayName reads the same way.
      </Caption>
      <Code
        at={{ x: col(0), y: CODE_Y }}
        width={span(12)}
        label="Fig. 2 — crates/ledger/tests/refunds.rs"
        lines={RUST}
        style={settle(f, s(0.5))}
        lineStyle={(i) => (i === 0 ? { opacity: 1 - 0.62 * prog(f, s(1.6), s(0.5)) } : undefined)}
      />
      {/* The comment, lifting out of the code and settling as the row's sentence. */}
      {flying && (
        <div
          style={{
            position: "absolute",
            left: from.x + (to.x - from.x) * lift,
            top: from.y + (to.y - from.y) * lift,
            fontFamily: MONO,
            fontSize: from.size + (to.size - from.size) * lift,
            letterSpacing: "-0.015em",
            lineHeight: 1.6,
            whiteSpace: "pre",
            color: INK.type,
            opacity: 1 - landed,
          }}
        >
          {lift < 0.15 ? `/// ${REFUND_GUARD}` : REFUND_GUARD}
        </div>
      )}
      <Figure at={{ x: col(0), y: ROW_Y }} width={576} height={44} scale={3} label="The claim, as Scryer shows it on Ledger" style={{ opacity: landed }}>
        <div className="shot-grade shot-claims h-full bg-[var(--surface)] px-3 pt-[9px] text-[var(--text)]">
          <ClaimRows host={LEDGER} rows={[{ claim: claim(REFUND_GUARD), state: "proven" }]} />
        </div>
      </Figure>
      {/* The leader from the comment to the claim it became. */}
      <div style={{ position: "absolute", left: 60, top: DOC_MID, width: 36 * leader, height: 2, background: INK.accent }} />
      <div style={{ position: "absolute", left: 60, top: DOC_MID, width: 2, height: (ROW_MID - DOC_MID + 2) * leader, background: INK.accent }} />
      <div style={{ position: "absolute", left: 60, top: ROW_MID, width: 36 * prog(f, s(3.3), s(0.3)), height: 2, background: INK.accent }} />
    </Page>
  );
}

// --- 3. The plan is a column of claims --------------------------------------------

export const PLAN = s(7.4);

export const FIVE = [
  "When a merchant refunds part of a capture, posts the refund against it",
  "When a refund exceeds the captured amount, rejects it",
  "Tracks the refundable remainder of each capture",
  "Refunds in the currency of the capture",
  "When a refund posts, notifies the merchant",
];

/** The agent's session in beat 3, line by line. */
const PLAN_SESSION = [
  { kind: "prompt" as const, text: "add partial refunds to the ledger" },
  { kind: "tool" as const, name: "orient", arg: "crates/ledger/tests/refunds.rs" },
  { kind: "out" as const, text: '  "path": "Aperture Pay / Ledger",' },
  { kind: "out" as const, text: '  "stubs": "crates/ledger/tests/refunds.rs"', tone: "accent" as const },
  { kind: "tool" as const, name: "write", arg: "crates/ledger/tests/refunds.rs" },
  { kind: "diff" as const, op: "+" as const, text: "#[test] fn refunds_part_of_a_capture() { todo!() }" },
  { kind: "out" as const, text: "  … four more todo tests", tone: "dim" as const },
  { kind: "prompt" as const, text: "go" },
];

/** Frames at which each session line appears, and when the prompt types. */
const PLAN_AT = [s(0.4), s(1.5), s(1.75), s(1.95), s(2.4), s(2.6), s(2.75), s(6.2)];

export function Plan({ f }: { f: number }) {
  const shown = PLAN_AT.filter((at) => f >= at).length;
  const typing = shown === 1 ? typedChars(f, PLAN_AT[0], 36) : shown === 8 ? typedChars(f, PLAN_AT[7], 20) : undefined;
  // Rows arrive 120 ms apart once the stubs are written.
  const arrived = revealed(f, s(3.0), 4, 5);
  // The developer deletes the fifth: a click, then the row folds away.
  const fold = prog(f, s(5.55), s(0.45), "move");
  const rows: Row[] = FIVE.map((text, i) => {
    const p = prog(f, s(3.0) + i * 4, s(0.5));
    const css =
      i < arrived
        ? i === 4
          ? `opacity: ${0.42 * p * (1 - fold)} !important; transform: translateY(${(1 - p) * 8}px); max-height: ${40 * (1 - fold)}px; margin-top: ${10 * (1 - fold)}px !important; overflow: hidden;`
          : `opacity: ${0.42 * p} !important; transform: translateY(${(1 - p) * 8}px);`
        : "opacity: 0 !important;";
    return { claim: claim(text), state: "planned", css };
  });
  const FIG_Y = 646;
  // The cursor's path: in from the right, onto the fifth row, a click.
  const cur = prog(f, s(4.5), s(0.9), "move");
  const cursorOn = f >= s(4.4) && f < s(6.1);
  const target = { x: col(0) + 1060, y: FIG_Y + 2 * (8 + 4 * 36 + 12) };
  const start = { x: W - 220, y: 1040 };
  const click = prog(f, s(5.45), s(0.3));
  return (
    <Page beat="03" subject="The plan is a column of claims">
      <Terminal
        at={{ x: col(0), y: 178 }}
        width={span(7)}
        label="The agent's session"
        lines={PLAN_SESSION}
        shown={shown}
        typed={typing}
        style={settle(f, 0)}
      />
      <Caption at={{ x: col(8), y: 178 }} width={span(4)} style={settle(f, s(1.9))}>
        <span style={{ fontFamily: MONO, fontSize: "0.88em", color: INK.type }}>orient</span> tells the agent where stubs go. It writes five todo tests: each one a claim you can read before any code exists.
      </Caption>
      <Caption at={{ x: col(8), y: 466 }} width={span(4)} style={settle(f, s(5.0))}>
        You cut the one you don't want, and say go.
      </Caption>
      <Figure at={{ x: col(0), y: FIG_Y }} width={864} height={176} scale={2} label="Fig. 3 — Ledger, planned" style={settle(f, s(2.9))}>
        <div className="shot-grade shot-claims h-full bg-[var(--surface)] px-4 pt-2 text-[var(--text)]">
          <ClaimRows host={LEDGER} rows={rows} />
        </div>
      </Figure>
      {cursorOn && (
        <svg
          width={30}
          height={40}
          viewBox="0 0 15 20"
          style={{
            position: "absolute",
            left: start.x + (target.x - start.x) * cur,
            top: start.y + (target.y - start.y) * cur,
            transform: `scale(${1 - 0.12 * Math.sin(Math.PI * click)})`,
            transformOrigin: "0 0",
            opacity: fadeOut(f, s(5.85), s(0.25)),
          }}
        >
          <path d="M1 1 L1 16 L5 12.5 L8 19 L10.5 18 L7.5 11.5 L13 11.5 Z" fill={INK.type} stroke={INK.page} strokeWidth={1.2} strokeLinejoin="round" />
        </svg>
      )}
    </Page>
  );
}

// --- 4. Proven, one by one --------------------------------------------------------

export const PROVEN = s(6.4);
const BEFORE = 64;

export function Proven({ f }: { f: number }) {
  const FOUR = FIVE.slice(0, 4);
  const ingested = s(2.6);
  const flips = FOUR.map((_, i) => ingested + s(0.35) + i * s(0.32));
  const proven = flips.filter((at) => f >= at).length;
  const lines = [
    { kind: "run" as const, text: "cargo test --test refunds 2>&1 | scripts/cargo-junit.sh > target/junit/cargo.xml" },
    { kind: "tool" as const, name: "ingest_test_report", arg: "target/junit/cargo.xml", accent: true },
    { kind: "out" as const, text: "  Ingested 4 case(s) from target/junit/cargo.xml — verdicts recorded for 4 claim(s)." },
  ];
  const at = [s(0.6), ingested, ingested + s(0.2)];
  const shown = at.filter((a) => f >= a).length;
  return (
    <Page beat="04" subject="Proven, one by one">
      <Lead at={{ x: col(0), y: 132 }} width={span(12)} style={settle(f, 0)}>
        Scryer turns them proven when the report comes in.
      </Lead>
      <Caption at={{ x: col(0), y: 222 }} width={span(9)} style={settle(f, s(0.3))}>
        Nobody marks anything by hand. The agent runs the tests and hands Scryer the JUnit file; the status is computed from it.
      </Caption>
      <Figure at={{ x: col(0), y: 410 }} width={864} height={180} scale={2} label="Fig. 4 — Ledger" style={settle(f, s(0.4))}>
        <div className="shot-grade shot-claims h-full bg-[var(--surface)] px-4 pt-2.5 text-[var(--text)]">
          <div className="flex items-center justify-between pb-1.5 pl-1">
            <span className="text-[13px] font-semibold text-[var(--text)]">Ledger</span>
            <span style={{ display: "inline-flex", transform: `scale(${1.3 + 0.08 * popAt(f, flips[proven - 1])})`, transformOrigin: "right center" }}>
              <TestChip covered={BEFORE + proven} testable={BEFORE + 4} />
            </span>
          </div>
          <ClaimRows
            host={LEDGER}
            rows={FOUR.map((text, i) => ({
              claim: claim(text),
              state: i < proven ? "proven" : "planned",
              css: i < proven ? `transform: scale(${1 + 0.012 * popAt(f, flips[i])}); transform-origin: left center;` : undefined,
            }))}
          />
        </div>
      </Figure>
      <Terminal at={{ x: col(0), y: 830 }} width={span(12)} lines={lines} shown={shown} typed={shown === 1 ? typedChars(f, at[0], 80) : undefined} style={settle(f, s(0.5))} />
    </Page>
  );
}

/** A quiet pop: 0→1→0 over 0.3 s from `at`. */
function popAt(f: number, at: number | undefined): number {
  if (at === undefined || f < at) return 0;
  return Math.sin(Math.PI * Math.min(1, (f - at) / s(0.3)));
}
