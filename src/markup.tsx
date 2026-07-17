/**
 * Statement markup — the display structure of a responsibility claim.
 *
 * A statement may carry a markdown-lite subset: `**bold**` marks the scan
 * anchors — the EARS keyword (including "then") and the response verbs. The
 * markers live inside the canonical statement string — there is no schema
 * field — so everything here is presentation: parse for display, strip for
 * comparison, search, and word-diffing. Malformed or unmatched markers are
 * never an error; they render literally. `_underscore_` spans are a retired
 * clause marker: still parsed and stripped (statements minted under the old
 * convention must not show literal underscores), but rendered as plain text —
 * the bold keyword and its comma already delimit the clause.
 *
 * Two producers write markers: agents author them directly (rule 21), and the
 * editor mints them for hand-typed marker-free statements via the positional
 * EARS pass ({@link earsTokenize} / {@link serializeEars}) — a tokenizer over
 * the grammar's closed keyword set and fixed clause order, never a linguistic
 * guess. {@link MarkupMirror} renders the live char-exact preview behind the
 * editor's transparent text, so what the author sees is what commits.
 */
import type { ReactNode } from "react";

/** `**…**` opens/closes on any non-asterisk run. `_…_` only opens after
 *  start/whitespace/( and only closes before end/whitespace/punctuation, so
 *  snake_case identifiers inside a statement never trigger a marker. */
const MARKER =
  /\*\*([^*]+?)\*\*|(?<=^|[\s(])_([^_\s](?:[^_]*?[^_\s])?)_(?=[\s,.;:!?)]|$)/g;

export interface MarkupSeg {
  text: string;
  /** `**…**` — a scan anchor (keyword / response verb). */
  bold?: boolean;
}

export function parseMarkup(statement: string): MarkupSeg[] {
  const segs: MarkupSeg[] = [];
  let last = 0;
  for (const m of statement.matchAll(MARKER)) {
    if (m.index > last) segs.push({ text: statement.slice(last, m.index) });
    if (m[1] !== undefined) segs.push({ text: m[1], bold: true });
    else segs.push({ text: m[2] });
    last = m.index + m[0].length;
  }
  if (last < statement.length) segs.push({ text: statement.slice(last) });
  return segs;
}

/** The statement as plain text — what comparisons, search, word-diffs, and
 *  labels operate on, so a markup-only touch never reads as a reword. */
export function stripMarkup(statement: string): string {
  return statement.replace(MARKER, (_, bold, dim) => bold ?? dim);
}

/** Whether the statement carries any well-formed marker. Gates the editor's
 *  commit-time minting: authored markup is truth, only marker-free text goes
 *  through the positional pass. */
export function hasMarkup(statement: string): boolean {
  return statement.match(MARKER) !== null;
}

/** EARS keyword opening a condition clause, only ever matched at a clause
 *  start. "Whenever…" fails the lookahead and falls through to ubiquitous. */
const CLAUSE_KW = /^(While|When|If|Where)(?=\s)/i;

/**
 * The positional EARS pass: style a MARKER-FREE statement from the grammar's
 * fixed shape alone. Leading keyword → bold; `then` after an If clause →
 * bold; the first word of the response → bold; everything else plain. Clauses
 * stack only as the grammar allows (`While …, when/if …,`). A clause still
 * missing its comma has no response yet — the keyword is the only anchor.
 * Response verbs beyond the first are unknowable positionally; they stay
 * plain (authored markup is the only way to bold them, per rule 21).
 */
