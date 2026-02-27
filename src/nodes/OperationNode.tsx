import { useEffect, useState } from "react";
import type { NodeProps } from "@xyflow/react";
import { useReactFlow } from "@xyflow/react";
import type { C4Node as C4NodeType, Hint } from "../types";
import { CenterHandle } from "./NodeHandles";
import { HintBadge } from "./HintBadge";
import { STATUS_COLORS } from "../statusColors";
import { DescriptionText } from "../DescriptionText";
import { MentionTextarea, type MentionItem } from "../MentionTextarea";
import { Code, Pencil } from "lucide-react";

export function OperationNode({ id, data, selected }: NodeProps<C4NodeType>) {
  const { updateNodeData } = useReactFlow();
  const nodeHints = (data._hints as Hint[] | undefined) ?? [];
  const mentionNames = (data._mentionNames as MentionItem[] | undefined) ?? [];
  const statusColor = data.status ? STATUS_COLORS[data.status] : null;
  const [editing, setEditing] = useState(false);

  // Exit edit mode when deselected
  useEffect(() => { if (!selected) setEditing(false); }, [selected]);

  return (
    <div
      className={`relative rounded-lg border min-w-[260px] max-w-[360px] ${
        selected
          ? "border-zinc-900 dark:border-zinc-300"
          : statusColor
            ? ""
            : "border-zinc-200 dark:border-zinc-700"
      } bg-white dark:bg-zinc-900 shadow-sm`}
      style={!selected && statusColor ? { borderColor: statusColor.hex + "99", borderWidth: 2 } : selected ? { borderWidth: 2 } : undefined}
    >
      <CenterHandle />
      <HintBadge nodeId={id} hints={nodeHints} />

      {/* Kind badge */}
      <div
        className="absolute top-1.5 left-1.5 z-10 flex items-center gap-0.5 px-1 rounded border text-[8px] leading-none font-medium
          bg-white/70 dark:bg-zinc-800/70 border-zinc-200/60 dark:border-zinc-700/60
          text-zinc-400 dark:text-zinc-500"
        style={{ height: 16 }}
      >
        <Code size={10} />
        <span>Operation</span>
      </div>

      {/* Edit toggle */}
      {selected && (
        <button
          type="button"
          className={`nodrag absolute top-1.5 right-1.5 z-10 p-0.5 rounded cursor-pointer transition-colors ${
            editing
              ? "text-blue-500 dark:text-blue-400"
              : "text-zinc-300 hover:text-zinc-500 dark:text-zinc-600 dark:hover:text-zinc-400"
          }`}
          onClick={() => setEditing(!editing)}
        >
          <Pencil size={10} />
        </button>
      )}

      {/* Header */}
      <div className={`px-3 py-2 pt-6 ${editing || data.description ? "border-b border-zinc-100 dark:border-zinc-800" : ""}`}>
        <span className="text-sm font-semibold font-mono text-zinc-800 dark:text-zinc-100 truncate">
          {data.name || "Untitled operation"}
        </span>
      </div>

      {/* Description */}
      {editing ? (
        <div className="px-3 py-2">
          <MentionTextarea
            className="nodrag w-full rounded bg-zinc-100 dark:bg-zinc-800 px-1.5 py-1 -mx-1.5 -my-1 outline-none text-[11px] leading-snug text-zinc-500 dark:text-zinc-400 resize-none placeholder:text-zinc-300 dark:placeholder:text-zinc-600 placeholder:italic"
            value={data.description}
            placeholder="Use @[Name] to reference members..."
            rows={3}
            mentionNames={mentionNames}
            onChange={(val) => updateNodeData(id, { description: val })}
          />
        </div>
      ) : data.description ? (
        <div className="px-3 py-2 text-[11px] leading-snug text-zinc-500 dark:text-zinc-400 text-left">
          <DescriptionText text={data.description} onMentionClick={(name) => window.dispatchEvent(new CustomEvent("mention-click", { detail: { name } }))} />
        </div>
      ) : null}
    </div>
  );
}
