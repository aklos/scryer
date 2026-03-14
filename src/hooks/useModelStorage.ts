import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { C4ModelData, C4Node, C4NodeData, C4Edge, StartingLevel, SourceLocation, Group, Contract, ContractItem, Flow, FlowStep, FlowTransition } from "../types";
import { useToast } from "../Toast";

/** Migrate old guidelines/string contract fields to ContractItem[] contract fields. */
function migrateContract(raw: unknown): Contract {
  const empty: Contract = { expect: [], ask: [], never: [] };
  if (!raw || typeof raw !== "object") return empty;
  const obj = raw as Record<string, unknown>;
  const migrate = (v: unknown): ContractItem[] => {
    if (Array.isArray(v)) return v.filter((x): x is ContractItem => typeof x === "string" || (typeof x === "object" && x !== null && "text" in x));
    if (typeof v === "string" && v.length > 0) return v.split("\n").map(s => s.trim()).filter(Boolean);
    return [];
  };
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
    // Strip invalid/old status values (e.g. "implemented", "changed", "deprecated" from older models)
    const VALID_STATUSES = new Set(["proposed", "wip", "ready"]);
    if (nodeData.status && !VALID_STATUSES.has(nodeData.status as string)) {
      nodeData.status = undefined;
    }
    // Person and external system nodes never have status
    const stripStatus = kind === "person" || (kind === "system" && nodeData.external);
    // Migrate notes: string → string[]
    const rawNotes = nodeData.notes;
    const notes = typeof rawNotes === "string" ? (rawNotes ? rawNotes.split("\n").filter(Boolean) : undefined)
      : Array.isArray(rawNotes) && rawNotes.length > 0 ? rawNotes : undefined;
    // Detect nodes with no position (null/undefined from Rust Option<Position>)
    const hasPosition = n.position != null && typeof n.position === "object";
    const position = hasPosition ? n.position as { x: number; y: number } : { x: 0, y: 0 };
    const patched = { ...n, type: expectedType, position, data: { ...nodeData, contract, sources, notes, guidelines: undefined, references: undefined, ...(stripStatus ? { status: undefined } : {}), ...(!hasPosition ? { _needsLayout: true } : {}) } };
    return patched as unknown as C4Node;
  });

  // Deduplicate edges by ID (keep first occurrence)
  const seenEdgeIds = new Set<string>();
  const edges: C4Edge[] = (data.edges ?? []).filter((e: C4Edge) => {
    if (seenEdgeIds.has(e.id)) return false;
    seenEdgeIds.add(e.id);
    return true;
  });

  return {
    nodes,
    edges,
    startingLevel: data.startingLevel ?? "system",
    sourceMap: data.sourceMap ?? {},
    projectPath: data.projectPath,
    refPositions: data.refPositions ?? {},
    groups: (data.groups ?? []).map((g: Record<string, unknown>) => ({ ...g, kind: g.kind ?? "deployment" })),
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
  flows: Flow[];
}

