/**
 * The application top bar: the scryer logo on the left is the app action menu
 * (open / close project, settings); the project name sits beside it for
 * context, and search lives on the right. Page content lives below; this bar
 * is app chrome only.
 */

import { useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { ChevronDown, FileText, Network, Search } from "lucide-react";
import { ContextMenu, type ContextMenuItem } from "./ContextMenu";

export type WorkspaceView = "wiki" | "diagram";

export function TopBar({
  projectPath,
  view,
  onSetView,
  onOpenProject,
  onCloseProject,
  onOpenSearch,
  onOpenSettings,
}: {
  projectPath: string | null;
  view: WorkspaceView;
  onSetView: (view: WorkspaceView) => void;
  onOpenProject: (path: string) => void;
  onCloseProject: () => void;
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
    { id: "close", label: "Close project", onSelect: onCloseProject },
    { id: "settings", label: "Settings", onSelect: onOpenSettings },
  ];

  return (
    <div className="flex h-9 shrink-0 items-center gap-0.5 border-b border-[var(--border)] bg-[var(--surface)] px-2 select-none">
      <button
        type="button"
        onClick={(e) => {
          const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
          setMenu({ x: r.left, y: r.bottom + 2 });
        }}
        title={projectPath ?? undefined}
        className="flex min-w-0 items-center gap-2 rounded-md px-2 py-1 text-xs font-semibold text-[var(--text)] hover:bg-[var(--surface-hover)] cursor-pointer"
      >
        <img src="/logo.png" alt="scryer" className="h-3.5 w-3.5 shrink-0 rounded" />
        <span className="truncate">{projectName}</span>
        <ChevronDown className="h-3 w-3 shrink-0 text-[var(--text-ghost)]" />
      </button>

      {projectPath && (
        <>
          <div className="mx-2 h-4 w-px shrink-0 bg-[var(--border-strong)]" />
          <span className="hidden truncate font-mono text-[11px] text-[var(--text-tertiary)] sm:inline">
            .scryer/model.scry
          </span>
        </>
      )}

      <div className="flex-1" />

      <button
        type="button"
        onClick={onOpenSearch}
        title="Search the model (Ctrl+K)"
        className="flex h-6 w-[230px] items-center gap-1.5 rounded-md border border-[var(--border)] bg-[var(--surface-canvas)] px-2.5 text-xs text-[var(--text-tertiary)] hover:border-[var(--border-strong)] cursor-text"
      >
        <Search className="h-3.5 w-3.5 shrink-0" />
        <span className="truncate">Search the model</span>
        <span className="ml-auto shrink-0 rounded border border-[var(--border-strong)] px-1 font-mono text-[10px] text-[var(--text-ghost)]">
          ⌘K
        </span>
      </button>

      {/* Wiki / Map view toggle — the diagram is the secondary nav surface onto
          the same model and selection. */}
      <div className="ml-1.5 flex items-center gap-0.5 rounded-md border border-[var(--border)] bg-[var(--surface-canvas)] p-0.5">
        {([
          { id: "wiki", label: "Wiki", Icon: FileText },
          { id: "diagram", label: "Map", Icon: Network },
        ] as const).map(({ id, label, Icon }) => (
          <button
            key={id}
            type="button"
            title={`${label} view`}
            aria-pressed={view === id}
            onClick={() => onSetView(id)}
            className={`flex items-center gap-1 rounded px-2.5 py-0.5 text-2xs font-medium transition-colors cursor-pointer ${
              view === id
                ? "bg-[var(--surface-active)] text-[var(--text)]"
                : "text-[var(--text-muted)] hover:text-[var(--text-secondary)]"
            }`}
          >
            <Icon className="h-3.5 w-3.5" />
            {label}
          </button>
        ))}
      </div>

      {menu && (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          items={items}
          searchable={false}
          onClose={() => setMenu(null)}
        />
      )}
    </div>
  );
}
