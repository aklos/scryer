import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  ReactFlowProvider,
  useReactFlow,
  applyEdgeChanges,
  addEdge,
} from "@xyflow/react";
import type {
  OnEdgesChange,
  OnConnect,
} from "@xyflow/react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { TopBar } from "./TopBar";
import { Sidebar } from "./Sidebar";
import { ContextPanel } from "./ContextPanel";
import { SettingsPanel } from "./SettingsPanel";
import { loadTheme, ThemeContext } from "./theme";
import { CommandPalette } from "./CommandPalette";
import { FlowScriptView } from "./FlowScriptView";
import { GroupsDndProvider, GroupsMain } from "./GroupsView";
import { C4Canvas } from "./C4Canvas";
import { SyncBar } from "./SyncBar";
import { autoLayout } from "./layout";
import { ErrorBoundary } from "./ErrorBoundary";
import { ToastProvider } from "./Toast";
import type { C4Kind, C4NodeData, C4Node, C4Edge, SourceLocation, Group, Flow, StartingLevel, AiToolsState } from "./types";
import { useModelStorage } from "./hooks/useModelStorage";
import type { ModelStorageState } from "./hooks/useModelStorage";
import { useHistory } from "./hooks/useHistory";
import { useAdvisor } from "./hooks/useAdvisor";
import { useCanvasEvents } from "./hooks/useCanvasEvents";
import { useVisibleNodes } from "./hooks/useVisibleNodes";
import { useNodesChange } from "./hooks/useNodesChange";
import { NodeDataProvider } from "./NodeDataContext";

/** Build breadcrumb path from expandedPath node IDs */
function buildBreadcrumbs(nodes: C4Node[], expandedPath: string[]): { id: string; name: string; kind: C4Kind }[] {
  return expandedPath.map((id) => {
    const node = nodes.find((n) => n.id === id);
    const data = node?.data as C4NodeData | undefined;
    return { id, name: data?.name ?? id, kind: data?.kind ?? "system" };
  });
}

/** Create a minimal starter model spanning all C4 levels. */
function buildStarterModel(): { nodes: C4Node[]; edges: C4Edge[]; flows: Flow[]; refPositions: Record<string, { x: number; y: number }> } {
  const personId = crypto.randomUUID();
  const systemId = crypto.randomUUID();
  const appId = crypto.randomUUID();
  const dbId = crypto.randomUUID();
  const serviceId = crypto.randomUUID();
  const repoId = crypto.randomUUID();
  const createRecordId = crypto.randomUUID();
  const validateId = crypto.randomUUID();
  const processId = crypto.randomUUID();
  const modelId = crypto.randomUUID();

  const c = "proposed" as const;
  const nodes: C4Node[] = [
    // System context
    { id: personId, type: "c4", position: { x: 0, y: 0 }, data: { name: "User", description: "", kind: "person" } },
    { id: systemId, type: "c4", position: { x: 360, y: 0 }, data: { name: "System", description: "", kind: "system", status: c } },
    // Containers
    { id: appId, type: "c4", position: { x: 0, y: 0 }, parentId: systemId, data: { name: "App", description: "", kind: "container", status: c, technology: "Node.js" } },
    { id: dbId, type: "c4", position: { x: 360, y: 0 }, parentId: systemId, data: { name: "Database", description: "", kind: "container", status: c, shape: "cylinder", technology: "MongoDB" } },
    // Components
    { id: serviceId, type: "c4", position: { x: 0, y: 0 }, parentId: appId, data: { name: "Service", description: "", kind: "component", status: c } },
    { id: repoId, type: "c4", position: { x: 380, y: 0 }, parentId: appId, data: { name: "Repository", description: "", kind: "component", status: c } },
    // Process inside Service
    { id: processId, type: "process", position: { x: 80, y: 0 }, parentId: serviceId, data: { name: "createRecord", description: "@[validateInput] then @[insertRecord]", kind: "process", status: c } },
    // Operations inside Service
    { id: validateId, type: "operation", position: { x: 0, y: 160 }, parentId: serviceId, data: { name: "validateInput", description: "Checks required fields and formats", kind: "operation", status: c } },
    { id: createRecordId, type: "operation", position: { x: 380, y: 160 }, parentId: serviceId, data: { name: "insertRecord", description: "Persists a new @[record] to the repository", kind: "operation", status: c } },
    // Model inside Repository
    { id: modelId, type: "model", position: { x: 0, y: 0 }, parentId: repoId, data: { name: "record", description: "", kind: "model", status: c, properties: [{ label: "id", description: "" }, { label: "name", description: "" }, { label: "createdAt", description: "" }] } },
  ];

  const edges: C4Edge[] = [
    { id: crypto.randomUUID(), source: personId, target: systemId, data: { label: "uses" } },
    { id: crypto.randomUUID(), source: personId, target: appId, data: { label: "uses" } },
    { id: crypto.randomUUID(), source: appId, target: dbId, data: { label: "reads/writes" } },
    { id: crypto.randomUUID(), source: personId, target: serviceId, data: { label: "uses" } },
    { id: crypto.randomUUID(), source: serviceId, target: repoId, data: { label: "uses" } },
    { id: crypto.randomUUID(), source: repoId, target: dbId, data: { label: "reads/writes" } },
  ];

  const step1 = crypto.randomUUID();
  const step2 = crypto.randomUUID();
  const flows: Flow[] = [
    {
      id: crypto.randomUUID(),
      name: "Core workflow",
      steps: [
        { id: step1, description: "@[User] sends payload with @[record] data" },
        { id: step2, description: "@[System] validates payload and creates @[record] in @[Database]" },
      ],
    },
  ];

  // Position references nicely at each level
  const refPositions: Record<string, { x: number; y: number }> = {
    // Container level: User above App
    [`${systemId}/${personId}`]: { x: 0, y: -280 },
    // Component level: User above Service, Database to the right of Repository
    [`${appId}/${personId}`]: { x: 0, y: -280 },
    [`${appId}/${dbId}`]: { x: 380, y: -280 },
  };

  return { nodes, edges, flows, refPositions };
}

