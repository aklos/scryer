import type { Flow } from "./types";

interface FlowEditPopupProps {
  flow: Flow;
  onUpdate: (updates: Partial<Flow>) => void;
  onDelete: () => void;
  onClose: () => void;
}

export function FlowEditPopup({ flow, onUpdate, onDelete, onClose }: FlowEditPopupProps) {
  return (
    <div
      className="absolute inset-0 z-20 flex items-start justify-center bg-black/20 backdrop-blur-[2px]"
      onClick={onClose}
    >
      <div
        className="mt-16 w-80 rounded-lg border border-zinc-200 bg-white shadow-xl dark:border-zinc-700 dark:bg-zinc-800 flex flex-col overflow-hidden"
        onClick={(e) => e.stopPropagation()}
        onKeyDown={(e) => { if (e.key === "Escape") onClose(); }}
      >
        <div className="p-4 flex flex-col gap-3">
          <input
            autoFocus
            className="w-full text-sm font-medium bg-transparent outline-none text-zinc-800 dark:text-zinc-100 placeholder-zinc-400 dark:placeholder-zinc-500 border-b border-zinc-200 dark:border-zinc-700 pb-2"
            value={flow.name}
            placeholder="Flow name..."
            onChange={(e) => onUpdate({ name: e.target.value })}
          />
          <textarea
            className="w-full text-sm bg-transparent outline-none text-zinc-700 dark:text-zinc-300 placeholder-zinc-400 dark:placeholder-zinc-500 resize-none"
            rows={3}
            value={flow.description ?? ""}
            placeholder="What does this flow describe?"
            onChange={(e) => onUpdate({ description: e.target.value || undefined })}
          />
        </div>
        <div className="border-t border-zinc-200 dark:border-zinc-700 px-4 py-2 flex justify-between">
          <button
            type="button"
            className="text-xs text-red-500 hover:text-red-600 dark:text-red-400 dark:hover:text-red-300 cursor-pointer transition-colors"
            onClick={onDelete}
          >
            Delete flow
          </button>
          <button
            type="button"
            className="text-xs text-zinc-500 hover:text-zinc-700 dark:text-zinc-400 dark:hover:text-zinc-200 cursor-pointer transition-colors"
            onClick={onClose}
          >
            Done
          </button>
        </div>
      </div>
    </div>
  );
}
