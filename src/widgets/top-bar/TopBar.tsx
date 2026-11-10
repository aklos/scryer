/**
 * The application top bar: the scryer logo on the left is the app action menu
 * (open / close project, settings) with the project name beside it and, when
 * the project has several git worktrees, the open checkout's branch, a menu
 * that switches to another checkout, and what the checkout changed — "Uncommitted
 * changes" or "N commits ahead of main" — which opens Activity, where those
 * changes are listed; search sits
 * dead-centre; the switch that lets agents move the view, the Wiki / Map view
 * switch (plus Activity, when the host passes an activity page), theme toggle
 * and window controls are on the right. Page
 * content lives below; this bar is app chrome only.
 *
 * The window is frameless (`decorations: false`), so this bar IS the titlebar:
 * every non-interactive part of it carries `data-tauri-drag-region` to move the
 * window. The attribute does not inherit — each element the cursor can land on
 * needs its own. On macOS the OS draws its own traffic lights and the bar leaves
 * room for them; elsewhere the bar draws minimize / maximize / close.
 *
 * As the window narrows the bar sheds words, never controls, one at a time and
 * only when they no longer fit (see `useShedStage`): the path goes first, then
 * the changes label, the branch, the search label and
 * last the view labels, each down to its icon. Only then does the project name
 * truncate. Nothing is cut mid-word before that.
 */

import { useEffect, useRef, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  ChevronDown,
  Copy,
  FileText,
  GitBranch,
  GitCompare,
  History,
  Minus,
  Moon,
  Network,
  PanelLeftClose,
  PanelLeftOpen,
  Search,
  Square,
  Sun,
  X,
} from "lucide-react";
import { ContextMenu, type ContextMenuItem } from "../../features/search/ContextMenu";
import { useWorktreeList } from "../../features/worktrees/useWorktrees";
import type { ChangeBase } from "../../features/model-storage/useModelStorage";
import type { ModelChanges } from "../../entities/model/changes";
import { useShedStage } from "./useShedStage";
import { applyColorMode, loadTheme, saveTheme, type ColorMode } from "../../shared/theme/theme";

export type WorkspaceView = "wiki" | "diagram";

// The search shortcut chip, honest about the platform (⌘ exists only on mac).
export const IS_MAC = /Mac|iP/.test(navigator.userAgent);
/** On macOS the window keeps its NATIVE traffic lights, drawn by the OS over
 *  the webview (`tauri.macos.conf.json`: overlay title bar). This is the room
 *  the top bar leaves them. */
const MAC_TRAFFIC_LIGHTS_WIDTH = 72;
const SEARCH_KEY = IS_MAC ? "⌘K" : "Ctrl K";

/** The words the bar sheds as it narrows, in order: at stage n, every piece
 *  numbered below n is gone. Past the last, the project name truncates. */
const SHED = { path: 1, changes: 2, branch: 3, search: 4, views: 5 } as const;
const LAST_STAGE = SHED.views;

