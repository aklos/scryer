/**
 * The secondary Diagram view — a read-only, navigational projection of the
 * model. It renders one level at a time (the children of a focus node) and
 * shares the workspace selection with the tree and the wiki page:
 *
 *   - single-click a node      → select it (drives the shared selection)
 *   - double-click a container → drill into it (descend a level)
 *   - the ↗ button on a card   → drill into it (same)
 *   - breadcrumb               → ascend to any ancestor level
 *
 * Two render modes, chosen by `buildDiagramScene`: the architecture tiers draw
 * as the ported C4 cards-and-edges (planar layout + handle routing); the symbol
 * tier draws as a force-directed dot graph. All editing stays in the tree and
 * the page — this surface only navigates.
 */

import { useEffect, useMemo, useRef, useState } from "react";
import {
  ReactFlow,
  ReactFlowProvider,
  Background,
  BackgroundVariant,
  ConnectionMode,
  Controls,
  useReactFlow,
  type Node as RFNode,
  type Edge as RFEdge,
  type NodeProps,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import { ChevronRight, CornerLeftUp } from "lucide-react";
import type { ScryModel } from "./viewmodel";
import type { ModelDiff } from "./planDiff";
import { indexDiff, MARK_META, type Mark, nodeMarks, resolveMark } from "./changeMarks";
import { assignAllHandles } from "./edgeRouting";
import { DiagramCard, type CardData, type RFCard } from "./nodes/DiagramCard";
import { CenterHandle } from "./nodes/NodeHandles";
import { RelationshipEdge, type EdgeData } from "./edges/RelationshipEdge";
import { buildDiagramScene, type DiagramScene } from "./diagramLayout";
import type { ModelHealthReport } from "./health";

type DotData = CardData;
type RFDot = RFNode<DotData, "dot">;

/** A code-tier dot (symbol): a change-mark-colored disc with a label beside it.
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
    <div className={`flex items-center gap-1.5 transition-opacity ${dimClass}`}>
      <span
        style={{ width: dia, height: dia }}
        className={`relative shrink-0 rounded-full ${discClass} ${
          selected ? "ring-2 ring-[var(--border-strong)] ring-offset-1 ring-offset-[var(--surface-canvas)]" : ""
        }`}
        title={ghost ? `${node.name} (referenced — double-click to open)` : meta?.label}
      >
        <CenterHandle />
      </span>
      <span
        className={`max-w-[160px] truncate text-xs ${
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

// Card footprint, for handle routing before React Flow has measured the nodes.
const CARD_W = 180;
const CARD_H = 160;

export function DiagramView({
  model,
  planDiff,
  report,
  focusId,
  selectedId,
  onFocus,
  onSelectNode,
}: {
  model: ScryModel;
  /** Live plan diff — colors each node/dot by its change mark. */
  planDiff: ModelDiff;
  /** Health report — its derived edges drive the implied-connection ghosts. */
  report: ModelHealthReport | null;
  /** The level being shown: children of this node, or top-level when null. */
  focusId: string | null;
  /** Currently selected node id (drives highlight). */
  selectedId: string | null;
  onFocus: (id: string | null) => void;
  onSelectNode: (id: string | null) => void;
}) {
  return (
    <ReactFlowProvider>
      <DiagramInner
        model={model}
        planDiff={planDiff}
        report={report}
        focusId={focusId}
        selectedId={selectedId}
        onFocus={onFocus}
        onSelectNode={onSelectNode}
      />
    </ReactFlowProvider>
  );
}

