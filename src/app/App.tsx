/**
 * App shell.
 *
 * Holds the open project's tree, read from its code. The workspace is the
 * tree (navigation) on the left and the selected node's wiki-style page, the
 * diagram, or a special page on the right. Edits land in the code through
 * the derived editor: a claim into its test file, a container name into
 * structure.toml.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ErrorBoundary } from "./ErrorBoundary";
import { useSidebar } from "./sidebar";
import { ToastProvider, useToast } from "../shared/feedback/Toast";
import { AgentFailureProvider } from "../features/agent-launch/AgentFailure";
import { ModelTree } from "../widgets/model-tree/ModelTree";
import { TopBar, type WorkspaceView } from "../widgets/top-bar/TopBar";
import { DiagramView } from "../widgets/diagram-canvas/DiagramView";
import { NodePage } from "../pages/node/NodePage";
import type { Selected, SpecialPage } from "../entities/model/navigation";
import { DarkCodePage, LandingPage } from "../pages";
import { makeDerivedEditor } from "../features/derived-tree/derivedEditor";
import { ProjectPicker } from "../pages/project-picker/ProjectPicker";
import { SearchPalette } from "../features/search/SearchPalette";
import { Powerline } from "../widgets/top-bar/Powerline";
import { SettingsPanel } from "../widgets/settings-panel/SettingsPanel";
import { useLaunchSettings } from "../features/agent-launch/useLaunchSettings";
import { useAgentLaunchGate } from "../features/agent-launch/AgentLaunchConfirm";
import { useModelStorage, type ModelStorage } from "../features/model-storage/useModelStorage";
import { useAgentSession } from "../features/agent-launch/useAgentSession";
import { previewableNodeIds, usePreviewServer } from "../features/preview-client/usePreviewServer";
import { useModelHealth } from "../features/health-feed/useModelHealth";
import { useTestStatuses } from "../features/health-feed/useTestStatuses";
import { useFollow } from "../features/sessions/useFollow";
import { sessionTitle } from "../features/sessions/useSessions";
import { FollowBar } from "../widgets/follow-bar/FollowBar";
import { structuralBySubtree } from "../entities/model/health";
import { effectiveSourceMap, type ScryModel } from "../entities/model/viewmodel";
import type { Editor } from "../entities/model/editor";

/** Navigation a host sends through the event layer: the demo film, which
 *  mounts this app and drives it frame by frame. The desktop backend never
 *  sends it. */
interface Navigate {
  project?: string;
  view?: WorkspaceView;
  /** A node id, or `"landing"` for the Sessions page. */
  page?: string;
  diagramFocus?: string | null;
  lens?: string | null;
}

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
    // in-flight agent session alive. Cancel it on load so a refresh doesn't
    // leave orphans.
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
  const { openProject, projectPath } = storage;
  const opened = useRef(projectPath);
  opened.current = projectPath;
  useEffect(() => {
    const un = listen<Navigate>("navigate", ({ payload }) => {
      if (payload.project && payload.project !== opened.current) void openProject(payload.project);
    });
    return () => void un.then((f) => f());
  }, [openProject]);
  const model = storage.model;
  if (!model || storage.status !== "ready") {
    return <ProjectPicker storage={storage} />;
  }
  return <Workspace model={model} storage={storage} />;
}

