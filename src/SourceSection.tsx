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
import { Anchor, ChevronRight, ExternalLink, FlaskConical, Slash } from "lucide-react";
import type { SourceLocation } from "./viewmodel";
import type { AnchorState } from "./health";

/** Whether a source anchor currently lands in real code — mirrors the backend
 *  `verify_anchor` command. Anything but `resolved` reads as a broken anchor. */
type AnchorStatus = "resolved" | "fileMissing" | "symbolMissing" | "lineOutOfRange";

const ANCHOR_TITLE: Record<AnchorStatus, string> = {
  resolved: "Anchored to source",
  fileMissing: "File not found in the codebase",
  symbolMissing: "Symbol not found in the file",
  lineOutOfRange: "Line is past the end of the file",
};

/** Element id for a responsibility row — a jump target for banners and the
 *  Needs-review page. */
export const respElementId = (respId: string) => `resp-${respId}`;

/** Element id for a property row — a jump target for banners and the Needs-review
 *  page. Properties have no id, so keyed by owning node + label. */
export const propElementId = (nodeId: string, label: string) =>
  `prop-${nodeId}-${label.trim().toLowerCase()}`;

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

/** Coarse token class → themed colour, per mode: the bright 300/400 tiers that
 *  read on a dark canvas wash out on paper, so light mode drops to the 700s.
 *  Empty kind inherits the line colour. */
const TOKEN_CLASS: Record<string, string> = {
  keyword: "text-violet-700 dark:text-violet-400",
  string: "text-emerald-700 dark:text-emerald-400",
  comment: "text-[var(--text-ghost)]",
  number: "text-orange-700 dark:text-orange-400",
  constant: "text-orange-800 dark:text-orange-300",
  function: "text-blue-700 dark:text-blue-400",
  type: "text-cyan-700 dark:text-cyan-400",
  property: "text-blue-800 dark:text-blue-300",
  tag: "text-red-700 dark:text-red-400",
  operator: "text-[var(--text-muted)]",
  punct: "text-[var(--text-muted)]",
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
  bleed,
}: {
  locations: SourceLocation[];
  projectPath: string | null;
  deleted?: boolean;
  /** Negative-margin classes that undo the host row's gutters/padding, so the
   *  peek panel spans the article column edge-to-edge. Host-specific because
   *  each row anatomy indents differently; omit to keep the peek in place. */
  bleed?: string;
}) {
  if (locations.length === 0) return null;
  return (
    <div className="mt-1 flex flex-col gap-0.5">
      {locations.map((loc, i) => (
        <SourceLine key={i} loc={loc} projectPath={projectPath} deleted={deleted} bleed={bleed} />
      ))}
    </div>
  );
}

