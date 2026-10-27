/**
 * The Diagram view — a navigational projection of the tree. It renders one
 * level at a time (the children of a focus node) and shares the workspace
 * selection with the tree and the wiki page:
 *
 *   - single-click a node      → select it (drives the shared selection)
 *   - double-click a container → drill into it (descend a level)
 *   - the ↗ button on a card   → drill into it (same)
 *   - breadcrumb               → ascend to any ancestor level
 *   - click a red edge         → list its structural violations
 *   - drag a dot (code tier)   → tug the live constellation apart (a resident
 *     d3-force sim, `useDotSim`) to trace connectivity; nothing persists
 *
 * `buildDiagramScene` picks the drawing: architecture tiers as the ported C4
 * cards and edges (the C4 arrangement + handle routing), a styled level inside its
 * style's regions, the symbol tier as a force-directed dot graph. Cards carry
 * change marks, completeness and violation counts, and dim under the concern
 * lens. The map is derived, never arranged: cards stay where the layout puts
 * them (a curated drawing is an export tool's job, not the app's).
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  ReactFlow,
  ReactFlowProvider,
  applyNodeChanges,
  Background,
  BackgroundVariant,
  ConnectionMode,
  ControlButton,
  Controls,
  useReactFlow,
  useUpdateNodeInternals,
  type Node as RFNode,
  type Edge as RFEdge,
  type NodeChange,
  type NodeProps,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import { ChevronRight, CornerLeftUp, Maximize, Spline } from "lucide-react";
import type { ScryModel } from "../../entities/model/viewmodel";
import { concernCounts } from "../../entities/model/viewmodel";
import { attentionState, childTally, CLAIM_TONE } from "../../entities/model/claimState";
import { SYMBOL_ICON } from "../../entities/model/kindIcon";
import { placeLabels } from "../../shared/diagram/labelPlacement";
import { edgeLines, linePoints } from "./edgeLines";
import { DiagramCard, type CardData, type RFCard } from "./DiagramCard";
import { CenterHandle } from "../../shared/diagram/NodeHandles";
import { RelationshipEdge, edgeText, type EdgeData } from "../../shared/diagram/RelationshipEdge";
import { buildDiagramScene, CARD_W, CARD_H, type DiagramScene, type DiagramNode } from "./diagramLayout";
import { useDotSim } from "./useDotSim";
import { drawingBounds, StyleRegions } from "./StyleRegions";
import { structuralBySubtree, type ModelHealthReport } from "../../entities/model/health";

type DotData = CardData;
type RFDot = RFNode<DotData, "dot">;

/** Whether an edge shows: always, unless it conforms to the level's style
 *  and the map's conforming-links control is off — then only while a
 *  selection reaches one of its ends. */
export function edgeShown(conforming: boolean, showConforming: boolean, lit: boolean): boolean {
  return !conforming || showConforming || lit;
}

/** The class word for a symbol's label, now that its glyph says it. */
const SYMBOL_CLASS_LABEL: Record<DiagramNode["symbolClass"], string> = {
  code: "code",
  model: "data shape",
  visual: "visual",
};

/** A code-tier symbol: its type's glyph — the tree's, so a symbol looks the
 *  same everywhere — sized by fan-in, with its name beneath. The glyph's
 *  tone says what is known about it: full for a symbol with claims, ghost
 *  for one with none (a big ghost glyph is a hub nobody has described, the
 *  find worth making), red when a claim on it fails. A todo is work, not
 *  a fault, so it stays in the plain tone like a proven claim. */
