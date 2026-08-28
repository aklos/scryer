/**
 * The secondary Diagram view — a navigational projection of the model. It
 * renders one level at a time (the children of a focus node) and shares the
 * workspace selection with the tree and the wiki page:
 *
 *   - single-click a node      → select it (drives the shared selection)
 *   - double-click a container → drill into it (descend a level)
 *   - the ↗ button on a card   → drill into it (same)
 *   - breadcrumb               → ascend to any ancestor level
 *   - drag a card (arch tiers) → place it manually; the spot persists on the
 *     node (`position`) and auto-layout only places the unpinned rest
 *   - drag a dot (code tier)   → tug the live constellation apart (a resident
 *     d3-force sim, `useDotSim`) to trace connectivity; nothing persists
 *
 * Two render modes, chosen by `buildDiagramScene`: the architecture tiers draw
 * as the ported C4 cards-and-edges (planar layout + handle routing); the symbol
 * tier draws as a force-directed dot graph. All semantic editing stays in the
 * tree and the page — this surface navigates, places, and untangles.
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
import { ChevronRight, CornerLeftUp, LayoutGrid } from "lucide-react";
import type { ScryModel } from "./viewmodel";
import { concernCounts } from "./viewmodel";
import type { ModelDiff } from "./planDiff";
import { collectPlanEntries, MARK_META, type Mark, nodeDrift, resolveMark, rollupMarks } from "./changeMarks";
import { assignAllHandles } from "./edgeRouting";
import { DiagramCard, type CardData, type RFCard } from "./nodes/DiagramCard";
import { CenterHandle } from "./nodes/NodeHandles";
import { RelationshipEdge, type EdgeData } from "./edges/RelationshipEdge";
import { buildDiagramScene, CARD_W, CARD_H, type DiagramScene, type DiagramNode } from "./diagramLayout";
import { useDotSim } from "./hooks/useDotSim";
import type { ModelHealthReport } from "./health";

type DotData = CardData;
type RFDot = RFNode<DotData, "dot">;

/** The C4-style class word shown under each dot — the three things a symbol can
 *  be (mirrors `kindIcon`/`typeTag`). */
const SYMBOL_CLASS_LABEL: Record<DiagramNode["symbolClass"], string> = {
  code: "Code",
  model: "Model",
  visual: "Visual",
};

/** A code-tier dot (symbol): a change-mark-colored disc with its name and class
 *  centered beneath it — the constellation cousin of the arch tiers' C4 cards.
 *  Unchanged symbols read ghost-grey; a plan/drift mark tints the disc via
 *  `bg-current` inheriting the mark's hue. */