function SourceLine({
  loc,
  projectPath,
  deleted,
  bleed,
}: {
  loc: SourceLocation;
  projectPath: string | null;
  deleted?: boolean;
  bleed?: string;
}) {
  const anchored = Boolean(loc.symbol) || loc.line != null;
  const [open, setOpen] = useState(false);
  const [status, setStatus] = useState<AnchorStatus | null>(null);
  const range = locRangeLabel(loc);
  // Once verified, a non-resolving anchor reads as broken: the link drops its
  // live-target styling to match the struck-through anchor. (null = still
  // checking, so keep the optimistic link look until we know.)
  const broken = status != null && status !== "resolved";

  // Verify the anchor against real code so the icon reflects whether it lands.
  // Deleted claims show no anchor; without a project path (e.g. demo fixtures)
  // there's nothing to check against, so we optimistically treat it as resolved.
  useEffect(() => {
    if (!anchored || deleted) return;
    if (!projectPath) {
      setStatus("resolved");
      return;
    }
    let cancelled = false;
    invoke<AnchorStatus>("verify_anchor", {
      projectPath,
      file: loc.pattern,
      symbol: loc.symbol ?? null,
      line: loc.line ?? null,
    })
      .then((s) => !cancelled && setStatus(s))
      .catch(() => !cancelled && setStatus(null));
    return () => {
      cancelled = true;
    };
  }, [anchored, deleted, projectPath, loc.pattern, loc.symbol, loc.line]);
  return (
    <div className="font-mono text-2xs leading-relaxed text-[var(--text-muted)]">
      <button
        type="button"
        data-cam="resp-source"
        disabled={!anchored}
        onClick={() => anchored && setOpen((o) => !o)}
        className={`group/src inline-flex items-baseline gap-1 text-left ${
          anchored ? "" : "cursor-default"
        }`}
        title={anchored ? "Peek at the source" : "Whole-file mapping — no line anchor"}
      >
        {/* The chevron is the line's only lead glyph (peek affordance); a
            second corner-arrow said nothing it doesn't. Unanchored lines keep
            a spacer so all paths align. */}
        {anchored ? (
          <ChevronRight
            className={`relative top-px h-3 w-3 shrink-0 text-[var(--text-ghost)] transition-transform ${
              open ? "rotate-90" : ""
            }`}
          />
        ) : (
          <span className="h-3 w-3 shrink-0" />
        )}
        {/* The anchor LEADS the line, mirroring the test lines' leading flask —
            the dimension marker is the first thing read on either line kind. */}
        <AnchorMark status={deleted ? null : status} />
        <span
          className={
            deleted
              ? "text-[var(--text-muted)] line-through decoration-red-400/50"
              : broken
                ? "text-[var(--text-muted)] decoration-[var(--text-ghost)] decoration-dotted underline"
                : anchored
                  ? // Quiet at rest — the claim above is the headline, the path a
                    // footnote. The link look (blue + underline) surfaces on hover.
                    "text-[var(--text-muted)] group-hover/src:text-blue-600 group-hover/src:underline dark:group-hover/src:text-blue-400"
                  : // A whole-file mapping opens nothing — no hover link look.
                    "text-[var(--text-muted)]"
          }
        >
          {loc.pattern}
          {range ? `:${range}` : ""}
        </span>
      </button>
      {open && anchored && <InlinePeek loc={loc} projectPath={projectPath} bleed={bleed} />}
    </div>
  );
}

// --- inline per-claim attached tests ------------------------------------------

/**
 * A claim's attached tests, with the same first-class anatomy as its source
 * anchors: one `↳ file · "test name"` line per attached test, each expanding
 * to a code peek in place and verified live against the codebase. The flask
 * mark carries the test dimension; regression states tint the line (orange =
 * the test's content changed since the last reconcile, red = the test no
 * longer resolves).
 */
export function ClaimTests({
  locations,
  state,
  projectPath,
  deleted,
  bleed,
}: {
  locations: SourceLocation[];
  /** Fingerprint verdict from the last hook pass (`test:` observations). */
  state: AnchorState | null;
  projectPath: string | null;
  deleted?: boolean;
  bleed?: string;
}) {
  if (locations.length === 0) return null;
  // One line per test FILE: several tests of one claim usually live in the
  // same spec file, and identical repeated tokens differentiate nothing. A
  // merged line carries a ×N count and its peek opens every test stacked.
  const byFile = new Map<string, SourceLocation[]>();
  for (const loc of locations) {
    const group = byFile.get(loc.pattern) ?? [];
    group.push(loc);
    byFile.set(loc.pattern, group);
  }
  return (
    // Same level as the source anchors: a test exercises the CLAIM, not any
    // one of its implementation sites, so the two dimensions are siblings —
    // the leading flask (vs the anchors' trailing ⚓) tells them apart.
    <div className="mt-0.5 flex flex-col gap-0.5">
      {[...byFile.values()].map((locs, i) => (
        <TestLine key={i} locs={locs} state={state} projectPath={projectPath} deleted={deleted} bleed={bleed} />
      ))}
    </div>
  );
}

