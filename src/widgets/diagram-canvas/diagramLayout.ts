/**
 * Diagram layout adapter — turns the v0.3 model into a positioned, renderable
 * scene for the secondary Diagram view. All layout math lives here so the
 * renderer (DiagramView) stays a pure projection of a `DiagramScene`.
 *
 * One "scene" is one level: the children of a focus node (or the top-level
 * nodes when focus is null), the links lifted to that level, and a position
 * per node. The mode is chosen by the children's altitude:
 *   - architecture tiers (system/container) → the C4 arrangement
 *     (`c4Layout`): users above, the level inside its boundary, what it
 *     uses below.
 *   - the component level of a styled container → the style's own drawing
 *     (`styledLayout`): rows, columns, rings or Cockburn's hexagon, with each
 *     card fixed to its layer's band and only the order within it free.
 *   - the code tier (symbols) → a force-directed dot graph, where boxes-and-
 *     lines would be noise and relationships read better as a constellation.
 *
 * A level with components is drawn at their altitude, as the tree reads it:
 * the root module's own symbols beside them (a crate's `main`, a constant in
 * `lib.rs`) are code, listed under the node in the tree, never drawn as cards.
 */

import { c4Layout } from "./c4Layout";
import type { ScryModel, Kind } from "../../entities/model/viewmodel";
import { isDataShape } from "../../entities/model/viewmodel";
import { styleTable, type ModelHealthReport } from "../../entities/model/health";
import { governingStyleDef, layerOf, type Drawing, type StyleDef } from "../../entities/model/styles";
import { classifyStyledEdge, toolingStrip, styledLayout, type LayerRegion } from "./styled";
import { depthLayout } from "./depth";

export type DiagramMode = "arch" | "styled" | "code";

/** How a code-tier symbol reads at a glance — the three things a symbol can be
 *  (mirrors `kindIcon`/`typeTag`): a data shape (`model`), a component the
 *  preview sidecar can render (`visual`, derived — never stored on the node),
 *  or plain `code`. Picks the symbol's glyph. */
export type SymbolClass = "code" | "model" | "visual";

export interface DiagramNode {
  id: string;
  kind: Kind;
  name: string;
  external: boolean;
  /** Manually placed: the position below is the user's stored placement, not
   *  auto-layout's. Pinned nodes stay put; layout only places the rest. */
  pinned: boolean;
  /** Drawn in the column beside the level's rows (an external system it
   *  calls): its links run sideways, not down the rows. */
  aside?: boolean;
  technology?: string;
  description?: string;
  /** Has its own children — so the renderer can mark it drillable. */
  hasChildren: boolean;
  /** Its direct children — what a drill into it shows; the card's foot
   *  counts them by state. */
  childIds: string[];
  /** Fan-in at this level (how many symbols depend on it) — encodes centrality. */
  degree: number;
  /** Rendered glyph size (px) on the code tier: one size for every symbol. */
  dotSize: number;
  /** A ghost: a node referenced from this level but living outside it. Rendered
   *  hollow; double-click navigates to where it actually lives. */
  reference: boolean;
  /** Code-tier classification — drives the muted class line under the dot. */
  symbolClass: SymbolClass;
  /** The architectural style this node declares (containers; a component
   *  overriding its container's). Drives the miniature glyph on the card. */
  style?: string;
  /** The layer this node plays in its governing style (components; symbols
   *  inherit). Drives the card's layer tag and the styled layout. */
  layer?: string;
  /** Carries claims or a data shape of its own. A symbol without
   *  any reads hollow on the code tier — a hub nobody has described. */
  hasClaims: boolean;
  x: number;
  y: number;
}

export interface DiagramEdge {
  id: string;
  source: string;
  target: string;
  label: string;
  method?: string;
  /** What the link is, when declared (styled containers). */
  kind?: string;
  /** A real dependency that conforms to the level's style (a legal step
   *  between its layers), or a tooling link, held to no style: the drawing
   *  already says it, so it draws quiet. Never a declared one. */
  conforming: boolean;
  /** Declared in structure.toml: what the developer says the relationship
   *  is, never drawn quiet, and in place of what imports imply. */
  declared?: boolean;
  /** Styled mode: both ends sit on one ring or band, so the straight chord
   *  would cut through what lies between — bow it away from `(cx, cy)`, the
   *  curve's middle displaced `offset` px from the chord's midpoint. */
  bow?: { cx: number; cy: number; offset: number };
  /** Box tiers: a link spanning rows runs through these points, down a lane
   *  between the cards it passes. */
  waypoints?: { x: number; y: number }[];
  /** Styled mode: every structural violation charged on this pair — drawn
   *  red, the reasons listed when the user clicks the edge. */
  violations?: string[];
}

