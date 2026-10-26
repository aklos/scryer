/**
 * The box tiers drawn the way a C4 diagram is drawn: who uses the level on
 * top, the level itself inside its boundary, what it depends on below.
 *
 *   above    people, and ghosts and externals that call into the level
 *   inside   the level's own nodes, layered by dependency: a node sits
 *            above the nodes it uses (cycles broken), in whichever of the
 *            rows its links leave it free to take draws best
 *   tooling  the level's tooling containers, a band at the foot of the
 *            boundary: part of the codebase, not of the product
 *   below    ghosts the level calls out to
 *   beside   external systems the level calls, in a column right of the
 *            boundary
 *
 * Above, inside, tooling and below are one stack of rows. A link that spans
 * rows runs down a lane between the cards of every row it passes, and the
 * rows are ordered by an exact search for the fewest crossings: none, when
 * the level can be drawn without any. The search has a step budget; a level
 * too big to finish in it keeps the best order found. Each card is then
 * nudged under what it links to, and the beside column takes the order that
 * crosses least.
 *
 * Positions are card top-lefts. The boundary is the rectangle around the
 * inside rows and the tooling band; the caller labels and draws it.
 */

export interface C4Edge {
  source: string;
  target: string;
}

export interface C4Input {
  inside: string[];
  tooling: string[];
  above: string[];
  beside: string[];
  below: string[];
  edges: C4Edge[];
  cardW: number;
  cardH: number;
}

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

type Pt = { x: number; y: number };

export interface C4Layout {
  positions: Map<string, Pt>;
  /** Around the inside rows and the tooling band. */
  boundary: Rect | null;
  /** Around the tooling band alone. */
  toolingBand: Rect | null;
  /** Per link that spans rows (keyed `source\0target`): the points it passes
   *  on the way, two per row it crosses — the top and bottom of that row's
   *  lane — so it is drawn between the cards. */
  routes: Map<string, Pt[]>;
}

/** Room between cards in a row, and between rows (edge labels live there). */
const GAP_X = 80;
const GAP_Y = 90;
/** Padding inside the boundary, and the strip on top for its label. */
const PAD = 40;
const LABEL_ROOM = 24;
/** Widest a dependency row grows before it wraps. */
const MAX_ROW = 5;
/** Room between a lane and its neighbour in a row. */
export const LANE_GAP = 20;
/** Most row assignments tried, and the search steps shared between them. */
const MAX_LAYERINGS = 24;
const SEARCH_BUDGET = 400_000;
/** Longest beside column whose every order is tried. */
const MAX_BESIDE_PERMUTED = 7;

export const isLane = (id: string) => id.startsWith("\0lane");

/** The inside links as a DAG (the edges a depth-first walk finds pointing
 *  back are dropped), with each node's highest and lowest possible row. */
function rankBounds(ids: string[], edges: C4Edge[]) {
  const idSet = new Set(ids);
  const out = new Map<string, string[]>(ids.map((id) => [id, []]));
  for (const e of edges) {
    if (idSet.has(e.source) && idSet.has(e.target) && e.source !== e.target) out.get(e.source)!.push(e.target);
  }
  const state = new Map<string, 0 | 1 | 2>();
  const dag = new Map<string, string[]>(ids.map((id) => [id, []]));
  const visit = (u: string) => {
    state.set(u, 1);
    for (const v of out.get(u)!) {
      const s = state.get(v) ?? 0;
      if (s === 1) continue;
      if (!dag.get(u)!.includes(v)) dag.get(u)!.push(v);
      if (s === 0) visit(v);
    }
    state.set(u, 2);
  };
  for (const id of ids) if (!state.get(id)) visit(id);

  // Topological order, then the longest path from a source (the highest row)
  // and to a sink (counted back from the bottom: the lowest).
  const indeg = new Map<string, number>(ids.map((id) => [id, 0]));
  for (const vs of dag.values()) for (const v of vs) indeg.set(v, indeg.get(v)! + 1);
  const topo: string[] = [];
  const queue = ids.filter((id) => indeg.get(id) === 0);
  while (queue.length) {
    const u = queue.shift()!;
    topo.push(u);
    for (const v of dag.get(u)!) {
      indeg.set(v, indeg.get(v)! - 1);
      if (indeg.get(v) === 0) queue.push(v);
    }
  }
  const asap = new Map<string, number>(ids.map((id) => [id, 0]));
  for (const u of topo) for (const v of dag.get(u)!) asap.set(v, Math.max(asap.get(v)!, asap.get(u)! + 1));
  const depth = Math.max(0, ...asap.values());
  const height = new Map<string, number>(ids.map((id) => [id, 0]));
  for (const u of [...topo].reverse()) for (const v of dag.get(u)!) height.set(u, Math.max(height.get(u)!, height.get(v)! + 1));
  const alap = new Map(ids.map((id) => [id, depth - height.get(id)!]));
  return { dag, topo, asap, alap };
}