function DotNode({ data }: NodeProps<RFDot>) {
  const { node, selected, mark } = data;
  const meta = mark ? MARK_META[mark] : null;
  const ghost = node.reference;
  // Dot size encodes fan-in — the more depended-upon a symbol, the larger it
  // reads. Sized relative to the busiest hub on this level (see diagramLayout).
  const dia = node.dotSize;
  // A ghost (lives elsewhere) reads hollow: an outlined ring, no fill. The dash
  // is reserved for true externals, so an external ghost is hollow + dashed, an
  // internal ghost hollow + solid.
  const discClass = ghost
    ? `border ${node.external ? "border-dashed" : ""} border-[var(--text-muted)] bg-transparent`
    : meta
      ? `bg-current ${meta.color}`
      : "bg-[var(--text-ghost)]";
  // Subgraph highlight: faded when a selection elsewhere doesn't touch this dot.
  const dimClass = data.dimmed ? "opacity-20" : ghost ? "opacity-70" : "";
  return (
    <div className={`flex flex-col items-center transition-opacity ${dimClass}`}>
      <span
        style={{ width: dia, height: dia }}
        className={`relative shrink-0 rounded-full ${discClass} ${
          selected ? "ring-2 ring-[var(--border-strong)] ring-offset-1 ring-offset-[var(--surface-canvas)]" : ""
        }`}
        title={ghost ? `${node.name} (referenced — double-click to open)` : meta?.label}
      >
        <CenterHandle />
      </span>
      <div className="mt-1.5 flex flex-col items-center leading-tight">
        <span
          className={`max-w-[130px] truncate text-center text-xs ${
            ghost
              ? "italic text-[var(--text-muted)]"
              : selected
                ? "font-medium text-[var(--text)]"
                : "text-[var(--text-secondary)]"
          }`}
        >
          {node.name || "·"}
        </span>
        <span className="text-[10px] tracking-wider text-[var(--text-ghost)]">
          {SYMBOL_CLASS_LABEL[node.symbolClass]}
        </span>
      </div>
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
const SNAP_GRID: [number, number] = [24, 24];

export function DiagramView({
  model,
  planDiff,
  committed,
  report,
  focusId,
  selectedId,
  onFocus,
  onSelectNode,
  onMoveNode,
  pendingIds,
  concernLens,
  previewable,
}: {
  model: ScryModel;
  /** Symbol ids the preview sidecar can render — derived, drives the `visual` class. */
  previewable: ReadonlySet<string>;
  /** Live plan diff — colors each node/dot by its change mark. */
  planDiff: ModelDiff;
  /** Committed layer — parents/endpoints for deleted elements in roll-ups. */
  committed: ScryModel | null;
  /** Health report — its derived edges drive the implied-connection ghosts. */
  report: ModelHealthReport | null;
  /** The level being shown: children of this node, or top-level when null. */
  focusId: string | null;
  /** Currently selected node id (drives highlight). */
  selectedId: string | null;
  onFocus: (id: string | null) => void;
  onSelectNode: (id: string | null) => void;
  /** Persist a manual placement (a card was dragged), or release it with null
   *  (the re-layout control) so auto-layout owns the node again. Unset — e.g.
   *  while an agent owns the file — leaves the whole surface undraggable. */
  onMoveNode?: (id: string, position: { x: number; y: number } | null) => void;
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
        planDiff={planDiff}
        committed={committed}
        report={report}
        focusId={focusId}
        selectedId={selectedId}
        onFocus={onFocus}
        onSelectNode={onSelectNode}
        onMoveNode={onMoveNode}
        pendingIds={pendingIds}
        concernLens={concernLens}
        previewable={previewable}
      />
    </ReactFlowProvider>
  );
}

function DiagramInner({
  model,
  planDiff,
  committed,
  report,
  focusId,
  selectedId,
  onFocus,
  onSelectNode,
  onMoveNode,
  pendingIds,
  concernLens,
  previewable,
}: {
  model: ScryModel;
  /** Symbol ids the preview sidecar can render — derived, drives the `visual` class. */
  previewable: ReadonlySet<string>;
  planDiff: ModelDiff;
  committed: ScryModel | null;
  report: ModelHealthReport | null;
  focusId: string | null;
  selectedId: string | null;
  onFocus: (id: string | null) => void;
  onSelectNode: (id: string | null) => void;
  onMoveNode?: (id: string, position: { x: number; y: number } | null) => void;
  pendingIds?: ReadonlySet<string>;
  concernLens?: string | null;
}) {
  const [scene, setScene] = useState<DiagramScene | null>(null);
  const { fitView } = useReactFlow();
  const updateNodeInternals = useUpdateNodeInternals();

  // The rendered React Flow nodes. Real state, not a memo projection: RF's
  // controlled mode hands EVERYTHING back through onNodesChange — drag
  // positions, selection, and crucially the measured dimensions — and expects
  // them applied onto these very objects. Deriving nodes fresh each render
  // (the previous design) dropped `measured`, and a node whose object identity
  // changes without it is treated as un-measured: RF hides it and aborts the
  // drag (the "card vanishes and the pane pans" bug).
  const [rfNodes, setRfNodes] = useState<Array<RFCard | RFDot>>([]);

  // Set when the re-layout control releases this level's placements: the next
  // scene is a fresh auto-layout, likely nowhere near the old viewport (its
  // coordinates reset to the layout origin) — refit to it when it lands.
  const refitOnScene = useRef(false);

  // Build the scene whenever the level or the model changes. The planar layout
  // is async; guard against a stale build landing after a newer one.
  useEffect(() => {
    let live = true;
    void buildDiagramScene(model, focusId, report, previewable).then((s) => {
      if (live) {
        setScene(s);
        if (refitOnScene.current) {
          refitOnScene.current = false;
          setTimeout(() => void fitView({ padding: 0.2, duration: 300 }), 0);
        }
      }
    });
    return () => {
      live = false;
    };
  }, [model, focusId, report, previewable, fitView]);

  // The canonical controlled-flow contract: every change RF emits is applied
  // back onto the node state. Position changes move the dragged card,
  // dimension changes persist `measured` onto our objects (so later identity
  // breaks keep the node initialized), select changes keep RF's bookkeeping
  // coherent with its internal drag/selection machinery.
  const onNodesChange = useCallback((changes: NodeChange<RFCard | RFDot>[]) => {
    setRfNodes((nds) => applyNodeChanges(changes, nds));
  }, []);

  // The live physics for the code tier: dots drag through a resident d3-force
  // sim (view-only — nothing persists), while arch cards drag through the
  // onNodesChange + onMoveNode persistence path. Each sim tick streams its
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

  // Per-node change mark (plan ?? drift) from the SHARED plan-entry
  // computation, with descendant marks rolled up — a diagram card IS its
  // subtree's face, so a change anywhere beneath it must show on the card.
  const markFor = useMemo(() => {
    const entries = collectPlanEntries(planDiff, model, committed);
    const entryMark = new Map(entries.map((e) => [e.id, e.mark] as const));
    const rolled = rollupMarks(model, committed, entries);
    const byId = new Map(model.nodes.map((n) => [n.id, n] as const));
    return (id: string): Mark | null => {
      const vm = byId.get(id);
      if (!vm) return null;
      const own = resolveMark({ plan: entryMark.get(id) ?? null, drift: nodeDrift(vm) });
      return own ?? resolveMark(rolled.get(id) ?? { plan: null, drift: null });
    };
  }, [model, committed, planDiff]);

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

  // Arch cards drag and persist via onMoveNode — except ghosts, whose stored
  // placement belongs to the surface where the node really lives. Code-tier
  // dots drag whenever the live sim is on: pure physics, nothing written, so
  // ghosts (and agent-run sessions) join in freely.
  const archDraggable = scene?.mode === "arch" && !!onMoveNode;

  // (Re)build the node state from the scene and its presentation inputs. Two
  // regimes, told apart by whether the SCENE object changed since last run:
  //   - new scene → positions reset to its layout (auto + pinned placements);
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
          draggable: scene.mode === "code" ? dotSim.live : archDraggable && !n.reference,
          measured: old?.measured,
          selected: old?.selected,
          dragging: old?.dragging,
          data: {
            node: n,
            selected: n.id === selectedId,
            mark: markFor(n.id),
            dimmed:
              (highlight.active && !highlight.neighbors.has(n.id)) ||
              (concernLit !== null && !concernLit.has(n.id)),
            pending: pendingIds?.has(n.id),
            completeness: report?.completeness[n.id],
          },
        };
      }) as Array<RFCard | RFDot>;
    });
  }, [scene, selectedId, markFor, highlight, pendingIds, report, concernLit, archDraggable, dotSim.live]);

  const rfEdges = useMemo<RFEdge<EdgeData>[]>(() => {
    if (!scene) return [];
    // Arch tier: assign the best handle pair per edge (main's routing) — from
    // the LIVE node positions (rfNodes), not the scene's, so a dragged card's
    // edges re-pick their attachment sides mid-drag instead of staying glued
    // to a face that now points away until the drop rebuilds the scene.
    const liveNode = new Map(rfNodes.map((n) => [n.id, n] as const));
    const handles =
      scene.mode === "arch"
        ? assignAllHandles(
            scene.nodes.map((n) => {
              const live = liveNode.get(n.id);
              return {
                id: n.id,
                position: live?.position ?? { x: n.x, y: n.y },
                measured: {
                  width: live?.measured?.width ?? CARD_W,
                  height: live?.measured?.height ?? CARD_H,
                },
              };
            }),
            scene.edges,
          )
        : null;
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
    // Count edges per unordered node pair, so a reverse edge between the same
    // two nodes can be split into parallel lanes. Needed on both tiers: code
    // edges share a center handle and arch reverse pairs are routed onto the
    // same swapped handles, so either way the two chords land exactly collinear.
    const pairCount = new Map<string, number>();
    for (const e of scene.edges) {
      const k = e.source < e.target ? `${e.source}\0${e.target}` : `${e.target}\0${e.source}`;
      pairCount.set(k, (pairCount.get(k) ?? 0) + 1);
    }
    return scene.edges.map((e) => {
      const h = handles?.get(e.id);
      const connected = highlight.active && (e.source === selectedId || e.target === selectedId);
      const ghostEdge = ghostIds.has(e.source) || ghostIds.has(e.target);
      const pairKey = e.source < e.target ? `${e.source}\0${e.target}` : `${e.target}\0${e.source}`;
      return {
        id: e.id,
        source: e.source,
        target: e.target,
        type: "rel" as const,
        selectable: false,
        // Generation choreography (demo-only, like the card `pending` flag): an
        // edge into a node that hasn't generated yet stays out of the frame
        // entirely — it arrives with the node, not before it.
        hidden: pendingIds ? pendingIds.has(e.source) || pendingIds.has(e.target) : undefined,
        sourceHandle: scene.mode === "code" ? "c" : h?.sourceHandle,
        targetHandle: scene.mode === "code" ? "c" : h?.targetHandle,
        data: {
          label: ghostEdge ? undefined : e.label || undefined,
          method: ghostEdge ? undefined : e.method,
          dot: scene.mode === "code",
          sourceR: radius?.get(e.source),
          targetR: radius?.get(e.target),
          highlighted: connected,
          dimmed:
            (highlight.active && !connected) ||
            (concernLit !== null && (!concernLit.has(e.source) || !concernLit.has(e.target))),
          parallel: (pairCount.get(pairKey) ?? 0) > 1,
        },
      };
    });
  }, [scene, rfNodes, selectedId, highlight, concernLit, pendingIds]);

  // Refit when the level changes (a fresh scene of a different size/shape).
  const fitKey = `${focusId ?? "root"}:${rfNodes.length}`;
  const lastFit = useRef("");
  useEffect(() => {
    if (rfNodes.length === 0 || lastFit.current === fitKey) return;
    lastFit.current = fitKey;
    const t = setTimeout(() => void fitView({ padding: 0.2, duration: 300 }), 0);
    return () => clearTimeout(t);
  }, [fitKey, rfNodes.length, fitView]);

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
  const lastCardsKey = useRef<string | null>(null);
  useEffect(() => {
    if (!scene || scene.nodes.length === 0) {
      lastCardsKey.current = null;
      setEdgesReady(false);
      return;
    }
    if (lastCardsKey.current === cardsKey) {
      // Same cards, new positions — refresh the handle anchors in place.
      scene.nodes.forEach((n) => updateInternalsRef.current(n.id));
      return;
    }
    lastCardsKey.current = cardsKey;
    setEdgesReady(false);
    const t = setTimeout(() => {
      scene.nodes.forEach((n) => updateInternalsRef.current(n.id));
      setEdgesReady(true);
    }, 250);
    return () => clearTimeout(t);
  }, [scene, cardsKey]);

  const crumbs = useMemo(() => breadcrumb(model, focusId), [model, focusId]);

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
            snapToGrid={mode === "arch"}
            snapGrid={SNAP_GRID}
            nodesDraggable={archDraggable || dotSim.live}
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
            // Arch tiers: dropping a card pins it — the spot persists on the
            // node and auto-layout stops placing it (until re-layout releases
            // it). Code tier: release the dot and let the physics cool.
            onNodeDragStop={(_, n) => {
              if (mode === "code") dotSim.onDragStop(n.id);
              else onMoveNode?.(n.id, { x: Math.round(n.position.x), y: Math.round(n.position.y) });
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
            <Controls showInteractive={false} className="!shadow-none">
              {archDraggable && (
                <ControlButton
                  title="Re-run auto-layout (releases this level's manual placements)"
                  disabled={!scene?.nodes.some((n) => n.pinned)}
                  className="disabled:!cursor-default disabled:opacity-35"
                  onClick={() => {
                    if (!scene) return;
                    refitOnScene.current = true;
                    for (const n of scene.nodes) {
                      if (n.pinned) onMoveNode?.(n.id, null);
                    }
                  }}
                >
                  <LayoutGrid />
                </ControlButton>
              )}
            </Controls>
          </ReactFlow>
        )}
      </div>
    </div>
  );
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
