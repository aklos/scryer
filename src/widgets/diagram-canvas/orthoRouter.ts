/**
 * Edges routed around the cards, for a drawing with no one flow (rings, the
 * hexagon) where a straight line between two cards would run through what
 * lies between them.
 *
 * Each edge leaves its source from a side, runs in straight stretches with
 * right-angle turns through the room between cards, and enters its target
 * from a side: the cheapest such path on a grid of lines a margin clear of
 * every card (the cards' inflated edges, the ports, and the midlines between
 * them), where a stretch costs its length, a turn costs more, and running
 * across or along an edge already routed costs more still. Edges are routed
 * shortest first. Then the edges sharing a side of a card spread along it,
 * ordered by where each heads, and everything is routed again from those
 * ports, so edges leave a card side by side instead of on one line.
 */

import type { Pt } from "../../shared/diagram/edgeGeometry";
import type { PortBox, Side } from "./edgePorts";

export interface RoutedEdge {
  sourceSide: Side;
  targetSide: Side;
  /** Offset along the side from its middle: right along top/bottom, down
   *  along left/right. */
  sourceShift: number;
  targetShift: number;
  /** The turns between the two ports, source end first. */
  waypoints: Pt[];
}

/** Clearance between a route and any card. */
const MARGIN = 20;
/** Step between two ports on one side, as on the port drawings. */
const PORT_GAP = 16;
const PORT_SPAN = 0.8;
/** Step between two stretches parted off one line. */
const TRACK_GAP = 8;
/** Most stretches on one line whose every track order is tried. */
const MAX_PERMUTED = 6;
/** What a turn, a crossing, and running along another edge cost, in px. */
const BEND = 40;
const CROSS = 400;
const SHARE = 0.2;

const SIDES: Side[] = ["top", "right", "bottom", "left"];
/** Outward direction per side, as a step on the grid. */
const OUT: Record<Side, [number, number]> = { top: [0, -1], right: [1, 0], bottom: [0, 1], left: [-1, 0] };

export function routeAround(boxes: PortBox[], edges: { id: string; source: string; target: string }[]): Map<string, RoutedEdge> {
  const boxOf = new Map(boxes.map((b) => [b.id, b]));
  const routable = edges.filter((e) => e.source !== e.target && boxOf.has(e.source) && boxOf.has(e.target));
  if (!routable.length) return new Map();
  const order = [...routable].sort((a, b) => span(boxOf, a) - span(boxOf, b) || a.id.localeCompare(b.id));

  // First pass: every side open, ports in the middle — picks each end's side.
  const first = routeAll(boxes, order, () => SIDES.map((side) => ({ side, shift: 0 })));

  // Spread the ports sharing a side, ordered by where each edge heads next.
  const groups = new Map<string, { id: string; end: "source" | "target"; key: number }[]>();
  for (const e of order) {
    const r = first.get(e.id);
    if (!r) continue;
    const pts = r.points;
    for (const end of ["source", "target"] as const) {
      const side = end === "source" ? r.sourceSide : r.targetSide;
      const toward = end === "source" ? pts[Math.min(2, pts.length - 1)] : pts[Math.max(0, pts.length - 3)];
      const k = `${end === "source" ? e.source : e.target}\0${side}`;
      (groups.get(k) ?? groups.set(k, []).get(k)!).push({
        id: e.id,
        end,
        key: side === "top" || side === "bottom" ? toward.x : toward.y,
      });
    }
  }
  const fixed = new Map<string, { side: Side; shift: number }>();
  for (const [k, members] of groups) {
    const [id, side] = k.split("\0") as [string, Side];
    const b = boxOf.get(id)!;
    const len = side === "top" || side === "bottom" ? b.w : b.h;
    const gap = members.length > 1 ? Math.min(PORT_GAP, (len * PORT_SPAN) / (members.length - 1)) : 0;
    members.sort((p, q) => p.key - q.key || p.id.localeCompare(q.id));
    members.forEach((m, i) => fixed.set(`${m.id}\0${m.end}`, { side, shift: (i - (members.length - 1) / 2) * gap }));
  }

  // Second pass: from the spread ports.
  const second = routeAll(boxes, order, (e, end) => {
    const f = fixed.get(`${e.id}\0${end}`);
    return f ? [f] : SIDES.map((side) => ({ side, shift: 0 }));
  });
  const routes = new Map([...first, ...second]);
  nudge(routes, boxes);
  const out = new Map<string, RoutedEdge>();
  for (const e of order) {
    const r = routes.get(e.id);
    if (!r) continue;
    out.set(e.id, {
      sourceSide: r.sourceSide,
      targetSide: r.targetSide,
      sourceShift: r.sourceShift,
      targetShift: r.targetShift,
      waypoints: r.points.slice(1, -1),
    });
  }
  return out;
}

