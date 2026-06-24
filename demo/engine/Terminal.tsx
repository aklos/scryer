/**
 * The agent-CLI terminal — a demo-only prop. The product has no terminal; this
 * stands in for the generic coding agent the user runs alongside scryer, which
 * edits the model through the scryer MCP server. It's a pure state→UI render:
 * the scene types the request into `input` and appends `lines` over time, so the
 * same director that drives the camera drives the terminal.
 *
 * It is deliberately neutral — no product branding — so it reads as "your agent,
 * whatever it is." The one identity cue is the violet ● marker + spinner, which
 * matches the agent-activity violet scryer uses on the powerline and the
 * generating barber, tying the two windows together as one session.
 */

import { useEffect, useRef } from "react";
import "./terminal.css";

/** A little ASCII mascot so the window reads as an AI agent at a glance. */
const ROBOT = ["   ╷", " ╭─┴─╮", " │▢ ▢│", " │ ▽ │", " ╰───╯"].join("\n");

/** One row of a streamed code edit: an added/removed/context line. */
export interface DiffRow {
  op: "+" | "-" | " ";
  text: string;
}

/** One streamed line of the conversation — a committed user prompt, the agent's
 *  prose/tool calls, or a code edit. Prompts live in the scrollback (not a single
 *  pinned bar) so the terminal reads as a multi-turn session. */
export type TermLine =
  | { kind: "user"; text: string }
  | { kind: "say"; text: string }
  | { kind: "tool"; tool: string; target?: string; arg?: string; status: "run" | "ok" }
  | { kind: "note"; text: string }
  | { kind: "diff"; file: string; rows: DiffRow[] };

export interface TerminalState {
  /** Shown in the window title bar, e.g. "~/aperture-pay". */
  cwd: string;
  /** The user's request — typed in char by char by the scene. */
  input: string;
  /** The agent is still working — the input caret rests and the trailing running
   *  tool shows the spinner. */
  running: boolean;
  /** The streamed output, appended to over the take. */
  lines: TermLine[];
}

function ToolRow({ line }: { line: Extract<TermLine, { kind: "tool" }> }) {
  // Status leads on the left; the call head (tool + target) sits beside it, and
  // a long argument drops to its own line beneath the head — so nothing has to
  // align into ragged columns and the check never floats off to the right edge.
  const argInline = line.arg && !line.target;
  return (
    <div className="term-tool">
      <span className="term-tool-status">
        {line.status === "ok" ? (
          <span className="term-tool-ok">✓</span>
        ) : (
          <span className="term-spin" />
        )}
      </span>
      <div className="term-tool-call">
        <div className="term-tool-head">
          <span className="term-tool-name">{line.tool}</span>
          {line.target && <span className="term-tool-target">{line.target}</span>}
          {argInline && <span className="term-tool-arg">"{line.arg}"</span>}
        </div>
        {line.arg && line.target && (
          <div className="term-tool-arg">"{line.arg}"</div>
        )}
      </div>
    </div>
  );
}

/** A streamed code edit — a file header over a compact +/- diff. The agent's
 *  implementation work shows as these blocks scrolling by. */
function DiffBlock({ line }: { line: Extract<TermLine, { kind: "diff" }> }) {
  return (
    <div className="term-diff">
      <div className="term-diff-file">{line.file}</div>
      <div className="term-diff-body">
        {line.rows.map((r, i) => (
          <div
            key={i}
            className={`term-diff-row term-diff-${r.op === "+" ? "add" : r.op === "-" ? "del" : "ctx"}`}
          >
            <span className="term-diff-sign">{r.op === " " ? " " : r.op}</span>
            <span className="term-diff-text">{r.text}</span>
          </div>
        ))}
      </div>
    </div>
  );
}

export function Terminal({ state }: { state: TerminalState }) {
  const { cwd, input, running, lines } = state;
  // Keep the newest line in view as the agent streams output — the body is
  // clipped (no scrollbar), so without this the stream would grow off the bottom.
  const bodyRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const el = bodyRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [lines]);
  return (
    <div className="term" data-cam="terminal">
      {/* A terminal tab-bar, not a centred window title — so it reads as its own
          app (a modern terminal), not a stripped copy of scryer's toolbar. */}
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

      {/* Scrollback: the agent-CLI banner, the committed request, then output. */}
      <div className="term-body" data-cam="term-scroll" ref={bodyRef}>
        <div className="term-banner">
          <div className="term-banner-id">
            <pre className="term-logo">{ROBOT}</pre>
            <div className="term-banner-titles">
              <div className="term-banner-kicker">AI CODING AGENT</div>
              <div className="term-banner-name">GIPPITEE 3000</div>
            </div>
          </div>
          <div className="term-banner-hint">
            Your coding agent, with your tools connected — the scryer model among them.
            Describe what you want to build.
          </div>
        </div>

        {lines.map((line, i) => {
          if (line.kind === "user") {
            return (
              <div className="term-prompt term-sent" data-cam="term-sent" key={i}>
                <span className="term-caret-prompt">❯</span>
                <span className="term-input">{line.text}</span>
              </div>
            );
          }
          if (line.kind === "say") {
            return (
              <div className="term-say" key={i}>
                <span className="term-say-mark">●</span>
                <span>{line.text}</span>
              </div>
            );
          }
          if (line.kind === "note") {
            return (
              <div className="term-note" key={i}>
                {line.text}
              </div>
            );
          }
          if (line.kind === "diff") {
            return <DiffBlock line={line} key={i} />;
          }
          return <ToolRow line={line} key={i} />;
        })}
      </div>

      {/* The permanent input box pinned at the bottom. The request types in here,
          then commits up into the scrollback on submit and the box clears. */}
      <div className="term-inputbox" data-cam="term-input">
        <span className="term-box-prompt">❯</span>
        <span className="term-input">
          {input}
          {!running && <span className="term-caret" />}
        </span>
      </div>

      {/* Camera anchor: the lower "active" zone — the prompt box and the freshest
          streamed output just above it. The implement beat frames this (closer
          than the whole window) so the eye sits where the action is. */}
      <div className="term-active" data-cam="term-active" aria-hidden />
    </div>
  );
}