export function TopBar({
  projectPath,
  view,
  onSetView,
  onOpenProject,
  onOpenWorktree,
  onCloseProject,
  onOpenSearch,
  onOpenSettings,
  sessionBadge = 0,
  sessionLive = false,
  sessionOpen = false,
  onOpenSession,
  treeOpen = true,
  onToggleTree,
  changeBase = null,
  changes,
}: {
  projectPath: string | null;
  view: WorkspaceView;
  onSetView: (view: WorkspaceView) => void;
  onOpenProject: (path: string) => void;
  /** Switch to another worktree of the open project. */
  onOpenWorktree: (path: string) => void;
  onCloseProject: () => void;
  onOpenSearch: () => void;
  onOpenSettings: () => void;
  /** Plan entries the session on screen left unbuilt. */
  sessionBadge?: number;
  /** A hook session is active — the badge pulses (a dot when it is empty). */
  sessionLive?: boolean;
  /** The session page is the one showing — the entry reads pressed. */
  sessionOpen?: boolean;
  onOpenSession?: () => void;
  /** The tree is showing — docked or as a drawer. */
  treeOpen?: boolean;
  /** Show or hide the tree; the toggle draws only when a host passes this. */
  onToggleTree?: () => void;
  /** Where the checkout's changes are read from; null outside git. */
  changeBase?: ChangeBase | null;
  /** What changed in the model since `changeBase`. */
  changes?: ModelChanges;
}) {
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const [worktreeMenu, setWorktreeMenu] = useState<{ x: number; y: number } | null>(null);
  const worktrees = useWorktreeList(projectPath);
  const here = worktrees.list.find((w) => w.current);
  const projectName = projectPath?.split("/").filter(Boolean).pop() ?? "scryer";
  const bar = useRef<HTMLDivElement>(null);
  const stage = useShedStage(
    bar,
    LAST_STAGE,
    [projectPath, worktrees.list.length > 1 && here?.branch, changesLabel(changeBase), sessionBadge, sessionLive, !!onOpenSession].join("|"),
  );
  const shows = (piece: keyof typeof SHED) => stage < SHED[piece];

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
      ref={bar}
      data-tauri-drag-region
      className="flex h-9 shrink-0 items-center overflow-hidden border-b border-[var(--border)] bg-[var(--surface)] px-2 select-none"
    >
      {/* Box-centered, with a 1px optical nudge on the path below. NOT
          items-baseline: the button's exported baseline is its first child's —
          the logo image, i.e. its bottom edge — which drags the path ~3px low. */}
      <div
        data-tauri-drag-region
        className={`flex flex-1 items-center ${stage >= LAST_STAGE ? "min-w-0" : "min-w-max"}`}
      >
        {IS_MAC && (
          <div data-tauri-drag-region className="shrink-0" style={{ width: MAC_TRAFFIC_LIGHTS_WIDTH }} />
        )}
        {onToggleTree && (
          <button
            type="button"
            data-tree-toggle
            onClick={onToggleTree}
            title={`${treeOpen ? "Hide" : "Show"} the tree (${IS_MAC ? "⌘B" : "Ctrl+B"})`}
            aria-label={treeOpen ? "Hide the tree" : "Show the tree"}
            aria-pressed={treeOpen}
            className="mr-0.5 flex h-6 w-6 shrink-0 items-center justify-center rounded-md text-[var(--text-tertiary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text)]"
          >
            {treeOpen ? <PanelLeftClose className="h-3.5 w-3.5" /> : <PanelLeftOpen className="h-3.5 w-3.5" />}
          </button>
        )}
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

        {worktrees.list.length > 1 && (
          <button
            type="button"
            data-worktree-switch
            onClick={(e) => {
              const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
              worktrees.refresh();
              setWorktreeMenu({ x: r.left, y: r.bottom + 2 });
            }}
            title="Switch to another worktree of this project"
            aria-label={`Branch ${here?.branch ?? "detached"}: switch worktree`}
            className="ml-1 flex shrink-0 items-center gap-1 rounded-md px-1.5 py-1 text-xs whitespace-nowrap text-[var(--text-secondary)] hover:bg-[var(--surface-hover)]"
          >
            <GitBranch className="h-3 w-3 shrink-0 text-[var(--text-ghost)]" />
            {shows("branch") && <span className="font-mono">{here?.branch ?? "detached"}</span>}
            <ChevronDown className="h-3 w-3 shrink-0 text-[var(--text-ghost)]" />
          </button>
        )}

        <ChangesStatus
          base={changeBase}
          changes={changes}
          onOpen={onOpenSession}
          compact={!shows("changes")}
        />

        {projectPath && shows("path") && (
          <span
            data-tauri-drag-region
            title={projectPath}
            // translate-y-px: 11px mono centered next to 12px sans sits ~1px
            // high by box math; the nudge lands the two baselines together.
            className="ml-2 block max-w-[340px] translate-y-px truncate font-mono text-2xs text-[var(--text-ghost)]"
          >
            {projectPath}
          </span>
        )}
      </div>

      <button
        type="button"
        onClick={onOpenSearch}
        title={`Search the model (${SEARCH_KEY})`}
        aria-label="Search the model"
        className={`flex h-7 shrink-0 items-center gap-2 rounded-md border border-[var(--border)] bg-[var(--surface-canvas)] text-xs whitespace-nowrap text-[var(--text-tertiary)] transition-colors hover:border-[var(--border-strong)] hover:text-[var(--text-secondary)] cursor-text ${
          shows("search") ? "mx-2 w-[280px] justify-start px-3" : "mx-1 w-7 justify-center"
        }`}
      >
        <Search className="h-3.5 w-3.5 shrink-0" />
        {shows("search") && (
          <>
            <span>Search the model</span>
            <span className="ml-auto shrink-0 rounded border border-[var(--border-strong)] px-1 font-mono text-2xs text-[var(--text-tertiary)]">
              {SEARCH_KEY}
            </span>
          </>
        )}
      </button>

      <div data-tauri-drag-region className="flex min-w-max flex-1 items-center justify-end">
        {/* Wiki / Map / Activity — three destinations, one joined control,
            exactly one lit. Activity is a wiki page underneath,
            but to the user it is its own place: lighting Wiki too read as two
            selections. Its cell carries a badge when a caller passes one. */}
        <div className="ml-2 flex shrink-0 items-stretch overflow-hidden rounded-md border border-[var(--border)] divide-x divide-[var(--border)]">
          {([
            { id: "wiki", label: "Wiki", Icon: FileText, title: "Wiki view (Ctrl+Space to switch)" },
            { id: "diagram", label: "Map", Icon: Network, title: "Map view (Ctrl+Space to switch)" },
            ...(onOpenSession
              ? [{
                  id: "session",
                  label: "Activity",
                  Icon: History,
                  title: `Activity — the agents working here and what each commit changed in the model${sessionLive ? " (an agent is working now)" : ""}`,
                }]
              : []),
          ] as const).map(({ id, label, Icon, title }) => {
            const lit = sessionOpen ? id === "session" : id === view;
            return (
              <button
                key={id}
                type="button"
                data-cam={id === "session" ? "session" : `view-${id}`}
                title={title}
                aria-label={label}
                aria-pressed={lit}
                onClick={() => (id === "session" ? onOpenSession?.() : onSetView(id as WorkspaceView))}
                className={`flex items-center gap-1.5 py-1 text-xs font-medium whitespace-nowrap transition-colors ${shows("views") ? "px-3" : "px-2"} ${
                  lit
                    ? "bg-[var(--surface-active)] text-[var(--text)]"
                    : "text-[var(--text-tertiary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text)]"
                }`}
              >
                <Icon className="h-3.5 w-3.5" />
                {shows("views") && <span>{label}</span>}
                {id === "session" && sessionBadge > 0 && (
                  <span
                    className={`inline-flex min-w-[18px] items-center justify-center rounded-full px-1 font-mono text-2xs font-semibold tabular-nums ${
                      sessionLive
                        ? "animate-pulse bg-violet-600 text-white dark:bg-violet-500"
                        : "bg-orange-600 text-white dark:bg-orange-500"
                    }`}
                  >
                    {sessionBadge > 99 ? "99+" : sessionBadge}
                  </span>
                )}
                {id === "session" && sessionBadge === 0 && sessionLive && (
                  <span className="h-1.5 w-1.5 animate-pulse rounded-full bg-violet-600 dark:bg-violet-500" />
                )}
              </button>
            );
          })}
        </div>

        <ThemeToggle />
        {!IS_MAC && <WindowControls />}
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
      {worktreeMenu && (
        <ContextMenu
          x={worktreeMenu.x}
          y={worktreeMenu.y}
          items={worktrees.list.map((w) => ({
            id: w.path,
            label: w.branch ?? "detached",
            hint: w.current ? "open" : undefined,
            onSelect: () => {
              if (!w.current) onOpenWorktree(w.path);
            },
          }))}
          searchable={false}
          onClose={() => setWorktreeMenu(null)}
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
/** The window's minimize / maximize / close buttons. Drawn here on Windows and
 *  Linux, where the window is frameless; on macOS the OS draws its own traffic
 *  lights, so there is nothing to render. */
export function WindowControls(props: { divider?: boolean } = {}) {
  return IS_MAC ? null : <DrawnWindowControls {...props} />;
}

function DrawnWindowControls({ divider = true }: { divider?: boolean }) {
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

/** What the checkout changed, in words: the commits ahead of the branch it
 *  diverged from, and whether anything is uncommitted; null when neither. */
export function changesLabel(base: ChangeBase | null): string | null {
  if (!base) return null;
  const ahead = base.against && base.ahead > 0 ? `${base.ahead} commit${base.ahead === 1 ? "" : "s"} ahead of ${base.against}` : null;
  if (ahead) return base.dirty ? `${ahead} · uncommitted` : ahead;
  return base.dirty ? "Uncommitted changes" : null;
}

/** What the checkout changed, as a status: opens Activity, where the changes
 *  are listed, when the host passes it. Drawn only while there is something
 *  to say. */
export function ChangesStatus({
  base,
  changes,
  onOpen,
  compact = false,
}: {
  base: ChangeBase | null;
  changes?: ModelChanges;
  onOpen?: () => void;
  /** Icon only; the words stay in the tooltip. */
  compact?: boolean;
}) {
  const label = changesLabel(base);
  if (!label) return null;
  const changed = changes?.nodes.size ?? 0;
  const removed = changes?.removed ?? 0;
  const tally =
    changed + removed === 0
      ? "nothing in the model changed"
      : [changed > 0 && `${changed} node${changed === 1 ? "" : "s"} changed`, removed > 0 && `${removed} removed`]
          .filter(Boolean)
          .join(", ");
  return (
    <button
      type="button"
      data-changes-status
      onClick={onOpen}
      disabled={!onOpen}
      title={`${label}: ${tally}.${onOpen ? " Open Activity to see them." : ""}`}
      aria-label={label}
      className="ml-1 flex shrink-0 items-center gap-1 rounded-md px-1.5 py-1 text-xs whitespace-nowrap text-[var(--text-secondary)] enabled:hover:bg-[var(--surface-hover)]"
    >
      <GitCompare className="h-3 w-3 shrink-0 text-amber-700 dark:text-amber-400" />
      {!compact && <span>{label}</span>}
    </button>
  );
}
