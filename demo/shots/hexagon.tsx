/**
 * The diagram's styled level, mounted on its own: the app's scene builder
 * lays a hexagonal container out by layer, and its real cards, relationship
 * edges and layer regions draw it in React Flow — with a viewport the shot
 * sets (a centre and zoom, or a fit to the whole drawing). React Flow zooms
 * natively, so the cards stay crisp and their handles stay true.
 *
 * Film mode sizes the text the viewer must read in frame pixels whatever the
 * zoom (card titles, counts, ring labels; see shots.css), drops the card
 * descriptions, and can tint one ring's label. At film size a ring label no
 * longer fits where the app puts it (an outer ring's at its left vertex runs
 * into the ring inside; a band's overruns the band), so film mode centres an
 * outer ring's label on its empty bottom edge and a band's above the band.
 */

import "@xyflow/react/dist/style.css";
import { ConnectionMode, ReactFlow, ReactFlowProvider, type Edge as RFEdge } from "@xyflow/react";
import { useEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import { continueRender, delayRender } from "remotion";
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

/** The drawing's extent in scene units: every card and every region. */
function bounds(scene: DiagramScene) {
  const xs: number[] = [];
  const ys: number[] = [];
  for (const n of scene.nodes) xs.push(n.x, n.x + CARD_W), ys.push(n.y, n.y + CARD_H);
  for (const r of scene.regions ?? []) {
    if (r.shape === "rect") xs.push(r.x, r.x + r.w), ys.push(r.y, r.y + r.h);
    else if (r.shape === "ring") xs.push(r.cx - r.r, r.cx + r.r), ys.push(r.cy - r.r, r.cy + r.r);
    else xs.push(r.cx - r.r, r.cx + r.r), ys.push(r.cy - r.r * 0.866, r.cy + r.r * 0.866);
  }
  return { x0: Math.min(...xs), x1: Math.max(...xs), y0: Math.min(...ys), y1: Math.max(...ys) };
}

/** The health report the scene reads: structural violations, nothing derived. */
function reportOf(violations: StyleViolation[]): ModelHealthReport {
  return {
    structural: { violations },
    derived: { resolvedEdges: [], linkAudit: [] },
    completeness: {},
    anchors: {},
    styles: [],
  } as unknown as ModelHealthReport;
}

export function Hexagon({
  model,
  focusId,
  violations,
  width,
  height,
  center,
  zoom,
  fit,
  film,
  tint,
  openViolations = false,
}: {
  model: ScryModel;
  focusId: string;
  violations: StyleViolation[];
  /** The canvas, in frame pixels. */
  width: number;
  height: number;
  /** The scene point at the canvas centre, and the zoom — or `fit`, the
   *  frame-pixel margin to fit the whole drawing in. */
  center?: { x: number; y: number };
  zoom?: number;
  fit?: number;
  /** Frame-pixel sizes for the text the viewer reads, and the edge width. */
  film?: { title: number; count: number; label: number; flag: number; edge: number };
  /** A ring whose label takes a colour (a layer name), and the colour. */
  tint?: { layer: string; color: string };
  /** Open each violation edge's panel, as a click on the edge does. */
  openViolations?: boolean;
}) {
  const report = useMemo(() => reportOf(violations), [violations]);
  const [scene, setScene] = useState<DiagramScene | null>(null);
  const [handle] = useState(() => delayRender("hexagon scene"));
  useEffect(() => {
    void buildDiagramScene(model, focusId, report, new Set()).then(setScene);
  }, [model, focusId, report]);
  // React Flow measures the cards before it can route edges: two frames.
  useEffect(() => {
    if (!scene) return;
    let raf = requestAnimationFrame(() => (raf = requestAnimationFrame(() => setTimeout(() => continueRender(handle), 300))));
    return () => cancelAnimationFrame(raf);
  }, [scene, handle]);

  const view = useMemo(() => {
    if (!fit || !scene) return { center: center ?? { x: 0, y: 0 }, zoom: zoom ?? 1 };
    const b = bounds(scene);
    return {
      center: { x: (b.x0 + b.x1) / 2, y: (b.y0 + b.y1) / 2 },
      zoom: Math.min((width - 2 * fit) / (b.x1 - b.x0), (height - 2 * fit) / (b.y1 - b.y0)),
    };
  }, [fit, scene, center, zoom, width, height]);

  const root = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!film || !scene) return;
    for (const g of root.current?.querySelectorAll("svg g") ?? []) {
      const text = g.querySelector(":scope > text");
      const shape = g.querySelector<SVGGraphicsElement>(":scope > polygon, :scope > rect");
      if (!text || !shape) continue;
      const b = shape.getBBox();
      const outer = shape.tagName === "polygon" && text.getAttribute("text-anchor") === "start";
      if (!outer && shape.tagName !== "rect") continue;
      text.setAttribute("x", String(b.x + b.width / 2));
      text.setAttribute("y", String(outer ? b.y + b.height - 28 / view.zoom : b.y - 14 / view.zoom));
      text.setAttribute("text-anchor", "middle");
    }
  });

  // The tinted ring's label: found by its text once the regions have drawn.
  useEffect(() => {
    if (!tint || !scene) return;
    const label = [...(root.current?.querySelectorAll("svg text") ?? [])].find((t) =>
      t.firstChild?.textContent?.trim().toLowerCase().startsWith(tint.layer.toLowerCase()),
    ) as SVGTextElement | undefined;
    if (label) label.style.fill = tint.color;
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

  const structural = useMemo(() => structuralBySubtree(model, report), [model, report]);
  const nodes = useMemo<RFCard[]>(
    () =>
      (scene?.nodes ?? []).map((n) => ({
        id: n.id,
        type: "card" as const,
        position: { x: n.x, y: n.y },
        draggable: false,
        data: { node: n, selected: false, structural: structural.get(n.id), styled: scene?.mode === "styled" },
      })),
    [scene, structural],
  );
  const edges = useMemo<RFEdge<EdgeData>[]>(() => {
    if (!scene) return [];
    const handles = assignAllHandles(
      scene.nodes.map((n) => ({ id: n.id, position: { x: n.x, y: n.y }, measured: { width: CARD_W, height: CARD_H } })),
      scene.edges,
    );
    return scene.edges
      .filter((e) => !e.implied)
      .map((e) => ({
        id: e.id,
        source: e.source,
        target: e.target,
        type: "rel" as const,
        selectable: false,
        sourceHandle: handles.get(e.id)?.sourceHandle,
        targetHandle: handles.get(e.id)?.targetHandle,
        data: { bow: e.bow, violations: e.violations, open: openViolations && !!e.violations?.length, label: e.label || undefined },
      }));
  }, [scene, openViolations]);

  return (
    <div ref={root} className={`dark ${film ? "shot-hex" : ""}`} style={{ width, height, background: "var(--surface-canvas)", ...filmVars }}>
      <ReactFlowProvider>
        <ReactFlow
          nodes={nodes}
          edges={edges}
          nodeTypes={nodeTypes}
          edgeTypes={edgeTypes}
          // Every card handle is a source; an edge may end on any of them.
          connectionMode={ConnectionMode.Loose}
          viewport={{ x: width / 2 - view.center.x * view.zoom, y: height / 2 - view.center.y * view.zoom, zoom: view.zoom }}
          minZoom={0.2}
          maxZoom={4}
          nodesDraggable={false}
          nodesConnectable={false}
          elementsSelectable={false}
          panOnDrag={false}
          zoomOnScroll={false}
          proOptions={{ hideAttribution: true }}
        >
          {scene?.regions && <StyleRegions regions={scene.regions} />}
        </ReactFlow>
      </ReactFlowProvider>
    </div>
  );
}
