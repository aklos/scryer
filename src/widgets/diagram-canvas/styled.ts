/**
 * Styled layout — one established drawing per architectural style.
 *
 * The component level of a styled container is not a free graph: every card
 * carries a layer, and the style fixes where a layer sits. The shape of the
 * map is the "you are here" for someone seeing a codebase for the first time:
 *
 *   - `rows`    (feature-sliced): stacked bands, first layer on top
 *   - `columns` (pipeline):       left to right, one column per stage
 *   - `rings`   (core-shell):     concentric, last layer innermost
 *   - `hexagon` (hexagonal, library): Cockburn's hexagon — domain centre,
 *                                 application ring, driving side left,
 *                                 driven side right, composition above
 *
 * Placement is deterministic: the layer fixes the band / ring / column, and
 * the only freedom is the order within it, chosen to shorten links. On the
 * bands, a link that skips a band runs down a lane between the cards of every
 * band it passes, and the order is the one with the fewest crossings — as on
 * the box tiers (`c4Layout`). Ghosts
 * (nodes referenced from this level but living elsewhere) sit outside the
 * drawing on the side their traffic comes from.
 *
 * Pure geometry: takes members with layers and the edges among them, returns
 * card centers plus the regions the renderer draws behind the cards.
 */

import type { Drawing, StyleDef } from "../../entities/model/styles";
import { LANE_GAP, isLane, laneRoutes, orderStack, placeStack, withLanes } from "./c4Layout";

export interface StyledMember {
  id: string;
  /** Undefined for ghosts (they live outside this container). */
  layer?: string;
}

export interface StyledEdge {
  source: string;
  target: string;
}

/** A layer's region behind the cards. Rects for rows/columns and the hex
 *  sides, rings for the concentric drawings, `hex` for the hexagon itself. */
interface RegionMeta {
  /** The label: a layer name, or a band caption like "called from". */
  layer: string;
  /** The layer's one-line description, drawn beside the label so the map
   *  explains its own vocabulary in place. */
  caption?: string;
  /** A band of ghosts (nodes living outside this level) — drawn open. */
  ghost?: boolean;
  /** The C4 boundary of the level's own node: labelled with its name and
   *  kind, drawn as a frame rather than a band. */
  boundary?: boolean;
}
export type LayerRegion =
  | (RegionMeta & { shape: "rect"; x: number; y: number; w: number; h: number })
  | (RegionMeta & { shape: "ring"; cx: number; cy: number; r: number })
  | (RegionMeta & { shape: "hex"; cx: number; cy: number; r: number });

export interface StyledLayout {
  /** Card CENTERS (the caller converts to React Flow's top-left). */
  centers: Map<string, { x: number; y: number }>;
  regions: LayerRegion[];
  /** Bands only: per link that skips a band (keyed `source\0target`), the
   *  points it passes on the way, between the cards. */
  routes?: Map<string, { x: number; y: number }[]>;
}

export const CARD_W = 180;
export const CARD_H = 160;
/** Horizontal pitch between cards in a band. */
const PITCH_X = 240;
/** Steps the band order search may take. */
const SEARCH_BUDGET = 400_000;
/** Vertical pitch between bands (card plus the band's own label room). */
const PITCH_Y = 250;
/** Padding between a card and its region's edge. */
const PAD = 28;
/** Room above the cards for the region label. */
const LABEL_ROOM = 22;

// ── ordering ────────────────────────────────────────────────────────────────

/** Barycenter ordering: order each band by the mean position of the neighbours
 *  in already-placed bands, sweeping down then up a few times. Ties keep the
 *  incoming (name-stable) order so the result is deterministic. */
