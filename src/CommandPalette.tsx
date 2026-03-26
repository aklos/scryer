import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { FileBox, FolderOpen, Pencil, Search, Trash2 } from "lucide-react";

interface CommandPaletteProps {
  templateList: string[];
  currentModel: string | null;
  onOpenCodebase: () => void;
  onLoadTemplate: (name: string) => void;
  onDeleteTemplate: (name: string) => void;
  onClose: () => void;
  onRefreshList: () => void;
}

export function CommandPalette({
  templateList,
  currentModel,
  onOpenCodebase,
  onLoadTemplate,
  onDeleteTemplate,
  onClose,
  onRefreshList,
}: CommandPaletteProps) {
  const [filter, setFilter] = useState("");
  const [selected, setSelected] = useState<string | null>(null);
  const [contextMenu, setContextMenu] = useState<{ x: number; y: number; name: string } | null>(null);
  const [renaming, setRenaming] = useState<string | null>(null);
  const [renameValue, setRenameValue] = useState("");
  const filterRef = useRef<HTMLInputElement>(null);
  const renameRef = useRef<HTMLInputElement>(null);
  const gridRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!renaming) filterRef.current?.focus();
  }, [renaming]);

  useEffect(() => {
    if (renaming) renameRef.current?.focus();
  }, [renaming]);

  const lowerFilter = filter.toLowerCase();
  const filtered = lowerFilter
    ? templateList.filter((n) => n.toLowerCase().includes(lowerFilter))
    : templateList;

  useEffect(() => {
    if (selected && !filtered.includes(selected)) setSelected(null);
  }, [filtered, selected]);

  const openSelected = () => {
    if (selected) { onLoadTemplate(selected); onClose(); }
  };

  const handleRename = async (oldName: string, newName: string) => {
    const slug = newName.trim().toLowerCase().replace(/[^a-z0-9_-]/g, "-");
    if (!slug || slug === oldName) { setRenaming(null); return; }
    try {
      await invoke("rename_template", { oldName, newName: slug });
      // If the renamed template was the current model, update
      if (currentModel === oldName) {
        onLoadTemplate(slug);
      }
      onRefreshList();
    } catch (e) {
      console.error("Rename failed:", e);
    }
    setRenaming(null);
  };

  // Close context menu on click outside
  useEffect(() => {
    if (!contextMenu) return;
    const close = () => setContextMenu(null);
    window.addEventListener("click", close);
    return () => window.removeEventListener("click", close);
  }, [contextMenu]);

  // Keyboard navigation
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        if (contextMenu) setContextMenu(null);
        else if (renaming) setRenaming(null);
        else if (selected) setSelected(null);
        else onClose();
        return;
      }

      if (renaming) return;

      if (e.key === "Enter" && selected) {
        e.preventDefault();
        openSelected();
        return;
      }

      if ((e.key === "Delete" || e.key === "Backspace") && selected && !filter) {
        e.preventDefault();
        onDeleteTemplate(selected);
        setSelected(null);
        return;
      }

      if (e.key === "F2" && selected) {
        e.preventDefault();
        setRenaming(selected);
        setRenameValue(selected);
        return;
      }

      if (["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight"].includes(e.key)) {
        e.preventDefault();
        if (filtered.length === 0) return;
        const cols = 5;
        const curIdx = selected ? filtered.indexOf(selected) : -1;
        let nextIdx = curIdx;
        if (e.key === "ArrowRight") nextIdx = Math.min(curIdx + 1, filtered.length - 1);
        else if (e.key === "ArrowLeft") nextIdx = Math.max(curIdx - 1, 0);
        else if (e.key === "ArrowDown") nextIdx = Math.min(curIdx + cols, filtered.length - 1);
        else if (e.key === "ArrowUp") nextIdx = Math.max(curIdx - cols, 0);
        if (curIdx === -1) nextIdx = 0;
        setSelected(filtered[nextIdx]);
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [selected, filtered, filter, onClose, onDeleteTemplate, contextMenu, renaming]);

  return (
    <>
    <div
      className="absolute inset-0 z-20 flex items-start justify-center bg-black/20 backdrop-blur-[2px]"
      onClick={onClose}
    >
      <div
        className="mt-10 w-[560px] max-h-[75vh] rounded-xl border border-[var(--border)] bg-[var(--surface)] shadow-xl flex flex-col overflow-hidden"
        onClick={(e) => { e.stopPropagation(); setSelected(null); setContextMenu(null); }}
      >
        {/* Open codebase — primary action */}
        <button
          type="button"
          className="flex items-center gap-3 px-4 py-3 text-sm font-medium text-[var(--text)] hover:bg-[var(--surface-tint)] cursor-pointer transition-colors border-b border-[var(--border)]"
          onClick={(e) => { e.stopPropagation(); onOpenCodebase(); onClose(); }}
        >
          <FolderOpen size={16} className="text-blue-500" />
          Open codebase…
        </button>

        {/* Template search */}
        <div className="flex items-center gap-2 px-4 py-2.5 border-b border-[var(--border)]">
          <Search size={14} className="text-[var(--text-muted)] shrink-0" />
          <input
            ref={filterRef}
            className="flex-1 text-sm bg-transparent outline-none text-[var(--text)] placeholder-[var(--text-muted)]"
            placeholder="Search templates…"
            value={filter}
            onChange={(e) => { setFilter(e.target.value); setSelected(null); }}
          />
        </div>

        {/* Template grid */}
        <div ref={gridRef} className="flex-1 overflow-y-auto p-4">
          {filtered.length === 0 ? (
            <div className="py-10 text-center text-sm text-[var(--text-muted)]">
              {lowerFilter ? "No templates match" : "No templates"}
            </div>
          ) : (
            <>
              <div className="px-1 pb-2 text-[10px] uppercase tracking-wider font-semibold text-[var(--text-muted)]">Templates</div>
              <div className="grid grid-cols-5 gap-1">
                {filtered.map((name) => {
                  const isCurrent = name === currentModel;
                  const isSelected = name === selected;
                  const isRenaming = name === renaming;
                  return (
                    <div
                      key={name}
                      className={`flex flex-col items-center gap-1.5 rounded-lg px-2 py-3 cursor-pointer transition-colors select-none ${
                        isSelected
                          ? "bg-blue-100 ring-2 ring-blue-400 dark:bg-blue-900/40 dark:ring-blue-500"
                          : isCurrent
                            ? "bg-blue-50 dark:bg-blue-900/20"
                            : "hover:bg-[var(--surface-hover)]"
                      }`}
                      onClick={(e) => { e.stopPropagation(); setSelected(name); setContextMenu(null); }}
                      onDoubleClick={() => { onLoadTemplate(name); onClose(); }}
                      onContextMenu={(e) => { e.preventDefault(); e.stopPropagation(); setContextMenu({ x: e.clientX, y: e.clientY, name }); setSelected(name); }}
                    >
                      <FileBox
                        size={40}
                        strokeWidth={1.2}
                        className={isSelected || isCurrent
                          ? "text-blue-500 dark:text-blue-400"
                          : "text-blue-400 dark:text-blue-500"
                        }
                      />
                      {isRenaming ? (
                        <input
                          ref={renameRef}
                          className="text-[11px] text-center w-full bg-[var(--surface)] border border-blue-400 rounded px-1 py-0.5 outline-none text-[var(--text)]"
                          value={renameValue}
                          onChange={(e) => setRenameValue(e.target.value)}
                          onBlur={() => handleRename(name, renameValue)}
                          onKeyDown={(e) => {
                            e.stopPropagation();
                            if (e.key === "Enter") handleRename(name, renameValue);
                            if (e.key === "Escape") setRenaming(null);
                          }}
                          onClick={(e) => e.stopPropagation()}
                        />
                      ) : (
                        <span className={`text-[11px] leading-tight text-center break-all max-w-full ${
                          isSelected
                            ? "text-blue-700 font-medium dark:text-blue-200"
                            : isCurrent
                              ? "text-blue-600 font-medium dark:text-blue-300"
                              : "text-[var(--text-secondary)]"
                        }`}>
                          {name}
                        </span>
                      )}
                    </div>
                  );
                })}
              </div>
            </>
          )}
        </div>

        {/* Status bar */}
        <div className="flex items-center justify-between px-4 py-1.5 border-t border-[var(--border)] bg-[var(--surface-raised)] text-[11px] text-[var(--text-muted)]">
          <span>{filtered.length} template{filtered.length !== 1 ? "s" : ""}</span>
          <span>
            {selected
              ? "Enter to open · F2 to rename · Delete to remove"
              : "Click to select · Double-click to open · Right-click for options"
            }
          </span>
        </div>
      </div>

    </div>

    {/* Context menu — rendered outside backdrop to avoid stacking context issues */}
    {contextMenu && (
      <div
        className="fixed z-50 min-w-[140px] rounded-lg border border-[var(--border-overlay)] bg-[var(--surface-overlay)] shadow-lg backdrop-blur-sm py-1"
        style={{ left: contextMenu.x, top: contextMenu.y }}
        onClick={(e) => e.stopPropagation()}
      >
        <button
          type="button"
          className="flex items-center gap-2 w-full px-3 py-1.5 text-xs text-[var(--text-secondary)] hover:bg-[var(--surface-tint)] cursor-pointer transition-colors"
          onClick={() => { onLoadTemplate(contextMenu.name); onClose(); }}
        >
          Open
        </button>
        <button
          type="button"
          className="flex items-center gap-2 w-full px-3 py-1.5 text-xs text-[var(--text-secondary)] hover:bg-[var(--surface-tint)] cursor-pointer transition-colors"
          onClick={() => { setRenaming(contextMenu.name); setRenameValue(contextMenu.name); setContextMenu(null); }}
        >
          <Pencil size={12} /> Rename
        </button>
        <button
          type="button"
          className="flex items-center gap-2 w-full px-3 py-1.5 text-xs text-red-600 hover:bg-red-50 dark:text-red-400 dark:hover:bg-red-900/30 cursor-pointer transition-colors"
          onClick={() => { onDeleteTemplate(contextMenu.name); setContextMenu(null); setSelected(null); }}
        >
          <Trash2 size={12} /> Delete
        </button>
      </div>
    )}
    </>
  );
}