interface Route {
  sourceSide: Side;
  targetSide: Side;
  sourceShift: number;
  targetShift: number;
  /** Port, turns, port. */
  points: Pt[];
}

type Port = { side: Side; shift: number };

function span(boxOf: Map<string, PortBox>, e: { source: string; target: string }): number {
  const a = boxOf.get(e.source)!, b = boxOf.get(e.target)!;
  return Math.abs(a.x + a.w / 2 - b.x - b.w / 2) + Math.abs(a.y + a.h / 2 - b.y - b.h / 2);
}

function portAt(b: PortBox, p: Port): Pt {
  switch (p.side) {
    case "top":
      return { x: b.x + b.w / 2 + p.shift, y: b.y };
    case "bottom":
      return { x: b.x + b.w / 2 + p.shift, y: b.y + b.h };
    case "left":
      return { x: b.x, y: b.y + b.h / 2 + p.shift };
    case "right":
      return { x: b.x + b.w, y: b.y + b.h / 2 + p.shift };
  }
}

/** The port's stub: a margin straight out from it, where its route starts. */
function stubAt(b: PortBox, p: Port): Pt {
  const q = portAt(b, p), [dx, dy] = OUT[p.side];
  return { x: q.x + dx * MARGIN, y: q.y + dy * MARGIN };
}