/** Row assignments that keep every link pointing down and the stack no
 *  taller: highest-possible first, lowest-possible second, then others. */
function layerings(ids: string[], edges: C4Edge[]): Map<string, number>[] {
  const { dag, topo, asap, alap } = rankBounds(ids, edges);
  const preds = new Map<string, string[]>(ids.map((id) => [id, []]));
  for (const [u, vs] of dag) for (const v of vs) preds.get(v)!.push(u);
  const out: Map<string, number>[] = [asap];
  const same = (a: Map<string, number>, b: Map<string, number>) => ids.every((id) => a.get(id) === b.get(id));
  if (!same(asap, alap)) out.push(alap);
  const r = new Map<string, number>();
  const walk = (i: number) => {
    if (out.length >= MAX_LAYERINGS) return;
    if (i === topo.length) {
      if (!out.some((m) => same(m, r))) out.push(new Map(r));
      return;
    }
    const v = topo[i];
    const lo = Math.max(asap.get(v)!, ...preds.get(v)!.map((p) => r.get(p)! + 1));
    for (let k = lo; k <= alap.get(v)!; k++) {
      r.set(v, k);
      walk(i + 1);
    }
  };
  walk(0);
  return out;
}

/** Undirected neighbours. */
function neighbours(edges: C4Edge[]): Map<string, string[]> {
  const nb = new Map<string, string[]>();
  const add = (a: string, b: string) => {
    const l = nb.get(a);
    if (l) l.push(b);
    else nb.set(a, [b]);
  };
  for (const e of edges) {
    if (e.source === e.target) continue;
    add(e.source, e.target);
    add(e.target, e.source);
  }
  return nb;
}

/** Crossings between each row and the one above it, and every card or lane
 *  a link out to the beside column passes on its way right (`pull`: such
 *  links per node). */
function totalCrossings(rows: string[][], up: Map<string, string[]>, pull: Map<string, number>): number {
  let n = 0;
  for (let i = 0; i < rows.length; i++) {
    const at = new Map((rows[i - 1] ?? []).map((id, j) => [id, j]));
    const row = rows[i];
    for (let a = 0; a < row.length; a++) {
      n += (pull.get(row[a]) ?? 0) * (row.length - 1 - a);
      for (let b = a + 1; b < row.length; b++)
        for (const p of up.get(row[a]) ?? []) for (const q of up.get(row[b]) ?? []) if (at.get(p)! > at.get(q)!) n++;
    }
  }
  return n;
}

/** Order each row by the mean slot of its neighbours in the row it is swept
 *  from, down then up: a quick first answer for the exact search to beat. */
