/**
 * Empty-state screen: open a project folder, or a recent one. Any folder
 * opens: its tree is derived from its packaging and tests.
 */

import { useCallback } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { FolderOpen, X } from "lucide-react";
import { EYEBROW_BASE } from "../../shared/ui/pagekit";
import type { ModelStorage } from "../../features/model-storage/useModelStorage";
import { WindowControls } from "../../widgets/top-bar/TopBar";

export function ProjectPicker({ storage }: { storage: ModelStorage }) {
  const pickFolder = useCallback(async () => {
    const dir = await openDialog({ directory: true, multiple: false });
    if (typeof dir !== "string") return;
    await storage.openProject(dir);
  }, [storage]);

  const onOpenRecent = useCallback(
    async (path: string) => {
      await storage.openProject(path);
    },
    [storage],
  );

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
            What your software is accountable for, read from its tests: every
            claim, whether it holds right now, and what your agents are
            changing as they work.
          </p>
        </div>
        <button
          type="button"
          className="flex items-center justify-center gap-2 rounded-md bg-blue-600 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-blue-500"
          onClick={pickFolder}
          disabled={storage.status === "loading"}
        >
          <FolderOpen className="h-4 w-4" />
          {storage.status === "loading" ? "Reading the project…" : "Open project folder"}
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