export function orderBands(bands: string[][], edges: StyledEdge[]): string[][] {
  const adj = new Map<string, string[]>();
  for (const e of edges) {
    if (e.source === e.target) continue;
    (adj.get(e.source) ?? adj.set(e.source, []).get(e.source)!).push(e.target);
    (adj.get(e.target) ?? adj.set(e.target, []).get(e.target)!).push(e.source);
  }
  const bandOf = new Map<string, number>();
  bands.forEach((b, i) => b.forEach((id) => bandOf.set(id, i)));
  const out = bands.map((b) => [...b]);
  const pos = new Map<string, number>();
  const stamp = () => out.forEach((b) => b.forEach((id, i) => pos.set(id, i)));
  stamp();
  // Each band is ordered against the band the sweep just placed (the one
  // above on the way down, below on the way up), so the first band keeps its
  // incoming order and the rest follow it instead of all shuffling at once.
  const sweep = (order: number[], from: number) => {
    for (const bi of order) {
      const band = out[bi];
      const ref = bi + from;
      if (band.length < 2 || ref < 0 || ref >= out.length) continue;
      const key = new Map<string, number>();
      for (const id of band) {
        const ns = (adj.get(id) ?? []).filter((n) => bandOf.get(n) === ref);
        key.set(
          id,
          ns.length === 0 ? pos.get(id)! : ns.reduce((s, n) => s + pos.get(n)!, 0) / ns.length,
        );
      }
      band.sort((a, b) => key.get(a)! - key.get(b)! || pos.get(a)! - pos.get(b)!);
      stamp();
    }
  };
  const down = out.map((_, i) => i);
  const up = [...down].reverse();
  for (let i = 0; i < 3; i++) {
    sweep(down, -1);
    sweep(up, +1);
  }
  return out;
}

/** Split members into bands by the style's layer order; unknown layers land in
 *  a trailing band so nothing vanishes. Empty layers are dropped. */
function bandsByLayer(def: StyleDef, members: StyledMember[]): { layer: string; ids: string[] }[] {
  const order = def.layers.map((l) => l.name);
  const byLayer = new Map<string, string[]>();
  for (const m of members) {
    const l = m.layer && order.includes(m.layer) ? m.layer : "?";
    (byLayer.get(l) ?? byLayer.set(l, []).get(l)!).push(m.id);
  }
  const out: { layer: string; ids: string[] }[] = [];
  for (const l of [...order, "?"]) {
    const ids = byLayer.get(l);
    if (ids?.length) out.push({ layer: l, ids });
  }
  return out;
}

/** Which side a ghost belongs on: `in` when it mostly feeds this level (a
 *  caller), `out` when it is mostly fed by it (a dependency). */
function ghostSides(ghosts: string[], memberIds: Set<string>, edges: StyledEdge[]): Map<string, "in" | "out"> {
  const score = new Map<string, number>();
  for (const e of edges) {
    if (memberIds.has(e.source) && !memberIds.has(e.target)) score.set(e.target, (score.get(e.target) ?? 0) - 1);
    if (memberIds.has(e.target) && !memberIds.has(e.source)) score.set(e.source, (score.get(e.source) ?? 0) + 1);
  }
  return new Map(ghosts.map((g) => [g, (score.get(g) ?? 0) >= 0 ? "in" : "out"]));
}

// ── rows / columns ──────────────────────────────────────────────────────────

function bandsLayout(
  def: StyleDef,
  members: StyledMember[],
  ghosts: string[],
  edges: StyledEdge[],
  transpose: boolean,
): StyledLayout {
  const memberIds = new Set(members.map((m) => m.id));
  const sides = ghostSides(ghosts, memberIds, edges);
  const inGhosts = ghosts.filter((g) => sides.get(g) === "in");
  const outGhosts = ghosts.filter((g) => sides.get(g) === "out");
  const layered = bandsByLayer(def, members);
  const raw: string[][] = [
    ...(inGhosts.length ? [inGhosts] : []),
    ...layered.map((b) => b.ids),
    ...(outGhosts.length ? [outGhosts] : []),
  ];
  // Every link gets its lane, the quiet ones the drawing implies too.
  const stack = withLanes(raw, edges.filter((e) => e.source !== e.target));
  const ordered = orderStack(stack, new Map(), SEARCH_BUDGET).rows;
  const cx = placeStack(ordered, stack.adj, CARD_W, PITCH_X - CARD_W);
  const xs = [...cx.entries()].flatMap(([id, x]) => (isLane(id) ? [x - LANE_GAP, x + LANE_GAP] : [x - CARD_W / 2, x + CARD_W / 2]));
  const x0 = (xs.length ? Math.min(...xs) : -CARD_W / 2) - PAD;
  const full = (xs.length ? Math.max(...xs) : CARD_W / 2) + PAD - x0;
  const flip = (p: { x: number; y: number }) => (transpose ? { x: p.y, y: p.x } : p);

  const centers = new Map<string, { x: number; y: number }>();
  const regions: LayerRegion[] = [];
  let bandIndex = 0;
  const place = (ids: string[], layer: string | null, ghost = false) => {
    for (const id of ids) if (!isLane(id)) centers.set(id, flip({ x: cx.get(id)!, y: bandIndex * PITCH_Y }));
    if (layer !== null) {
      const y0 = bandIndex * PITCH_Y - CARD_H / 2 - PAD - LABEL_ROOM;
      const thick = CARD_H + 2 * PAD + LABEL_ROOM;
      const caption = def.layers.find((l) => l.name === layer)?.description;
      regions.push(
        transpose
          ? { layer, caption, ghost, shape: "rect", x: y0, y: x0, w: thick, h: full }
          : { layer, caption, ghost, shape: "rect", x: x0, y: y0, w: full, h: thick },
      );
    }
    bandIndex++;
  };
  let k = 0;
  if (inGhosts.length) place(ordered[k++], "called from", true);
  for (const b of layered) place(ordered[k++], b.layer === "?" ? "unlayered" : b.layer);
  if (outGhosts.length) place(ordered[k++], "depends on", true);
  const tops = ordered.map((_, i) => i * PITCH_Y - CARD_H / 2);
  const routes = new Map(
    [...laneRoutes(stack, cx, tops, CARD_H)].map(([key, pts]) => [key, pts.map(flip)] as const),
  );
  return { centers, regions, routes };
}