function sweepRows(
  rows: string[][],
  adj: Map<string, string[]>,
  up: Map<string, string[]>,
  pull: Map<string, number>,
): string[][] {
  let best = rows.map((r) => [...r]);
  let bestCount = totalCrossings(best, up, pull);
  const sweep = (row: string[], from: string[]) => {
    const at = new Map(from.map((id, i) => [id, i]));
    const key = new Map<string, number>();
    row.forEach((id, i) => {
      const ns = (adj.get(id) ?? []).filter((n) => at.has(n));
      key.set(id, ns.length ? ns.reduce((s, n) => s + at.get(n)!, 0) / ns.length : (i * from.length) / Math.max(1, row.length));
    });
    row.sort((a, b) => key.get(a)! - key.get(b)! || a.localeCompare(b));
  };
  const work = rows.map((r) => [...r]);
  for (let it = 0; it < 8 && bestCount > 0; it++) {
    for (let i = 1; i < work.length; i++) sweep(work[i], work[i - 1]);
    for (let i = work.length - 2; i >= 0; i--) sweep(work[i], work[i + 1]);
    const c = totalCrossings(work, up, pull);
    if (c < bestCount) {
      bestCount = c;
      best = work.map((r) => [...r]);
    }
  }
  return best;
}

/**
 * The row orders with the fewest crossings, by branch and bound: rows are
 * filled top-down one card at a time, each placement charged the crossings it
 * makes with the row above, and a branch is cut as soon as what it has spent
 * plus what the rest of its row must still cost reaches the best found. Every
 * row reversed draws the same, so the first row with two cards is searched
 * one way round only — unless links pull to the beside column, which has a
 * side. `budget` caps the placements tried.
 */
function exactOrder(
  start: string[][],
  up: Map<string, string[]>,
  pull: Map<string, number>,
  bound: number,
  budget: number,
): { rows: string[][] | null; crossings: number; steps: number } {
  let best: string[][] | null = null;
  let bestCost = bound;
  const cur: string[][] = start.map(() => []);
  const slot = new Map<string, number>();
  const mirrorRow = pull.size ? -1 : start.findIndex((r) => r.length > 1);
  let steps = 0;
  const rowSearch = (i: number, cost: number): void => {
    if (steps > budget) return;
    if (i === start.length) {
      if (cost < bestCost) {
        bestCost = cost;
        best = cur.map((r) => [...r]);
      }
      return;
    }
    const items = start[i];
    const k = items.length;
    // c[a][b]: crossings with the row above when a sits left of b, and b
    // passed by a's links out to the beside column.
    const c = items.map((a) =>
      items.map((b) => {
        let n = a !== b ? (pull.get(a) ?? 0) : 0;
        if (a !== b) for (const p of up.get(a) ?? []) for (const q of up.get(b) ?? []) if (slot.get(p)! > slot.get(q)!) n++;
        return n;
      }),
    );
    // Try cards nearest their neighbours' mean first, so good orders come early.
    const bary = items.map((a, j) => {
      const ns = up.get(a) ?? [];
      return ns.length ? ns.reduce((s, p) => s + slot.get(p)!, 0) / ns.length : j;
    });
    const order = items.map((_, j) => j).sort((a, b) => bary[a] - bary[b] || a - b);
    const used = new Array<boolean>(k).fill(false);
    const placed: number[] = [];
    const fill = (spent: number): void => {
      if (steps > budget) return;
      if (placed.length === k) {
        if (i === mirrorRow && placed[0] > placed[k - 1]) return;
        cur[i] = placed.map((j) => items[j]);
        cur[i].forEach((id, p) => slot.set(id, p));
        rowSearch(i + 1, spent);
        return;
      }
      for (const j of order) {
        if (used[j]) continue;
        steps++;
        let add = 0;
        for (const p of placed) add += c[p][j];
        // What the rest of the row must still cost: every placed card sits
        // left of every unplaced one, and each unplaced pair takes its
        // cheaper way round.
        let rest = 0;
        for (const q of order) {
          if (used[q] || q === j) continue;
          rest += c[j][q];
          for (const p of placed) rest += c[p][q];
        }
        for (let a = 0; a < k; a++) {
          if (used[a] || a === j) continue;
          for (let b = a + 1; b < k; b++) if (!used[b] && b !== j) rest += Math.min(c[a][b], c[b][a]);
        }
        if (spent + add + rest >= bestCost) continue;
        used[j] = true;
        placed.push(j);
        fill(spent + add);
        placed.pop();
        used[j] = false;
      }
    };
    fill(cost);
  };
  rowSearch(0, 0);
  return { rows: best, crossings: bestCost, steps };
}

