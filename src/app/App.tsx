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
import { ErrorBoundary } from "./ErrorBoundary";
import { ToastProvider } from "../shared/feedback/Toast";
import { AgentFailureProvider } from "../features/agent-launch/AgentFailure";
import { ModelTree } from "../widgets/model-tree/ModelTree";
import { TopBar, type WorkspaceView } from "../widgets/top-bar/TopBar";
import { DiagramView } from "../widgets/diagram-canvas/DiagramView";
import { NodePage, type Selected, type SpecialPage } from "../pages/node/NodePage";
import { ChangesPage, DarkCodePage, SessionPage, UnmappedClaimsPage } from "../pages";
import { ProjectPicker } from "../pages/project-picker/ProjectPicker";
import { SearchPalette } from "../features/search/SearchPalette";
import { planCounts } from "../features/change-marks/changeMarks";
import { Powerline } from "../widgets/top-bar/Powerline";
import { SettingsPanel } from "../widgets/settings-panel/SettingsPanel";
import { useLaunchSettings } from "../features/agent-launch/useLaunchSettings";
import { useMcpSetup } from "../features/mcp-setup/useMcpSetup";
import { McpSetupPrompt } from "../features/mcp-setup/McpSetupPrompt";
import { useAgentLaunchGate } from "../features/agent-launch/AgentLaunchConfirm";
import { useModelStorage } from "../features/model-storage/useModelStorage";
import { useModelBuild, type ModelBuild } from "../features/model-build/useModelBuild";
import { useAgentSession } from "../features/agent-launch/useAgentSession";
import { previewableNodeIds, usePreviewServer } from "../features/preview-client/usePreviewServer";
import { useModelHealth } from "../features/health-feed/useModelHealth";
import { useTestStatuses } from "../features/health-feed/useTestStatuses";
import { useSessionLog } from "../features/session-log/useSessionLog";
import { rollupTestFindings, testFindings } from "../entities/model/health";
import {
  addGroup as addGroupHelper,
  addLink as addLinkHelper,
  addNode as addNodeHelper,
  addProperty,
  addResponsibility,
  effectiveSourceMap,
  moveNode as moveNodeHelper,
  moveResponsibility as moveResponsibilityHelper,
  removeGroup as removeGroupHelper,
  removeLink as removeLinkHelper,
  removeNode as removeNodeHelper,
  removeProperty,
  removeResponsibility,
  renameConcern,
  setNodeGroup as setNodeGroupHelper,
  setNodePosition,
  updateGroup as updateGroupHelper,
  updateLink as updateLinkHelper,
  updateNode as updateNodeHelper,
  updateProperty,
  updateResponsibility,
  type DriftScope,
  type ScryModel,
} from "../entities/model/viewmodel";
import type { Editor } from "../entities/model/editor";

/** Whether a keydown landed inside an editable field (input, textarea, or an
 *  in-place contentEditable). Global shortcuts that overlap with text entry
 *  (Ctrl+Space) bail out when this is true. */
function isEditableTarget(target: EventTarget | null): boolean {
  const el = target as HTMLElement | null;
  if (!el) return false;
  return el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.isContentEditable;
}

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
        <AgentFailureProvider>
          <AppBody />
        </AgentFailureProvider>
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
      committed={storage.committed}
      planDiff={storage.planDiff}
      updateModel={storage.updateModel}
      projectPath={storage.projectPath}
      modelRef={storage.modelRef}
      build={build}
      setAgentRunning={storage.setAgentRunning}
      reloadFromDisk={storage.reloadFromDisk}
      changeLog={storage.changeLog}
      history={storage.history}
      externalGeneration={storage.externalGeneration}
      openProject={storage.openProject}
      closeProject={storage.closeProject}
    />
  );
}

