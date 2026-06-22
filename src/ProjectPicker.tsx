/**
 * Empty-state screen: choose a project to open, or generate a model from a
 * codebase via the configured AI agent. Generation itself is driven by
 * `useModelBuild` (lifted to the app shell) so it streams onto the canvas — the
 * picker just triggers it and gets out of the way.
 */

import { useCallback, useEffect, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { FolderOpen, Sparkles, X, AlertTriangle } from "lucide-react";
import type { ModelStorage } from "./hooks/useModelStorage";
import type { ModelBuild } from "./hooks/useModelBuild";
import { useLaunchSettings } from "./hooks/useLaunchSettings";
import { useAgentLaunchGate } from "./AgentLaunchConfirm";

type Phase = "picker" | "needs-model";

export function ProjectPicker({
  storage,
  build,
}: {
  storage: ModelStorage;
  build: ModelBuild;
}) {
  const [phase, setPhase] = useState<Phase>("picker");
  const launchGate = useAgentLaunchGate(useLaunchSettings());

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
            className="mt-1 rounded-lg border border-[var(--border)] bg-[var(--surface-raised)] px-4 py-2.5 text-sm text-[var(--text-secondary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text)] transition-colors"
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
    return (
      <>
      {launchGate.modal}
      <Centered>
        <div className="flex flex-col items-center gap-6 max-w-md w-full text-center">
          <div className="flex flex-col items-center gap-1.5">
            <h2 className="text-base font-semibold text-[var(--text)]">
              No model in this project yet
            </h2>
            <p className="text-sm font-medium text-[var(--text-secondary)]">{folder}</p>
            <p className="text-2xs text-[var(--text-muted)] truncate max-w-md" title={storage.projectPath}>
              {storage.projectPath}
            </p>
          </div>
          <div className="flex flex-col gap-3 w-full">
            <div className="flex flex-col gap-2">
              <button
                type="button"
                data-cam="generate"
                className="flex items-center justify-center gap-2 rounded-lg border border-violet-400/50 bg-violet-500/5 px-4 py-2.5 text-sm font-medium text-[var(--text)] hover:bg-violet-500/10 transition-colors"
                onClick={onGenerate}
              >
                <Sparkles className="h-4 w-4 text-violet-500 dark:text-violet-400" />
                Generate from codebase
              </button>
              <p className="px-1 text-2xs leading-relaxed text-[var(--text-muted)]">
                Runs an in-depth agent pass over your whole codebase using your
                configured AI agent &mdash; this can take a while and use significant
                tokens (mind the model you&rsquo;ve set if you pay per token). You can
                cancel it any time mid-run.
              </p>
            </div>
            <button
              type="button"
              className="flex items-center justify-center gap-2 rounded-lg border border-[var(--border)] bg-[var(--surface-raised)] px-4 py-2.5 text-sm text-[var(--text-secondary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text)] transition-colors"
              onClick={onCreateBlank}
            >
              Start blank
            </button>
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

  return (
    <Centered>
      <div className="flex flex-col gap-8 max-w-md w-full">
        <div className="flex flex-col items-center gap-5 text-center">
          <div className="flex items-center gap-3.5">
            <img src="/logo.png" alt="" className="h-14 w-14" />
            <h1
              className="text-5xl font-bold tracking-tight text-[var(--text)]"
              style={{ fontFamily: "'Space Grotesk Variable', sans-serif" }}
            >
              scryer
            </h1>
          </div>
          <p className="text-sm leading-relaxed text-[var(--text-tertiary)] max-w-sm">
            A living model of what your software is accountable for, kept beside
            your code. You plan changes here first; your AI agent reads it over MCP
            and builds the code to match &mdash; the model leads, the code follows.
          </p>
        </div>
        <p className="self-center text-xs leading-relaxed text-[var(--text-muted)] text-center max-w-sm">
          Open a codebase, generate its model or shape one by hand, then build
          together as your agent reads and edits it over MCP.
        </p>
        <button
          type="button"
          className="flex items-center justify-center gap-2 rounded-lg bg-[var(--text)] px-4 py-3 text-sm font-medium text-[var(--surface-raised)] shadow-sm hover:opacity-90 transition-opacity"
          onClick={pickFolder}
        >
          <FolderOpen className="h-4 w-4" />
          Open project folder
        </button>
        {storage.recentProjects.length > 0 && (
          <div>
            <h3 className="text-xs font-semibold uppercase tracking-wide text-[var(--text-ghost)] mb-2">
              Recent
            </h3>
            <ul className="flex flex-col gap-1">
              {storage.recentProjects.map((path) => (
                <li
                  key={path}
                  className="group flex items-center justify-between rounded px-2 py-1.5 hover:bg-[var(--surface-hover)]"
                >
                  <button
                    type="button"
                    className="flex-1 text-left text-sm text-[var(--text-secondary)] hover:text-[var(--text)] truncate"
                    onClick={() => onOpenRecent(path)}
                    title={path}
                  >
                    {path}
                  </button>
                  <button
                    type="button"
                    className="ml-2 opacity-0 group-hover:opacity-100 text-[var(--text-ghost)] hover:text-[var(--text-secondary)]"
                    onClick={() => storage.forgetRecent(path)}
                    aria-label="Remove from recents"
                  >
                    <X className="h-3.5 w-3.5" />
                  </button>
                </li>
              ))}
            </ul>
          </div>
        )}
        {storage.error && (
          <div className="rounded border border-red-500/40 bg-red-500/10 px-3 py-2 text-xs text-red-600 dark:text-red-300">
            {storage.error}
          </div>
        )}
      </div>
    </Centered>
  );
}

function Centered({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex h-screen w-screen items-center justify-center bg-[var(--surface-canvas)] p-8">
      {children}
    </div>
  );
}