// ── rings ───────────────────────────────────────────────────────────────────

/** Pack ids on a compact grid around the origin (the innermost cluster). */
function packCenter(ids: string[], centers: Map<string, { x: number; y: number }>): number {
  const n = ids.length;
  if (n === 0) return CARD_W / 2;
  const cols = Math.ceil(Math.sqrt(n));
  const rows = Math.ceil(n / cols);
  const pitchX = CARD_W + 40;
  const pitchY = CARD_H + 40;
  ids.forEach((id, i) => {
    const c = i % cols;
    const r = Math.floor(i / cols);
    centers.set(id, { x: (c - (cols - 1) / 2) * pitchX, y: (r - (rows - 1) / 2) * pitchY });
  });
  // Half-diagonal of the cluster's footprint.
  const w = (cols - 1) * pitchX + CARD_W;
  const h = (rows - 1) * pitchY + CARD_H;
  return Math.hypot(w, h) / 2;
}

/** Place ids evenly on a circle of at least `minR`, growing the radius until
 *  neighbouring cards have room. The ring is turned so the cards sit, on
 *  average, at the angles they prefer (`prefer`: toward what they touch);
 *  with no preference it starts at `startAngle`. Returns the radius used. */
function ringPlace(
  ids: string[],
  minR: number,
  startAngle: number,
  centers: Map<string, { x: number; y: number }>,
  prefer: Map<string, number> = new Map(),
): number {
  const n = ids.length;
  const minArc = Math.hypot(CARD_W, CARD_H) + 30;
  const r = Math.max(minR, n <= 1 ? minR : (n * minArc) / (2 * Math.PI));
  const step = (2 * Math.PI) / Math.max(n, 1);
  const wants = ids.flatMap((id, i) => (prefer.has(id) ? [prefer.get(id)! - i * step] : []));
  if (wants.length) {
    // Circular mean of each card's wanted start.
    const sx = wants.reduce((t, a) => t + Math.cos(a), 0);
    const sy = wants.reduce((t, a) => t + Math.sin(a), 0);
    if (Math.hypot(sx, sy) > 1e-9) startAngle = Math.atan2(sy, sx);
  }
  ids.forEach((id, i) => {
    const a = startAngle + (2 * Math.PI * i) / Math.max(n, 1);
    centers.set(id, { x: r * Math.cos(a), y: r * Math.sin(a) });
  });
  return r;
}

/** Turn a placed ring about the origin, by less than one card's step, to
 *  the angle where the fewest `paths` run through its cards; ties keep the
 *  turn smallest. */
