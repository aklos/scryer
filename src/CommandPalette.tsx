import { useEffect, useRef, useState } from "react";
import { FileBox, Plus, Search, Trash2 } from "lucide-react";

interface CommandPaletteProps {
  modelList: string[];
  currentModel: string | null;
  onNewModel: () => void;
  onLoadModel: (name: string) => void;
  onSaveAs: (name: string) => void;
  onDeleteModel: (name: string) => void;
  onClose: () => void;
}

export function CommandPalette({
  modelList,
  currentModel,
  onNewModel,
  onLoadModel,
  onSaveAs,
  onDeleteModel,
  onClose,
}: CommandPaletteProps) {
  const [filter, setFilter] = useState("");
  const [selected, setSelected] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [saveName, setSaveName] = useState("");
  const filterRef = useRef<HTMLInputElement>(null);
  const saveRef = useRef<HTMLInputElement>(null);
  const gridRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (saving) saveRef.current?.focus();
    else filterRef.current?.focus();
  }, [saving]);

  const lowerFilter = filter.toLowerCase();
  const filtered = lowerFilter
    ? modelList.filter((n) => n.toLowerCase().includes(lowerFilter))
    : modelList;

  // Clear selection when filter changes and selected is no longer visible
  useEffect(() => {
    if (selected && !filtered.includes(selected)) setSelected(null);
  }, [filtered, selected]);

  const handleSave = () => {
    const name = saveName.trim().toLowerCase().replace(/[^a-z0-9_-]/g, "-");
    if (name) { onSaveAs(name); onClose(); }
  };

  const openSelected = () => {
    if (selected) { onLoadModel(selected); onClose(); }
  };

  // Keyboard navigation
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        if (saving) { setSaving(false); setSaveName(""); }
        else if (selected) setSelected(null);
        else onClose();
        return;
      }

      // Don't handle arrow/enter/delete when typing in save input
      if (saving) return;

      if (e.key === "Enter" && selected) {
        e.preventDefault();
        openSelected();
        return;
      }

      if ((e.key === "Delete" || e.key === "Backspace") && selected && !filter) {
        e.preventDefault();
        onDeleteModel(selected);
        setSelected(null);
        return;
      }

      // Arrow key grid navigation
      if (["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight"].includes(e.key)) {
        e.preventDefault();
        if (filtered.length === 0) return;

        // Compute column count from grid
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
  }, [saving, selected, filtered, filter, onClose, onDeleteModel]);

  return (
    <div
      className="absolute inset-0 z-20 flex items-start justify-center bg-black/20 backdrop-blur-[2px]"
      onClick={onClose}
    >
      <div
        className="mt-10 w-[560px] max-h-[75vh] rounded-xl border border-zinc-200 bg-zinc-50 shadow-xl dark:border-zinc-700 dark:bg-zinc-900 flex flex-col overflow-hidden"
        onClick={(e) => { e.stopPropagation(); setSelected(null); }}
      >
        {/* Toolbar */}
        <div className="flex items-center gap-2 px-4 py-2.5 border-b border-zinc-200 dark:border-zinc-700 bg-white dark:bg-zinc-800">
          <Search size={14} className="text-zinc-400 dark:text-zinc-500 shrink-0" />
          <input
            ref={filterRef}
            className="flex-1 text-sm bg-transparent outline-none text-zinc-800 dark:text-zinc-100 placeholder-zinc-400 dark:placeholder-zinc-500"
            placeholder="Search models…"
            value={filter}
            onChange={(e) => { setFilter(e.target.value); setSelected(null); }}
          />
          {selected && (
            <button
              type="button"
              className="flex items-center gap-1 rounded-md px-2.5 py-1 text-xs font-medium text-red-500 hover:text-red-600 hover:bg-red-50 dark:text-red-400 dark:hover:text-red-300 dark:hover:bg-red-900/20 cursor-pointer transition-colors"
              onClick={(e) => { e.stopPropagation(); onDeleteModel(selected); setSelected(null); }}
              title="Delete selected model"
            >
              <Trash2 size={13} />
              Delete
            </button>
          )}
          <button
            type="button"
            className="flex items-center gap-1 rounded-md px-2.5 py-1 text-xs font-medium text-zinc-500 hover:text-zinc-700 hover:bg-zinc-100 dark:text-zinc-400 dark:hover:text-zinc-200 dark:hover:bg-zinc-700 cursor-pointer transition-colors"
            onClick={(e) => { e.stopPropagation(); setSaving(true); setSaveName(""); }}
            title="Save current model as…"
          >
            Save as…
          </button>
          <button
            type="button"
            className="flex items-center gap-1 rounded-md px-2.5 py-1 text-xs font-medium text-zinc-500 hover:text-zinc-700 hover:bg-zinc-100 dark:text-zinc-400 dark:hover:text-zinc-200 dark:hover:bg-zinc-700 cursor-pointer transition-colors"
            onClick={(e) => { e.stopPropagation(); onNewModel(); onClose(); }}
            title="New empty model"
          >
            <Plus size={14} />
            New
          </button>
        </div>

        {/* Save-as inline */}
        {saving && (
          <form
            className="flex items-center gap-2 px-4 py-2 border-b border-zinc-200 dark:border-zinc-700 bg-white dark:bg-zinc-800"
            onClick={(e) => e.stopPropagation()}
            onSubmit={(e) => { e.preventDefault(); handleSave(); }}
          >
            <input
              ref={saveRef}
              className="flex-1 min-w-0 rounded-md border border-zinc-300 bg-zinc-50 px-2.5 py-1.5 text-sm outline-none focus:border-blue-500 dark:border-zinc-600 dark:bg-zinc-700 dark:text-zinc-100 dark:focus:border-blue-400"
              placeholder="model-name"
              value={saveName}
              onChange={(e) => setSaveName(e.target.value)}
            />
            <button type="submit" className="rounded-md bg-blue-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-blue-700 cursor-pointer shrink-0">
              Save
            </button>
            <button
              type="button"
              className="rounded-md px-2 py-1.5 text-xs text-zinc-400 hover:text-zinc-600 dark:hover:text-zinc-300 cursor-pointer"
              onClick={() => { setSaving(false); setSaveName(""); }}
            >
              Cancel
            </button>
          </form>
        )}

        {/* Icon grid */}
        <div ref={gridRef} className="flex-1 overflow-y-auto p-4">
          {filtered.length === 0 ? (
            <div className="py-10 text-center text-sm text-zinc-400 dark:text-zinc-500">
              {lowerFilter ? "No models match" : "No saved models"}
            </div>
          ) : (
            <div className="grid grid-cols-5 gap-1">
              {filtered.map((name) => {
                const isCurrent = name === currentModel;
                const isSelected = name === selected;
                return (
                  <div
                    key={name}
                    className={`flex flex-col items-center gap-1.5 rounded-lg px-2 py-3 cursor-pointer transition-colors select-none ${
                      isSelected
                        ? "bg-blue-100 ring-2 ring-blue-400 dark:bg-blue-900/40 dark:ring-blue-500"
                        : isCurrent
                          ? "bg-blue-50 dark:bg-blue-900/20"
                          : "hover:bg-zinc-200/60 dark:hover:bg-zinc-700/50"
                    }`}
                    onClick={(e) => { e.stopPropagation(); setSelected(name); }}
                    onDoubleClick={() => { onLoadModel(name); onClose(); }}
                  >
                    <FileBox
                      size={40}
                      strokeWidth={1.2}
                      className={isSelected || isCurrent
                        ? "text-blue-500 dark:text-blue-400"
                        : "text-blue-400 dark:text-blue-500"
                      }
                    />
                    <span className={`text-[11px] leading-tight text-center break-all max-w-full ${
                      isSelected
                        ? "text-blue-700 font-medium dark:text-blue-200"
                        : isCurrent
                          ? "text-blue-600 font-medium dark:text-blue-300"
                          : "text-zinc-600 dark:text-zinc-400"
                    }`}>
                      {name}
                    </span>
                  </div>
                );
              })}
            </div>
          )}
        </div>

        {/* Status bar */}
        <div className="flex items-center justify-between px-4 py-1.5 border-t border-zinc-200 dark:border-zinc-700 bg-white dark:bg-zinc-800 text-[11px] text-zinc-400 dark:text-zinc-500">
          <span>{filtered.length} model{filtered.length !== 1 ? "s" : ""}</span>
          <span>
            {selected
              ? "Enter to open · Delete to remove"
              : "Click to select · Double-click to open"
            }
          </span>
        </div>
      </div>
    </div>
  );
}
