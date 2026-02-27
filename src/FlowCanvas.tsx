import { forwardRef, memo, useCallback, useEffect, useImperativeHandle, useMemo, useRef, useState } from "react";
import {
  ReactFlow,
  Background,
  BackgroundVariant,
  Controls,
  Panel,
  MarkerType,
  ConnectionMode,
  applyNodeChanges,
  applyEdgeChanges,
  useReactFlow,
} from "@xyflow/react";
import type {
  DefaultEdgeOptions,
  OnNodesChange,
  OnEdgesChange,
  OnConnect,
  Node,
  Edge,
} from "@xyflow/react";
import { FlowStepNode } from "./nodes/FlowStepNode";
import { edgeTypes } from "./edges";
import { Trash2, Plus } from "lucide-react";
import { Button } from "./ui";
import type { C4Node, C4NodeData, Flow, FlowStep, FlowTransition, Status } from "./types";
import type { MentionItem } from "./MentionTextarea";
import { assignAllHandles } from "./edgeRouting";
import { FlowGuidePanel } from "./GuidePanels";
import type { MentionNodeInfo } from "./DescriptionText";

export type LinkedProcess = { id: string; name: string; status?: Status };

const flowNodeTypes = { flowStep: memo(FlowStepNode) };

const defaultEdgeOptions: DefaultEdgeOptions = {
  markerEnd: { type: MarkerType.ArrowClosed, width: 16, height: 16 },
};

const STEP_W = 220;

/** Compute step labels from the DAG: 1 → 2 → 3, forks as 2A/2B, merges resume numbering.
 *  Branch letter order is determined by spatial position (left-to-right, then top-to-bottom). */
function computeStepLabels(steps: FlowStep[], transitions: FlowTransition[], positionMap?: Map<string, { x: number; y: number }>): Map<string, string> {
  const stepIds = new Set(steps.map((s) => s.id));
  const adj = new Map<string, string[]>();
  const parents = new Map<string, string[]>();
  for (const s of steps) {
    adj.set(s.id, []);
    parents.set(s.id, []);
  }
  for (const t of transitions) {
    if (stepIds.has(t.source) && stepIds.has(t.target)) {
      adj.get(t.source)!.push(t.target);
      parents.get(t.target)!.push(t.source);
    }
  }

  const labels = new Map<string, string>();
  const connected = new Set<string>();
  for (const t of transitions) {
    if (stepIds.has(t.source) && stepIds.has(t.target)) {
      connected.add(t.source);
      connected.add(t.target);
    }
  }
  const sortByPosition = (ids: string[]) => {
    if (!positionMap || ids.length <= 1) return ids;
    return [...ids].sort((a, b) => {
      const pa = positionMap.get(a) ?? { x: 0, y: 0 };
      const pb = positionMap.get(b) ?? { x: 0, y: 0 };
      return pa.x !== pb.x ? pa.x - pb.x : pa.y - pb.y;
    });
  };

  const roots = steps.filter((s) => connected.has(s.id) && parents.get(s.id)!.length === 0);
  if (roots.length === 0) return labels;

  const LETTERS = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
  const queue: { id: string; label: string }[] = [];

  if (roots.length === 1) {
    queue.push({ id: roots[0].id, label: "1" });
    labels.set(roots[0].id, "1");
  } else {
    const sortedRoots = sortByPosition(roots.map((r) => r.id));
    sortedRoots.forEach((id, i) => {
      const l = `1${LETTERS[i] ?? i}`;
      queue.push({ id, label: l });
      labels.set(id, l);
    });
  }

  while (queue.length > 0) {
    const { id, label } = queue.shift()!;
    const children = adj.get(id) ?? [];
    if (children.length === 0) continue;

    const baseNum = parseInt(label.replace(/[^0-9]/g, ""), 10) + 1;
    const letterSuffix = label.replace(/[0-9]/g, "");

    // Handle merge children first (any child with multiple parents)
    let deferred = false;
    const nonMerge: string[] = [];

    for (const child of children) {
      if (labels.has(child)) continue;
      const childParents = parents.get(child)!;
      if (childParents.length > 1) {
        const allLabeled = childParents.every((p) => labels.has(p));
        if (!allLabeled) { deferred = true; continue; }
        const maxParent = Math.max(...childParents.map((p) => parseInt(labels.get(p)!.replace(/[^0-9]/g, ""), 10)));
        const mergeLabel = String(maxParent + 1);
        labels.set(child, mergeLabel);
        queue.push({ id: child, label: mergeLabel });
      } else {
        nonMerge.push(child);
      }
    }

    if (deferred) { queue.push({ id, label }); }

    // Label non-merge children
    if (nonMerge.length === 1) {
      // Single continuation — preserve branch letter (e.g. 2A → 3A)
      const childLabel = `${baseNum}${letterSuffix}`;
      labels.set(nonMerge[0], childLabel);
      queue.push({ id: nonMerge[0], label: childLabel });
    } else if (nonMerge.length > 1) {
      // Fork — assign new letters
      const sorted = sortByPosition(nonMerge);
      sorted.forEach((child, i) => {
        const childLabel = `${baseNum}${LETTERS[i] ?? i}`;
        labels.set(child, childLabel);
        queue.push({ id: child, label: childLabel });
      });
    }
  }

  return labels;
}