export function DotNode({ data }: NodeProps<RFDot>) {
  const { node, selected, attention } = data;
  const meta = attention === "failing" ? { color: CLAIM_TONE.failing, label: "a claim here is failing" } : null;
  const ghost = node.reference;
  // Size encodes fan-in — the more depended-upon a symbol, the larger it
  // reads. Sized relative to the busiest hub on this level (see diagramLayout).
  const dia = node.dotSize;
  const Icon = SYMBOL_ICON[node.symbolClass];
  const undescribed = !ghost && !node.hasClaims;
  // A ghost lives elsewhere: its state is told on its own level, never here.
  const tone = ghost
    ? "text-[var(--text-muted)]"
    : meta
      ? meta.color
      : undescribed
        ? "text-[var(--text-ghost)]"
        : "text-[var(--text-secondary)]";
  // Subgraph highlight: faded when a selection elsewhere doesn't touch this
  // symbol. Leaves (undescribed, one caller at most) rest faded so the
  // important ones carry the level.
  const leaf = undescribed && node.degree <= 1;
  const dimClass = data.dimmed ? "opacity-20" : ghost ? "opacity-70" : leaf ? "opacity-45" : "";
  const said = [node.name, SYMBOL_CLASS_LABEL[node.symbolClass], ghost ? "referenced — double-click to open" : meta?.label]
    .filter(Boolean)
    .join(", ");
  return (
    <div className={`flex flex-col items-center transition-opacity ${dimClass}`}>
      <span
        style={{ width: dia, height: dia }}
        className={`relative flex shrink-0 items-center justify-center rounded-md ${tone} ${
          selected ? "ring-2 ring-[var(--border-strong)] ring-offset-2 ring-offset-[var(--surface-canvas)]" : ""
        }`}
        aria-label={said}
      >
        {/* The stroke thickens a little with size, so a hub's glyph holds
            its weight without the small ones going bold. */}
        <Icon width={dia} height={dia} absoluteStrokeWidth strokeWidth={1.5 + dia / 40} />
        <CenterHandle />
      </span>
      <span
        className={`mt-1.5 max-w-[130px] truncate text-center text-xs leading-tight ${
          ghost
            ? "italic text-[var(--text-muted)]"
            : selected
              ? "font-medium text-[var(--text)]"
              : "text-[var(--text-secondary)]"
        }`}
      >
        {node.name || "·"}
      </span>
    </div>
  );
}

const nodeTypes = { card: DiagramCard, dot: DotNode };
const edgeTypes = { rel: RelationshipEdge };

// Stable empty edge set, shown until a fresh level's cards are measured.
const NO_EDGES: RFEdge<EdgeData>[] = [];

// Drag snap pitch — one background dot (the Background gap below), so manual
// placements land on the visible grid. Arch tiers only: snapping the code
// tier's dots would fight the physics.

export function DiagramView({
  model,
  report,
  focusId,
  selectedId,
  onFocus,
  onSelectNode,
  pendingIds,
  concernLens,
  previewable,
}: {
  model: ScryModel;
  /** Symbol ids the preview sidecar can render — derived, drives the `visual` class. */
  previewable: ReadonlySet<string>;
  /** Health report — its derived edges drive the ghosts (cards outside the level),
   *  its derived status the claim bar on each card. */
  report: ModelHealthReport | null;
  /** The level being shown: children of this node, or top-level when null. */
  focusId: string | null;
  /** Currently selected node id (drives highlight). */
  selectedId: string | null;
  onFocus: (id: string | null) => void;
  onSelectNode: (id: string | null) => void;
  /** Nodes whose semantics aren't generated yet — drawn as pulsing placeholders.
   *  Used by the trailer's build sequence; unset (empty) in the product. */
  pendingIds?: ReadonlySet<string>;
  /** The active concern lens (a registry slug, or null): cards whose subtree
   *  holds no responsibility tagged with it dim, mirroring the tree. */
  concernLens?: string | null;
}) {
  return (
    <ReactFlowProvider>
      <DiagramInner
        model={model}
        report={report}
        focusId={focusId}
        selectedId={selectedId}
        onFocus={onFocus}
        onSelectNode={onSelectNode}
        pendingIds={pendingIds}
        concernLens={concernLens}
        previewable={previewable}
      />
    </ReactFlowProvider>
  );
}

