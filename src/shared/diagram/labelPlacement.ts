/**
 * Where each edge's label pill goes, decided for all of a level's edges at
 * once so no pill covers what the drawing is trying to show. A pill is tried
 * at points along its own line, middle first, then just beside it; each spot
 * is charged for every other edge it would hide, every pill and card it would
 * sit on, and its distance from the middle. Longest labels choose first.
 */

import type { Pt } from "./edgeGeometry";

export interface LabelRequest {
  id: string;
  /** The edge's drawn line, source to target. */
  points: Pt[];
  label?: string;
  method?: string;
  /** Id shared by an edge and its reverse: hiding the twin costs less, since
   *  its own lane runs on beside the pill. */
  pair?: string;
}

interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** The pill's footprint (px), estimated from its text: 10px type, about
 *  5.6px a glyph, wrapped at 150px, with the method line under it. */
export function labelSize(label?: string, method?: string): { w: number; h: number } {
  const textW = (label?.length ?? 0) * 5.6;
  const lines = label ? Math.max(1, Math.ceil(textW / 136)) : 0;
  const w = Math.max(label ? Math.min(150, textW + 14) : 0, method ? (method.length + 2) * 5 : 0);
  const h = (label ? lines * 12.5 + 6 : 0) + (method ? 12 : 0);
  return { w, h };
}

/** Whether segment pq passes through rect r. */
function segmentHits(p: Pt, q: Pt, r: Rect): boolean {
  // Liang–Barsky: clip the segment's parameter range against each side.
  let t0 = 0, t1 = 1;
  const dx = q.x - p.x, dy = q.y - p.y;
  const sides: [number, number][] = [
    [-dx, p.x - r.x],
    [dx, r.x + r.w - p.x],
    [-dy, p.y - r.y],
    [dy, r.y + r.h - p.y],
  ];
  for (const [a, b] of sides) {
    if (a === 0) {
      if (b < 0) return false;
      continue;
    }
    const t = b / a;
    if (a < 0) t0 = Math.max(t0, t);
    else t1 = Math.min(t1, t);
    if (t0 > t1) return false;
  }
  return true;
}

const overlaps = (a: Rect, b: Rect, pad = 0) =>
  a.x < b.x + b.w + pad && b.x < a.x + a.w + pad && a.y < b.y + b.h + pad && b.y < a.y + a.h + pad;

/** A point `f` of the way along a polyline, with the direction there. */
function along(points: Pt[], f: number): { at: Pt; dir: Pt } {
  const legs = points.slice(1).map((q, i) => Math.hypot(q.x - points[i].x, q.y - points[i].y));
  const total = legs.reduce((s, l) => s + l, 0) || 1;
  let d = f * total;
  for (let i = 0; i < legs.length; i++) {
    if (d <= legs[i] || i === legs.length - 1) {
      const p = points[i], q = points[i + 1], l = legs[i] || 1;
      const t = Math.min(1, d / l);
      return { at: { x: p.x + (q.x - p.x) * t, y: p.y + (q.y - p.y) * t }, dir: { x: (q.x - p.x) / l, y: (q.y - p.y) / l } };
    }
    d -= legs[i];
  }
  return { at: points[0], dir: { x: 1, y: 0 } };
}

/** Fractions along the line, middle first. */
const STOPS = [0.5, 0.42, 0.58, 0.34, 0.66, 0.26, 0.74, 0.18, 0.82];

/** Steps off the line, in pill-clearances: on it, then out to either side.
 *  Positive is the edge's own outer side — where a reverse pair's lane
 *  already leans. */
const SIDES = [0, 1, -1, 2, -2];

/** Charges per spot: an edge hidden, a twin's lane hidden, a pill or card
 *  sat on, each step off the line (one more on the inner side), and each
 *  unit of distance from the middle. Hiding a twin costs more than a step
 *  aside, so a reverse pair's pills sit out on their own sides. */
const HIDES_EDGE = 10;
const HIDES_TWIN = 8;
const ON_PILL = 60;
const ON_CARD = 40;
const OFF_LINE = 6;
const FROM_MIDDLE = 4;

/** Pill centres per edge id, for the edges that carry a label or method. */
export function placeLabels(edges: LabelRequest[], cards: Rect[]): Map<string, Pt> {
  const legs = edges.map((e) => e.points.slice(1).map((q, i) => [e.points[i], q] as [Pt, Pt]));
  const want = edges
    .map((e, i) => ({ e, i, size: labelSize(e.label, e.method) }))
    .filter((w) => w.size.w > 0)
    .sort((a, b) => b.size.w * b.size.h - a.size.w * a.size.h || a.e.id.localeCompare(b.e.id));
  const placed: Rect[] = [];
  const out = new Map<string, Pt>();
  for (const { e, i, size } of want) {
    let best: { at: Pt; cost: number } | null = null;
    for (const f of STOPS) {
      const { at, dir } = along(e.points, f);
      // On the line, then pushed off it to either side: far enough that the
      // pill's near edge just clears the line.
      const nx = -dir.y, ny = dir.x;
      const clear = Math.abs(nx) * (size.w / 2) + Math.abs(ny) * (size.h / 2) + 4;
      for (const side of SIDES) {
        const c = { x: at.x + nx * clear * side, y: at.y + ny * clear * side };
        const r = { x: c.x - size.w / 2, y: c.y - size.h / 2, w: size.w, h: size.h };
        let cost = Math.abs(f - 0.5) * FROM_MIDDLE + Math.abs(side) * OFF_LINE + (side < 0 ? 1 : 0);
        edges.forEach((o, j) => {
          if (j === i) return;
          if (legs[j].some(([p, q]) => segmentHits(p, q, r))) cost += o.pair && o.pair === e.pair ? HIDES_TWIN : HIDES_EDGE;
        });
        for (const p of placed) if (overlaps(r, p, 4)) cost += ON_PILL;
        for (const card of cards) if (overlaps(r, card)) cost += ON_CARD;
        if (!best || cost < best.cost) best = { at: c, cost };
      }
    }
    out.set(e.id, best!.at);
    placed.push({ x: best!.at.x - size.w / 2, y: best!.at.y - size.h / 2, w: size.w, h: size.h });
  }
  return out;
}
