import { useCallback, useMemo, useRef, useState } from "react";
import { Plus, Trash2, GitBranch, GripVertical } from "lucide-react";
import { MentionTextarea, type MentionItem } from "./MentionTextarea";
import { DescriptionText, type MentionNodeInfo } from "./DescriptionText";
import type {
  C4Node,
  C4NodeData,
  Flow,
  FlowStep,
  FlowBranch,
} from "./types";


function buildNodeMentions(
  allNodes: C4Node[],
  flow: Flow,
  labels: Map<string, string>,
): {
  mentionItems: MentionItem[];
  nodeMap: Map<string, MentionNodeInfo>;
  stepIdToLabel: Map<string, string>;
} {
  const mentionItems: MentionItem[] = [];
  const nodeMap = new Map<string, MentionNodeInfo>();
  const stepIdToLabel = new Map<string, string>();
  for (const n of allNodes) {
    const d = n.data as C4NodeData;
    const k = d.kind;
    if (k === "process") {
      nodeMap.set(d.name, { kind: k, status: d.status });
      mentionItems.push({ name: d.name, kind: k });
    }
  }
  // Add flow steps — insert step ID in text, show "Step N" in dropdown
  const addSteps = (steps: FlowStep[]) => {
    for (const step of steps) {
      const label = labels.get(step.id) ?? "?";
      const displayName = `Step ${label}`;
      stepIdToLabel.set(step.id, displayName);
      nodeMap.set(step.id, { kind: "step" });
      mentionItems.push({
        name: displayName,
        insertValue: step.id,
        kind: "step",
      });
      if (step.branches) {
        for (const b of step.branches) addSteps(b.steps);
      }
    }
  };
  addSteps(flow.steps);
  return { mentionItems, nodeMap, stepIdToLabel };
}

/** Count all steps recursively including branch sub-steps */
export function countAllSteps(steps: FlowStep[]): number {
  let count = 0;
  for (const step of steps) {
    count++;
    if (step.branches) {
      for (const branch of step.branches) {
        count += countAllSteps(branch.steps);
      }
    }
  }
  return count;
}

function computeNumbering(
  steps: FlowStep[],
  prefix: string,
  startNum: number,
): Map<string, string> {
  const labels = new Map<string, string>();
  let num = startNum;
  for (const step of steps) {
    labels.set(step.id, `${prefix}${num}`);
    if (step.branches && step.branches.length > 0) {
      const letter = (i: number) => String.fromCharCode(97 + i); // a, b, c...
      for (let bi = 0; bi < step.branches.length; bi++) {
        const branchLabels = computeNumbering(
          step.branches[bi].steps,
          `${prefix}${num}${letter(bi)}.`,
          1,
        );
        for (const [id, label] of branchLabels) labels.set(id, label);
      }
    }
    num++;
  }
  return labels;
}

function nextStepId(flow: Flow): string {
  const allIds = collectAllStepIds(flow.steps);
  const max = allIds
    .map((id) => id.replace("step-", ""))
    .map(Number)
    .filter((n) => !isNaN(n))
    .reduce((m, n) => Math.max(m, n), 0);
  return `step-${max + 1}`;
}

function collectAllStepIds(steps: FlowStep[]): string[] {
  const ids: string[] = [];
  for (const step of steps) {
    ids.push(step.id);
    if (step.branches) {
      for (const branch of step.branches) {
        ids.push(...collectAllStepIds(branch.steps));
      }
    }
  }
  return ids;
}

// --- Step Card ---