function turnClear(
  ids: string[],
  centers: Map<string, { x: number; y: number }>,
  /** The paths, read off `centers` as they stand for the turn tried. */
  paths: () => [{ x: number; y: number }, { x: number; y: number }][],
) {
  if (ids.length === 0) return;
  const at = ids.map((id) => centers.get(id)!);
  const step = (2 * Math.PI) / ids.length;
  const tries = 24;
  const turned = (d: number) =>
    at.map((p) => ({ x: p.x * Math.cos(d) - p.y * Math.sin(d), y: p.x * Math.sin(d) + p.y * Math.cos(d) }));
  const hits = (ps: { x: number; y: number }[]) => {
    ps.forEach((p, i) => centers.set(ids[i], p));
    return paths().reduce((n, [a, b]) => n + ps.filter((c) => crossesCard(a, b, c)).length, 0);
  };
  let best = { d: 0, hits: hits(at), off: 0 };
  for (let k = 1; k < tries; k++) {
    const d = (k * step) / tries;
    const h = hits(turned(d));
    const off = Math.min(d, step - d);
    if (h < best.hits || (h === best.hits && off < best.off)) best = { d, hits: h, off };
  }
  turned(best.d).forEach((p, i) => centers.set(ids[i], p));
}

/** Whether the segment a→b passes through a card centred at `c`, with a
 *  little air around it (Liang–Barsky clip against the card's box). */
function crossesCard(a: { x: number; y: number }, b: { x: number; y: number }, c: { x: number; y: number }): boolean {
  const air = 12;
  const [hw, hh] = [CARD_W / 2 + air, CARD_H / 2 + air];
  const dx = b.x - a.x, dy = b.y - a.y;
  let t0 = 0, t1 = 1;
  for (const [p, q] of [
    [-dx, a.x - (c.x - hw)],
    [dx, c.x + hw - a.x],
    [-dy, a.y - (c.y - hh)],
    [dy, c.y + hh - a.y],
  ]) {
    if (p === 0) {
      if (q < 0) return false;
    } else {
      const t = q / p;
      if (p < 0) t0 = Math.max(t0, t);
      else t1 = Math.min(t1, t);
      if (t0 > t1) return false;
    }
  }
  return true;
}

/** Order a ring's ids by the mean angle of the neighbours already placed, so
 *  a shell card sits over the core it calls. */
function orderByAngle(ids: string[], edges: StyledEdge[], centers: Map<string, { x: number; y: number }>): string[] {
  return byAngle(ids, edges, centers).order;
}

/** Each id's preferred angle — the circular mean of its placed neighbours'
 *  — and the ids in that order (ids with no placed neighbour last). */
function byAngle(
  ids: string[],
  edges: StyledEdge[],
  centers: Map<string, { x: number; y: number }>,
): { order: string[]; prefer: Map<string, number> } {
  const prefer = new Map<string, number>();
  const key = new Map<string, number>();
  ids.forEach((id, i) => {
    const ns: number[] = [];
    for (const e of edges) {
      const other = e.source === id ? e.target : e.target === id ? e.source : null;
      if (!other) continue;
      const p = centers.get(other);
      if (p && (p.x !== 0 || p.y !== 0)) ns.push(Math.atan2(p.y, p.x));
    }
    if (ns.length === 0) key.set(id, 1000 + i);
    else {
      // Circular mean.
      const sx = ns.reduce((s, a) => s + Math.cos(a), 0);
      const sy = ns.reduce((s, a) => s + Math.sin(a), 0);
      key.set(id, Math.atan2(sy, sx));
      prefer.set(id, Math.atan2(sy, sx));
    }
  });
  return { order: [...ids].sort((a, b) => key.get(a)! - key.get(b)!), prefer };
}

