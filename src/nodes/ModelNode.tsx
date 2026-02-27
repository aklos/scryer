import { useEffect, useState } from "react";
import type { NodeProps, Node } from "@xyflow/react";
import { useReactFlow } from "@xyflow/react";
import type { Status, ModelProperty, Hint } from "../types";
import { CenterHandle } from "./NodeHandles";
import { HintBadge } from "./HintBadge";
import { STATUS_COLORS } from "../statusColors";
import { DescriptionText } from "../DescriptionText";
import { Table, Pencil } from "lucide-react";

type ModelNodeData = {
  name: string;
  description: string;
  properties?: ModelProperty[];
  status?: Status;
  [key: string]: unknown;
};

type ModelNodeType = Node<ModelNodeData>;

function sanitizeIdentifier(raw: string): string {
  const stripped = raw.replace(/[^a-zA-Z0-9_]/g, "");
  if (stripped.length === 0) return "";
  const first = stripped[0];
  if (/[a-zA-Z]/.test(first)) return first.toLowerCase() + stripped.slice(1);
  return stripped.slice(1);
}

export function ModelNode({ id, data, selected }: NodeProps<ModelNodeType>) {
  const { updateNodeData } = useReactFlow();
  const properties = data.properties ?? [];
  const nodeHints = (data._hints as Hint[] | undefined) ?? [];
  const statusColor = data.status ? STATUS_COLORS[data.status] : null;
  const [editing, setEditing] = useState(false);

  useEffect(() => { if (!selected) setEditing(false); }, [selected]);

  const updateProperty = (index: number, updates: Partial<ModelProperty>) => {
    const next = properties.map((p, i) => (i === index ? { ...p, ...updates } : p));
    updateNodeData(id, { properties: next });
  };

  const addProperty = () => {
    const next = [...properties, { label: "", description: "" }];
    updateNodeData(id, { properties: next });
  };

  const removeProperty = (index: number) => {
    const next = properties.filter((_, i) => i !== index);
    updateNodeData(id, { properties: next });
  };

  return (
    <div
      className={`relative rounded-lg border min-w-[260px] max-w-[360px] ${
        selected
          ? "border-zinc-900 dark:border-zinc-300"
          : statusColor
            ? ""
            : "border-zinc-300 dark:border-zinc-600"
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
        <Table size={10} />
        <span>Model</span>
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
      <div className={`px-3 py-2 pt-6 ${editing || data.description || properties.length > 0 ? "border-b border-zinc-100 dark:border-zinc-800" : ""}`}>
        <span className="text-sm font-semibold text-zinc-800 dark:text-zinc-100 truncate">
          {data.name || "Untitled model"}
        </span>
      </div>

      {/* Description */}
      {editing ? (
        <div className="px-3 py-2">
          <textarea
            className="nodrag w-full rounded bg-zinc-100 dark:bg-zinc-800 px-1.5 py-1 -mx-1.5 -my-1 outline-none text-[11px] leading-snug text-zinc-500 dark:text-zinc-400 resize-none placeholder:text-zinc-300 dark:placeholder:text-zinc-600 placeholder:italic"
            value={data.description}
            placeholder="Description..."
            rows={3}
            onChange={(e) => updateNodeData(id, { description: e.target.value })}
          />
        </div>
      ) : data.description ? (
        <div className="px-3 py-2 text-[11px] leading-snug text-zinc-500 dark:text-zinc-400 text-left">
          <DescriptionText text={data.description} onMentionClick={(name) => window.dispatchEvent(new CustomEvent("mention-click", { detail: { name } }))} />
        </div>
      ) : null}

      {/* Properties */}
      {editing ? (
        <div className="px-3 py-1.5 flex flex-col gap-0.5">
          {properties.map((prop, i) => (
            <div key={i} className="group flex items-center gap-1 text-[10px] leading-snug">
              <input
                className="nodrag w-20 flex-shrink-0 font-semibold font-mono bg-transparent outline-none text-zinc-600 dark:text-zinc-300 placeholder:text-zinc-400 dark:placeholder:text-zinc-600 placeholder:italic placeholder:font-normal"
                value={prop.label}
                placeholder="name"
                onChange={(e) => updateProperty(i, { label: sanitizeIdentifier(e.target.value) })}
              />
              <span className="text-zinc-300 dark:text-zinc-600">—</span>
              <input
                className="nodrag flex-1 min-w-0 bg-transparent outline-none text-zinc-400 dark:text-zinc-500 placeholder:text-zinc-300 dark:placeholder:text-zinc-600 placeholder:italic"
                value={prop.description}
                placeholder="description"
                onChange={(e) => updateProperty(i, { description: e.target.value })}
              />
              <button
                type="button"
                className="nodrag shrink-0 text-xs text-zinc-300 hover:text-red-400 dark:text-zinc-600 dark:hover:text-red-400 cursor-pointer opacity-0 group-hover:opacity-100"
                onClick={() => removeProperty(i)}
              >
                &times;
              </button>
            </div>
          ))}
          <button
            type="button"
            className="nodrag self-start text-[10px] text-zinc-400 hover:text-zinc-600 dark:text-zinc-500 dark:hover:text-zinc-300 cursor-pointer"
            onClick={addProperty}
          >
            + add property
          </button>
        </div>
      ) : properties.length > 0 ? (
        <div className="px-3 py-2 flex flex-col gap-0.5">
          {properties.map((prop, i) => (
            <div key={i} className="text-[10px] leading-snug text-zinc-600 dark:text-zinc-300">
              <span className="font-semibold font-mono">{prop.label || <span className="font-normal italic text-zinc-400 dark:text-zinc-600">name</span>}</span>
              <span className="text-zinc-400 dark:text-zinc-500"> — {prop.description || <span className="italic text-zinc-400 dark:text-zinc-600">description</span>}</span>
            </div>
          ))}
        </div>
      ) : null}
    </div>
  );
}
