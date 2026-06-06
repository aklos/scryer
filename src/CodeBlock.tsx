/**
 * Mapped-source renderer. Given a source location (file + symbol/line anchor),
 * lazily fetches the span via `read_source_span` (tree-sitter highlighted) once
 * it scrolls near view and renders it with the focus range emphasized. The
 * backend returns the whole enclosing symbol (or file); we never truncate, so
 * the focus is auto-scrolled into view inside the fixed-height viewport. A
 * whole-file mapping (no symbol/line anchor) renders as an honest one-liner
 * rather than dumping the file head.
 *
 * Promoted out of the old inspector panel to a first-class page region: the
 * "read through to code" surface every node page leans on.
 */

import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { CircleSlash, ExternalLink } from "lucide-react";

interface Segment {
  text: string;
  kind: string;
}

export interface SourceSpan {
  file: string;
  startLine: number;
  focusStart: number;
  focusEnd: number;
  lines: Segment[][];
}

/** Coarse token class → themed colour. Empty kind inherits the line colour. */
const TOKEN_COLOR: Record<string, string> = {
  keyword: "var(--color-violet-400)",
  string: "var(--color-emerald-400)",
  comment: "var(--text-ghost)",
  number: "var(--color-orange-400)",
  constant: "var(--color-orange-300)",
  function: "var(--color-blue-400)",
  type: "var(--color-cyan-400)",
  property: "var(--color-blue-300)",
  tag: "var(--color-red-400)",
  operator: "var(--text-muted)",
  punct: "var(--text-muted)",
};

export function CodeBlock({
  projectPath,
  pattern,
  symbol,
  line,
  endLine,
}: {
  projectPath: string | null;
  pattern: string;
  symbol?: string;
  line?: number;
  endLine?: number;
}) {
  // A mapping with neither a symbol anchor nor an explicit line is whole-file:
  // there's no precise span to show, so don't dump the file head as "code".
  const anchored = Boolean(symbol) || line != null;

  const containerRef = useRef<HTMLDivElement>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  const focusRef = useRef<HTMLDivElement>(null);
  const [visible, setVisible] = useState(false);
  const [span, setSpan] = useState<SourceSpan | null>(null);
  const [err, setErr] = useState<string | null>(null);

  // Lazy: only fetch + parse once the block scrolls near view.
  useEffect(() => {
    if (!anchored) return;
    const el = containerRef.current;
    if (!el) return;
    const obs = new IntersectionObserver(
      (entries) => {
        if (entries.some((e) => e.isIntersecting)) {
          setVisible(true);
          obs.disconnect();
        }
      },
      { rootMargin: "300px" },
    );
    obs.observe(el);
    return () => obs.disconnect();
  }, [anchored]);

  useEffect(() => {
    if (!anchored || !visible) return;
    if (!projectPath) {
      setErr("No project path.");
      return;
    }
    let cancelled = false;
    setSpan(null);
    setErr(null);
    invoke<SourceSpan>("read_source_span", {
      projectPath,
      file: pattern,
      symbol: symbol ?? null,
      line: line ?? null,
      endLine: endLine ?? null,
    })
      .then((s) => !cancelled && setSpan(s))
      .catch((e) => !cancelled && setErr(String(e)));
    return () => {
      cancelled = true;
    };
  }, [anchored, visible, projectPath, pattern, symbol, line, endLine]);

  // Center the focus in the viewport once the span renders — the symbol body
  // can be long, so opening at the top would bury the focus.
  useEffect(() => {
    if (!span) return;
    const c = scrollRef.current;
    const f = focusRef.current;
    if (!c || !f) return;
    c.scrollTop = Math.max(0, f.offsetTop - c.clientHeight / 3);
  }, [span]);

  const openInEditor = () =>
    void invoke("open_in_editor", {
      file: pattern,
      line: span?.focusStart ?? line ?? null,
      projectPath,
    });

  // Whole-file mapping: honest one-liner, never an import dump.
  if (!anchored) {
    return (
      <button
        type="button"
        onClick={openInEditor}
        title="Open in editor"
        className="flex w-full items-center gap-2 rounded-md border border-dashed border-[var(--border)] px-3 py-2 text-left transition-colors hover:bg-[var(--surface-hover)] cursor-pointer"
      >
        <CircleSlash className="h-3.5 w-3.5 shrink-0 text-[var(--text-ghost)]" />
        <span className="min-w-0 flex-1">
          <span className="block font-mono text-[11px] text-[var(--text-secondary)]">
            {pattern}
          </span>
          <span className="text-[10px] text-[var(--text-ghost)]">
            whole-file mapping — no symbol anchor
          </span>
        </span>
        <ExternalLink className="h-3.5 w-3.5 shrink-0 text-[var(--text-ghost)]" />
      </button>
    );
  }

  return (
    <div
      ref={containerRef}
      className="overflow-hidden rounded-md border border-[var(--border)]"
    >
      <button
        type="button"
        onClick={openInEditor}
        title="Open in editor"
        className="flex w-full items-center gap-2 border-b border-[var(--border-subtle)] bg-[var(--surface-raised)] px-3 py-2 text-left transition-colors hover:bg-[var(--surface-hover)] cursor-pointer"
      >
        {symbol && (
          <span className="shrink-0 font-mono text-[11px] font-semibold text-[var(--text)]">
            {symbol}
          </span>
        )}
        <span className="min-w-0 flex-1 truncate font-mono text-[10px] text-[var(--text-muted)]">
          {pattern}
          {span && (
            <span className="text-[var(--text-ghost)]">
              :{span.focusStart}
              {span.focusEnd !== span.focusStart ? `–${span.focusEnd}` : ""}
            </span>
          )}
        </span>
        <ExternalLink className="h-3.5 w-3.5 shrink-0 text-[var(--text-ghost)]" />
      </button>

      {err ? (
        <div className="px-3 py-2 font-mono text-[10.5px] text-red-400/80">
          {err}
        </div>
      ) : !span ? (
        <div className="px-3 py-2 font-mono text-[10.5px] text-[var(--text-ghost)]">
          loading…
        </div>
      ) : (
        <div
          ref={scrollRef}
          className="relative max-h-[280px] overflow-auto bg-[var(--surface-inset)] py-1.5 font-mono text-[11px] leading-[1.6]"
        >
          {span.lines.map((segs, i) => {
            const lineNo = span.startLine + i;
            const focus = lineNo >= span.focusStart && lineNo <= span.focusEnd;
            return (
              <div
                key={i}
                ref={lineNo === span.focusStart ? focusRef : undefined}
                className="flex"
                style={
                  focus
                    ? {
                        backgroundColor:
                          "color-mix(in srgb, var(--color-blue-500) 11%, transparent)",
                        boxShadow:
                          "inset 2px 0 0 0 color-mix(in srgb, var(--color-blue-500) 70%, transparent)",
                      }
                    : { opacity: 0.5 }
                }
              >
                <span className="w-10 shrink-0 select-none pr-2 text-right tabular-nums text-[var(--text-ghost)]">
                  {lineNo}
                </span>
                <span className="whitespace-pre pr-4 text-[var(--text)]">
                  {segs.length === 0
                    ? " "
                    : segs.map((s, j) => (
                        <span
                          key={j}
                          style={s.kind ? { color: TOKEN_COLOR[s.kind] } : undefined}
                        >
                          {s.text}
                        </span>
                      ))}
                </span>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
