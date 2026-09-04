/**
 * The application top bar: the scryer logo on the left is the app action menu
 * (open / close project, settings); the project name sits beside it for
 * context, and search lives on the right. Page content lives below; this bar
 * is app chrome only.
 *
 * The window is frameless (`decorations: false`), so this bar IS the titlebar:
 * every non-interactive part of it carries `data-tauri-drag-region` to move the
 * window, and the minimize / maximize / close controls live at its right edge.
 * The attribute does not inherit — each element the cursor can land on needs
 * its own, so the flex wings and the project path carry it too.
 */

import { useEffect, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  ChevronDown,
  Copy,
  FileText,
  Inbox,
  Minus,
  Moon,
  Network,
  Search,
  Square,
  Sun,
  X,
} from "lucide-react";
import { ContextMenu, type ContextMenuItem } from "./ContextMenu";
import { applyColorMode, loadTheme, saveTheme, type ColorMode } from "./theme";

export type WorkspaceView = "wiki" | "diagram";

// The search shortcut chip, honest about the platform (⌘ exists only on mac).
const IS_MAC = /Mac|iP/.test(navigator.userAgent);
const SEARCH_KEY = IS_MAC ? "⌘K" : "Ctrl K";

export function TopBar({
  projectPath,
  view,
  onSetView,
  onOpenProject,
  onCloseProject,
  onOpenSearch,
  onOpenSettings,
  inboxUnread = 0,
  inboxLive = false,
  inboxOpen = false,
  onOpenInbox,
}: {
  projectPath: string | null;
  view: WorkspaceView;
  onSetView: (view: WorkspaceView) => void;
  onOpenProject: (path: string) => void;
  onCloseProject: () => void;
  onOpenSearch: () => void;
  onOpenSettings: () => void;
  /** Cards in the inbox the developer has not seen. */
  inboxUnread?: number;
  /** A hook session is active — the badge pulses. */
  inboxLive?: boolean;
  /** The inbox page is the one showing — the entry reads pressed. */
  inboxOpen?: boolean;
  onOpenInbox?: () => void;
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
    // Three zones — identity | search | view — with the search dead-center
    // (the command-center idiom): equal flex wings on either side keep it
    // centered regardless of how long the project path runs.
    <div
      data-tauri-drag-region
      className="flex h-9 shrink-0 items-center border-b border-[var(--border)] bg-[var(--surface)] px-2 select-none"
    >
      {/* Box-centered, with a 1px optical nudge on the path below. NOT
          items-baseline: the button's exported baseline is its first child's —
          the logo image, i.e. its bottom edge — which drags the path ~3px low. */}
      <div data-tauri-drag-region className="flex min-w-0 flex-1 items-center">
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
            data-tauri-drag-region
            title={projectPath}
            // translate-y-px: 11px mono centered next to 12px sans sits ~1px
            // high by box math; the nudge lands the two baselines together.
            className="ml-2 hidden min-w-0 max-w-[340px] translate-y-px truncate font-mono text-2xs text-[var(--text-ghost)] sm:block"
          >
            {projectPath}
          </span>
        )}
      </div>

      <button
        type="button"
        onClick={onOpenSearch}
        title="Search the model (Ctrl+K)"
        className="flex h-7 w-[280px] max-w-[38vw] shrink-0 items-center gap-2 rounded-md border border-[var(--border)] bg-[var(--surface-canvas)] px-3 text-xs text-[var(--text-tertiary)] transition-colors hover:border-[var(--border-strong)] hover:text-[var(--text-secondary)] cursor-text"
      >
        <Search className="h-3.5 w-3.5 shrink-0" />
        <span className="truncate">Search the model</span>
        <span className="ml-auto shrink-0 rounded border border-[var(--border-strong)] px-1 font-mono text-2xs text-[var(--text-tertiary)]">
          {SEARCH_KEY}
        </span>
      </button>

      <div data-tauri-drag-region className="flex min-w-0 flex-1 items-center justify-end">
        {/* Wiki / Map view toggle — a primary nav surface onto the same model
            and selection. Ctrl+Space flips it. One joined control: single
            border, hairline dividers, the active cell filled. */}
        <div className="ml-2 flex items-stretch overflow-hidden rounded-md border border-[var(--border)] divide-x divide-[var(--border)]">
          {([
            { id: "wiki", label: "Wiki", Icon: FileText },
            { id: "diagram", label: "Map", Icon: Network },
          ] as const).map(({ id, label, Icon }) => (
            <button
              key={id}
              type="button"
              data-cam={`view-${id}`}
              title={`${label} view (Ctrl+Space to switch)`}
              aria-pressed={view === id}
              onClick={() => onSetView(id)}
              className={`flex items-center gap-1.5 px-3 py-1 text-xs font-medium transition-colors ${
                view === id
                  ? "bg-[var(--surface-active)] text-[var(--text)]"
                  : "text-[var(--text-tertiary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text)]"
              }`}
            >
              <Icon className="h-3.5 w-3.5" />
              {label}
            </button>
          ))}
        </div>

        {/* The inbox — the in-session verdict queue. Unread count as a badge;
            while a hook session is live the badge pulses (the queue is being
            fed right now), otherwise it is the same queue at rest. */}
        {onOpenInbox && (
          <button
            type="button"
            data-cam="inbox"
            onClick={onOpenInbox}
            aria-pressed={inboxOpen}
            title={
              inboxUnread > 0
                ? `Inbox — ${inboxUnread} unread item${inboxUnread === 1 ? "" : "s"} awaiting your verdict${inboxLive ? " (session live)" : ""}`
                : `Inbox — nothing unread${inboxLive ? " (session live)" : ""}`
            }
            className={`relative ml-1.5 flex h-7 items-center gap-1.5 rounded-md border px-2 text-xs font-medium transition-colors ${
              inboxOpen
                ? "border-[var(--border)] bg-[var(--surface-active)] text-[var(--text)]"
                : "border-transparent text-[var(--text-tertiary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text)]"
            }`}
          >
            <Inbox className="h-3.5 w-3.5" />
            <span className="hidden md:inline">Inbox</span>
            {inboxUnread > 0 && (
              <span
                className={`inline-flex min-w-[18px] items-center justify-center rounded-full px-1 font-mono text-2xs font-semibold tabular-nums ${
                  inboxLive
                    ? "animate-pulse bg-violet-600 text-white dark:bg-violet-500"
                    : "bg-orange-600 text-white dark:bg-orange-500"
                }`}
              >
                {inboxUnread > 99 ? "99+" : inboxUnread}
              </span>
            )}
          </button>
        )}

        <ThemeToggle />
        <WindowControls />
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