function ringsLayout(def: StyleDef, members: StyledMember[], ghosts: string[], edges: StyledEdge[]): StyledLayout {
  const bands = bandsByLayer(def, members); // outer → inner
  const centers = new Map<string, { x: number; y: number }>();
  const regions: LayerRegion[] = [];
  // Outside cards go in a band above (callers) or below (what it uses), as
  // on the rows drawing. Stand them far off on their side while the rings
  // are laid, so each ring turns its cards toward them.
  const memberIds = new Set(members.map((m) => m.id));
  const sides = ghostSides(ghosts, memberIds, edges);
  const inG = ghosts.filter((g) => sides.get(g) === "in");
  const outG = ghosts.filter((g) => sides.get(g) === "out");
  for (const g of inG) centers.set(g, { x: 0, y: -1e5 });
  for (const g of outG) centers.set(g, { x: 0, y: 1e5 });
  const inner = bands[bands.length - 1];
  let r = 0;
  if (inner) {
    r = packCenter(inner.ids, centers) + PAD;
    regions.push({ layer: inner.layer === "?" ? "unlayered" : inner.layer, caption: describe(def, inner.layer), shape: "ring", cx: 0, cy: 0, r });
  }
  for (let i = bands.length - 2; i >= 0; i--) {
    const b = bands[i];
    const { order, prefer } = byAngle(b.ids, edges, centers);
    const used = ringPlace(order, r + PAD + CARD_H / 2 + 10, -Math.PI / 2, centers, prefer);
    r = used + CARD_H / 2 + PAD;
    regions.push({ layer: b.layer === "?" ? "unlayered" : b.layer, caption: describe(def, b.layer), shape: "ring", cx: 0, cy: 0, r });
  }
  // Innermost region first so the renderer paints outer rings underneath.
  regions.reverse();
  const row = (ids: string[], y: number, layer: string) => {
    if (ids.length === 0) return;
    // Each over the mean x of the cards it touches.
    const meanX = new Map<string, number>();
    ids.forEach((id, i) => {
      const xs: number[] = [];
      for (const e of edges) {
        const other = e.source === id ? e.target : e.target === id ? e.source : null;
        const p = other && memberIds.has(other) ? centers.get(other) : undefined;
        if (p) xs.push(p.x);
      }
      meanX.set(id, xs.length ? xs.reduce((t, x) => t + x, 0) / xs.length : 1e6 + i);
    });
    const ordered = [...ids].sort((a, b) => meanX.get(a)! - meanX.get(b)!);
    const w = (ordered.length - 1) * PITCH_X;
    ordered.forEach((id, i) => centers.set(id, { x: -w / 2 + i * PITCH_X, y }));
    regions.push({
      layer,
      ghost: true,
      shape: "rect",
      x: -w / 2 - CARD_W / 2 - PAD,
      y: y - CARD_H / 2 - PAD - LABEL_ROOM,
      w: w + CARD_W + 2 * PAD,
      h: CARD_H + 2 * PAD + LABEL_ROOM,
    });
  };
  const gap = PAD * 2 + LABEL_ROOM + CARD_H / 2;
  row(inG, -r - gap, "called from");
  row(outG, r + gap, "depends on");
  return { centers, regions };
}

// ── hexagon ─────────────────────────────────────────────────────────────────

/** The container's dev tooling (examples, benches) in one row under a styled
 *  drawing: part of the codebase, held to no layer, so apart from them. */
export function toolingStrip(
  laid: StyledLayout,
  ids: string[],
): { centers: Map<string, { x: number; y: number }>; region?: LayerRegion } {
  return stripBelow(laid, ids, {
    layer: "tooling",
    caption: "Dev tooling: part of the codebase, not of the product, and held to no style. Its links are drawn faint.",
  });
}

function stripBelow(
  laid: StyledLayout,
  ids: string[],
  meta: { layer: string; caption: string },
): { centers: Map<string, { x: number; y: number }>; region?: LayerRegion } {
  const centers = new Map<string, { x: number; y: number }>();
  if (ids.length === 0) return { centers };
  let minX = Infinity, maxX = -Infinity, bottom = -Infinity;
  for (const c of laid.centers.values()) {
    minX = Math.min(minX, c.x - CARD_W / 2);
    maxX = Math.max(maxX, c.x + CARD_W / 2);
    bottom = Math.max(bottom, c.y + CARD_H / 2);
  }
  for (const r of laid.regions) {
    if (r.shape === "rect") {
      minX = Math.min(minX, r.x);
      maxX = Math.max(maxX, r.x + r.w);
      bottom = Math.max(bottom, r.y + r.h);
    } else {
      minX = Math.min(minX, r.cx - r.r);
      maxX = Math.max(maxX, r.cx + r.r);
      bottom = Math.max(bottom, r.cy + r.r);
    }
  }
  if (!Number.isFinite(bottom)) [minX, maxX, bottom] = [0, 0, 0];
  const pitch = CARD_W + 40;
  const pad = 24;
  const top = bottom + 60;
  const width = ids.length * pitch - 40;
  const left = (minX + maxX) / 2 - width / 2;
  ids.forEach((id, i) => centers.set(id, { x: left + i * pitch + CARD_W / 2, y: top + pad + CARD_H / 2 }));
  return {
    centers,
    region: {
      shape: "rect",
      ...meta,
      x: left - pad,
      y: top,
      w: width + 2 * pad,
      h: CARD_H + 2 * pad,
    },
  };
}

/** Positions as close to `want` (ascending) as a minimum `gap` allows, in
 *  order: runs of cards that would overlap share one offset, the mean of what
 *  each wants. A card with nothing to sit level with (`want` past 1e5) goes
 *  below the rest. */
