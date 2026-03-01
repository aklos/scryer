import { useRef, useState, type DragEvent } from "react";
import type { NodeProps } from "@xyflow/react";
import type {
  C4Node as C4NodeType,
  C4NodeData,
  C4Kind,
  Hint,
  Attachment,
} from "../types";
import { NodeHandles, CenterHandle } from "./NodeHandles";
import { ShapeBackground, resolveShape, getContentInsets } from "../shapes";
import { HintBadge } from "./HintBadge";
import { STATUS_COLORS } from "../statusColors";
import { KIND_ICON } from "../kindIcons";
import { DescriptionText } from "../DescriptionText";
import { Code, Workflow, Table } from "lucide-react";

/** Whether this kind can be drilled into */
function isExpandable(kind: C4NodeData["kind"]): boolean {
  return kind === "system" || kind === "container" || kind === "component";
}

/** Compact kind badge for the top-left corner of a node */
function KindTab({ kind, dimmed }: { kind: C4Kind; dimmed?: boolean }) {
  const ki = KIND_ICON[kind];
  return (
    <div
      className={`absolute top-1.5 left-1.5 z-10 flex items-center gap-0.5 px-1 rounded border text-[8px] leading-none font-medium
        bg-white/70 dark:bg-zinc-800/70 border-zinc-200/60 dark:border-zinc-700/60
        ${ki.color} ${dimmed ? "opacity-50" : ""}`}
      style={{ height: 16 }}
    >
      <ki.Icon size={10} />
      <span>{ki.label}</span>
    </div>
  );
}

const MAX_VISIBLE_CHIPS = 8;

const KIND_CHIP_ICON: Record<string, typeof Code> = {
  process: Workflow,
  model: Table,
  operation: Code,
};

type ChipItem = { id: string; name: string; status?: string; kind: string };

/** Render process/model/operation chips with overflow cap */
function MemberChipList({
  processes,
  models,
  operations,
  dimmed,
}: {
  processes?: { id: string; name: string; status?: string }[];
  models?: { id: string; name: string; status?: string }[];
  operations?: { id: string; name: string; status?: string }[];
  dimmed?: boolean;
}) {
  const all: ChipItem[] = [];
  if (processes) for (const p of processes) all.push({ ...p, kind: "process" });
  if (models) for (const m of models) all.push({ ...m, kind: "model" });
  if (operations)
    for (const o of operations) all.push({ ...o, kind: "operation" });
  if (all.length === 0) return null;

  const overflow = all.length - MAX_VISIBLE_CHIPS;
  const visible = overflow > 0 ? all.slice(0, MAX_VISIBLE_CHIPS) : all;

  return (
    <div className="flex flex-col gap-1">
      {visible.map((item) => {
        const Icon = KIND_CHIP_ICON[item.kind] ?? Code;
        const name = item.name || "unnamed";
        const sc = item.status
          ? STATUS_COLORS[item.status as keyof typeof STATUS_COLORS]
          : null;
        return (
          <div
            key={item.id}
            draggable
            className={`nodrag flex items-center gap-1 rounded px-1.5 py-0.5 text-[9px] font-mono leading-tight cursor-grab border ${
              dimmed
                ? "bg-zinc-100/50 text-zinc-400 dark:bg-zinc-800/30 dark:text-zinc-500"
                : "bg-zinc-100 text-zinc-600 dark:bg-zinc-800 dark:text-zinc-300"
            } ${!sc ? (dimmed ? "border-zinc-200/60 dark:border-zinc-700/40" : "border-zinc-200 dark:border-zinc-700") : ""}`}
            style={
              sc
                ? { borderColor: dimmed ? sc.hex + "40" : sc.hex + "99" }
                : undefined
            }
            onDragStart={(e: DragEvent) => {
              e.dataTransfer.setData("text/plain", item.id);
              e.dataTransfer.effectAllowed = "move";
              document.body.style.cursor = "grabbing";
              const ghost = document.createElement("div");
              ghost.textContent = name;
              ghost.style.cssText =
                "position:fixed;top:-100px;padding:2px 8px;border-radius:4px;font-size:10px;background:#27272a;color:#d4d4d8;border:1px solid #3f3f46;white-space:nowrap;font-family:monospace;";
              document.body.appendChild(ghost);
              e.dataTransfer.setDragImage(ghost, 0, 0);
              requestAnimationFrame(() => ghost.remove());
            }}
            onDragEnd={() => {
              document.body.style.cursor = "";
            }}
          >
            <Icon size={10} className="shrink-0 opacity-50" />
            <span className="truncate -mb-0.5">{name}</span>
          </div>
        );
      })}
      {overflow > 0 && (
        <div
          className={`text-center text-[9px] font-mono leading-tight py-0.5 ${
            dimmed
              ? "text-zinc-500 dark:text-zinc-600"
              : "text-zinc-400 dark:text-zinc-500"
          }`}
        >
          +{overflow} more
        </div>
      )}
    </div>
  );
}