function DiagramInner({
  model,
  report,
  focusId,
  selectedId,
  onFocus,
  onSelectNode,
  pendingIds,
  concernLens,
  previewable,
}: {
  model: ScryModel;
  /** Symbol ids the preview sidecar can render — derived, drives the `visual` class. */
  previewable: ReadonlySet<string>;
  report: ModelHealthReport | null;
  focusId: string | null;
  selectedId: string | null;
  onFocus: (id: string | null) => void;
  onSelectNode: (id: string | null) => void;
  pendingIds?: ReadonlySet<string>;
  concernLens?: string | null;
}) {
  const [scene, setScene] = useState<DiagramScene | null>(null);
  const { fitBounds, getNodes, getNodesBounds } = useReactFlow();
  const updateNodeInternals = useUpdateNodeInternals();

  // The rendered React Flow nodes. Real state, not a memo projection: RF's
  // controlled mode hands EVERYTHING back through onNodesChange — drag
  // positions, selection, and crucially the measured dimensions — and expects
  // them applied onto these very objects. Deriving nodes fresh each render
  // (the previous design) dropped `measured`, and a node whose object identity
  // changes without it is treated as un-measured: RF hides it and aborts the
  // drag (the "card vanishes and the pane pans" bug).
  const [rfNodes, setRfNodes] = useState<Array<RFCard | RFDot>>([]);
  // The edges that conform to a level's style, shown quiet or (from the map
  // controls) hidden until a selection reaches one of their ends.
  const [showConforming, setShowConforming] = useState(true);

  // Build the scene whenever the level or the model changes. The build is
  // async; guard against a stale build landing after a newer one.
  useEffect(() => {
    let live = true;
    void buildDiagramScene(model, focusId, report, previewable).then((s) => {
      if (live) setScene(s);
    });
    return () => {
      live = false;
    };
  }, [model, focusId, report, previewable]);

  // The canonical controlled-flow contract: every change RF emits is applied
  // back onto the node state. Position changes move the dragged card,
  // dimension changes persist `measured` onto our objects (so later identity
  // breaks keep the node initialized), select changes keep RF's bookkeeping
  // coherent with its internal drag/selection machinery.
  const onNodesChange = useCallback((changes: NodeChange<RFCard | RFDot>[]) => {
    setRfNodes((nds) => applyNodeChanges(changes, nds));
  }, []);

  // The live physics for the code tier: dots drag through a resident d3-force
  // sim (view-only — nothing persists); cards don't drag. Each sim tick streams its
  // positions straight into the node state; `measured` and friends ride along
  // on the spread.
  const applySimPositions = useCallback(
    (positions: ReadonlyMap<string, { x: number; y: number }>) => {
      setRfNodes((nds) =>
        nds.map((n) => {
          const at = positions.get(n.id);
          return at ? { ...n, position: at } : n;
        }),
      );
    },
    [],
  );
  const dotSim = useDotSim(scene, applySimPositions);
  const mode = scene?.mode ?? null;

  // A card's ↗ button drills in via a window event (mirrors main's decoupling).
  useEffect(() => {
    const onExpand = (e: Event) => {
      const id = (e as CustomEvent<{ nodeId: string }>).detail?.nodeId;
      if (id) onFocus(id);
    };
    window.addEventListener("diagram-expand", onExpand);
    return () => window.removeEventListener("diagram-expand", onExpand);
  }, [onFocus]);

  // Claim states per node, over its subtree — a card IS its subtree's face,
  // so a failing or planned claim anywhere beneath it shows on the card.
  const claimsOf = report?.derivedStatus?.nodes;

  // Subgraph highlight: when the selected node is on this level, its incident
  // edges and neighbour nodes stay lit and everything else dims. Inactive when
  // the selection lives at another level (nothing here to anchor it).
  const highlight = useMemo(() => {
    if (!scene || !selectedId || !scene.nodes.some((n) => n.id === selectedId)) {
      return { active: false, neighbors: new Set<string>() };
    }
    const neighbors = new Set<string>([selectedId]);
    for (const e of scene.edges) {
      if (e.source === selectedId) neighbors.add(e.target);
      else if (e.target === selectedId) neighbors.add(e.source);
    }
    return { active: true, neighbors };
  }, [scene, selectedId]);

  // Concern lens: a card is lit when its subtree holds a responsibility tagged
  // with the active slug; everything else dims. Composes with the subgraph
  // highlight — either reason to dim, dims.
  const concernLit = useMemo(() => {
    if (!concernLens) return null;
    return concernCounts(model, concernLens);
  }, [model, concernLens]);

  // Structural violations per subtree — each card counts what lies inside it.
  const structural = useMemo(() => structuralBySubtree(model, report ?? null), [model, report]);

  // (Re)build the node state from the scene and its presentation inputs. Two
  // regimes, told apart by whether the SCENE object changed since last run:
  //   - new scene → positions reset to its layout;
  //   - data-only refresh (selection, marks, lens…) → each node keeps its
  //     current position, so a warmed sim or an in-flight drag isn't snapped
  //     back to the static layout by a mere click.
  // RF bookkeeping (`measured`, `selected`, `dragging`) is carried over by id
  // in both regimes — losing `measured` un-initializes (hides) a node.
  const lastBuiltScene = useRef<DiagramScene | null>(null);
  useEffect(() => {
    const sceneChanged = lastBuiltScene.current !== scene;
    lastBuiltScene.current = scene;
    if (!scene) {
      setRfNodes([]);
      return;
    }
    const type = scene.mode === "code" ? "dot" : "card";
    setRfNodes((prev) => {
      const prevById = new Map(prev.map((p) => [p.id, p]));
      return scene.nodes.map((n) => {
        const old = prevById.get(n.id);
        return {
          id: n.id,
          type,
          position: sceneChanged ? { x: n.x, y: n.y } : old?.position ?? { x: n.x, y: n.y },
          // Code-tier dots drag whenever the live sim is on: pure physics,
          // nothing written. Cards stay where the layout put them.
          draggable: scene.mode === "code" && dotSim.live,
          measured: old?.measured,
          selected: old?.selected,
          dragging: old?.dragging,
          data: {
            node: n,
            selected: n.id === selectedId,
            children: n.childIds.length
              ? childTally(n.childIds.map((c) => claimsOf?.[c]?.subtree))
              : undefined,
            attention: attentionState(claimsOf?.[n.id]?.subtree),
            dimmed:
              (highlight.active && !highlight.neighbors.has(n.id)) ||
              (concernLit !== null && !concernLit.has(n.id)),
            pending: pendingIds?.has(n.id),
            structural: structural.get(n.id),
            styled: scene.mode === "styled",
          },
        };
      }) as Array<RFCard | RFDot>;
    });
  }, [scene, selectedId, claimsOf, highlight, pendingIds, structural, concernLit, dotSim.live]);

  // Each card's box, at its measured size once React Flow has measured it, and
  // each edge's ends and turns over them (as the layout-quality score reads
  // them too). Routing is the costly part, so it reruns only when the scene
  // or a card's size changes, never on a selection.
  const sizeKey = rfNodes.map((n) => `${n.id}:${n.measured?.width ?? ""}x${n.measured?.height ?? ""}`).join("|");
  const geometry = useMemo(() => {
    if (!scene) return null;
    const liveNode = new Map(rfNodes.map((n) => [n.id, n] as const));
    const boxes = scene.nodes.map((n) => {
      const live = liveNode.get(n.id);
      return {
        id: n.id,
        x: n.x,
        y: n.y,
        w: live?.measured?.width ?? CARD_W,
        h: live?.measured?.height ?? CARD_H,
      };
    });
    return { boxes, boxOf: new Map(boxes.map((b) => [b.id, b])), lines: edgeLines(scene, boxes) };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- `sizeKey` stands for the sizes read off `rfNodes`
  }, [scene, sizeKey]);

  const rfEdges = useMemo<RFEdge<EdgeData>[]>(() => {
    if (!scene || !geometry) return [];
    const { boxes, boxOf, lines } = geometry;
    // Dot radius per node, so the edge insets to the rim of each (now variable-
    // sized) dot instead of a fixed guess.
    const radius =
      scene.mode === "code"
        ? new Map(scene.nodes.map((n) => [n.id, n.dotSize / 2] as const))
        : null;
    // Edges touching a ghost (cross-boundary reference) node carry a ×n count
    // label that isn't useful — drop it on every tier.
    const ghostIds = new Set(
      scene.nodes.filter((n) => n.reference).map((n) => n.id),
    );
    // Every edge is drawn; one the drawing already implies (a legal step
    // between a style's layers, a tooling link) sits quiet, so the ones that
    // say something new stand out — or, with the map control off, hides until
    // a selection reaches one of its ends.
    const out = scene.edges.map((e) => {
      const line = lines.get(e.id)!;
      const connected = highlight.active && (e.source === selectedId || e.target === selectedId);
      const ghostEdge = ghostIds.has(e.source) || ghostIds.has(e.target);
      return {
        id: e.id,
        source: e.source,
        target: e.target,
        type: "rel" as const,
        selectable: false,
        // Generation choreography (demo-only, like the card `pending` flag): an
        // edge into a node that hasn't generated yet stays out of the frame
        // entirely — it arrives with the node, not before it.
        hidden:
          (pendingIds ? pendingIds.has(e.source) || pendingIds.has(e.target) : false) ||
          !edgeShown(e.conforming, showConforming, connected),
        sourceHandle: scene.mode === "code" ? "c" : line.sourceHandle,
        targetHandle: scene.mode === "code" ? "c" : line.targetHandle,
        data: {
          bow: line.bow,
          waypoints: line.waypoints,
          sourceShift: line.sourceShift,
          targetShift: line.targetShift,
          violations: e.violations,
          quiet: e.conforming,
          label: ghostEdge ? undefined : e.label || undefined,
          method: ghostEdge ? undefined : e.method,
          dot: scene.mode === "code",
          sourceR: radius?.get(e.source),
          targetR: radius?.get(e.target),
          highlighted: connected,
          dimmed:
            (highlight.active && !connected) ||
            (concernLit !== null && (!concernLit.has(e.source) || !concernLit.has(e.target))),
          parallel: line.parallel,
        },
      };
    });
    // Box tiers: every label placed at once, each clear of the other edges,
    // pills and cards (`placeLabels`) — read off the same line the edge draws.
    if (scene.mode !== "code") {
      const shown = out.filter((e) => !e.hidden && boxOf.has(e.source) && boxOf.has(e.target));
      const spots = placeLabels(
        shown.map((e) => ({
          id: e.id,
          points: linePoints(e, lines.get(e.id)!, boxOf),
          label: edgeText(e.data),
          method: e.data.method,
          pair: e.data.parallel ? [e.source, e.target].sort().join("\0") : undefined,
        })),
        boxes,
      );
      for (const e of out) (e.data as EdgeData).labelAt = spots.get(e.id);
    }
    return out;
  }, [scene, geometry, selectedId, highlight, concernLit, pendingIds, showConforming]);

  // Fit the whole drawing — the cards and the regions behind them — so a
  // ring or a band label is never cut off at the edge.
  const regions = scene?.regions;
  const fitDrawing = useCallback(
    (duration = 300) => {
      const nodes = getNodes();
      if (nodes.length === 0) return;
      void fitBounds(drawingBounds(getNodesBounds(nodes), regions ?? []), { padding: 0.08, duration });
    },
    [getNodes, getNodesBounds, fitBounds, regions],
  );

  // Refit when the level changes (a fresh scene of a different size/shape).
  const fitKey = `${focusId ?? "root"}:${rfNodes.length}`;
  const lastFit = useRef("");
  useEffect(() => {
    if (rfNodes.length === 0 || lastFit.current === fitKey) return;
    lastFit.current = fitKey;
    const t = setTimeout(() => fitDrawing(), 0);
    return () => clearTimeout(t);
  }, [fitKey, rfNodes.length, fitDrawing]);

  // Edges route off the card handles, but React Flow doesn't know a handle's
  // real position until it has MEASURED the card — so an edge that mounts before
  // its cards anchors to a guessed box and only snaps right on the first
  // interaction. So for a fresh level we withhold the edges for a beat (by which
  // point the cards have laid out and measured), recompute the handle positions,
  // then mount them — they route correctly on first paint. As a bonus the cards
  // land first and the connections follow, which reads well during the build.
  const [edgesReady, setEdgesReady] = useState(false);
  const updateInternalsRef = useRef(updateNodeInternals);
  updateInternalsRef.current = updateNodeInternals;
  // Key of the card SET on screen — a scene rebuild that only moved cards (a
  // drag committing its placement) keeps the same key, and must not blank the
  // edges for the measure beat: the cards are already measured.
  const cardsKey = scene
    ? `${scene.focusId ?? "root"}:${scene.nodes.map((n) => n.id).sort().join("|")}`
    : "";
  const sceneRef = useRef(scene);
  sceneRef.current = scene;
  // Mount the edges a beat after a new card SET lands. Keyed on the set alone:
  // a rebuild that keeps the cards (the health report arriving a moment after
  // the tree) must not cancel the pending mount, or the level stays edgeless.
  useEffect(() => {
    if (!cardsKey) {
      setEdgesReady(false);
      return;
    }
    setEdgesReady(false);
    const t = setTimeout(() => {
      for (const n of sceneRef.current?.nodes ?? []) updateInternalsRef.current(n.id);
      setEdgesReady(true);
    }, 250);
    return () => clearTimeout(t);
  }, [cardsKey]);
  // Same cards, new scene (moved, or re-derived): refresh the handle anchors.
  useEffect(() => {
    if (scene) scene.nodes.forEach((n) => updateInternalsRef.current(n.id));
  }, [scene]);

  const crumbs = useMemo(() => breadcrumb(model, focusId), [model, focusId]);
  const level = c4Level(model, focusId);

  return (
    // select-none: the map is a diagram, not copyable prose — without it, a
    // text selection swept in from the surrounding chrome paints the
    // full-canvas SVG layers (background, edges) as giant highlight slabs.
    <div className="relative flex h-full min-w-0 flex-1 select-none flex-col bg-[var(--surface-canvas)]">
      {/* Breadcrumb / level bar */}
      <div className="flex h-9 shrink-0 items-center gap-1 border-b border-[var(--border)] bg-[var(--surface)] px-3 text-xs">
        {focusId !== null && (
          <button
            type="button"
            title="Up a level"
            onClick={() => onFocus(crumbs.length >= 2 ? crumbs[crumbs.length - 2].id : null)}
            className="mr-1 flex items-center rounded p-1 text-[var(--text-muted)] hover:bg-[var(--surface-hover)] hover:text-[var(--text)]"
          >
            <CornerLeftUp className="h-3.5 w-3.5" />
          </button>
        )}
        <button
          type="button"
          onClick={() => onFocus(null)}
          className={`rounded px-1.5 py-0.5 hover:bg-[var(--surface-hover)] ${
            focusId === null ? "font-medium text-[var(--text)]" : "text-[var(--text-muted)]"
          }`}
        >
          All systems
        </button>
        {crumbs.map((c, i) => (
          <span key={c.id} className="flex items-center gap-1">
            <ChevronRight className="h-3 w-3 text-[var(--text-ghost)]" />
            <button
              type="button"
              onClick={() => onFocus(c.id)}
              className={`max-w-[180px] truncate rounded px-1.5 py-0.5 hover:bg-[var(--surface-hover)] ${
                i === crumbs.length - 1
                  ? "font-medium text-[var(--text)]"
                  : "text-[var(--text-muted)]"
              }`}
            >
              {c.name || "Untitled"}
            </button>
          </span>
        ))}
        <span className="flex-1" />
        {level && (
          <span className="flex shrink-0 items-baseline gap-1.5" aria-label={level.hint}>
            <span className="font-medium text-[var(--text-secondary)]">{level.name}</span>
            <span className="font-mono text-2xs text-[var(--text-muted)]">C4 level {level.n}</span>
          </span>
        )}
      </div>

      <div className="relative min-h-0 flex-1">
        {scene && scene.nodes.length === 0 ? (
          <div className="flex h-full items-center justify-center px-6 text-center text-xs text-[var(--text-muted)]">
            Nothing to diagram at this level.
          </div>
        ) : (
          <ReactFlow
            nodes={rfNodes}
            edges={edgesReady ? rfEdges : NO_EDGES}
            nodeTypes={nodeTypes}
            edgeTypes={edgeTypes}
            connectionMode={ConnectionMode.Loose}
            nodesDraggable={dotSim.live}
            nodesConnectable={false}
            elementsSelectable
            edgesFocusable={false}
            proOptions={{ hideAttribution: true }}
            minZoom={0.2}
            maxZoom={2}
            onNodesChange={onNodesChange}
            // Code tier: the drag pins the dot to the pointer and heats the
            // sim, so its neighbours chase it (pure exploration, no writes).
            onNodeDragStart={(_, n) => {
              if (mode === "code") dotSim.onDragStart(n.id, n.position);
            }}
            onNodeDrag={(_, n) => {
              if (mode === "code") dotSim.onDrag(n.id, n.position);
            }}
            // Release the dot and let the physics cool.
            onNodeDragStop={(_, n) => {
              if (mode === "code") dotSim.onDragStop(n.id);
            }}
            onNodeClick={(_, n) => onSelectNode(n.id)}
            // Clicking the empty canvas selects the parent of this level (the
            // focus node), which practically deselects every node on screen. At
            // the top level there's no parent, so it just clears the selection.
            onPaneClick={() => onSelectNode(focusId)}
            onNodeDoubleClick={(_, n) => {
              const data = n.data as CardData;
              if (data.node.reference) {
                // Ghost: jump to where it actually lives — frame its real
                // parent level and select it there.
                const real = model.nodes.find((m) => m.id === n.id);
                onFocus(real?.parentId ?? null);
                onSelectNode(n.id);
              } else if (data.node.hasChildren) {
                onFocus(n.id);
              }
            }}
            fitView
          >
            <Background
              variant={BackgroundVariant.Dots}
              gap={24}
              size={1}
              color="var(--border-subtle)"
              className="!bg-[var(--surface-canvas)]"
            />
            {scene?.regions && <StyleRegions regions={scene.regions} />}
            <Controls showInteractive={false} showFitView={false} className="!shadow-none">
              <ControlButton
                onClick={() => fitDrawing()}
                className="react-flow__controls-fitview"
                aria-label="Fit the whole drawing"
                title="Fit view"
              >
                <Maximize />
              </ControlButton>
              <ControlButton
                onClick={() => setShowConforming((v) => !v)}
                aria-pressed={showConforming}
                aria-label={showConforming ? "Hide links that conform to the style" : "Show links that conform to the style"}
                title="Conforming links"
                style={showConforming ? undefined : { opacity: 0.45 }}
              >
                <Spline />
              </ControlButton>
            </Controls>
          </ReactFlow>
        )}
      </div>
    </div>
  );
}