function seat(want: number[], gap: number): number[] {
  const known = want.filter((y) => y < 1e5);
  let last = known.length ? Math.max(...known) : 0;
  const ys = want.map((y) => (y < 1e5 ? y : (last += gap)));
  const blocks: { start: number; n: number; pos: number }[] = [];
  ys.forEach((y, i) => {
    blocks.push({ start: i, n: 1, pos: y });
    for (;;) {
      const b = blocks[blocks.length - 1], a = blocks[blocks.length - 2];
      if (!a || a.pos + a.n * gap <= b.pos) break;
      // b's cards want b.pos + k·gap; seated after a's, they want
      // (b.pos − a.n·gap) as the merged block's start.
      const pos = (a.pos * a.n + (b.pos - a.n * gap) * b.n) / (a.n + b.n);
      blocks.splice(-2, 2, { start: a.start, n: a.n + b.n, pos });
    }
  });
  const out: number[] = [];
  for (const b of blocks) for (let k = 0; k < b.n; k++) out.push(b.pos + k * gap);
  return out;
}

/**
 * Cockburn's drawing, read off the style's layer order. The last four layers
 * are the hexagon: the driving side (presentation) as a column on the LEFT,
 * the driven side (infrastructure) as a column on the RIGHT, application on
 * the hexagon ring, domain in the centre. A fifth, first layer (composition,
 * the wiring) is a band ABOVE the whole drawing. Four layers (a project style
 * without composition) draw without the band; any other count falls back to
 * rings.
 */