/** Minimize / maximize / close for the frameless window. Without these the
 *  window can only be quit from outside the app, since `decorations: false`
 *  removes the native controls along with the titlebar. The maximize icon
 *  mirrors the real window state, which the OS can change behind our back
 *  (a double-click on the drag region, a window-manager shortcut), so it is
 *  re-read on every resize rather than tracked locally. */
export function WindowControls({ divider = true }: { divider?: boolean } = {}) {
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    const win = getCurrentWindow();
    let alive = true;
    let unlisten: (() => void) | undefined;

    const sync = () => {
      void win
        .isMaximized()
        .then((m) => {
          if (alive) setMaximized(m);
        })
        .catch(() => {});
    };

    sync();
    void win
      .onResized(sync)
      .then((fn) => {
        // The listener may resolve after unmount; drop it rather than leak.
        if (alive) unlisten = fn;
        else fn();
      })
      .catch(() => {});

    return () => {
      alive = false;
      unlisten?.();
    };
  }, []);

  const controls = [
    {
      id: "minimize",
      label: "Minimize",
      Icon: Minus,
      onClick: () => getCurrentWindow().minimize(),
    },
    {
      id: "maximize",
      label: maximized ? "Restore" : "Maximize",
      Icon: maximized ? Copy : Square,
      onClick: () => getCurrentWindow().toggleMaximize(),
    },
    {
      id: "close",
      label: "Close",
      Icon: X,
      onClick: () => getCurrentWindow().close(),
      danger: true,
    },
  ];

  return (
    // Separated from the app's own controls by a hairline: these act on the
    // window, not on the model, and shouldn't read as part of the toolbar.
    // On a bare titlebar there is nothing to separate from, hence `divider`.
    <div
      className={`flex shrink-0 items-center gap-0.5 ${
        divider ? "ml-2 border-l border-[var(--border)] pl-2" : ""
      }`}
    >
      {controls.map(({ id, label, Icon, onClick, danger }) => (
        <button
          key={id}
          type="button"
          title={label}
          aria-label={label}
          onClick={() => void onClick().catch(() => {})}
          className={`flex h-6 w-6 items-center justify-center rounded-md text-[var(--text-tertiary)] ${
            // Close gets the conventional red wash — no palette token for it,
            // and it should read as terminal rather than as one more toolbar hit.
            danger
              ? "hover:bg-red-600 hover:text-white"
              : "hover:bg-[var(--surface-hover)] hover:text-[var(--text)]"
          }`}
        >
          <Icon className="h-3.5 w-3.5" />
        </button>
      ))}
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