function TestLine({
  locs,
  state,
  projectPath,
  deleted,
  bleed,
}: {
  /** The claim's attached tests in ONE file (grouped by the caller). */
  locs: SourceLocation[];
  state: AnchorState | null;
  projectPath: string | null;
  deleted?: boolean;
  bleed?: string;
}) {
  const anchoredLocs = locs.filter((l) => Boolean(l.symbol) || l.line != null);
  const anchored = anchoredLocs.length > 0;
  const file = locs[0].pattern;
  const [open, setOpen] = useState(false);
  // The one disclosed test whose peek is open — accordion, not stack: the
  // peeks share one recess, so opening a test closes the previous one.
  const [openCase, setOpenCase] = useState<number | null>(null);
  const [status, setStatus] = useState<AnchorStatus | null>(null);

  // Live check, same as a source anchor: the backend resolves a test's name
  // string through the same lookup as a code identifier. A merged line is as
  // healthy as its weakest member — one unresolvable test marks the group.
  const anchorKey = JSON.stringify(anchoredLocs.map((l) => [l.symbol, l.line]));
  useEffect(() => {
    if (!anchored || deleted) return;
    if (!projectPath) {
      setStatus("resolved");
      return;
    }
    let cancelled = false;
    Promise.all(
      anchoredLocs.map((l) =>
        invoke<AnchorStatus>("verify_anchor", {
          projectPath,
          file: l.pattern,
          symbol: l.symbol ?? null,
          line: l.line ?? null,
        }),
      ),
    )
      .then(
        (all) => !cancelled && setStatus(all.find((s) => s !== "resolved") ?? "resolved"),
      )
      .catch(() => !cancelled && setStatus(null));
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- anchorKey covers anchoredLocs
  }, [anchored, deleted, projectPath, file, anchorKey]);

  // Two inputs, one verdict: the live resolve (does it land right now) and
  // the fingerprint observation (did it change since the reconcile). Gone
  // outranks changed — an unresolvable test can't be "just edited".
  const gone =
    (status != null && status !== "resolved") || state === "broken" || state === "fileMissing";
  const changed = !gone && state === "changed";
  // The visible token is MINIMAL — one line per file, no test names. The
  // claim above already says what the tests exercise; names and run command
  // live in the tooltip, and the peek shows the tests themselves.
  const detail = [
    ...locs.map((l) => (l.symbol ? `“${l.symbol}”` : null)),
    file,
    locs[0].command ? `run: ${locs[0].command}` : null,
  ]
    .filter(Boolean)
    .join("\n");
  const noun = locs.length > 1 ? `${locs.length} attached tests` : "Attached test";
  const title = deleted
    ? `${noun} of a claim removed in the plan\n${detail}`
    : gone
      ? `An attached test is GONE — it no longer resolves. Attach a live test, or clear the entry.\n${detail}`
      : changed
        ? `An attached test CHANGED since the last reconcile — check it still exercises this claim.\n${detail}`
        : `${noun} — click to peek\n${detail}`;

  return (
    <div className="font-mono text-2xs leading-relaxed text-[var(--text-muted)]">
      <button
        type="button"
        data-cam="resp-test"
        disabled={!anchored}
        onClick={() => {
          if (!anchored) return;
          // Reopening starts all-collapsed — no memory of the last open test.
          setOpenCase(null);
          setOpen((o) => !o);
        }}
        className={`group/src inline-flex max-w-full items-baseline gap-1 text-left ${
          anchored ? "" : "cursor-default"
        }`}
        title={title}
      >
        {anchored ? (
          <ChevronRight
            className={`relative top-px h-3 w-3 shrink-0 text-[var(--text-ghost)] transition-transform ${
              open ? "rotate-90" : ""
            }`}
          />
        ) : (
          <span className="h-3 w-3 shrink-0" />
        )}
        {/* The flask LEADS the line — the dimension is the first thing read,
            so test lines separate from anchor lines at a glance while sitting
            at the same level (both belong to the claim). No test-name suffix:
            names restate the claim the line sits under, so the claim IS the
            identifier (names live in the tooltip). */}
        <span
          className={`relative top-px inline-block h-3 w-3 shrink-0 ${
            deleted
              ? "text-[var(--text-muted)]"
              : gone
                ? "text-red-600 dark:text-red-400"
                : changed
                  ? "text-orange-600 dark:text-orange-400"
                  : "text-[var(--text-muted)]"
          }`}
        >
          <FlaskConical className="h-3 w-3" />
          {gone && !deleted && <Slash className="absolute inset-0 h-3 w-3" />}
        </span>
        <span
          className={
            deleted
              ? "text-[var(--text-muted)] line-through decoration-red-400/50"
              : gone
                ? "text-red-700 decoration-red-400/50 decoration-dotted underline dark:text-red-400"
                : changed
                  ? "text-orange-700 group-hover/src:underline dark:text-orange-400"
                  : anchored
                    ? // Same rest/hover treatment as the source anchors: grey
                      // footnote at rest, link look on hover.
                      "text-[var(--text-muted)] group-hover/src:text-blue-600 group-hover/src:underline dark:group-hover/src:text-blue-400"
                    : "text-[var(--text-muted)]"
          }
        >
          {file}
        </span>
        {locs.length > 1 && <span className="text-[var(--text-muted)]">×{locs.length}</span>}
      </button>
      {/* A lone test opens its peek directly. A merged ×N line discloses one
          indented line PER TEST — full name on its own row, the same
          chevron-line anatomy as everything else here — and each line toggles
          its own peek. No tabs: pills are foreign to this page's language,
          and truncating long it() names to fit them said nothing. */}
      {open &&
        (anchoredLocs.length === 1 ? (
          <InlinePeek loc={anchoredLocs[0]} projectPath={projectPath} bleed={bleed} />
        ) : (
          anchoredLocs.map((l, i) => (
            <TestCaseLine
              key={i}
              loc={l}
              projectPath={projectPath}
              bleed={bleed}
              open={openCase === i}
              onToggle={() => setOpenCase((o) => (o === i ? null : i))}
            />
          ))
        ))}
    </div>
  );
}

