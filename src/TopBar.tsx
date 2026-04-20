import { useCallback, useEffect, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { Minus, Square, X, Settings, Keyboard, FolderX, Menu, FolderOpen, Save } from "lucide-react";
import type { C4Kind, AiToolsState } from "./types";

interface TopBarProps {
  currentModel: string | null;
  onOpenPalette: () => void;
  onNavigateToRoot: () => void;
  onOpenSettings: () => void;
  onCloseModel: () => void;
  onSaveAs: (name: string) => void;
  hasModel: boolean;

  breadcrumbs: { id: string; name: string; kind: C4Kind }[];
  currentParentKind: C4Kind | undefined;
  navigateToBreadcrumb: (targetId: string | null) => void;
  activeFlowId: string | null;
  activeFlowName: string | null;

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
        checked ? "bg-blue-500" : "bg-[var(--border-strong)]"
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
}: {
  onClose: () => void;
  projectPath?: string;
  aiTools: AiToolsState;
  onAiToolsChange: (tools: AiToolsState) => void;
  triggerRef: React.RefObject<HTMLButtonElement | null>;
  onSetProjectPath: (path: string | undefined) => void;
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
      className="absolute top-full left-0 mt-1 z-50 min-w-[240px] rounded-lg border border-[var(--border-overlay)] bg-[var(--surface-overlay)] shadow-sm backdrop-blur-sm py-1"
    >
      <button
        type="button"
        className="flex items-center gap-2 w-full px-3 py-1.5 text-xs text-[var(--text-secondary)] hover:bg-[var(--surface-tint)] cursor-pointer transition-colors"
        onClick={async () => {
          const selected = await openDialog({ directory: true, title: "Select project folder", defaultPath: projectPath });
          if (selected) onSetProjectPath(selected);
        }}
      >
        <FolderOpen className="h-3.5 w-3.5 text-[var(--text-muted)]" />
        <span className="flex-1 text-left truncate">{projectPath ? "Change codebase" : "Link codebase"}</span>
      </button>
      {projectPath && (
        <div className="px-3 py-1 text-[10px] text-[var(--text-muted)] truncate max-w-[240px]" title={projectPath}>
          {projectPath}
        </div>
      )}

      {(showClaude || showCodex) && (
        <>
          <div className="my-1 border-t border-[var(--border-subtle)]" />
          <div className="px-3 py-1.5 text-[10px] font-medium uppercase tracking-wider text-[var(--text-muted)]">
            MCP Server
          </div>
          {showClaude && (
            <>
              <div className="flex items-center justify-between px-3 py-1.5">
                <div>
                  <div className="text-xs text-[var(--text-secondary)]">Claude Code</div>
                  <div className="text-[10px] text-[var(--text-muted)] max-w-[160px]">.mcp.json</div>
                </div>
                <Toggle
                  checked={aiTools.claudeMcpEnabled}
                  onChange={(checked) => { if (checked) handleToggle("claudeMcpEnabled", checked); }}
                />
              </div>
              {aiTools.claudeMcpEnabled && (
                <div className="flex items-center justify-between px-3 py-1.5 pl-6">
                  <div>
                    <div className="text-xs text-[var(--text-secondary)]">Auto-approve reads</div>
                    <div className="text-[10px] text-[var(--text-muted)] max-w-[140px]">settings.local.json</div>
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
                <div className="text-xs text-[var(--text-secondary)]">Codex</div>
                <div className="text-[10px] text-[var(--text-muted)] max-w-[160px]">.codex/config.toml</div>
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

function AppMenu({ onClose, onOpenSettings, onOpenPalette, onCloseModel, onSaveAs, hasModel, canSaveAs, triggerRef }: { onClose: () => void; onOpenSettings: () => void; onOpenPalette: () => void; onCloseModel: () => void; onSaveAs: (name: string) => void; hasModel: boolean; canSaveAs: boolean; triggerRef: React.RefObject<HTMLButtonElement | null> }) {
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
    { label: "Save as\u2026", icon: Save, onClick: () => {
      const name = window.prompt("Model name:");
      if (!name) return;
      const sanitized = name.trim().toLowerCase().replace(/[^a-z0-9_-]/g, "-").replace(/-+/g, "-").replace(/^-|-$/g, "");
      if (!sanitized) return;
      onSaveAs(sanitized);
      onClose();
    }, disabled: !canSaveAs },
    { label: "Close model", icon: FolderX, onClick: () => { onCloseModel(); onClose(); }, disabled: !hasModel },
    { label: "Settings", icon: Settings, onClick: () => { onOpenSettings(); onClose(); } },
  ];

  return (
    <div
      ref={menuRef}
      className="absolute top-full left-0 mt-1 z-50 min-w-[180px] rounded-lg border border-[var(--border-overlay)] bg-[var(--surface-overlay)] shadow-sm backdrop-blur-sm py-1"
    >
      {items.map((item) => (
        <button
          key={item.label}
          type="button"
          className={`flex items-center gap-2 w-full px-3 py-1.5 text-xs transition-colors ${item.disabled ? "text-[var(--text-ghost)] cursor-default" : "text-[var(--text-secondary)] hover:bg-[var(--surface-tint)] cursor-pointer"}`}
          onClick={item.disabled ? undefined : item.onClick}
        >
          <item.icon className={`h-3.5 w-3.5 ${item.active ? "text-blue-500 dark:text-blue-400" : "text-[var(--text-muted)]"}`} />
          <span className="flex-1 text-left">{item.label}</span>
          {item.active !== undefined && (
            <span className={`text-[10px] ${item.active ? "text-blue-500 dark:text-blue-400" : "text-[var(--text-muted)]"}`}>
              {item.active ? "on" : "off"}
            </span>
          )}
          {item.shortcut && (
            <span className="text-[10px] text-[var(--text-muted)]">{item.shortcut}</span>
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
      className="flex items-center h-9 shrink-0 border-b border-[var(--border)] bg-[var(--surface)] select-none"
      data-tauri-drag-region
    >
      {/* Logo + app menu */}
      <div className="relative shrink-0 flex items-center">
        <button
          ref={appMenuTriggerRef}
          type="button"
          className="h-9 w-10 flex items-center justify-center text-[var(--text-muted)] hover:bg-[var(--surface-hover)] cursor-pointer transition-colors"
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
            onSaveAs={onSaveAs}
            hasModel={hasModel}
            canSaveAs={hasModel}
            triggerRef={appMenuTriggerRef}
          />
        )}
      </div>

      {/* Left: model name */}
      <div
        className="w-50 shrink-0 flex items-center gap-1 px-2 border-r border-[var(--border)] h-full"
        data-tauri-drag-region
      >
        <span
          className="truncate flex-1 text-xs font-semibold text-[var(--text-secondary)] cursor-pointer"
          onClick={onNavigateToRoot}
        >
          {currentModel?.startsWith("project:") ? currentModel.replace(/^project:.*[/\\]/, "") : currentModel ?? "Untitled"}
        </span>
        {hasModel && (
          <div className="relative shrink-0">
            <button
              ref={projectMenuTriggerRef}
              type="button"
              className="rounded px-1.5 py-0.5 text-[var(--text-muted)] hover:text-[var(--text-secondary)] hover:bg-[var(--surface-hover)] text-xs shrink-0 cursor-pointer transition-colors"
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
                triggerRef={projectMenuTriggerRef}
              />
            )}
          </div>
        )}
      </div>

      {/* Center: level indicator + toolbar */}
      <div className="flex-1 flex items-center gap-2 px-3 h-full min-w-0" data-tauri-drag-region>
        {hasModel && !isFlow && (
          <div className="flex items-baseline gap-1.5 text-[11px] shrink-0">
            {breadcrumbs.length > 0 && (
              <button
                type="button"
                className="text-[var(--text-muted)] hover:text-[var(--text)] cursor-pointer transition-colors"
                onClick={() => {
                  const parent = breadcrumbs.length >= 2 ? breadcrumbs[breadcrumbs.length - 2].id : null;
                  navigateToBreadcrumb(parent);
                }}
                title="Go up"
              >
                &larr;
              </button>
            )}
            {breadcrumbs.length > 0 && (
              <span className="text-[var(--text-secondary)] font-medium">
                {breadcrumbs[breadcrumbs.length - 1].name}
              </span>
            )}
            <span className="text-[10px] text-[var(--text-muted)]">
              {currentParentKind === "component" ? "Code"
                : currentParentKind === "container" ? "Components"
                : currentParentKind === "system" ? "Containers"
                : "System context"}
            </span>
          </div>
        )}
        {hasModel && isFlow && activeFlowName && (
          <span className="text-[11px] text-[var(--text-muted)] shrink-0">
            Flow: {activeFlowName}
          </span>
        )}
      </div>

      {/* Right: window controls */}
      <div className="flex items-center gap-1.5 shrink-0 pr-2.5">
        <button
          type="button"
          className="h-6 w-6 flex items-center justify-center rounded-full text-[var(--text-muted)] hover:text-[var(--text-secondary)] hover:bg-[var(--surface-hover)] transition-colors cursor-pointer"
          onClick={() => appWindow.minimize()}
          title="Minimize"
        >
          <Minus className="h-3 w-3" />
        </button>
        <button
          type="button"
          className="h-6 w-6 flex items-center justify-center rounded-full text-[var(--text-muted)] hover:text-[var(--text-secondary)] hover:bg-[var(--surface-hover)] transition-colors cursor-pointer"
          onClick={() => appWindow.toggleMaximize()}
          title="Maximize"
        >
          <Square className="h-2.5 w-2.5" />
        </button>
        <button
          type="button"
          className="h-6 w-6 flex items-center justify-center rounded-full text-[var(--text-muted)] hover:text-[var(--text-secondary)] hover:bg-[var(--surface-hover)] transition-colors cursor-pointer"
          onClick={() => appWindow.close()}
          title="Close"
        >
          <X className="h-3 w-3" />
        </button>
      </div>
    </div>
  );
}