function StepCard({
  step,
  stepLabel,
  labels,
  mentionItems,
  nodeMap,
  onUpdateStep,
  onDeleteStep,
  onAddBranches,
  onUpdateBranch,
  onAddStepToBranch,
  onDeleteBranch,
  onAddBranchArm,
  flow,
  editingStepId,
  setEditingStepId,
  stepIdToLabel,
  highlightedStepId,
  setHighlightedStepId,
  dragSourceRef,
  dragOverId,
  onDragStart,
  onDragOver,
  onDragEnd,
}: {
  step: FlowStep;
  stepLabel: string;
  labels: Map<string, string>;
  mentionItems: MentionItem[];
  nodeMap: Map<string, MentionNodeInfo>;
  onUpdateStep: (id: string, updates: Partial<FlowStep>) => void;
  onDeleteStep: (id: string) => void;
  onAddBranches: (stepId: string) => void;
  onUpdateBranch: (
    stepId: string,
    branchIndex: number,
    updates: Partial<FlowBranch>,
  ) => void;
  onAddStepToBranch: (stepId: string, branchIndex: number) => void;
  onDeleteBranch: (stepId: string, branchIndex: number) => void;
  onAddBranchArm: (stepId: string) => void;
  flow: Flow;
  editingStepId: string | null;
  setEditingStepId: (id: string | null) => void;
  stepIdToLabel: Map<string, string>;
  highlightedStepId: string | null;
  setHighlightedStepId: (id: string | null) => void;
  dragSourceRef: React.RefObject<string | null>;
  dragOverId: string | null;
  onDragStart: (e: React.DragEvent, stepId: string) => void;
  onDragOver: (e: React.DragEvent, stepId: string) => void;
  onDragEnd: () => void;
}) {
  const editing = editingStepId === step.id;
  const highlighted = highlightedStepId === step.id;

  return (
    <div
      className={
        dragOverId === step.id
          ? "border-t-2 border-zinc-400 dark:border-zinc-500 -mt-0.5 pt-0.5"
          : ""
      }
      draggable
      onDragStart={(e) => {
        if (dragSourceRef.current !== step.id) {
          e.preventDefault();
          return;
        }
        e.stopPropagation();
        onDragStart(e, step.id);
      }}
      onDragOver={(e) => {
        e.stopPropagation();
        onDragOver(e, step.id);
      }}
      onDragEnd={(e) => {
        e.stopPropagation();
        onDragEnd();
      }}
    >
      <div
        className={`w-[480px] text-left rounded-lg border px-3 py-2.5 transition-colors group ${
          highlighted
            ? "border-zinc-400 dark:border-zinc-300 ring-1 ring-zinc-400/30 dark:ring-zinc-300/20"
            : "border-zinc-200 bg-white hover:border-zinc-300 dark:border-zinc-700/80 dark:bg-zinc-900 dark:hover:border-zinc-500"
        }`}
      >
        <div className="flex items-start gap-2">
          {/* Drag handle */}
          <span
            className="shrink-0 flex items-center cursor-grab active:cursor-grabbing text-zinc-300 hover:text-zinc-400 dark:text-zinc-600 dark:hover:text-zinc-500 mt-0.5 -ml-1"
            onMouseDown={() => {
              dragSourceRef.current = step.id;
            }}
          >
            <GripVertical size={12} />
          </span>

          {/* Step number */}
          <span className="shrink-0 flex items-center justify-center min-w-5 h-5 rounded-full bg-zinc-100 dark:bg-zinc-800 px-1.5 text-[10px] font-semibold text-zinc-500 dark:text-zinc-400 mt-px">
            {stepLabel}
          </span>

          {/* Description */}
          <div className="flex-1 min-w-0">
            {editing ? (
              <div className="mt-0.5">
                <MentionTextarea
                  value={step.description ?? ""}
                  mentionNames={mentionItems}
                  placeholder="e.g. System validates credentials"
                  rows={1}
                  autoSize
                  autoFocus
                  maxLength={400}
                  className="w-full bg-transparent px-0 py-0 outline-none text-[11px] leading-relaxed text-zinc-700 dark:text-zinc-200 resize-none overflow-hidden placeholder:text-zinc-400 dark:placeholder:text-zinc-500 placeholder:italic"
                  onChange={(val) => {
                    onUpdateStep(step.id, { description: val || undefined });
                  }}
                />
                <div className="text-right text-[9px] text-zinc-400 dark:text-zinc-600 mt-0.5">
                  {(step.description ?? "").length}/400
                </div>
              </div>
            ) : (
              <div
                className={`mt-0.5 text-[11px] leading-relaxed break-words cursor-text min-h-[20px] ${
                  step.description
                    ? "text-zinc-500 dark:text-zinc-400"
                    : "text-zinc-400 dark:text-zinc-500 italic"
                }`}
                onClick={() => setEditingStepId(step.id)}
              >
                {step.description ? (
                  <DescriptionText
                    text={step.description}
                    nodeMap={nodeMap}
                    resolveMap={stepIdToLabel}
                    onMentionClick={(name) => setHighlightedStepId(name)}
                    onMentionHover={(name) => setHighlightedStepId(name)}
                  />
                ) : (
                  "Empty step"
                )}
              </div>
            )}

          </div>

          {/* Actions — hover reveal */}
          <div className="shrink-0 flex items-center gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity">
            {!step.branches?.length && (
              <button
                type="button"
                className="p-0.5 rounded hover:bg-zinc-100 dark:hover:bg-zinc-800 text-zinc-400 hover:text-zinc-600 dark:hover:text-zinc-300 cursor-pointer"
                title="Add branches"
                onClick={() => onAddBranches(step.id)}
              >
                <GitBranch size={12} />
              </button>
            )}
            <button
              type="button"
              className="p-0.5 rounded hover:bg-red-100 dark:hover:bg-red-900/30 text-zinc-400 hover:text-red-500 cursor-pointer"
              title="Delete step"
              onClick={() => onDeleteStep(step.id)}
            >
              <Trash2 className="h-3 w-3" />
            </button>
          </div>
        </div>
      </div>

      {/* Branches */}
      {step.branches && step.branches.length > 0 && (
        <div className="ml-6 mt-1 mb-1">
          {step.branches.map((branch, bi) => (
            <div
              key={bi}
              className="border-l-2 border-zinc-200 dark:border-zinc-700 pl-3 py-1"
            >
              {/* Branch label */}
              <div className="flex items-center gap-1 mb-1 group/branch">
                <input
                  className="text-[10px] font-mono font-medium text-zinc-400 dark:text-zinc-500 bg-transparent outline-none w-20 placeholder:text-zinc-300 dark:placeholder:text-zinc-600"
                  value={branch.condition}
                  placeholder={
                    bi === 0
                      ? "if:"
                      : bi === (step.branches?.length ?? 0) - 1
                        ? "else:"
                        : "elif:"
                  }
                  onChange={(e) =>
                    onUpdateBranch(step.id, bi, { condition: e.target.value })
                  }
                />
                <button
                  type="button"
                  className="p-0.5 rounded text-zinc-300 hover:text-red-500 dark:text-zinc-600 dark:hover:text-red-400 cursor-pointer opacity-0 group-hover/branch:opacity-100 transition-opacity"
                  title="Delete branch"
                  onClick={() => onDeleteBranch(step.id, bi)}
                >
                  <Trash2 size={10} />
                </button>
              </div>

              {/* Sub-steps */}
              <div className="space-y-1.5">
                {branch.steps.map((subStep) => (
                  <StepCard
                    key={subStep.id}
                    step={subStep}
                    stepLabel={labels.get(subStep.id) ?? "?"}
                    labels={labels}
                    mentionItems={mentionItems}
                    nodeMap={nodeMap}
                    onUpdateStep={onUpdateStep}
                    onDeleteStep={onDeleteStep}
                    onAddBranches={onAddBranches}
                    onUpdateBranch={onUpdateBranch}
                    onAddStepToBranch={onAddStepToBranch}
                    onDeleteBranch={onDeleteBranch}
                    onAddBranchArm={onAddBranchArm}
                    flow={flow}
                    editingStepId={editingStepId}
                    setEditingStepId={setEditingStepId}
                    stepIdToLabel={stepIdToLabel}
                    highlightedStepId={highlightedStepId}
                    setHighlightedStepId={setHighlightedStepId}
                    dragSourceRef={dragSourceRef}
                    dragOverId={dragOverId}
                    onDragStart={onDragStart}
                    onDragOver={onDragOver}
                    onDragEnd={onDragEnd}
                  />
                ))}
              </div>

              {/* Add step to branch */}
              <button
                type="button"
                className="flex items-center gap-1 text-[10px] text-zinc-400 hover:text-zinc-600 dark:text-zinc-500 dark:hover:text-zinc-300 cursor-pointer py-1 mt-0.5"
                onClick={() => onAddStepToBranch(step.id, bi)}
              >
                <Plus size={10} /> step
              </button>
            </div>
          ))}
          <button
            type="button"
            className="flex items-center gap-1 text-[10px] text-zinc-400 hover:text-zinc-600 dark:text-zinc-500 dark:hover:text-zinc-300 cursor-pointer py-0.5 ml-1"
            onClick={() => onAddBranchArm(step.id)}
          >
            <GitBranch size={10} /> branch
          </button>
        </div>
      )}
    </div>
  );
}