function buildPositionMap(steps: FlowStep[]): Map<string, { x: number; y: number }> {
  const map = new Map<string, { x: number; y: number }>();
  steps.forEach((s, i) => map.set(s.id, s.position ?? { x: (i % 4) * (STEP_W + 40), y: Math.floor(i / 4) * 100 }));
  return map;
}

function buildNodeMentions(allNodes: C4Node[]): { mentionItems: MentionItem[]; nodeMap: Map<string, MentionNodeInfo> } {
  const mentionItems: MentionItem[] = [];
  const nodeMap = new Map<string, MentionNodeInfo>();
  for (const n of allNodes) {
    const d = n.data as C4NodeData;
    const k = d.kind;
    if (k === "person" || k === "system" || k === "container" || k === "component" || k === "operation" || k === "process" || k === "model") {
      nodeMap.set(d.name, { kind: k, status: d.status });
      mentionItems.push({ name: d.name, kind: k });
    }
  }
  return { mentionItems, nodeMap };
}

function stepMentionNames(labels: Map<string, string>, excludeId: string, nodeMentions: MentionItem[]): MentionItem[] {
  const items: MentionItem[] = [];
  for (const [id, label] of labels) {
    if (id !== excludeId) items.push({ name: label, kind: "step" });
  }
  items.push(...nodeMentions);
  return items;
}

function stepsToNodes(steps: FlowStep[], transitions: FlowTransition[], selectedStepId: string | null, allNodes: C4Node[]): Node[] {
  const labels = computeStepLabels(steps, transitions, buildPositionMap(steps));
  const processMap = new Map<string, LinkedProcess>(
    allNodes
      .filter((n) => (n.data as C4NodeData).kind === "process")
      .map((n) => [n.id, { id: n.id, name: (n.data as C4NodeData).name, status: (n.data as C4NodeData).status }]),
  );
  const { mentionItems, nodeMap } = buildNodeMentions(allNodes);
  return steps.map((step, i) => {
    const linkedProcesses = (step.processIds ?? [])
      .map((pid) => processMap.get(pid))
      .filter((p): p is LinkedProcess => !!p);
    return {
      id: step.id,
      type: "flowStep",
      position: step.position ?? { x: (i % 4) * (STEP_W + 40), y: Math.floor(i / 4) * 100 },
      data: { description: step.description, stepLabel: labels.get(step.id), linkedProcesses, _mentionNames: stepMentionNames(labels, step.id, mentionItems), _nodeMap: nodeMap },
      selected: step.id === selectedStepId,
    };
  });
}

function transitionsToEdges(transitions: FlowTransition[]): Edge[] {
  return transitions.map((t, i) => ({
    id: `st-${t.source}-${t.target}-${i}`,
    source: t.source,
    target: t.target,
    data: t.label ? { label: t.label } : undefined,
    label: t.label,
  }));
}

export interface FlowCanvasHandle {
  addStep: () => void;
  deleteSelected: () => void;
  updateTransitionLabel: (label: string) => void;
}

interface FlowCanvasProps {
  flow: Flow;
  onUpdate: (updated: Flow) => void;
  selectedStepId: string | null;
  onSelectStep: (id: string | null) => void;
  allNodes: C4Node[];
  onSelectionInfo: (info: { hasNodeSelection: boolean; selectedTransitionIndex: number }) => void;
}

