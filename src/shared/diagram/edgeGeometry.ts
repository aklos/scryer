/**
 * The line a relationship edge draws, from its two handle points: inset to
 * dot rims on the code tier, shifted to a parallel lane beside its reverse,
 * bowed off a ring, or run through waypoints in straight stretches with
 * rounded turns. Pure,
 * so the edge renders it and the label placement reads the same shape.
 */

export interface Pt {
  x: number;
  y: number;
}

export interface EdgeShapeOptions {
  /** Code tier: endpoints are dot centers — inset to the dot rim. */
  dot?: boolean;
  sourceR?: number;
  targetR?: number;
  /** A reverse edge runs between the same two nodes. */
  parallel?: boolean;
  bow?: { cx: number; cy: number; offset: number };
  waypoints?: Pt[];
}

export interface EdgeShape {
  /** Where the drawn line starts and ends. */
  sx: number;
  sy: number;
  tx: number;
  ty: number;
  /** Unit vector of the plain chord, source to target. */
  ux: number;
  uy: number;
  /** SVG path data. */
  path: string;
  /** The line as a polyline (a bowed edge sampled), for hit and overlap tests. */
  points: Pt[];
  /** Midpoint of the line: the hover handle, and the default label spot. */
  midX: number;
  midY: number;
  /** Direction the line arrives at the target, for the arrowhead. */
  arrowAngle: number;
}

/** Gap between a dot's rim and where the line/arrow starts, so it kisses the
 *  rim instead of overlapping the disc. Added to each end's own radius. */
const DOT_GAP = 3;
/** Sideways shift (px) for an edge that shares a node pair with its reverse, so
 *  the two land as separate parallel lanes instead of on top of each other. */
const PARALLEL_GAP = 5;
/** How far a routed edge's turns round off (px), at most. */
const CORNER_R = 10;

/** A path through `pts` with each inner corner rounded by a quadratic. */
function roundedPolyline(pts: Pt[]): string {
  let d = `M ${pts[0].x} ${pts[0].y}`;
  for (let i = 1; i < pts.length - 1; i++) {
    const a = pts[i - 1], p = pts[i], b = pts[i + 1];
    const la = Math.hypot(p.x - a.x, p.y - a.y) || 1, lb = Math.hypot(b.x - p.x, b.y - p.y) || 1;
    const r = Math.min(CORNER_R, la / 2, lb / 2);
    d += ` L ${p.x - ((p.x - a.x) / la) * r} ${p.y - ((p.y - a.y) / la) * r}`;
    d += ` Q ${p.x} ${p.y} ${p.x + ((b.x - p.x) / lb) * r} ${p.y + ((b.y - p.y) / lb) * r}`;
  }
  const z = pts[pts.length - 1];
  return `${d} L ${z.x} ${z.y}`;
}
export function edgeShape(sourceX: number, sourceY: number, targetX: number, targetY: number, o: EdgeShapeOptions = {}): EdgeShape {
  // For dot-tier edges the endpoints are dot centers; pull each end in by its
  // own dot radius so the line starts/ends at the rim and the arrow lands on it.
  const dx = targetX - sourceX, dy = targetY - sourceY;
  const len = Math.hypot(dx, dy) || 1;
  const ux = dx / len, uy = dy / len;
  const sInset = o.dot ? Math.min((o.sourceR ?? 6) + DOT_GAP, len / 2 - 1) : 0;
  const tInset = o.dot ? Math.min((o.targetR ?? 6) + DOT_GAP, len / 2 - 1) : 0;
  // A reverse edge between the same two nodes would land exactly on top of this
  // one now that the line is straight — shift the whole chord sideways by its
  // perpendicular. The perpendicular flips with direction, so A→B and B→A push
  // to opposite sides and read as two parallel lanes.
  const off = o.parallel ? PARALLEL_GAP : 0;
  const ox = -uy * off, oy = ux * off;
  const sx = sourceX + ux * sInset + ox, sy = sourceY + uy * sInset + oy;
  const tx = targetX - ux * tInset + ox, ty = targetY - uy * tInset + oy;

  if (o.bow) {
    // A bowed edge (styled rings) is a quadratic curve whose control point
    // sits past the chord's midpoint, away from the drawing's centre.
    const mx = (sx + tx) / 2, my = (sy + ty) / 2;
    let vx = mx - o.bow.cx, vy = my - o.bow.cy;
    let vl = Math.hypot(vx, vy);
    if (vl < 8) {
      // Antipodal ends: the chord runs through the centre, so bow to the
      // chord's right-hand side instead.
      vx = -uy; vy = ux; vl = 1;
    }
    // A quadratic's midpoint sits halfway to its control point, so doubling
    // displaces the curve's middle exactly `offset` px from the chord.
    const cxp = mx + (vx / vl) * o.bow.offset * 2;
    const cyp = my + (vy / vl) * o.bow.offset * 2;
    const at = (t: number) => ({
      x: (1 - t) * (1 - t) * sx + 2 * (1 - t) * t * cxp + t * t * tx,
      y: (1 - t) * (1 - t) * sy + 2 * (1 - t) * t * cyp + t * t * ty,
    });
    const mid = at(0.5);
    return {
      sx, sy, tx, ty, ux, uy,
      path: `M ${sx} ${sy} Q ${cxp} ${cyp} ${tx} ${ty}`,
      points: Array.from({ length: 13 }, (_, i) => at(i / 12)),
      midX: mid.x,
      midY: mid.y,
      arrowAngle: Math.atan2(ty - cyp, tx - cxp),
    };
  }

  // Waypoints shift with the lane like the ends do.
  const via = (o.waypoints ?? []).map((p) => ({ x: p.x + ox, y: p.y + oy }));
  if (via.length) {
    const points = [{ x: sx, y: sy }, ...via, { x: tx, y: ty }];
    // The middle of the middle leg.
    const a = points[Math.floor((points.length - 1) / 2)], b = points[Math.floor((points.length - 1) / 2) + 1];
    const last = via[via.length - 1];
    return {
      sx, sy, tx, ty, ux, uy,
      path: roundedPolyline(points),
      points,
      midX: (a.x + b.x) / 2,
      midY: (a.y + b.y) / 2,
      arrowAngle: Math.atan2(ty - last.y, tx - last.x),
    };
  }

  return {
    sx, sy, tx, ty, ux, uy,
    path: `M ${sx} ${sy} L ${tx} ${ty}`,
    points: [{ x: sx, y: sy }, { x: tx, y: ty }],
    midX: (sx + tx) / 2,
    midY: (sy + ty) / 2,
    arrowAngle: Math.atan2(ty - sy, tx - sx),
  };
}