function routeAll(
  boxes: PortBox[],
  edges: { id: string; source: string; target: string }[],
  portsOf: (e: { id: string; source: string; target: string }, end: "source" | "target") => Port[],
): Map<string, Route> {
  const boxOf = new Map(boxes.map((b) => [b.id, b]));
  const ends = edges.map((e) => ({
    e,
    from: portsOf(e, "source").map((p) => ({ p, at: stubAt(boxOf.get(e.source)!, p) })),
    to: portsOf(e, "target").map((p) => ({ p, at: stubAt(boxOf.get(e.target)!, p) })),
  }));

  // The grid: each card's inflated edges and centre lines, every stub, and
  // the midline between each two neighbouring lines.
  const xs0: number[] = [], ys0: number[] = [];
  for (const b of boxes) {
    xs0.push(b.x - MARGIN, b.x + b.w + MARGIN, b.x + b.w / 2);
    ys0.push(b.y - MARGIN, b.y + b.h + MARGIN, b.y + b.h / 2);
  }
  for (const { from, to } of ends) for (const { at } of [...from, ...to]) xs0.push(at.x), ys0.push(at.y);
  const xs = withMidlines(xs0), ys = withMidlines(ys0);
  const nx = xs.length, ny = ys.length;
  const xi = new Map(xs.map((v, i) => [v, i])), yi = new Map(ys.map((v, i) => [v, i]));

  // Cards inflated by just under the margin: a grid point inside one is shut.
  const inflated = boxes.map((b) => ({ x0: b.x - MARGIN + 1, y0: b.y - MARGIN + 1, x1: b.x + b.w + MARGIN - 1, y1: b.y + b.h + MARGIN - 1 }));
  const inside = (x: number, y: number) => inflated.some((r) => x > r.x0 && x < r.x1 && y > r.y0 && y < r.y1);
  const open = new Uint8Array(nx * ny);
  for (let j = 0; j < ny; j++) for (let i = 0; i < nx; i++) open[j * nx + i] = inside(xs[i], ys[j]) ? 0 : 1;
  // A stretch between two open neighbours is clear unless a card covers its
  // middle (no card edge lies strictly between two neighbouring lines).
  const clearH = new Uint8Array(nx * ny), clearV = new Uint8Array(nx * ny);
  for (let j = 0; j < ny; j++)
    for (let i = 0; i + 1 < nx; i++)
      clearH[j * nx + i] = open[j * nx + i] && open[j * nx + i + 1] && !inside((xs[i] + xs[i + 1]) / 2, ys[j]) ? 1 : 0;
  for (let j = 0; j + 1 < ny; j++)
    for (let i = 0; i < nx; i++)
      clearV[j * nx + i] = open[j * nx + i] && open[(j + 1) * nx + i] && !inside(xs[i], (ys[j] + ys[j + 1]) / 2) ? 1 : 0;

  // How many routed edges run along each stretch, and through each point
  // across and along.
  const useH = new Uint16Array(nx * ny), useV = new Uint16Array(nx * ny);
  const throughH = new Uint16Array(nx * ny), throughV = new Uint16Array(nx * ny);

  const out = new Map<string, Route>();
  for (const { e, from, to } of ends) {
    const node = (p: Pt) => {
      const i = xi.get(p.x), j = yi.get(p.y);
      return i === undefined || j === undefined || !open[j * nx + i] ? -1 : j * nx + i;
    };
    const starts = from.map((f) => ({ ...f, n: node(f.at) })).filter((f) => f.n >= 0);
    const goals = to.map((t) => ({ ...t, n: node(t.at) })).filter((t) => t.n >= 0);
    if (!starts.length || !goals.length) continue;
    const path = search(starts, goals);
    if (!path) continue;
    const s = boxOf.get(e.source)!, t = boxOf.get(e.target)!;
    const cells = path.cells;
    for (let k = 1; k < cells.length; k++) {
      const a = Math.min(cells[k - 1], cells[k]), b = Math.max(cells[k - 1], cells[k]);
      if (b - a === 1) useH[a]++;
      else useV[a]++;
    }
    for (let k = 0; k < cells.length; k++) {
      const moves = [cells[k - 1], cells[k + 1]].filter((c) => c !== undefined).map((c) => Math.abs(c - cells[k]));
      if (moves.includes(1)) throughH[cells[k]]++;
      if (moves.includes(nx)) throughV[cells[k]]++;
    }
    const pts = [portAt(s, path.from.p), ...cells.map((c) => ({ x: xs[c % nx], y: ys[Math.floor(c / nx)] })), portAt(t, path.to.p)];
    out.set(e.id, {
      sourceSide: path.from.p.side,
      targetSide: path.to.p.side,
      sourceShift: path.from.p.shift,
      targetShift: path.to.p.shift,
      points: corners(pts),
    });
  }
  return out;

  /** Cheapest path from a start stub to a goal stub: A* over (point, heading). */
  function search<S extends { p: Port; n: number }>(starts: S[], goals: S[]): { from: S; to: S; cells: number[] } | null {
    const dirs: [number, number][] = [[0, -1], [1, 0], [0, 1], [-1, 0]];
    const dirOf = (side: Side) => SIDES.indexOf(side);
    const goalAt = new Map<number, S[]>();
    for (const g of goals) (goalAt.get(g.n) ?? goalAt.set(g.n, []).get(g.n)!).push(g);
    const h = (n: number) => {
      const x = xs[n % nx], y = ys[Math.floor(n / nx)];
      let best = Infinity;
      for (const g of goals) best = Math.min(best, Math.abs(x - xs[g.n % nx]) + Math.abs(y - ys[Math.floor(g.n / nx)]));
      return best;
    };
    const key = (n: number, d: number) => n * 4 + d;
    const cost = new Map<number, number>();
    const prev = new Map<number, number>();
    const origin = new Map<number, S>();
    const heap = new MinHeap();
    for (const s of starts) {
      const k = key(s.n, dirOf(s.p.side));
      cost.set(k, 0);
      origin.set(k, s);
      heap.push(h(s.n), k);
    }
    let best: { k: number; total: number; goal: S } | null = null;
    while (heap.size) {
      const [f, k] = heap.pop();
      if (best && f >= best.total) break;
      const c = cost.get(k)!;
      if (f - h(Math.floor(k / 4)) > c + 1e-6) continue;
      const n = Math.floor(k / 4), d = k % 4;
      for (const g of goalAt.get(n) ?? []) {
        // The last step runs into the target: a turn unless already heading in.
        const inward = (dirOf(g.p.side) + 2) % 4;
        const total = c + (d === inward ? 0 : BEND);
        if (!best || total < best.total) best = { k, total, goal: g };
      }
      const i = n % nx, j = Math.floor(n / nx);
      for (let nd = 0; nd < 4; nd++) {
        if (nd === (d + 2) % 4) continue;
        const [dx, dy] = dirs[nd];
        const ii = i + dx, jj = j + dy;
        if (ii < 0 || jj < 0 || ii >= nx || jj >= ny) continue;
        const m = jj * nx + ii;
        const lo = Math.min(n, m);
        if (dx !== 0 ? !clearH[lo] : !clearV[lo]) continue;
        const len = Math.abs(xs[ii] - xs[i]) + Math.abs(ys[jj] - ys[j]);
        const shared = dx !== 0 ? useH[lo] : useV[lo];
        const crossing = dx !== 0 ? throughV[m] : throughH[m];
        const step = len * (1 + SHARE * shared) + (nd === d ? 0 : BEND) + CROSS * crossing;
        const nk = key(m, nd);
        const nc = c + step;
        if (nc < (cost.get(nk) ?? Infinity)) {
          cost.set(nk, nc);
          prev.set(nk, k);
          heap.push(nc + h(m), nk);
        }
      }
    }
    if (!best) return null;
    const cells: number[] = [];
    let k: number | undefined = best.k;
    while (k !== undefined) {
      cells.push(Math.floor(k / 4));
      if (origin.has(k)) break;
      k = prev.get(k);
    }
    cells.reverse();
    return { from: origin.get(k!)!, to: best.goal, cells };
  }
}