/** Which C4 diagram a level is: what the boxes on it are. */
function c4Level(
  model: ScryModel,
  focusId: string | null,
): { n: number; name: string; hint: string } | null {
  if (focusId === null)
    return { n: 1, name: "System context", hint: "The systems, the people who use them, and what they depend on" };
  const kind = model.nodes.find((n) => n.id === focusId)?.kind;
  if (kind === "system")
    return { n: 2, name: "Containers", hint: "The deployables and libraries inside this system, and what they use" };
  if (kind === "container")
    return { n: 3, name: "Components", hint: "The modules inside this container, and how they depend on each other" };
  if (kind === "component")
    return { n: 4, name: "Code", hint: "The symbols inside this component, by call depth" };
  return null;
}

/** Root→focus chain (excluding root), one entry per ancestor, for the breadcrumb. */
function breadcrumb(
  model: ScryModel,
  focusId: string | null,
): { id: string; name: string }[] {
  if (focusId === null) return [];
  const byId = new Map(model.nodes.map((n) => [n.id, n]));
  const chain: { id: string; name: string }[] = [];
  let cur = byId.get(focusId);
  const seen = new Set<string>();
  while (cur && !seen.has(cur.id)) {
    seen.add(cur.id);
    chain.unshift({ id: cur.id, name: cur.name });
    cur = cur.parentId ? byId.get(cur.parentId) : undefined;
  }
  return chain;
}
