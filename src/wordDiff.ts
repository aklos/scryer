/**
 * Word-level diff — the inline "what it was → what it is now" rendering for a
 * reworded claim, description, or link label. Splits both sides into words (and
 * the whitespace between them, so spacing round-trips), runs a classic LCS over
 * the word arrays, and emits a run of segments tagged equal / added / removed.
 *
 * Granularity is the word, not the character: a reworded sentence reads as a
 * few replaced phrases, not a storm of single-letter edits. The caller paints
 * `added` and `removed` segments (green / struck red) and leaves `equal` plain.
 */

export type WordSeg = { kind: "equal" | "added" | "removed"; text: string };

/** Split into whitespace runs, word runs (letters/digits/underscore), and
 *  punctuation runs — keeping punctuation off the word so editing text next to
 *  it doesn't change the word token, and keeping the whitespace so a reassembled
 *  "equal + added" run preserves the original spacing. */
function tokenize(s: string): string[] {
  return s.match(/\s+|[\p{L}\p{N}_]+|[^\s\p{L}\p{N}_]+/gu) ?? [];
}

/** Longest-common-subsequence table over two token arrays (Wagner–Fischer). */
function lcsTable(a: string[], b: string[]): number[][] {
  const m = a.length;
  const n = b.length;
  const dp: number[][] = Array.from({ length: m + 1 }, () => new Array(n + 1).fill(0));
  for (let i = m - 1; i >= 0; i--)
    for (let j = n - 1; j >= 0; j--)
      dp[i][j] = a[i] === b[j] ? dp[i + 1][j + 1] + 1 : Math.max(dp[i + 1][j], dp[i][j + 1]);
  return dp;
}

/** Diff `from` → `to` at word granularity. Returns segments in `to`-order with
 *  removed runs spliced in at the point they were dropped. Adjacent segments of
 *  the same kind are coalesced so the output is a handful of runs, not tokens. */
export function wordDiff(from: string, to: string): WordSeg[] {
  if (from === to) return to ? [{ kind: "equal", text: to }] : [];
  const a = tokenize(from);
  const b = tokenize(to);
  const dp = lcsTable(a, b);
  const out: WordSeg[] = [];
  const push = (kind: WordSeg["kind"], text: string) => {
    if (!text) return;
    const last = out[out.length - 1];
    if (last && last.kind === kind) last.text += text;
    else out.push({ kind, text });
  };
  let i = 0;
  let j = 0;
  while (i < a.length && j < b.length) {
    if (a[i] === b[j]) {
      push("equal", a[i]);
      i++;
      j++;
    } else if (dp[i + 1][j] >= dp[i][j + 1]) {
      push("removed", a[i++]);
    } else {
      push("added", b[j++]);
    }
  }
  while (i < a.length) push("removed", a[i++]);
  while (j < b.length) push("added", b[j++]);
  return out;
}