function hexagonLayout(def: StyleDef, members: StyledMember[], ghosts: string[], edges: StyledEdge[]): StyledLayout {
  const names = def.layers.map((l) => l.name);
  if (names.length !== 4 && names.length !== 5) return ringsLayout(def, members, ghosts, edges);
  const wiring = names.length === 5 ? names[0] : undefined;
  const [driving, driven, ring, core] = names.slice(names.length - 4);
  const ids = (layer: string) => members.filter((m) => m.layer === layer).map((m) => m.id);
  const unknown = members.filter((m) => !m.layer || !def.layers.some((l) => l.name === m.layer)).map((m) => m.id);
  const centers = new Map<string, { x: number; y: number }>();
  const regions: LayerRegion[] = [];

  const memberIds = new Set(members.map((m) => m.id));
  const sides = ghostSides(ghosts, memberIds, edges);

  // Centre + ring. An empty ring stays a thin band, so the hexagon still
  // reads as one without a moat of nothing around the centre.
  const coreIds = ids(core);
  let r = packCenter(coreIds, centers) + PAD;
  regions.push({ layer: core, caption: describe(def, core), shape: "hex", cx: 0, cy: 0, r });
  const ringIds = orderByAngle(ids(ring), edges, centers);
  if (ringIds.length) {
    const ringR = ringPlace(ringIds, r + PAD + CARD_H / 2 + 10, -Math.PI / 2, centers);
    r = ringR + CARD_H / 2 + PAD;
    turnClear(ringIds, centers, outsidePaths);
  } else {
    r += PAD * 2;
  }
  regions.push({ layer: ring, caption: describe(def, ring), shape: "hex", cx: 0, cy: 0, r });

  /** The straight paths from each centre card to what it links to outside
   *  the hexagon: composition where its band will seat it, over the mean x
   *  of what it touches, and the rest far off on the side they will stand. */
  function outsidePaths(): [{ x: number; y: number }, { x: number; y: number }][] {
    const far = 1e4;
    const layerOf = new Map(members.map((m) => [m.id, m.layer]));
    const bandY = -r - PAD * 2 - LABEL_ROOM - CARD_H / 2;
    const meanX = (id: string) => {
      const xs = edges.flatMap((e) => {
        const o = e.source === id ? e.target : e.target === id ? e.source : null;
        const p = o ? centers.get(o) : undefined;
        return p ? [p.x] : [];
      });
      return xs.length ? xs.reduce((t, x) => t + x, 0) / xs.length : 0;
    };
    const aim = (id: string): { x: number; y: number } | null => {
      const l = layerOf.get(id);
      if (l === wiring) return { x: meanX(id), y: bandY };
      if (l === driving || sides.get(id) === "in") return { x: -far, y: 0 };
      if (l === driven || sides.get(id) === "out") return { x: far, y: 0 };
      if (memberIds.has(id) && l !== core && l !== ring) return { x: 0, y: far };
      return null;
    };
    const paths: [{ x: number; y: number }, { x: number; y: number }][] = [];
    for (const e of edges) {
      for (const [inner, other] of [[e.source, e.target], [e.target, e.source]]) {
        const p = coreIds.includes(inner) ? centers.get(inner) : undefined;
        const q = p && aim(other);
        if (p && q) paths.push([p, q]);
      }
    }
    return paths;
  }

  // Side columns, ordered to sit level with the ring cards they touch.
  const column = (colIds: string[], x: number, layer: string | null, ghost = false) => {
    if (colIds.length === 0) return;
    const key = new Map<string, number>();
    colIds.forEach((id, i) => {
      const ys: number[] = [];
      for (const e of edges) {
        const other = e.source === id ? e.target : e.target === id ? e.source : null;
        const p = other ? centers.get(other) : undefined;
        if (p) ys.push(p.y);
      }
      key.set(id, ys.length ? ys.reduce((s, y) => s + y, 0) / ys.length : 1e6 + i);
    });
    const ordered = [...colIds].sort((a, b) => key.get(a)! - key.get(b)!);
    // An outside column is seated, each card level with what it links to;
    // a side column is packed around the hexagon's middle.
    const ys = ghost ? seat(ordered.map((id) => key.get(id)!), CARD_H + 40) : ordered.map((_, i) => i * (CARD_H + 40));
    const mid = ghost ? 0 : (ys[ys.length - 1] - ys[0]) / 2;
    ordered.forEach((id, i) => centers.set(id, { x, y: ys[i] - mid }));
    const [top, bottom] = [ys[0] - mid, ys[ys.length - 1] - mid];
    if (layer !== null) {
      regions.push({
        layer,
        caption: ghost ? undefined : describe(def, layer),
        ghost,
        shape: "rect",
        x: x - CARD_W / 2 - PAD,
        y: top - CARD_H / 2 - PAD - LABEL_ROOM,
        w: CARD_W + 2 * PAD,
        h: bottom - top + CARD_H + 2 * PAD + LABEL_ROOM,
      });
    }
  };
  const sideX = r + PAD + CARD_W / 2 + 20;
  column(ids(driving), -sideX, driving);
  column(ids(driven), sideX, driven);

  // Composition: one band above everything it wires, each card over the mean
  // x of what it touches.
  const wiringIds = wiring ? ids(wiring) : [];
  if (wiring && wiringIds.length) {
    const top = Math.min(-r, ...[...centers.values()].map((p) => p.y - CARD_H / 2));
    const y = top - PAD * 2 - LABEL_ROOM - CARD_H / 2;
    const meanX = new Map<string, number>();
    wiringIds.forEach((id, i) => {
      const xs: number[] = [];
      for (const e of edges) {
        const other = e.source === id ? e.target : e.target === id ? e.source : null;
        const p = other ? centers.get(other) : undefined;
        if (p) xs.push(p.x);
      }
      meanX.set(id, xs.length ? xs.reduce((s, x) => s + x, 0) / xs.length : 1e6 + i);
    });
    const ordered = [...wiringIds].sort((a, b) => meanX.get(a)! - meanX.get(b)!);
    const w = (ordered.length - 1) * PITCH_X;
    ordered.forEach((id, i) => centers.set(id, { x: -w / 2 + i * PITCH_X, y }));
    regions.push({
      layer: wiring,
      caption: describe(def, wiring),
      shape: "rect",
      x: -w / 2 - CARD_W / 2 - PAD,
      y: y - CARD_H / 2 - PAD - LABEL_ROOM,
      w: w + CARD_W + 2 * PAD,
      h: CARD_H + 2 * PAD + LABEL_ROOM,
    });
  }
  column(unknown, 0, unknown.length ? "unlayered" : null);
  if (unknown.length) {
    // Unlayered cards go below the hexagon rather than on top of the core.
    for (const id of unknown) {
      const p = centers.get(id)!;
      centers.set(id, { x: p.x, y: p.y + r + CARD_H + PAD * 2 });
    }
    const reg = regions[regions.length - 1];
    if (reg.shape === "rect") reg.y += r + CARD_H + PAD * 2;
  }

  // Ghosts: callers on the far left, dependencies on the far right.
  // A side with no cards of its own leaves its place to the outside column,
  // rather than a gap between the hexagon and what talks to it. Either way
  // the column clears a composition band wider than the hexagon, since an
  // outside card may be seated level with it.
  const bandHalf = wiringIds.length ? ((wiringIds.length - 1) * PITCH_X) / 2 + CARD_W / 2 + PAD : 0;
  const ghostX = (side: string) =>
    Math.max(ids(side).length ? sideX + CARD_W + PAD * 2 : sideX, bandHalf + PAD * 2 + CARD_W / 2);
  const inG = ghosts.filter((g) => sides.get(g) === "in");
  const outG = ghosts.filter((g) => sides.get(g) === "out");
  column(inG, -ghostX(driving), inG.length ? "driven by" : null, true);
  column(outG, ghostX(driven), outG.length ? "drives" : null, true);

  return { centers, regions };
}

