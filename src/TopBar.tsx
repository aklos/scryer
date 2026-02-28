import { useCallback, useEffect, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Minus, Square, X, Settings, Keyboard, FolderX, SaveAll } from "lucide-react";
import type { C4Kind } from "./types";

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
}

const appWindow = getCurrentWindow();

function LogoIcon({ className }: { className?: string }) {
  return (
    <img src="/logo.png" alt="scryer" className={`${className ?? ""} saturate-[0.7] opacity-90`} />
  );
}

function AppMenu({ onClose, onOpenSettings, onOpenPalette, onCloseModel, onSaveAs, hasModel }: { onClose: () => void; onOpenSettings: () => void; onOpenPalette: () => void; onCloseModel: () => void; onSaveAs: () => void; hasModel: boolean }) {
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const handler = (e: PointerEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        onClose();
      }
    };
    document.addEventListener("pointerdown", handler, true);
    return () => document.removeEventListener("pointerdown", handler, true);
  }, [onClose]);

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [onClose]);

  const items: { label: string; icon: typeof Settings; shortcut?: string; onClick: () => void; disabled?: boolean }[] = [
    { label: "Open model", icon: Keyboard, shortcut: "Ctrl+K", onClick: () => { onOpenPalette(); onClose(); } },
    { label: "Save as…", icon: SaveAll, onClick: () => { onSaveAs(); onClose(); }, disabled: !hasModel },
    { label: "Close model", icon: FolderX, onClick: () => { onCloseModel(); onClose(); }, disabled: !hasModel },
    { label: "AI settings", icon: Settings, onClick: () => { onOpenSettings(); onClose(); } },
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
          <item.icon className="h-3.5 w-3.5 text-zinc-400 dark:text-zinc-500" />
          <span className="flex-1 text-left">{item.label}</span>
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
}: TopBarProps) {
  const isFlow = !!activeFlowId;
  const [menuOpen, setMenuOpen] = useState(false);
  const toggleMenu = useCallback(() => setMenuOpen((prev) => !prev), []);

  return (
    <div
      className="flex items-center h-9 shrink-0 border-b border-zinc-200 bg-zinc-50 dark:border-zinc-700 dark:bg-zinc-900 select-none"
      data-tauri-drag-region
    >
      {/* Logo + app menu */}
      <div className="relative shrink-0 flex items-center">
        <button
          type="button"
          className="h-9 w-10 flex items-center justify-center text-zinc-400 hover:bg-zinc-200 dark:hover:bg-zinc-700/60 cursor-pointer transition-colors"
          onClick={toggleMenu}
          title="Menu"
        >
          <LogoIcon className="h-4 w-4" />
        </button>
        {menuOpen && (
          <AppMenu
            onClose={() => setMenuOpen(false)}
            onOpenSettings={onOpenSettings}
            onOpenPalette={onOpenPalette}
            onCloseModel={onCloseModel}
            onSaveAs={onSaveAs}
            hasModel={hasModel}
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
        <button
          type="button"
          className="rounded px-1.5 py-0.5 text-zinc-400 hover:text-zinc-600 hover:bg-zinc-200 dark:text-zinc-500 dark:hover:text-zinc-300 dark:hover:bg-zinc-700 text-xs shrink-0 cursor-pointer transition-colors"
          onClick={onOpenPalette}
        >
          &#8943;
        </button>
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
              {currentParentKind === "component" ? "Operations"
                : currentParentKind === "container" ? "Components"
                : currentParentKind === "system" ? "Containers"
                : "System context"}
            </span>
            {breadcrumbs.length > 0 && (
              <span className="text-zinc-400 dark:text-zinc-500">
                {breadcrumbs[breadcrumbs.length - 1].name}
              </span>
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
