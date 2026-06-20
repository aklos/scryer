/**
 * Inline source — the page's read-through-to-code, attached to each claim it
 * discharges rather than pooled in a footnote section. A claim's diff row lists
 * its `↳ file:range` location(s); clicking one expands a code peek in place,
 * with the mapped range inked amber. The node's own definition file rides the
 * type line instead. (Backed by the `read_source_span` Tauri command, which
 * returns the enclosing span tokenized + a focus range to highlight.)
 */

import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Check, ChevronRight, ExternalLink } from "lucide-react";
import type { SourceLocation } from "./viewmodel";

/** Element id for a responsibility row — a jump target for banners and the
 *  Needs-review page. */
export const respElementId = (respId: string) => `resp-${respId}`;

// --- code span (from the backend) --------------------------------------------

interface Segment {
  text: string;
  kind: string;
}

interface SourceSpan {
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

/** A range label for a location: line(s) when explicit, else the symbol name. */
function locRangeLabel(loc: SourceLocation): string {
  if (loc.line != null)
    return `${loc.line}${loc.endLine && loc.endLine !== loc.line ? `–${loc.endLine}` : ""}`;
  return loc.symbol ?? "";
}

// --- inline per-claim source --------------------------------------------------

/**
 * A claim's source, rendered inline under its diff row: one `↳ file:range` link
 * per mapped location, each expanding to a code peek in place. A `deleted`
 * claim's source reads as struck (it goes away on the next implement); an
 * unanchored whole-file mapping has no peek.
 */
export function ClaimSource({
  locations,
  projectPath,
  deleted,
}: {
  locations: SourceLocation[];
  projectPath: string | null;
  deleted?: boolean;
}) {
  if (locations.length === 0) return null;
  return (
    <div className="mt-1 flex flex-col gap-0.5">
      {locations.map((loc, i) => (
        <SourceLine key={i} loc={loc} projectPath={projectPath} deleted={deleted} />
      ))}
    </div>
  );
}

function SourceLine({
  loc,
  projectPath,
  deleted,
}: {
  loc: SourceLocation;
  projectPath: string | null;
  deleted?: boolean;
}) {
  const anchored = Boolean(loc.symbol) || loc.line != null;
  const [open, setOpen] = useState(false);
  const range = locRangeLabel(loc);
  return (
    <div className="font-mono text-2xs leading-relaxed text-[var(--text-tertiary)]">
      <button
        type="button"
        disabled={!anchored}
        onClick={() => anchored && setOpen((o) => !o)}
        className={`group/src inline-flex items-baseline gap-1 text-left ${
          anchored ? "" : "cursor-default"
        }`}
        title={anchored ? "Peek at the source" : "Whole-file mapping — no line anchor"}
      >
        {anchored && (
          <ChevronRight
            className={`relative top-px h-3 w-3 shrink-0 text-[var(--text-ghost)] transition-transform ${
              open ? "rotate-90" : ""
            }`}
          />
        )}
        <span
          className={
            deleted
              ? "text-[var(--text-muted)] line-through decoration-red-400/50"
              : "text-blue-600 group-hover/src:underline dark:text-blue-400"
          }
        >
          ↳ {loc.pattern}
          {range ? `:${range}` : ""}
        </span>
        {anchored && !deleted && <Check className="relative top-px h-3 w-3 shrink-0 text-emerald-500" />}
      </button>
      {open && anchored && <InlinePeek loc={loc} projectPath={projectPath} />}
    </div>
  );
}

/** The code peek for one anchored location — fetched on first open, the mapped
 *  range inked amber, with an "open in editor" affordance. */
function InlinePeek({ loc, projectPath }: { loc: SourceLocation; projectPath: string | null }) {
  const [span, setSpan] = useState<SourceSpan | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  const focusRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!projectPath) {
      setErr("No project path.");
      return;
    }
    let cancelled = false;
    invoke<SourceSpan>("read_source_span", {
      projectPath,
      file: loc.pattern,
      symbol: loc.symbol ?? null,
      line: loc.line ?? null,
      endLine: loc.endLine ?? null,
    })
      .then((s) => !cancelled && setSpan(s))
      .catch((e) => !cancelled && setErr(String(e)));
    return () => {
      cancelled = true;
    };
  }, [projectPath, loc.pattern, loc.symbol, loc.line, loc.endLine]);

  // Open scrolled to the focus range — the enclosing symbol can be long.
  useEffect(() => {
    if (!span) return;
    const c = scrollRef.current;
    const f = focusRef.current;
    if (c && f) c.scrollTop = Math.max(0, f.offsetTop - 16);
  }, [span]);

  const range = locRangeLabel(loc);
  return (
    <div className="mb-1.5 ml-1 mt-1 overflow-hidden rounded-md border border-[var(--border-subtle)]">
      <div className="flex items-center gap-2 border-b border-[var(--border-subtle)] bg-[var(--surface)] px-2.5 py-1 font-mono text-2xs text-[var(--text-muted)]">
        <span className="min-w-0 truncate text-[var(--text-secondary)]">{loc.pattern}</span>
        {range && <span className="shrink-0">{range}</span>}
        <button
          type="button"
          onClick={() => void invoke("open_in_editor", { file: loc.pattern, line: loc.line ?? null, projectPath })}
          className="ml-auto inline-flex shrink-0 items-center gap-1 text-blue-600 hover:underline dark:text-blue-400"
        >
          open <ExternalLink className="h-3 w-3" />
        </button>
      </div>
      {err ? (
        <div className="px-2.5 py-1.5 font-mono text-2xs text-red-500/80 dark:text-red-400/80">{err}</div>
      ) : !span ? (
        <div className="px-2.5 py-1.5 font-mono text-2xs text-[var(--text-muted)]">loading…</div>
      ) : (
        <div
          ref={scrollRef}
          className="max-h-[240px] overflow-auto bg-[var(--surface-inset)] py-1 font-mono text-2xs leading-[1.6]"
        >
          {span.lines.map((segs, i) => {
            const lineNo = span.startLine + i;
            const focus = loc.line != null && lineNo >= span.focusStart && lineNo <= span.focusEnd;
            return (
              <div
                key={i}
                ref={lineNo === span.focusStart ? focusRef : undefined}
                className="flex"
                style={
                  focus
                    ? {
                        backgroundColor: "color-mix(in srgb, var(--color-amber-400) 12%, transparent)",
                        boxShadow: "inset 2px 0 0 0 color-mix(in srgb, var(--color-amber-500) 70%, transparent)",
                      }
                    : undefined
                }
              >
                <span className="w-10 shrink-0 select-none pr-2.5 text-right tabular-nums text-[var(--text-ghost)]">
                  {lineNo}
                </span>
                <span className="whitespace-pre pr-3 text-[var(--text)]">
                  {segs.length === 0
                    ? " "
                    : segs.map((s, j) => (
                        <span key={j} style={s.kind ? { color: TOKEN_COLOR[s.kind] } : undefined}>
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
