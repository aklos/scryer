import { Fragment } from "react";
import { Bot } from "lucide-react";
import { wordDiff } from "../wordDiff";

const EMPTY_HINT =
  "Empty — no responsibilities or properties. Give it a business responsibility or remove the node.";

/** The one page content column every full-page surface shares: article (900) +
 *  gap (32) + rail (300) + horizontal padding (56), centered at wide widths so
 *  pages compose instead of hugging the tree and stranding a dead right region.
 *  Headers span their pane (border/surface are chrome) but wrap their content
 *  in this; special pages cap their inner prose narrower inside it. */
export const PAGE_COL = "mx-auto w-full max-w-[1288px] px-7";

// Field length caps, in characters. The description and technology caps mirror
// the backend validator (scryer-core `DESCRIPTION_MAX_CHARS` /
// `TECHNOLOGY_MAX_CHARS`); the name cap is a UI-only limit on human-authored
// titles (symbol names are exempt — they're bound to real code identifiers).
export const DESCRIPTION_MAX = 200;
export const TECHNOLOGY_MAX = 80;
export const NAME_MAX = 40;

/** Inline word-level diff: unchanged text plain, added words highlighted, removed
 *  words struck through. Used for reworded claims, descriptions, and link labels. */
export function WordDiffText({ from, to }: { from: string; to: string }) {
  const segs = wordDiff(from, to);
  return (
    <>
      {segs.map((s, i) => {
        // A substitution (removed word immediately followed by its replacement)
        // glues the two together — "forin" — because the tokenizer keeps the
        // surrounding spaces in the unchanged runs, leaving none between the
        // changed words. Reinsert that gap so the strike and the pill read apart.
        const sep = i > 0 && segs[i - 1].kind !== "equal" && s.kind !== "equal";
        return (
          <Fragment key={i}>
            {sep && " "}
            {s.kind === "equal" ? (
              <span>{s.text}</span>
            ) : s.kind === "added" ? (
              <span className="rounded-xs bg-emerald-500/15 px-px text-emerald-700 dark:text-emerald-300">
                {s.text}
              </span>
            ) : (
              <del className="text-red-700 decoration-red-400/60 dark:text-red-400/90">
                {s.text}
              </del>
            )}
          </Fragment>
        );
      })}
    </>
  );
}

/** Coerce text to a valid source identifier: drop anything that isn't
 *  `[A-Za-z0-9_]`, and a leading character that can't start one. Mirrors the
 *  backend's `is_valid_identifier` (which allows an uppercase start). */
export function sanitizeIdentifier(text: string): string {
  const cleaned = text.replace(/[^A-Za-z0-9_]/g, "");
  return cleaned.replace(/^[0-9]+/, "");
}

/** The `empty` flag (see rollup.isNodeEmpty) — a symbol with no semantic
 *  content. An attention state, not a lifecycle one, so it carries the drift
 *  orange instead of fading into the chrome. */
export function EmptyFlag({ className = "" }: { className?: string }) {
  return (
    <span
      title={EMPTY_HINT}
      className={`inline-flex shrink-0 items-center rounded-full border border-dashed border-orange-400/70 bg-orange-500/5 px-2 py-px text-2xs font-medium text-orange-700 dark:text-orange-300 ${className}`}
    >
      empty
    </span>
  );
}

/** The tree-row counterpart to {@link EmptyFlag}: a hollow ring where a status
 *  dot would sit. */
export function EmptyDot({ className = "" }: { className?: string }) {
  return (
    <span
      title={EMPTY_HINT}
      className={`inline-block h-1.5 w-1.5 shrink-0 rounded-full border border-orange-500 dark:border-orange-400 ${className}`}
    />
  );
}

/** The eyebrow — the one spec for every micro uppercase section label. Size,
 *  weight, and tracking are fixed here so the idiom can't drift per file.
 *  EYEBROW_BASE carries the type spec; EYEBROW adds the default (tertiary)
 *  ink. Dimmed variants pair the base with their own ink token. */
export const EYEBROW_BASE = "text-2xs font-semibold uppercase tracking-[0.07em]";
export const EYEBROW = `${EYEBROW_BASE} text-[var(--text-tertiary)]`;

/** One button system for the page (the mockup's `.btn`): bordered, sentence
 *  case, color = role. Set off the mono content in its own lane. */
const BTN_BASE =
  "pointer-events-auto inline-flex items-center gap-1 rounded border px-2.5 py-0.5 text-2xs transition-colors whitespace-nowrap";
export const BTN = `${BTN_BASE} border-[var(--border-strong)] bg-[var(--surface-hover)] text-[var(--text-secondary)] hover:bg-[var(--surface-active)] hover:text-[var(--text)]`;
export const BTN_GO = `${BTN_BASE} border-emerald-500/45 bg-emerald-500/10 text-emerald-700 hover:bg-emerald-500/20 dark:text-emerald-400`;
export const BTN_DANGER = `${BTN_BASE} border-red-500/45 bg-red-500/10 text-red-700 hover:bg-red-500/20 dark:text-red-400`;
/** Spawns an AI agent (a billable, possibly long-running fill). Violet is the
 *  agent signal throughout — the powerline's launch readout and activity pole —
 *  so every button that launches one carries it, warning the user before the
 *  click what kind of action this is. */
export const BTN_AGENT = `${BTN_BASE} border-violet-500/45 bg-violet-500/10 text-violet-600 hover:bg-violet-500/20 dark:text-violet-400`;

/** Chromeless icon button — window/panel dismissals, small toggles. Neutral
 *  ink that lifts on hover; no border so it sits quietly in header rows. */
export const BTN_ICON =
  "rounded p-0.5 text-[var(--text-tertiary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text)]";

/** Inline text link — the wikilink blue ({@link WikiLink}'s palette) for plain
 *  navigational text buttons that sit inside prose or data rows. */
export const LINK = "text-blue-700 hover:underline dark:text-blue-400";

/** The agent's mark — the Bot icon, violet per the color contract (violet =
 *  the agent). Em-sized so it scales with the surrounding type, and the
 *  default ink can be overridden via `className` (pass `""` to inherit, e.g.
 *  inside an already-violet button). */
export function AgentMark({
  className = "text-violet-500 dark:text-violet-400",
}: {
  className?: string;
}) {
  return (
    <Bot
      aria-hidden
      className={`inline-block h-[1.2em] w-[1.2em] shrink-0 select-none align-text-bottom ${className}`}
    />
  );
}

/** Per-row edit controls (the mockup's `.ctl`): floated over the row's right
 *  edge, top-aligned to the first line, with a gradient fade so they read over
 *  the text and take NO layout space — the field keeps its full read-mode width
 *  and nothing reflows on edit. The gradient is `pointer-events-none` so a click
 *  just past the text falls through to the field (caret lands at the end); the
 *  buttons re-enable pointer events themselves. Hidden until row hover; pair
 *  with a `relative` row and a `group/erow` ancestor. */
export const CTL =
  "pointer-events-none invisible absolute right-0 top-0 z-10 flex h-6 items-center gap-1.5 pl-9 [background-image:linear-gradient(90deg,transparent,var(--surface-tint)_28px)] group-hover/erow:visible";
