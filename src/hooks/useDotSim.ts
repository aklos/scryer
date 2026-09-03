/**
 * The live physics behind the code tier's dot graph. The static ForceAtlas2
 * constellation (`dotLayout`) stays the opening shape; this hook wraps it in a
 * resident d3-force simulation so the graph can be *tugged*: grab a dot and its
 * neighbours chase it, stretching a tangle apart far enough to read what is
 * actually wired to what. Exploration only — dot positions are never written
 * anywhere (unlike the arch tiers' pinned placements), so the sim works even
 * while an agent owns the model file.
 *
 * At rest the sim is indistinguishable from the static layout: it is created
 * cold (alpha 0, stopped) at the scene's positions and only heats up when a
 * drag starts. Release lets it cool back to a standstill. Each tick hands the
 * full position set to `onTick` — the caller owns the node state and applies
 * them there.
 */

import { useEffect, useMemo, useRef } from "react";
import {
  forceCollide,
  forceLink,
  forceManyBody,
  forceSimulation,
  forceX,
  forceY,
  type Simulation,
  type SimulationLinkDatum,
  type SimulationNodeDatum,
} from "d3-force";
import { dotCollideRadius, type DiagramScene } from "../diagramLayout";

/** Above this many dots a per-tick relayout of every node janks — the level
 *  falls back to the static constellation (no dragging). */
export const LIVE_DOT_MAX = 450;

/** Breathing room (px) the collide force keeps between dot rows while the
 *  graph is in motion. */
const COLLIDE_PAD = 6;

interface SimNode extends SimulationNodeDatum {
  id: string;
  r: number;
}

export interface DotSim {
  /** The code tier is live: dots are draggable and physics responds. */
  live: boolean;
  /** Wire these to React Flow's node-drag events for code-tier nodes. */
  onDragStart: (id: string, at: { x: number; y: number }) => void;
  onDrag: (id: string, at: { x: number; y: number }) => void;
  onDragStop: (id: string) => void;
}

export function useDotSim(
  scene: DiagramScene | null,
  onTick: (positions: ReadonlyMap<string, { x: number; y: number }>) => void,
): DotSim {
  const live =
    scene !== null &&
    scene.mode === "code" &&
    scene.regions === undefined &&
    scene.nodes.length > 1 &&
    scene.nodes.length <= LIVE_DOT_MAX;

  const simRef = useRef<Simulation<SimNode, SimulationLinkDatum<SimNode>> | null>(null);
  const nodeById = useRef<Map<string, SimNode>>(new Map());
  // The tick consumer, behind a ref so a re-render never rebuilds the sim.
  const onTickRef = useRef(onTick);
  onTickRef.current = onTick;

  useEffect(() => {
    simRef.current?.stop();
    simRef.current = null;
    nodeById.current = new Map();
    if (!live || !scene) return;

    // Seed at the settled ForceAtlas2 layout; r mirrors the rendered row.
    const simNodes: SimNode[] = scene.nodes.map((n) => ({
      id: n.id,
      x: n.x,
      y: n.y,
      r: dotCollideRadius(n.name, n.dotSize),
    }));
    const byId = new Map(simNodes.map((n) => [n.id, n]));
    const links: SimulationLinkDatum<SimNode>[] = scene.edges
      .filter((e) => byId.has(e.source) && byId.has(e.target) && e.source !== e.target)
      .map((e) => ({ source: e.source, target: e.target }));

    // Anchor the weak gravity on the seeded centroid, so cooling never walks
    // the constellation away from where the viewport framed it.
    const cx = simNodes.reduce((s, n) => s + (n.x ?? 0), 0) / simNodes.length;
    const cy = simNodes.reduce((s, n) => s + (n.y ?? 0), 0) / simNodes.length;

    const sim = forceSimulation<SimNode>(simNodes)
      .force(
        "link",
        forceLink<SimNode, SimulationLinkDatum<SimNode>>(links)
          .id((n) => n.id)
          .distance((l) => (l.source as SimNode).r + (l.target as SimNode).r + 30),
      )
      .force("charge", forceManyBody().strength(-160))
      .force("collide", forceCollide<SimNode>((n) => n.r + COLLIDE_PAD).iterations(2))
      .force("x", forceX(cx).strength(0.04))
      .force("y", forceY(cy).strength(0.04))
      .on("tick", () => {
        const next = new Map<string, { x: number; y: number }>();
        for (const n of simNodes) next.set(n.id, { x: n.x ?? 0, y: n.y ?? 0 });
        onTickRef.current(next);
      })
      // Cold start: at rest exactly on the static layout until a drag heats it.
      .alpha(0)
      .stop();

    simRef.current = sim;
    nodeById.current = byId;
    return () => {
      sim.stop();
    };
  }, [scene, live]);

  return useMemo<DotSim>(
    () => ({
      live,
      onDragStart: (id, at) => {
        const n = nodeById.current.get(id);
        const sim = simRef.current;
        if (!n || !sim) return;
        n.fx = at.x;
        n.fy = at.y;
        // Reheat and hold warm for the duration of the drag.
        sim.alphaTarget(0.3).restart();
      },
      onDrag: (id, at) => {
        const n = nodeById.current.get(id);
        if (!n) return;
        n.fx = at.x;
        n.fy = at.y;
      },
      onDragStop: (id) => {
        const n = nodeById.current.get(id);
        const sim = simRef.current;
        if (!n || !sim) return;
        n.fx = null;
        n.fy = null;
        // Release the heat target; the sim cools to a standstill on its own.
        sim.alphaTarget(0);
      },
    }),
    [live],
  );
}
