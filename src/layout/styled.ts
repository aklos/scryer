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
 *   - `hexagon` (hexagonal):      Cockburn's hexagon — domain centre,
 *                                 application ring, driving side left,
 *                                 driven side right
 *
 * Placement is deterministic: the layer fixes the band / ring / column, and
 * the only freedom is the order within it, chosen to shorten links. Ghosts
 * (nodes referenced from this level but living elsewhere) sit outside the
 * drawing on the side their traffic comes from.
 *
 * Pure geometry: takes members with layers and the edges among them, returns
 * card centers plus the regions the renderer draws behind the cards.
 */

import type { Drawing, StyleDef } from "../styles";

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
export type LayerRegion =
  | { layer: string; shape: "rect"; x: number; y: number; w: number; h: number }
  | { layer: string; shape: "ring"; cx: number; cy: number; r: number }
  | { layer: string; shape: "hex"; cx: number; cy: number; r: number };

export interface StyledLayout {
  /** Card CENTERS (the caller converts to React Flow's top-left). */
  centers: Map<string, { x: number; y: number }>;
  regions: LayerRegion[];
}

export const CARD_W = 180;
export const CARD_H = 160;
/** Horizontal pitch between cards in a band. */
const PITCH_X = 240;
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
function orderBands(bands: string[][], edges: StyledEdge[]): string[][] {
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
  const ordered = orderBands(raw, edges);
  const widest = Math.max(1, ...ordered.map((b) => b.length));
  const span = (widest - 1) * PITCH_X;

  const centers = new Map<string, { x: number; y: number }>();
  const regions: LayerRegion[] = [];
  let bandIndex = 0;
  const place = (ids: string[], layer: string | null) => {
    const w = (ids.length - 1) * PITCH_X;
    const start = (span - w) / 2;
    ids.forEach((id, i) => {
      const along = start + i * PITCH_X;
      const across = bandIndex * PITCH_Y;
      centers.set(id, transpose ? { x: across, y: along } : { x: along, y: across });
    });
    if (layer !== null) {
      const x0 = -CARD_W / 2 - PAD;
      const full = span + CARD_W + 2 * PAD;
      const y0 = bandIndex * PITCH_Y - CARD_H / 2 - PAD - LABEL_ROOM;
      const thick = CARD_H + 2 * PAD + LABEL_ROOM;
      regions.push(
        transpose
          ? { layer, shape: "rect", x: y0, y: x0, w: thick, h: full }
          : { layer, shape: "rect", x: x0, y: y0, w: full, h: thick },
      );
    }
    bandIndex++;
  };
  let k = 0;
  if (inGhosts.length) place(ordered[k++], null);
  for (const b of layered) place(ordered[k++], b.layer === "?" ? "unlayered" : b.layer);
  if (outGhosts.length) place(ordered[k++], null);
  return { centers, regions };
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
 *  neighbouring cards have room. Returns the radius used. */
function ringPlace(
  ids: string[],
  minR: number,
  startAngle: number,
  centers: Map<string, { x: number; y: number }>,
): number {
  const n = ids.length;
  const minArc = Math.hypot(CARD_W, CARD_H) + 30;
  const r = Math.max(minR, n <= 1 ? minR : (n * minArc) / (2 * Math.PI));
  ids.forEach((id, i) => {
    const a = startAngle + (2 * Math.PI * i) / Math.max(n, 1);
    centers.set(id, { x: r * Math.cos(a), y: r * Math.sin(a) });
  });
  return r;
}

/** Order a ring's ids by the mean angle of the neighbours already placed, so
 *  a shell card sits over the core it calls. */
function orderByAngle(ids: string[], edges: StyledEdge[], centers: Map<string, { x: number; y: number }>): string[] {
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
    }
  });
  return [...ids].sort((a, b) => key.get(a)! - key.get(b)!);
}

function ringsLayout(def: StyleDef, members: StyledMember[], ghosts: string[], edges: StyledEdge[]): StyledLayout {
  const bands = bandsByLayer(def, members); // outer → inner
  const centers = new Map<string, { x: number; y: number }>();
  const regions: LayerRegion[] = [];
  const inner = bands[bands.length - 1];
  let r = 0;
  if (inner) {
    r = packCenter(inner.ids, centers) + PAD;
    regions.push({ layer: inner.layer === "?" ? "unlayered" : inner.layer, shape: "ring", cx: 0, cy: 0, r });
  }
  for (let i = bands.length - 2; i >= 0; i--) {
    const b = bands[i];
    const ordered = orderByAngle(b.ids, edges, centers);
    const used = ringPlace(ordered, r + PAD + CARD_H / 2 + 10, -Math.PI / 2, centers);
    r = used + CARD_H / 2 + PAD;
    regions.push({ layer: b.layer === "?" ? "unlayered" : b.layer, shape: "ring", cx: 0, cy: 0, r });
  }
  if (ghosts.length) {
    const ordered = orderByAngle(ghosts, edges, centers);
    ringPlace(ordered, r + PAD + CARD_H / 2 + 20, -Math.PI / 2 + 0.3, centers);
  }
  // Innermost region first so the renderer paints outer rings underneath.
  regions.reverse();
  return { centers, regions };
}