/**
 * Part the stretches that run along one line onto tracks of their own, a
 * step apart about that line, as far as the room to the nearest card allows.
 * The tracks go in the order whose turns cross each other least: a stretch
 * that turns off inside another's span, towards it, crosses it there. A
 * route's first and last stretch, out of a port, stay put.
 */
function nudge(routes: Map<string, Route>, boxes: PortBox[]) {
  interface Run { pts: Pt[]; i: number; across: boolean; at: number; lo: number; hi: number; dLo: number; dHi: number; id: string }
  const runs: Run[] = [];
  for (const [id, r] of routes) {
    const pts = r.points;
    for (let i = 1; i + 2 < pts.length; i++) {
      const p = pts[i], q = pts[i + 1];
      const across = p.y === q.y;
      if (!across && p.x !== q.x) continue;
      const at = across ? p.y : p.x;
      // Which way the route turns off at each end of the stretch.
      const side = (o: Pt) => Math.sign((across ? o.y : o.x) - at);
      const forward = (across ? q.x - p.x : q.y - p.y) > 0;
      runs.push({
        pts, i, across, at, id,
        lo: Math.min(across ? p.x : p.y, across ? q.x : q.y),
        hi: Math.max(across ? p.x : p.y, across ? q.x : q.y),
        dLo: side(forward ? pts[i - 1] : pts[i + 2]),
        dHi: side(forward ? pts[i + 2] : pts[i - 1]),
      });
    }
  }
  /** Crossings when `a` takes the track before `b` (nearer the low side). */
  const cost = (a: Run, b: Run) => {
    let n = 0;
    if (a.lo > b.lo + 1) n += a.dLo > 0 ? 1 : 0;
    else if (b.lo > a.lo + 1) n += b.dLo < 0 ? 1 : 0;
    else n += a.dLo > 0 && b.dLo < 0 ? 1 : 0;
    if (a.hi < b.hi - 1) n += a.dHi > 0 ? 1 : 0;
    else if (b.hi < a.hi - 1) n += b.dHi < 0 ? 1 : 0;
    else n += a.dHi > 0 && b.dHi < 0 ? 1 : 0;
    return n;
  };
  const arrange = (bunch: Run[]): Run[] => {
    const total = (order: Run[]) => {
      let n = 0;
      for (let i = 0; i < order.length; i++) for (let j = i + 1; j < order.length; j++) n += cost(order[i], order[j]);
      return n;
    };
    const byId = [...bunch].sort((a, b) => a.id.localeCompare(b.id) || a.i - b.i);
    if (bunch.length <= MAX_PERMUTED) {
      let best = byId, bestN = total(byId);
      for (const perm of permutations(byId)) {
        const n = total(perm);
        if (n < bestN) [best, bestN] = [perm, n];
      }
      return best;
    }
    // Too many to try every order: insert each where it costs least.
    const out: Run[] = [];
    for (const r of byId) {
      let at = 0, least = Infinity;
      for (let k = 0; k <= out.length; k++) {
        const n = total([...out.slice(0, k), r, ...out.slice(k)]);
        if (n < least) [at, least] = [k, n];
      }
      out.splice(at, 0, r);
    }
    return out;
  };
  const lines = new Map<string, Run[]>();
  for (const run of runs) {
    const k = `${run.across}\0${run.at}`;
    (lines.get(k) ?? lines.set(k, []).get(k)!).push(run);
  }
  for (const line of lines.values()) {
    line.sort((a, b) => a.lo - b.lo);
    // Runs that overlap along the line, as bunches.
    const bunches: Run[][] = [];
    let reach = -Infinity;
    for (const run of line) {
      if (run.lo >= reach - 4) bunches.push([]);
      bunches[bunches.length - 1].push(run);
      reach = Math.max(reach, run.hi);
    }
    for (const bunch of bunches) {
      if (bunch.length < 2) continue;
      const { across, at } = bunch[0];
      const lo = Math.min(...bunch.map((r) => r.lo)), hi = Math.max(...bunch.map((r) => r.hi));
      // Room either way to the nearest card beside the stretch.
      let room = MARGIN;
      for (const b of boxes) {
        const [b0, b1, c0, c1] = across ? [b.x, b.x + b.w, b.y, b.y + b.h] : [b.y, b.y + b.h, b.x, b.x + b.w];
        if (b1 <= lo || b0 >= hi) continue;
        room = Math.min(room, Math.max(0, at <= c0 ? c0 - at : at >= c1 ? at - c1 : 0));
      }
      const n = bunch.length;
      const step = Math.min(TRACK_GAP, (2 * Math.max(0, room - 4)) / (n - 1));
      if (step < 2) continue;
      arrange(bunch).forEach((run, t) => {
        const off = (t - (n - 1) / 2) * step;
        for (const p of [run.pts[run.i], run.pts[run.i + 1]]) {
          if (across) p.y = at + off;
          else p.x = at + off;
        }
      });
    }
  }
}