// --- Main View ---

interface FlowScriptViewProps {
  flow: Flow;
  onUpdate: (updated: Flow) => void;
  allNodes: C4Node[];
}

export function FlowScriptView({
  flow,
  onUpdate,
  allNodes,
}: FlowScriptViewProps) {
  const labels = useMemo(
    () => computeNumbering(flow.steps, "", 1),
    [flow.steps],
  );


  const { mentionItems, nodeMap, stepIdToLabel } = useMemo(
    () => buildNodeMentions(allNodes, flow, labels),
    [allNodes, flow, labels],
  );

  // --- Step CRUD ---

  const updateStepDeep = useCallback(
    (steps: FlowStep[], id: string, updates: Partial<FlowStep>): FlowStep[] => {
      return steps.map((s) => {
        if (s.id === id) return { ...s, ...updates };
        if (s.branches?.length) {
          return {
            ...s,
            branches: s.branches.map((b) => ({
              ...b,
              steps: updateStepDeep(b.steps, id, updates),
            })),
          };
        }
        return s;
      });
    },
    [],
  );

  const deleteStepDeep = useCallback(
    (steps: FlowStep[], id: string): FlowStep[] => {
      return steps
        .filter((s) => s.id !== id)
        .map((s) => {
          if (s.branches?.length) {
            return {
              ...s,
              branches: s.branches.map((b) => ({
                ...b,
                steps: deleteStepDeep(b.steps, id),
              })),
            };
          }
          return s;
        });
    },
    [],
  );

  const onUpdateStep = useCallback(
    (id: string, updates: Partial<FlowStep>) => {
      onUpdate({ ...flow, steps: updateStepDeep(flow.steps, id, updates) });
    },
    [flow, onUpdate, updateStepDeep],
  );

  const onDeleteStep = useCallback(
    (id: string) => {
      onUpdate({ ...flow, steps: deleteStepDeep(flow.steps, id) });
    },
    [flow, onUpdate, deleteStepDeep],
  );

  const onAddStepBottom = useCallback(() => {
    const newStep: FlowStep = { id: nextStepId(flow) };
    onUpdate({ ...flow, steps: [...flow.steps, newStep] });
  }, [flow, onUpdate]);

  const onAddBranches = useCallback(
    (stepId: string) => {
      onUpdateStep(stepId, {
        branches: [
          { condition: "if:", steps: [] },
          { condition: "else:", steps: [] },
        ],
      });
    },
    [onUpdateStep],
  );

  const onUpdateBranch = useCallback(
    (stepId: string, branchIndex: number, updates: Partial<FlowBranch>) => {
      const step = findStepDeep(flow.steps, stepId);
      if (!step?.branches) return;
      onUpdateStep(stepId, {
        branches: step.branches.map((b, i) =>
          i === branchIndex ? { ...b, ...updates } : b,
        ),
      });
    },
    [flow.steps, onUpdateStep],
  );

  const onAddStepToBranch = useCallback(
    (stepId: string, branchIndex: number) => {
      const step = findStepDeep(flow.steps, stepId);
      if (!step?.branches) return;
      const newStep: FlowStep = { id: nextStepId(flow) };
      onUpdateStep(stepId, {
        branches: step.branches.map((b, i) =>
          i === branchIndex ? { ...b, steps: [...b.steps, newStep] } : b,
        ),
      });
    },
    [flow, onUpdateStep],
  );

  const onDeleteBranch = useCallback(
    (stepId: string, branchIndex: number) => {
      const step = findStepDeep(flow.steps, stepId);
      if (!step?.branches) return;
      const remaining = step.branches.filter((_, i) => i !== branchIndex);
      if (remaining.length <= 1) {
        const flattenedSteps = remaining.length === 1 ? remaining[0].steps : [];
        onUpdate({
          ...flow,
          steps: flattenBranchInto(flow.steps, stepId, flattenedSteps),
        });
      } else {
        onUpdateStep(stepId, { branches: remaining });
      }
    },
    [flow, onUpdate, onUpdateStep],
  );

  const onAddBranchArm = useCallback(
    (stepId: string) => {
      const step = findStepDeep(flow.steps, stepId);
      if (!step?.branches) return;
      onUpdateStep(stepId, {
        branches: [...step.branches, { condition: "", steps: [] }],
      });
    },
    [flow.steps, onUpdateStep],
  );

  // Editing state — only one step at a time
  const [editingStepId, setEditingStepId] = useState<string | null>(null);
  // Highlight state for mention hover/click
  const [highlightedStepId, setHighlightedStepId] = useState<string | null>(
    null,
  );

  // Drag reorder
  const [dragOverId, setDragOverId] = useState<string | null>(null);
  const dragSourceRef = useRef<string | null>(null);

  const onDragStart = useCallback((e: React.DragEvent, stepId: string) => {
    e.dataTransfer.effectAllowed = "move";
    e.dataTransfer.setData("text/plain", stepId);
    dragSourceRef.current = stepId;
  }, []);

  const onDragOver = useCallback(
    (e: React.DragEvent, stepId: string) => {
      const sourceId = dragSourceRef.current;
      if (!sourceId || !areSiblings(flow.steps, sourceId, stepId)) return;
      e.preventDefault();
      e.dataTransfer.dropEffect = "move";
      setDragOverId(stepId);
    },
    [flow.steps],
  );

  const onDragEnd = useCallback(() => {
    const sourceId = dragSourceRef.current;
    const targetId = dragOverId;
    dragSourceRef.current = null;
    setDragOverId(null);
    if (!sourceId || !targetId || sourceId === targetId) return;

    onUpdate({
      ...flow,
      steps: reorderStepDeep(flow.steps, sourceId, targetId),
    });
  }, [dragOverId, flow, onUpdate]);

  // Shared props for all step cards
  const stepProps = useMemo(
    () => ({
      labels,
      mentionItems,
      nodeMap,
      onUpdateStep,
      onDeleteStep,
      onAddBranches,
      onUpdateBranch,
      onAddStepToBranch,
      onDeleteBranch,
      onAddBranchArm,
      flow,
      editingStepId,
      setEditingStepId,
      stepIdToLabel,
      highlightedStepId,
      setHighlightedStepId,
      dragSourceRef,
      dragOverId,
      onDragStart,
      onDragOver,
      onDragEnd,
    }),
    [
      labels,
      mentionItems,
      nodeMap,
      onUpdateStep,
      onDeleteStep,
      onAddBranches,
      onUpdateBranch,
      onAddStepToBranch,
      onDeleteBranch,
      onAddBranchArm,
      flow,
      editingStepId,
      stepIdToLabel,
      highlightedStepId,
      dragOverId,
      onDragStart,
      onDragOver,
      onDragEnd,
    ],
  );

  return (
    <div className="flex-1 flex flex-col overflow-hidden bg-zinc-50 dark:bg-zinc-950">
      {/* Step list */}
      <div
        className="flex-1 overflow-y-auto"
        onClick={(e) => {
          if (e.target === e.currentTarget) setEditingStepId(null);
        }}
      >
        <div className="max-w-2xl mx-auto py-8 px-6 space-y-2">
          {flow.steps.length === 0 && (
            <div className="flex flex-col items-center py-16 text-zinc-400 dark:text-zinc-500 text-xs gap-4">
              <div className="text-center space-y-1.5">
                <p className="text-sm font-medium text-zinc-500 dark:text-zinc-400">Describe a sequence</p>
                <p className="text-[11px] max-w-xs leading-relaxed">
                  Flows model user journeys, data pipelines, or any multi-step process.
                  Each step is one meaningful system interaction.
                </p>
              </div>
              <div className="text-[10px] max-w-xs space-y-1 text-zinc-400 dark:text-zinc-600">
                <p>Use <span className="font-mono bg-zinc-200/60 dark:bg-zinc-800 px-1 rounded">@</span> to reference processes or other steps</p>
                <p>Add branches for conditional paths (if/else)</p>
                <p>Drag the <span className="inline-flex align-text-bottom"><GripVertical size={10} /></span> handle to reorder</p>
              </div>
              <button
                type="button"
                className="flex items-center gap-1.5 mt-2 text-xs text-zinc-500 hover:text-zinc-700 dark:text-zinc-400 dark:hover:text-zinc-200 cursor-pointer"
                onClick={onAddStepBottom}
              >
                <Plus size={12} /> Add first step
              </button>
            </div>
          )}
          {flow.steps.map((step) => (
            <StepCard
              key={step.id}
              step={step}
              stepLabel={labels.get(step.id) ?? "?"}
              {...stepProps}
            />
          ))}

          {/* Add step button */}
          {flow.steps.length > 0 && (
            <button
              type="button"
              className="flex items-center gap-1.5 text-[11px] text-zinc-400 hover:text-zinc-600 dark:text-zinc-500 dark:hover:text-zinc-300 cursor-pointer py-1"
              onClick={onAddStepBottom}
            >
              <Plus size={12} /> Add step
            </button>
          )}
        </div>
      </div>
    </div>
  );
}

