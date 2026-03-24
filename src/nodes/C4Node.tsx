import { useContext, useRef, useState, type DragEvent } from "react";
import type { NodeProps } from "@xyflow/react";
import type {
  C4Node as C4NodeType,
  C4NodeData,
  Hint,
  Status,
  Contract,
} from "../types";
import { NodeHandles, CenterHandle } from "./NodeHandles";
import { ShapeBackground, resolveShape, getContentInsets } from "../shapes";
import { HintBadge } from "./HintBadge";
import { ContractBadge } from "./ContractBadge";
import { STATUS_COLORS, statusHex } from "../statusColors";
import { getThemedHex, ThemeContext } from "../theme";
import { DescriptionText } from "../DescriptionText";
import { Code, Workflow, Table, Layers, RefreshCw } from "lucide-react";

/** Whether this kind can be drilled into */
function isExpandable(kind: C4NodeData["kind"]): boolean {
  return kind === "system" || kind === "container" || kind === "component";
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
            className={`nodrag flex items-center gap-1 rounded px-1.5 py-0.5 text-[10px] font-mono leading-tight cursor-grab border ${
              dimmed
                ? "bg-[var(--surface-inset)] text-[var(--text-muted)]"
                : "bg-[var(--surface-tint)] text-[var(--text-secondary)]"
            } ${!sc ? (dimmed ? "border-[var(--border-subtle)]" : "border-[var(--border)]") : ""}`}
            style={
              sc && item.status
                ? { borderColor: dimmed ? statusHex(item.status as Status) + "40" : statusHex(item.status as Status) + "99" }
                : undefined
            }
            onDragStart={(e: DragEvent) => {
              e.dataTransfer.setData("text/plain", item.id);
              e.dataTransfer.effectAllowed = "move";
              document.body.style.cursor = "grabbing";
              const ghost = document.createElement("div");
              ghost.textContent = name;
              ghost.style.cssText =
                `position:fixed;top:-100px;padding:2px 8px;border-radius:4px;font-size:10px;background:${getThemedHex("zinc", "900")};color:${getThemedHex("zinc", "300")};border:1px solid ${getThemedHex("zinc", "700")};white-space:nowrap;font-family:monospace;`;
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
          className={`text-center text-[10px] font-mono leading-tight py-0.5 ${
            dimmed
              ? "text-[var(--text-tertiary)]"
              : "text-[var(--text-muted)]"
          }`}
        >
          +{overflow} more
        </div>
      )}
    </div>
  );
}


