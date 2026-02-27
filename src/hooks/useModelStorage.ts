import { useCallback, useEffect, useMemo, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { autoLayout } from "../layout";
import type { C4ModelData, C4Node, C4Edge, StartingLevel, SourceLocation, Group, Contract, Flow } from "../types";
import { useToast } from "../Toast";

/**
 * Detect whether node positions look like the MCP server's grid layout.
 * MCP places nodes on a 250x220 grid offset by (100,100).
 * If 3+ nodes match this pattern, positions are MCP-generated.
 */
function needsAutoLayout(nodes: C4Node[]): boolean {
  if (nodes.length < 2) return false;
  const TOLERANCE = 5;
  let gridCount = 0;
  for (const n of nodes) {
    const xMod = Math.abs(((n.position.x - 100) % 250 + 250) % 250);
    const yMod = Math.abs(((n.position.y - 100) % 220 + 220) % 220);
    if (xMod <= TOLERANCE && yMod <= TOLERANCE) {
      gridCount++;
      if (gridCount >= 3) return true;
    }
  }
  return false;
}

/**
 * Run ELK layout on each group of sibling nodes (same parentId) independently.
 * Returns a new nodes array with updated positions.
 */
async function layoutByParent(nodes: C4Node[], edges: C4Edge[], modelGroups?: Group[]): Promise<C4Node[]> {
  // Group nodes by parentId (undefined → "root")
  const siblingBuckets = new Map<string, C4Node[]>();
  for (const n of nodes) {
    const key = n.parentId ?? "__root__";
    const arr = siblingBuckets.get(key);
    if (arr) arr.push(n);
    else siblingBuckets.set(key, [n]);
  }

  // Layout each sibling group independently
  const posMap = new Map<string, { x: number; y: number }>();
  const promises: Promise<void>[] = [];

  for (const [, siblings] of siblingBuckets) {
    if (siblings.length < 2) continue;
    const siblingIds = new Set(siblings.map((n) => n.id));
    const siblingEdges = edges.filter(
      (e) => siblingIds.has(e.source) && siblingIds.has(e.target),
    );
    // Filter visual groups to those relevant at this sibling level
    const levelGroups = modelGroups
      ?.map((g) => ({ ...g, memberIds: g.memberIds.filter((id) => siblingIds.has(id)) }))
      .filter((g) => g.memberIds.length >= 2);
    promises.push(
      autoLayout(siblings, siblingEdges, levelGroups).then((laid) => {
        for (const n of laid) {
          posMap.set(n.id, n.position);
        }
      }),
    );
  }

  await Promise.all(promises);

  return nodes.map((n) => {
    const pos = posMap.get(n.id);
    return pos ? { ...n, position: pos } : n;
  });
}

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

/** Parse raw JSON into a typed C4ModelData. */
export function parseModelData(raw: string): C4ModelData {
  const data = JSON.parse(raw);
  // Ensure operation/process/model nodes have the correct ReactFlow type + migrate fields
  const nodes: C4Node[] = (data.nodes ?? []).map((n: C4Node) => {
    const kind = n.data.kind as string;
    const expectedType = kind === "operation" ? "operation" : kind === "process" ? "process" : kind === "model" ? "model" : "c4";
    const nd = n.data as Record<string, unknown>;
    // Migrate guidelines→contract and array→string
    const rawContract = nd.contract ?? nd.guidelines;
    const contract = rawContract ? migrateContract(rawContract) : undefined;
    // Migrate references→sources
    const sources = nd.sources ?? nd.references;
    // Person and external system nodes never have status
    const stripStatus = kind === "person" || (kind === "system" && nd.external);
    const patched = { ...n, type: expectedType, data: { ...n.data, contract, sources, guidelines: undefined, references: undefined, ...(stripStatus ? { status: undefined } : {}) } };
    return patched;
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
    flows: data.flows ?? data.scenarios ?? [],
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

  // Load model list, templates on mount
  useEffect(() => {
    invoke<string[]>("list_models").then(setModelList).catch(() => toast("Failed to load model list"));
    invoke<string[]>("list_templates").then(setTemplateList).catch(() => {});
  }, [setModelList, setTemplateList]);

  // Stable fingerprint of saveable node data — ignores selection/measured/transient changes.
  // Only changes when actual model data (positions, data fields, parentId) changes.
  const nodeFingerprint = useMemo(() => {
    return nodes.map((n) => `${n.id}:${n.parentId ?? ""}:${n.position.x},${n.position.y}:${n.type}:${JSON.stringify(n.data)}`).join("|");
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
      const data: C4ModelData = { nodes, edges, startingLevel, sourceMap, projectPath, refPositions, groups, contract, flows };
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
      if (needsAutoLayout(data.nodes)) {
        data.nodes = await layoutByParent(data.nodes, data.edges, data.groups);
      }
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
      if (needsAutoLayout(data.nodes)) {
        data.nodes = await layoutByParent(data.nodes, data.edges, data.groups);
      }
      applyModelData(data, true);
      setRefPositions(data.refPositions ?? {});
    } catch {
      // Silently ignore — model may have been deleted externally
    }
  }, [applyModelData, setRefPositions]);

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
