/**
 * The agent-CLI terminal — a film-only prop. The product has no terminal; this
 * stands in for whatever coding agent the user runs beside scryer, talking to
 * it over MCP. A pure render of TerminalState at a frame: the script types the
 * request into `input` and appends `lines`, and the caret blink and spinner
 * turn with the frame, so every frame renders the same alone.
 *
 * Deliberately unbranded so it reads as "your agent, whatever it is". The one
 * identity cue is the violet ● marker and spinner, the agent-activity violet
 * scryer uses, tying the two windows into one session.
 */

import { interpolate, useCurrentFrame } from "remotion";
import type { TermLine, TerminalState } from "./state";
import "./terminal.css";

/** A little ASCII mascot so the window reads as an AI agent at a glance. */
const ROBOT = ["   ╷", " ╭─┴─╮", " │▢ ▢│", " │ ▽ │", " ╰───╯"].join("\n");

function Spinner() {
  const frame = useCurrentFrame();
  return <span className="term-spin" style={{ transform: `rotate(${(frame * 17) % 360}deg)` }} />;
}

function ToolRow({ line }: { line: Extract<TermLine, { kind: "tool" }> }) {
  // Status leads on the left; the call head (tool + target) sits beside it, and
  // a long argument drops to its own line beneath the head.
  const argInline = line.arg && !line.target;
  return (
    <div className="term-tool">
      <span className="term-tool-status">
        {line.status === "ok" ? <span className="term-tool-ok">✓</span> : <Spinner />}
      </span>
      <div className="term-tool-call">
        <div className="term-tool-head">
          <span className="term-tool-name">{line.tool}</span>
          {line.target && <span className="term-tool-target">{line.target}</span>}
          {argInline && <span className="term-tool-arg">"{line.arg}"</span>}
        </div>
        {line.arg && line.target && <div className="term-tool-arg">"{line.arg}"</div>}
      </div>
    </div>
  );
}

/** A streamed code edit: a file header over a compact +/- diff. */
function DiffBlock({ line }: { line: Extract<TermLine, { kind: "diff" }> }) {
  return (
    <div className="term-diff">
      <div className="term-diff-file">{line.file}</div>
      <div className="term-diff-body">
        {line.rows.map((r, i) => (
          <div key={i} className={`term-diff-row term-diff-${r.op === "+" ? "add" : r.op === "-" ? "del" : "ctx"}`}>
            <span className="term-diff-sign">{r.op === " " ? " " : r.op}</span>
            <span className="term-diff-text">{r.text}</span>
          </div>
        ))}
      </div>
    </div>
  );
}

/** A test run: one entry per test, a dot until its result fills in. */
function TestRun({ line }: { line: Extract<TermLine, { kind: "test" }> }) {
  return (
    <div className="term-test">
      <div className="term-test-cmd">$ {line.command}</div>
      <div className="term-test-dots">
        {line.tests.map((t, i) => {
          const done = i < line.filled;
          const cls = done ? (t.result === "pass" ? "term-test-pass" : "term-test-fail") : "term-test-pending";
          return (
            <span className="term-test-case" key={t.name}>
              <span className={cls}>{done ? (t.result === "pass" ? "✓" : "✗") : "·"}</span>
              <span>{t.name}</span>
            </span>
          );
        })}
      </div>
    </div>
  );
}

function Line({ line }: { line: TermLine }) {
  switch (line.kind) {
    case "user":
      return (
        <div className="term-prompt term-sent">
          <span className="term-caret-prompt">❯</span>
          <span className="term-input">{line.text}</span>
        </div>
      );
    case "say":
      return (
        <div className="term-say">
          <span className="term-say-mark">●</span>
          <span>{line.text}</span>
        </div>
      );
    case "note":
      return <div className="term-note">{line.text}</div>;
    case "diff":
      return <DiffBlock line={line} />;
    case "test":
      return <TestRun line={line} />;
    case "tool":
      return <ToolRow line={line} />;
  }
}

export function Terminal({ state }: { state: TerminalState }) {
  const frame = useCurrentFrame();
  const { cwd, input, running, lines } = state;
  const caretOn = Math.floor(frame / 16) % 2 === 0;
  // A terminal that joins mid-take slides in from the right.
  const slide = interpolate(frame - state.since, [0, 14], [1, 0], { extrapolateLeft: "clamp", extrapolateRight: "clamp" });
  return (
    <div className="term" style={{ transform: `translateX(${slide * 110}%)`, opacity: 1 - slide }}>
      <div className="term-bar">
        <div className="term-dots">
          <span className="term-dot term-dot--r" />
          <span className="term-dot term-dot--y" />
          <span className="term-dot term-dot--g" />
        </div>
        <div className="term-tab">
          <span className="term-tab-glyph">❯_</span>
          <span className="term-tab-label">{cwd}</span>
        </div>
        <span className="term-shell-badge">zsh</span>
      </div>

      <div className="term-body">
        <div className="term-banner">
          <div className="term-banner-id">
            <pre className="term-logo">{ROBOT}</pre>
            <div className="term-banner-titles">
              <div className="term-banner-kicker">AI CODING AGENT</div>
              <div className="term-banner-name">GIPPITEE 3000</div>
            </div>
          </div>
          <div className="term-banner-hint">
            Your coding agent, with your tools connected — scryer among them. Describe what you want to build.
          </div>
        </div>
        {lines.map((line, i) => (
          <Line line={line} key={i} />
        ))}
      </div>

      <div className="term-inputbox">
        <span className="term-box-prompt">❯</span>
        <span className="term-input">
          {input}
          {!running && <span className="term-caret" style={{ opacity: caretOn ? 1 : 0 }} />}
        </span>
      </div>
    </div>
  );
}
