import { useCallback, useEffect, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { Minus, Square, X, Settings, Keyboard, FolderX, SaveAll, Menu, FolderOpen } from "lucide-react";
import type { C4Kind, AiToolsState } from "./types";
import type { RackDependency } from "./CodeLevelRack";

interface TopBarProps {
  currentModel: string | null;
  onOpenPalette: () => void;
  onNavigateToRoot: () => void;
  onOpenSettings: () => void;
  onCloseModel: () => void;
  onSaveAs: () => void;
  hasModel: boolean;

  breadcrumbs: { id: string; name: string; kind: C4Kind }[];
  currentParentKind: C4Kind | undefined;
  navigateToBreadcrumb: (targetId: string | null) => void;
  activeFlowId: string | null;
  activeFlowName: string | null;
  dependencies?: RackDependency[];
  onNavigateToNode?: (id: string) => void;

  projectPath?: string;
  aiTools: AiToolsState;
  onAiToolsChange: (tools: AiToolsState) => void;
  onSetProjectPath: (path: string | undefined) => void;
}

const appWindow = getCurrentWindow();

function Toggle({ checked, onChange }: { checked: boolean; onChange: (v: boolean) => void }) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      className={`relative inline-flex h-5 w-9 shrink-0 cursor-pointer rounded-full transition-colors ${
        checked ? "bg-blue-500" : "bg-zinc-300 dark:bg-zinc-600"
      }`}
      onClick={() => onChange(!checked)}
    >
      <span
        className={`pointer-events-none inline-block h-4 w-4 rounded-full bg-white shadow transform transition-transform mt-0.5 ${
          checked ? "translate-x-[18px]" : "translate-x-0.5"
        }`}
      />
    </button>
  );
}

function ProjectMenu({
  onClose,
  projectPath,
  aiTools,
  onAiToolsChange,
  triggerRef,
  onSetProjectPath,
  onSaveAs,
}: {
  onClose: () => void;
  projectPath?: string;
  aiTools: AiToolsState;
  onAiToolsChange: (tools: AiToolsState) => void;
  triggerRef: React.RefObject<HTMLButtonElement | null>;
  onSetProjectPath: (path: string | undefined) => void;
  onSaveAs: () => void;
}) {
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const handler = (e: PointerEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)
          && !(triggerRef.current && triggerRef.current.contains(e.target as Node))) {
        onClose();
      }
    };
    document.addEventListener("pointerdown", handler, true);
    return () => document.removeEventListener("pointerdown", handler, true);
  }, [onClose, triggerRef]);

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [onClose]);

  const showClaude = aiTools.claude && !!projectPath;
  const showCodex = aiTools.codex && !!projectPath;

  const handleToggle = async (field: "claudeMcpEnabled" | "codexMcpEnabled" | "claudeReadApproved", checked: boolean) => {
    const actionMap: Record<string, string> = {
      claudeMcpEnabled: "mcp",
      codexMcpEnabled: "mcp_codex",
      claudeReadApproved: "claude_read_approve",
    };
    try {
      await invoke<string>("setup_mcp_integration", {
        action: actionMap[field],
        projectPath: projectPath ?? null,
      });
      onAiToolsChange({ ...aiTools, [field]: checked });
    } catch {
      // silently fail — user will see toggle didn't change
    }
  };

  return (
    <div
      ref={menuRef}
      className="absolute top-full left-0 mt-1 z-50 min-w-[240px] rounded-lg border border-zinc-200/80 bg-white/80 shadow-sm backdrop-blur-sm dark:border-zinc-700/80 dark:bg-zinc-900/80 py-1"
    >
      <button
        type="button"
        className="flex items-center gap-2 w-full px-3 py-1.5 text-xs text-zinc-600 hover:bg-zinc-100 dark:text-zinc-300 dark:hover:bg-zinc-700 cursor-pointer transition-colors"
        onClick={() => { onSaveAs(); onClose(); }}
      >
        <SaveAll className="h-3.5 w-3.5 text-zinc-400 dark:text-zinc-500" />
        <span className="flex-1 text-left">Save as…</span>
      </button>
      <button
        type="button"
        className="flex items-center gap-2 w-full px-3 py-1.5 text-xs text-zinc-600 hover:bg-zinc-100 dark:text-zinc-300 dark:hover:bg-zinc-700 cursor-pointer transition-colors"
        onClick={async () => {
          const selected = await openDialog({ directory: true, title: "Select project folder", defaultPath: projectPath });
          if (selected) onSetProjectPath(selected);
        }}
      >
        <FolderOpen className="h-3.5 w-3.5 text-zinc-400 dark:text-zinc-500" />
        <span className="flex-1 text-left truncate">{projectPath ? "Change codebase" : "Link codebase"}</span>
      </button>
      {projectPath && (
        <div className="px-3 py-1 text-[10px] text-zinc-400 dark:text-zinc-500 truncate max-w-[240px]" title={projectPath}>
          {projectPath}
        </div>
      )}

      {(showClaude || showCodex) && (
        <>
          <div className="my-1 border-t border-zinc-200/60 dark:border-zinc-700/60" />
          <div className="px-3 py-1.5 text-[10px] font-medium uppercase tracking-wider text-zinc-400 dark:text-zinc-500">
            MCP Server
          </div>
          {showClaude && (
            <>
              <div className="flex items-center justify-between px-3 py-1.5">
                <div>
                  <div className="text-xs text-zinc-600 dark:text-zinc-300">Claude Code</div>
                  <div className="text-[10px] text-zinc-400 dark:text-zinc-500 max-w-[160px]">.mcp.json</div>
                </div>
                <Toggle
                  checked={aiTools.claudeMcpEnabled}
                  onChange={(checked) => { if (checked) handleToggle("claudeMcpEnabled", checked); }}
                />
              </div>
              {aiTools.claudeMcpEnabled && (
                <div className="flex items-center justify-between px-3 py-1.5 pl-6">
                  <div>
                    <div className="text-xs text-zinc-600 dark:text-zinc-300">Auto-approve reads</div>
                    <div className="text-[10px] text-zinc-400 dark:text-zinc-500 max-w-[140px]">settings.local.json</div>
                  </div>
                  <Toggle
                    checked={aiTools.claudeReadApproved}
                    onChange={(checked) => { if (checked) handleToggle("claudeReadApproved", checked); }}
                  />
                </div>
              )}
            </>
          )}
          {showCodex && (
            <div className="flex items-center justify-between px-3 py-1.5">
              <div>
                <div className="text-xs text-zinc-600 dark:text-zinc-300">Codex</div>
                <div className="text-[10px] text-zinc-400 dark:text-zinc-500 max-w-[160px]">.codex/config.toml</div>
              </div>
              <Toggle
                checked={aiTools.codexMcpEnabled}
                onChange={(checked) => { if (checked) handleToggle("codexMcpEnabled", checked); }}
              />
            </div>
          )}
        </>
      )}
    </div>
  );
}

