/**
 * How well a level is drawn, scored off the drawing itself: the cards where
 * the layout put them and each edge's line as the map draws it (`edgeLines`).
 * The faults a reader trips on are counted, never weighed against each other:
 *
 *   overlaps      two cards on top of each other — never acceptable
 *   throughCards  an edge running through a card that is not one of its ends,
 *                 so it reads as passing through that card
 *   crossings     points where two edges cross (meeting at a card is not one)
 *   sharedRuns    stretches where two edges run along one line, so neither
 *                 can be told from the other there
 *   bends         turns along the edges
 *
 * Pure geometry, so a test can hold every level of a real model to it.
 */

import type { Pt } from "../../shared/diagram/edgeGeometry";

export interface ScoredBox {
  id: string;
  /** Top-left. */
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface ScoredLine {
  id: string;
  source: string;
  target: string;
  points: Pt[];
}

export interface LayoutQuality {
  overlaps: [string, string][];
  throughCards: { edge: string; card: string }[];
  crossings: number;
  sharedRuns: number;
  bends: number;
}

/** A turn sharper than this counts as a bend (a bowed line's samples don't). */
const BEND_ANGLE = (20 * Math.PI) / 180;
/** Room a line may graze a card's border by without passing through it. */
const GRAZE = 2;
/** Shortest stretch two edges must share to count as running together. */
const SHARED = 4;

export function layoutQuality(boxes: ScoredBox[], lines: ScoredLine[]): LayoutQuality {
  const overlaps: [string, string][] = [];
  for (let i = 0; i < boxes.length; i++)
    for (let j = i + 1; j < boxes.length; j++) {
      const a = boxes[i], b = boxes[j];
      if (a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h) overlaps.push([a.id, b.id]);
    }

  const throughCards: { edge: string; card: string }[] = [];
  for (const l of lines)
    for (const b of boxes) {
      if (b.id === l.source || b.id === l.target) continue;
      if (segments(l).some(([p, q]) => entersBox(p, q, b))) throughCards.push({ edge: l.id, card: b.id });
    }

  let crossings = 0, sharedRuns = 0;
  const segs = lines.map(segments);
  for (let i = 0; i < lines.length; i++)
    for (let j = i + 1; j < lines.length; j++)
      for (const [p, q] of segs[i])
        for (const [r, s] of segs[j]) {
          if (cross(p, q, r, s)) crossings++;
          if (sharedLength(p, q, r, s) > SHARED) sharedRuns++;
        }

  let bends = 0;
  for (const l of lines)
    for (let i = 1; i < l.points.length - 1; i++) {
      const a = l.points[i - 1], p = l.points[i], b = l.points[i + 1];
      const turn = Math.abs(angleBetween(p.x - a.x, p.y - a.y, b.x - p.x, b.y - p.y));
      if (turn > BEND_ANGLE) bends++;
    }

  return { overlaps, throughCards, crossings, sharedRuns, bends };
}

function segments(l: ScoredLine): [Pt, Pt][] {
  const out: [Pt, Pt][] = [];
  for (let i = 1; i < l.points.length; i++) out.push([l.points[i - 1], l.points[i]]);
  return out;
}

/** Whether segment pq passes through the box's inside, grazing excepted
 *  (Liang–Barsky clip against the box shrunk by `GRAZE`). */
function entersBox(p: Pt, q: Pt, b: ScoredBox): boolean {
  const [x0, y0, x1, y1] = [b.x + GRAZE, b.y + GRAZE, b.x + b.w - GRAZE, b.y + b.h - GRAZE];
  const dx = q.x - p.x, dy = q.y - p.y;
  let t0 = 0, t1 = 1;
  for (const [m, n] of [
    [-dx, p.x - x0],
    [dx, x1 - p.x],
    [-dy, p.y - y0],
    [dy, y1 - p.y],
  ]) {
    if (m === 0) {
      if (n < 0) return false;
    } else {
      const t = n / m;
      if (m < 0) t0 = Math.max(t0, t);
      else t1 = Math.min(t1, t);
      if (t0 > t1) return false;
    }
  }
  return t1 - t0 > 1e-9;
}

/** Whether segments pq and rs cross at a point inside both. */
function cross(p: Pt, q: Pt, r: Pt, s: Pt): boolean {
  const o = (a: Pt, b: Pt, c: Pt) => Math.sign((b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x));
  return o(p, q, r) * o(p, q, s) < 0 && o(r, s, p) * o(r, s, q) < 0;
}

/** How long two axis-aligned segments run along one line together. */
function sharedLength(p: Pt, q: Pt, r: Pt, s: Pt): number {
  const along = (a: number, b: number, c: number, d: number) =>
    Math.min(Math.max(a, b), Math.max(c, d)) - Math.max(Math.min(a, b), Math.min(c, d));
  if (p.y === q.y && r.y === s.y && Math.abs(p.y - r.y) < 1) return along(p.x, q.x, r.x, s.x);
  if (p.x === q.x && r.x === s.x && Math.abs(p.x - r.x) < 1) return along(p.y, q.y, r.y, s.y);
  return 0;
}

function angleBetween(ax: number, ay: number, bx: number, by: number): number {
  return Math.atan2(ax * by - ay * bx, ax * bx + ay * by);
}