/** Spread one row's wishes into positions that keep its order and its
 *  separations: the mean of pushing every clash rightwards and every clash
 *  leftwards, which moves the row as little as either. */
function settle(want: number[], sep: (i: number) => number): number[] {
  const n = want.length;
  const l = [...want], r = [...want];
  for (let i = 1; i < n; i++) l[i] = Math.max(l[i], l[i - 1] + sep(i));
  for (let i = n - 2; i >= 0; i--) r[i] = Math.min(r[i], r[i + 1] - sep(i + 1));
  return l.map((v, i) => (v + r[i]) / 2);
}

/** Whether segments pq and rs cross at a point inside both. */
function cross(p: Pt, q: Pt, r: Pt, s: Pt): boolean {
  const o = (a: Pt, b: Pt, c: Pt) => Math.sign((b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x));
  return o(p, q, r) * o(p, q, s) < 0 && o(r, s, p) * o(r, s, q) < 0;
}

function* permutations(n: number): Generator<number[]> {
  const a = [...Array(n).keys()];
  const c = new Array<number>(n).fill(0);
  yield [...a];
  for (let i = 0; i < n; ) {
    if (c[i] < i) {
      const j = i % 2 ? c[i] : 0;
      [a[j], a[i]] = [a[i], a[j]];
      yield [...a];
      c[i]++;
      i = 0;
    } else c[i++] = 0;
  }
}

/** A stack of rows with a lane point in every row a link passes. */
export interface Stack {
  rows: string[][];
  adj: Map<string, string[]>;
  /** Per node, its neighbours in the row above. */
  up: Map<string, string[]>;
  /** Per link that spans rows (keyed `source\0target`): its lane points,
   *  from the source's end. */
  lanes: Map<string, string[]>;
  rowOf: Map<string, number>;
  laneCount: number;
}

/** The stack for rows `base` and the links among them: a link that spans
 *  rows gets a lane point in each row it passes, so it runs between the
 *  cards there. Links with an end outside the rows are left out. */
export function withLanes(base: string[][], edges: C4Edge[]): Stack {
  const rows = base.map((r) => [...r]);
  const rowOf = new Map<string, number>();
  rows.forEach((row, ri) => row.forEach((id) => rowOf.set(id, ri)));
  const adj = new Map<string, string[]>();
  const up = new Map<string, string[]>();
  const link = (a: string, b: string) => {
    (adj.get(a) ?? adj.set(a, []).get(a)!).push(b);
    (adj.get(b) ?? adj.set(b, []).get(b)!).push(a);
    const [hi, lo] = rowOf.get(a)! < rowOf.get(b)! ? [a, b] : [b, a];
    (up.get(lo) ?? up.set(lo, []).get(lo)!).push(hi);
  };
  const lanes = new Map<string, string[]>();
  let laneCount = 0;
  for (const e of edges) {
    const a = rowOf.get(e.source), b = rowOf.get(e.target);
    if (a === undefined || b === undefined || a === b) continue;
    const step = b > a ? 1 : -1;
    const chain = [e.source];
    for (let ri = a + step; ri !== b; ri += step) {
      const id = `\0lane${laneCount++}:${ri}`;
      rows[ri].push(id);
      rowOf.set(id, ri);
      chain.push(id);
    }
    chain.push(e.target);
    for (let i = 1; i < chain.length; i++) link(chain[i - 1], chain[i]);
    if (chain.length > 2) lanes.set(`${e.source}\0${e.target}`, chain.slice(1, -1));
  }
  return { rows, adj, up, lanes, rowOf, laneCount };
}

/** The stack's rows in the order with the fewest crossings: a quick sweep,
 *  then the exact search within `budget` steps (none needed when the sweep
 *  found no crossing). */