/** One test of a disclosed ×N line: an indented `› “name”` row toggling its
 *  own peek — the file line above already names where they all live. */
function TestCaseLine({
  loc,
  projectPath,
  bleed,
  open,
  onToggle,
}: {
  loc: SourceLocation;
  projectPath: string | null;
  bleed?: string;
  open: boolean;
  onToggle: () => void;
}) {
  const name = loc.symbol ? `“${loc.symbol}”` : `line ${loc.line}`;
  return (
    <>
      <div className="pl-4">
        <button
          type="button"
          onClick={onToggle}
          className="group/src inline-flex max-w-full items-baseline gap-1 text-left"
          title={loc.symbol ?? "Peek at the test"}
        >
          <ChevronRight
            className={`relative top-px h-3 w-3 shrink-0 text-[var(--text-ghost)] transition-transform ${
              open ? "rotate-90" : ""
            }`}
          />
          <span className="min-w-0 truncate text-[var(--text-muted)] group-hover/src:text-blue-600 group-hover/src:underline dark:group-hover/src:text-blue-400">
            {name}
          </span>
        </button>
      </div>
      {open && <InlinePeek loc={loc} projectPath={projectPath} bleed={bleed} />}
    </>
  );
}

/** The anchor mark leading a source ref — the dimension marker (code anchor,
 *  vs the test lines' flask). A plain anchor means the ref lands in real code;
 *  a struck-through anchor means the symbol or file it points at isn't there.
 *  The slash carries the meaning — both states stay monochrome. `null` (still
 *  verifying, or a deleted claim's ref) reads as the plain anchor. */
function AnchorMark({ status }: { status: AnchorStatus | null }) {
  const broken = status != null && status !== "resolved";
  return (
    <span
      className="relative top-px inline-block h-3 w-3 shrink-0 text-[var(--text-muted)]"
      title={status ? ANCHOR_TITLE[status] : undefined}
    >
      <Anchor className="h-3 w-3" />
      {broken && <Slash className="absolute inset-0 h-3 w-3" />}
    </span>
  );
}

