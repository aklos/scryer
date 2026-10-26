/**
 * Code-level layout — symbols by call depth from the component's surface.
 *
 * The free constellation made every symbol equal. This reads top-down:
 * the top row is what the component EXPOSES (symbols reached from outside
 * it, or called by nothing inside it), each row below is what the row above
 * calls, and the bottom is the helpers everything rests on. Callers from
 * outside sit in a band above; dependencies outside sit in a band below.
 * Size (fan-in) and fill (has claims) are the renderer's; position is ours.
 */

import { orderBands, type LayerRegion, type StyledEdge } from "./styled";

export interface DepthMember {
  id: string;
  /** Rendered dot diameter — the row pitch leaves room for the largest. */
  size: number;
  /** Estimated label width, so a row's pitch fits its widest row. */
  labelW: number;
}

export interface DepthLayout {
  centers: Map<string, { x: number; y: number }>;
  regions: LayerRegion[];
}

const ROW_PITCH = 140;
/** A band spans the disc and the two label lines beneath it. */
const BAND_TOP = 44;
const BAND_H = 118;
const MIN_PITCH_X = 150;
const BAND_PAD = 26;
/** Pitch between the lines of one wrapped band — tighter than between
 *  bands, so a band's lines read as one. */
const LINE_PITCH = 112;
/** A wrapped band never has fewer per line than this. */
const MIN_PER_LINE = 6;
/** A row of the component's own symbols sets the level's width up to this
 *  many; past it, it wraps too. */
const MAX_MEMBER_LINE = 12;
/** How much wider than tall a wrapped band's block is laid (lines ≈
 *  per-line / this). */
const LINE_ASPECT = 2;

/** BFS depth from the entry set over the internal edges (source → target).
 *  Unreachable symbols (cycles with no entry, or isolated) land one row past
 *  the deepest reached row, so nothing vanishes. */
function depths(memberIds: string[], entries: Set<string>, edges: StyledEdge[]): Map<string, number> {
  const members = new Set(memberIds);
  const out = new Map<string, string[]>();
  for (const e of edges) {
    if (members.has(e.source) && members.has(e.target) && e.source !== e.target) {
      (out.get(e.source) ?? out.set(e.source, []).get(e.source)!).push(e.target);
    }
  }
  const depth = new Map<string, number>();
  const queue: string[] = [];
  for (const id of memberIds) {
    if (entries.has(id)) {
      depth.set(id, 0);
      queue.push(id);
    }
  }
  while (queue.length) {
    const cur = queue.shift()!;
    const d = depth.get(cur)!;
    for (const n of out.get(cur) ?? []) {
      if (!depth.has(n)) {
        depth.set(n, d + 1);
        queue.push(n);
      }
    }
  }
  let deepest = 0;
  for (const d of depth.values()) deepest = Math.max(deepest, d);
  for (const id of memberIds) if (!depth.has(id)) depth.set(id, deepest + (depth.size ? 1 : 0));
  return depth;
}

export function depthLayout(members: DepthMember[], ghosts: string[], edges: StyledEdge[]): DepthLayout {
  const memberIds = members.map((m) => m.id);
  const memberSet = new Set(memberIds);
  const ghostSet = new Set(ghosts);
  // Entry = called from outside the component, or called by nothing inside it.
  const calledFromInside = new Set<string>();
  const calledFromOutside = new Set<string>();
  const callers: string[] = [];
  const deps: string[] = [];
  for (const e of edges) {
    if (memberSet.has(e.target) && memberSet.has(e.source)) calledFromInside.add(e.target);
    if (memberSet.has(e.target) && ghostSet.has(e.source)) {
      calledFromOutside.add(e.target);
      if (!callers.includes(e.source)) callers.push(e.source);
    }
    if (memberSet.has(e.source) && ghostSet.has(e.target) && !deps.includes(e.target)) deps.push(e.target);
  }
  // A ghost both calling in and being called reads as a caller (it drives us).
  const depOnly = deps.filter((g) => !callers.includes(g));
  const entries = new Set(memberIds.filter((id) => calledFromOutside.has(id) || !calledFromInside.has(id)));
  const depth = depths(memberIds, entries.size ? entries : new Set(memberIds), edges);

  let rows = 0;
  for (const d of depth.values()) rows = Math.max(rows, d + 1);
  const rowIds: string[][] = Array.from({ length: rows }, () => []);
  for (const id of memberIds) rowIds[depth.get(id)!].push(id);
  const bands: string[][] = [
    ...(callers.length ? [callers] : []),
    ...rowIds,
    ...(depOnly.length ? [depOnly] : []),
  ];
  const ordered = orderBands(bands, edges);

  const sizeOf = new Map(members.map((m) => [m.id, m]));
  const pitchFor = (ids: string[]) =>
    Math.max(MIN_PITCH_X, ...ids.map((id) => (sizeOf.get(id)?.labelW ?? 120) + 30));
  // A band wider than the level wraps into lines, so a few symbols with
  // thirty dependencies don't stretch the level into a strip too thin to
  // read. The level is as wide as its widest row of its own symbols, and
  // never narrower than a near-square block of its largest band.
  const largest = Math.max(0, ...ordered.map((b) => b.length));
  const perLine = Math.max(
    MIN_PER_LINE,
    Math.ceil(Math.sqrt(largest * LINE_ASPECT)),
    ...ordered.slice(callers.length ? 1 : 0, callers.length ? 1 + rowIds.length : rowIds.length).map((b) =>
      Math.min(b.length, MAX_MEMBER_LINE),
    ),
  );
  const linesOf = (ids: string[]) =>
    Array.from({ length: Math.ceil(ids.length / perLine) }, (_, i) => ids.slice(i * perLine, (i + 1) * perLine));
  // Every other line steps half a pitch over, so an edge to a far line
  // runs between the dots of the nearer one.
  const widthOf = (ids: string[]) => {
    const lines = linesOf(ids);
    return (Math.min(ids.length, perLine) - 1 + (lines.length > 1 ? 0.5 : 0)) * pitchFor(ids);
  };
  const span = Math.max(0, ...ordered.map(widthOf));

  const centers = new Map<string, { x: number; y: number }>();
  const regions: LayerRegion[] = [];
  let y = 0;
  const place = (ids: string[], caption: string | null, ghost: boolean) => {
    const pitch = pitchFor(ids);
    const lines = linesOf(ids);
    const top = y;
    lines.forEach((line, li) => {
      const shift = li % 2 ? pitch / 2 : 0;
      const w = (line.length - 1) * pitch + (lines.length > 1 ? pitch / 2 : 0);
      const start = (span - w) / 2 + shift;
      line.forEach((id, i) => centers.set(id, { x: start + i * pitch, y }));
      y += li < lines.length - 1 ? LINE_PITCH : 0;
    });
    if (caption !== null) {
      regions.push({
        layer: caption,
        shape: "rect",
        x: -MIN_PITCH_X / 2 - BAND_PAD,
        y: top - BAND_TOP,
        w: span + MIN_PITCH_X + 2 * BAND_PAD,
        h: BAND_H + (y - top),
        ghost,
      });
    }
    y += ROW_PITCH;
  };
  let k = 0;
  if (callers.length) place(ordered[k++], "called from", true);
  rowIds.forEach((_, i) => place(ordered[k++], i === 0 ? "surface" : null, false));
  if (depOnly.length) place(ordered[k++], "depends on", true);
  return { centers, regions };
}