export function C4Node({ id, data, selected }: NodeProps<C4NodeType>) {
  useContext(ThemeContext); // re-render on theme change for inline hex styles
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
    const refSilhouetteFill = "var(--scryer-person-fill)";
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
              fillClass="fill-[var(--scryer-ref-bg)]"
              strokeClass={
                selected
                  ? "stroke-[var(--text)]"
                  : "stroke-[var(--scryer-outline-stroke)]"
              }
              strokeWidth={selected ? 2.5 : 1}
              strokeDasharray={data.external ? "6 3" : undefined}
              kind={data.kind}
              external={!!data.external}
            />
          )}
          {data._codeLevel ? <CenterHandle /> : <NodeHandles />}

          <div
            className={`absolute flex flex-col justify-center items-center text-[var(--text-tertiary)] ${isRefPerson ? "overflow-visible" : "overflow-hidden"}`}
            style={{
              top: isRefPerson && (data.description?.length ?? 0) > 80 ? -20 : insets.top,
              bottom: insets.bottom,
              left: insets.left,
              right: insets.right,
            }}
          >
            {isRefPerson && (
              <svg
                className="pointer-events-none overflow-visible shrink-0"
                width="200"
                height="80"
                viewBox="0 0 180 72"
                style={{ marginBottom: -34 }}
              >
                <defs>
                  <linearGradient
                    id={`person-fade-ref-${id}`}
                    gradientUnits="userSpaceOnUse"
                    x1="0"
                    y1="40"
                    x2="0"
                    y2="72"
                  >
                    <stop offset="0%" stopColor={refSilhouetteFill} stopOpacity="1" />
                    <stop offset="100%" stopColor={refSilhouetteFill} stopOpacity="0" />
                  </linearGradient>
                  <linearGradient
                    id={`person-stroke-fade-ref-${id}`}
                    gradientUnits="userSpaceOnUse"
                    x1="0"
                    y1="0"
                    x2="0"
                    y2="72"
                  >
                    <stop offset="0%" stopColor={selected ? "var(--scryer-select-stroke)" : "var(--scryer-outline-stroke)"} stopOpacity={selected ? 1 : 0.8} />
                    <stop offset="70%" stopColor={selected ? "var(--scryer-select-stroke)" : "var(--scryer-outline-stroke)"} stopOpacity={selected ? 0.8 : 0.3} />
                    <stop offset="100%" stopColor={selected ? "var(--scryer-select-stroke)" : "var(--scryer-outline-stroke)"} stopOpacity="0" />
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
                <path
                  d={[
                    "M 33,72 C 33,42 48,28 76,24",
                    "A 22,26 0 1,1 104,24",
                    "C 132,28 147,42 147,72",
                  ].join(" ")}
                  fill="none"
                  stroke={`url(#person-stroke-fade-ref-${id})`}
                  strokeWidth={selected ? 2.5 : 1}
                />
              </svg>
            )}
            <div className="w-full text-center text-sm font-semibold leading-tight break-all">
              <span className="text-[var(--text-muted)]">
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
  const hasChildren = !!(data as Record<string, unknown>)._hasChildren;
  const nodeHints = (data._hints as Hint[] | undefined) ?? [];
  const statusColor =
    data.status && data.kind !== "person" && !isExternal
      ? STATUS_COLORS[data.status]
      : null;

  // Person nodes: silhouette above, no background rect, normal text layout
  if (data.kind === "person") {
    const silhouetteFill = "var(--scryer-person-fill)";
    const longDesc = (data.description?.length ?? 0) > 80;
    return (
      <div className="relative w-[180px] h-[160px]">
        <HintBadge nodeId={id} hints={nodeHints} />
        <NodeHandles />
        {/* Content — silhouette flows with text */}
        <div
          className="absolute flex flex-col justify-center items-center text-[var(--text)] overflow-visible"
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
                y1="40"
                x2="0"
                y2="72"
              >
                <stop offset="0%" stopColor={silhouetteFill} stopOpacity="1" />
                <stop offset="100%" stopColor={silhouetteFill} stopOpacity="0" />
              </linearGradient>
              <linearGradient
                id={`person-stroke-fade-${id}`}
                gradientUnits="userSpaceOnUse"
                x1="0"
                y1="0"
                x2="0"
                y2="72"
              >
                <stop offset="0%" stopColor={selected ? "var(--scryer-select-stroke)" : "var(--scryer-outline-stroke)"} stopOpacity={selected ? 1 : 0.8} />
                <stop offset="70%" stopColor={selected ? "var(--scryer-select-stroke)" : "var(--scryer-outline-stroke)"} stopOpacity={selected ? 0.8 : 0.3} />
                <stop offset="100%" stopColor={selected ? "var(--scryer-select-stroke)" : "var(--scryer-outline-stroke)"} stopOpacity="0" />
              </linearGradient>
            </defs>
            <path
              d={[
                "M 33,72 C 33,42 48,28 76,24",
                "A 22,26 0 1,1 104,24",
                "C 132,28 147,42 147,72",
                "Z",
              ].join(" ")}
              fill={`url(#person-fade-${id})`}
            />
            <path
              d={[
                "M 33,72 C 33,42 48,28 76,24",
                "A 22,26 0 1,1 104,24",
                "C 132,28 147,42 147,72",
              ].join(" ")}
              fill="none"
              stroke={`url(#person-stroke-fade-${id})`}
              strokeWidth={selected ? 2.5 : 1}
            />
          </svg>
          <div className="w-full text-center text-sm font-semibold leading-tight break-all">
            {data.name}
          </div>
          {data.description && (
            <div className="mt-2 w-full text-[10px] leading-snug text-[var(--text-muted)] break-words overflow-hidden text-center">
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
  const changed = (data as Record<string, unknown>)._changed;
  const drifted = (data as Record<string, unknown>)._drifted;

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
        <ContractBadge contract={data.contract as Contract | undefined} />
        <ShapeBackground
          shape={shape}
          fillClass={
            isExternal
              ? "fill-[var(--scryer-ext-bg)]"
              : "fill-[var(--scryer-node-bg)]"
          }
          strokeClass={
            selected
              ? "stroke-[var(--text)]"
              : statusColor
                ? groupHighlight
                  ? statusColor.strokeClass
                  : statusColor.dimStrokeClass
                : groupHighlight
                  ? "stroke-[var(--text-muted)]"
                  : isExternal
                    ? "stroke-[var(--scryer-outline-stroke)]"
                    : "stroke-[var(--border)]"
          }
          strokeWidth={selected ? 2.5 : groupHighlight || statusColor ? 2 : 1}
          strokeDasharray={isExternal ? "6 3" : undefined}
          kind={data.kind}
          external={!!isExternal}
          changed={!!changed}
        />
        <NodeHandles />

        {/* Drill-in — visible when selected */}
        {expandable && selected && (
          <div className="absolute top-1.5 right-1.5 flex items-center z-10">
            <button
              className="nodrag flex items-center justify-center w-5 h-5 rounded bg-[var(--surface-tint)] text-[var(--text-secondary)] text-xs cursor-pointer hover:bg-[var(--surface-active)] transition-colors"
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

        {/* Drift indicator — bottom left */}
        {!!drifted && (
          <div className="absolute bottom-2 left-2.5 z-10 pointer-events-none">
            <RefreshCw size={12} strokeWidth={2} className="text-indigo-500 dark:text-indigo-400" />
          </div>
        )}

        {/* Has-children indicator */}
        {expandable && hasChildren && (
          <div className="absolute bottom-2 right-2.5 z-10 text-[var(--text-ghost)] pointer-events-none">
            <Layers size={12} strokeWidth={1.5} />
          </div>
        )}

        {/* Content area */}
        <div
          className="absolute flex flex-col justify-center items-center text-[var(--text)] overflow-hidden"
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
            <div className="mt-0.5 text-center text-[10px] tracking-wider text-[var(--text-tertiary)]">
              {data.technology}
            </div>
          )}

          {/* Description */}
          {data.description && (
            <div className="mt-2 w-full text-[10px] leading-snug text-[var(--text-muted)] break-words overflow-hidden text-center">
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
    </div>
  );
}