/** The recess panel's side treatment: bg, borders and cast shadow all dissolve
 *  over the last ~28px instead of stopping at a hard edge. */
const SIDE_FADE =
  "linear-gradient(90deg, transparent, #000 28px, #000 calc(100% - 28px), transparent)";

/** The code peek for one anchored location — not a nested card but a recessed
 *  panel the page splits open to reveal: it bleeds out of the row indent to the
 *  article column's edges (the host-supplied `bleed` margins), sits on the
 *  canvas layer a step BEHIND the page, and the page edges above and below cast
 *  inset shadows onto it. Fetched on first open, the mapped range inked amber,
 *  with an "open in editor" affordance. */
function InlinePeek({
  loc,
  projectPath,
  bleed = "",
}: {
  loc: SourceLocation;
  projectPath: string | null;
  bleed?: string;
}) {
  const [span, setSpan] = useState<SourceSpan | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  const focusRef = useRef<HTMLDivElement>(null);
  // Mounted-at-0fr, then flipped to 1fr on the next frame — the split-open
  // slide is a grid-rows transition, so the page visibly parts.
  const [shown, setShown] = useState(false);
  useEffect(() => {
    const id = requestAnimationFrame(() => setShown(true));
    return () => cancelAnimationFrame(id);
  }, []);

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

  return (
    // The 0fr→1fr grid is the reveal: its child clips (overflow-hidden also
    // zeroes the grid item's automatic minimum), so the panel slides open.
    <div
      data-cam="source-peek"
      className={`grid transition-[grid-template-rows] duration-200 ease-out ${bleed}`}
      style={{ gridTemplateRows: shown ? "1fr" : "0fr" }}
    >
      <div className="overflow-hidden">
        {/* No header bar: the source line above the peek already names
            file:range — repeating it here said nothing. The one affordance the
            peek adds (open in editor) floats over the top-right corner instead
            — OUTSIDE the masked panel, so the side fade doesn't wash it out.
            Edge-to-edge across the article column: the page parts and the code
            sits on the recess layer behind it — shadow cast from the page
            edges above and below, and the sides dissolving via a mask rather
            than terminating in a border (a hard edge mid-surface reads as
            clipped, not revealed). */}
        <div className="relative my-1.5">
          <button
            type="button"
            onClick={() =>
              void invoke("open_in_editor", {
                file: loc.pattern,
                line: loc.line ?? null,
                symbol: loc.symbol ?? null,
                projectPath,
              })
            }
            className="absolute right-2 top-1.5 z-10 inline-flex shrink-0 items-center gap-1 rounded bg-[var(--surface-overlay)] px-1.5 py-0.5 font-mono text-2xs text-blue-600 backdrop-blur-sm hover:underline dark:text-blue-400"
          >
            open <ExternalLink className="h-3 w-3" />
          </button>
          <div
            className="border-y border-[var(--border-subtle)] bg-[var(--surface-recess)]"
            style={{
              boxShadow:
                "inset 0 12px 14px -12px var(--shadow-recess), inset 0 -12px 14px -12px var(--shadow-recess)",
              WebkitMaskImage: SIDE_FADE,
              maskImage: SIDE_FADE,
            }}
          >
          {err ? (
            <div className="px-4 py-2.5 font-mono text-xs text-red-500/80 dark:text-red-400/80">{err}</div>
          ) : !span ? (
            <div className="px-4 py-2.5 font-mono text-xs text-[var(--text-muted)]">loading…</div>
          ) : (
            <div
              ref={scrollRef}
              className="max-h-[420px] overflow-auto py-2 font-mono text-xs leading-[1.6]"
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
                    <span className="w-14 shrink-0 select-none pr-3 text-right tabular-nums text-[var(--text-ghost)]">
                      {lineNo}
                    </span>
                    <span className="whitespace-pre pr-4 text-[var(--text)]">
                      {segs.length === 0
                        ? " "
                        : segs.map((s, j) => (
                            <span key={j} className={s.kind ? TOKEN_CLASS[s.kind] : undefined}>
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
        </div>
      </div>
    </div>
  );
}
