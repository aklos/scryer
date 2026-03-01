import { useCallback, useEffect, useMemo, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { C4ModelData, C4Node, C4Edge, StartingLevel, SourceLocation, Group, Contract, Flow, FlowStep, FlowTransition } from "../types";
import { useToast } from "../Toast";

/** Migrate old guidelines/string contract fields to string[] contract fields. */
function migrateContract(raw: unknown): Contract {
  const empty: Contract = { expect: [], ask: [], never: [] };
  if (!raw || typeof raw !== "object") return empty;
  const obj = raw as Record<string, unknown>;
  const migrate = (v: unknown): string[] =>
    Array.isArray(v) ? v.filter((x): x is string => typeof x === "string") : typeof v === "string" && v.length > 0 ? v.split("\n").map(s => s.trim()).filter(Boolean) : [];
  return {
    expect: migrate(obj.expect ?? obj.always),
    ask: migrate(obj.ask),
    never: migrate(obj.never),
  };
}

/** Migrate old flat steps+transitions into topologically ordered steps (no branches). */
function migrateFlowTransitions(steps: FlowStep[], transitions: FlowTransition[]): FlowStep[] {
  if (!transitions || transitions.length === 0) return steps;

  // Build adjacency from transitions
  const stepIds = new Set(steps.map((s) => s.id));
  const adj = new Map<string, string[]>();
  const inDegree = new Map<string, number>();
  for (const s of steps) { adj.set(s.id, []); inDegree.set(s.id, 0); }
  for (const t of transitions) {
    if (stepIds.has(t.source) && stepIds.has(t.target)) {
      adj.get(t.source)!.push(t.target);
      inDegree.set(t.target, (inDegree.get(t.target) ?? 0) + 1);
    }
  }

  // Topological sort (Kahn's algorithm)
  const queue = steps.filter((s) => (inDegree.get(s.id) ?? 0) === 0).map((s) => s.id);
  const sorted: string[] = [];
  while (queue.length > 0) {
    const id = queue.shift()!;
    sorted.push(id);
    for (const next of adj.get(id) ?? []) {
      const d = (inDegree.get(next) ?? 1) - 1;
      inDegree.set(next, d);
      if (d === 0) queue.push(next);
    }
  }
  // Append any disconnected/cyclic steps
  for (const s of steps) {
    if (!sorted.includes(s.id)) sorted.push(s.id);
  }

  const stepMap = new Map(steps.map((s) => [s.id, s]));
  // Return steps in topological order, stripping position (no longer used)
  return sorted.map((id) => {
    const s = stepMap.get(id)!;
    const { position: _pos, ...rest } = s as FlowStep & { position?: unknown };
    return rest;
  });
}

/** Parse raw JSON into a typed C4ModelData. */
export function parseModelData(raw: string): C4ModelData {
  const data = JSON.parse(raw);
  // Ensure operation/process/model nodes have the correct ReactFlow type + migrate fields
  const nodes: C4Node[] = (data.nodes ?? []).map((n: Record<string, unknown>) => {
    const nodeData = n.data as Record<string, unknown>;
    const kind = nodeData.kind as string;
    const expectedType = kind === "operation" ? "operation" : kind === "process" ? "process" : kind === "model" ? "model" : "c4";
    // Migrate guidelines→contract and array→string
    const rawContract = nodeData.contract ?? nodeData.guidelines;
    const contract = rawContract ? migrateContract(rawContract) : undefined;
    // Migrate references→sources
    const sources = nodeData.sources ?? nodeData.references;
    // Person and external system nodes never have status
    const stripStatus = kind === "person" || (kind === "system" && nodeData.external);
    // Detect nodes with no position (null/undefined from Rust Option<Position>)
    const hasPosition = n.position != null && typeof n.position === "object";
    const position = hasPosition ? n.position as { x: number; y: number } : { x: 0, y: 0 };
    const patched = { ...n, type: expectedType, position, data: { ...nodeData, contract, sources, guidelines: undefined, references: undefined, ...(stripStatus ? { status: undefined } : {}), ...(!hasPosition ? { _needsLayout: true } : {}) } };
    return patched as unknown as C4Node;
  });

  return {
    nodes,
    edges: data.edges ?? [],
    startingLevel: data.startingLevel ?? "system",
    sourceMap: data.sourceMap ?? {},
    projectPath: data.projectPath,
    refPositions: data.refPositions ?? {},
    groups: (data.groups ?? []).map((g: Record<string, unknown>) => ({ ...g, kind: g.kind ?? "deployment" })),
    contract: migrateContract(data.contract ?? data.guidelines),
    flows: (data.flows ?? data.scenarios ?? []).map((f: Record<string, unknown>) => {
      const steps = (f.steps ?? []) as FlowStep[];
      const transitions = (f.transitions ?? []) as FlowTransition[];
      return {
        ...f,
        steps: migrateFlowTransitions(steps, transitions),
        transitions: undefined,
      } as unknown as Flow;
    }),
  };
}

export interface ModelStorageSetters {
  setNodes: React.Dispatch<React.SetStateAction<C4Node[]>>;
  setEdges: React.Dispatch<React.SetStateAction<C4Edge[]>>;
  setStartingLevel: React.Dispatch<React.SetStateAction<StartingLevel>>;
  setSourceMap: React.Dispatch<React.SetStateAction<Record<string, SourceLocation[]>>>;
  setProjectPath: React.Dispatch<React.SetStateAction<string | undefined>>;
  setRefPositions: React.Dispatch<React.SetStateAction<Record<string, { x: number; y: number }>>>;
  setGroups: React.Dispatch<React.SetStateAction<Group[]>>;
  setContract: React.Dispatch<React.SetStateAction<Contract>>;
  setFlows: React.Dispatch<React.SetStateAction<Flow[]>>;
  setCurrentModel: React.Dispatch<React.SetStateAction<string | null>>;
  setExpandedPath: React.Dispatch<React.SetStateAction<string[]>>;
  setActiveFlowId: React.Dispatch<React.SetStateAction<string | null>>;
  setModelList: React.Dispatch<React.SetStateAction<string[]>>;
  setTemplateList: React.Dispatch<React.SetStateAction<string[]>>;
}

export interface ModelStorageState {
  nodes: C4Node[];
  edges: C4Edge[];
  currentModel: string | null;
  startingLevel: StartingLevel;
  sourceMap: Record<string, SourceLocation[]>;
  projectPath: string | undefined;
  refPositions: Record<string, { x: number; y: number }>;
  groups: Group[];
  contract: Contract;
  flows: Flow[];
}

export function useModelStorage(
  state: ModelStorageState,
  setters: ModelStorageSetters,
  scheduleFitView: () => void,
) {
  const { nodes, edges, currentModel, startingLevel, sourceMap, projectPath, refPositions, groups, contract, flows } = state;
  const {
    setNodes, setEdges, setStartingLevel, setSourceMap, setProjectPath,
    setRefPositions, setGroups, setContract,
    setFlows, setCurrentModel, setExpandedPath, setActiveFlowId,
    setModelList, setTemplateList,
  } = setters;

  const { toast } = useToast();
  const saveTimer = useRef<ReturnType<typeof setTimeout>>(null);
  const skipSave = useRef(false);
  const reloadTimer = useRef<ReturnType<typeof setTimeout>>(null);
  const lastWriteAt = useRef(0); // timestamp of our last write to disk
  const nodesRef = useRef(nodes);
  nodesRef.current = nodes;

  // Load model list, templates on mount
  useEffect(() => {
    invoke<string[]>("list_models").then(setModelList).catch(() => toast("Failed to load model list"));
    invoke<string[]>("list_templates").then(setTemplateList).catch(() => {});
  }, [setModelList, setTemplateList]);

  // Stable fingerprint of saveable node data — ignores selection/measured/transient changes.
  // Only changes when actual model data (positions, data fields, parentId) changes.
  const nodeFingerprint = useMemo(() => {
    return nodes.map((n) => {
      const { _needsLayout, ...data } = n.data;
      return `${n.id}:${n.parentId ?? ""}:${n.position.x},${n.position.y}:${n.type}:${JSON.stringify(data)}`;
    }).join("|");
  }, [nodes]);

  // Auto-save with debounce
  useEffect(() => {
    if (!currentModel || skipSave.current) {
      skipSave.current = false;
      return;
    }
    if (saveTimer.current) clearTimeout(saveTimer.current);
    saveTimer.current = setTimeout(() => {
      saveTimer.current = null;
      // Strip transient _needsLayout flag before persisting
      const cleanNodes = nodes.map((n) => {
        if (!n.data._needsLayout) return n;
        const { _needsLayout, ...data } = n.data;
        return { ...n, data };
      });
      const data: C4ModelData = { nodes: cleanNodes as C4Node[], edges, startingLevel, sourceMap, projectPath, refPositions, groups, contract, flows };
      lastWriteAt.current = Date.now();
      invoke("write_model", { name: currentModel, data: JSON.stringify(data) }).catch(() => toast("Failed to save model"));
    }, 500);
    return () => {
      if (saveTimer.current) clearTimeout(saveTimer.current);
    };
  }, [nodeFingerprint, edges, currentModel, startingLevel, sourceMap, refPositions, groups, contract, flows]);

  const refreshList = useCallback(async () => {
    const list = await invoke<string[]>("list_models").catch(() => { toast("Failed to refresh model list"); return []; });
    setModelList(list);
  }, [setModelList]);

  const applyModelData = useCallback((data: C4ModelData, preserveSelection = false) => {
    skipSave.current = true;
    if (preserveSelection) {
      // Merge incoming node data while keeping current selection and measured state
      setNodes((prev) => {
        const prevMap = new Map(prev.map((n) => [n.id, n]));
        return data.nodes.map((n) => {
          const existing = prevMap.get(n.id);
          if (existing) {
            return { ...n, selected: existing.selected, measured: existing.measured };
          }
          return n;
        });
      });
    } else {
      setNodes(data.nodes);
    }
    skipSave.current = true;
    setEdges(data.edges);
    setStartingLevel(data.startingLevel ?? "system");
    setSourceMap(data.sourceMap ?? {});
    setProjectPath(data.projectPath);
    setGroups(data.groups ?? []);
    setContract(data.contract ?? { expect: [], ask: [], never: [] });
    setFlows(data.flows ?? []);
  }, [setNodes, setEdges, setStartingLevel, setSourceMap, setProjectPath, setGroups, setContract, setFlows]);

  const loadModel = useCallback(async (name: string) => {
    try {
      const raw = await invoke<string>("read_model", { name });
      const data = parseModelData(raw);
      applyModelData(data);
      setCurrentModel(name);
      setExpandedPath([]);
      setActiveFlowId(null);
      setRefPositions(data.refPositions ?? {});
      scheduleFitView();
    } catch {
      toast("Failed to load model");
    }
  }, [applyModelData, setCurrentModel, setExpandedPath, setActiveFlowId, setRefPositions, scheduleFitView, toast]);

  const reloadModel = useCallback(async (name: string) => {
    try {
      const raw = await invoke<string>("read_model", { name });
      const data = parseModelData(raw);

      // Diff old vs new nodes to find where changes occurred
      const oldNodes = nodesRef.current;
      const oldMap = new Map(oldNodes.map((n) => [n.id, n]));
      const newMap = new Map(data.nodes.map((n) => [n.id, n]));

      // Collect parentIds of changed/added/deleted nodes
      const changedParents = new Map<string, number>();
      const bumpParent = (parentId: string | undefined) => {
        const key = parentId ?? "";
        changedParents.set(key, (changedParents.get(key) ?? 0) + 1);
      };

      for (const n of data.nodes) {
        const old = oldMap.get(n.id);
        if (!old) {
          // New node
          bumpParent(n.parentId);
        } else if ((() => {
          // Strip transient fields before comparing
          const { _needsLayout: _a, ...oldData } = old.data;
          const { _needsLayout: _b, ...newData } = n.data;
          return JSON.stringify(oldData) !== JSON.stringify(newData) || old.parentId !== n.parentId;
        })()) {
          // Changed node
          bumpParent(n.parentId);
        }
      }
      for (const n of oldNodes) {
        if (!newMap.has(n.id)) {
          // Deleted node
          bumpParent(n.parentId);
        }
      }

      applyModelData(data, true);
      setRefPositions(data.refPositions ?? {});

      // Auto-navigate to the changed level.
      // Skip when the model was empty — AI typically defines systems + containers
      // together on first set_model, so stay at system context level.
      // When changes span multiple depths (e.g. components + operations), go to
      // the shallowest — showing the broader context is more useful than drilling
      // into one component's code level.
      const wasEmpty = oldNodes.length === 0;
      if (changedParents.size > 0 && !wasEmpty) {
        const nodeById = new Map(data.nodes.map((n) => [n.id, n]));
        let bestParentId = "";
        let bestDepth = Infinity;
        let bestCount = 0;

        for (const [parentId, count] of changedParents) {
          if (!parentId) {
            // Changes at root level (top-level nodes) — skip, no navigation needed
            continue;
          }
          let depth = 0;
          let cur = parentId;
          while (cur) {
            depth++;
            const parent = nodeById.get(cur);
            if (!parent?.parentId) break;
            cur = parent.parentId;
          }
          if (depth < bestDepth || (depth === bestDepth && count > bestCount)) {
            bestParentId = parentId;
            bestDepth = depth;
            bestCount = count;
          }
        }

        if (bestParentId) {
          // Build expandedPath by walking up from the target parent
          const path: string[] = [];
          let cur: string | undefined = bestParentId;
          while (cur) {
            path.unshift(cur);
            const parent = nodeById.get(cur);
            cur = parent?.parentId;
          }
          setExpandedPath(path);
          setActiveFlowId(null);
          scheduleFitView();
        }
      }
    } catch {
      // Silently ignore — model may have been deleted externally
    }
  }, [applyModelData, setRefPositions, setExpandedPath, setActiveFlowId, scheduleFitView]);

  const deleteModel = useCallback(async (name: string) => {
    await invoke("delete_model", { name }).catch(() => toast("Failed to delete model"));
    if (currentModel === name) {
      skipSave.current = true;
      setNodes([]);
      skipSave.current = true;
      setEdges([]);
      setCurrentModel(null);
      setExpandedPath([]);
      setRefPositions({});
      setSourceMap({});
      setProjectPath(undefined);
      setGroups([]);
      setContract({ expect: [], ask: [], never: [] });
      setFlows([]);
      setActiveFlowId(null);
    }
    await refreshList();
  }, [currentModel, refreshList, setNodes, setEdges, setCurrentModel, setExpandedPath, setRefPositions, setSourceMap, setProjectPath, setGroups, setContract, setFlows, setActiveFlowId]);

  const newModel = useCallback(() => {
    skipSave.current = true;
    setNodes([]);
    skipSave.current = true;
    setEdges([]);
    setCurrentModel(null);
    setStartingLevel("system");
    setExpandedPath([]);
    setRefPositions({});
    setSourceMap({});
    setProjectPath(undefined);
    setGroups([]);
    setContract({ expect: [], ask: [], never: [] });
    setFlows([]);
    setActiveFlowId(null);
  }, [setNodes, setEdges, setCurrentModel, setStartingLevel, setExpandedPath, setRefPositions, setSourceMap, setProjectPath, setGroups, setContract, setFlows, setActiveFlowId]);

  const loadTemplate = useCallback(async (templateName: string) => {
    try {
      const raw = await invoke<string>("load_template", { name: templateName });
      const data = parseModelData(raw);
      applyModelData(data);
      setCurrentModel(null);
      setExpandedPath([]);
      setRefPositions({});
      setActiveFlowId(null);
      scheduleFitView();
    } catch {
      toast("Failed to load template");
    }
  }, [applyModelData, setCurrentModel, setExpandedPath, setRefPositions, setActiveFlowId, scheduleFitView, toast]);

  const saveModelAs = useCallback(async (name: string) => {
    const data: C4ModelData = { nodes, edges, startingLevel, sourceMap, refPositions, groups, contract, flows };
    await invoke("write_model", { name, data: JSON.stringify(data) }).catch(() => toast("Failed to save model"));
    setCurrentModel(name);
    await refreshList();
  }, [nodes, edges, startingLevel, sourceMap, refPositions, groups, contract, flows, refreshList, setCurrentModel]);

  // Auto-open models created externally (e.g. by MCP agent)
  useEffect(() => {
    const unlisten = listen<string>("model-created", (event) => {
      loadModel(event.payload);
    });
    return () => { unlisten.then((fn) => fn()); };
  }, [loadModel]);

  // File watcher: reload when external tools modify model files.
  // If a save is pending (saveTimer active), the file change is from the UI — skip reload.
  useEffect(() => {
    const unlisten = listen<string>("model-changed", (event) => {
      const name = event.payload;
      // If the UI has a pending save, this event is likely from our own write — skip reload
      if (saveTimer.current) {
        return;
      }
      skipSave.current = true;
      if (reloadTimer.current) clearTimeout(reloadTimer.current);
      reloadTimer.current = setTimeout(() => {
        if (name === currentModel) {
          reloadModel(name);
        }
        refreshList();
      }, 300);
    });
    return () => {
      unlisten.then((fn) => fn());
      if (reloadTimer.current) clearTimeout(reloadTimer.current);
    };
  }, [currentModel, reloadModel, refreshList]);

  return {
    loadModel,
    reloadModel,
    deleteModel,
    newModel,
    loadTemplate,
    saveModelAs,
    refreshList,
  };
}