function AppMenu({ onClose, onOpenSettings, onOpenPalette, onCloseModel, hasModel, triggerRef }: { onClose: () => void; onOpenSettings: () => void; onOpenPalette: () => void; onCloseModel: () => void; hasModel: boolean; triggerRef: React.RefObject<HTMLButtonElement | null> }) {
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const handler = (e: PointerEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)
          && !(triggerRef.current && triggerRef.current.contains(e.target as Node))) {
        onClose();
      }
    };
    document.addEventListener("pointerdown", handler, true);
    return () => document.removeEventListener("pointerdown", handler, true);
  }, [onClose, triggerRef]);

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [onClose]);

  const items: { label: string; icon: typeof Settings; shortcut?: string; onClick: () => void; disabled?: boolean; active?: boolean }[] = [
    { label: "Open model", icon: Keyboard, shortcut: "Ctrl+K", onClick: () => { onOpenPalette(); onClose(); } },
    { label: "Close model", icon: FolderX, onClick: () => { onCloseModel(); onClose(); }, disabled: !hasModel },
    { label: "Settings", icon: Settings, onClick: () => { onOpenSettings(); onClose(); } },
  ];

  return (
    <div
      ref={menuRef}
      className="absolute top-full left-0 mt-1 z-50 min-w-[180px] rounded-lg border border-zinc-200/80 bg-white/80 shadow-sm backdrop-blur-sm dark:border-zinc-700/80 dark:bg-zinc-900/80 py-1"
    >
      {items.map((item) => (
        <button
          key={item.label}
          type="button"
          className={`flex items-center gap-2 w-full px-3 py-1.5 text-xs transition-colors ${item.disabled ? "text-zinc-300 dark:text-zinc-600 cursor-default" : "text-zinc-600 hover:bg-zinc-100 dark:text-zinc-300 dark:hover:bg-zinc-700 cursor-pointer"}`}
          onClick={item.disabled ? undefined : item.onClick}
        >
          <item.icon className={`h-3.5 w-3.5 ${item.active ? "text-blue-500 dark:text-blue-400" : "text-zinc-400 dark:text-zinc-500"}`} />
          <span className="flex-1 text-left">{item.label}</span>
          {item.active !== undefined && (
            <span className={`text-[10px] ${item.active ? "text-blue-500 dark:text-blue-400" : "text-zinc-400 dark:text-zinc-500"}`}>
              {item.active ? "on" : "off"}
            </span>
          )}
          {item.shortcut && (
            <span className="text-[10px] text-zinc-400 dark:text-zinc-500">{item.shortcut}</span>
          )}
        </button>
      ))}
    </div>
  );
}

