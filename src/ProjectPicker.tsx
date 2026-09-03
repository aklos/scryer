/**
 * Empty-state screen: choose a project to open, or generate a model from a
 * codebase via the configured AI agent. Generation itself is driven by
 * `useModelBuild` (lifted to the app shell) so it streams onto the canvas — the
 * picker just triggers it and gets out of the way.
 */

import { useCallback, useEffect, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { FolderOpen, X, AlertTriangle } from "lucide-react";
import { AgentMark, EYEBROW_BASE } from "./pagekit";
import type { ModelStorage } from "./hooks/useModelStorage";
import type { ModelBuild } from "./hooks/useModelBuild";
import { useLaunchSettings } from "./hooks/useLaunchSettings";
import { useMcpSetup } from "./hooks/useMcpSetup";
import { McpSetupPrompt } from "./McpSetupPrompt";
import { useAgentLaunchGate } from "./AgentLaunchConfirm";
import { AGENT_LABEL, SettingsPanel } from "./SettingsPanel";
import { WindowControls } from "./TopBar";

type Phase = "picker" | "needs-model";

export function ProjectPicker({
  storage,
  build,
}: {
  storage: ModelStorage;
  build: ModelBuild;
}) {
  const [phase, setPhase] = useState<Phase>("picker");
  const launchSettings = useLaunchSettings();
  const [settingsOpen, setSettingsOpen] = useState(false);
  const launchGate = useAgentLaunchGate(launchSettings, () => setSettingsOpen(true));
  // Offer MCP wiring on the same screen where the model store is created, so a
  // fresh project gets set up in one sitting. The `.mcp.json` etc. are written
  // independently of `.scryer/`, so this works whether they enable before or
  // after creating the model.
  const mcpSetup = useMcpSetup(storage.projectPath);

  useEffect(() => {
    if (storage.status === "needs-model") {
      setPhase("needs-model");
    }
  }, [storage.status]);

  const pickFolder = useCallback(async () => {
    const dir = await openDialog({ directory: true, multiple: false });
    if (typeof dir !== "string") return;
    setPhase("picker");
    await storage.openProject(dir);
  }, [storage]);

  const onCreateBlank = useCallback(async () => {
    if (!storage.projectPath) return;
    await storage.createBlankModel(storage.projectPath);
  }, [storage]);

  const onGenerate = useCallback(() => {
    if (!storage.projectPath) return;
    // Kick off the orchestrated build; it creates the model, opens the canvas,
    // and streams nodes in. The picker unmounts as soon as the model loads.
    launchGate.request(
      {
        action: "Build the architecture model from your whole codebase.",
        detail: "An in-depth pass over every file — the longest, most token-heavy run.",
      },
      () => void build.start(storage.projectPath!),
    );
  }, [storage.projectPath, build, launchGate]);

  const onOpenRecent = useCallback(
    async (path: string) => {
      await storage.openProject(path);
    },
    [storage],
  );

  if (storage.status === "legacy") {
    return (
      <Centered>
        <div className="flex flex-col items-center gap-4 max-w-md text-center">
          <AlertTriangle className="h-8 w-8 text-orange-500 dark:text-orange-400" />
          <h2 className="text-base font-semibold text-[var(--text)]">
            Legacy model
          </h2>
          <p className="text-sm text-[var(--text-muted)]">{storage.error}</p>
          <button
            type="button"
            className="mt-1 rounded-md border border-[var(--border)] bg-[var(--surface-raised)] px-4 py-2.5 text-sm text-[var(--text-secondary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text)] transition-colors"
            onClick={storage.closeProject}
          >
            Pick another project
          </button>
        </div>
      </Centered>
    );
  }

  if (phase === "needs-model" && storage.projectPath) {
    const folder = storage.projectPath.split(/[/\\]/).filter(Boolean).pop();
    const launch = launchSettings.launch;
    return (
      <>
      {launchGate.modal}
      {settingsOpen && (
        <SettingsPanel
          projectPath={storage.projectPath}
          onClose={() => {
            setSettingsOpen(false);
            launchSettings.reload();
          }}
        />
      )}
      <Centered>
        <div className="flex flex-col items-center gap-6 max-w-md w-full text-center">
          <div className="flex flex-col items-center gap-1.5">
            <h2 className="text-base font-semibold text-[var(--text)]">
              No model in this project yet
            </h2>
            <p className="text-sm font-medium text-[var(--text-secondary)]">{folder}</p>
            <p className="max-w-md truncate font-mono text-xs text-[var(--text-muted)]" title={storage.projectPath}>
              {storage.projectPath}
            </p>
          </div>
          <div className="flex flex-col gap-3 w-full">
            <div className="flex flex-col gap-2">
              <button
                type="button"
                data-cam="generate"
                className="flex items-center justify-center gap-2 rounded-md border border-violet-400/50 bg-violet-500/5 px-4 py-2.5 text-sm font-medium text-[var(--text)] hover:bg-violet-500/10 transition-colors"
                onClick={onGenerate}
              >
                <AgentMark />
                Generate from codebase
              </button>
              <p className="px-1 text-xs leading-relaxed text-[var(--text-muted)]">
                Runs an in-depth agent pass over your whole codebase &mdash; this can take a
                while and use significant tokens. You can cancel it any time mid-run.
              </p>
              {/* The launch this button will make, and the way to change it —
                  the one place a fresh project can be configured before the
                  costliest run there is. */}
              <button
                type="button"
                className="flex items-center justify-center gap-1.5 px-1 text-2xs text-[var(--text-tertiary)] hover:text-[var(--text)]"
                onClick={() => setSettingsOpen(true)}
                title="Agent settings"
              >
                {launch.agent ? (
                  <span>
                    <span className="text-[var(--text-secondary)]">{AGENT_LABEL[launch.agent]}</span>
                    {" · "}
                    {launch.model || "default model"}
                    {" · "}
                    {launch.effort} effort
                  </span>
                ) : (
                  <span>No agent detected</span>
                )}
                <span className="underline decoration-dotted underline-offset-2">change</span>
              </button>
            </div>
            <button
              type="button"
              className="flex items-center justify-center gap-2 rounded-md border border-[var(--border)] bg-[var(--surface-raised)] px-4 py-2.5 text-sm text-[var(--text-secondary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text)] transition-colors"
              onClick={onCreateBlank}
            >
              Start blank
            </button>
            <p className="px-1 text-xs leading-relaxed text-[var(--text-muted)]">
              Either option creates a <code className="font-mono">.scryer/</code> folder &mdash; your
              model store &mdash; in this project.
            </p>
            {mcpSetup.needsSetup && !mcpSetup.dismissed && (
              <div className="mt-1 text-left">
                <McpSetupPrompt setup={mcpSetup} onDone={launchSettings.reload} />
              </div>
            )}
            <button
              type="button"
              className="text-xs text-[var(--text-tertiary)] hover:text-[var(--text-secondary)] mt-1"
              onClick={storage.closeProject}
            >
              Pick a different project
            </button>
          </div>
        </div>
      </Centered>
      </>
    );
  }

  // The landing reads as a tool, not a splash page: identity row and a single
  // grounded sentence, then straight to work — open a folder, or reopen a
  // recent project. Recents lead with the project NAME (what you scan for);
  // the full path rides beneath in mono (machine truth).
  return (
    <Centered>
      <div className="flex flex-col gap-7 max-w-md w-full">
        <div className="flex flex-col gap-3">
          <div className="flex items-center gap-2.5">
            <img src="/logo.png" alt="" className="h-9 w-9" />
            <h1 className="text-2xl font-semibold tracking-tight text-[var(--text)]">
              scryer
            </h1>
          </div>
          <p className="text-sm leading-relaxed text-[var(--text-secondary)]">
            A living model of what your software is accountable for, kept beside
            the code. Plan changes in the model; your agent reads and edits it
            over MCP and builds the code to match.
          </p>
        </div>
        <button
          type="button"
          className="flex items-center justify-center gap-2 rounded-md bg-blue-600 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-blue-500"
          onClick={pickFolder}
        >
          <FolderOpen className="h-4 w-4" />
          Open project folder
        </button>
        {storage.recentProjects.length > 0 && (
          <div>
            <h3 className={`${EYEBROW_BASE} text-[var(--text-ghost)] mb-1`}>
              Recent
            </h3>
            <ul className="-mx-2 flex flex-col">
              {storage.recentProjects.map((path) => {
                const name = path.split(/[/\\]/).filter(Boolean).pop() ?? path;
                return (
                  <li
                    key={path}
                    className="group flex items-center gap-2 rounded-md px-2 py-1.5 hover:bg-[var(--surface-hover)]"
                  >
                    <button
                      type="button"
                      className="min-w-0 flex-1 text-left"
                      onClick={() => onOpenRecent(path)}
                      title={path}
                    >
                      <span className="block truncate text-sm font-medium text-[var(--text)]">
                        {name}
                      </span>
                      <span className="block truncate font-mono text-xs text-[var(--text-muted)]">
                        {path}
                      </span>
                    </button>
                    <button
                      type="button"
                      className="shrink-0 rounded p-0.5 opacity-0 group-hover:opacity-100 text-[var(--text-ghost)] hover:text-[var(--text-secondary)]"
                      onClick={() => storage.forgetRecent(path)}
                      aria-label="Remove from recents"
                    >
                      <X className="h-3.5 w-3.5" />
                    </button>
                  </li>
                );
              })}
            </ul>
          </div>
        )}
        {storage.error && (
          <div className="flex items-start gap-2 rounded border border-[var(--border)] bg-[var(--surface-inset)] px-3 py-2 text-xs text-[var(--text-secondary)]">
            <span className="shrink-0 font-medium text-red-600 dark:text-red-400">!</span>
            {storage.error}
          </div>
        )}
      </div>
    </Centered>
  );
}

function Centered({ children }: { children: React.ReactNode }) {
  return (
    // The window is frameless and TopBar isn't mounted yet on these screens, so
    // this strip is the only titlebar: without it the window can't be moved or
    // closed before a project is open — which is the first thing a user sees.
    <div className="flex h-screen w-screen flex-col bg-[var(--surface-canvas)]">
      <div
        data-tauri-drag-region
        className="flex h-9 shrink-0 items-center justify-end px-2 select-none"
      >
        <WindowControls divider={false} />
      </div>
      <div className="flex min-h-0 flex-1 items-center justify-center p-8">
        {children}
      </div>
    </div>
  );
}
