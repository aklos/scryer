/**
 * Where each edge meets its cards on the box tiers, for every edge of a level
 * at once. A drawing has a flow — down a stack of rows, or across a row of
 * columns — and an edge between two places along it leaves its source's foot
 * and enters its target's head (the reverse when it climbs); any other edge
 * (one along a row, one into the column beside the rows) runs side to side.
 * Edges sharing a side of a card spread along it, one even step apart,
 * ordered by where each is heading, so none piles onto another and none
 * crosses its neighbour at the card. A reverse pair gets two ports: two
 * lines, never one on the other. Then an edge whose ends nearly line up has
 * one of its ports slid along its side until they do, where the side has
 * room, so it runs straight instead of jogging.
 */

import type { Pt } from "../../shared/diagram/edgeGeometry";

export type Side = "top" | "bottom" | "left" | "right";

export interface PortBox {
  id: string;
  /** Top-left. */
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface PortEdge {
  id: string;
  source: string;
  target: string;
  /** A routed lane's points, source end first. */
  waypoints?: Pt[];
}

export interface Port {
  sourceSide: Side;
  targetSide: Side;
  /** Offset along the side from its middle (px): right along top/bottom,
   *  down along left/right. */
  sourceShift: number;
  targetShift: number;
  flow: "down" | "across";
}

/** Step between two ports on one side, the same everywhere. */
const PORT_GAP = 16;
/** Share of a side's length its ports may spread over: a crowded side
 *  closes its steps up to fit. */
const PORT_SPAN = 0.8;
/** Nearest a port comes to its side's corners. */
const PORT_INSET = 20;

/** A side's middle point, moved `shift` along the side. */
export function portPoint(b: PortBox, side: Side, shift = 0): Pt {
  switch (side) {
    case "top":
      return { x: b.x + b.w / 2 + shift, y: b.y };
    case "bottom":
      return { x: b.x + b.w / 2 + shift, y: b.y + b.h };
    case "left":
      return { x: b.x, y: b.y + b.h / 2 + shift };
    case "right":
      return { x: b.x + b.w, y: b.y + b.h / 2 + shift };
  }
}

/** `flow`: which way the level's drawing runs; `aside`: cards in a column
 *  beside it, whose links always run across the flow. */
export function assignPorts(
  boxes: PortBox[],
  edges: PortEdge[],
  flow: "down" | "across",
  aside: ReadonlySet<string> = new Set(),
): Map<string, Port> {
  const box = new Map(boxes.map((b) => [b.id, b]));
  const centre = (b: PortBox): Pt => ({ x: b.x + b.w / 2, y: b.y + b.h / 2 });
  const out = new Map<string, Port>();

  // Sides first.
  for (const e of edges) {
    const s = box.get(e.source), t = box.get(e.target);
    if (!s || !t) continue;
    const a = centre(s), b = centre(t);
    // Along the flow between cards apart that way (two rows, two columns);
    // across within one, and to the column beside.
    const apart = flow === "down" ? s.y + s.h <= t.y || t.y + t.h <= s.y : s.x + s.w <= t.x || t.x + t.w <= s.x;
    const along = !!e.waypoints?.length || (apart && !aside.has(e.source) && !aside.has(e.target));
    const ahead = flow === "down" ? b.y > a.y : b.x > a.x;
    const rightward = flow === "down" ? b.x > a.x : b.y > a.y;
    const edgeFlow: "down" | "across" = along === (flow === "down") ? "down" : "across";
    const [fwdS, fwdT, backS, backT]: Side[] =
      edgeFlow === "down" ? ["bottom", "top", "top", "bottom"] : ["right", "left", "left", "right"];
    const forward = along ? ahead : rightward;
    out.set(e.id, {
      sourceSide: forward ? fwdS : backS,
      targetSide: forward ? fwdT : backT,
      sourceShift: 0,
      targetShift: 0,
      flow: edgeFlow,
    });
  }

  // Then spread each side's ports, ordered by where each edge heads next.
  const groups = new Map<string, { edge: PortEdge; end: "source" | "target"; key: number; tie: string }[]>();
  for (const e of edges) {
    const p = out.get(e.id);
    if (!p) continue;
    for (const end of ["source", "target"] as const) {
      const me = box.get(end === "source" ? e.source : e.target)!;
      const other = box.get(end === "source" ? e.target : e.source)!;
      const side = end === "source" ? p.sourceSide : p.targetSide;
      const wp = e.waypoints ?? [];
      const toward = (end === "source" ? wp[0] : wp[wp.length - 1]) ?? centre(other);
      const horizontal = side === "top" || side === "bottom";
      const k = `${me.id}\0${side}`;
      (groups.get(k) ?? groups.set(k, []).get(k)!).push({
        edge: e,
        end,
        key: horizontal ? toward.x : toward.y,
        // A reverse pair heads to one place: keep its two in a fixed order,
        // the same at both cards, so they run side by side without crossing.
        tie: `${[e.source, e.target].sort().join("\0")}\0${e.source < e.target ? 0 : 1}`,
      });
    }
  }
  const lenOf = (k: string) => {
    const [id, side] = k.split("\0") as [string, Side];
    const b = box.get(id)!;
    return side === "top" || side === "bottom" ? b.w : b.h;
  };
  for (const [k, members] of groups) {
    if (members.length < 2) continue;
    const len = lenOf(k);
    const gap = Math.min(PORT_GAP, (len * PORT_SPAN) / (members.length - 1));
    members.sort((p, q) => p.key - q.key || p.tie.localeCompare(q.tie));
    members.forEach((m, i) => {
      const shift = (i - (members.length - 1) / 2) * gap;
      const p = out.get(m.edge.id)!;
      if (m.end === "source") p.sourceShift = shift;
      else p.targetShift = shift;
    });
  }

  // Straighten: an edge with no lane whose ports nearly line up slides one
  // of them (the target's first) along its side to line up exactly, if it
  // stays clear of its side's corners and of the ports beside it. Nearest
  // misses first, so a small fix never loses its spot to a larger one.
  const sideOf = new Map<string, string>();
  for (const [k, members] of groups) for (const m of members) sideOf.set(`${m.edge.id}\0${m.end}`, k);
  const shiftOf = (id: string, end: "source" | "target") => (end === "source" ? out.get(id)!.sourceShift : out.get(id)!.targetShift);
  const misses = edges
    .filter((e) => out.has(e.id) && !e.waypoints?.length)
    .map((e) => {
      const p = out.get(e.id)!;
      const a = portPoint(box.get(e.source)!, p.sourceSide, p.sourceShift);
      const b = portPoint(box.get(e.target)!, p.targetSide, p.targetShift);
      const horizontal = p.sourceSide === "top" || p.sourceSide === "bottom";
      return { e, d: horizontal ? a.x - b.x : a.y - b.y };
    })
    .filter((m) => m.d !== 0)
    .sort((m, n) => Math.abs(m.d) - Math.abs(n.d));
  for (const { e, d } of misses) {
    for (const [end, by] of [["target", d], ["source", -d]] as const) {
      const k = sideOf.get(`${e.id}\0${end}`)!;
      const to = shiftOf(e.id, end) + by;
      const half = lenOf(k) / 2 - PORT_INSET;
      const others = groups.get(k)!.filter((m) => m.edge.id !== e.id || m.end !== end).map((m) => shiftOf(m.edge.id, m.end));
      const from = shiftOf(e.id, end);
      // Room, and no neighbour passed or crowded on the way.
      const clear = others.every((o) => Math.abs(o - to) >= PORT_GAP * 0.75 && Math.sign(o - from) === Math.sign(o - to));
      if (Math.abs(to) > half || !clear) continue;
      if (end === "source") out.get(e.id)!.sourceShift = to;
      else out.get(e.id)!.targetShift = to;
      break;
    }
  }
  return out;
}

export interface OrthoEdge {
  id: string;
  /** Its two ports: where it leaves its source and enters its target. */
  start: Pt;
  end: Pt;
  /** Which way it leaves: down/up out of a foot or head, or sideways. */
  axis: "down" | "across";
  /** A routed lane's points between, source end first. */
  waypoints?: Pt[];
}

/** Room between two tracks in one gap. */
const TRACK_GAP = 8;
/** Room kept between the outermost track and the cards either side. */
const TRACK_MARGIN = 14;
/** Turns this close along the flow share one gap's tracks, though their legs
 *  start at different cards (the column beside the rows). */
const SAME_GAP = 60;

/**
 * Each edge as straight stretches with right-angle turns (the renderer rounds
 * them): out of its port along the flow, one turn in the gap before the next
 * row (or column) to line up with where it goes, then straight on. Edges
 * turning in one gap take tracks of their own there, so no two run along one
 * line. Returns the turn points between the ports; none for a straight edge.
 */
export function orthoRoutes(edges: OrthoEdge[]): Map<string, Pt[]> {
  // Every jog: a stretch across the flow, in the gap between two stops.
  // `u`, `v`: where it crosses the gap's near and far edge (top then bottom
  // of a gap between rows; left then right of one between columns).
  interface Jog { id: string; axis: "down" | "across"; leg: number; base: number; min: number; max: number; lo: number; hi: number; u: number; v: number }
  const legs = new Map<string, Pt[]>();
  const jogs: Jog[] = [];
  for (const e of edges) {
    const stops = [e.start, ...(e.waypoints ?? []), e.end];
    legs.set(e.id, stops);
    for (let i = 1; i < stops.length; i++) {
      const p = stops[i - 1], q = stops[i];
      const [pa, qa, pc, qc] = e.axis === "down" ? [p.y, q.y, p.x, q.x] : [p.x, q.x, p.y, q.y];
      if (Math.abs(pc - qc) < 1) continue;
      jogs.push({
        id: e.id, axis: e.axis, leg: i, base: (pa + qa) / 2, min: Math.min(pa, qa), max: Math.max(pa, qa),
        lo: Math.min(pc, qc), hi: Math.max(pc, qc), u: pa < qa ? pc : qc, v: pa < qa ? qc : pc,
      });
    }
  }
  // The gaps: jogs on one axis whose turns fall close together.
  const gaps: Jog[][] = [];
  for (const j of [...jogs].sort((a, b) => (a.axis === b.axis ? a.base - b.base : a.axis < b.axis ? -1 : 1))) {
    const last = gaps[gaps.length - 1];
    if (last && last[0].axis === j.axis && j.base - last[0].base <= SAME_GAP) last.push(j);
    else gaps.push([j]);
  }
  const at = new Map<Jog, number>();
  for (const gap of gaps) {
    // Jogs whose stretches overlap need tracks of their own; apart, they
    // share the gap's middle.
    gap.sort((a, b) => a.lo - b.lo || a.hi - b.hi);
    const bunches: Jog[][] = [];
    let reach = -Infinity;
    for (const j of gap) {
      if (j.lo > reach + TRACK_GAP) bunches.push([]);
      bunches[bunches.length - 1].push(j);
      reach = Math.max(reach, j.hi);
    }
    const base = gap.reduce((t, j) => t + j.base, 0) / gap.length;
    const room = Math.min(...gap.map((j) => j.max - j.min));
    for (const bunch of bunches) {
      // Tracks from the gap's near edge to its far one. A jog heading up the
      // cross axis (v < u) turns early when it starts early; one heading
      // down it turns late when it starts early, so its later neighbours'
      // first stretches pass under its turn. Either way parallel runs nest.
      const back = bunch.filter((j) => j.v < j.u).sort((a, b) => a.u - b.u || a.v - b.v);
      const ahead = bunch.filter((j) => j.v >= j.u).sort((a, b) => b.u - a.u || b.v - a.v);
      const order = [...back, ...ahead];
      const n = order.length;
      const step = n > 1 ? Math.min(TRACK_GAP, Math.max(0, room - 2 * TRACK_MARGIN) / (n - 1)) : 0;
      // Each on its track about the gap's middle, kept inside its own leg.
      order.forEach((j, t) => {
        const m = base + (t - (n - 1) / 2) * step;
        at.set(j, Math.min(j.max - TRACK_MARGIN, Math.max(j.min + TRACK_MARGIN, m)));
      });
    }
  }
  const jogAt = new Map(jogs.map((j) => [`${j.id}\0${j.leg}`, j]));
  const out = new Map<string, Pt[]>();
  for (const e of edges) {
    const stops = legs.get(e.id)!;
    const pts: Pt[] = [];
    for (let i = 1; i < stops.length; i++) {
      const p = stops[i - 1], q = stops[i];
      const j = jogAt.get(`${e.id}\0${i}`);
      if (j) {
        const m = at.get(j)!;
        if (e.axis === "down") pts.push({ x: p.x, y: m }, { x: q.x, y: m });
        else pts.push({ x: m, y: p.y }, { x: m, y: q.y });
      }
      if (i < stops.length - 1) pts.push(q);
    }
    // A stop that a straight stretch runs through is no turn.
    out.set(
      e.id,
      pts.filter((q, i) => {
        const a = i === 0 ? e.start : pts[i - 1], b = i === pts.length - 1 ? e.end : pts[i + 1];
        return Math.abs((q.x - a.x) * (b.y - q.y) - (q.y - a.y) * (b.x - q.x)) > 0.5;
      }),
    );
  }
  return out;
}