function DiagramInner({
  model,
  planDiff,
  report,
  focusId,
  selectedId,
  onFocus,
  onSelectNode,
}: {
  model: ScryModel;
  planDiff: ModelDiff;
  report: ModelHealthReport | null;
  focusId: string | null;
  selectedId: string | null;
  onFocus: (id: string | null) => void;
  onSelectNode: (id: string | null) => void;
}) {
  const [scene, setScene] = useState<DiagramScene | null>(null);
  const { fitView } = useReactFlow();

  // Build the scene whenever the level or the model changes. The planar layout
  // is async; guard against a stale build landing after a newer one.
  useEffect(() => {
    let live = true;
    void buildDiagramScene(model, focusId, report).then((s) => {
      if (live) setScene(s);
    });
    return () => {
      live = false;
    };
  }, [model, focusId, report]);

  // A card's ↗ button drills in via a window event (mirrors main's decoupling).
  useEffect(() => {
    const onExpand = (e: Event) => {
      const id = (e as CustomEvent<{ nodeId: string }>).detail?.nodeId;
      if (id) onFocus(id);
    };
    window.addEventListener("diagram-expand", onExpand);
    return () => window.removeEventListener("diagram-expand", onExpand);
  }, [onFocus]);

  // Index the plan diff once, and look the full model node up per scene node so
  // its change mark (plan ?? drift) can tint the card / dot.
  const diffIndex = useMemo(() => indexDiff(planDiff), [planDiff]);
  const markFor = useMemo(() => {
    const byId = new Map(model.nodes.map((n) => [n.id, n] as const));
    return (id: string): Mark | null => {
      const vm = byId.get(id);
      return vm ? resolveMark(nodeMarks(vm, diffIndex)) : null;
    };
  }, [model, diffIndex]);

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

  const rfNodes = useMemo<Array<RFCard | RFDot>>(() => {
    if (!scene) return [];
    const type = scene.mode === "code" ? "dot" : "card";
    return scene.nodes.map((n) => ({
      id: n.id,
      type,
      position: { x: n.x, y: n.y },
      data: {
        node: n,
        selected: n.id === selectedId,
        mark: markFor(n.id),
        dimmed: highlight.active && !highlight.neighbors.has(n.id),
      },
    })) as Array<RFCard | RFDot>;
  }, [scene, selectedId, markFor, highlight]);

  const rfEdges = useMemo<RFEdge<EdgeData>[]>(() => {
    if (!scene) return [];
    // Arch tier: assign the best handle pair per edge (main's routing). Dot
    // tier: connect center-to-center through the single "c" handle.
    const handles =
      scene.mode === "arch"
        ? assignAllHandles(
            scene.nodes.map((n) => ({
              id: n.id,
              position: { x: n.x, y: n.y },
              measured: { width: CARD_W, height: CARD_H },
            })),
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
    return scene.edges.map((e) => {
      const h = handles?.get(e.id);
      const connected = highlight.active && (e.source === selectedId || e.target === selectedId);
      const ghostEdge = ghostIds.has(e.source) || ghostIds.has(e.target);
      return {
        id: e.id,
        source: e.source,
        target: e.target,
        type: "rel" as const,
        selectable: false,
        sourceHandle: scene.mode === "code" ? "c" : h?.sourceHandle,
        targetHandle: scene.mode === "code" ? "c" : h?.targetHandle,
        data: {
          label: ghostEdge ? undefined : e.label || undefined,
          method: ghostEdge ? undefined : e.method,
          dot: scene.mode === "code",
          sourceR: radius?.get(e.source),
          targetR: radius?.get(e.target),
          highlighted: connected,
          dimmed: highlight.active && !connected,
        },
      };
    });
  }, [scene, selectedId, highlight]);

  // Refit when the level changes (a fresh scene of a different size/shape).
  const fitKey = `${focusId ?? "root"}:${rfNodes.length}`;
  const lastFit = useRef("");
  useEffect(() => {
    if (rfNodes.length === 0 || lastFit.current === fitKey) return;
    lastFit.current = fitKey;
    const t = setTimeout(() => void fitView({ padding: 0.2, duration: 300 }), 0);
    return () => clearTimeout(t);
  }, [fitKey, rfNodes.length, fitView]);

  const crumbs = useMemo(() => breadcrumb(model, focusId), [model, focusId]);

  return (
    <div className="relative flex h-full min-w-0 flex-1 flex-col bg-[var(--surface-canvas)]">
      {/* Breadcrumb / level bar */}
      <div className="flex h-9 shrink-0 items-center gap-1 border-b border-[var(--border)] bg-[var(--surface)] px-3 text-xs">
        {focusId !== null && (
          <button
            type="button"
            title="Up a level"
            onClick={() => onFocus(crumbs.length >= 2 ? crumbs[crumbs.length - 2].id : null)}
            className="mr-1 flex items-center rounded p-1 text-[var(--text-muted)] hover:bg-[var(--surface-hover)] hover:text-[var(--text)] cursor-pointer"
          >
            <CornerLeftUp className="h-3.5 w-3.5" />
          </button>
        )}
        <button
          type="button"
          onClick={() => onFocus(null)}
          className={`rounded px-1.5 py-0.5 cursor-pointer hover:bg-[var(--surface-hover)] ${
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
              className={`max-w-[180px] truncate rounded px-1.5 py-0.5 cursor-pointer hover:bg-[var(--surface-hover)] ${
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
            edges={rfEdges}
            nodeTypes={nodeTypes}
            edgeTypes={edgeTypes}
            connectionMode={ConnectionMode.Loose}
            nodesDraggable={false}
            nodesConnectable={false}
            elementsSelectable
            edgesFocusable={false}
            proOptions={{ hideAttribution: true }}
            minZoom={0.2}
            maxZoom={2}
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
            <Controls showInteractive={false} className="!shadow-none" />
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