export function earsTokenize(plain: string): MarkupSeg[] {
  const segs: MarkupSeg[] = [];
  const push = (text: string, bold?: boolean) => {
    if (text) segs.push(bold ? { text, bold: true } : { text });
  };
  let pos = 0;
  let sawIf = false;
  let lastKw = "";
  for (let clause = 0; clause < 2; clause++) {
    const m = plain.slice(pos).match(CLAUSE_KW);
    if (!m) break;
    const kw = m[1].toLowerCase();
    if (clause === 1 && !(lastKw === "while" && (kw === "when" || kw === "if"))) break;
    push(m[1], true);
    pos += m[1].length;
    if (kw === "if") sawIf = true;
    const comma = plain.indexOf(",", pos);
    if (comma === -1) {
      // Clause in progress (no comma yet) — no response to anchor.
      push(plain.slice(pos));
      return segs;
    }
    let after = comma + 1;
    while (plain[after] === " ") after++;
    push(plain.slice(pos, after));
    pos = after;
    lastKw = kw;
  }
  if (sawIf) {
    const then = plain.slice(pos).match(/^then(?=\s)/i);
    if (then) {
      push(then[0], true);
      pos += then[0].length;
      const ws = plain.slice(pos).match(/^\s+/);
      if (ws) {
        push(ws[0]);
        pos += ws[0].length;
      }
    }
  }
  const verb = plain.slice(pos).match(/^[A-Za-z][A-Za-z'-]*/);
  if (verb) {
    push(verb[0], true);
    pos += verb[0].length;
  }
  push(plain.slice(pos));
  return segs;
}

/** Mint markers from the positional pass — the editor's commit step for a
 *  hand-typed, marker-free statement. What {@link MarkupMirror} previewed is
 *  exactly what this writes. */
export function serializeEars(plain: string): string {
  return earsTokenize(plain)
    .map((s) => (s.bold ? `**${s.text}**` : s.text))
    .join("");
}

/**
 * The editor's live preview layer: the raw field text rendered CHAR-EXACT
 * (every marker glyph kept, ghosted) so it can sit on top of the transparent
 * `contentEditable` without disturbing caret or wrap positions — safe because
 * statements render in a monospace face, where bold keeps the advance width.
 * Marked text shows its authored markup; marker-free text shows the positional
 * EARS preview that commit will mint.
 */
export function MarkupMirror({ text }: { text: string }) {
  if (!text) return null;
  const nodes: ReactNode[] = [];
  let key = 0;
  if (hasMarkup(text)) {
    let last = 0;
    for (const m of text.matchAll(MARKER)) {
      if (m.index > last) nodes.push(<span key={key++}>{text.slice(last, m.index)}</span>);
      const bold = m[1] !== undefined;
      const mark = bold ? "**" : "_";
      nodes.push(
        <span key={key++} className="text-[var(--text-ghost)]">{mark}</span>,
        <span key={key++} className={bold ? "font-bold" : ""}>
          {bold ? m[1] : m[2]}
        </span>,
        <span key={key++} className="text-[var(--text-ghost)]">{mark}</span>,
      );
      last = m.index + m[0].length;
    }
    if (last < text.length) nodes.push(<span key={key++}>{text.slice(last)}</span>);
  } else {
    for (const s of earsTokenize(text))
      nodes.push(
        <span key={key++} className={s.bold ? "font-bold" : ""}>
          {s.text}
        </span>,
      );
  }
  return <>{nodes}</>;
}

/** The calm rows' anchor lift above the secondary body. Light mode reaches
 *  `--text` (a real jump from secondary); dark mode's `--text` sits nearly ON
 *  secondary, so anchors take the same two-shade lift the diff tints get
 *  (`DIFF_ANCHOR`, hue-300 → hue-100) in the theme's own slate hue. */
export const ANCHOR_CALM = "text-[var(--text)] dark:text-slate-100";

/**
 * A claim statement with its markers rendered. `anchor` carries the bold
 * segments' color — a tone one step up from the row's body, so the anchors
 * pop within the row's own hue: {@link ANCHOR_CALM} on calm secondary rows,
 * `DIFF_ANCHOR.add`/`.delete` inside diff tints. Rows that already shout
 * (vagrant bright, relocated muted) omit it and take weight only, so the
 * markup never fights the row's own signal.
 */
export function StatementText({ text, anchor }: { text: string; anchor?: string }) {
  return (
    <>
      {parseMarkup(text).map((s, i) =>
        s.bold ? (
          <span key={i} className={`font-bold ${anchor ?? ""}`}>
            {s.text}
          </span>
        ) : (
          <span key={i}>{s.text}</span>
        ),
      )}
    </>
  );
}