// ── hexagon ─────────────────────────────────────────────────────────────────

/**
 * Cockburn's drawing, read off the style's layer order: layers[0] (the
 * driving side, presentation) as a column on the LEFT, layers[1] (the driven
 * side, infrastructure) as a column on the RIGHT, layers[2] (application) on
 * the hexagon ring, layers[3] (domain) in the centre. A style with another
 * layer count falls back to rings.
 */
function hexagonLayout(def: StyleDef, members: StyledMember[], ghosts: string[], edges: StyledEdge[]): StyledLayout {
  if (def.layers.length !== 4) return ringsLayout(def, members, ghosts, edges);
  const [driving, driven, ring, core] = def.layers.map((l) => l.name);
  const ids = (layer: string) => members.filter((m) => m.layer === layer).map((m) => m.id);
  const unknown = members.filter((m) => !m.layer || !def.layers.some((l) => l.name === m.layer)).map((m) => m.id);
  const centers = new Map<string, { x: number; y: number }>();
  const regions: LayerRegion[] = [];

  // Centre + ring.
  const coreIds = ids(core);
  let r = packCenter(coreIds, centers) + PAD;
  regions.push({ layer: core, shape: "hex", cx: 0, cy: 0, r });
  const ringIds = orderByAngle(ids(ring), edges, centers);
  const ringR = ringPlace(ringIds, r + PAD + CARD_H / 2 + 10, -Math.PI / 2, centers);
  r = ringR + CARD_H / 2 + PAD;
  regions.push({ layer: ring, shape: "hex", cx: 0, cy: 0, r });

  // Side columns, ordered to sit level with the ring cards they touch.
  const column = (colIds: string[], x: number, layer: string | null) => {
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
    const h = (ordered.length - 1) * (CARD_H + 40);
    ordered.forEach((id, i) => centers.set(id, { x, y: -h / 2 + i * (CARD_H + 40) }));
    if (layer !== null) {
      regions.push({
        layer,
        shape: "rect",
        x: x - CARD_W / 2 - PAD,
        y: -h / 2 - CARD_H / 2 - PAD - LABEL_ROOM,
        w: CARD_W + 2 * PAD,
        h: h + CARD_H + 2 * PAD + LABEL_ROOM,
      });
    }
  };
  const sideX = r + PAD + CARD_W / 2 + 20;
  column(ids(driving), -sideX, driving);
  column(ids(driven), sideX, driven);
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
  const memberIds = new Set(members.map((m) => m.id));
  const sides = ghostSides(ghosts, memberIds, edges);
  const ghostX = sideX + CARD_W + PAD * 2;
  column(ghosts.filter((g) => sides.get(g) === "in"), -ghostX, null);
  column(ghosts.filter((g) => sides.get(g) === "out"), ghostX, null);

  return { centers, regions };
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

/** Is an edge between two members "implied" by the drawing — its position
 *  already says it, so the renderer hides it until one end is selected?
 *  Anything into the innermost layer, an adjacent-layer step inward on a
 *  banded drawing, and an adapter onto its port (`implements`). Same-layer
 *  links and anything the matrix forbids are never implied — those are the
 *  lines worth seeing. */
export function isImpliedEdge(
  def: StyleDef,
  sourceLayer: string | undefined,
  targetLayer: string | undefined,
  kind: string | undefined,
): boolean {
  if (!sourceLayer || !targetLayer || sourceLayer === targetLayer) return false;
  const allowed = def.matrix[sourceLayer] ?? [];
  if (!allowed.includes(targetLayer)) return false;
  if (kind === "implements") return true;
  const order = def.layers.map((l) => l.name);
  const si = order.indexOf(sourceLayer);
  const ti = order.indexOf(targetLayer);
  if (ti === order.length - 1) return true;
  if ((def.drawing === "rows" || def.drawing === "columns") && ti === si + 1) return true;
  return false;
}