export function orderStack(
  g: Stack,
  pull: Map<string, number>,
  budget: number,
): { rows: string[][]; crossings: number; steps: number } {
  const quick = sweepRows(g.rows, g.adj, g.up, pull);
  const quickCost = totalCrossings(quick, g.up, pull);
  if (quickCost === 0 || budget <= 0) return { rows: quick, crossings: quickCost, steps: 0 };
  const found = exactOrder(quick, g.up, pull, quickCost, budget);
  return found.rows
    ? { rows: found.rows, crossings: found.crossings, steps: found.steps }
    : { rows: quick, crossings: quickCost, steps: found.steps };
}

/** Each card's and lane's x centre, the whole centred on 0: each row packed
 *  (`gapX` between cards), then nudged towards the mean of its neighbours,
 *  down and up, so a card sits under what uses it and a lane runs straight. */
export function placeStack(rows: string[][], adj: Map<string, string[]>, cardW: number, gapX: number): Map<string, number> {
  const half = (id: string) => (isLane(id) ? 0 : cardW / 2);
  const gap = (a: string, b: string) => (isLane(a) && isLane(b) ? LANE_GAP : isLane(a) || isLane(b) ? LANE_GAP + 8 : gapX);
  const sepIn = (row: string[]) => (i: number) => half(row[i - 1]) + half(row[i]) + gap(row[i - 1], row[i]);
  const cx = new Map<string, number>();
  for (const row of rows) {
    const packed = settle(row.map(() => 0), sepIn(row));
    row.forEach((id, i) => cx.set(id, packed[i]));
  }
  const place = (row: string[], from: Set<string> | null) => {
    const sep = sepIn(row);
    const want: (number | null)[] = row.map((id) => {
      const xs = (adj.get(id) ?? []).filter((n) => !from || from.has(n)).map((n) => cx.get(n)!);
      return xs.length ? xs.reduce((s, v) => s + v, 0) / xs.length : null;
    });
    // A card with nothing to follow keeps close to its row-mates.
    if (want.every((w) => w === null)) return;
    for (let i = 1; i < row.length; i++) if (want[i] === null && want[i - 1] !== null) want[i] = want[i - 1]! + sep(i);
    for (let i = row.length - 2; i >= 0; i--) if (want[i] === null) want[i] = want[i + 1]! - sep(i + 1);
    settle(want as number[], sep).forEach((x, i) => cx.set(row[i], x));
  };
  for (let it = 0; it < 6; it++) {
    for (let i = 1; i < rows.length; i++) place(rows[i], new Set(rows[i - 1]));
    for (let i = rows.length - 2; i >= 0; i--) place(rows[i], new Set(rows[i + 1]));
  }
  for (const row of rows) place(row, null);
  // Straighten each lane: its points all want the lane's mean, the cards
  // stay where they are, and each row keeps its order and spacing — so a
  // long link runs down one line instead of stepping aside at every row.
  const chainOf = new Map<string, string[]>();
  for (const row of rows)
    for (const id of row) {
      if (!isLane(id) || chainOf.has(id)) continue;
      const chain = [id];
      for (let i = 0; i < chain.length; i++)
        for (const n of adj.get(chain[i]) ?? []) if (isLane(n) && !chain.includes(n)) chain.push(n);
      for (const l of chain) chainOf.set(l, chain);
    }
  for (let it = 0; it < 8 && chainOf.size; it++) {
    const mean = new Map<string[], number>();
    for (const chain of new Set(chainOf.values())) mean.set(chain, chain.reduce((t, l) => t + cx.get(l)!, 0) / chain.length);
    for (const row of rows) {
      if (!row.some(isLane)) continue;
      const want = row.map((id) => (isLane(id) ? mean.get(chainOf.get(id)!)! : cx.get(id)!));
      settle(want, sepIn(row)).forEach((x, i) => cx.set(row[i], x));
    }
  }
  // A row with nothing to follow (the tooling band, as a rule) centres under
  // the rows that have.
  const free = rows.filter((row) => row.every((id) => !(adj.get(id) ?? []).length));
  const held = rows.filter((row) => !free.includes(row)).flat().map((id) => cx.get(id)!);
  if (held.length) {
    const centre = (Math.min(...held) + Math.max(...held)) / 2;
    for (const row of free) {
      const rx = row.map((id) => cx.get(id)!);
      const shift = centre - (Math.min(...rx) + Math.max(...rx)) / 2;
      for (const id of row) cx.set(id, cx.get(id)! + shift);
    }
  }
  const xs = [...cx.values()];
  const mid = xs.length ? (Math.min(...xs) + Math.max(...xs)) / 2 : 0;
  for (const [id, x] of cx) cx.set(id, x - mid);
  return cx;
}

