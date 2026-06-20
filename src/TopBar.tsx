/**
 * The application top bar: the scryer logo on the left is the app action menu
 * (open / close project, settings); the project name sits beside it for
 * context, and search lives on the right. Page content lives below; this bar
 * is app chrome only.
 */

import { useEffect, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { ChevronDown, FileText, Moon, Network, Search, Sun } from "lucide-react";
import { ContextMenu, type ContextMenuItem } from "./ContextMenu";
import { applyColorMode, loadTheme, saveTheme, type ColorMode } from "./theme";

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
        className="flex min-w-0 items-center gap-2 rounded-md px-2 py-1 text-xs font-semibold text-[var(--text)] hover:bg-[var(--surface-hover)]"
      >
        <img src="/logo.png" alt="scryer" className="h-3.5 w-3.5 shrink-0 rounded" />
        <span className="truncate">{projectName}</span>
        <ChevronDown className="h-3 w-3 shrink-0 text-[var(--text-ghost)]" />
      </button>

      {projectPath && (
        <span
          title={projectPath}
          className="ml-2 hidden max-w-[340px] truncate font-mono text-[11px] text-[var(--text-ghost)] sm:inline-block"
        >
          {projectPath}
        </span>
      )}

      <div className="flex-1" />

      <button
        type="button"
        onClick={onOpenSearch}
        title="Search the model (Ctrl+K)"
        className="flex h-7 w-[240px] items-center gap-2 rounded-md border border-[var(--border)] bg-[var(--surface-canvas)] px-3 text-xs text-[var(--text-secondary)] hover:border-[var(--border-strong)] cursor-text"
      >
        <Search className="h-4 w-4 shrink-0" />
        <span className="truncate">Search the model</span>
        <span className="ml-auto shrink-0 rounded border border-[var(--border-strong)] px-1 font-mono text-2xs text-[var(--text-tertiary)]">
          ⌘K
        </span>
      </button>

      {/* Wiki / Map view toggle — a primary nav surface onto the same model and
          selection, so it reads big and high-contrast. Ctrl+Space flips it. */}
      <div className="ml-2 flex items-center gap-0.5 rounded-md border border-[var(--border)] bg-[var(--surface-canvas)] p-0.5">
        {([
          { id: "wiki", label: "Wiki", Icon: FileText },
          { id: "diagram", label: "Map", Icon: Network },
        ] as const).map(({ id, label, Icon }) => (
          <button
            key={id}
            type="button"
            title={`${label} view (Ctrl+Space to switch)`}
            aria-pressed={view === id}
            onClick={() => onSetView(id)}
            className={`flex items-center gap-1.5 rounded px-3 py-1 text-xs font-medium transition-colors ${
              view === id
                ? "bg-[var(--surface-raised)] text-[var(--text)] shadow-sm"
                : "text-[var(--text-tertiary)] hover:text-[var(--text)]"
            }`}
          >
            <Icon className="h-4 w-4" />
            {label}
          </button>
        ))}
      </div>

      <ThemeToggle />

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

/** Binary light/dark switch for the top bar. Flips the currently-resolved mode
 *  to an explicit choice (so it overrides "system" on first click), persists it,
 *  and applies it. The icon mirrors `<html>.dark` via an observer, staying honest
 *  if the OS flips while still on "system" mode (the theme's own listener toggles
 *  that class). */
function ThemeToggle() {
  const [isDark, setIsDark] = useState(() =>
    document.documentElement.classList.contains("dark"),
  );

  useEffect(() => {
    const el = document.documentElement;
    const obs = new MutationObserver(() => setIsDark(el.classList.contains("dark")));
    obs.observe(el, { attributes: true, attributeFilter: ["class"] });
    return () => obs.disconnect();
  }, []);

  const toggle = () => {
    const next: ColorMode = isDark ? "light" : "dark";
    const theme = loadTheme();
    theme.colorMode = next;
    saveTheme(theme);
    applyColorMode(next);
  };

  return (
    <button
      type="button"
      onClick={toggle}
      title={isDark ? "Switch to light mode" : "Switch to dark mode"}
      aria-label={isDark ? "Switch to light mode" : "Switch to dark mode"}
      className="ml-1.5 flex h-6 w-6 shrink-0 items-center justify-center rounded-md text-[var(--text-tertiary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text)]"
    >
      {isDark ? <Sun className="h-3.5 w-3.5" /> : <Moon className="h-3.5 w-3.5" />}
    </button>
  );
}
