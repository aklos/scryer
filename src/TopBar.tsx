/**
 * The application top bar: the project (file) menu on the left — open /
 * close / reload the model — and global affordances on the right (search,
 * settings). Page content lives below; this bar is app chrome only.
 */

import { useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { ChevronDown, FolderOpen, Search, Settings2 } from "lucide-react";
import { ContextMenu, type ContextMenuItem } from "./ContextMenu";

export function TopBar({
  projectPath,
  onOpenProject,
  onCloseProject,
  onReload,
  onOpenSearch,
  onOpenSettings,
}: {
  projectPath: string | null;
  onOpenProject: (path: string) => void;
  onCloseProject: () => void;
  onReload: () => void;
  onOpenSearch: () => void;
  onOpenSettings: () => void;
}) {
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const projectName = projectPath?.split("/").filter(Boolean).pop() ?? "scryer";

  const items: ContextMenuItem[] = [
    {
      id: "open",
      label: "Open project…",
      onSelect: () => {
        void openDialog({ directory: true }).then((p) => {
          if (typeof p === "string") onOpenProject(p);
        });
      },
    },
    { id: "reload", label: "Reload model from disk", onSelect: onReload },
    { id: "close", label: "Close project", onSelect: onCloseProject },
  ];

  return (
    <div className="flex h-9 shrink-0 items-center gap-1 border-b border-[var(--border)] bg-[var(--surface)] px-2 select-none">
      <button
        type="button"
        onClick={(e) => {
          const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
          setMenu({ x: r.left, y: r.bottom + 2 });
        }}
        title={projectPath ?? undefined}
        className="flex min-w-0 items-center gap-1.5 rounded px-2 py-1 text-xs font-medium text-[var(--text-secondary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text)] cursor-pointer"
      >
        <FolderOpen className="h-3.5 w-3.5 shrink-0 text-[var(--text-muted)]" />
        <span className="truncate">{projectName}</span>
        <ChevronDown className="h-3 w-3 shrink-0 text-[var(--text-ghost)]" />
      </button>

      <div className="flex-1" />

      <button
        type="button"
        onClick={onOpenSearch}
        title="Search the model (Ctrl+K)"
        className="flex items-center rounded p-1 text-[var(--text-muted)] hover:bg-[var(--surface-hover)] hover:text-[var(--text)] cursor-pointer"
      >
        <Search className="h-3.5 w-3.5" />
      </button>
      <button
        type="button"
        onClick={onOpenSettings}
        title="Settings"
        className="flex items-center rounded p-1 text-[var(--text-muted)] hover:bg-[var(--surface-hover)] hover:text-[var(--text)] cursor-pointer"
      >
        <Settings2 className="h-3.5 w-3.5" />
      </button>

      {menu && (
        <ContextMenu x={menu.x} y={menu.y} items={items} onClose={() => setMenu(null)} />
      )}
    </div>
  );
}
