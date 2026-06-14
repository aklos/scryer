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
import { Loader2 } from "lucide-react";
import type { VariationState } from "./NodePage";
import { ErrorBoundary } from "./ErrorBoundary";
import { ToastProvider } from "./Toast";
import { ModelTree } from "./ModelTree";
import { TopBar } from "./TopBar";
import { NodePage, type Selected, type SpecialPage } from "./NodePage";
import { buildReviewIndex, NeedsReviewPage, RecentChangesPage } from "./SpecialPages";
import { ProjectPicker } from "./ProjectPicker";
import { SearchPalette } from "./SearchPalette";
import { SyncBar } from "./SyncBar";
import { SettingsPanel } from "./SettingsPanel";
import { useModelStorage } from "./hooks/useModelStorage";
import { useModelBuild, type ModelBuild } from "./hooks/useModelBuild";
import { useAgentSession } from "./hooks/useAgentSession";
import { useModelHealth } from "./hooks/useModelHealth";
import {
  addGroup as addGroupHelper,
  addLink as addLinkHelper,
  addNode as addNodeHelper,
  addProperty,
  addResponsibility,
  moveNode as moveNodeHelper,
  moveResponsibility as moveResponsibilityHelper,
  removeGroup as removeGroupHelper,
  removeLink as removeLinkHelper,
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
      changeLog={storage.changeLog}
      clearAllNew={storage.clearAllNew}
      openProject={storage.openProject}
      closeProject={storage.closeProject}
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
  changeLog,
  clearAllNew,
  openProject,
  closeProject,
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
  changeLog: ReturnType<typeof useModelStorage>["changeLog"];
  clearAllNew: () => void;
  openProject: (path: string) => Promise<void>;
  closeProject: () => void;
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

  // The observability feed: coverage, anchor fingerprints, link evidence.
  // Refreshes on open and whenever an agent run finishes.
  const { report: healthReport, refresh: refreshHealth } = useModelHealth(
    projectPath,
    writing,
  );

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
  const [searchOpen, setSearchOpen] = useState(false);

  // Ctrl/Cmd+K — jump to any node or group by name.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "k" && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        setSearchOpen((o) => !o);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

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
      setExpanded((prev) => new Set([...prev, ...anc]));
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

  const onFixture = useCallback(
    (nodeId: string, renderStatus: string, renderError: string | null) => {
      if (!projectPath || !modelRefStr || writing) return;
      const node = modelRef.current.nodes.find((n) => n.id === nodeId);
      agent.startFixture(projectPath, modelRefStr, nodeId, node?.name ?? "component", renderStatus, renderError);
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
      moveNode: (nodeId, newParentId) =>
        updateModel((m) => moveNodeHelper(m, nodeId, newParentId)),
      addLink: (src, dst, label) => {
        let newId = "";
        updateModel((m) => {
          const { model: next, id } = addLinkHelper(m, src, dst, label ?? "");
          newId = id;
          return next;
        });
        return newId;
      },
      deleteLink: (linkId) => updateModel((m) => removeLinkHelper(m, linkId)),
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

  const onDismissDrift = useCallback(() => {
    if (!projectPath) return;
    // Optimistic: clear the nudge now; the anchor write makes it stick.
    setDriftScopes([]);
    invoke("reconcile_drift", { cwd: projectPath })
      .then(() => refreshHealth())
      .catch(() => {});
  }, [projectPath, refreshHealth]);

  const onCheckDrift = useCallback(() => {
    if (!projectPath) return;
    build.checkDrift(projectPath);
  }, [projectPath, build]);

  const openSpecial = useCallback((page: SpecialPage) => {
    setSelected({ kind: "special", id: page });
  }, []);

  // The status-bar counters, shared with the special pages so the number and
  // the list can never disagree.
  const reviewIndex = buildReviewIndex(model, healthReport, driftScopes, newNodeIds, newRespIds);
  const plannedCount = model.nodes.reduce((n, node) => {
    if (node.external) return n;
    const resps = (node.responsibilities ?? []).filter((r) => {
      if (r.vagrant) return false;
      const s = r.status ?? "proposed";
      return s === "proposed" || s === "changed";
    }).length;
    const props = (node.properties ?? []).filter((p) => {
      const s = p.status ?? "proposed";
      return s === "proposed" || s === "changed";
    }).length;
    return n + resps + props;
  }, 0);

  return (
    <div className="flex h-screen w-screen flex-col bg-[var(--surface-canvas)]">
      <TopBar
        projectPath={projectPath}
        onOpenProject={(p) => void openProject(p)}
        onCloseProject={closeProject}
        onReload={() => void reloadFromDisk()}
        onOpenSearch={() => setSearchOpen(true)}
        onOpenSettings={() => setSettingsOpen(true)}
      />
      {writing && (
        // Indigo = the agent. While it owns the model file the page is
        // read-only ([edit] affordances disappear); this banner says why.
        <div className="flex shrink-0 items-center justify-center gap-1.5 bg-indigo-500/10 py-1 text-2xs text-indigo-600 dark:text-indigo-400">
          <Loader2 className="h-3 w-3 animate-spin" />
          {build.building
            ? "Agent is building the model"
            : build.checking
              ? "Agent is checking for drift"
              : agent.label}
          {build.active && build.phase ? ` — ${build.phase}` : " — editing locked until it finishes"}
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
          onOpenSearch={() => setSearchOpen(true)}
          activeNodeIds={build.active ? build.activeNodeIds : EMPTY_IDS}
          newNodeIds={newNodeIds}
        />
        {selected?.kind === "special" ? (
          selected.id === "changes" ? (
            <RecentChangesPage changeLog={changeLog} onSelectNode={selectNode} />
          ) : (
            <NeedsReviewPage
              model={model}
              report={healthReport}
              driftScopes={driftScopes}
              newNodeIds={newNodeIds}
              newRespIds={newRespIds}
              editor={pageEditor}
              onSelectNode={selectNode}
              onCheckDrift={pageEditor ? onCheckDrift : undefined}
              onDismissDrift={pageEditor ? onDismissDrift : undefined}
              onClearAllNew={clearAllNew}
            />
          )
        ) : selected ? (
          <NodePage
            key={previewKey}
            model={model}
            selected={selected}
            report={healthReport}
            projectPath={projectPath}
            editor={pageEditor}
            onSelectNode={selectNode}
            onSelectGroup={selectGroup}
            onFill={projectPath && !writing ? onFill : undefined}
            onFixture={projectPath && !writing ? onFixture : undefined}
            variationState={variationState}
            onStartVariation={!writing || variationState ? onStartVariation : undefined}
            onAcceptVariation={onAcceptVariation}
            onDiscardVariations={onDiscardVariations}
            onSelectVariation={onSelectVariation}
            newRespIds={newRespIds}
            onClearNewResp={clearNewResp}
            changeLog={changeLog}
            driftScopes={driftScopes}
            onCheckDrift={pageEditor ? onCheckDrift : undefined}
            onDismissDrift={pageEditor ? onDismissDrift : undefined}
          />
        ) : (
          <div className="flex flex-1 items-center justify-center text-xs text-[var(--text-muted)]">
            Select a node from the tree.
          </div>
        )}
      </div>
      <SyncBar
        model={model}
        agent={agent}
        build={build}
        reviewCount={reviewIndex.total}
        plannedCount={plannedCount}
        onOpenSpecial={openSpecial}
      />
      {settingsOpen && <SettingsPanel onClose={() => setSettingsOpen(false)} />}
      {searchOpen && (
        <SearchPalette
          model={model}
          onSelectNode={selectNode}
          onSelectGroup={selectGroup}
          onClose={() => setSearchOpen(false)}
        />
      )}
    </div>
  );
}

/** Stable empty set so children don't re-render on a fresh `new Set()` each pass. */
const EMPTY_IDS: ReadonlySet<string> = new Set();