function* permutations<T>(items: T[]): Generator<T[]> {
  if (items.length <= 1) {
    yield items;
    return;
  }
  for (let i = 0; i < items.length; i++)
    for (const rest of permutations([...items.slice(0, i), ...items.slice(i + 1)])) yield [items[i], ...rest];
}

/** Sorted distinct values, with the midpoint between each two neighbours. */
function withMidlines(vs: number[]): number[] {
  const u = [...new Set(vs)].sort((a, b) => a - b);
  const out: number[] = [];
  u.forEach((v, i) => {
    if (i > 0 && v - u[i - 1] > 2 * PORT_GAP) out.push((v + u[i - 1]) / 2);
    out.push(v);
  });
  return out;
}

/** The points where the line turns, its two ends kept. */
function corners(pts: Pt[]): Pt[] {
  const out: Pt[] = [pts[0]];
  for (let i = 1; i < pts.length - 1; i++) {
    const a = out[out.length - 1], p = pts[i], b = pts[i + 1];
    if (p.x === a.x && p.y === a.y) continue;
    if (Math.abs((p.x - a.x) * (b.y - p.y) - (p.y - a.y) * (b.x - p.x)) > 0.5) out.push(p);
  }
  out.push(pts[pts.length - 1]);
  return out;
}

class MinHeap {
  private f: number[] = [];
  private v: number[] = [];
  get size() {
    return this.f.length;
  }
  push(f: number, v: number) {
    this.f.push(f);
    this.v.push(v);
    let i = this.f.length - 1;
    while (i > 0) {
      const p = (i - 1) >> 1;
      if (this.f[p] <= this.f[i]) break;
      this.swap(i, p);
      i = p;
    }
  }
  pop(): [number, number] {
    const top: [number, number] = [this.f[0], this.v[0]];
    const lf = this.f.pop()!, lv = this.v.pop()!;
    if (this.f.length) {
      this.f[0] = lf;
      this.v[0] = lv;
      let i = 0;
      for (;;) {
        const l = 2 * i + 1, r = l + 1;
        let m = i;
        if (l < this.f.length && this.f[l] < this.f[m]) m = l;
        if (r < this.f.length && this.f[r] < this.f[m]) m = r;
        if (m === i) break;
        this.swap(i, m);
        i = m;
      }
    }
    return top;
  }
  private swap(a: number, b: number) {
    [this.f[a], this.f[b]] = [this.f[b], this.f[a]];
    [this.v[a], this.v[b]] = [this.v[b], this.v[a]];
  }
}