function Workspace({
  model,
  committed,
  planDiff,
  updateModel,
  projectPath,
  modelRef: modelRefStr,
  build,
  setAgentRunning,
  reloadFromDisk,
  changeLog,
  history,
  externalGeneration,
  openProject,
  closeProject,
}: {
  model: ScryModel;
  committed: ScryModel | null;
  planDiff: ReturnType<typeof useModelStorage>["planDiff"];
  updateModel: ReturnType<typeof useModelStorage>["updateModel"];
  projectPath: string | null;
  modelRef: string | null;
  build: ModelBuild;
  setAgentRunning: (running: boolean) => void;
  reloadFromDisk: () => Promise<void>;
  changeLog: ReturnType<typeof useModelStorage>["changeLog"];
  history: ReturnType<typeof useModelStorage>["history"];
  externalGeneration: number;
  openProject: (path: string) => Promise<void>;
  closeProject: () => void;
}) {
  const agent = useAgentSession();
  // One preview sidecar per open project; node pages derive their Preview
  // section from the exports it reports.
  const preview = usePreviewServer(projectPath);
  // Which symbols the sidecar can render — derived from its export list and
  // each node's anchored source, never a stored flag. Drives the preview glyph
  // in the tree, the diagram's `visual` class, and the node page icon.
  const previewable = useMemo(() => {
    const sourceMap = effectiveSourceMap(committed, model);
    return previewableNodeIds(model.nodes, preview.components, (id) => sourceMap[id]?.[0]?.pattern);
  }, [model, committed, preview.components]);

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
    }, 1500);
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

  // Claim test verdicts: cheap, refreshed live when the on-disk cache changes
  // (an agent ingesting a report mid-session) — not only on the busy edge.
  const { verdicts: testVerdicts, probes: probeResults } = useTestStatuses(projectPath, writing);

  // The agent session log: what each session was asked and what it did,
  // re-read live as the hook server appends. The top bar shows its badge.
  const sessionLog = useSessionLog({ modelRef: modelRefStr });

  // Cheap, agent-free nudge: which scopes have code changes since the last
  // reconcile. Refreshes on open, when ANY writer finishes (builds and
  // per-node fills alike), and after verdict actions that change what counts
  // as drift — not only on whole-model builds.
  const [driftScopes, setDriftScopes] = useState<DriftScope[]>([]);
  // Last applied result, serialized: an unchanged poll must not re-render.
  const lastDriftJson = useRef("[]");
  const refreshDrift = useCallback(() => {
    if (!projectPath) return;
    invoke<DriftScope[]>("get_drift_status", { cwd: projectPath })
      .then((s) => {
        const next = Array.isArray(s) ? s : [];
        const json = JSON.stringify(next);
        if (json === lastDriftJson.current) return;
        lastDriftJson.current = json;
        setDriftScopes(next);
      })
      .catch(() => {});
  }, [projectPath]);
  useEffect(() => {
    if (writing) return;
    refreshDrift();
  }, [writing, refreshDrift]);

  // One observability refresh for verdict/anchor actions: health and the
  // drift nudge move together, so a verdict can't clear one and not the other.
  const refreshObservability = useCallback(() => {
    refreshHealth();
    refreshDrift();
  }, [refreshHealth, refreshDrift]);

  // External writers (an agent over MCP) reload the model layers live through
  // the file watcher, but health/drift come from these extractor commands —
  // without this they'd refresh only on open and on the in-app writing edge,
  // leaving every report-fed indicator stale until a manual reload. Trailing
  // debounce so a burst of agent writes costs one recompute; in-app runs are
  // excluded (their busy→idle edge already refreshes).
  const observedGeneration = useRef(externalGeneration);
  useEffect(() => {
    if (writing || externalGeneration === observedGeneration.current) return;
    const t = setTimeout(() => {
      observedGeneration.current = externalGeneration;
      refreshObservability();
    }, 5000);
    return () => clearTimeout(t);
  }, [externalGeneration, writing, refreshObservability]);

  const [selected, setSelected] = useState<Selected | null>(null);
  const [expanded, setExpanded] = useState<ReadonlySet<string>>(() => new Set());
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [searchOpen, setSearchOpen] = useState(false);

  // The subagent launch setup (which agent + model + effort a fill will run
  // with), surfaced read-only in the powerline and named in the pre-launch
  // confirm gate. Reloaded whenever the settings panel closes so an edit there
  // reflects immediately.
  const launchSettings = useLaunchSettings();
  const { launch } = launchSettings;
  // Confirm gate for every agent-spawning action — names what will run before
  // the (billable) launch; "don't ask again" clears it (the violet buttons stay
  // as the standing cue).
  const launchGate = useAgentLaunchGate(launchSettings);
  // A modelled project can still be missing its MCP wiring (e.g. opened before
  // setup, or never wired at all) — offer one-click integration on open. The
  // new-project screen handles the empty case; this catches everything already
  // past it.
  const mcpSetup = useMcpSetup(projectPath);

  // The Wiki/Diagram toggle. The diagram is a secondary nav surface onto the
  // same model and selection; `diagramFocus` is the level it currently shows
  // (children of this node, or top-level when null).
  const [view, setView] = useState<WorkspaceView>(
    () => (localStorage.getItem("scryer:view") === "diagram" ? "diagram" : "wiki"),
  );
  const setWorkspaceView = useCallback((v: WorkspaceView) => {
    localStorage.setItem("scryer:view", v);
    setView(v);
  }, []);
  // The session log is a wiki page, but the top bar shows it as its own
  // destination, so "Wiki" must land on a real page — the last one open before
  // it, or the top node — never straight back into the session log.
  const sessionOpen = view === "wiki" && selected?.kind === "special" && selected.id === "session";
  const lastWikiRef = useRef<Selected | null>(null);
  useEffect(() => {
    if (!(selected?.kind === "special" && selected.id === "session")) lastWikiRef.current = selected;
  }, [selected]);
  const showView = useCallback(
    (v: WorkspaceView) => {
      setWorkspaceView(v);
      if (v === "wiki" && sessionOpen) {
        const top = model.nodes.find((n) => !n.parentId);
        setSelected(lastWikiRef.current ?? (top ? { kind: "node", id: top.id } : null));
      }
    },
    [setWorkspaceView, sessionOpen, model.nodes],
  );
  // Flip wiki↔diagram in one step (the Ctrl+Space shortcut).
  const toggleView = useCallback(() => showView(view === "diagram" ? "wiki" : "diagram"), [showView, view]);
  const [diagramFocus, setDiagramFocus] = useState<string | null>(null);

  // The concern lens — the cross-cutting third axis ("where does auth live?").
  // One selected slug dims every tree row and diagram card whose subtree holds
  // no responsibility tagged with it. Shared by the tree (picker + dimming) and
  // the map (dimming), so both surfaces answer the question together. Guarded
  // against the registry: a slug that left the model (project switch, agent
  // rename) silently deactivates instead of dimming everything.
  const [concernLensRaw, setConcernLens] = useState<string | null>(null);
  const concernLens =
    concernLensRaw && (model.concerns ?? []).some((c) => c.slug === concernLensRaw)
      ? concernLensRaw
      : null;

  // Global shortcuts: Ctrl/Cmd+K jumps to any node by name; Ctrl+Space flips
  // wiki↔diagram (skipped while typing in a field, so it can't yank you out of
  // an in-place edit).
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "k" && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        setSearchOpen((o) => !o);
      } else if (
        e.code === "Space" &&
        e.ctrlKey &&
        !e.metaKey &&
        !e.altKey &&
        !isEditableTarget(e.target)
      ) {
        e.preventDefault();
        toggleView();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [toggleView]);

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
      // Keep the diagram framed on the selection's level: show the node among
      // its siblings (its parent's children). Drilling deeper is the diagram's
      // own double-click; this only reframes on selection.
      const node = modelRef.current.nodes.find((n) => n.id === id);
      setDiagramFocus(node?.parentId ?? null);
    },
    [ancestorsToExpand],
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

  // Drilling into a node on the diagram should also open it in the tree, so the
  // two surfaces stay in lockstep: reveal the node's children by expanding it
  // (and its ancestor chain) alongside reframing the diagram on that level.
  const drillDiagram = useCallback(
    (id: string | null) => {
      setDiagramFocus(id);
      if (id) setExpanded((prev) => new Set([...prev, id, ...ancestorsToExpand(id)]));
    },
    [ancestorsToExpand],
  );

  // Selecting a node from within the diagram highlights it (and reveals it in
  // the tree) without reframing the diagram — unlike `selectNode`, which jumps
  // the diagram to the selection's level. The diagram drives its own focus via
  // `drillDiagram`, so a single click should leave the current frame alone.
  const selectFromDiagram = useCallback(
    (id: string | null) => {
      // A pane click passes the level's parent (null at the top level), which
      // deselects everything on the current view; select it, or clear when null.
      if (id === null) {
        setSelected(null);
        return;
      }
      setSelected({ kind: "node", id });
      setExpanded((prev) => new Set([...prev, ...ancestorsToExpand(id)]));
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

  // Open the first top-level node ONCE when a project first loads, so the right
  // pane lands on a real page instead of the empty state. One-shot per project —
  // not an invariant: a deliberate deselect (clicking the empty diagram pane)
  // must stick, so we never re-fill a selection the user cleared.
  const autoSelectedFor = useRef<string | null>(null);
  useEffect(() => {
    const key = projectPath ?? "";
    if (autoSelectedFor.current === key) return;
    const top = model.nodes.find((n) => !n.parentId);
    if (!top) return; // model not loaded yet; try again when it is
    autoSelectedFor.current = key;
    if (!selected) selectNode(top.id);
  }, [model, projectPath, selected, selectNode]);

  const onFixture = useCallback(
    (nodeId: string, renderStatus: string, renderError: string | null) => {
      if (!projectPath || !modelRefStr || writing) return;
      const node = modelRef.current.nodes.find((n) => n.id === nodeId);
      const name = node?.name ?? "component";
      launchGate.request(
        { action: `Generate placeholder preview data for “${name}”.` },
        () => agent.startFixture(projectPath, modelRefStr, nodeId, name, renderStatus, renderError),
      );
    },
    [agent, projectPath, modelRefStr, writing, launchGate],
  );

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
          }, committed);
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
          }, committed);
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
          const { model: next, id } = addLinkHelper(m, src, dst, label ?? "", committed);
          newId = id;
          return next;
        });
        return newId;
      },
      updateLink: (linkId, patch) =>
        updateModel((m) => updateLinkHelper(m, linkId, patch)),
      deleteLink: (linkId) => updateModel((m) => removeLinkHelper(m, linkId)),
      setNodeGroup: (nodeId, groupId) =>
        updateModel((m) => setNodeGroupHelper(m, nodeId, groupId)),
      addResponsibility: (host, hostId) => {
        let newId = "";
        updateModel((m) => {
          const { model: next, id } = addResponsibility(m, host, hostId, "", committed);
          newId = id;
          return next;
        });
        return newId;
      },
      updateResponsibility: (host, hostId, respId, patch) =>
        updateModel((m) => updateResponsibility(m, host, hostId, respId, patch)),
      removeResponsibility: (host, hostId, respId) =>
        updateModel((m) => removeResponsibility(m, host, hostId, respId)),
      renameConcern: (from, to) => updateModel((m) => renameConcern(m, from, to)),
      adoptResponsibility: (respId) => {
        if (!projectPath) return;
        // Backend folds the claim into the committed model (the code already
        // exists); the file watcher then re-reads both layers into the UI.
        invoke("adopt_responsibility", { cwd: projectPath, respId })
          .then(() => refreshObservability())
          .catch((e) => console.error("adopt_responsibility failed", e));
      },
      rejectResponsibility: (respId) => {
        if (!projectPath) return;
        // Backend folds the claim into the committed model then drops it from the
        // plan, leaving a deletion work item; the watcher re-reads both layers.
        invoke("reject_responsibility", { cwd: projectPath, respId })
          .then(() => refreshObservability())
          .catch((e) => console.error("reject_responsibility failed", e));
      },
      dropResponsibility: (respId) => {
        if (!projectPath) return;
        // Code is right: delete the stale claim from both layers; watcher refreshes.
        invoke("drop_responsibility", { cwd: projectPath, respId })
          .then(() => refreshObservability())
          .catch((e) => console.error("drop_responsibility failed", e));
      },
      reimplementResponsibility: (respId) => {
        if (!projectPath) return;
        // Model is right: remove from committed so it reads as an Added to-do.
        invoke("reimplement_responsibility", { cwd: projectPath, respId })
          .then(() => refreshObservability())
          .catch((e) => console.error("reimplement_responsibility failed", e));
      },
      rewordResponsibility: (respId, statement) => {
        if (!projectPath) return;
        // Code diverged: the new wording already matches it, so write it to both
        // layers and clear the flag — no to-do, the model just catches up.
        invoke("reword_responsibility", { cwd: projectPath, respId, statement })
          .then(() => refreshObservability())
          .catch((e) => console.error("reword_responsibility failed", e));
      },
      dropNode: (nodeId) => {
        if (!projectPath) return;
        // Code is right: delete the stale node + subtree from both layers.
        invoke("drop_node", { cwd: projectPath, nodeId })
          .then(() => refreshObservability())
          .catch((e) => console.error("drop_node failed", e));
      },
      reimplementNode: (nodeId) => {
        if (!projectPath) return;
        // Model is right: keep the subtree in the plan as a rebuild to-do.
        invoke("reimplement_node", { cwd: projectPath, nodeId })
          .then(() => refreshObservability())
          .catch((e) => console.error("reimplement_node failed", e));
      },
      moveResponsibility: (fromNodeId, toNodeId, respId) =>
        updateModel((m) => moveResponsibilityHelper(m, fromNodeId, toNodeId, respId)),
      addProperty: (nodeId) => updateModel((m) => addProperty(m, nodeId, "", "")),
      updateProperty: (nodeId, index, patch) =>
        updateModel((m) => updateProperty(m, nodeId, index, patch)),
      removeProperty: (nodeId, index) => updateModel((m) => removeProperty(m, nodeId, index)),
      adoptProperty: (nodeId, label) => {
        if (!projectPath) return;
        // Backend folds the field into the committed model (the code already
        // exists); the file watcher then re-reads both layers into the UI.
        invoke("adopt_property", { cwd: projectPath, nodeId, label })
          .then(() => refreshObservability())
          .catch((e) => console.error("adopt_property failed", e));
      },
      rejectProperty: (nodeId, label) => {
        if (!projectPath) return;
        // Backend folds the field into committed then drops it from the plan,
        // leaving a deletion work item; the watcher re-reads both layers.
        invoke("reject_property", { cwd: projectPath, nodeId, label })
          .then(() => refreshObservability())
          .catch((e) => console.error("reject_property failed", e));
      },
      dropProperty: (nodeId, label) => {
        if (!projectPath) return;
        // Code is right: delete the stale field from both layers; watcher refreshes.
        invoke("drop_property", { cwd: projectPath, nodeId, label })
          .then(() => refreshObservability())
          .catch((e) => console.error("drop_property failed", e));
      },
      reimplementProperty: (nodeId, label) => {
        if (!projectPath) return;
        // Model is right: remove from committed so it reads as an Added to-do.
        invoke("reimplement_property", { cwd: projectPath, nodeId, label })
          .then(() => refreshObservability())
          .catch((e) => console.error("reimplement_property failed", e));
      },
    }),
    [updateModel, projectPath, refreshHealth, committed],
  );

  const pageEditor = writing ? undefined : editor;

  // Persist a manual card placement from the map (null releases it back to
  // auto-layout — the re-layout control). Pure cosmetics — the plan diff
  // ignores `position` — but still a plan write, so it's gated off while an
  // agent owns the file, like every other canvas edit.
  const moveDiagramNode = useCallback(
    (id: string, position: { x: number; y: number } | null) =>
      updateModel((m) => setNodePosition(m, id, position)),
    [updateModel],
  );

  const onCheckDrift = useCallback(() => {
    if (!projectPath) return;
    launchGate.request(
      {
        action: "Re-check the model against your code for drift.",
        detail: "Reads the changed code under the flagged nodes.",
      },
      () => build.checkDrift(projectPath),
    );
  }, [projectPath, build, launchGate]);

  const openSpecial = useCallback(
    (page: SpecialPage) => {
      // Special pages are part of the wiki; surfacing one returns from the diagram.
      setWorkspaceView("wiki");
      setSelected({ kind: "special", id: page });
    },
    [setWorkspaceView],
  );

  // The tree's Tests lens: test findings rolled
  // up per subtree so a branch reads as "N below" and a clean one as nothing.
  const testTally = rollupTestFindings(
    model,
    testFindings(model, committed, testVerdicts, probeResults),
  );
  const plan = planCounts(planDiff, model, committed);

  return (
    <div className="relative flex h-screen w-screen flex-col bg-[var(--surface-canvas)]">
      {mcpSetup.needsSetup && !mcpSetup.dismissed && (
        <div className="absolute right-3 top-12 z-30 w-[300px]">
          <McpSetupPrompt setup={mcpSetup} onDone={launchSettings.reload} dismissable />
        </div>
      )}
      <TopBar
        projectPath={projectPath}
        view={view}
        onSetView={showView}
        onOpenProject={(p) => void openProject(p)}
        onCloseProject={closeProject}
        onOpenSearch={() => setSearchOpen(true)}
        onOpenSettings={() => setSettingsOpen(true)}
        sessionBadge={sessionLog.badge}
        sessionLive={sessionLog.live}
        sessionOpen={sessionOpen}
        onOpenSession={() => openSpecial("session")}
      />
      <div className="flex min-h-0 flex-1">
        <ModelTree
          model={model}
          previewable={previewable}
          planDiff={planDiff}
          committed={committed}
          selected={selected}
          expanded={expanded}
          onSelectNode={selectNode}
          onSelectGroup={selectGroup}
          onToggle={toggle}
          editor={pageEditor}
          activeNodeIds={build.active ? build.activeNodeIds : EMPTY_IDS}
          // The map's current level (children of the focused node) — tinted in
          // the tree so it mirrors what the diagram is showing. Only while in
          // map view; undefined elsewhere disables the tint.
          activeLevel={view === "diagram" ? diagramFocus : undefined}
          completeness={healthReport?.completeness}
          concernLens={concernLens}
          onSetConcernLens={setConcernLens}
          testTally={testTally}
        />
        {view === "diagram" ? (
          <DiagramView
            model={model}
            previewable={previewable}
            planDiff={planDiff}
            committed={committed}
            report={healthReport}
            focusId={diagramFocus}
            selectedId={selected?.kind === "node" ? selected.id : null}
            onFocus={drillDiagram}
            onSelectNode={selectFromDiagram}
            onMoveNode={writing ? undefined : moveDiagramNode}
            concernLens={concernLens}
          />
        ) : selected?.kind === "special" ? (
          selected.id === "changes" ? (
            <ChangesPage
              planDiff={planDiff}
              model={model}
              committed={committed}
              changeLog={changeLog}
              onSelectNode={selectNode}
            />
          ) : selected.id === "session" ? (
            <SessionPage model={model} log={sessionLog} onSelectNode={selectNode} onSelectGroup={selectGroup} />
          ) : selected.id === "dark" ? (
            <DarkCodePage model={model} report={healthReport} onSelectNode={selectNode} />
          ) : (
            <UnmappedClaimsPage
              committed={committed}
              model={model}
              report={healthReport}
              onSelectNode={selectNode}
            />
          )
        ) : selected ? (
          <NodePage
            model={model}
            committed={committed}
            selected={selected}
            report={healthReport}
            testVerdicts={testVerdicts}
            probeResults={probeResults}
            projectPath={projectPath}
            editor={pageEditor}
            onSelectNode={selectNode}
            onSelectGroup={selectGroup}
            onFixture={projectPath && !writing ? onFixture : undefined}
            preview={preview}
            changeLog={changeLog}
            history={history}
            driftScopes={driftScopes}
            onCheckDrift={pageEditor ? onCheckDrift : undefined}
          />
        ) : (
          <div className="flex flex-1 items-center justify-center text-xs text-[var(--text-muted)]">
            Select a node from the tree.
          </div>
        )}
      </div>
      <Powerline
        model={model}
        agent={agent}
        build={build}
        plan={plan}
        health={healthReport}
        launch={launch}
        onOpenSpecial={openSpecial}
        onOpenSettings={() => setSettingsOpen(true)}
      />
      {settingsOpen && (
        <SettingsPanel
          projectPath={projectPath}
          onClose={() => {
            setSettingsOpen(false);
            launchSettings.reload();
          }}
        />
      )}
      {launchGate.modal}
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