/** Each lane's route: two points per row it passes, the top and bottom of
 *  that row's cards (`rowTop`), in the link's direction. */
export function laneRoutes(g: Pick<Stack, "lanes" | "rowOf">, cx: Map<string, number>, rowTop: number[], cardH: number): Map<string, Pt[]> {
  const routes = new Map<string, Pt[]>();
  for (const [key, chain] of g.lanes) {
    const [s, t] = key.split("\0");
    const down = g.rowOf.get(s)! < g.rowOf.get(t)!;
    routes.set(
      key,
      chain.flatMap((id) => {
        const x = cx.get(id)!, y = rowTop[g.rowOf.get(id)!];
        return down ? [{ x, y }, { x, y: y + cardH }] : [{ x, y: y + cardH }, { x, y }];
      }),
    );
  }
  return routes;
}

export function c4Layout(input: C4Input): C4Layout {
  const { inside, tooling, above, beside, below, edges, cardW, cardH } = input;
  const pitchY = cardH + GAP_Y;
  const pos = new Map<string, Pt>();

  // Inside: dependency rows. A node with no edge inside the level has no
  // row to speak of: it fills out the last row, or rows of its own.
  const insideSet = new Set(inside);
  const insideEdges = edges.filter((e) => insideSet.has(e.source) && insideSet.has(e.target) && e.source !== e.target);
  const linked = new Set(insideEdges.flatMap((e) => [e.source, e.target]));
  const ranked = inside.filter((id) => linked.has(id));
  const loose = inside.filter((id) => !linked.has(id)).sort();
  const insideRowsFor = (r: Map<string, number>): string[][] => {
    const byRank: string[][] = [];
    for (const id of ranked) (byRank[r.get(id)!] ??= []).push(id);
    // Wrap a rank wider than MAX_ROW into consecutive rows.
    const rows = byRank.filter(Boolean).flatMap((row) => {
      row.sort();
      const out: string[][] = [];
      for (let i = 0; i < row.length; i += MAX_ROW) out.push(row.slice(i, i + MAX_ROW));
      return out;
    });
    if (loose.length) {
      const last = rows[rows.length - 1];
      if (last && last.length + loose.length <= MAX_ROW) last.push(...loose);
      else for (let i = 0; i < loose.length; i += MAX_ROW) rows.push(loose.slice(i, i + MAX_ROW));
    }
    return rows;
  };
  const toolRows: string[][] = [];
  const sortedTools = [...tooling].sort();
  const per = Math.max(MAX_ROW, Math.ceil(sortedTools.length / 2));
  for (let i = 0; i < sortedTools.length; i += per) toolRows.push(sortedTools.slice(i, i + per));

  // The links the rows are ordered by: all but the beside column's (drawn
  // sideways). Tooling's count too, though drawn quiet: they still need
  // lanes clear of the cards.
  const stacked = new Set([...above, ...inside, ...tooling, ...below]);
  const besideSet = new Set(beside);
  const rowEdges = edges.filter((e) => e.source !== e.target && stacked.has(e.source) && stacked.has(e.target));

  // The whole stack for one inside row assignment, with a lane point in
  // every row a link passes so it runs between the cards there.
  const build = (insideRows: string[][]) => {
    const g = withLanes(
      [...(above.length ? [[...above].sort()] : []), ...insideRows, ...toolRows, ...(below.length ? [[...below].sort()] : [])],
      rowEdges,
    );
    const { rowOf } = g;
    const pull = new Map<string, number>();
    for (const e of edges) {
      if (e.source === e.target) continue;
      for (const [a, b] of [[e.source, e.target], [e.target, e.source]])
        if (besideSet.has(b) && rowOf.has(a)) pull.set(a, (pull.get(a) ?? 0) + 1);
    }
    return { ...g, pull };
  };

  // Every row assignment, ordered for the fewest crossings; ties go to the
  // fewer lane points (shorter links), then to the earlier assignment.
  const assignments = ranked.length ? layerings(ranked, insideEdges) : [new Map<string, number>()];
  let chosen: ReturnType<typeof build> | null = null;
  let chosenCost = Infinity;
  let budget = SEARCH_BUDGET;
  for (const r of assignments) {
    const g = build(insideRowsFor(r));
    const share = budget > 0 ? Math.max(1, Math.floor(budget / Math.max(1, assignments.length - assignments.indexOf(r)))) : 0;
    const { rows, crossings: cost, steps } = orderStack(g, g.pull, share);
    budget -= steps;
    if (!chosen || cost < chosenCost || (cost === chosenCost && g.laneCount < chosen.laneCount)) {
      chosen = { ...g, rows };
      chosenCost = cost;
    }
    if (chosenCost === 0 && g.laneCount === 0) break;
  }
  const { rows, adj, rowOf } = chosen!;

  // Each row's height. Above sits clear of the boundary's label strip, the
  // tooling band under the inside rows, below clear of the boundary's foot.
  const aboveN = above.length ? 1 : 0;
  const insideN = rows.length - aboveN - toolRows.length - (below.length ? 1 : 0);
  const insideBottom = insideN ? (insideN - 1) * pitchY + cardH : 0;
  const toolTop = insideN ? insideBottom + GAP_Y : 0;
  const enclosed = inside.length + tooling.length > 0;
  const enclosedBottom = toolRows.length ? toolTop + LABEL_ROOM + (toolRows.length - 1) * pitchY + cardH + PAD / 2 : insideBottom;
  const rowY = rows.map((_, ri) => {
    if (ri < aboveN) return enclosed ? -PAD - LABEL_ROOM - GAP_Y - cardH : -GAP_Y - cardH;
    const k = ri - aboveN;
    if (k < insideN) return k * pitchY;
    if (k < insideN + toolRows.length) return toolTop + LABEL_ROOM + (k - insideN) * pitchY;
    return enclosed ? enclosedBottom + PAD + GAP_Y : GAP_Y;
  });

  // Horizontal places: a card under what uses it, a lane straight.
  const cx = placeStack(rows, adj, cardW, GAP_X);
  rows.forEach((row, ri) => row.forEach((id) => !isLane(id) && pos.set(id, { x: cx.get(id)! - cardW / 2, y: rowY[ri] })));
  const routes = laneRoutes(chosen!, cx, rowY, cardH);
  const enclosedLaneXs = [...cx.entries()]
    .filter(([id]) => isLane(id) && rowOf.get(id)! >= aboveN && rowOf.get(id)! < aboveN + insideN + toolRows.length)
    .map(([, x]) => x);

  // Tooling: the band around its rows.
  let toolingBand: Rect | null = null;
  if (tooling.length) {
    const txs = tooling.map((id) => pos.get(id)!.x);
    toolingBand = {
      x: Math.min(...txs) - PAD / 2,
      y: toolTop - PAD / 2,
      w: Math.max(...txs) - Math.min(...txs) + cardW + PAD,
      h: enclosedBottom - toolTop + PAD / 2,
    };
  }

  // The boundary: around everything inside the level.
  let boundary: Rect | null = null;
  if (enclosed) {
    let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
    for (const id of [...inside, ...tooling]) {
      const p = pos.get(id)!;
      x0 = Math.min(x0, p.x); y0 = Math.min(y0, p.y);
      x1 = Math.max(x1, p.x + cardW); y1 = Math.max(y1, p.y + cardH);
    }
    for (const x of enclosedLaneXs) {
      x0 = Math.min(x0, x - LANE_GAP);
      x1 = Math.max(x1, x + LANE_GAP);
    }
    if (toolingBand) {
      x0 = Math.min(x0, toolingBand.x); x1 = Math.max(x1, toolingBand.x + toolingBand.w);
      y1 = Math.max(y1, toolingBand.y + toolingBand.h);
    }
    boundary = { x: x0 - PAD, y: y0 - PAD - LABEL_ROOM, w: x1 - x0 + 2 * PAD, h: y1 - y0 + 2 * PAD + LABEL_ROOM };
  }

  // Beside: a column right of the boundary, each card level with the mean of
  // what it links to, in whichever order crosses the drawn links least.
  if (beside.length) {
    const nb = neighbours(edges);
    const colX = (boundary ? boundary.x + boundary.w : Math.max(0, ...[...pos.values()].map((p) => p.x + cardW))) + 3 * GAP_X + 60;
    const centre = (id: string) => {
      const p = pos.get(id)!;
      return { x: p.x + cardW / 2, y: p.y + cardH / 2 };
    };
    const wish = beside.map((id, i) => {
      const ys = (nb.get(id) ?? []).filter((n) => pos.has(n)).map((n) => pos.get(n)!.y);
      return ys.length ? ys.reduce((s, v) => s + v, 0) / ys.length : i * pitchY;
    });
    const drawn: [Pt, Pt][] = [];
    for (const e of rowEdges) {
      if (!pos.has(e.source) || !pos.has(e.target)) continue;
      const path = [centre(e.source), ...(routes.get(`${e.source}\0${e.target}`) ?? []), centre(e.target)];
      for (let i = 1; i < path.length; i++) drawn.push([path[i - 1], path[i]]);
    }
    const sideEdges = edges.filter((e) => e.source !== e.target && (besideSet.has(e.source) || besideSet.has(e.target)));
    const columnFor = (order: number[]) => {
      const ys = settle(order.map((i) => wish[i]), () => pitchY);
      // Keep the column centred where the wishes were.
      const drift = ys.reduce((s, v) => s + v, 0) / ys.length - wish.reduce((s, v) => s + v, 0) / wish.length;
      return new Map(order.map((i, k) => [beside[i], ys[k] - drift]));
    };
    const score = (col: Map<string, number>) => {
      const at = (id: string) => (col.has(id) ? { x: colX + cardW / 2, y: col.get(id)! + cardH / 2 } : pos.has(id) ? centre(id) : null);
      const segs = sideEdges.map((e) => [at(e.source), at(e.target)]).filter((s): s is [Pt, Pt] => !!s[0] && !!s[1]);
      let n = 0;
      for (let i = 0; i < segs.length; i++) {
        for (const d of drawn) if (cross(segs[i][0], segs[i][1], d[0], d[1])) n++;
        for (let j = i + 1; j < segs.length; j++) if (cross(segs[i][0], segs[i][1], segs[j][0], segs[j][1])) n++;
      }
      let moved = 0;
      beside.forEach((id, i) => (moved += Math.abs(col.get(id)! - wish[i])));
      return { n, moved };
    };
    const byWish = beside.map((_, i) => i).sort((a, b) => wish[a] - wish[b] || beside[a].localeCompare(beside[b]));
    let col = columnFor(byWish);
    if (beside.length <= MAX_BESIDE_PERMUTED) {
      let best = score(col);
      for (const perm of permutations(beside.length)) {
        const c = columnFor(perm.map((k) => byWish[k]));
        const s = score(c);
        if (s.n < best.n || (s.n === best.n && s.moved < best.moved - 1e-6)) {
          best = s;
          col = c;
        }
      }
    }
    for (const id of beside) pos.set(id, { x: colX, y: col.get(id)! });
  }

  return { positions: pos, boundary, toolingBand, routes };
}