function Flow() {

  const [nodes, setNodes] = useState<C4Node[]>([]);
  const [edges, setEdges] = useState<C4Edge[]>([]);
  const [modelList, setModelList] = useState<string[]>([]);
  const [currentModel, setCurrentModel] = useState<string | null>(null);
  const [startingLevel, setStartingLevel] = useState<StartingLevel>("system");
  const [expandedPath, setExpandedPath] = useState<string[]>([]);
  const [refPositions, setRefPositions] = useState<Record<string, { x: number; y: number }>>({});
  const [sourceMap, setSourceMap] = useState<Record<string, SourceLocation[]>>({});
  const [projectPath, setProjectPath] = useState<string | undefined>();
  const [templateList, setTemplateList] = useState<string[]>([]);
  const [groups, setGroups] = useState<Group[]>([]);
  const [selectedGroupId, setSelectedGroupId] = useState<string | null>(null);
  const [multiSelected, setMultiSelected] = useState<string[]>([]);
  const [totalSelected, setTotalSelected] = useState(0);

  // Flows
  const [flows, setFlows] = useState<Flow[]>([]);
  const [activeFlowId, setActiveFlowId] = useState<string | null>(null);
  const [canvasMode, setCanvasMode] = useState<"topology" | "groups">("topology");

  useEffect(() => {
    if (canvasMode !== "groups") return;
    setNodes((nds) => nds.some((n) => n.selected) ? nds.map((n) => n.selected ? { ...n, selected: false } : n) as C4Node[] : nds);
    setEdges((eds) => eds.some((e) => e.selected) ? eds.map((e) => e.selected ? { ...e, selected: false } : e) as C4Edge[] : eds);
    setSelectedGroupId(null);
    setMultiSelected([]);
    setTotalSelected(0);
  }, [canvasMode]);

  // Viewport is uncontrolled — use ReactFlow instance methods to read/set

  // Command palette
  const [paletteOpen, setPaletteOpen] = useState(false);

  // Theme
  const [settingsTab, setSettingsTab] = useState<"ai" | "theme" | null>(null);
  const [aiTools, setAiTools] = useState<AiToolsState>({ claude: false, codex: false, claudeMcpEnabled: false, codexMcpEnabled: false, claudeReadApproved: false });
  const [themeTick, setThemeTick] = useState(0); // force re-render on theme change

  // Drift detection + agent sync
  type DriftInfo = { nodeId: string; nodeName: string; patterns: string[] };
  const [driftedNodes, setDriftedNodes] = useState<DriftInfo[]>([]);
  const [structureChanged, setStructureChanged] = useState(false);
  const [syncStatus, setSyncStatus] = useState<"idle" | "running" | "error">("idle");
  const [syncMessage, setSyncMessage] = useState<string | null>(null);
  const [syncLog, setSyncLog] = useState<string[]>([]);
  const [activeAgent, setActiveAgent] = useState<{ name: string; available: boolean } | null>(null);
  const [implementing, setImplementing] = useState(false);
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === "k" && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        setPaletteOpen((prev) => !prev);
      }
};
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, []);

  const { screenToFlowPosition, fitView, getNode: getFlowNode, getViewport, setViewport } = useReactFlow();

  // Fit view after navigation or model load
  const fitViewRevision = useRef(0);
  const scheduleFitView = useCallback(() => {
    fitViewRevision.current++;
    const rev = fitViewRevision.current;
    setTimeout(() => {
      if (fitViewRevision.current === rev) {
        fitView({ padding: 0.15 });
      }
    }, 80);
  }, [fitView]);

  const currentParentId = expandedPath.length > 0
    ? expandedPath[expandedPath.length - 1]
    : undefined;

  const levelPrefix = currentParentId ?? "root";

  // --- Hooks ---

  const advisor = useAdvisor({ nodes, edges, startingLevel, sourceMap });

  const storage = useModelStorage(
    { nodes, edges, currentModel, startingLevel, sourceMap, projectPath, refPositions, groups, flows },
    {
      setNodes, setEdges, setStartingLevel, setSourceMap, setProjectPath,
      setRefPositions, setGroups,
      setFlows, setCurrentModel, setExpandedPath, setActiveFlowId,
      setModelList, setTemplateList,
    },
    scheduleFitView,
  );

  // Detect Claude Code integration state when project changes
  const [aiToolsReady, setAiToolsReady] = useState(false);
  useEffect(() => {
    setAiToolsReady(false);
    invoke<AiToolsState>("detect_ai_tools", { projectPath: projectPath ?? null })
      .then((tools) => { setAiTools(tools); setAiToolsReady(true); })
      .catch(() => { setAiToolsReady(true); });
  }, [projectPath, currentModel]);

  // Check for drifted nodes on window focus
  const checkDrift = useCallback(() => {
    if (!currentModel) return;
    invoke<{ nodes: DriftInfo[]; structureChanged: boolean; implementing: boolean }>("check_drift", { modelName: currentModel })
      .then((report) => {
        setDriftedNodes(report.nodes);
        setStructureChanged(report.structureChanged);
        setImplementing(report.implementing);
      })
      .catch(() => { setDriftedNodes([]); setStructureChanged(false); });
    invoke<{ name: string; available: boolean }>("get_active_agent")
      .then(setActiveAgent)
      .catch(() => setActiveAgent(null));
  }, [currentModel]);

  useEffect(() => {
    checkDrift();
    const handler = () => checkDrift();
    window.addEventListener("focus", handler);
    // Poll every 30s when auto-sync is enabled so drift is caught while user is active
    return () => {
      window.removeEventListener("focus", handler);
    };
  }, [checkDrift]);

  const handleSync = useCallback(async () => {
    if (!currentModel || !projectPath || syncStatus === "running") return;
    setSyncStatus("running");
    setSyncMessage(null);
    setSyncLog([]);
    setNodes((nds) => nds.map((n) => ({ ...n, selected: false })) as C4Node[]);
    try {
      await invoke<string>("start_agent_session", { cwd: projectPath, modelName: currentModel });
    } catch (e) {
      const msg = typeof e === "string" ? e : (e as Error)?.message ?? "Unknown error";
      setSyncMessage(msg);
      setSyncStatus("error");
    }
  }, [currentModel, projectPath, syncStatus]);


  // Listen for agent session completion/failure/toolCall events
  useEffect(() => {
    const unlisten = listen<{ kind: string; error?: string; name?: string; text?: string }>("agent-event", (event) => {
      const { kind, error, text } = event.payload;
      if (kind === "message" && text) {
        setSyncLog((prev) => {
          if (prev.length > 0) {
            const last = prev[prev.length - 1];
            // Check if last entry is the same message (with or without counter)
            const match = last.match(/^(.*) \((\d+)\)$/);
            const lastText = match ? match[1] : last;
            const lastCount = match ? parseInt(match[2]) : 1;
            if (lastText === text) {
              return [...prev.slice(0, -1), `${text} (${lastCount + 1})`];
            }
          }
          return [...prev.slice(-199), text];
        });
      } else if (kind === "completed" || kind === "cancelled") {
        // Mark this model as synced so drift baseline moves forward
        if (currentModel) {
          invoke("mark_synced", { modelName: currentModel }).catch(() => {});
        }
        setSyncStatus("idle");
            setDriftedNodes([]);
        setStructureChanged(false);
        checkDrift(); // re-check against new baseline
        if (kind === "completed" && currentModel) {
          invoke<string>("sync_diff", { modelName: currentModel })
            .then((summary) => setSyncMessage(summary))
            .catch(() => setSyncMessage("Sync complete"));
        }
      } else if (kind === "failed") {
        setSyncMessage(error ?? "Unknown error");
        setSyncStatus("error");
          }
    });
    return () => { unlisten.then((f) => f()); };
  }, [checkDrift, currentModel, nodes]);

  const handleToggleLock = useCallback(async () => {
    if (!currentModel) return;
    try {
      const locked = await invoke<boolean>("toggle_drift_lock", { modelName: currentModel });
      setImplementing(locked);
      if (!locked) checkDrift();
    } catch { /* ignore */ }
  }, [currentModel, checkDrift]);

  const handleCancelSync = useCallback(async () => {
    if (!currentModel) return;
    try {
      await invoke("cancel_agent_session", { modelName: currentModel });
    } catch { /* ignore */ }
    // syncStatus will be set to "idle" by the agent-event listener when cancellation completes
  }, [currentModel]);

  // --- History (undo/redo) ---
  const history = useHistory();
  const storageState: ModelStorageState = { nodes, edges, currentModel, startingLevel, sourceMap, projectPath, refPositions, groups, flows, syncing: syncStatus === "running" };

  useEffect(() => {
    history.capture(storageState);
  }, [nodes, edges, startingLevel, sourceMap, refPositions, groups, flows]);

  const applySnapshot = useCallback((snapshot: ModelStorageState) => {
    history.skipNextCapture();
    setNodes(snapshot.nodes);
    setEdges(snapshot.edges);
    setStartingLevel(snapshot.startingLevel);
    setSourceMap(snapshot.sourceMap);
    setProjectPath(snapshot.projectPath);
    setRefPositions(snapshot.refPositions);
    setGroups(snapshot.groups);
    setFlows(snapshot.flows);
  }, []);

  const onUndo = useCallback(() => {
    const snapshot = history.undo();
    if (snapshot) applySnapshot(snapshot);
  }, [history, applySnapshot]);

  const onRedo = useCallback(() => {
    const snapshot = history.redo();
    if (snapshot) applySnapshot(snapshot);
  }, [history, applySnapshot]);

  // Keyboard shortcuts for undo/redo
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      const mod = e.metaKey || e.ctrlKey;
      if (!mod) return;
      if (e.key === "z" && !e.shiftKey) {
        e.preventDefault();
        onUndo();
      } else if (e.key === "z" && e.shiftKey) {
        e.preventDefault();
        onRedo();
      } else if (e.key === "y") {
        e.preventDefault();
        onRedo();
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [onUndo, onRedo]);

  const loadModelWithClear = useCallback(async (name: string) => {
    history.clear();
    await storage.loadModel(name);
  }, [storage, history]);

  const driftedNodeIdSet = useMemo(() => new Set(driftedNodes.map((d) => d.nodeId)), [driftedNodes]);

  const [nonPlanarEdgeIds, setNonPlanarEdgeIds] = useState<Set<string>>(new Set());

  const { visibleNodes, visibleNodesWithHints, visibleEdges, refNodeIds } = useVisibleNodes({
    nodes, edges, currentParentId, refPositions,
    groups, setRefPositions, activeHints: advisor.hints,
    changedNodeIds: storage.changedNodeIds,
    driftedNodeIds: driftedNodeIdSet,
    nonPlanarEdgeIds,
  });

  const onNodesChange = useNodesChange({
    refNodeIds, levelPrefix,
    setNodes, setEdges, setRefPositions, setGroups, setSelectedGroupId,
    setSourceMap,
  });

  useCanvasEvents({
    expandNode,
    setNodes, setEdges, screenToFlowPosition, nodes, fitView,
  });

  // --- Remaining inline logic ---

  // Safety: if the expanded path points to a deleted node or a node with no
  // children, pop up until we find a valid level. Prevents blank canvas when
  // AI deletes/rearranges nodes.
  useEffect(() => {
    if (expandedPath.length === 0 || nodes.length === 0) return;
    const nodeById = new Map(nodes.map((n) => [n.id, n]));
    // Find the deepest valid ancestor that still has children
    let validDepth = 0;
    for (let i = 0; i < expandedPath.length; i++) {
      if (!nodeById.has(expandedPath[i])) break;
      validDepth = i + 1;
      // Check if this level has at least one child node
      const parentId = expandedPath[i];
      const hasChildren = nodes.some((n) => n.parentId === parentId);
      if (!hasChildren) break;
    }
    if (validDepth < expandedPath.length) {
      setExpandedPath(expandedPath.slice(0, validDepth));
    }
  }, [expandedPath, nodes, setExpandedPath]);

  const breadcrumbs = useMemo(
    () => buildBreadcrumbs(nodes, expandedPath),
    [nodes, expandedPath],
  );

  const currentParentKind = breadcrumbs.length > 0
    ? breadcrumbs[breadcrumbs.length - 1].kind
    : undefined;

  const selectedNode = useMemo(() => {
    return visibleNodes.find((n) => n.selected && !n.data._reference) ?? null;
  }, [visibleNodes]);

  /** Includes reference nodes — used by the toolbar for disconnect action. */
  const selectedCanvasNode = useMemo(() => {
    return visibleNodes.find((n) => n.selected) ?? null;
  }, [visibleNodes]);

  const selectedEdge = useMemo(() => {
    return visibleEdges.find((e) => e.selected) ?? null;
  }, [visibleEdges]);

  const isCodeLevelSelected = selectedNode?.data?.kind === "process" || selectedNode?.data?.kind === "operation";

  // Keep canvas centered on window resize (not internal layout shifts like panel open/close)
  useEffect(() => {
    let prevW = window.innerWidth;
    let prevH = window.innerHeight;
    const handler = () => {
      const newW = window.innerWidth;
      const newH = window.innerHeight;
      if (newW === prevW && newH === prevH) return;
      const dw = newW - prevW;
      const dh = newH - prevH;
      prevW = newW;
      prevH = newH;
      const vp = getViewport();
      setViewport({ x: vp.x + dw / 2, y: vp.y + dh / 2, zoom: vp.zoom });
    };
    window.addEventListener("resize", handler);
    return () => window.removeEventListener("resize", handler);
  }, [getViewport, setViewport]);

  const onSelectionChange = useCallback(({ nodes: selectedNodes, edges: selectedEdges }: { nodes: C4Node[]; edges: C4Edge[] }) => {
    const ids = selectedNodes
      .filter((n) => !n.data._reference)
      .map((n) => n.id);
    setMultiSelected(ids.length >= 2 ? ids : []);
    // Node-priority: when nodes are in selection, deselect edges
    if (selectedNodes.length > 0 && selectedEdges.length > 0) {
      setEdges((eds) => eds.map((e) => e.selected ? { ...e, selected: false } : e) as C4Edge[]);
      setTotalSelected(selectedNodes.length);
    } else {
      setTotalSelected(selectedNodes.length + selectedEdges.length);
    }
  }, []);

  const currentParentKindForGroup = currentParentId ? (nodes.find((n) => n.id === currentParentId)?.data as C4NodeData | undefined)?.kind : undefined;
  const canGroup = !!currentParentId && currentParentKindForGroup !== "component";
  const isCodeLevel = currentParentKindForGroup === "component";