export function useModelStorage(
  state: ModelStorageState,
  setters: ModelStorageSetters,
  scheduleFitView: () => void,
) {
  const { nodes, edges, currentModel, startingLevel, sourceMap, projectPath, refPositions, groups, flows } = state;
  const {
    setNodes, setEdges, setStartingLevel, setSourceMap, setProjectPath,
    setRefPositions, setGroups,
    setFlows, setCurrentModel, setExpandedPath, setActiveFlowId,
    setModelList, setTemplateList,
  } = setters;

  const { toast } = useToast();
  const saveTimer = useRef<ReturnType<typeof setTimeout>>(null);
  const skipSave = useRef(false);
  const reloadTimer = useRef<ReturnType<typeof setTimeout>>(null);
  const lastKnownDisk = useRef<string>(""); // last JSON string we wrote or loaded from disk
  const [changedNodeIds, setChangedNodeIds] = useState<Set<string>>(new Set());
  const changeClearTimer = useRef<ReturnType<typeof setTimeout>>(null);
  const nodesRef = useRef(nodes);
  nodesRef.current = nodes;
  const edgesRef = useRef(edges);
  edgesRef.current = edges;

  // "Follow AI" — auto-navigate to where the AI made changes
  const [followAI, setFollowAIState] = useState(() => {
    const stored = localStorage.getItem("scryer:followAI");
    return stored === null ? true : stored === "true";
  });
  const followAIRef = useRef(followAI);
  followAIRef.current = followAI;
  const setFollowAI = useCallback((value: boolean) => {
    setFollowAIState(value);
    localStorage.setItem("scryer:followAI", String(value));
  }, []);

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
      const data: C4ModelData = { nodes: cleanNodes as C4Node[], edges, startingLevel, sourceMap, projectPath, refPositions, groups, flows };
      const json = JSON.stringify(data);
      lastKnownDisk.current = json;
      invoke("write_model", { name: currentModel, data: json }).catch(() => toast("Failed to save model"));
    }, 500);
    return () => {
      if (saveTimer.current) clearTimeout(saveTimer.current);
    };
  }, [nodeFingerprint, edges, currentModel, startingLevel, sourceMap, refPositions, groups, flows]);

  const refreshList = useCallback(async () => {
    const list = await invoke<string[]>("list_models").catch(() => { toast("Failed to refresh model list"); return []; });
    setModelList(list);
  }, [setModelList]);

  const applyModelData = useCallback((data: C4ModelData, preserveSelection = false) => {
    skipSave.current = true;
    if (preserveSelection) {
      // Merge incoming node data while keeping current selection, measured, and
      // laid-out positions for unchanged nodes. MCP writes strip positions, so
      // parseModelData sets _needsLayout on nodes without positions. For nodes
      // that already exist in memory with a laid-out position, preserve it —
      // otherwise auto-layout work gets lost on every AI write.
      setNodes((prev) => {
        const prevMap = new Map(prev.map((n) => [n.id, n]));
        return data.nodes.map((n) => {
          const existing = prevMap.get(n.id);
          if (existing) {
            // Incoming node has _needsLayout (no position from disk) but we
            // already have a position in memory — keep it, clear the flag.
            if (n.data._needsLayout && !existing.data._needsLayout) {
              const { _needsLayout, ...data } = n.data;
              return { ...n, position: existing.position, selected: existing.selected, measured: existing.measured, data: data as C4NodeData };
            }
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
    setFlows(data.flows ?? []);
  }, [setNodes, setEdges, setStartingLevel, setSourceMap, setProjectPath, setGroups, setFlows]);

  const loadModel = useCallback(async (name: string) => {
    try {
      const raw = await invoke<string>("read_model", { name });
      lastKnownDisk.current = raw;
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
      if (raw === lastKnownDisk.current) return; // our own write — skip
      lastKnownDisk.current = raw;
      const data = parseModelData(raw);

      // Diff old vs new nodes to find where changes occurred
      const oldNodes = nodesRef.current;
      const oldMap = new Map(oldNodes.map((n) => [n.id, n]));
      const newMap = new Map(data.nodes.map((n) => [n.id, n]));

      // Collect parentIds of changed/added/deleted nodes
      const changedParents = new Map<string, number>();
      const changedIds = new Set<string>();
      const bumpParent = (parentId: string | undefined) => {
        const key = parentId ?? "";
        changedParents.set(key, (changedParents.get(key) ?? 0) + 1);
      };

      for (const n of data.nodes) {
        const old = oldMap.get(n.id);
        if (!old) {
          // New node
          bumpParent(n.parentId);
          changedIds.add(n.id);
        } else if ((() => {
          // Strip transient fields before comparing
          const { _needsLayout: _a, ...oldData } = old.data;
          const { _needsLayout: _b, ...newData } = n.data;
          return JSON.stringify(oldData) !== JSON.stringify(newData) || old.parentId !== n.parentId;
        })()) {
          // Changed node
          bumpParent(n.parentId);
          changedIds.add(n.id);
        }
      }
      for (const n of oldNodes) {
        if (!newMap.has(n.id)) {
          // Deleted node
          bumpParent(n.parentId);
        }
      }

      // Detect edge changes — topology changes should trigger re-layout
      const oldEdges = edgesRef.current;
      const edgesChanged = oldEdges.length !== data.edges.length ||
        data.edges.some((e, i) => e.id !== oldEdges[i]?.id || e.source !== oldEdges[i]?.source || e.target !== oldEdges[i]?.target);

      if (edgesChanged) {
        // Find which parent scopes have changed edges
        const affectedParents = new Set<string | undefined>();
        for (const e of data.edges) {
          if (oldEdges.every((oe) => oe.id !== e.id)) {
            // New edge — flag parents of both endpoints
            affectedParents.add(newMap.get(e.source)?.parentId);
            affectedParents.add(newMap.get(e.target)?.parentId);
          }
        }
        for (const oe of oldEdges) {
          if (data.edges.every((e) => e.id !== oe.id)) {
            // Deleted edge — flag parents of old endpoints
            affectedParents.add(newMap.get(oe.source)?.parentId ?? oldMap.get(oe.source)?.parentId);
            affectedParents.add(newMap.get(oe.target)?.parentId ?? oldMap.get(oe.target)?.parentId);
          }
        }
        // Strip positions only on nodes at affected levels
        for (const n of data.nodes) {
          if (affectedParents.has(n.parentId)) {
            n.position = { x: 0, y: 0 };
            (n.data as C4NodeData)._needsLayout = true;
          }
        }
      }

      applyModelData(data, true);
      setRefPositions(data.refPositions ?? {});

      // Flash changed nodes briefly
      if (changedIds.size > 0) {
        setChangedNodeIds(changedIds);
        if (changeClearTimer.current) clearTimeout(changeClearTimer.current);
        changeClearTimer.current = setTimeout(() => setChangedNodeIds(new Set()), 3000);
      }

      // Auto-navigate to the changed level.
      // Skip when the model was empty — AI typically defines systems + containers
      // together on first set_model, so stay at system context level.
      // When changes span multiple depths (e.g. components + operations), go to
      // the shallowest — showing the broader context is more useful than drilling
      // into one component's code level.
      const wasEmpty = oldNodes.length === 0;
      if (changedParents.size > 0 && !wasEmpty) {
        const nodeById = new Map(data.nodes.map((n) => [n.id, n]));

        if (followAIRef.current) {
          // Auto-navigate to the changed level
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
            // Don't navigate into code level (inside a component)
            const bestNode = nodeById.get(bestParentId);
            if (bestNode && (bestNode.data as C4NodeData).kind === "component" && bestNode.parentId) {
              bestParentId = bestNode.parentId;
            }
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
        } else {
          // Follow AI disabled — only reset if current path has deleted nodes
          setExpandedPath((currentPath) => {
            if (currentPath.length === 0) return currentPath;
            // Trim path to the last valid ancestor
            const trimmed = [];
            for (const id of currentPath) {
              if (!nodeById.has(id)) break;
              trimmed.push(id);
            }
            return trimmed.length === currentPath.length ? currentPath : trimmed;
          });
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
      setFlows([]);
      setActiveFlowId(null);
    }
    await refreshList();
  }, [currentModel, refreshList, setNodes, setEdges, setCurrentModel, setExpandedPath, setRefPositions, setSourceMap, setProjectPath, setGroups, setFlows, setActiveFlowId]);

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
    setFlows([]);
    setActiveFlowId(null);
  }, [setNodes, setEdges, setCurrentModel, setStartingLevel, setExpandedPath, setRefPositions, setSourceMap, setProjectPath, setGroups, setFlows, setActiveFlowId]);

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
    const data: C4ModelData = { nodes, edges, startingLevel, sourceMap, refPositions, groups, flows };
    await invoke("write_model", { name, data: JSON.stringify(data) }).catch(() => toast("Failed to save model"));
    setCurrentModel(name);
    await refreshList();
  }, [nodes, edges, startingLevel, sourceMap, refPositions, groups, flows, refreshList, setCurrentModel]);

  // File watcher: reload when external tools (MCP, etc.) modify model files.
  // Handles both model-created and model-changed events.
  // On Windows, atomic rename (write_model_raw) fires Remove + Create instead of
  // Modify, producing both events — the debounce collapses them into a single reload.
  // Self-writes are caught by the lastKnownDisk comparison in reloadModel.
  useEffect(() => {
    const handler = (name: string) => {
      if (reloadTimer.current) clearTimeout(reloadTimer.current);
      reloadTimer.current = setTimeout(() => {
        if (name === currentModel) {
          reloadModel(name);
        }
        refreshList();
      }, 300);
    };
    const unlistenCreated = listen<string>("model-created", (e) => handler(e.payload));
    const unlistenChanged = listen<string>("model-changed", (e) => handler(e.payload));
    return () => {
      unlistenCreated.then((fn) => fn());
      unlistenChanged.then((fn) => fn());
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
    followAI,
    setFollowAI,
    changedNodeIds,
  };
}
