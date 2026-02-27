import { useCallback, useEffect, useRef, useState } from "react";
import type { NodeProps } from "@xyflow/react";
import { NodeHandles } from "./NodeHandles";
import { Pencil, Workflow } from "lucide-react";
import { MentionTextarea, type MentionItem } from "../MentionTextarea";
import { DescriptionText, type MentionNodeInfo } from "../DescriptionText";
import { STATUS_COLORS } from "../statusColors";
import type { LinkedProcess } from "../FlowCanvas";

const GRID = 20;

export function FlowStepNode({ id, data, selected }: NodeProps) {
  const description = (data as { description?: string }).description;
  const stepLabel = (data as { stepLabel?: string }).stepLabel;
  const linkedProcesses = (data as { linkedProcesses?: LinkedProcess[] }).linkedProcesses;
  const mentionNames = ((data as { _mentionNames?: MentionItem[] })._mentionNames ?? []);
  const nodeMap = (data as { _nodeMap?: Map<string, MentionNodeInfo> })._nodeMap;
  const innerRef = useRef<HTMLDivElement>(null);
  const slotRef = useRef<HTMLDivElement>(null);
  const [editing, setEditing] = useState(false);

  // Exit edit mode when deselected
  useEffect(() => { if (!selected) setEditing(false); }, [selected]);

  const snapHeight = useCallback(() => {
    const el = innerRef.current;
    const slot = slotRef.current;
    if (!el) return;
    // Hide slot so scrollHeight reflects only the card content
    if (slot) slot.style.display = "none";
    el.style.minHeight = "auto";
    const natural = el.scrollHeight;
    const snapped = Math.max(GRID * 3, Math.round(natural / GRID) * GRID);
    el.style.minHeight = `${snapped}px`;
    if (slot) slot.style.display = "";
  }, []);

  useEffect(() => { snapHeight(); }, [description, editing, linkedProcesses, snapHeight]);

  return (
    <div
      ref={innerRef}
      className={`relative flex items-start gap-2.5 rounded-lg border-2 bg-white px-3 py-2.5 dark:bg-zinc-900 ${
        selected
          ? "border-zinc-900 shadow-md dark:border-zinc-300"
          : "border-zinc-300 dark:border-zinc-600"
      }`}
      style={{ width: 220 }}
    >
      {stepLabel && (
        <span className="shrink-0 flex items-center justify-center min-w-5 h-5 rounded-full bg-zinc-200 px-1.5 text-[10px] font-semibold text-zinc-600 dark:bg-zinc-700 dark:text-zinc-300 mt-px">
          {stepLabel}
        </span>
      )}

      {/* Edit toggle */}
      {selected && (
        <button
          type="button"
          className={`nodrag absolute top-1.5 right-1.5 z-10 p-0.5 rounded cursor-pointer transition-colors ${
            editing
              ? "text-zinc-700 dark:text-zinc-300"
              : "text-zinc-300 hover:text-zinc-500 dark:text-zinc-600 dark:hover:text-zinc-400"
          }`}
          onClick={() => setEditing(!editing)}
        >
          <Pencil size={10} />
        </button>
      )}

      {editing ? (
        <MentionTextarea
          value={description ?? ""}
          mentionNames={mentionNames}
          placeholder="e.g. System validates credentials"
          rows={1}
          autoSize
          className="nodrag min-w-0 w-full rounded bg-zinc-100 dark:bg-zinc-800 px-1.5 py-1 -mx-1.5 -my-1 outline-none text-xs leading-snug text-zinc-600 dark:text-zinc-300 resize-none overflow-hidden placeholder:text-zinc-400 dark:placeholder:text-zinc-500 placeholder:italic"
          onChange={(val) => {
            window.dispatchEvent(new CustomEvent("update-step-description", {
              detail: { stepId: id, description: val || undefined },
            }));
            requestAnimationFrame(snapHeight);
          }}
        />
      ) : (
        <div className={`flex-1 min-w-0 text-xs leading-snug break-words ${
          description
            ? "text-zinc-600 dark:text-zinc-300"
            : "text-zinc-400 dark:text-zinc-500 italic"
        }`}>
          {description ? <DescriptionText text={description} onMentionClick={(name) => window.dispatchEvent(new CustomEvent("mention-click", { detail: { name } }))} nodeMap={nodeMap} /> : "Empty step"}
        </div>
      )}
      {linkedProcesses && linkedProcesses.length > 0 && (
        <div ref={slotRef} className="absolute -bottom-0.5 left-3 right-3 flex flex-col gap-1 translate-y-full pt-1">
          {linkedProcesses.map((p) => {
            const sc = p.status ? STATUS_COLORS[p.status] : null;
            return (
              <div
                key={p.id}
                className={`flex items-center gap-1 rounded px-1.5 py-0.5 text-[9px] font-mono leading-tight border bg-zinc-100 text-zinc-600 dark:bg-zinc-800 dark:text-zinc-300 ${!sc ? "border-zinc-200 dark:border-zinc-700" : ""}`}
                style={sc ? { borderColor: sc.hex + "99" } : undefined}
              >
                <Workflow size={10} className="shrink-0 opacity-50" />
                <span className="truncate">{p.name || "Untitled"}</span>
              </div>
            );
          })}
        </div>
      )}
      <NodeHandles />
    </div>
  );
}