export const FlowCanvas = forwardRef<FlowCanvasHandle, FlowCanvasProps>(function FlowCanvas({ flow, onUpdate, selectedStepId, onSelectStep, allNodes, onSelectionInfo }, ref) {
  const [nodes, setNodes] = useState<Node[]>(() =>
    stepsToNodes(flow.steps, flow.transitions, selectedStepId, allNodes),
  );
  const [selectedEdgeId, setSelectedEdgeId] = useState<string | null>(null);
  const prevLabelsRef = useRef<Map<string, string>>(new Map());

  // Sync nodes when steps are added/removed (by id set), or transitions change.
  // Do NOT rebuild on position-only changes — those are handled by applyNodeChanges.
  const stepIdKey = flow.steps.map((s) => s.id).join(",");
  const transitionKey = flow.transitions.map((t) => `${t.source}-${t.target}`).join(",");
  const processIdsKey = flow.steps.map((s) => (s.processIds ?? []).join("|")).join(",");
  useEffect(() => {
    setNodes((prev) => {
      const prevIds = new Set(prev.map((n) => n.id));
      const nextIds = new Set(flow.steps.map((s) => s.id));
      const idsMatch = prevIds.size === nextIds.size && [...prevIds].every((id) => nextIds.has(id));

      if (!idsMatch) {
        // Steps added or removed — rebuild, preserving positions of existing nodes
        const posMap = new Map(prev.map((n) => [n.id, n.position]));
        return stepsToNodes(flow.steps, flow.transitions, selectedStepId, allNodes).map((n) => {
          const existing = posMap.get(n.id);
          return existing ? { ...n, position: existing } : n;
        });
      }

      // Same step IDs — just refresh labels, descriptions, and linked processes
      const livePosMap = new Map(prev.map((n) => [n.id, n.position]));
      const labels = computeStepLabels(flow.steps, flow.transitions, livePosMap);
      const processMap = new Map<string, LinkedProcess>(
        allNodes
          .filter((n) => (n.data as C4NodeData).kind === "process")
          .map((n) => [n.id, { id: n.id, name: (n.data as C4NodeData).name, status: (n.data as C4NodeData).status }]),
      );
      const { mentionItems, nodeMap } = buildNodeMentions(allNodes);
      return prev.map((n) => {
        const step = flow.steps.find((s) => s.id === n.id);
        const newLabel = labels.get(n.id);
        const newDesc = step?.description;
        const newLinked = (step?.processIds ?? []).map((pid) => processMap.get(pid)).filter((x): x is LinkedProcess => !!x);
        const mentions = stepMentionNames(labels, n.id, mentionItems);
        return { ...n, data: { ...n.data, description: newDesc, stepLabel: newLabel, linkedProcesses: newLinked, _mentionNames: mentions, _nodeMap: nodeMap } };
      });
    });
  }, [stepIdKey, transitionKey, processIdsKey, flow.steps, flow.transitions, selectedStepId, allNodes]);

  // Auto-rename @[label] mentions when step labels change
  useEffect(() => {
    const livePosMap = new Map(nodes.map((n) => [n.id, n.position]));
    const newLabels = computeStepLabels(flow.steps, flow.transitions, livePosMap);
    const prev = prevLabelsRef.current;

    // Build rename map: oldLabel → newLabel (only for steps whose label actually changed)
    const renames = new Map<string, string>();
    for (const [id, newLabel] of newLabels) {
      const oldLabel = prev.get(id);
      if (oldLabel && oldLabel !== newLabel) {
        renames.set(oldLabel, newLabel);
      }
    }

    prevLabelsRef.current = newLabels;

    if (renames.size === 0) return;

    // Replace @[oldLabel] → @[newLabel] in all step descriptions
    let changed = false;
    const updatedSteps = flow.steps.map((s) => {
      if (!s.description) return s;
      let desc = s.description;
      for (const [oldL, newL] of renames) {
        const re = new RegExp(`@\\[${oldL.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}\\]`, "g");
        desc = desc.replace(re, `@[${newL}]`);
      }
      if (desc === s.description) return s;
      changed = true;
      return { ...s, description: desc };
    });

    if (changed) {
      onUpdate({ ...flow, steps: updatedSteps });
    }
  }, [transitionKey, stepIdKey]);

  // Listen for inline description edits from step nodes
  useEffect(() => {
    const handler = (e: Event) => {
      const { stepId, description } = (e as CustomEvent).detail;
      onUpdate({
        ...flow,
        steps: flow.steps.map((s) =>
          s.id === stepId ? { ...s, description } : s,
        ),
      });
    };
    window.addEventListener("update-step-description", handler);
    return () => window.removeEventListener("update-step-description", handler);
  }, [flow, onUpdate]);

  // mention-click — select step by label (flow level)
  useEffect(() => {
    const handler = (e: Event) => {
      const { name } = (e as CustomEvent).detail;
      // Find the step whose label matches the clicked mention
      const match = nodes.find((n) => (n.data as { stepLabel?: string }).stepLabel === name);
      if (match) {
        onSelectStep(match.id);
        setNodes((prev) => prev.map((n) => ({ ...n, selected: n.id === match.id })));
      }
    };
    window.addEventListener("mention-click", handler);
    return () => window.removeEventListener("mention-click", handler);
  }, [nodes, onSelectStep]);

  // Sync selection when a specific step is programmatically selected (e.g. from sidebar)
  // Don't clear selection when null — that would break multi-select
  useEffect(() => {
    if (!selectedStepId) return;
    setNodes((prev) => {
      let changed = false;
      const next = prev.map((n) => {
        const shouldSelect = n.id === selectedStepId;
        if (n.selected === shouldSelect) return n;
        changed = true;
        return { ...n, selected: shouldSelect };
      });
      return changed ? next : prev;
    });
  }, [selectedStepId]);

  const [edges, setEdges] = useState<Edge[]>([]);

  // Rebuild edges when transitions or node positions change, preserving selection
  useEffect(() => {
    const raw = transitionsToEdges(flow.transitions);
    const handles = assignAllHandles(nodes, raw);
    setEdges((prev) => {
      const selSet = new Set(prev.filter((e) => e.selected).map((e) => e.id));
      return raw.map((e) => {
        const h = handles.get(e.id);
        const withHandles = h ? { ...e, sourceHandle: h.sourceHandle, targetHandle: h.targetHandle } : e;
        return selSet.has(e.id) ? { ...withHandles, selected: true } : withHandles;
      });
    });
  }, [flow.transitions, nodes]);

  const onNodesChange: OnNodesChange = useCallback(
    (changes) => {
      const hasSelect = changes.some((c) => c.type === "select");
      setNodes((prev) => {
        const updated = applyNodeChanges(changes, prev);
        if (hasSelect) {
          const selected = updated.filter((n) => n.selected);
          if (selected.length === 1) {
            onSelectStep(selected[0].id);
          } else {
            onSelectStep(null);
          }
          if (selected.length > 0) {
            setSelectedEdgeId(null);
            setEdges((prevEdges) => prevEdges.map((e) => e.selected ? { ...e, selected: false } : e));
          }
        }
        let changed = updated.length !== prev.length;
        if (!changed) {
          for (let i = 0; i < updated.length; i++) {
            if (updated[i] !== prev[i]) { changed = true; break; }
          }
        }
        return changed ? updated : prev;
      });
    },
    [onSelectStep],
  );

  // Persist positions on drag end
  const onNodeDragStop = useCallback(
    (_: unknown, node: Node) => {
      const snapped = { x: Math.round(node.position.x / 20) * 20, y: Math.round(node.position.y / 20) * 20 };
      // Update flow data — this will NOT cause a full node rebuild since step IDs haven't changed
      onUpdate({
        ...flow,
        steps: flow.steps.map((s) =>
          s.id === node.id ? { ...s, position: snapped } : s,
        ),
      });
    },
    [flow, onUpdate],
  );

  const onEdgesChange: OnEdgesChange = useCallback(
    (changes) => {
      for (const c of changes) {
        if (c.type === "select") {
          setSelectedEdgeId(c.selected ? c.id : null);
        }
      }
      const removeIds = new Set(
        changes.filter((c) => c.type === "remove").map((c) => c.id),
      );
      if (removeIds.size > 0) {
        onUpdate({
          ...flow,
          transitions: flow.transitions.filter(
            (_, i) => !removeIds.has(`st-${flow.transitions[i].source}-${flow.transitions[i].target}-${i}`),
          ),
        });
        setSelectedEdgeId(null);
      }
      setEdges((prev) => applyEdgeChanges(changes, prev));
    },
    [flow, onUpdate],
  );

  const onConnect: OnConnect = useCallback(
    (connection) => {
      if (!connection.source || !connection.target) return;
      const exists = flow.transitions.some(
        (t) => t.source === connection.source && t.target === connection.target,
      );
      if (exists) return;
      // Prevent cycles
      const adj = new Map<string, string[]>();
      for (const s of flow.steps) adj.set(s.id, []);
      for (const t of flow.transitions) {
        adj.get(t.source)?.push(t.target);
      }
      const visited = new Set<string>();
      const stack = [connection.target];
      while (stack.length > 0) {
        const cur = stack.pop()!;
        if (cur === connection.source) return;
        if (visited.has(cur)) continue;
        visited.add(cur);
        for (const next of adj.get(cur) ?? []) stack.push(next);
      }
      onUpdate({
        ...flow,
        transitions: [
          ...flow.transitions,
          { source: connection.source, target: connection.target },
        ],
      });
    },
    [flow, onUpdate],
  );

  const { screenToFlowPosition } = useReactFlow();

  const addStep = useCallback((screenPos?: { x: number; y: number }) => {
    const maxNum = flow.steps
      .map((s) => s.id.replace("step-", ""))
      .map(Number)
      .filter((n) => !isNaN(n))
      .reduce((m, n) => Math.max(m, n), 0);
    const center = screenToFlowPosition(screenPos ?? {
      x: window.innerWidth / 2,
      y: window.innerHeight / 2,
    });
    const snapped = {
      x: Math.round(center.x / 20) * 20,
      y: Math.round(center.y / 20) * 20,
    };
    const newStep: FlowStep = {
      id: `step-${maxNum + 1}`,
      position: snapped,
    };
    onUpdate({ ...flow, steps: [...flow.steps, newStep] });
    onSelectStep(newStep.id);
  }, [flow, onUpdate, onSelectStep, screenToFlowPosition]);

  const deleteSelected = useCallback(() => {
    const selectedNodeIds = new Set(nodes.filter((n) => n.selected).map((n) => n.id));
    const selectedEdgeIds = new Set(edges.filter((e) => e.selected).map((e) => e.id));

    if (selectedNodeIds.size === 0 && selectedEdgeIds.size === 0) return;

    onUpdate({
      ...flow,
      steps: flow.steps.filter((s) => !selectedNodeIds.has(s.id)),
      transitions: flow.transitions.filter(
        (t, i) =>
          !selectedNodeIds.has(t.source) &&
          !selectedNodeIds.has(t.target) &&
          !selectedEdgeIds.has(`st-${t.source}-${t.target}-${i}`),
      ),
    });
    onSelectStep(null);
    setSelectedEdgeId(null);
  }, [flow, nodes, edges, onUpdate, onSelectStep]);

  // Find selected transition by edge ID (format: st-{source}-{target}-{index})
  const selectedTransitionIndex = useMemo(() => {
    if (!selectedEdgeId) return -1;
    return flow.transitions.findIndex(
      (t, i) => `st-${t.source}-${t.target}-${i}` === selectedEdgeId,
    );
  }, [selectedEdgeId, flow.transitions]);

  const updateTransitionLabel = useCallback(
    (label: string) => {
      if (selectedTransitionIndex < 0) return;
      onUpdate({
        ...flow,
        transitions: flow.transitions.map((t, i) =>
          i === selectedTransitionIndex ? { ...t, label: label || undefined } : t,
        ),
      });
    },
    [flow, selectedTransitionIndex, onUpdate],
  );

  // Expose actions to parent via ref
  useImperativeHandle(ref, () => ({
    addStep,
    deleteSelected,
    updateTransitionLabel,
  }), [addStep, deleteSelected, updateTransitionLabel]);

  // Context menu
  const [contextMenu, setContextMenu] = useState<{ x: number; y: number } | null>(null);

  useEffect(() => {
    if (!contextMenu) return;
    const close = () => setContextMenu(null);
    const onKey = (e: KeyboardEvent) => { if (e.key === "Escape") close(); };
    window.addEventListener("pointerdown", close);
    window.addEventListener("keydown", onKey);
    return () => { window.removeEventListener("pointerdown", close); window.removeEventListener("keydown", onKey); };
  }, [contextMenu]);

  const handleContextMenu = useCallback((e: React.MouseEvent) => {
    const target = e.target as HTMLElement;
    if (!target.closest(".react-flow__pane")) return;
    e.preventDefault();
    setContextMenu({ x: e.clientX, y: e.clientY });
  }, []);

  // Report selection state to parent
  const hasNodeSelection = nodes.some((n) => n.selected);
  useEffect(() => {
    onSelectionInfo({ hasNodeSelection, selectedTransitionIndex });
  }, [hasNodeSelection, selectedTransitionIndex, onSelectionInfo]);

  return (
    <div className="flex-1 relative" onContextMenu={handleContextMenu}>
      <ReactFlow
        nodes={nodes}
        edges={edges}
        nodeTypes={flowNodeTypes}
        edgeTypes={edgeTypes}
        onNodesChange={onNodesChange}
        onEdgesChange={onEdgesChange}
        onConnect={onConnect}
        onNodeDragStop={onNodeDragStop}
        onPaneClick={() => {
          setSelectedEdgeId(null);
          onSelectStep(null);
        }}
        connectionMode={ConnectionMode.Loose}
        defaultEdgeOptions={defaultEdgeOptions}
        selectionOnDrag
        multiSelectionKeyCode="Shift"
        deleteKeyCode="Delete"
        snapToGrid
        snapGrid={[20, 20]}
        colorMode="system"
        proOptions={{ hideAttribution: true }}
        fitView
        fitViewOptions={{ padding: 0.3 }}
      >
        <Background gap={20} variant={BackgroundVariant.Dots} size={1} color="var(--grid-color, #e4e4e7)" />
        <Panel position="top-center" className="!mt-3">
          <div className="flex items-center gap-0.5 rounded-lg border border-zinc-200/80 bg-white/80 backdrop-blur-sm shadow-sm px-1 py-0.5 dark:border-zinc-700/80 dark:bg-zinc-900/80">
            {selectedTransitionIndex >= 0 && !hasNodeSelection && (
              <input
                className="w-28 rounded bg-zinc-100 dark:bg-zinc-800 px-2 py-1 text-xs text-zinc-700 dark:text-zinc-200 outline-none placeholder:text-zinc-400 dark:placeholder:text-zinc-500"
                value={flow.transitions[selectedTransitionIndex]?.label ?? ""}
                maxLength={30}
                placeholder="Edge label..."
                onChange={(e) => updateTransitionLabel(e.target.value)}
              />
            )}
            {(hasNodeSelection || selectedTransitionIndex >= 0) && (
              <Button variant="ghost" color="danger" onClick={deleteSelected}>
                <Trash2 className="h-3 w-3" />
                delete
              </Button>
            )}
            <Button variant="ghost" onClick={() => addStep()}>
              <Plus className="h-3 w-3" />
              step
            </Button>
          </div>
        </Panel>
        <Controls />
      </ReactFlow>
      <div className="absolute top-3 left-3 z-10">
        <FlowGuidePanel
          stepCount={flow.steps.length}
          transitionCount={flow.transitions.length}
          stepsWithDescription={flow.steps.filter((s) => (s.description ?? "").trim().length > 0).length}
        />
      </div>
      {contextMenu && (
        <div
          className="fixed z-50 min-w-[140px] rounded-lg border border-zinc-200/80 bg-white/80 shadow-sm backdrop-blur-sm dark:border-zinc-700/80 dark:bg-zinc-900/80 py-1"
          style={{ left: contextMenu.x, top: contextMenu.y }}
          onPointerDown={(e) => e.stopPropagation()}
        >
          <button type="button" className="flex items-center gap-2 w-full px-3 py-1.5 text-xs text-zinc-600 hover:bg-zinc-100 dark:text-zinc-300 dark:hover:bg-zinc-700 cursor-pointer transition-colors" onClick={() => { addStep(contextMenu); setContextMenu(null); }}>
            <Plus className="h-3 w-3" /> Add step
          </button>
        </div>
      )}
    </div>
  );
});