// At code level, ReactFlow isn't rendered so updateNodeData must go through setNodes directly
  const codeLevelUpdateNodeData = useCallback((id: string, data: Record<string, unknown>) => {
    setNodes((nds) => nds.map((n) =>
      n.id === id ? { ...n, data: { ...n.data, ...data } } : n,
    ) as C4Node[]);
  }, []);

  const processMentionNames = useMemo((): { name: string; kind: "operation" | "process" | "model"; ref?: boolean }[] => {
    if (!isCodeLevelSelected || !selectedNode) return [];
    // Visible nodes have parentId stripped — look up the original to find the real parent component.
    const original = nodes.find((n) => n.id === selectedNode.id);
    const compId = original?.parentId;
    if (!compId) return [];
    const sid = selectedNode.id;
    const members = nodes
      .filter((n) => n.parentId === compId && (n.data as C4NodeData).kind === "operation" && n.id !== sid)
      .map((n) => ({ name: (n.data as C4NodeData).name, kind: "operation" as const }));
    const siblingProcs = nodes
      .filter((n) => n.parentId === compId && (n.data as C4NodeData).kind === "process" && n.id !== sid)
      .map((n) => ({ name: (n.data as C4NodeData).name, kind: "process" as const }));
    const siblingModels = nodes
      .filter((n) => n.parentId === compId && (n.data as C4NodeData).kind === "model" && n.id !== sid)
      .map((n) => ({ name: (n.data as C4NodeData).name, kind: "model" as const }));
    // Include members from components this one depends on
    const compEdges = edges.filter((e) => e.source === compId || e.target === compId);
    const refMembers: { name: string; kind: "operation" | "process" | "model"; ref?: boolean }[] = [];
    for (const edge of compEdges) {
      const otherId = edge.source === compId ? edge.target : edge.source;
      const other = nodes.find((n) => n.id === otherId);
      if (!other || (other.data as C4NodeData).kind !== "component") continue;
      for (const kind of ["operation", "process", "model"] as const) {
        nodes
          .filter((n) => n.parentId === otherId && (n.data as C4NodeData).kind === kind)
          .forEach((n) => refMembers.push({ name: (n.data as C4NodeData).name, kind, ref: true }));
      }
    }
    return [...members, ...siblingProcs, ...siblingModels, ...refMembers];
  }, [isCodeLevelSelected, selectedNode, nodes, edges]);

  const onEdgesChange: OnEdgesChange = useCallback(
    (changes) => setEdges((eds) => applyEdgeChanges(changes, eds) as C4Edge[]),
    [],
  );

  const onConnect: OnConnect = useCallback(
    (connection) => {
      if (currentParentKind === "component") return;
      if (connection.source === connection.target) return;
      if (refNodeIds.has(connection.source) && refNodeIds.has(connection.target)) return;
      setEdges((eds) => addEdge({ ...connection, data: { label: "" } }, eds) as C4Edge[]);
    },
    [currentParentKind, refNodeIds],
  );

  const updateEdgeData = useCallback(
    (id: string, data: { label?: string; method?: string }) => {
      setEdges((eds) =>
        eds.map((e) =>
          e.id === id ? { ...e, data: { ...e.data, ...data } } : e,
        ) as C4Edge[],
      );
    },
    [],
  );

  // Drill into a node
  function expandNode(nodeId: string) {
    const node = nodes.find((n) => n.id === nodeId);
    if (!node) return;
    const kind = (node.data as C4NodeData).kind;
    if (kind !== "system" && kind !== "container" && kind !== "component") return;
    setExpandedPath((prev) => [...prev, nodeId]);
    scheduleFitView();
  }

  const navigateToBreadcrumb = useCallback(
    (targetId: string | null) => {
      if (targetId === null) {
        setExpandedPath([]);
      } else {
        const idx = expandedPath.indexOf(targetId);
        setExpandedPath(expandedPath.slice(0, idx + 1));
      }
      setActiveFlowId(null);
           scheduleFitView();
    },
    [expandedPath, scheduleFitView],
  );

  const navigateToNode = useCallback(
    (id: string) => {
      const node = nodes.find((n) => n.id === id);
      if (!node) return;
      setActiveFlowId(null);
      const path: string[] = [];
      let cur = node.parentId;
      while (cur) {
        path.unshift(cur);
        const parent = nodes.find((n) => n.id === cur);
        cur = parent?.parentId;
      }
      setExpandedPath(path);
      scheduleFitView();
      // Wait for ReactFlow to remount (key changes with expandedPath) and measure nodes
      setTimeout(() => {
        setNodes((nds) =>
          nds.map((n) => ({ ...n, selected: n.id === id })) as C4Node[],
        );
        const tryCenter = (attempts: number) => {
          const flowNode = getFlowNode(id);
          if (flowNode?.measured?.width) {
            const cx = flowNode.position.x + flowNode.measured.width / 2;
            const cy = flowNode.position.y + (flowNode.measured.height ?? 0) / 2;
            const container = document.querySelector('.react-flow');
            const w = container?.clientWidth ?? window.innerWidth;
            const h = container?.clientHeight ?? window.innerHeight;
            const currentZoom = getViewport().zoom;
            setViewport({
              x: w / 2 - cx * currentZoom,
              y: h / 2 - cy * currentZoom,
              zoom: currentZoom,
            });
          } else if (attempts > 0) {
            requestAnimationFrame(() => tryCenter(attempts - 1));
          }
        };
        requestAnimationFrame(() => tryCenter(10));
      }, 150);
    },
    [nodes, getFlowNode, getViewport, setViewport, scheduleFitView],
  );


  const onAddNode = useCallback((kindOverride?: C4Kind, screenPos?: { x: number; y: number }) => {
    let kind: C4Kind = kindOverride ?? "system";
    if (!kindOverride && currentParentId) {
      const parentNode = nodes.find((n) => n.id === currentParentId);
      if (parentNode) {
        const parentKind = (parentNode.data as C4NodeData).kind;
        if (parentKind === "system") kind = "container";
        else if (parentKind === "container") kind = "component";
        else if (parentKind === "component") kind = "operation";
      }
    }
    const position = screenToFlowPosition(screenPos ?? {
      x: window.innerWidth / 2,
      y: window.innerHeight / 2,
    });
    const nodeType = kind === "operation" ? "operation" : kind === "process" ? "process" : kind === "model" ? "model" : "c4";
    const defaultName = kind === "operation" ? "newOperation"
      : kind === "process" ? "New process"
      : kind === "model" ? "NewModel"
      : `New ${kind}`;
    const newNode: C4Node = {
      id: crypto.randomUUID(),
      type: nodeType,
      position,
      data: {
        name: defaultName,
        description: "",
        kind,
        ...(kind === "model" ? { properties: [] } : {}),
      },
      ...(currentParentId ? { parentId: currentParentId } : {}),
    };
    setNodes((nds) => [...nds, newNode]);
  }, [nodes, currentParentId, screenToFlowPosition]);

  const onNewBlankModel = useCallback(() => {
    const starter = buildStarterModel();
    setNodes(starter.nodes);
    setEdges(starter.edges);
    setFlows(starter.flows);
    setRefPositions(starter.refPositions);
    setTimeout(() => fitView({ padding: 0.3, duration: 300 }), 50);
  }, [fitView]);

  const handleOpenCodebase = useCallback(async () => {
    const { open: openDialog } = await import("@tauri-apps/plugin-dialog");
    const selected = await openDialog({ directory: true, title: "Select project folder" });
    if (!selected) return;

    // Check if project already has a model
    const refStr = `project:${selected}`;
    try {
      await invoke<string>("read_model", { name: refStr });
      // Model exists — just load it
      storage.loadModel(refStr);
      return;
    } catch {
      // No existing model — check if it's actually a codebase
    }

    const codebase = await invoke<boolean>("is_codebase", { path: selected });
    if (!codebase) {
      const { ask } = await import("@tauri-apps/plugin-dialog");
      const proceed = await ask(
        "This directory doesn't look like a codebase (no package.json, Cargo.toml, .git, etc). Open it anyway?",
        { title: "Not a codebase", kind: "warning" },
      );
      if (!proceed) return;
    }

    const folderName = selected.split(/[/\\]/).filter(Boolean).pop() ?? "project";
    await storage.createAndLoadBlankModel(folderName, selected);
  }, [storage]);

  const handleBuildWithAI = useCallback(async () => {
    if (!currentModel || !projectPath) return;
    setSyncStatus("running");
    setSyncMessage(null);
    setSyncLog([]);
    try {
      await invoke<string>("start_initial_model_session", {
        cwd: projectPath,
        modelName: currentModel,
      });
    } catch (e) {
      const msg = typeof e === "string" ? e : (e as Error)?.message ?? "Unknown error";
      setSyncMessage(msg);
      setSyncStatus("error");
    }
  }, [currentModel, projectPath]);

  const deleteNode = useCallback(
    (id: string) => {
      const toDelete = new Set<string>([id]);
      let changed = true;
      while (changed) {
        changed = false;
        for (const n of nodes) {
          if (n.parentId && toDelete.has(n.parentId) && !toDelete.has(n.id)) {
            toDelete.add(n.id);
            changed = true;
          }
        }
      }
      setNodes((nds) => nds.filter((n) => !toDelete.has(n.id)));
      setEdges((eds) => eds.filter((e) => !toDelete.has(e.source) && !toDelete.has(e.target)));
      setSourceMap((prev) => {
        const next = { ...prev };
        for (const nid of toDelete) delete next[nid];
        return next;
      });
      setGroups((prev) => {
        const updated = prev
          .map((g) => ({ ...g, memberIds: g.memberIds.filter((mid) => !toDelete.has(mid)) }))
          .filter((g) => g.memberIds.length > 0);
        return updated;
      });
    },
    [nodes],
  );

  const onBulkDelete = useCallback(
    ({ nodes: delNodes, edges: delEdges }: { nodes: C4Node[]; edges: C4Edge[] }) => {
      const realNodeIds: string[] = [];
      const refNodeIdsToSever = new Set<string>();

      for (const n of delNodes) {
        if (n.data._reference) refNodeIdsToSever.add(n.id);
        else realNodeIds.push(n.id);
      }

      // Reference node deletion: sever all edges touching the reference node
      if (refNodeIdsToSever.size > 0) {
        setEdges((eds) => eds.filter((e) =>
          !refNodeIdsToSever.has(e.source) && !refNodeIdsToSever.has(e.target)
        ));
      }

      if (realNodeIds.length > 0) {
        const toDelete = new Set<string>(realNodeIds);
        let changed = true;
        while (changed) {
          changed = false;
          for (const n of nodes) {
            if (n.parentId && toDelete.has(n.parentId) && !toDelete.has(n.id)) {
              toDelete.add(n.id);
              changed = true;
            }
          }
        }
        setNodes((nds) => nds.filter((n) => !toDelete.has(n.id)));
        setEdges((eds) => eds.filter((e) => !toDelete.has(e.source) && !toDelete.has(e.target)));
        setSourceMap((prev) => {
          const next = { ...prev };
          for (const nid of toDelete) delete next[nid];
          return next;
        });
        setGroups((prev) =>
          prev
            .map((g) => ({ ...g, memberIds: g.memberIds.filter((mid) => !toDelete.has(mid)) }))
            .filter((g) => g.memberIds.length > 0),
        );
      }

      if (delEdges.length > 0) {
        const edgeIds = new Set(delEdges.map((e) => e.id));
        setEdges((eds) => eds.filter((e) => !edgeIds.has(e.id)));
      }
      setMultiSelected([]);
    },
    [nodes],
  );

  const onAutoLayout = useCallback(async () => {
    // Code level uses rack view — no spatial layout needed
    if (currentParentKind === "component") return;

    const layoutNodes = visibleNodes.map((n) => ({
      ...n,
      parentId: undefined,
      extent: undefined,
    }));
    const layoutIds = new Set(layoutNodes.map((n) => n.id));
    const layoutEdges = edges.filter(
      (e) => layoutIds.has(e.source) && layoutIds.has(e.target),
    );

    const result = await autoLayout(layoutNodes, layoutEdges, false, true);
    const posMap = new Map(result.nodes.map((n) => [n.id, n.position]));
    setNonPlanarEdgeIds(result.nonPlanarEdgeIds);

    const refIds = new Set(visibleNodes.filter((n) => n.data._reference).map((n) => n.id));
    setNodes((nds) =>
      nds.map((n) => {
        if (refIds.has(n.id)) return n;
        const newPos = posMap.get(n.id);
        return newPos ? { ...n, position: newPos } : n;
      }),
    );

    const refUpdates: Record<string, { x: number; y: number }> = {};
    for (const id of refIds) {
      const pos = posMap.get(id);
      if (pos) refUpdates[`${levelPrefix}/${id}`] = pos;
    }
    setRefPositions((prev) => ({ ...prev, ...refUpdates }));

    setTimeout(() => fitView({ padding: 0.15, duration: 300 }), 50);
  }, [visibleNodes, edges, fitView, levelPrefix, currentParentKind]);

  // Deferred auto-layout: when visible nodes with _needsLayout get measured, run the same layout as the button
  const [layoutPending, setLayoutPending] = useState(() => nodes.some((n) => n.data._needsLayout));
  useEffect(() => {
    const hasNeedsLayout = visibleNodes.some((n) => n.data._needsLayout);

    // Code level uses rack view — just clear _needsLayout flags, no positioning needed
    if (currentParentKind === "component" && hasNeedsLayout) {
      const visibleIds = new Set(visibleNodes.map((n) => n.id));
      setNodes((nds) =>
        nds.map((n) => {
          if (!n.data._needsLayout || !visibleIds.has(n.id)) return n;
          const { _needsLayout, ...rest } = n.data;
          return { ...n, data: rest as C4NodeData };
        }),
      );
      return;
    }

    // Architecture-level layout (ELK stress) — needs measurement, uses pending/layoutPending gate
    if (!layoutPending) {
      if (hasNeedsLayout) setLayoutPending(true);
      return;
    }
    const pending = visibleNodes.filter((n) => n.data._needsLayout);
    if (pending.length === 0) {
      setLayoutPending(false);
      return;
    }
    if (!pending.every((n) => n.measured)) return;

    // Clear flags only for nodes at the current level (not deeper levels that haven't been laid out yet)
    const visibleIds = new Set(visibleNodes.map((n) => n.id));
    setNodes((nds) => nds.map((n) => {
      if (!n.data._needsLayout || !visibleIds.has(n.id)) return n;
      const { _needsLayout, ...data } = n.data;
      return { ...n, data: data as C4NodeData };
    }));

    const layoutNodes = visibleNodes.map((n) => ({ ...n, parentId: undefined, extent: undefined }));
    const layoutIds = new Set(layoutNodes.map((n) => n.id));
    const layoutEdges = edges.filter((e) => layoutIds.has(e.source) && layoutIds.has(e.target));

    autoLayout(layoutNodes, layoutEdges, false).then((result) => {
      const posMap = new Map(result.nodes.map((n) => [n.id, n.position]));
      setNonPlanarEdgeIds(result.nonPlanarEdgeIds);
      const refIds = new Set(visibleNodes.filter((n) => n.data._reference).map((n) => n.id));
      setNodes((nds) =>
        nds.map((n) => {
          const pos = posMap.get(n.id);
          if (refIds.has(n.id)) return n;
          return pos ? { ...n, position: pos } : n;
        }),
      );

      const refUpdates: Record<string, { x: number; y: number }> = {};
      for (const id of refIds) {
        const pos = posMap.get(id);
        if (pos) refUpdates[`${levelPrefix}/${id}`] = pos;
      }
      if (Object.keys(refUpdates).length > 0) {
        setRefPositions((prev) => ({ ...prev, ...refUpdates }));
      }
      setLayoutPending(false);
      scheduleFitView();
    }).catch(() => {
      setLayoutPending(false);
    });
  }, [layoutPending, visibleNodes, edges, groups, setNodes, setRefPositions, levelPrefix, scheduleFitView, currentParentKind]);

  // --- Render ---

  const activeFlow = activeFlowId ? flows.find((s) => s.id === activeFlowId) ?? null : null;

  // Integration nudge: show when AI tools are installed, project has a path, but MCP not configured
  const [nudgeDismissed, setNudgeDismissed] = useState(false);
  const needsSetup = (aiTools.claude && !aiTools.claudeMcpEnabled)
    || (aiTools.codex && !aiTools.codexMcpEnabled);
  const alreadyDismissed = !!projectPath && !!localStorage.getItem(`scryer:mcpNudgeDismissed:${projectPath}`);
  const showNudge = aiToolsReady
    && currentModel !== null
    && (aiTools.claude || aiTools.codex)
    && !!projectPath
    && needsSetup
    && !nudgeDismissed
    && !alreadyDismissed;

  const handleNudgeSetup = useCallback(async () => {
    try {
      if (aiTools.claude && !aiTools.claudeMcpEnabled) {
        await invoke<string>("setup_mcp_integration", { action: "mcp", projectPath });
      }
      if (aiTools.codex && !aiTools.codexMcpEnabled) {
        await invoke<string>("setup_mcp_integration", { action: "mcp_codex", projectPath });
      }
      setAiTools((prev) => ({
        ...prev,
        claudeMcpEnabled: prev.claude || prev.claudeMcpEnabled,
        codexMcpEnabled: prev.codex || prev.codexMcpEnabled,
      }));
    } catch {
      // fallback: just dismiss
    }
    setNudgeDismissed(true);
  }, [projectPath, aiTools]);

  const handleNudgeDismiss = useCallback(() => {
    if (projectPath) {
      localStorage.setItem(`scryer:mcpNudgeDismissed:${projectPath}`, "1");
    }
    setNudgeDismissed(true);
  }, [projectPath]);

  return (
    <ThemeContext.Provider value={themeTick}>
    <div className="flex flex-col h-screen w-screen">
      <TopBar
        currentModel={currentModel}
        onOpenPalette={() => setPaletteOpen(true)}
        onNavigateToRoot={() => navigateToBreadcrumb(null)}
        onOpenSettings={() => setSettingsTab("ai")}
        onCloseModel={storage.newModel}
        onSaveAs={storage.saveModelAs}
        hasModel={currentModel !== null || nodes.length > 0}
        breadcrumbs={breadcrumbs}
        currentParentKind={currentParentKind}
        navigateToBreadcrumb={navigateToBreadcrumb}
        activeFlowId={activeFlowId}
        activeFlowName={activeFlow?.name ?? null}
        projectPath={projectPath}
        aiTools={aiTools}
        onAiToolsChange={setAiTools}
        onSetProjectPath={setProjectPath}
      />
      <GroupsDndProvider
        allNodes={nodes}
        groups={groups}
        onUpdateGroups={(fn) => setGroups(fn)}
        currentParentId={currentParentId}
        onNavigateToNode={navigateToNode}
      >
      <div className="flex flex-1 min-h-0">
        {currentModel && <Sidebar
          nodes={nodes}
          selectedNodeId={selectedNode?.id ?? null}
          expandedPath={expandedPath}
          onNavigateToNode={navigateToNode}
          onExpandNode={expandNode}
          groups={groups}
          onHighlightGroup={(groupId) => {
            const group = groups.find((g) => g.id === groupId);
            if (!group || group.memberIds.length === 0) return;
            const firstMember = nodes.find((n) => group.memberIds.includes(n.id));
            if (!firstMember) return;
            const path: string[] = [];
            let cur = firstMember.parentId;
            while (cur) {
              path.unshift(cur);
              const parent = nodes.find((n) => n.id === cur);
              cur = parent?.parentId;
            }
            setExpandedPath(path);
            setTimeout(() => {
              setNodes((nds) =>
                nds.map((n) => ({ ...n, selected: group.memberIds.includes(n.id) })) as C4Node[],
              );
              scheduleFitView();
            }, 50);
          }}
          flows={flows}
          activeFlowId={activeFlowId}
          onSelectFlow={(id) => { setActiveFlowId(id); }}
          onNewFlow={() => {
            const maxNum = flows
              .map((s) => s.id.replace("scenario-", ""))
              .map(Number)
              .filter((n) => !isNaN(n))
              .reduce((m, n) => Math.max(m, n), 0);
            const newId = `scenario-${maxNum + 1}`;
            const newFlow: Flow = { id: newId, name: "New flow", steps: [] };
            setFlows((prev) => [...prev, newFlow]);
            setActiveFlowId(newId);
                     }}
        />}
        <div className="flex-1 flex flex-col relative">
          {currentModel && !activeFlow && (currentParentKind === "system" || currentParentKind === "container") && (
            <div className="absolute top-3 right-3 z-20 flex items-center gap-0.5 rounded-lg border border-[var(--border-overlay)] bg-[var(--surface-overlay)] backdrop-blur-sm shadow-sm px-1 py-0.5">
              <button
                type="button"
                className={`px-2 py-0.5 text-[10px] uppercase tracking-wider font-semibold cursor-pointer transition-colors rounded ${
                  canvasMode === "topology"
                    ? "text-[var(--text-secondary)] bg-[var(--surface-active)]"
                    : "text-[var(--text-muted)] hover:text-[var(--text-secondary)] hover:bg-[var(--surface-tint)]"
                }`}
                onClick={() => setCanvasMode("topology")}
              >
                Topology
              </button>
              <button
                type="button"
                className={`px-2 py-0.5 text-[10px] uppercase tracking-wider font-semibold cursor-pointer transition-colors rounded ${
                  canvasMode === "groups"
                    ? "text-[var(--text-secondary)] bg-[var(--surface-active)]"
                    : "text-[var(--text-muted)] hover:text-[var(--text-secondary)] hover:bg-[var(--surface-tint)]"
                }`}
                onClick={() => setCanvasMode("groups")}
              >
                Groups
              </button>
            </div>
          )}
          {showNudge && (
            <div className="absolute top-14 right-3 z-10 flex items-start gap-3 px-4 py-3 rounded-lg border border-zinc-200/80 bg-white/90 shadow-lg backdrop-blur-sm dark:border-zinc-700/80 dark:bg-zinc-800/90 max-w-[320px]">
              <div className="flex-1 min-w-0">
                <div className="text-xs font-medium text-zinc-700 dark:text-zinc-200 mb-1">AI tool integration</div>
                <div className="text-[11px] text-zinc-500 dark:text-zinc-400 leading-relaxed">Set up MCP server for this project so AI tools can read and update the model.</div>
                <button
                  type="button"
                  className="mt-2 px-2.5 py-1 rounded bg-blue-500 text-white hover:bg-blue-600 cursor-pointer text-[11px] font-medium transition-colors"
                  onClick={handleNudgeSetup}
                >
                  Enable
                </button>
              </div>
              <button
                type="button"
                className="text-zinc-400 hover:text-zinc-600 dark:text-zinc-500 dark:hover:text-zinc-300 cursor-pointer text-sm leading-none mt-0.5"
                onClick={handleNudgeDismiss}
                title="Dismiss"
              >
                &times;
              </button>
            </div>
          )}
          {activeFlow ? (
            <FlowScriptView
              flow={activeFlow}
              onUpdate={(updated: Flow) => setFlows((prev) => prev.map((s) => s.id === updated.id ? updated : s))}
              onDelete={() => {
                const idx = flows.findIndex((s) => s.id === activeFlowId);
                const remaining = flows.filter((s) => s.id !== activeFlowId);
                setFlows(remaining);
                const next = remaining[Math.min(idx, remaining.length - 1)] ?? null;
                setActiveFlowId(next?.id ?? null);
              }}
              allNodes={nodes}
              sourceMap={sourceMap}
              projectPath={projectPath}
            />
          ) : canvasMode === "groups" && (currentParentKind === "system" || currentParentKind === "container") ? (
            <GroupsMain />
          ) : (
            <C4Canvas
              currentModel={currentModel}
              syncing={syncStatus === "running"}
              projectPath={projectPath}
              onBuildWithAI={handleBuildWithAI}
              expandedPath={expandedPath}
              visibleNodesWithHints={visibleNodesWithHints}
              visibleEdges={visibleEdges}
              onNodesChange={syncStatus === "running" ? (changes) => onNodesChange(changes.filter((c) => c.type !== "select")) : onNodesChange}
              onEdgesChange={syncStatus === "running" ? (changes) => onEdgesChange(changes.filter((c: any) => c.type !== "select")) : onEdgesChange}
              onConnect={onConnect}
              onBulkDelete={onBulkDelete}
              onSelectionChange={syncStatus === "running" ? () => {} : onSelectionChange}
              currentParentId={currentParentId}
              nodes={nodes}
              onAutoLayout={onAutoLayout}
              onNewBlankModel={onNewBlankModel}
              onOpenCodebase={handleOpenCodebase}
              templateList={templateList}
              loadTemplate={storage.loadTemplate}
              aiConfigured={advisor.aiConfigured}
              aiEnabled={advisor.aiEnabled}
              hintLoading={advisor.hintLoading}
              fetchHints={advisor.fetchHints}
              setSettingsOpen={(open: boolean) => { if (open) setSettingsTab("ai"); else setSettingsTab(null); }}
              parentName={currentParentId ? (nodes.find((n) => n.id === currentParentId)?.data as C4NodeData | undefined)?.name : undefined}
              parentKind={currentParentKindForGroup}
              selectedNode={selectedCanvasNode}
              selectedEdge={selectedEdge}
              deleteNode={deleteNode}
              setEdges={setEdges}
              onAddNode={onAddNode}
              currentParentKind={currentParentKind}
              layoutPending={layoutPending || visibleNodes.some((n) => n.data._needsLayout)}
              setNodes={setNodes}
              followAI={storage.followAI}
              onToggleFollowAI={() => storage.setFollowAI(!storage.followAI)}
            />
          )}
          {currentModel && (
            <SyncBar
              activeAgent={activeAgent}
              driftedNodes={driftedNodes}
              structureChanged={structureChanged}
              implementing={implementing}
              syncStatus={syncStatus}
              syncMessage={syncMessage}
              syncLog={syncLog}
              projectPath={projectPath}
              onSync={handleSync}
              onCancelSync={handleCancelSync}
              onDismissMessage={() => { setSyncMessage(null); if (syncStatus === "error") setSyncStatus("idle"); }}
              onDismissDrift={() => { if (currentModel) { invoke("mark_synced", { modelName: currentModel }).then(() => { setDriftedNodes([]); setStructureChanged(false); }).catch(() => {}); } }}
              onToggleLock={handleToggleLock}
              onNavigateToNode={navigateToNode}
            />
          )}
          {/* Command palette */}
          {paletteOpen && (
            <CommandPalette
              templateList={modelList}
              currentModel={currentModel}
              onOpenCodebase={handleOpenCodebase}
              onLoadTemplate={loadModelWithClear}
              onDeleteTemplate={storage.deleteModel}
              onRefreshList={storage.refreshList}
              onClose={() => setPaletteOpen(false)}
            />
          )}
          {/* Settings panel */}
          {settingsTab != null && (
            <SettingsPanel
              onClose={() => setSettingsTab(null)}
              onSaved={(configured: boolean) => advisor.setAiConfigured(configured)}
              theme={loadTheme()}
              onThemeChange={() => setThemeTick((t) => t + 1)}
              initialTab={settingsTab ?? "ai"}
            />
          )}
        </div>
        {/* Properties panel */}
        <NodeDataProvider value={isCodeLevel ? codeLevelUpdateNodeData : null}>
        <ContextPanel
          node={selectedNode}
          edge={selectedEdge}
          selectedGroupId={selectedGroupId}
          onUpdateEdge={updateEdgeData}
          codeLevel={!!currentParentId && (nodes.find((n) => n.id === currentParentId)?.data as C4NodeData | undefined)?.kind === "component"}
          hints={selectedNode ? advisor.hints[selectedNode.id] : undefined}
          groups={groups}
          onUpdateGroups={setGroups}
          allNodes={nodes}
          allEdges={edges}
          sourceMap={sourceMap}
          nodeDiffs={storage.nodeDiffs}
          onDismissDiff={storage.clearNodeDiff}
          onFixHint={(hint) => {
            if (hint.action?.type === "setShape") {
              setNodes((nds) => nds.map((n) =>
                n.id === hint.nodeId ? { ...n, data: { ...n.data, shape: (hint.action as { type: "setShape"; shape: string }).shape } } : n,
              ) as C4Node[]);
            } else if (hint.action?.type === "setExternal") {
              setNodes((nds) => nds.map((n) =>
                n.id === hint.nodeId ? { ...n, data: { ...n.data, external: (hint.action as { type: "setExternal"; value: boolean }).value } } : n,
              ) as C4Node[]);
            }
          }}
          onDismissHint={advisor.dismissHint}
          projectPath={projectPath}
          onUpdateOperationData={(fnId, data) => {
            setNodes((nds) => nds.map((n) =>
              n.id === fnId ? { ...n, data: { ...n.data, ...data } } : n,
            ) as C4Node[]);
          }}
          processMentionNames={processMentionNames}
          multiSelected={multiSelected}
          totalSelected={totalSelected}
          canGroup={canGroup}
          onCreateGroup={(name, memberIds) => {
            const memberSet = new Set(memberIds);
            setGroups((prev) => {
              const cleaned = prev.map((g) => ({
                ...g,
                memberIds: g.memberIds.filter((id) => !memberSet.has(id)),
              })).filter((g) => g.memberIds.length > 0);
              return [...cleaned, { id: crypto.randomUUID(), name, memberIds }];
            });
            setMultiSelected([]);
          }}
          onAddToGroup={(groupId, memberIds) => {
            const memberSet = new Set(memberIds);
            setGroups((prev) => prev.map((g) => {
              if (g.id === groupId) {
                return { ...g, memberIds: [...new Set([...g.memberIds, ...memberIds])] };
              }
              const filtered = g.memberIds.filter((id) => !memberSet.has(id));
              return filtered.length !== g.memberIds.length ? { ...g, memberIds: filtered } : g;
            }).filter((g) => g.memberIds.length > 0));
            setMultiSelected([]);
          }}
          activeFlow={activeFlow}
          groupsPaletteMode={canvasMode === "groups"}
        />
        </NodeDataProvider>
      </div>
      </GroupsDndProvider>
    </div>
    </ThemeContext.Provider>
  );
}

export default function App() {
  return (
    <ErrorBoundary>
      <ToastProvider>
        <ReactFlowProvider>
          <Flow />
        </ReactFlowProvider>
      </ToastProvider>
    </ErrorBoundary>
  );
}
