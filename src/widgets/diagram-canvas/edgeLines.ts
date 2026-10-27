/**
 * Where each edge of a box-tier scene runs, as the map draws it: the handle or
 * port it leaves and enters by, the turns between, and the line through them.
 * Pure, so the renderer (DiagramView) and the layout-quality score
 * (`layoutQuality`) read one geometry.
 *
 * A drawing that runs one way — the C4 stack of rows, a style's bands or
 * columns — meets its cards at ports along that flow (`assignPorts`), each
 * edge in straight runs and turns between them (`orthoRoutes`). Rings and the
 * hexagon have no one direction: each edge is routed around the cards from
 * whichever sides serve it best (`routeAround`).
 */

import { assignAllHandles, getHandlePositions } from "../../shared/diagram/edgeRouting";
import { edgeShape, type Pt } from "../../shared/diagram/edgeGeometry";
import { assignPorts, orthoRoutes, portPoint, type PortBox, type Side } from "./edgePorts";
import { routeAround } from "./orthoRouter";
import type { DiagramEdge, DiagramScene } from "./diagramLayout";

export interface EdgeLine {
  sourceHandle?: string;
  targetHandle?: string;
  /** Offset of a port along its side (port drawings). */
  sourceShift?: number;
  targetShift?: number;
  /** The turns between the two ends, source end first. */
  waypoints?: Pt[];
  /** Shares its node pair with a reverse edge: drawn as a parallel lane. */
  parallel: boolean;
  /** Bowed off a ring (an edge left unrouted). */
  bow?: DiagramEdge["bow"];
}

/** Each edge's ends and turns over `boxes` (top-left, at their drawn size). */
export function edgeLines(scene: DiagramScene, boxes: PortBox[]): Map<string, EdgeLine> {
  const boxOf = new Map(boxes.map((b) => [b.id, b]));
  const drawing = scene.styled?.drawing;
  const flow = scene.mode === "arch" || drawing === "rows" ? "down" : drawing === "columns" ? "across" : null;
  const ports = flow
    ? assignPorts(boxes, scene.edges, flow, new Set(scene.nodes.filter((n) => n.aside).map((n) => n.id)))
    : null;
  const turns = ports
    ? orthoRoutes(
        scene.edges.flatMap((e) => {
          const p = ports.get(e.id);
          const s = boxOf.get(e.source), t = boxOf.get(e.target);
          if (!p || !s || !t) return [];
          return [{
            id: e.id,
            start: portPoint(s, p.sourceSide, p.sourceShift),
            end: portPoint(t, p.targetSide, p.targetShift),
            axis: p.flow,
            waypoints: e.waypoints,
          }];
        }),
      )
    : null;
  const routed = scene.mode !== "code" && !ports ? routeAround(boxes, scene.edges) : null;
  // An edge no route was found for: the nearest handle pair, the line straight.
  const stray = routed ? scene.edges.filter((e) => !routed.has(e.id)) : [];
  const handles = stray.length
    ? assignAllHandles(
        boxes.map((b) => ({ id: b.id, position: { x: b.x, y: b.y }, measured: { width: b.w, height: b.h } })),
        stray,
      )
    : null;
  // Count edges per unordered node pair, so a reverse edge between the same
  // two nodes can be split into parallel lanes. Needed on both tiers: code
  // edges share a center handle and arch reverse pairs are routed onto the
  // same swapped handles, so either way the two chords land exactly collinear.
  const pairCount = new Map<string, number>();
  for (const e of scene.edges) pairCount.set(pairKey(e), (pairCount.get(pairKey(e)) ?? 0) + 1);
  const out = new Map<string, EdgeLine>();
  for (const e of scene.edges) {
    const onPort = ports?.get(e.id), route = routed?.get(e.id);
    const port = onPort ?? route;
    const h = port ? { sourceHandle: port.sourceSide, targetHandle: port.targetSide } : handles?.get(e.id);
    out.set(e.id, {
      sourceHandle: h?.sourceHandle,
      targetHandle: h?.targetHandle,
      sourceShift: port?.sourceShift,
      targetShift: port?.targetShift,
      waypoints: onPort ? turns!.get(e.id) : route ? route.waypoints : e.waypoints,
      // Ports already part a reverse pair onto two lines.
      parallel: !port && (pairCount.get(pairKey(e)) ?? 0) > 1,
      bow: port ? undefined : e.bow,
    });
  }
  return out;
}

/** The line an edge draws between its two cards, as a polyline. */
export function linePoints(
  e: Pick<DiagramEdge, "source" | "target">,
  line: EdgeLine,
  boxOf: ReadonlyMap<string, PortBox>,
): Pt[] {
  const end = (id: string, handle: string | undefined, shift: number | undefined, fallback: Side) => {
    const b = boxOf.get(id)!;
    const side = (handle ?? fallback) as Side;
    return ["top", "bottom", "left", "right"].includes(side)
      ? portPoint(b, side, shift ?? 0)
      : getHandlePositions({ id, position: { x: b.x, y: b.y }, measured: { width: b.w, height: b.h } })[
          side as unknown as "right"
        ];
  };
  const s = end(e.source, line.sourceHandle, line.sourceShift, "right");
  const t = end(e.target, line.targetHandle, line.targetShift, "left");
  return edgeShape(s.x, s.y, t.x, t.y, { bow: line.bow, waypoints: line.waypoints, parallel: line.parallel }).points;
}

const pairKey = (e: { source: string; target: string }) =>
  e.source < e.target ? `${e.source}\0${e.target}` : `${e.target}\0${e.source}`;