/** The word for each box kind, as the boundary label names the level. */
const KIND_WORD: Partial<Record<Kind, string>> = {
  system: "software system",
  container: "container",
  component: "component",
};

/** The drawing behind a styled level: which style, and the region per layer. */
export interface StyledScene {
  name: string;
  drawing: Drawing;
  regions: LayerRegion[];
  /** The style's layers, outermost first. */
  layers: string[];
  /** What the style is for, in its own words. */
  purpose: string;
  /** Each layer and the layers it may import. */
  mayImport: Record<string, string[]>;
}

export interface DiagramScene {
  mode: DiagramMode;
  /** The focus level: null = top-level (systems/persons). */
  focusId: string | null;
  nodes: DiagramNode[];
  edges: DiagramEdge[];
  /** Present in `styled` mode. */
  styled?: StyledScene;
  /** Regions drawn behind the nodes: a style's layers, the code tier's
   *  depth bands, or a box tier's boundary and tooling band. */
  regions?: LayerRegion[];
}

// Card footprint on the arch tiers — used for handle routing before React Flow
// has measured the cards (DiagramView) and by the C4 arrangement.
export const CARD_W = 180;
export const CARD_H = 160;


// One glyph size for every symbol and reference on the code tier: a size
// read off this map's own arrows measured the wrong thing (a symbol the
// whole codebase calls drew tiny, the components it calls drew huge), and
// sizes meant nothing from one map to the next.
const DOT = 32;
const MIN_DOT = DOT;

// Label geometry for the centered-below layout: the glyph sits on top, the
// name beneath it, both horizontally centered. The collision box is the union
// of the glyph and the name.
const DISC_LABEL_GAP = 5; // vertical gap between disc bottom and the name
const NAME_H = 15; // text-xs name line
const LABEL_BLOCK_H = NAME_H; // the name under the glyph
// Estimated rendered label width (px): text-xs ≈ 6.2px/glyph, capped at the
// 130px truncate the dot label uses.
const estLabelWidth = (name: string) =>
  Math.min(130, (name || "·").length * 6.2 + 6);

/** Collision radius (px) of a code-tier dot row — the disc plus its centered
 *  label block, approximated as a circle. Shared with the live simulation's
 *  collide force (`useDotSim`) so physics and pixels agree on how much room a
 *  dot claims. */
export function dotCollideRadius(name: string, dotSize: number): number {
  const w = Math.max(dotSize, estLabelWidth(name));
  const h = dotSize + DISC_LABEL_GAP + LABEL_BLOCK_H;
  return Math.max(w, h) / 2;
}

/**
 * Build the scene for a level.
 */
