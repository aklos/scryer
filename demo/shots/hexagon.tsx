/**
 * The diagram's styled level, mounted on its own: the app's scene builder
 * lays a hexagonal container out by layer, and its real cards, relationship
 * edges and layer regions draw it in React Flow — with a viewport the shot
 * sets (centre and zoom), where the app would fit the whole level. React Flow
 * zooms natively, so the cards stay crisp and their handles stay true.
 */

import "@xyflow/react/dist/style.css";
import { ConnectionMode, ReactFlow, ReactFlowProvider, type Edge as RFEdge } from "@xyflow/react";
import { useEffect, useMemo, useState } from "react";
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
  openViolations = false,
}: {
  model: ScryModel;
  focusId: string;
  violations: StyleViolation[];
  /** The canvas, in frame pixels. */
  width: number;
  height: number;
  /** The scene point at the canvas centre, and the zoom. */
  center: { x: number; y: number };
  zoom: number;
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
    <div className="dark" style={{ width, height, background: "var(--surface-canvas)" }}>
      <ReactFlowProvider>
        <ReactFlow
          nodes={nodes}
          edges={edges}
          nodeTypes={nodeTypes}
          edgeTypes={edgeTypes}
          // Every card handle is a source; an edge may end on any of them.
          connectionMode={ConnectionMode.Loose}
          viewport={{ x: width / 2 - center.x * zoom, y: height / 2 - center.y * zoom, zoom }}
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