function Workspace({ model, storage }: { model: ScryModel; storage: ModelStorage }) {
  const { committed, projectPath, reloadFromDisk, externalGeneration, openProject, closeProject } = storage;
  const agent = useAgentSession();
  const { toast } = useToast();
  // One preview sidecar per open project; node pages derive their Preview
  // section from the exports it reports.
  const preview = usePreviewServer(projectPath);
  // Which symbols the sidecar can render — derived from its export list and
  // each node's anchored source, never a stored flag.
  const previewable = useMemo(() => {
    const sourceMap = effectiveSourceMap(committed, model);
    return previewableNodeIds(model.nodes, preview.components, (id) => sourceMap[id]?.[0]?.pattern);
  }, [model, committed, preview.components]);

  const writing = agent.running;

  // Health over the derived tree: coverage, link evidence, style conformance,
  // dark files and every claim's status. Refreshes on open, when a fixture
  // fill ends, when the tree changes, and when a test report is ingested.
  const { report: healthReport, refresh: refreshHealth } = useModelHealth(projectPath, writing);
  // Claim verdicts and probe results for the tree's lanes, refreshed live
  // when the on-disk verdict cache changes.
  const { verdicts: testVerdicts, probes: probeResults, refresh: refreshTests } = useTestStatuses(projectPath, writing);

  // A changed tree moves health; a burst of changes costs one recompute.
  const observedGeneration = useRef(externalGeneration);
  useEffect(() => {
    if (writing || externalGeneration === observedGeneration.current) return;
    const t = setTimeout(() => {
      observedGeneration.current = externalGeneration;
      refreshHealth();
    }, 2000);
    return () => clearTimeout(t);
  }, [externalGeneration, writing, refreshHealth]);
  // An ingest moves claim states without touching the tree.
  useEffect(() => {
    const un = listen("test-results-changed", () => refreshHealth());
    return () => {
      void un.then((f) => f());
    };
  }, [refreshHealth]);

  const [selected, setSelected] = useState<Selected | null>(null);
  const [expanded, setExpanded] = useState<ReadonlySet<string>>(() => new Set());
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [searchOpen, setSearchOpen] = useState(false);

  // The subagent launch setup a fixture fill runs with, shown in the
  // powerline and named in the pre-launch confirm gate.
  const launchSettings = useLaunchSettings();
  const { launch } = launchSettings;
  const launchGate = useAgentLaunchGate(launchSettings);

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

  // The tree docked beside the page, hidden, or (in a narrow window) a drawer
  // over it.
  const sidebar = useSidebar();
  const { toggle: toggleTree, dismiss: dismissTree } = sidebar;

  // Global shortcuts: Ctrl/Cmd+K jumps to any node by name; Ctrl/Cmd+B shows
  // or hides the tree; Ctrl+Space flips wiki↔diagram (skipped while typing in
  // a field, so it can't yank you out of an in-place edit).
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "k" && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        setSearchOpen((o) => !o);
      } else if (e.key === "b" && (e.metaKey || e.ctrlKey) && !e.altKey && !isEditableTarget(e.target)) {
        e.preventDefault();
        toggleTree();
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
  }, [toggleView, toggleTree]);

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

  // The project opens on its agent sessions.
  const openedFor = useRef<string | null>(null);
  useEffect(() => {
    const key = projectPath ?? "";
    if (openedFor.current === key) return;
    openedFor.current = key;
    setSelected({ kind: "special", id: "session" });
  }, [projectPath]);

  // Following a live agent session: its edits take the wiki and map along.
  const hasNode = useCallback((id: string) => modelRef.current.nodes.some((n) => n.id === id), []);
  const follow = useFollow({
    projectPath,
    model,
    selected,
    hasNode,
    onGo: selectNode,
    onView: setWorkspaceView,
    onEnded: (s) => toast(`${sessionTitle(s, "this branch")} ended — stopped following`),
  });
  const followedIds = useMemo<ReadonlySet<string>>(
    () => (follow.target ? new Set([follow.target]) : EMPTY_IDS),
    [follow.target],
  );
  const nodeNames = useMemo(() => new Map(model.nodes.map((n) => [n.id, n.name])), [model]);

  useEffect(() => {
    const un = listen<Navigate>("navigate", ({ payload: nav }) => {
      if (nav.view) setWorkspaceView(nav.view);
      if (nav.page === "landing") setSelected({ kind: "special", id: "session" });
      else if (nav.page) selectNode(nav.page);
      if (nav.diagramFocus !== undefined) drillDiagram(nav.diagramFocus);
      if (nav.lens !== undefined) setConcernLens(nav.lens);
    });
    return () => void un.then((f) => f());
  }, [setWorkspaceView, selectNode, drillDiagram]);

  const onFixture = useCallback(
    (nodeId: string, renderStatus: string, renderError: string | null) => {
      if (!projectPath || writing) return;
      const node = modelRef.current.nodes.find((n) => n.id === nodeId);
      const name = node?.name ?? "component";
      launchGate.request(
        { action: `Generate placeholder preview data for “${name}”.` },
        () => agent.startFixture(projectPath, nodeId, name, renderStatus, renderError),
      );
    },
    [agent, projectPath, writing, launchGate],
  );

  // Edits land in the code; a write re-reads the tree and its status.
  const editor = useMemo<Editor | undefined>(
    () =>
      projectPath
        ? makeDerivedEditor({
            projectPath,
            model,
            onWritten: () => {
              void reloadFromDisk();
              refreshHealth();
              refreshTests();
            },
            onError: (m) => toast(m),
          })
        : undefined,
    [projectPath, model, reloadFromDisk, refreshHealth, refreshTests, toast],
  );
  const pageEditor = writing ? undefined : editor;

  const openSpecial = useCallback(
    (page: SpecialPage) => {
      // Special pages are part of the wiki; surfacing one returns from the diagram.
      setWorkspaceView("wiki");
      setSelected({ kind: "special", id: page });
    },
    [setWorkspaceView],
  );

  // A pick in the tree drawer is where the user wanted to go: the drawer
  // closes on it, uncovering the page. A docked tree stays put.
  const selectNodeFromTree = useCallback(
    (id: string) => {
      selectNode(id);
      dismissTree();
    },
    [selectNode, dismissTree],
  );
  const selectGroupFromTree = useCallback(
    (id: string) => {
      selectGroup(id);
      dismissTree();
    },
    [selectGroup, dismissTree],
  );

  // Structural violations per subtree — the tree marks every row that leads
  // down to one.
  const structural = structuralBySubtree(model, healthReport);

  return (
    <div className="relative flex h-screen w-screen flex-col bg-[var(--surface-canvas)]">
      <TopBar
        projectPath={projectPath}
        view={view}
        onSetView={showView}
        onOpenProject={(p) => void openProject(p)}
        onOpenWorktree={(p) => void openProject(p, { remember: false })}
        onCloseProject={closeProject}
        onOpenSearch={() => setSearchOpen(true)}
        onOpenSettings={() => setSettingsOpen(true)}
        sessionOpen={sessionOpen}
        onOpenSession={() => openSpecial("session")}
        treeOpen={sidebar.open}
        onToggleTree={toggleTree}
      />
      <div className="relative flex min-h-0 flex-1">
        {sidebar.mode === "drawer" && (
          <div className="absolute inset-0 z-30 bg-black/30" onClick={dismissTree} />
        )}
        {/* One tree in one place whatever the mode, so hiding it keeps its
            filter and scroll. Docked, it never takes more than half the
            window; as a drawer, it leaves a strip of the page to tap away. */}
        <div
          className={
            sidebar.mode === "docked"
              ? "flex max-w-[50vw] shrink-0"
              : sidebar.mode === "drawer"
                ? "absolute inset-y-0 left-0 z-40 flex max-w-[calc(100vw-48px)] shadow-xl"
                : "hidden"
          }
        >
          <ModelTree
            model={model}
            previewable={previewable}
            selected={selected}
            expanded={expanded}
            onSelectNode={selectNodeFromTree}
            onSelectGroup={selectGroupFromTree}
            onToggle={toggle}
            editor={pageEditor}
            // Where the followed session is editing now.
            activeNodeIds={followedIds}
            // The map's current level — tinted in the tree so it mirrors what
            // the diagram is showing. Only while in map view.
            activeLevel={view === "diagram" ? diagramFocus : undefined}
            concernLens={concernLens}
            onSetConcernLens={setConcernLens}
            structural={structural}
            claimStatus={healthReport?.derivedStatus?.nodes}
          />
        </div>
        {view === "diagram" ? (
          <DiagramView
            model={model}
            previewable={previewable}
            report={healthReport}
            focusId={diagramFocus}
            selectedId={selected?.kind === "node" ? selected.id : null}
            onFocus={drillDiagram}
            onSelectNode={selectFromDiagram}
            concernLens={concernLens}
          />
        ) : selected?.kind === "special" ? (
          selected.id === "dark" ? (
            <DarkCodePage model={model} report={healthReport} onSelectNode={selectNode} />
          ) : (
            <LandingPage
              projectPath={projectPath}
              model={model}
              following={follow.following}
              onFollow={follow.follow}
              onSelectNode={selectNode}
              onOpenWorktree={(p) => void openProject(p, { remember: false })}
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
          />
        ) : (
          <div className="flex flex-1 items-center justify-center text-xs text-[var(--text-muted)]">
            Select a node from the tree.
          </div>
        )}
      </div>
      {follow.following && !sessionOpen && (
        <FollowBar
          session={follow.session}
          title={follow.session ? sessionTitle(follow.session, follow.sessions?.branch ?? "detached") : "session"}
          names={nodeNames}
          onStop={follow.stop}
        />
      )}
      <Powerline
        model={model}
        agent={agent}
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
