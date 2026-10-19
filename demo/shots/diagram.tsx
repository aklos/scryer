/**
 * The app's diagram, mounted on its own and moved by the film. The app's
 * scene builder lays out each model given (a level, a styled container);
 * its real cards, relationship edges and layer regions draw them in React
 * Flow. The film passes a step between two of those scenes: cards present
 * in both move from one placement to the other, cards in one only fade, the
 * layer regions cross-fade. The view fits every scene at once, so a beat
 * that adds or moves a card keeps its frame. React Flow zooms natively, so
 * cards stay sharp and their handles stay true.
 *
 * Film mode sizes the text the viewer must read in frame pixels whatever the
 * zoom (card titles, counts, ring labels; shots.css), drops card
 * descriptions, and sets ring labels where they read at that size: an outer
 * ring's on its empty bottom edge, a band's above the band.
 */

import "@xyflow/react/dist/style.css";
import { ConnectionMode, ReactFlow, ReactFlowProvider, type Edge as RFEdge } from "@xyflow/react";
import { useEffect, useLayoutEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import { continueRender, delayRender, Easing } from "remotion";
import type { ModelHealthReport, StyleViolation } from "../../src/entities/model/health";
import { structuralBySubtree } from "../../src/entities/model/health";
import type { ScryModel } from "../../src/entities/model/viewmodel";
import { assignAllHandles } from "../../src/shared/diagram/edgeRouting";
import { RelationshipEdge, type EdgeData } from "../../src/shared/diagram/RelationshipEdge";
import { DiagramCard, type RFCard } from "../../src/widgets/diagram-canvas/DiagramCard";
import { buildDiagramScene, CARD_H, CARD_W, type DiagramScene } from "../../src/widgets/diagram-canvas/diagramLayout";
import { StyleRegions } from "../../src/widgets/diagram-canvas/StyleRegions";

const nodeTypes = { card: DiagramCard };
const edgeTypes = { rel: RelationshipEdge };
const ease = Easing.bezier(0.65, 0, 0.35, 1);

/** One state of the drawing: a model at a level, and its violations. */
export interface SceneSpec {
  model: ScryModel;
  focusId: string | null;
  violations?: StyleViolation[];
}

function reportOf(violations: StyleViolation[]): ModelHealthReport {
  return {
    structural: { violations },
    derived: { resolvedEdges: [], linkAudit: [] },
    completeness: {},
    anchors: {},
    styles: [],
  } as unknown as ModelHealthReport;
}

/** The drawing's extent in scene units: every card and every region. */
function bounds(scenes: DiagramScene[]) {
  const xs: number[] = [];
  const ys: number[] = [];
  for (const s of scenes) {
    for (const n of s.nodes) xs.push(n.x, n.x + CARD_W), ys.push(n.y, n.y + CARD_H);
    for (const r of s.regions ?? []) {
      if (r.shape === "rect") xs.push(r.x, r.x + r.w), ys.push(r.y, r.y + r.h);
      else if (r.shape === "ring") xs.push(r.cx - r.r, r.cx + r.r), ys.push(r.cy - r.r, r.cy + r.r);
      else xs.push(r.cx - r.r, r.cx + r.r), ys.push(r.cy - r.r * 0.866, r.cy + r.r * 0.866);
    }
  }
  return { x0: Math.min(...xs), x1: Math.max(...xs), y0: Math.min(...ys), y1: Math.max(...ys) };
}

export function Diagram({
  scenes: specs,
  from,
  to = from,
  t = 0,
  width,
  height,
  fit = 40,
  extra = 0,
  clipLeft,
  film,
  edgeReveal,
  hideImplied = true,
  selected,
  tint,
  style,
}: {
  scenes: SceneSpec[];
  /** The step: from scene `from` to scene `to`, `t` of the way (0–1). */
  from: number;
  to?: number;
  t?: number;
  /** The canvas, in frame px, and the margin the drawing is fitted within. */
  width: number;
  height: number;
  fit?: number;
  /** Extra scene units around the drawing (for labels set outside it). */
  extra?: number;
  /** Leave the drawing left of this scene x out of the fit (an empty side). */
  clipLeft?: number;
  /** Frame-pixel sizes for the text the viewer reads, and the edge width. */
  film?: { title: number; count: number; label: number; flag: number; edge: number };
  /** Per edge (by source→target names' ids), how drawn it is (0–1); an
   *  edge with no entry is fully drawn. Implied edges show only through it. */
  edgeReveal?: (e: { id: string; source: string; target: string; implied?: boolean; violation: boolean }) => number | undefined;
  hideImplied?: boolean;
  /** A card the shot is about, drawn selected. */
  selected?: string;
  /** A ring whose label takes a colour (a layer name), and the colour. */
  tint?: { layer: string; color: string };
  style?: CSSProperties;
}) {
  const [scenes, setScenes] = useState<DiagramScene[] | null>(null);
  const [handle] = useState(() => delayRender("diagram scenes"));
  useEffect(() => {
    void Promise.all(specs.map((s) => buildDiagramScene(s.model, s.focusId, reportOf(s.violations ?? []), new Set()))).then(setScenes);
    // The specs are fixed for a beat; build once.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  // React Flow measures the cards before it can route edges.
  useEffect(() => {
    if (!scenes) return;
    const id = setTimeout(() => continueRender(handle), 400);
    return () => clearTimeout(id);
  }, [scenes, handle]);

  const view = useMemo(() => {
    if (!scenes) return { x: 0, y: 0, zoom: 1 };
    const b = bounds(scenes);
    if (clipLeft !== undefined) b.x0 = Math.max(b.x0, clipLeft);
    b.x0 -= extra;
    b.x1 += extra;
    b.y0 -= extra;
    const zoom = Math.min((width - 2 * fit) / (b.x1 - b.x0), (height - 2 * fit) / (b.y1 - b.y0));
    const cx = (b.x0 + b.x1) / 2;
    const cy = (b.y0 + b.y1) / 2;
    return { x: width / 2 - cx * zoom, y: height / 2 - cy * zoom, zoom };
  }, [scenes, width, height, fit, extra, clipLeft]);

  const k = ease(Math.max(0, Math.min(1, t)));
  const a = scenes?.[from];
  const b = scenes?.[to];
  const now = k < 0.5 ? a : b;
  const nowSpec = specs[k < 0.5 ? from : to];
  const report = useMemo(() => reportOf(nowSpec.violations ?? []), [nowSpec]);
  const structural = useMemo(() => structuralBySubtree(nowSpec.model, report), [nowSpec, report]);

  const nodes: RFCard[] = [];
  if (a && b) {
    const inA = new Map(a.nodes.map((n) => [n.id, n]));
    const inB = new Map(b.nodes.map((n) => [n.id, n]));
    for (const id of new Set([...inA.keys(), ...inB.keys()])) {
      const na = inA.get(id);
      const nb = inB.get(id);
      const n = (k < 0.5 ? na : nb) ?? na ?? nb!;
      const pa = na ?? nb!;
      const pb = nb ?? na!;
      const opacity = na && nb ? 1 : na ? 1 - k : k;
      nodes.push({
        id,
        type: "card",
        position: { x: pa.x + (pb.x - pa.x) * k, y: pa.y + (pb.y - pa.y) * k },
        draggable: false,
        style: { opacity },
        data: { node: n, selected: id === selected, structural: structural.get(id), styled: now?.mode === "styled" },
      });
    }
  }

  const edges: RFEdge<EdgeData>[] = [];
  let edgeStyles = "";
  if (now) {
    const handles = assignAllHandles(
      nodes.map((n) => ({ id: n.id, position: n.position, measured: { width: CARD_W, height: CARD_H } })),
      now.edges,
    );
    for (const [i, e] of now.edges.entries()) {
      const violation = !!e.violations?.length;
      const reveal = edgeReveal?.({ id: e.id, source: e.source, target: e.target, implied: e.implied, violation });
      if (e.implied && hideImplied && reveal === undefined) continue;
      if (reveal !== undefined && reveal <= 0) continue;
      const id = `film-edge-${i}`;
      if (reveal !== undefined) edgeStyles += `.react-flow__edge[data-id="${id}"] { opacity: ${reveal}; }\n`;
      edges.push({
        id,
        source: e.source,
        target: e.target,
        type: "rel",
        selectable: false,
        sourceHandle: handles.get(e.id)?.sourceHandle,
        targetHandle: handles.get(e.id)?.targetHandle,
        className: e.implied ? "shot-implied" : undefined,
        data: { bow: e.bow, violations: e.violations, label: film ? undefined : e.label || undefined },
      });
    }
  }

  // Regions: the outgoing scene's fade out as the incoming scene's fade in.
  const root = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    const portals = root.current?.querySelectorAll<HTMLElement>(".react-flow__viewport-portal > *");
    if (portals && portals.length === 2) {
      portals[0].style.opacity = String(1 - k);
      portals[1].style.opacity = String(k);
    }
    if (!film) return;
    for (const g of root.current?.querySelectorAll("svg g") ?? []) {
      const text = g.querySelector<SVGTextElement>(":scope > text");
      const shape = g.querySelector<SVGGraphicsElement>(":scope > polygon, :scope > rect");
      if (!text || !shape) continue;
      const box = shape.getBBox();
      const kind =
        text.dataset.film ?? (shape.tagName === "rect" ? "band" : text.getAttribute("text-anchor") === "start" ? "outer" : "inner");
      text.dataset.film = kind;
      text.setAttribute("x", String(box.x + box.width / 2));
      text.setAttribute(
        "y",
        String(kind === "outer" ? box.y + box.height - 30 / view.zoom : kind === "inner" ? box.y + box.height + 40 / view.zoom : box.y - 16 / view.zoom),
      );
      text.setAttribute("text-anchor", "middle");
    }
    for (const label of root.current?.querySelectorAll<SVGTextElement>("svg text") ?? []) {
      const name = label.firstChild?.textContent?.trim().toLowerCase() ?? "";
      label.style.fill = tint && name.startsWith(tint.layer.toLowerCase()) ? tint.color : "";
    }
  });

  const filmVars = film
    ? ({
        "--film-title": `${film.title / view.zoom}px`,
        "--film-count": `${film.count / view.zoom}px`,
        "--film-label": `${film.label / view.zoom}px`,
        "--film-flag": film.flag / view.zoom,
        "--film-edge": `${film.edge / view.zoom}px`,
      } as CSSProperties)
    : undefined;

  const regionsA = a?.regions;
  const regionsB = b?.regions;
  return (
    <div ref={root} className={film ? "shot-hex" : undefined} style={{ width, height, background: "var(--surface)", ...filmVars, ...style }}>
      <ReactFlowProvider>
        <ReactFlow
          nodes={nodes}
          edges={edges}
          nodeTypes={nodeTypes}
          edgeTypes={edgeTypes}
          // Every card handle is a source; an edge may end on any of them.
          connectionMode={ConnectionMode.Loose}
          viewport={view}
          minZoom={0.1}
          maxZoom={4}
          nodesDraggable={false}
          nodesConnectable={false}
          elementsSelectable={false}
          panOnDrag={false}
          zoomOnScroll={false}
          proOptions={{ hideAttribution: true }}
        >
          <StyleRegions regions={regionsA ?? []} />
          <StyleRegions regions={regionsB ?? []} />
        </ReactFlow>
      </ReactFlowProvider>
      <style>{edgeStyles}</style>
    </div>
  );
}