// --- Helpers ---

function findStepDeep(steps: FlowStep[], id: string): FlowStep | undefined {
  for (const s of steps) {
    if (s.id === id) return s;
    if (s.branches) {
      for (const b of s.branches) {
        const found = findStepDeep(b.steps, id);
        if (found) return found;
      }
    }
  }
  return undefined;
}

/** Check if two step IDs are siblings (in the same list) */
function areSiblings(steps: FlowStep[], idA: string, idB: string): boolean {
  const hasA = steps.some((s) => s.id === idA);
  const hasB = steps.some((s) => s.id === idB);
  if (hasA && hasB) return true;
  for (const s of steps) {
    if (s.branches) {
      for (const b of s.branches) {
        if (areSiblings(b.steps, idA, idB)) return true;
      }
    }
  }
  return false;
}

/** Reorder: move sourceId to targetId's position within the same sibling list */
function reorderStepDeep(
  steps: FlowStep[],
  sourceId: string,
  targetId: string,
): FlowStep[] {
  const sourceIdx = steps.findIndex((s) => s.id === sourceId);
  const targetIdx = steps.findIndex((s) => s.id === targetId);
  if (sourceIdx >= 0 && targetIdx >= 0) {
    const reordered = [...steps];
    const [moved] = reordered.splice(sourceIdx, 1);
    reordered.splice(targetIdx, 0, moved);
    return reordered;
  }
  // Recurse into branches
  return steps.map((s) => {
    if (!s.branches?.length) return s;
    return {
      ...s,
      branches: s.branches.map((b) => ({
        ...b,
        steps: reorderStepDeep(b.steps, sourceId, targetId),
      })),
    };
  });
}

function flattenBranchInto(
  steps: FlowStep[],
  stepId: string,
  insertSteps: FlowStep[],
): FlowStep[] {
  const result: FlowStep[] = [];
  for (const s of steps) {
    if (s.id === stepId) {
      result.push({ ...s, branches: undefined });
      result.push(...insertSteps);
    } else if (s.branches?.length) {
      result.push({
        ...s,
        branches: s.branches.map((b) => ({
          ...b,
          steps: flattenBranchInto(b.steps, stepId, insertSteps),
        })),
      });
    } else {
      result.push(s);
    }
  }
  return result;
}