function describe(def: StyleDef, layer: string): string | undefined {
  return def.layers.find((l) => l.name === layer)?.description;
}

// ── entry ───────────────────────────────────────────────────────────────────

export function styledLayout(
  def: StyleDef,
  members: StyledMember[],
  ghosts: string[],
  edges: StyledEdge[],
): StyledLayout {
  const drawing: Drawing = def.drawing;
  switch (drawing) {
    case "rows":
      return bandsLayout(def, members, ghosts, edges, false);
    case "columns":
      return bandsLayout(def, members, ghosts, edges, true);
    case "rings":
      return ringsLayout(def, members, ghosts, edges);
    case "hexagon":
      return hexagonLayout(def, members, ghosts, edges);
  }
}

/** How an edge between two members reads against the style:
 *  - `implied`: a legal cross-layer dependency — the drawing already says it
 *    (inner layers sit where legal imports point), so the renderer hides it
 *    until one end is selected;
 *  - `violation`: a cross-layer dependency the matrix forbids — drawn, and
 *    drawn red, because it is the one thing the map exists to expose;
 *  - `plain`: a same-layer link (a `uses` between siblings) — drawn as usual.
 */
export function classifyStyledEdge(
  def: StyleDef,
  sourceLayer: string | undefined,
  targetLayer: string | undefined,
  /** Which end lives outside this container (a ghost). Traffic from outside
   *  must enter on an inbound layer; traffic to the outside must leave from
   *  an outbound (driven) layer. Both are implied when legal. `otherDef` /
   *  `otherLayer` describe the ghost's own container, for the module rule
   *  below. */
  ends: {
    sourceGhost?: boolean;
    targetGhost?: boolean;
    otherDef?: StyleDef;
    otherLayer?: string;
  } = {},
): "implied" | "violation" | "plain" {
  if (ends.sourceGhost && ends.targetGhost) return "plain";
  if (ends.sourceGhost) {
    if (!targetLayer) return "plain";
    return def.inbound.length === 0 || def.inbound.includes(targetLayer) ? "implied" : "violation";
  }
  if (ends.targetGhost) {
    if (!sourceLayer) return "plain";
    // A library is imported as a module, not talked to as a system: our own
    // matrix decides, exactly as it would for a module of this container.
    const other = ends.otherLayer;
    if (ends.otherDef?.importedAsModule && other && def.layers.some((l) => l.name === other)) {
      // Matrix first; a library's facade is reached from the driven side.
      const byMatrix = (def.matrix[sourceLayer] ?? []).includes(other);
      const byFacade = (def.outbound ?? []).includes(sourceLayer);
      return byMatrix || byFacade ? "implied" : "violation";
    }
    const out = def.outbound ?? [];
    return out.length === 0 || out.includes(sourceLayer) ? "implied" : "violation";
  }
  if (!sourceLayer || !targetLayer) return "plain";
  if (sourceLayer === targetLayer) {
    // Siblings on a layer: normal traffic where the style allows it (the
    // position says nothing about it, but drawing every one is a mesh), a
    // violation where the style isolates slices.
    return def.isolation === "strict" ? "violation" : "implied";
  }
  const allowed = def.matrix[sourceLayer] ?? [];
  return allowed.includes(targetLayer) ? "implied" : "violation";
}

/** @deprecated use {@link classifyStyledEdge} */
export function isImpliedEdge(
  def: StyleDef,
  sourceLayer: string | undefined,
  targetLayer: string | undefined,
  _kind: string | undefined,
): boolean {
  return classifyStyledEdge(def, sourceLayer, targetLayer) === "implied";
}
