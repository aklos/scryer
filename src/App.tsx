/**
 * App shell.
 *
 * Holds the v0.3 model in storage (project-local on disk, auto-saved). The UI is
 * a two-pane workspace: the model tree (definition surface) on the left and the
 * selected node/group's wiki-style page on the right. Mutations flow through the
 * Editor intents into the viewmodel helpers.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { VariationState } from "./NodePage";
import { ErrorBoundary } from "./ErrorBoundary";
import { ToastProvider } from "./Toast";
import { Breadcrumbs } from "./Breadcrumbs";
import { ModelTree } from "./ModelTree";
import { NodePage, type Selected } from "./NodePage";
import { ProjectPicker } from "./ProjectPicker";
import { SyncBar } from "./SyncBar";
import { SettingsPanel } from "./SettingsPanel";
import { useModelStorage } from "./hooks/useModelStorage";
import { useModelBuild, type ModelBuild } from "./hooks/useModelBuild";
import { useAgentSession } from "./hooks/useAgentSession";
import {
  addGroup as addGroupHelper,
  addNode as addNodeHelper,
  addProperty,
  addResponsibility,
  moveResponsibility as moveResponsibilityHelper,
  removeGroup as removeGroupHelper,
  removeNode as removeNodeHelper,
  removeProperty,
  removeResponsibility,
  setNodeGroup as setNodeGroupHelper,
  unlockRelocatedSource as unlockRelocatedSourceHelper,
  updateGroup as updateGroupHelper,
  updateNode as updateNodeHelper,
  updateProperty,
  updateResponsibility,
  type DriftScope,
  type ScryModel,
} from "./viewmodel";
import type { Editor } from "./editor";

export default function App() {
  useEffect(() => {
    // A dev refresh reloads the webview but not the Rust backend, leaving any
    // in-flight agent session alive and still editing the model. Cancel it on
    // load so a refresh doesn't leave orphans.
    void invoke("cancel_agent_session").catch(() => {});
  }, []);

  return (
    <ErrorBoundary>
      <ToastProvider>
        <AppBody />
      </ToastProvider>
    </ErrorBoundary>
  );
}

function AppBody() {
  const storage = useModelStorage();
  const build = useModelBuild(storage);
  const model = storage.model;

  if (!model || storage.status !== "ready") {
    return <ProjectPicker storage={storage} build={build} />;
  }
  return (
    <Workspace
      model={model}
      updateModel={storage.updateModel}
      projectPath={storage.projectPath}
      modelRef={storage.modelRef}
      build={build}
      setAgentRunning={storage.setAgentRunning}
      reloadFromDisk={storage.reloadFromDisk}
      newNodeIds={storage.newNodeIds}
      clearNewNode={storage.clearNewNode}
      newRespIds={storage.newRespIds}
      clearNewResp={storage.clearNewResp}
    />
  );
}

function Workspace({
  model,
  updateModel,
  projectPath,
  modelRef: modelRefStr,
  build,
  setAgentRunning,
  reloadFromDisk,
  newNodeIds,
  clearNewNode,
  newRespIds,
  clearNewResp,
}: {
  model: ScryModel;
  updateModel: ReturnType<typeof useModelStorage>["updateModel"];
  projectPath: string | null;
  modelRef: string | null;
  build: ModelBuild;
  setAgentRunning: (running: boolean) => void;
  reloadFromDisk: () => Promise<void>;
  newNodeIds: ReadonlySet<string>;
  clearNewNode: (id: string) => void;
  newRespIds: ReadonlySet<string>;
  clearNewResp: (id: string) => void;
}) {
  const agent = useAgentSession();

  // Per-node fills also own the file while running: suppress canvas write-back.
  useEffect(() => {
    setAgentRunning(agent.running || build.active);
  }, [agent.running, build.active, setAgentRunning]);

  // While a fill runs, the agent owns the file. Poll its writes onto the page
  // and load the final result when it ends — never write our stale in-memory
  // model back, which would clobber the agent's work.
  useEffect(() => {
    if (!agent.running) return;
    const t = setInterval(() => {
      void reloadFromDisk();
    }, 500);
    return () => {
      clearInterval(t);
      void reloadFromDisk();
    };
  }, [agent.running, reloadFromDisk]);

  const writing = agent.running || build.active;

  // Cheap, agent-free nudge: which scopes have code changes since the last
  // reconcile. Refreshes on open and whenever an agent run finishes.
  const [driftScopes, setDriftScopes] = useState<DriftScope[]>([]);
  useEffect(() => {
    if (!projectPath || build.active) return;
    let live = true;
    invoke<DriftScope[]>("get_drift_status", { cwd: projectPath })
      .then((s) => {
        if (live) setDriftScopes(Array.isArray(s) ? s : []);
      })
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [projectPath, build.active]);

  const [selected, setSelected] = useState<Selected | null>(null);
  const [expanded, setExpanded] = useState<ReadonlySet<string>>(() => new Set());
  const [settingsOpen, setSettingsOpen] = useState(false);

  const modelRef = useRef(model);
  modelRef.current = model;

  // The set of ids that must be open for `nodeId` to be visible in the tree:
  // its parent chain, and any group folder wrapping a node along that chain.
  const ancestorsToExpand = useCallback((nodeId: string): string[] => {
    const m = modelRef.current;
    const out: string[] = [];
    const seen = new Set<string>();
    let cur = m.nodes.find((n) => n.id === nodeId);
    while (cur && !seen.has(cur.id)) {
      seen.add(cur.id);
      const g = m.groups.find((gr) => gr.memberIds.includes(cur!.id));
      if (g) out.push(g.id);
      if (!cur.parentId) break;
      out.push(cur.parentId);
      cur = m.nodes.find((n) => n.id === cur!.parentId);
    }
    return out;
  }, []);

  const selectNode = useCallback(
    (id: string) => {
      setSelected({ kind: "node", id });
      const anc = ancestorsToExpand(id);
      setExpanded((prev) => new Set([...prev, ...anc, id]));
      clearNewNode(id);
    },
    [ancestorsToExpand, clearNewNode],
  );

  const selectGroup = useCallback(
    (id: string) => {
      setSelected({ kind: "group", id });
      const m = modelRef.current;
      const g = m.groups.find((gr) => gr.id === id);
      const container =
        g?.parentNodeId ??
        m.nodes.find((n) => n.id === (g?.memberIds[0] ?? ""))?.parentId ??
        null;
      const anc = container ? [container, ...ancestorsToExpand(container)] : [];
      setExpanded((prev) => new Set([...prev, ...anc]));
    },
    [ancestorsToExpand],
  );

  const toggle = useCallback((id: string, expand?: boolean) => {
    setExpanded((prev) => {
      const has = prev.has(id);
      const next = new Set(prev);
      if (expand === true || (expand === undefined && !has)) next.add(id);
      else next.delete(id);
      return next;
    });
  }, []);

  // Open the first top-level node once a model loads with nothing selected.
  useEffect(() => {
    if (selected) return;
    const top = model.nodes.find((n) => !n.parentId);
    if (top) selectNode(top.id);
  }, [model, selected, selectNode]);

  const onFill = useCallback(
    (nodeId: string) => {
      if (!projectPath || !modelRefStr) return;
      const node = modelRef.current.nodes.find((n) => n.id === nodeId);
      agent.startFill(projectPath, modelRefStr, nodeId, node?.name ?? "node");
    },
    [agent, projectPath, modelRefStr],
  );

  const onRender = useCallback(
    (nodeId: string) => {
      if (!projectPath || !modelRefStr || writing) return;
      const node = modelRef.current.nodes.find((n) => n.id === nodeId);
      agent.startPreview(projectPath, modelRefStr, nodeId, node?.name ?? "component");
    },
    [agent, projectPath, modelRefStr, writing],
  );

  // --- visual variation planning ---

  const [variationState, setVariationState] = useState<VariationState | null>(null);
  const prevRunning = useRef(false);

  useEffect(() => {
    if (prevRunning.current && !agent.running && variationState?.status === "generating") {
      setVariationState((prev) => prev ? { ...prev, status: "ready" } : null);
    }
    prevRunning.current = agent.running;
  }, [agent.running, variationState?.status]);

  const onStartVariation = useCallback(
    (nodeId: string, prompt: string, count?: number, baseVariationIdx?: number) => {
      if (!projectPath || !modelRefStr || agent.running) return;
      const n = count ?? 3;
      const node = modelRef.current.nodes.find((nd) => nd.id === nodeId);
      setVariationState({ nodeId, prompt, status: "generating", count: n, selectedIdx: null });
      agent.startVariation(projectPath, modelRefStr, nodeId, node?.name ?? "component", prompt, n, baseVariationIdx);
    },
    [agent, projectPath, modelRefStr],
  );

  const [previewKey, setPreviewKey] = useState(0);

  const onAcceptVariation = useCallback(
    (nodeId: string, variationIdx: number) => {
      if (!projectPath || !modelRefStr) return;
      invoke("accept_visual_variation", {
        cwd: projectPath, modelRef: modelRefStr, nodeId, variationIdx,
      })
        .then(() => {
          setVariationState(null);
          setPreviewKey((k) => k + 1);
          void reloadFromDisk();
        })
        .catch(() => {});
    },
    [projectPath, modelRefStr, reloadFromDisk],
  );

  const onDiscardVariations = useCallback(
    (nodeId: string) => {
      if (!projectPath) return;
      invoke("discard_visual_variations", { cwd: projectPath, nodeId }).catch(() => {});
      setVariationState(null);
    },
    [projectPath],
  );

  const onSelectVariation = useCallback((idx: number | null) => {
    setVariationState((prev) => prev ? { ...prev, selectedIdx: idx } : null);
  }, []);

  const editor = useMemo<Editor>(
    () => ({
      updateNode: (nodeId, patch) => updateModel((m) => updateNodeHelper(m, nodeId, patch)),
      deleteNode: (nodeId) => updateModel((m) => removeNodeHelper(m, nodeId)),
      addNode: (init) => {
        let newId = "";
        updateModel((m) => {
          const { model: next, id } = addNodeHelper(m, {
            kind: init.kind,
            name: "",
            parentId: init.parentId,
            groupId: init.groupId,
            external: init.external,
          });
          newId = id;
          return next;
        });
        return newId;
      },
      updateGroup: (groupId, patch) => updateModel((m) => updateGroupHelper(m, groupId, patch)),
      deleteGroup: (groupId) => updateModel((m) => removeGroupHelper(m, groupId)),
      addGroup: (init) => {
        let newId = "";
        updateModel((m) => {
          let { model: next, id } = addGroupHelper(m, {
            name: "",
            parentNodeId: init.parentNodeId,
          });
          newId = id;
          // Enclose the requested members in the same write (each is moved out of
          // any prior group), so the group never lands without its member.
          for (const mid of init.memberIds ?? []) {
            next = setNodeGroupHelper(next, mid, id);
          }
          return next;
        });
        return newId;
      },
      setNodeGroup: (nodeId, groupId) =>
        updateModel((m) => setNodeGroupHelper(m, nodeId, groupId)),
      addResponsibility: (host, hostId) => {
        let newId = "";
        updateModel((m) => {
          const { model: next, id } = addResponsibility(m, host, hostId);
          newId = id;
          return next;
        });
        return newId;
      },
      updateResponsibility: (host, hostId, respId, patch) =>
        updateModel((m) => updateResponsibility(m, host, hostId, respId, patch)),
      removeResponsibility: (host, hostId, respId) =>
        updateModel((m) => {
          const resps =
            host === "node"
              ? m.nodes.find((n) => n.id === hostId)?.responsibilities
              : m.groups.find((g) => g.id === hostId)?.responsibilities;
          const deleted = resps?.find((r) => r.id === respId);
          let next = removeResponsibility(m, host, hostId, respId);
          if (deleted?.relocatedFrom) next = unlockRelocatedSourceHelper(next, deleted);
          return next;
        }),
      moveResponsibility: (fromNodeId, toNodeId, respId) =>
        updateModel((m) => moveResponsibilityHelper(m, fromNodeId, toNodeId, respId)),
      addProperty: (nodeId) => updateModel((m) => addProperty(m, nodeId, "", "")),
      updateProperty: (nodeId, index, patch) =>
        updateModel((m) => updateProperty(m, nodeId, index, patch)),
      removeProperty: (nodeId, index) => updateModel((m) => removeProperty(m, nodeId, index)),
    }),
    [updateModel],
  );

  const pageEditor = writing ? undefined : editor;

  return (
    <div className="flex h-screen w-screen flex-col bg-[var(--surface-canvas)]">
      <Breadcrumbs
        model={model}
        selected={selected}
        onSelectNode={selectNode}
        onSelectGroup={selectGroup}
        projectPath={projectPath}
      />
      {writing && (
        <div className="flex shrink-0 items-center justify-center bg-amber-500/10 py-1 text-[11px] text-amber-600 dark:text-amber-400">
          {build.building
            ? "Building the model"
            : build.checking
              ? "Checking for drift"
              : agent.label}
          {build.active && build.phase ? ` — ${build.phase}` : " — editing locked"}
        </div>
      )}
      <div className="flex min-h-0 flex-1">
        <ModelTree
          model={model}
          selected={selected}
          expanded={expanded}
          onSelectNode={selectNode}
          onSelectGroup={selectGroup}
          onToggle={toggle}
          editor={pageEditor}
          onFill={projectPath && !writing ? onFill : undefined}
          activeNodeIds={build.active ? build.activeNodeIds : EMPTY_IDS}
          newNodeIds={newNodeIds}
        />
        {selected ? (
          <NodePage
            key={previewKey}
            model={model}
            selected={selected}
            projectPath={projectPath}
            editor={pageEditor}
            onSelectNode={selectNode}
            onSelectGroup={selectGroup}
            onRender={!writing ? onRender : undefined}
            variationState={variationState}
            onStartVariation={!writing || variationState ? onStartVariation : undefined}
            onAcceptVariation={onAcceptVariation}
            onDiscardVariations={onDiscardVariations}
            onSelectVariation={onSelectVariation}
            newRespIds={newRespIds}
            onClearNewResp={clearNewResp}
          />
        ) : (
          <div className="flex flex-1 items-center justify-center text-[12px] text-[var(--text-muted)]">
            Select a node from the tree.
          </div>
        )}
        <div className="w-[340px] shrink-0" aria-hidden />
      </div>
      <SyncBar
        model={model}
        agent={agent}
        build={build}
        projectPath={projectPath}
        driftScopes={driftScopes}
        onRevealNode={selectNode}
        onOpenSettings={() => setSettingsOpen(true)}
      />
      {settingsOpen && <SettingsPanel onClose={() => setSettingsOpen(false)} />}
    </div>
  );
}

/** Stable empty set so children don't re-render on a fresh `new Set()` each pass. */
const EMPTY_IDS: ReadonlySet<string> = new Set();