/** Compact thumbnail strip for node attachments */
function AttachmentStrip({ attachments }: { attachments: Attachment[] }) {
  if (attachments.length === 0) return null;
  return (
    <div className="w-full px-2 py-1">
      <div className="flex gap-1 overflow-hidden">
        {attachments.slice(0, 3).map((att) => (
          <img
            key={att.id}
            src={`data:${att.mimeType};base64,${att.data}`}
            alt={att.filename}
            className="rounded border border-zinc-200 dark:border-zinc-700 object-cover"
            style={{ width: 52, height: 36 }}
          />
        ))}
        {attachments.length > 3 && (
          <div
            className="flex items-center justify-center rounded border border-zinc-200 dark:border-zinc-700 bg-zinc-100 dark:bg-zinc-800 text-[9px] text-zinc-400 dark:text-zinc-500"
            style={{ width: 52, height: 36 }}
          >
            +{attachments.length - 3}
          </div>
        )}
      </div>
    </div>
  );
}

export function C4Node({ id, data, selected }: NodeProps<C4NodeType>) {
  const shape = resolveShape(data.kind, data.shape);
  const insets = getContentInsets(shape);
  const members = data._operations as
    | { id: string; name: string }[]
    | undefined;
  const processes = data._processes as
    | { id: string; name: string; status?: string }[]
    | undefined;
  const models = data._models as
    | { id: string; name: string; status?: string }[]
    | undefined;
  const hasMembers =
    (members && members.length > 0) ||
    (processes && processes.length > 0) ||
    (models && models.length > 0);
  const attachments = data.attachments ?? [];
  const isComponent = data.kind === "component";

  const dragCounter = useRef(0);
  const [dragOver, setDragOver] = useState(false);

  const handleDragOver = (e: DragEvent) => {
    if (!isComponent) return;
    e.preventDefault();
    e.dataTransfer.dropEffect = "move";
  };

  const handleDragEnter = (e: DragEvent) => {
    if (!isComponent) return;
    e.preventDefault();
    dragCounter.current++;
    if (dragCounter.current === 1) setDragOver(true);
  };

  const handleDragLeave = () => {
    dragCounter.current--;
    if (dragCounter.current <= 0) {
      dragCounter.current = 0;
      setDragOver(false);
    }
  };

  const handleDrop = (e: DragEvent) => {
    e.preventDefault();
    dragCounter.current = 0;
    setDragOver(false);
    if (!isComponent) return;
    const operationId = e.dataTransfer.getData("text/plain");
    if (!operationId) return;
    // Don't accept if already a child of this component
    const alreadyChild = members?.some((fn) => fn.id === operationId);
    if (alreadyChild) return;
    window.dispatchEvent(
      new CustomEvent("operation-reparent", {
        detail: { operationId, newParentId: id },
      }),
    );
  };

  // Reference node: dimmed, non-editable, shows relationships instead of description
  if (data._reference) {
    const isRefPerson = data.kind === "person";
    const refSilhouetteFill = selected ? "#a1a1aa" : "#52525b";
    return (
      <div
        className={`relative w-[180px]${dragOver ? " ring-2 ring-zinc-400 ring-dashed rounded" : ""}`}
        onDragOver={handleDragOver}
        onDragEnter={handleDragEnter}
        onDragLeave={handleDragLeave}
        onDrop={handleDrop}
      >
        <div className="relative h-[160px]">
          {!isRefPerson && (
            <ShapeBackground
              shape={shape}
              fillClass="fill-zinc-200 dark:fill-zinc-900"
              strokeClass={
                selected
                  ? "stroke-zinc-400 dark:stroke-zinc-500"
                  : "stroke-zinc-300 dark:stroke-zinc-700"
              }
              strokeWidth={selected ? 2 : 1}
              strokeDasharray={data.external ? "6 3" : undefined}
            />
          )}
          {!isRefPerson && <KindTab kind={data.kind} dimmed />}
          {data._codeLevel ? <CenterHandle /> : <NodeHandles />}

          <div
            className={`absolute flex flex-col justify-center items-center text-zinc-500 dark:text-zinc-400 ${isRefPerson ? "overflow-visible" : "overflow-hidden"}`}
            style={{
              top: insets.top,
              bottom: insets.bottom,
              left: insets.left,
              right: insets.right,
            }}
          >
            {isRefPerson && (
              <svg
                className="pointer-events-none overflow-visible shrink-0"
                width="180"
                height="72"
                viewBox="0 0 180 72"
                style={{ marginBottom: -20 }}
              >
                <defs>
                  <linearGradient
                    id={`person-fade-ref-${id}`}
                    gradientUnits="userSpaceOnUse"
                    x1="0"
                    y1="-20"
                    x2="0"
                    y2="72"
                  >
                    <stop
                      offset="0%"
                      stopColor={refSilhouetteFill}
                      stopOpacity="1"
                    />
                    <stop
                      offset="100%"
                      stopColor={refSilhouetteFill}
                      stopOpacity="0"
                    />
                  </linearGradient>
                </defs>
                <path
                  d={[
                    "M 33,72 C 33,42 48,28 76,24",
                    "A 22,26 0 1,1 104,24",
                    "C 132,28 147,42 147,72",
                    "Z",
                  ].join(" ")}
                  fill={`url(#person-fade-ref-${id})`}
                  opacity="0.7"
                />
              </svg>
            )}
            <div className="w-full text-center text-sm font-semibold leading-tight break-all">
              <span className="text-zinc-400 dark:text-zinc-500">
                {data._relationships?.[0]?.direction === "out"
                  ? "\u2190"
                  : "\u2192"}
              </span>{" "}
              {data.name}
            </div>
            {data.technology && (
              <div className="mt-0.5 text-center text-[10px] tracking-wider">
                {data.technology}
              </div>
            )}

            {data.description && (
              <div className="mt-2 w-full text-[10px] leading-snug break-words overflow-hidden text-center">
                <DescriptionText
                  text={data.description}
                  onMentionClick={(name) =>
                    window.dispatchEvent(
                      new CustomEvent("mention-click", { detail: { name } }),
                    )
                  }
                />
              </div>
            )}
          </div>
        </div>
        {isComponent && hasMembers && (
          <div className="w-full px-2 py-1.5 opacity-60">
            <MemberChipList
              processes={processes}
              models={models}
              operations={members}
              dimmed
            />
          </div>
        )}
      </div>
    );
  }

  const isExternal = data.external;
  const expandable = isExpandable(data.kind) && !isExternal;
  const nodeHints = (data._hints as Hint[] | undefined) ?? [];
  const statusColor =
    data.status && data.kind !== "person" && !isExternal
      ? STATUS_COLORS[data.status]
      : null;

  // Person nodes: silhouette above, no background rect, normal text layout
  if (data.kind === "person") {
    const isDark = window.matchMedia("(prefers-color-scheme: dark)").matches;
    const silhouetteFill = selected
      ? isDark
        ? "#a1a1aa"
        : "#27272a"
      : "#52525b";
    const longDesc = (data.description?.length ?? 0) > 80;
    return (
      <div className="relative w-[180px] h-[160px]">
        <HintBadge nodeId={id} hints={nodeHints} />
        <NodeHandles />
        {/* Content — silhouette flows with text */}
        <div
          className="absolute flex flex-col justify-center items-center text-zinc-800 dark:text-zinc-100 overflow-visible"
          style={{ top: longDesc ? -20 : 6, bottom: 6, left: 8, right: 8 }}
        >
          {/* Person silhouette — inline, extends above via negative margin */}
          <svg
            className="pointer-events-none overflow-visible shrink-0"
            width="200"
            height="80"
            viewBox="0 0 180 72"
            style={{ marginBottom: -34 }}
          >
            <defs>
              <linearGradient
                id={`person-fade-${id}`}
                gradientUnits="userSpaceOnUse"
                x1="0"
                y1="-20"
                x2="0"
                y2="72"
              >
                <stop offset="0%" stopColor={silhouetteFill} stopOpacity="1" />
                <stop
                  offset="100%"
                  stopColor={silhouetteFill}
                  stopOpacity="0"
                />
              </linearGradient>
            </defs>
            <path
              d={[
                "M 33,72 C 33,42 48,28 76,24", // left shoulder up
                "A 22,26 0 1,1 104,24", // head oval
                "C 132,28 147,42 147,72", // right shoulder down
                "Z",
              ].join(" ")}
              fill={`url(#person-fade-${id})`}
            />
          </svg>
          <div className="w-full text-center text-sm font-semibold leading-tight break-all">
            {data.name}
          </div>
          {data.description && (
            <div className="mt-2 w-full text-[10px] leading-snug text-zinc-400 dark:text-zinc-500 break-words overflow-hidden text-center">
              <DescriptionText
                text={data.description}
                onMentionClick={(name) =>
                  window.dispatchEvent(
                    new CustomEvent("mention-click", { detail: { name } }),
                  )
                }
              />
            </div>
          )}
        </div>
      </div>
    );
  }

  const groupHighlight = (data as Record<string, unknown>)._groupHighlight;

  return (
    <div
      className={`relative w-[180px]${dragOver ? " ring-2 ring-zinc-400 ring-dashed rounded" : ""}`}
      onDragOver={handleDragOver}
      onDragEnter={handleDragEnter}
      onDragLeave={handleDragLeave}
      onDrop={handleDrop}
    >
      <div className="relative h-[160px]">
        <HintBadge nodeId={id} hints={nodeHints} />
        <ShapeBackground
          shape={shape}
          fillClass={
            isExternal
              ? "fill-zinc-100 dark:fill-zinc-700"
              : "fill-white dark:fill-zinc-800"
          }
          strokeClass={
            selected
              ? "stroke-zinc-900 dark:stroke-zinc-300"
              : statusColor
                ? groupHighlight
                  ? statusColor.strokeClass
                  : statusColor.dimStrokeClass
                : groupHighlight
                  ? "stroke-zinc-400 dark:stroke-zinc-500"
                  : "stroke-zinc-200 dark:stroke-zinc-700"
          }
          strokeWidth={selected ? 2.5 : groupHighlight || statusColor ? 2 : 1}
          strokeDasharray={isExternal ? "6 3" : undefined}
        />
        {/* Deprecated status: line through center */}
        {data.status === "deprecated" && !isExternal && (
          <svg
            className="absolute inset-0 pointer-events-none"
            width="180"
            height="160"
            viewBox="0 0 180 160"
          >
            <line
              x1="20"
              y1="80"
              x2="160"
              y2="80"
              stroke="#f87171"
              strokeWidth="1.5"
              opacity="0.5"
            />
          </svg>
        )}
        <NodeHandles />
        <KindTab kind={data.kind} />

        {/* Drill-in — visible when selected */}
        {expandable && selected && (
          <div className="absolute top-1.5 right-1.5 flex items-center z-10">
            <button
              className="nodrag text-[10px] tracking-wider text-zinc-400 dark:text-zinc-500 cursor-pointer"
              onClick={() =>
                window.dispatchEvent(
                  new CustomEvent("node-expand", { detail: { nodeId: id } }),
                )
              }
            >
              &#8599;
            </button>
          </div>
        )}

        {/* Content area */}
        <div
          className="absolute flex flex-col justify-center items-center text-zinc-800 dark:text-zinc-100 overflow-hidden"
          style={{
            top: insets.top,
            bottom: insets.bottom,
            left: insets.left,
            right: insets.right,
          }}
        >
          {/* Title */}
          <div className="w-full text-center text-sm font-semibold leading-tight break-all">
            {data.name}
          </div>

          {/* Technology */}
          {data.technology && (
            <div className="mt-0.5 text-center text-[10px] tracking-wider text-zinc-500 dark:text-zinc-400">
              {data.technology}
            </div>
          )}

          {/* Description */}
          {data.description && (
            <div className="mt-2 w-full text-[10px] leading-snug text-zinc-400 dark:text-zinc-500 break-words overflow-hidden text-center">
              <DescriptionText
                text={data.description}
                onMentionClick={(name) =>
                  window.dispatchEvent(
                    new CustomEvent("mention-click", { detail: { name } }),
                  )
                }
              />
            </div>
          )}
        </div>
      </div>
      {isComponent && hasMembers && (
        <div className="w-full px-2 py-1.5">
          <MemberChipList
            processes={processes}
            models={models}
            operations={members}
          />
        </div>
      )}
      {(data.kind === "container" || data.kind === "component") && (
        <AttachmentStrip attachments={attachments} />
      )}
    </div>
  );
}