export async function buildDiagramScene(
  model: ScryModel,
  focusId: string | null,
  report: ModelHealthReport | null,
  previewable: ReadonlySet<string> = new Set(),
  styles: ReadonlyMap<string, StyleDef> = styleTable(report),
): Promise<DiagramScene> {
  const byId = new Map(model.nodes.map((n) => [n.id, n]));
  // The root's own symbols beside components: not part of the drawn level.
  const hasComponents = new Set(model.nodes.filter((n) => n.kind === "component").map((n) => n.parentId ?? null));
  const offLevel = (n: (typeof model.nodes)[number]) => n.kind === "symbol" && hasComponents.has(n.parentId ?? null);
  const childIdsOf = new Map<string, string[]>();
  for (const n of model.nodes)
    if (n.parentId && !offLevel(n)) (childIdsOf.get(n.parentId) ?? childIdsOf.set(n.parentId, []).get(n.parentId)!).push(n.id);

  const children = model.nodes.filter((n) => (n.parentId ?? null) === focusId && !offLevel(n));
  const childIds = new Set(children.map((c) => c.id));

  // Lift a link endpoint to the visible child that contains it: walk up the
  // parent chain until the parent is the focus level. Returns null when the
  // endpoint lives outside this level (so the link doesn't belong here), or
  // in a root symbol the level doesn't draw.
  const liftToLevel = (nodeId: string): string | null => {
    let cur = byId.get(nodeId);
    const seen = new Set<string>();
    while (cur && !seen.has(cur.id)) {
      seen.add(cur.id);
      const parent = cur.parentId ?? null;
      if (parent === focusId) return childIds.has(cur.id) ? cur.id : null;
      if (parent === null) return null;
      cur = byId.get(parent);
    }
    return null;
  };

  // An end outside the focus as this level draws it (C4): the highest node
  // holding it that does not hold the focus. Another container shows as that
  // container, another system as that system, a person as the person. An
  // end inside the focus that the level does not draw, or one that is the
  // focus or holds it, has no place here.
  const focusLine = new Set<string>();
  for (let id = focusId; id !== null && !focusLine.has(id); id = byId.get(id)?.parentId ?? null) focusLine.add(id);
  const outsideAt = (nodeId: string): string | null => {
    let cur = byId.get(nodeId);
    let top: string | null = null;
    const seen = new Set<string>();
    while (cur && !seen.has(cur.id)) {
      seen.add(cur.id);
      if (cur.id === focusId) return null;
      if (focusLine.has(cur.id)) break;
      top = cur.id;
      cur = cur.parentId ? byId.get(cur.parentId) : undefined;
    }
    return top;
  };

  const ghostIds = new Set<string>();
  const edgeMap = new Map<string, DiagramEdge>();
  const addEdge = (
    linkId: string,
    label: string,
    method: string | undefined,
    source: string,
    target: string,
    kind?: string,
    declared = false,
  ) => {
    if (source === target) return;
    const key = `${source}\0${target}`;
    const existing = edgeMap.get(key);
    if (!existing) {
      edgeMap.set(key, { id: linkId, source, target, label, method, kind, conforming: false, declared });
      return;
    }
    // A declared link takes the place of one imports imply; otherwise a
    // label fills an arrow that has none.
    if (declared && !existing.declared) Object.assign(existing, { id: linkId, label, method, kind, declared });
    else if (!existing.label && label && !existing.declared) existing.label = label;
  };
  // Every link at this level (C4): both ends inside, between the focus's
  // children; one end outside, to that end drawn as a ghost at the highest
  // node that does not hold the focus. Declared links keep their label at
  // every level; links read off the code carry none.
  const endAt = (nodeId: string) => {
    const inside = liftToLevel(nodeId);
    if (inside) return { id: inside, inside: true };
    const outside = focusId === null ? null : outsideAt(nodeId);
    return outside ? { id: outside, inside: false } : null;
  };
  // Links between the focus's children first, then the imports crossing its
  // boundary, then the other links that do, so the outside ends keep the
  // order the drawing places them in.
  const ends = model.links.map((link) => ({ link, s: endAt(link.src), t: endAt(link.dst) }));
  const addLink = ({ link, s, t }: (typeof ends)[number]) => {
    for (const end of [s!, t!]) if (!end.inside) ghostIds.add(end.id);
    addEdge(link.id, link.label ?? "", link.method, s!.id, t!.id, link.kind, !!link.declared);
  };
  for (const e of ends) if (e.s?.inside && e.t?.inside) addLink(e);

  // Imports the tree holds below its links: each crossing the focus's
  // boundary, its inside end rolled to the child it passes through and its
  // outside end drawn as above. Unlabelled: calculated, not declared.
  if (focusId !== null) {
    for (const e of report?.derived.resolvedEdges ?? []) {
      const s = endAt(e.srcNode), t = endAt(e.dstNode);
      if (!s || !t || s.inside === t.inside) continue;
      for (const end of [s, t]) if (!end.inside) ghostIds.add(end.id);
      addEdge("import:" + s.id + ">" + t.id, "", undefined, s.id, t.id);
    }
  }
  for (const e of ends) if (e.s && e.t && e.s.inside !== e.t.inside) addLink(e);
  const edges = [...edgeMap.values()];

  // Fan-in per visible node on this level: how many of its arrows end there.
  const degree = new Map<string, number>();
  for (const e of edges) {
    degree.set(e.target, (degree.get(e.target) ?? 0) + 1);
  }

  const base = (id: string, reference: boolean): Omit<DiagramNode, "x" | "y" | "dotSize" | "pinned"> => {
    const n = byId.get(id)!;
    return {
      id: n.id,
      kind: n.kind,
      name: n.name,
      external: Boolean(n.external),
      technology: n.technology,
      description: n.description,
      hasChildren: childIdsOf.has(n.id),
      childIds: childIdsOf.get(n.id) ?? [],
      degree: degree.get(n.id) ?? 0,
      reference,
      symbolClass: previewable.has(n.id) ? "visual" : isDataShape(n) ? "model" : "code",
      style: n.style,
      layer: layerOf(model, n.id),
      hasClaims: (n.claims?.length ?? 0) > 0 || (n.properties?.length ?? 0) > 0,
    };
  };

  // The component level of a styled container draws in its style's shape.
  const styledLevel = focusId !== null && children.length > 0 && children.every((c) => c.kind === "component");
  const styleDef = styledLevel ? governingStyleDef(model, focusId, styles) : undefined;
  const mode: DiagramMode =
    children.length > 0 && children.every((c) => c.kind === "symbol")
      ? "code"
      : styleDef
        ? "styled"
        : "arch";

  if (children.length === 0) {
    return { mode, focusId, nodes: [], edges };
  }

  // Real children first, then ghosts (referenced from this level, living
  // elsewhere). Both feed the layout so edges to ghosts route properly.
  const ghosts = [...ghostIds];
  const layoutIds = [...children.map((c) => c.id), ...ghosts];

  const sizeById = new Map(layoutIds.map((id) => [id, DOT] as const));

  // A position given on a node fixes it and auto-layout places the rest. The
  // app never gives one (its maps are derived); a caller drawing a curated
  // map does — the demo's film placements, an export. Arch tiers only, and
  // never ghosts: a ghost's position belongs to the surface where it lives.
  const pinnedPos = new Map<string, { x: number; y: number }>(
    mode === "arch"
      ? children
          .filter((c) => c.position)
          .map((c) => [c.id, { x: c.position!.x, y: c.position!.y }] as const)
      : [],
  );

  let styled: StyledScene | undefined;
  let regions: LayerRegion[] | undefined;
  let positions: Map<string, { x: number; y: number }>;
  let aside = new Set<string>();
  if (mode === "code") {
    // Symbols by call depth from the component's surface — importance in
    // position — instead of a free constellation.
    const laid = depthLayout(
      children.map((c) => ({ id: c.id, size: sizeById.get(c.id)!, labelW: estLabelWidth(c.name) })),
      ghosts,
      edges,
    );
    // The layout gives disc CENTRES; the dot node's origin is the disc's
    // top-left, so each dot shifts by its own radius or big dots drift.
    positions = new Map(
      [...laid.centers].map(([id, c]) => {
        const r = (sizeById.get(id) ?? MIN_DOT) / 2;
        return [id, { x: c.x - r, y: c.y - r }];
      }),
    );
    regions = laid.regions;
  } else if (mode === "styled" && styleDef) {
    // Structural violations from the health report — imports and declared
    // links the style forbids — are the only source of red. They are drawn even when no link
    // declares the pair: a declared-links-only view would read as compliance
    // the code does not have.
    for (const v of report?.structural?.violations ?? []) {
      if (!v.other) continue;
      const s = liftToLevel(v.node), t = liftToLevel(v.other);
      if (!s || !t || s === t) continue;
      const key = `${s}\0${t}`;
      const existing = edgeMap.get(key);
      if (existing) {
        existing.violations ??= [];
        if (!existing.violations.includes(v.detail)) existing.violations.push(v.detail);
        existing.conforming = false;
        continue;
      }
      const e: DiagramEdge = {
        id: "violation:" + key,
        source: s,
        target: t,
        label: "",
        method: undefined,
        kind: undefined,
        conforming: false,
        violations: [v.detail],
      };
      edgeMap.set(key, e);
      edges.push(e);
    }
    // Tooling components (examples, benches) hold no layer: a strip of
    // their own under the drawing.
    const toolIds = new Set(children.filter((c) => c.kind === "component" && c.tooling).map((c) => c.id));
    const members = children
      .filter((c) => c.kind === "component" && !toolIds.has(c.id))
      .map((c) => ({ id: c.id, layer: c.layer }));
    const laid = styledLayout(styleDef, members, ghosts, edges);
    for (const e of edges) e.waypoints = laid.routes?.get(`${e.source}\0${e.target}`);
    const tools = toolingStrip(laid, [...toolIds]);
    positions = new Map(
      [...laid.centers, ...tools.centers].map(([id, c]) => [id, { x: c.x - CARD_W / 2, y: c.y - CARD_H / 2 }]),
    );
    for (const [id, at] of pinnedPos) positions.set(id, at);
    const allRegions = tools.region ? [...laid.regions, tools.region] : laid.regions;
    styled = {
      name: styleDef.name,
      drawing: styleDef.drawing,
      regions: allRegions,
      layers: styleDef.layers.map((l) => l.name),
      purpose: styleDef.description,
      mayImport: styleDef.matrix,
    };
    regions = allRegions;
    // The drawing already says the links that conform: drawn quiet. Links
    // that leave the container say what the drawing cannot — who calls in,
    // what is used — so they are drawn plain.
    const layerById = new Map(children.map((c) => [c.id, c.layer] as const));
    const ringDrawing = styleDef.drawing === "rings" || styleDef.drawing === "hexagon";
    const innermost = styleDef.layers[styleDef.layers.length - 1]?.name;
    for (const e of edges) {
      if (e.id.startsWith("violation:")) continue;
      if (ghostIds.has(e.source) || ghostIds.has(e.target)) {
        e.conforming = false;
        continue;
      }
      // Tooling is held to no style, so its links break none, and say little
      // about the product: drawn quiet.
      if (toolIds.has(e.source) || toolIds.has(e.target)) {
        e.conforming = true;
        continue;
      }
      const sl = layerById.get(e.source), tl = layerById.get(e.target);
      e.conforming = !e.violations && classifyStyledEdge(styleDef, sl, tl) === "conforming";
      // A same-layer chord crosses whatever sits between its ends — the
      // centre on a ring, the neighbouring cards on a band. Bow it clear:
      // out to the ring itself, or above the band (left of the column).
      if (sl && sl === tl) {
        const sc = laid.centers.get(e.source), tc = laid.centers.get(e.target);
        if (!sc || !tc) continue;
        const mid = { x: (sc.x + tc.x) / 2, y: (sc.y + tc.y) / 2 };
        if (ringDrawing && sl !== innermost) {
          const ring = (Math.hypot(sc.x, sc.y) + Math.hypot(tc.x, tc.y)) / 2;
          e.bow = { cx: 0, cy: 0, offset: Math.max(40, ring - Math.hypot(mid.x, mid.y)) };
        } else if (!ringDrawing && Math.abs(styleDef.drawing === "columns" ? sc.y - tc.y : sc.x - tc.x) > CARD_W * 1.2) {
          // Only when a card can sit between the ends (further apart than one pitch).
          e.bow =
            styleDef.drawing === "columns"
              ? { cx: mid.x + 1e6, cy: mid.y, offset: CARD_W * 0.7 }
              : { cx: mid.x, cy: mid.y + 1e6, offset: CARD_H * 0.75 };
        }
      }
    }
  } else {
    // C4 arrangement: who uses the level above it, the level inside its
    // boundary (tooling in a band at its foot), external systems it calls
    // beside it, the rest of what it uses below.
    const inside: string[] = [], tooling: string[] = [], above: string[] = [];
    const beside: string[] = [], below: string[] = [];
    for (const id of layoutIds) {
      const n = byId.get(id)!;
      // An outside node that mostly calls into the level uses it: above.
      const calls = edges.filter((e) => e.source === id).length;
      const called = edges.filter((e) => e.target === id).length;
      if (n.kind === "person") above.push(id);
      else if (n.external) (calls > called ? above : beside).push(id);
      else if (ghostIds.has(id)) (calls > called ? above : below).push(id);
      else if (n.tooling) tooling.push(id);
      else inside.push(id);
    }
    const laid = c4Layout({ inside, tooling, above, beside, below, edges, cardW: CARD_W, cardH: CARD_H });
    aside = new Set(beside);
    positions = laid.positions;
    for (const [id, at] of pinnedPos) positions.set(id, at);
    // A pinned end moved off the rows the lanes were laid between.
    for (const e of edges) {
      const route = laid.routes.get(`${e.source}\0${e.target}`);
      if (route && !pinnedPos.has(e.source) && !pinnedPos.has(e.target)) e.waypoints = route;
    }
    // Tooling is held to no style, so its links break none, and say little
    // about the product: drawn quiet.
    const toolingSet = new Set(tooling);
    for (const e of edges) if (toolingSet.has(e.source) || toolingSet.has(e.target)) e.conforming = true;
    const focus = focusId ? byId.get(focusId) : undefined;
    regions = [];
    if (focus && laid.boundary) {
      regions.push({ shape: "rect", ...laid.boundary, layer: `${focus.name} · ${KIND_WORD[focus.kind] ?? focus.kind}`, boundary: true });
    }
    if (laid.toolingBand) {
      regions.push({
        shape: "rect",
        ...laid.toolingBand,
        layer: "tooling",
        caption: "Dev tooling: part of the codebase, not of the product, and held to no style. Its links are drawn faint.",
      });
    }
  }

  const nodes: DiagramNode[] = layoutIds.map((id) => {
    const p = positions.get(id) ?? { x: 0, y: 0 };
    return {
      ...base(id, ghostIds.has(id)),
      pinned: pinnedPos.has(id),
      aside: aside.has(id) || undefined,
      dotSize: sizeById.get(id) ?? MIN_DOT,
      x: p.x,
      y: p.y,
    };
  });

  // A declared link says something the drawing cannot: never quiet.
  for (const e of edges) if (e.declared) e.conforming = false;

  return { mode, focusId, nodes, edges, styled, regions };
}