export function TopBar({
  currentModel, onOpenPalette, onNavigateToRoot, onOpenSettings, onCloseModel, onSaveAs, hasModel,
  breadcrumbs, currentParentKind, navigateToBreadcrumb,
  activeFlowId, activeFlowName,
  dependencies = [], onNavigateToNode,
  projectPath, aiTools, onAiToolsChange, onSetProjectPath,
}: TopBarProps) {
  const isFlow = !!activeFlowId;
  const [menuOpen, setMenuOpen] = useState(false);
  const [projectMenuOpen, setProjectMenuOpen] = useState(false);
  const projectMenuTriggerRef = useRef<HTMLButtonElement>(null);
  const appMenuTriggerRef = useRef<HTMLButtonElement>(null);
  const toggleMenu = useCallback(() => setMenuOpen((prev) => !prev), []);

  return (
    <div
      className="flex items-center h-9 shrink-0 border-b border-zinc-200 bg-zinc-50 dark:border-zinc-700 dark:bg-zinc-900 select-none"
      data-tauri-drag-region
    >
      {/* Logo + app menu */}
      <div className="relative shrink-0 flex items-center">
        <button
          ref={appMenuTriggerRef}
          type="button"
          className="h-9 w-10 flex items-center justify-center text-zinc-400 hover:bg-zinc-200 dark:hover:bg-zinc-700/60 cursor-pointer transition-colors"
          onClick={toggleMenu}
          title="Menu"
        >
          <Menu className="h-4 w-4" />
        </button>
        {menuOpen && (
          <AppMenu
            onClose={() => setMenuOpen(false)}
            onOpenSettings={onOpenSettings}
            onOpenPalette={onOpenPalette}
            onCloseModel={onCloseModel}
            hasModel={hasModel}
            triggerRef={appMenuTriggerRef}
          />
        )}
      </div>

      {/* Left: model name */}
      <div
        className="w-50 shrink-0 flex items-center gap-1 px-2 border-r border-zinc-200 dark:border-zinc-700 h-full"
        data-tauri-drag-region
      >
        <span
          className="truncate flex-1 text-xs font-semibold text-zinc-700 dark:text-zinc-300 cursor-pointer"
          onClick={onNavigateToRoot}
        >
          {currentModel ?? "Untitled"}
        </span>
        {hasModel && (
          <div className="relative shrink-0">
            <button
              ref={projectMenuTriggerRef}
              type="button"
              className="rounded px-1.5 py-0.5 text-zinc-400 hover:text-zinc-600 hover:bg-zinc-200 dark:text-zinc-500 dark:hover:text-zinc-300 dark:hover:bg-zinc-700 text-xs shrink-0 cursor-pointer transition-colors"
              onClick={() => setProjectMenuOpen((prev) => !prev)}
            >
              &#8943;
            </button>
            {projectMenuOpen && (
              <ProjectMenu
                onClose={() => setProjectMenuOpen(false)}
                projectPath={projectPath}
                aiTools={aiTools}
                onAiToolsChange={onAiToolsChange}
                onSetProjectPath={onSetProjectPath}
                onSaveAs={onSaveAs}
                triggerRef={projectMenuTriggerRef}
              />
            )}
          </div>
        )}
      </div>

      {/* Center: level indicator + toolbar */}
      <div className="flex-1 flex items-center gap-2 px-3 h-full min-w-0" data-tauri-drag-region>
        {hasModel && !isFlow && (
          <div className="flex items-center gap-1.5 text-[11px] shrink-0">
            {breadcrumbs.length > 0 && (
              <button
                type="button"
                className="text-zinc-400 hover:text-zinc-700 dark:hover:text-white cursor-pointer transition-colors"
                onClick={() => {
                  const parent = breadcrumbs.length >= 2 ? breadcrumbs[breadcrumbs.length - 2].id : null;
                  navigateToBreadcrumb(parent);
                }}
                title="Go up"
              >
                &larr;
              </button>
            )}
            <span className={`rounded-full px-2 py-0.5 text-[10px] font-medium ${
              currentParentKind === "component" ? "bg-zinc-200/60 text-zinc-500 dark:bg-zinc-700/60 dark:text-zinc-400"
                : currentParentKind === "container" ? "bg-violet-100 text-violet-500 dark:bg-violet-900/30 dark:text-violet-400"
                : currentParentKind === "system" ? "bg-cyan-100 text-cyan-600 dark:bg-cyan-900/30 dark:text-cyan-400"
                : "bg-slate-100 text-slate-500 dark:bg-slate-800/50 dark:text-slate-400"
            }`}>
              {currentParentKind === "component" ? "Code"
                : currentParentKind === "container" ? "Components"
                : currentParentKind === "system" ? "Containers"
                : "System context"}
            </span>
            {breadcrumbs.length > 0 && (
              <span className="text-zinc-400 dark:text-zinc-500">
                {breadcrumbs[breadcrumbs.length - 1].name}
              </span>
            )}
            {dependencies.length > 0 && (
              <>
                <div className="w-px h-3 bg-zinc-300 dark:bg-zinc-600 mx-1" />
                {dependencies.filter((d) => d.direction === "out").length > 0 && (
                  <>
                    <span className="text-[10px] text-zinc-400 dark:text-zinc-500 shrink-0">depends on</span>
                    {dependencies.filter((d) => d.direction === "out").map((d) => (
                      <button
                        key={d.id}
                        type="button"
                        className="rounded px-1.5 py-0.5 text-[10px] text-zinc-500 dark:text-zinc-400 bg-zinc-100 dark:bg-zinc-800 hover:bg-zinc-200 dark:hover:bg-zinc-700 cursor-pointer transition-colors truncate max-w-[140px]"
                        onClick={() => onNavigateToNode?.(d.id)}
                        title={`${d.label || "depends on"} ${d.name}`}
                      >
                        {d.name}
                      </button>
                    ))}
                  </>
                )}
                {dependencies.filter((d) => d.direction === "in").length > 0 && (
                  <>
                    <span className="text-[10px] text-zinc-400 dark:text-zinc-500 shrink-0">used by</span>
                    {dependencies.filter((d) => d.direction === "in").map((d) => (
                      <button
                        key={d.id}
                        type="button"
                        className="rounded px-1.5 py-0.5 text-[10px] text-zinc-500 dark:text-zinc-400 bg-zinc-100 dark:bg-zinc-800 hover:bg-zinc-200 dark:hover:bg-zinc-700 cursor-pointer transition-colors truncate max-w-[140px]"
                        onClick={() => onNavigateToNode?.(d.id)}
                        title={`${d.label || "used by"} ${d.name}`}
                      >
                        {d.name}
                      </button>
                    ))}
                  </>
                )}
              </>
            )}
          </div>
        )}
        {hasModel && isFlow && activeFlowName && (
          <span className="text-[11px] text-zinc-400 dark:text-zinc-500 shrink-0">
            Flow: {activeFlowName}
          </span>
        )}
      </div>

      {/* Right: window controls */}
      <div className="flex items-center gap-1.5 shrink-0 pr-2.5">
        <button
          type="button"
          className="h-6 w-6 flex items-center justify-center rounded-full text-zinc-400 hover:text-zinc-600 hover:bg-zinc-200 dark:text-zinc-500 dark:hover:text-zinc-300 dark:hover:bg-zinc-700 transition-colors cursor-pointer"
          onClick={() => appWindow.minimize()}
          title="Minimize"
        >
          <Minus className="h-3 w-3" />
        </button>
        <button
          type="button"
          className="h-6 w-6 flex items-center justify-center rounded-full text-zinc-400 hover:text-zinc-600 hover:bg-zinc-200 dark:text-zinc-500 dark:hover:text-zinc-300 dark:hover:bg-zinc-700 transition-colors cursor-pointer"
          onClick={() => appWindow.toggleMaximize()}
          title="Maximize"
        >
          <Square className="h-2.5 w-2.5" />
        </button>
        <button
          type="button"
          className="h-6 w-6 flex items-center justify-center rounded-full text-zinc-400 hover:text-zinc-600 hover:bg-zinc-200 dark:text-zinc-500 dark:hover:text-zinc-300 dark:hover:bg-zinc-700 transition-colors cursor-pointer"
          onClick={() => appWindow.close()}
          title="Close"
        >
          <X className="h-3 w-3" />
        </button>
      </div>
    </div>
  );
}
