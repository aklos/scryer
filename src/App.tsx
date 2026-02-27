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
import { TopBar } from "./TopBar";
import { Sidebar } from "./Sidebar";
import { PropertiesPanel } from "./PropertiesPanel";
import { SettingsPanel } from "./SettingsPanel";
import { CommandPalette } from "./CommandPalette";
import { FlowCanvas } from "./FlowCanvas";
import type { FlowCanvasHandle } from "./FlowCanvas";
import { C4Canvas } from "./C4Canvas";
import { FlowEditPopup } from "./FlowEditPopup";
import { autoLayout } from "./layout";
import { ErrorBoundary } from "./ErrorBoundary";
import { ToastProvider } from "./Toast";
import type { C4Kind, C4NodeData, C4Node, C4Edge, SourceLocation, Group, Contract, Flow, StartingLevel } from "./types";
import { useModelStorage } from "./hooks/useModelStorage";
import type { ModelStorageState } from "./hooks/useModelStorage";
import { useHistory } from "./hooks/useHistory";
import { useAdvisor } from "./hooks/useAdvisor";
import { useCanvasEvents } from "./hooks/useCanvasEvents";
import { useVisibleNodes } from "./hooks/useVisibleNodes";
import { useNodesChange } from "./hooks/useNodesChange";

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
    { id: systemId, type: "c4", position: { x: 350, y: 0 }, data: { name: "System", description: "", kind: "system", status: c } },
    // Containers
    { id: appId, type: "c4", position: { x: 0, y: 0 }, parentId: systemId, data: { name: "App", description: "", kind: "container", status: c, technology: "Node.js" } },
    { id: dbId, type: "c4", position: { x: 350, y: 0 }, parentId: systemId, data: { name: "Database", description: "", kind: "container", status: c, shape: "cylinder", technology: "MongoDB" } },
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
    { id: crypto.randomUUID(), source: personId, target: systemId, data: { label: "Uses" } },
    { id: crypto.randomUUID(), source: personId, target: appId, data: { label: "Uses" } },
    { id: crypto.randomUUID(), source: appId, target: dbId, data: { label: "Reads/writes" } },
    { id: crypto.randomUUID(), source: personId, target: serviceId, data: { label: "Uses" } },
    { id: crypto.randomUUID(), source: serviceId, target: repoId, data: { label: "Uses" } },
    { id: crypto.randomUUID(), source: repoId, target: dbId, data: { label: "Reads/writes" } },
  ];

  const step1 = crypto.randomUUID();
  const step2 = crypto.randomUUID();
  const flows: Flow[] = [
    {
      id: crypto.randomUUID(),
      name: "Core workflow",
      steps: [
        { id: step1, description: "@[User] sends payload with @[record] data", position: { x: 0, y: 0 } },
        { id: step2, description: "@[System] validates payload and creates @[record] in @[Database]", position: { x: 280, y: 0 }, processIds: [processId] },
      ],
      transitions: [
        { source: step1, target: step2 },
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
  const [contract, setContract] = useState<Contract>({ expect: [], ask: [], never: [] });
  const [selectedGroupId, setSelectedGroupId] = useState<string | null>(null);
  const [multiSelected, setMultiSelected] = useState<string[]>([]);
  const [totalSelected, setTotalSelected] = useState(0);

  // Flows
  const [flows, setFlows] = useState<Flow[]>([]);
  const [activeFlowId, setActiveFlowId] = useState<string | null>(null);
  const [editingFlow, setEditingFlow] = useState(false);
  const [selectedFlowStepId, setSelectedFlowStepId] = useState<string | null>(null);
  const flowRef = useRef<FlowCanvasHandle>(null);
  const onFlowSelectionInfo = useCallback((_info: { hasNodeSelection: boolean; selectedTransitionIndex: number }) => {
    // Selection info is now consumed by FlowCanvas's own toolbar
  }, []);

  // Viewport is uncontrolled — use ReactFlow instance methods to read/set

  // Command palette
  const [paletteOpen, setPaletteOpen] = useState(false);
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
    { nodes, edges, currentModel, startingLevel, sourceMap, projectPath, refPositions, groups, contract, flows },
    {
      setNodes, setEdges, setStartingLevel, setSourceMap, setProjectPath,
      setRefPositions, setGroups, setContract,
      setFlows, setCurrentModel, setExpandedPath, setActiveFlowId,
      setModelList, setTemplateList,
    },
    scheduleFitView,
  );

  // --- History (undo/redo) ---
  const history = useHistory();
  const storageState: ModelStorageState = { nodes, edges, currentModel, startingLevel, sourceMap, projectPath, refPositions, groups, contract, flows };

  useEffect(() => {
    history.capture(storageState);
  }, [nodes, edges, startingLevel, sourceMap, refPositions, groups, contract, flows]);

  const applySnapshot = useCallback((snapshot: ModelStorageState) => {
    history.skipNextCapture();
    setNodes(snapshot.nodes);
    setEdges(snapshot.edges);
    setStartingLevel(snapshot.startingLevel);
    setSourceMap(snapshot.sourceMap);
    setProjectPath(snapshot.projectPath);
    setRefPositions(snapshot.refPositions);
    setGroups(snapshot.groups);
    setContract(snapshot.contract);
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

  const newModelWithClear = useCallback(() => {
    history.clear();
    storage.newModel();
    const starter = buildStarterModel();
    setNodes(starter.nodes);
    setEdges(starter.edges);
    setFlows(starter.flows);
    setRefPositions(starter.refPositions);
    setTimeout(() => fitView({ padding: 0.3, duration: 300 }), 50);
  }, [storage, history, fitView]);

  const { visibleNodes, visibleNodesWithHints, visibleEdges, refNodeIds, groupNodeIds } = useVisibleNodes({
    nodes, edges, currentParentId, refPositions,
    groups, selectedGroupId, setRefPositions, activeHints: advisor.hints,
  });

  const onNodesChange = useNodesChange({
    refNodeIds, groupNodeIds, levelPrefix,
    setNodes, setEdges, setRefPositions, setGroups, setSelectedGroupId,
    setSourceMap,
  });

  useCanvasEvents({
    expandNode,
    setNodes, setEdges, screenToFlowPosition, nodes,
  });

  // --- Remaining inline logic ---

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

  const selectedEdge = useMemo(() => {
    return visibleEdges.find((e) => e.selected) ?? null;
  }, [visibleEdges]);

  const isCodeLevelSelected = selectedNode?.data?.kind === "process" || selectedNode?.data?.kind === "operation";

  const onSelectionChange = useCallback(({ nodes: selectedNodes, edges: selectedEdges }: { nodes: C4Node[]; edges: C4Edge[] }) => {
    const ids = selectedNodes
      .filter((n) => !n.data._reference && n.type !== "groupBox")
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

  const processMentionNames = useMemo((): { name: string; kind: "operation" | "process" | "model"; ref?: boolean }[] => {
    if (!isCodeLevelSelected || !selectedNode || !currentParentId) return [];
    const compId = currentParentId;
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
    const compEdges = edges.filter((e) => e.source === compId || e.target === compId);
    const refIds = new Set(compEdges.map((e) => (e.source === compId ? e.target : e.source)));
    const refMembers: { name: string; kind: "operation" | "process" | "model"; ref?: boolean }[] = [];
    for (const refId of refIds) {
      const refNode = nodes.find((n) => n.id === refId);
      if (!refNode || (refNode.data as C4NodeData).kind !== "component") continue;
      for (const kind of ["operation", "process", "model"] as const) {
        nodes
          .filter((n) => n.parentId === refId && (n.data as C4NodeData).kind === kind)
          .forEach((n) => refMembers.push({ name: (n.data as C4NodeData).name, kind, ref: true }));
      }
    }
    return [...members, ...siblingProcs, ...siblingModels, ...refMembers];
  }, [isCodeLevelSelected, selectedNode, currentParentId, nodes, edges]);

  const onEdgesChange: OnEdgesChange = useCallback(
    (changes) => setEdges((eds) => applyEdgeChanges(changes, eds) as C4Edge[]),
    [],
  );

  const onConnect: OnConnect = useCallback(
    (connection) => {
      if (currentParentKind === "component") return;
      setEdges((eds) => addEdge({ ...connection, data: { label: "" } }, eds) as C4Edge[]);
    },
    [currentParentKind],
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
      setSelectedFlowStepId(null);
      scheduleFitView();
    },
    [expandedPath, scheduleFitView],
  );

  const navigateToNode = useCallback(
    (id: string) => {
      const node = nodes.find((n) => n.id === id);
      if (!node) return;
      setActiveFlowId(null);
      setSelectedFlowStepId(null);
      const path: string[] = [];
      let cur = node.parentId;
      while (cur) {
        path.unshift(cur);
        const parent = nodes.find((n) => n.id === cur);
        cur = parent?.parentId;
      }
      setExpandedPath(path);
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
      }, 50);
    },
    [nodes, getFlowNode, getViewport, setViewport],
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
      : kind === "model" ? "newModel"
      : kind === "person" ? "New person"
      : "New node";
    const newNode: C4Node = {
      id: crypto.randomUUID(),
      type: nodeType,
      position,
      data: {
        name: defaultName,
        description: "",
        kind,
        status: kind === "person" ? undefined : "proposed",
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
      const groupIds = new Set<string>();
      const realNodeIds: string[] = [];

      for (const n of delNodes) {
        if (n.type === "groupBox") groupIds.add(n.id);
        else realNodeIds.push(n.id);
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

      if (groupIds.size > 0) {
        setGroups((prev) => prev.filter((g) => !groupIds.has(g.id)));
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
    const layoutNodes = visibleNodes
      .filter((n) => n.type !== "groupBox")
      .map((n) => ({
        ...n,
        parentId: undefined,
        extent: undefined,
      }));
    const layoutIds = new Set(layoutNodes.map((n) => n.id));
    const layoutEdges = edges.filter(
      (e) => layoutIds.has(e.source) && layoutIds.has(e.target),
    );

    // Filter groups to those with visible members at this level
    const activeGroups = groups
      .map((g) => ({ ...g, memberIds: g.memberIds.filter((id) => layoutIds.has(id)) }))
      .filter((g) => g.memberIds.length >= 2);

    const laid = await autoLayout(layoutNodes, layoutEdges, activeGroups);
    const posMap = new Map(laid.map((n) => [n.id, n.position]));

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
  }, [visibleNodes, edges, groups, fitView, levelPrefix]);


  // --- Render ---

  const activeFlow = activeFlowId ? flows.find((s) => s.id === activeFlowId) ?? null : null;

  return (
    <div className="flex flex-col h-screen w-screen">
      <TopBar
        currentModel={currentModel}
        onOpenPalette={() => setPaletteOpen(true)}
        onNavigateToRoot={() => navigateToBreadcrumb(null)}
        onOpenSettings={() => advisor.setSettingsOpen(true)}
        onCloseModel={storage.newModel}
        onSaveAs={() => {
          const raw = window.prompt("Save model as:", currentModel ?? "");
          if (!raw) return;
          const name = raw.trim().toLowerCase().replace(/[^a-z0-9_-]/g, "-");
          if (name) storage.saveModelAs(name);
        }}
        hasModel={currentModel !== null || nodes.length > 0}
        breadcrumbs={breadcrumbs}
        currentParentKind={currentParentKind}
        navigateToBreadcrumb={navigateToBreadcrumb}
        activeFlowId={activeFlowId}
        activeFlowName={activeFlow?.name ?? null}
      />
      <div className="flex flex-1 min-h-0">
        <Sidebar
          currentModel={currentModel}
          nodes={nodes}
          selectedNodeId={selectedNode?.id ?? null}
          expandedPath={expandedPath}
          modelList={modelList}
          onLoadModel={loadModelWithClear}
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
          onSelectFlow={(id) => { setActiveFlowId(id); setSelectedFlowStepId(null); setEditingFlow(false); }}
          onEditFlow={(id) => { setActiveFlowId(id); setSelectedFlowStepId(null); setEditingFlow(true); }}
          onNewFlow={() => {
            const maxNum = flows
              .map((s) => s.id.replace("scenario-", ""))
              .map(Number)
              .filter((n) => !isNaN(n))
              .reduce((m, n) => Math.max(m, n), 0);
            const newId = `scenario-${maxNum + 1}`;
            const newFlow: Flow = { id: newId, name: "New flow", steps: [], transitions: [] };
            setFlows((prev) => [...prev, newFlow]);
            setActiveFlowId(newId);
            setSelectedFlowStepId(null);
          }}
        />
        <div className="flex-1 flex flex-col">
          {activeFlow ? (
            <FlowCanvas
              ref={flowRef}
              flow={activeFlow}
              onUpdate={(updated: Flow) => setFlows((prev) => prev.map((s) => s.id === updated.id ? updated : s))}
              selectedStepId={selectedFlowStepId}
              onSelectStep={setSelectedFlowStepId}
              allNodes={nodes}
              onSelectionInfo={onFlowSelectionInfo}
            />
          ) : (
            <C4Canvas
              currentModel={currentModel}
              expandedPath={expandedPath}
              visibleNodesWithHints={visibleNodesWithHints}
              visibleEdges={visibleEdges}
              onNodesChange={onNodesChange}
              onEdgesChange={onEdgesChange}
              onConnect={onConnect}
              onBulkDelete={onBulkDelete}
              onSelectionChange={onSelectionChange}
              currentParentId={currentParentId}
              nodes={nodes}
              contract={contract}
              setContract={setContract}
              onAutoLayout={onAutoLayout}
              onNewBlankModel={onNewBlankModel}
              templateList={templateList}
              loadTemplate={storage.loadTemplate}
              aiConfigured={advisor.aiConfigured}
              aiEnabled={advisor.aiEnabled}
              hintLoading={advisor.hintLoading}
              fetchHints={advisor.fetchHints}
              setSettingsOpen={advisor.setSettingsOpen}
              parentName={currentParentId ? (nodes.find((n) => n.id === currentParentId)?.data as C4NodeData | undefined)?.name : undefined}
              parentKind={currentParentKindForGroup}
              selectedNode={selectedNode}
              selectedEdge={selectedEdge}
              deleteNode={deleteNode}
              setEdges={setEdges}
              setGroups={setGroups}
              onAddNode={onAddNode}
              currentParentKind={currentParentKind}
            />
          )}
          {/* Flow edit popup */}
          {editingFlow && activeFlowId && (() => {
            const sc = flows.find((s) => s.id === activeFlowId);
            if (!sc) return null;
            return (
              <FlowEditPopup
                flow={sc}
                onUpdate={(updates) => setFlows((prev) => prev.map((s) => s.id === activeFlowId ? { ...s, ...updates } : s))}
                onDelete={() => {
                  setFlows((prev) => prev.filter((s) => s.id !== activeFlowId));
                  setActiveFlowId(null);
                  setSelectedFlowStepId(null);
                  setEditingFlow(false);
                }}
                onClose={() => setEditingFlow(false)}
              />
            );
          })()}
          {/* Command palette */}
          {paletteOpen && (
            <CommandPalette
              modelList={modelList}
              currentModel={currentModel}
              onNewModel={newModelWithClear}
              onLoadModel={loadModelWithClear}
              onSaveAs={storage.saveModelAs}
              onDeleteModel={storage.deleteModel}
              onClose={() => setPaletteOpen(false)}
            />
          )}
          {/* Settings panel */}
          {advisor.settingsOpen && (
            <SettingsPanel
              onClose={() => advisor.setSettingsOpen(false)}
              onSaved={(configured: boolean) => advisor.setAiConfigured(configured)}
            />
          )}
        </div>
        {/* Properties panel */}
        <PropertiesPanel
          node={selectedNode}
          edge={selectedEdge}
          onUpdateEdge={updateEdgeData}
          codeLevel={!!currentParentId && (nodes.find((n) => n.id === currentParentId)?.data as C4NodeData | undefined)?.kind === "component"}
          hints={selectedNode ? advisor.hints[selectedNode.id] : undefined}
          groups={groups}
          onUpdateGroups={setGroups}
          allNodes={nodes}
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
          sourceLocations={selectedNode ? sourceMap[selectedNode.id] ?? [] : undefined}
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
          groupKind={currentParentKindForGroup === "container" ? "package" : "deployment"}
          onCreateGroup={(name, memberIds) => {
            const memberSet = new Set(memberIds);
            setGroups((prev) => {
              const cleaned = prev.map((g) => ({
                ...g,
                memberIds: g.memberIds.filter((id) => !memberSet.has(id)),
              })).filter((g) => g.memberIds.length > 0);
              const kind = currentParentKindForGroup === "container" ? "package" as const : "deployment" as const;
              return [...cleaned, { id: crypto.randomUUID(), kind, name, memberIds }];
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
        />
      </div>
    </div>
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
