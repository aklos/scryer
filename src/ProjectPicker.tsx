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

type Phase = "picker" | "needs-model";

export function ProjectPicker({
  storage,
  build,
}: {
  storage: ModelStorage;
  build: ModelBuild;
}) {
  const [phase, setPhase] = useState<Phase>("picker");

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
    void build.start(storage.projectPath);
  }, [storage.projectPath, build]);

  const onOpenRecent = useCallback(
    async (path: string) => {
      await storage.openProject(path);
    },
    [storage],
  );

  if (storage.status === "legacy") {
    return (
      <Centered>
        <div className="flex flex-col items-center gap-3 max-w-md text-center">
          <AlertTriangle className="h-8 w-8 text-amber-500" />
          <h2 className="text-base font-semibold text-[var(--text)]">
            Legacy model
          </h2>
          <p className="text-sm text-[var(--text-muted)]">{storage.error}</p>
          <button
            type="button"
            className="mt-2 rounded border border-[var(--border)] bg-[var(--surface-raised)] px-3 py-1.5 text-sm text-[var(--text-secondary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text)]"
            onClick={storage.closeProject}
          >
            Pick another project
          </button>
        </div>
      </Centered>
    );
  }

  if (phase === "needs-model" && storage.projectPath) {
    return (
      <Centered>
        <div className="flex flex-col items-center gap-4 max-w-md text-center">
          <h2 className="text-base font-semibold text-[var(--text)]">
            No model in this project yet
          </h2>
          <p className="text-sm text-[var(--text-muted)]">
            {storage.projectPath}
          </p>
          <div className="flex flex-col gap-2 w-full mt-2">
            <button
              type="button"
              className="flex items-center justify-center gap-2 rounded border border-[var(--border)] bg-[var(--surface-raised)] px-3 py-2 text-sm text-[var(--text-secondary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text)]"
              onClick={onGenerate}
            >
              <Sparkles className="h-4 w-4 text-amber-400" />
              Generate from codebase
              <span className="text-xs text-[var(--text-ghost)]">
                (uses your local AI agent)
              </span>
            </button>
            <button
              type="button"
              className="flex items-center justify-center gap-2 rounded border border-[var(--border)] bg-[var(--surface-raised)] px-3 py-2 text-sm text-[var(--text-secondary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text)]"
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
    );
  }

  return (
    <Centered>
      <div className="flex flex-col gap-6 max-w-md w-full">
        <div>
          <h1 className="text-lg font-semibold text-[var(--text)]">scryer</h1>
          <p className="mt-1 text-sm text-[var(--text-muted)]">
            Open a project to load or build its architecture model.
          </p>
        </div>
        <button
          type="button"
          className="flex items-center justify-center gap-2 rounded border border-[var(--border)] bg-[var(--surface-raised)] px-4 py-3 text-sm text-[var(--text-secondary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text)]"
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
          <div className="rounded border border-red-500/40 bg-red-500/10 px-3 py-2 text-xs text-red-300">
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
