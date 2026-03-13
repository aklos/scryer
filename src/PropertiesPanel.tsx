import { useState, useEffect, useRef, type ReactNode } from "react";
import { useReactFlow } from "@xyflow/react";
import { useNodeDataOverride } from "./NodeDataContext";
import { invoke } from "@tauri-apps/api/core";
import type { C4Node, C4NodeData, C4Edge, Hint, Status, SourceLocation, ModelProperty, Group, Contract, ContractItem, ContractImage, Flow } from "./types";
import { contractText, contractPassed, contractUrl } from "./types";
import { ShapeIcon, resolveShape, defaultShapeForKind, ALL_SHAPES } from "./shapes";
import { statusHex } from "./statusColors";
import { getThemedHex } from "./theme";
import { Button, Input, Textarea, Section, Divider, KVRow, Toggle, Field } from "./ui";
import { STATUS_COLORS } from "./statusColors";
import { MentionTextarea, type MentionItem } from "./MentionTextarea";

/** Use context override (code-level rack) or fall back to ReactFlow's updateNodeData */
function useUpdateNodeData(): (id: string, data: Record<string, unknown>) => void {
  const override = useNodeDataOverride();
  const rf = useReactFlow();
  return override ?? rf.updateNodeData;
}

/** Sanitize to camelCase / snake_case: only [a-zA-Z0-9_], first char must be lowercase letter */
function sanitizeIdentifier(raw: string): string {
  const stripped = raw.replace(/[^a-zA-Z0-9_]/g, "");
  if (stripped.length === 0) return "";
  const first = stripped[0];
  if (/[a-zA-Z]/.test(first)) return first.toLowerCase() + stripped.slice(1);
  return stripped.slice(1);
}

/** Like sanitizeIdentifier but allows PascalCase (uppercase first letter) */
function sanitizeTypeName(raw: string): string {
  const stripped = raw.replace(/[^a-zA-Z0-9_]/g, "");
  if (stripped.length === 0) return "";
  const first = stripped[0];
  if (/[a-zA-Z]/.test(first)) return first + stripped.slice(1);
  return stripped.slice(1);
}

/* ── Status bar ───────────────────────────────────────────────────── */

const STATUS_OPTIONS: { value: Status | undefined; label: string }[] = [
  { value: undefined, label: "None" },
  { value: "proposed", label: "Proposed" },
  { value: "wip", label: "WIP" },
  { value: "ready", label: "Ready" },
];

function StatusBar({ value, onChange }: { value: Status | undefined; onChange: (s: Status | undefined) => void }) {
  return (
    <div className="flex items-center gap-0.5 rounded-lg bg-zinc-100 dark:bg-zinc-800/80 p-0.5">
      {STATUS_OPTIONS.map((opt) => {
        const isActive = value === opt.value;
        const sc = opt.value ? STATUS_COLORS[opt.value] : null;
        return (
          <button
            key={String(opt.value ?? "__none__")}
            type="button"
            className={`flex-1 flex items-center justify-center gap-1.5 py-1.5 text-[11px] font-medium rounded-md cursor-pointer transition-all ${
              isActive
                ? sc
                  ? `${sc.pillClass} shadow-sm`
                  : "bg-white dark:bg-zinc-700 text-zinc-600 dark:text-zinc-300 shadow-sm"
                : "text-zinc-400 dark:text-zinc-500 hover:text-zinc-500 dark:hover:text-zinc-400"
            }`}
            onClick={() => onChange(opt.value)}
          >
            {sc && isActive && <span className={`w-1.5 h-1.5 rounded-full ${sc.dotClass}`} />}
            {opt.label}
          </button>
        );
      })}
    </div>
  );
}

/* ── Tab type ─────────────────────────────────────────────────────── */

type PanelTab = { id: string; label: string; content: ReactNode; badge?: ReactNode };

/* ── Constants ────────────────────────────────────────────────────── */

const TECHNOLOGY_SUGGESTIONS: Record<string, string[]> = {
  container: ["TypeScript", "Rust", "Python", "Go", "Java", "C#", "PostgreSQL", "Redis"],
  component: ["React", "Next.js", "Express", "Spring", "Django", "Axum", "GraphQL", "gRPC"],
};

/* ── Sub-panels (edge, group — no tabs) ──────────────────────────── */

function GroupPropertiesContent({ node, groups, onUpdateGroups, allNodes }: { node: C4Node; groups: Group[]; onUpdateGroups: (fn: (prev: Group[]) => Group[]) => void; allNodes: C4Node[] }) {
  const group = groups.find((g) => g.id === node.id);
  if (!group) return null;

  const removeMember = (memberId: string) => {
    onUpdateGroups((prev) => {
      const updated = prev.map((g) =>
        g.id === group.id ? { ...g, memberIds: g.memberIds.filter((id) => id !== memberId) } : g,
      );
      return updated.filter((g) => g.memberIds.length > 0);
    });
  };

  return (
    <>
      <Input
        variant="title"
        value={group.name}
        onChange={(e) => onUpdateGroups((prev) => prev.map((g) => g.id === group.id ? { ...g, name: e.target.value } : g))}
        placeholder="Group name..."
      />
      <Divider />
      <Field label="Description">
        <Textarea
          rows={3}
          value={group.description ?? ""}
          onChange={(e) => onUpdateGroups((prev) => prev.map((g) => g.id === group.id ? { ...g, description: e.target.value || undefined } : g))}
          placeholder="What does this group represent?"
        />
      </Field>
      <Divider />
      <Section title="Members" count={group.memberIds.length}>
        <div className="flex flex-col gap-1">
          {group.memberIds.map((memberId) => {
            const memberNode = allNodes.find((n) => n.id === memberId);
            const name = (memberNode?.data as C4NodeData | undefined)?.name ?? memberId;
            return (
              <div key={memberId} className="flex items-center gap-1.5 group rounded-md px-1.5 py-1 -mx-1.5 hover:bg-zinc-100/60 dark:hover:bg-zinc-800/60 transition-colors">
                <span className="truncate flex-1 text-xs text-zinc-600 dark:text-zinc-300">{name}</span>
                <button
                  type="button"
                  className="shrink-0 text-xs text-zinc-300 hover:text-red-400 dark:text-zinc-600 dark:hover:text-red-400 cursor-pointer opacity-0 group-hover:opacity-100"
                  title="Remove from group"
                  onClick={() => removeMember(memberId)}
                >
                  &times;
                </button>
              </div>
            );
          })}
        </div>
      </Section>
    </>
  );
}

function GroupContractContent({ node, groups, onUpdateGroups }: { node: C4Node; groups: Group[]; onUpdateGroups: (fn: (prev: Group[]) => Group[]) => void }) {
  const group = groups.find((g) => g.id === node.id);
  if (!group) return null;
  const raw = group.contract;
  const contract = { expect: raw?.expect ?? [], ask: raw?.ask ?? [], never: raw?.never ?? [] };

  const updateField = (field: keyof Contract, items: ContractItem[]) => {
    onUpdateGroups((prev) => prev.map((g) => g.id === group.id ? { ...g, contract: { ...contract, [field]: items } } : g));
  };

  return (
    <>
      <Section title="Expected">
        <ContractList items={contract.expect} onChange={(items) => updateField("expect", items)} placeholder="Expected to..." />
      </Section>
      <Divider />
      <Section title="Ask first">
        <ContractList items={contract.ask} onChange={(items) => updateField("ask", items)} placeholder="Confirm before..." showToggle={false} />
      </Section>
      <Divider />
      <Section title="Never">
        <ContractList items={contract.never} onChange={(items) => updateField("never", items)} placeholder="Must never..." showToggle={false} />
      </Section>
    </>
  );
}

function MultiSelectionPanel({ selectedIds, groups, groupKind, onCreateGroup, onAddToGroup }: {
  selectedIds: string[];
  groups: Group[];
  groupKind?: "deployment" | "package";
  onCreateGroup: (name: string, memberIds: string[]) => void;
  onAddToGroup: (groupId: string, memberIds: string[]) => void;
}) {
  const [name, setName] = useState("New group");

  return (
    <>
      <span className="text-xs font-medium text-zinc-500 dark:text-zinc-400">
        {selectedIds.length} nodes selected
      </span>
      <Divider />
      <Section title={groupKind === "package" ? "Create package group" : "Create deployment group"}>
        <Input
          variant="title"
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="Group name..."
          onKeyDown={(e) => {
            if (e.key === "Enter" && name.trim()) {
              onCreateGroup(name.trim(), selectedIds);
              setName("New group");
            }
          }}
        />
        <Button
          variant="primary"
          onClick={() => {
            if (name.trim()) {
              onCreateGroup(name.trim(), selectedIds);
              setName("New group");
            }
          }}
        >
          Create group
        </Button>
      </Section>
      {groups.length > 0 && (
        <>
          <Divider />
          <Section title="Add to existing">
            <div className="flex flex-col gap-1">
              {groups.map((g) => (
                <button
                  key={g.id}
                  type="button"
                  className="flex items-center gap-1.5 rounded-md px-2 py-1.5 text-xs text-left text-zinc-600 dark:text-zinc-300 hover:bg-zinc-100/60 dark:hover:bg-zinc-800/60 transition-colors cursor-pointer"
                  onClick={() => onAddToGroup(g.id, selectedIds)}
                >
                  <span className="truncate flex-1">{g.name}</span>
                  <span className="text-[10px] text-zinc-500 dark:text-zinc-500">{g.memberIds.length}</span>
                </button>
              ))}
            </div>
          </Section>
        </>
      )}
    </>
  );
}

function EdgePanel({ edge, onUpdate, codeLevel }: { edge: C4Edge; onUpdate: (data: { label?: string; method?: string }) => void; codeLevel?: boolean }) {
  const label = edge.data?.label ?? "";
  const method = edge.data?.method ?? "";

  return (
    <>
      <Section title="Connection">
        <KVRow label="Label">
          <Input
            variant="inline"
            value={label}
            maxLength={30}
            placeholder="e.g. reads from"
            onChange={(e) => onUpdate({ label: e.target.value, method: method || undefined })}
          />
        </KVRow>
        {!codeLevel && (
          <KVRow label="Method">
            <Input
              variant="inline"
              value={method}
              placeholder="e.g. REST/JSON"
              onChange={(e) => onUpdate({ label, method: e.target.value || undefined })}
            />
          </KVRow>
        )}
      </Section>
    </>
  );
}


/* ── Image resize helper ──────────────────────────────────────────── */

const MAX_IMAGE_DIM = 1280;

function resizeImage(file: File): Promise<ContractImage> {
  return new Promise((resolve) => {
    const img = new Image();
    img.onload = () => {
      let { width, height } = img;
      if (width <= MAX_IMAGE_DIM && height <= MAX_IMAGE_DIM) {
        const reader = new FileReader();
        reader.onload = () => {
          const b64 = (reader.result as string).split(",")[1];
          if (b64) resolve({ filename: file.name, mimeType: file.type || "image/png", data: b64 });
        };
        reader.readAsDataURL(file);
        return;
      }
      const scale = Math.min(MAX_IMAGE_DIM / width, MAX_IMAGE_DIM / height);
      width = Math.round(width * scale);
      height = Math.round(height * scale);
      const canvas = document.createElement("canvas");
      canvas.width = width;
      canvas.height = height;
      const ctx = canvas.getContext("2d")!;
      ctx.drawImage(img, 0, 0, width, height);
      const dataUrl = canvas.toDataURL("image/jpeg", 0.85);
      resolve({ filename: file.name, mimeType: "image/jpeg", data: dataUrl.split(",")[1]! });
    };
    img.src = URL.createObjectURL(file);
  });
}

/* ── Node tab content ─────────────────────────────────────────────── */

function NodePropertiesContent({ node, sourceLocations, projectPath, mentionNames }: { node: C4Node; sourceLocations?: SourceLocation[]; projectPath?: string; mentionNames?: MentionItem[] }) {
  const updateNodeData = useUpdateNodeData();
  const { data } = node;
  const isCodeLevel = data.kind === "operation";
  const showTechnology = data.kind === "container" || data.kind === "component";
  const showShape = data.kind !== "person" && !isCodeLevel;
  const suggestions = TECHNOLOGY_SUGGESTIONS[data.kind] ?? [];
  const listId = `panel-tech-${node.id}`;

  return (
    <>
      {/* ── Identity ── */}
      <Input
        variant="title"
        value={data.name}
        placeholder={isCodeLevel ? "e.g. handleLogin" : "Name..."}
        className={isCodeLevel ? "font-mono" : undefined}
        onChange={(e) => updateNodeData(node.id, { name: isCodeLevel ? sanitizeIdentifier(e.target.value) : e.target.value })}
      />

      {showTechnology && (
        <>
          <Divider />
          <KVRow label="Technology">
            <Input
              variant="inline"
              list={listId}
              value={data.technology ?? ""}
              placeholder="e.g. REST API"
              maxLength={28}
              onChange={(e) => updateNodeData(node.id, { technology: e.target.value || undefined })}
            />
            <datalist id={listId}>
              {suggestions.map((s) => <option key={s} value={s} />)}
            </datalist>
          </KVRow>
        </>
      )}

      {showShape && (
        <>
          <Divider />
          <KVRow label="Shape">
            <div className="flex gap-0.5">
              {ALL_SHAPES.filter((s) => s !== "person").map((s) => {
                const effective = resolveShape(data.kind, data.shape);
                const isDefault = s === defaultShapeForKind(data.kind);
                return (
                  <ShapeIcon
                    key={s}
                    shape={s}
                    active={effective === s}
                    onClick={() =>
                      updateNodeData(node.id, { shape: isDefault ? undefined : s })
                    }
                  />
                );
              })}
            </div>
          </KVRow>
        </>
      )}

      {data.kind === "system" && (
        <>
          <Divider />
          <KVRow label="External">
            <Toggle value={!!data.external} onChange={(v) => updateNodeData(node.id, { external: v || undefined, ...(v ? { status: undefined } : {}) })} />
          </KVRow>
        </>
      )}

      <Divider />

      {/* ── Description ── */}
      <Field
        label="Description"
        trailing={
          <span className={`text-[10px] tabular-nums ${data.description.length > 180 ? "text-amber-500" : "text-zinc-400/50 dark:text-zinc-500/50"}`}>
            {data.description.length}/200
          </span>
        }
      >
        {isCodeLevel && mentionNames ? (
          <MentionTextarea
            value={data.description}
            onChange={(val) => updateNodeData(node.id, { description: val })}
            mentionNames={mentionNames}
            maxLength={200}
            rows={5}
            className="w-full rounded-md border border-zinc-200 dark:border-transparent bg-zinc-100/60 dark:bg-zinc-800/60 focus:bg-zinc-100 dark:focus:bg-zinc-700/60 px-2.5 py-2 text-xs text-zinc-700 dark:text-zinc-200 leading-relaxed outline-none resize-none transition-colors placeholder:text-zinc-400 dark:placeholder:text-zinc-500"
            placeholder="Describe this operation... Use @[Name] to reference sibling operations, processes, or models."
          />
        ) : (
          <Textarea
            rows={5}
            maxLength={200}
            value={data.description}
            placeholder="Describe this node..."
            onChange={(e) => updateNodeData(node.id, { description: e.target.value })}
          />
        )}
      </Field>

      {/* ── Status ── */}
      {data.kind !== "person" && !data.external && (
        <>
          <Divider />
          <Field label="Status">
            <StatusBar value={data.status} onChange={(s) => updateNodeData(node.id, { status: s, statusReason: undefined })} />
            {data.statusReason && (
              <p className="mt-1 text-[11px] text-zinc-500 dark:text-zinc-400 leading-relaxed italic">{data.statusReason}</p>
            )}
          </Field>
        </>
      )}

      {/* ── Sources ── */}
      {sourceLocations && sourceLocations.length > 0 && (
        <>
          <Divider />
          <Field label="Sources">
            <div className="flex flex-col gap-0.5 rounded-md border border-zinc-200/50 dark:border-zinc-700/50 overflow-hidden">
              {sourceLocations.map((loc, i) => {
                const isGlob = /[*?{}\[\]]/.test(loc.pattern);
                const inner = (
                  <>
                    <span className="truncate font-mono">{loc.pattern}</span>
                    {loc.line != null && <span className="shrink-0 text-zinc-500 font-mono">:{loc.line}</span>}
                  </>
                );
                return isGlob ? (
                  <span key={i} className="flex items-baseline gap-1 px-2.5 py-1.5 text-[11px] text-zinc-500 dark:text-zinc-400 truncate" title={loc.pattern}>
                    {inner}
                  </span>
                ) : (
                  <button
                    key={i}
                    type="button"
                    className="flex items-baseline gap-1 px-2.5 py-1.5 text-left text-[11px] text-blue-400 hover:bg-zinc-100 dark:hover:bg-zinc-800 cursor-pointer truncate transition-colors"
                    title={loc.pattern + (loc.line ? `:${loc.line}` : "")}
                    onClick={() => invoke("open_in_editor", { file: loc.pattern, line: loc.line, projectPath }).catch((e) => console.error("open_in_editor:", e))}
                  >
                    {inner}
                  </button>
                );
              })}
            </div>
          </Field>
        </>
      )}

      {(data.kind === "person" || data.external) && (
        <>
          <Divider />
          <span className="text-xs text-zinc-300 dark:text-zinc-600 italic px-3">No implementation tracking</span>
        </>
      )}

      {/* ── Notes ── */}
      {!isCodeLevel && (
        <>
          <Divider />
          <Section title="Notes" count={(data.notes ?? []).filter(Boolean).length || undefined}>
            <NotesList
              items={data.notes ?? []}
              onChange={(items) => updateNodeData(node.id, { notes: items.length ? items : undefined })}
            />
          </Section>
        </>
      )}

    </>
  );
}

function NodeChildrenContent({ node, onUpdateOperationData }: { node: C4Node; onUpdateOperationData?: (fnId: string, data: Record<string, unknown>) => void }) {
  const procs = node.data._processes as { id: string; name: string; status?: string }[] | undefined;
  const mdls = node.data._models as { id: string; name: string; status?: string }[] | undefined;
  const fns = node.data._operations as { id: string; name: string; status?: string }[] | undefined;

  return (
    <>
      {/* Processes */}
      <Section title="Processes" count={procs?.length}>
        {procs && procs.length > 0 && (
          <div className="flex flex-col gap-1">
            {procs.map((p) => {
              return (
              <div key={p.id} className="flex items-center gap-1.5 group rounded-md px-1.5 py-1 -mx-1.5 hover:bg-zinc-100/60 dark:hover:bg-zinc-800/60 transition-colors">
                <span
                  className="shrink-0 w-1.5 h-1.5 rounded-full"
                  style={{ backgroundColor: p.status ? statusHex(p.status as import("./types").Status) : getThemedHex("zinc", "400") }}
                />
                <Input
                  variant="inline"
                  className="!w-auto !text-left font-mono text-zinc-600 dark:text-zinc-300 focus:border-zinc-300 dark:focus:border-zinc-600"
                  value={p.name}
                  onChange={(e) => onUpdateOperationData?.(p.id, { name: e.target.value })}
                />
                <button
                  type="button"
                  className="shrink-0 text-xs text-zinc-300 hover:text-red-400 dark:text-zinc-600 dark:hover:text-red-400 cursor-pointer opacity-0 group-hover:opacity-100"
                  title="Remove process"
                  onClick={() => window.dispatchEvent(new CustomEvent("remove-node", { detail: { nodeId: p.id } }))}
                >
                  &times;
                </button>
              </div>
              );
            })}
          </div>
        )}
        <Button
          variant="link"
          onClick={() => window.dispatchEvent(new CustomEvent("add-process", { detail: { componentId: node.id } }))}
        >
          + add process
        </Button>
      </Section>

      <Divider />

      {/* Models */}
      <Section title="Models" count={mdls?.length}>
        {mdls && mdls.length > 0 && (
          <div className="flex flex-col gap-1">
            {mdls.map((m) => {
              return (
              <div key={m.id} className="flex items-center gap-1.5 group rounded-md px-1.5 py-1 -mx-1.5 hover:bg-zinc-100/60 dark:hover:bg-zinc-800/60 transition-colors">
                <span
                  className="shrink-0 w-1.5 h-1.5 rounded-full"
                  style={{ backgroundColor: m.status ? statusHex(m.status as import("./types").Status) : getThemedHex("zinc", "400") }}
                />
                <Input
                  variant="inline"
                  className="!w-auto !text-left font-mono text-zinc-600 dark:text-zinc-300 focus:border-zinc-300 dark:focus:border-zinc-600"
                  value={m.name}
                  onChange={(e) => onUpdateOperationData?.(m.id, { name: sanitizeIdentifier(e.target.value) })}
                />
                <button
                  type="button"
                  className="shrink-0 text-xs text-zinc-300 hover:text-red-400 dark:text-zinc-600 dark:hover:text-red-400 cursor-pointer opacity-0 group-hover:opacity-100"
                  title="Remove model"
                  onClick={() => window.dispatchEvent(new CustomEvent("remove-node", { detail: { nodeId: m.id } }))}
                >
                  &times;
                </button>
              </div>
              );
            })}
          </div>
        )}
        <Button
          variant="link"
          onClick={() => window.dispatchEvent(new CustomEvent("add-model", { detail: { componentId: node.id } }))}
        >
          + add model
        </Button>
      </Section>

      <Divider />

      {/* Operations */}
      <Section title="Operations" count={fns?.length}>
        {fns && fns.length > 0 && (
          <div className="flex flex-col gap-1">
            {fns.map((fn) => {
              return (
                <div key={fn.id} className="flex items-center gap-1.5 group rounded-md px-1.5 py-1 -mx-1.5 hover:bg-zinc-100/60 dark:hover:bg-zinc-800/60 transition-colors">
                  <span className="shrink-0 w-1.5 h-1.5 rounded-full" style={{ backgroundColor: fn.status ? statusHex(fn.status as import("./types").Status) : getThemedHex("zinc", "400") }} />
                  <Input
                    variant="inline"
                    className="!w-auto !text-left font-mono text-zinc-600 dark:text-zinc-300 focus:border-zinc-300 dark:focus:border-zinc-600"
                    value={fn.name}
                    onChange={(e) => onUpdateOperationData?.(fn.id, { name: sanitizeIdentifier(e.target.value) })}
                  />
                  <button
                    type="button"
                    className="shrink-0 text-xs text-zinc-300 hover:text-red-400 dark:text-zinc-600 dark:hover:text-red-400 cursor-pointer opacity-0 group-hover:opacity-100"
                    title="Remove operation"
                    onClick={() => window.dispatchEvent(new CustomEvent("remove-node", { detail: { nodeId: fn.id } }))}
                  >
                    &times;
                  </button>
                </div>
              );
            })}
          </div>
        )}
        <Button
          variant="link"
          onClick={() => window.dispatchEvent(new CustomEvent("add-operation", { detail: { componentId: node.id } }))}
        >
          + add operation
        </Button>
      </Section>
    </>
  );
}


/* ── Node contract content ──────────────────────────────────────── */

function ContractItemToggle({ passed, onClick }: { passed?: boolean; onClick: () => void }) {
  return (
    <button
      type="button"
      className={`shrink-0 w-3.5 h-3.5 rounded-full border cursor-pointer transition-colors mt-[3px] ${
        passed === true
          ? "bg-emerald-500 border-emerald-500"
          : passed === false
            ? "bg-red-500 border-red-500"
            : "border-zinc-300 dark:border-zinc-600 hover:border-zinc-400 dark:hover:border-zinc-500"
      }`}
      onClick={onClick}
      title={passed === true ? "Passing" : passed === false ? "Failing" : "Unchecked — click to mark"}
    >
      {passed === true && (
        <svg viewBox="0 0 14 14" className="w-full h-full text-white">
          <path d="M3.5 7.5 L6 10 L10.5 4.5" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" />
        </svg>
      )}
      {passed === false && (
        <svg viewBox="0 0 14 14" className="w-full h-full text-white">
          <path d="M4 4 L10 10 M10 4 L4 10" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
        </svg>
      )}
    </button>
  );
}

function contractImage(item: ContractItem): ContractImage | undefined {
  return typeof item === "string" ? undefined : item.image;
}

function ContractBulletItem({ item, focused, placeholder, onCommit, onFocus, onBlur, onEnter, onDeleteEmpty, onRemove, onToggle, onUrlChange, onImageChange }: {
  item: ContractItem; focused: boolean; placeholder?: string;
  onCommit: (value: string) => void; onFocus: () => void; onBlur: () => void;
  onEnter: () => void; onDeleteEmpty: () => void; onRemove: () => void;
  onToggle?: () => void; onUrlChange: (url: string | undefined) => void;
  onImageChange: (image: ContractImage | undefined) => void;
}) {
  const spanRef = useRef<HTMLSpanElement>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const [menuOpen, setMenuOpen] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!menuOpen) return;
    const close = (e: MouseEvent) => { if (menuRef.current && !menuRef.current.contains(e.target as Node)) setMenuOpen(false); };
    document.addEventListener("mousedown", close);
    return () => document.removeEventListener("mousedown", close);
  }, [menuOpen]);
  const text = contractText(item);
  const passed = contractPassed(item);
  const url = contractUrl(item);
  const image = contractImage(item);
  const [editingUrl, setEditingUrl] = useState(false);
  const [imageExpanded, setImageExpanded] = useState(false);

  useEffect(() => {
    if (spanRef.current && spanRef.current.textContent !== text && document.activeElement !== spanRef.current) {
      spanRef.current.textContent = text;
    }
  }, [text]);

  useEffect(() => {
    if (focused && spanRef.current && document.activeElement !== spanRef.current) {
      const el = spanRef.current;
      el.focus();
      const range = document.createRange();
      range.selectNodeContents(el);
      range.collapse(false);
      const sel = window.getSelection();
      sel?.removeAllRanges();
      sel?.addRange(range);
    }
  }, [focused]);

  const handleFileSelect = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;
    resizeImage(file).then((img) => onImageChange(img));
    e.target.value = "";
  };

  return (
    <div className={`group relative rounded-md px-1.5 py-1 border border-transparent ${focused ? "bg-zinc-100 dark:bg-zinc-800/80" : "hover:bg-zinc-50 dark:hover:bg-zinc-800/40"}`} onMouseLeave={() => setMenuOpen(false)}>
      <div className="flex items-start gap-2">
        {onToggle ? (
          <ContractItemToggle passed={passed} onClick={onToggle} />
        ) : (
          <span className="shrink-0 w-1.5 h-1.5 rounded-full bg-zinc-400 dark:bg-zinc-500 mt-[7px]" />
        )}
        <span
          ref={spanRef}
          contentEditable
          suppressContentEditableWarning
          className="flex-1 min-w-0 text-xs text-zinc-700 dark:text-zinc-200 caret-current outline-none leading-relaxed break-words empty:before:content-[attr(data-placeholder)] empty:before:text-zinc-400 dark:empty:before:text-zinc-500"
          data-placeholder={placeholder}
          onFocus={onFocus}
          onBlur={(e) => {
            onCommit((e.target as HTMLSpanElement).textContent ?? "");
            onBlur();
          }}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              onCommit((e.target as HTMLSpanElement).textContent ?? "");
              onEnter();
            } else if (e.key === "Backspace" && (e.target as HTMLSpanElement).textContent === "") {
              e.preventDefault();
              onDeleteEmpty();
            }
          }}
        />
      </div>
      {/* URL display / edit */}
      {editingUrl && (
        <div className="flex items-center gap-1.5 mt-0.5 ml-5.5">
          <Input
            variant="inline"
            className="!text-left !w-full text-xs"
            value={url ?? ""}
            placeholder="https://..."
            onChange={(e) => onUrlChange(e.target.value || undefined)}
            onKeyDown={(e) => { if (e.key === "Enter") setEditingUrl(false); }}
            onBlur={() => setEditingUrl(false)}
            autoFocus
          />
        </div>
      )}
      {url && !editingUrl && (
        <a
          href={url}
          target="_blank"
          rel="noopener noreferrer"
          className="mt-0.5 ml-5.5 text-[11px] text-blue-400 hover:text-blue-300 hover:underline truncate block"
          title={url}
        >
          {url}
        </a>
      )}
      {/* Image display */}
      {image && (
        <div className="mt-0.5 ml-5.5">
          <button
            type="button"
            className="cursor-pointer"
            onClick={() => setImageExpanded(!imageExpanded)}
          >
            <img
              src={`data:${image.mimeType};base64,${image.data}`}
              alt={image.filename}
              className="rounded border border-zinc-200 dark:border-zinc-700 object-cover w-full"
              style={{ maxHeight: imageExpanded ? "none" : "80px" }}
            />
          </button>
          <span className="text-[10px] text-zinc-500 truncate block mt-0.5">{image.filename}</span>
        </div>
      )}
      {/* Hidden file input */}
      <input ref={fileInputRef} type="file" accept="image/*,.pdf,.doc,.docx" className="hidden" onChange={handleFileSelect} />
      {/* Overflow menu — hover to reveal trigger, click to open */}
      <div ref={menuRef} className="absolute top-1 right-0.5 opacity-0 group-hover:opacity-100 transition-opacity">
        <button type="button" className="cursor-pointer text-zinc-400 hover:text-zinc-600 dark:text-zinc-500 dark:hover:text-zinc-300 px-1" onClick={() => setMenuOpen(!menuOpen)}>
          <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor"><circle cx="8" cy="3" r="1.5"/><circle cx="8" cy="8" r="1.5"/><circle cx="8" cy="13" r="1.5"/></svg>
        </button>
        {menuOpen && (
          <div className="absolute right-0 top-full z-50 bg-white dark:bg-zinc-800 border border-zinc-200 dark:border-zinc-700 rounded-md shadow-lg py-1 min-w-[120px] text-[11px]">
            <button type="button" className="w-full text-left px-2.5 py-1 text-zinc-600 dark:text-zinc-300 hover:bg-zinc-100 dark:hover:bg-zinc-700 cursor-pointer" onClick={() => { setMenuOpen(false); setEditingUrl(!editingUrl); }}>
              {url ? "Edit link" : "Add link"}
            </button>
            {url && (
              <button type="button" className="w-full text-left px-2.5 py-1 text-zinc-600 dark:text-zinc-300 hover:bg-zinc-100 dark:hover:bg-zinc-700 cursor-pointer" onClick={() => { setMenuOpen(false); onUrlChange(undefined); setEditingUrl(false); }}>
                Remove link
              </button>
            )}
            <button type="button" className="w-full text-left px-2.5 py-1 text-zinc-600 dark:text-zinc-300 hover:bg-zinc-100 dark:hover:bg-zinc-700 cursor-pointer" onClick={() => { setMenuOpen(false); image ? onImageChange(undefined) : fileInputRef.current?.click(); }}>
              {image ? "Remove image" : "Add image"}
            </button>
            <div className="border-t border-zinc-200 dark:border-zinc-700 my-1" />
            <button type="button" className="w-full text-left px-2.5 py-1 text-red-500 hover:bg-zinc-100 dark:hover:bg-zinc-700 cursor-pointer" onClick={() => { setMenuOpen(false); onRemove(); }}>
              Remove
            </button>
          </div>
        )}
      </div>
    </div>
  );
}

function ContractList({ items, onChange, placeholder, showToggle = true }: { items: ContractItem[]; onChange: (items: ContractItem[]) => void; placeholder?: string; showToggle?: boolean }) {
  const [focusedIndex, setFocusedIndex] = useState<number | null>(null);

  /** Build a full ContractItem preserving all fields, with overrides */
  const patchItem = (i: number, patch: Partial<{ text: string; passed: boolean | undefined; url: string | undefined; image: ContractImage | undefined }>) => {
    const item = items[i];
    const base = typeof item === "string" ? { text: item } : { ...item };
    const merged = { ...base, ...patch };
    onChange(items.map((x, j) => j === i ? merged : x));
  };

  const togglePassed = (i: number) => {
    const current = contractPassed(items[i]);
    const next = current === undefined ? true : current === true ? false : undefined;
    patchItem(i, { passed: next });
  };

  return (
    <div className="flex flex-col">
      {items.map((item, i) => (
        <ContractBulletItem
          key={i}
          item={item}
          focused={focusedIndex === i}
          placeholder={placeholder}
          onCommit={(v) => {
            if (v !== contractText(item)) patchItem(i, { text: v });
          }}
          onFocus={() => setFocusedIndex(i)}
          onBlur={() => setFocusedIndex(null)}
          onEnter={() => onChange([...items.slice(0, i + 1), { text: "" }, ...items.slice(i + 1)])}
          onDeleteEmpty={() => onChange(items.filter((_, j) => j !== i))}
          onRemove={() => onChange(items.filter((_, j) => j !== i))}
          onToggle={showToggle ? () => togglePassed(i) : undefined}
          onUrlChange={(url) => patchItem(i, { url })}
          onImageChange={(image) => patchItem(i, { image })}
        />
      ))}
      {/* Ghost item */}
      <div className="flex items-start gap-1.5 min-h-[22px] px-1.5 py-1 rounded opacity-40 hover:opacity-70 focus-within:opacity-100 transition-opacity">
        {showToggle ? (
          <span className="shrink-0 w-3.5 h-3.5 rounded-full border border-zinc-300 dark:border-zinc-600 mt-[3px]" />
        ) : (
          <span className="shrink-0 w-1.5 h-1.5 rounded-full bg-zinc-400 dark:bg-zinc-500 mt-[7px]" />
        )}
        <span
          contentEditable
          suppressContentEditableWarning
          className="flex-1 min-w-0 text-xs text-zinc-700 dark:text-zinc-200 caret-current outline-none leading-relaxed break-words empty:before:content-[attr(data-placeholder)] empty:before:text-zinc-400 dark:empty:before:text-zinc-500"
          data-placeholder={placeholder ?? "Add..."}
          onInput={(e) => {
            const text = (e.target as HTMLSpanElement).textContent ?? "";
            if (text) {
              onChange([...items, { text }]);
              (e.target as HTMLSpanElement).textContent = "";
              setFocusedIndex(items.length);
            }
          }}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              const text = (e.target as HTMLSpanElement).textContent ?? "";
              if (text) {
                onChange([...items, { text }]);
                (e.target as HTMLSpanElement).textContent = "";
                setFocusedIndex(items.length);
              }
            }
          }}
        />
      </div>
    </div>
  );
}

function ContractTabBadge({ contract }: { contract?: Contract }) {
  if (!contract) return null;
  const expectItems = contract.expect ?? [];
  const totalItems = expectItems.length + (contract.ask?.length ?? 0) + (contract.never?.length ?? 0);
  if (totalItems === 0) return null;
  let passed = 0, failed = 0;
  for (const item of expectItems) {
    const p = contractPassed(item);
    if (p === true) passed++;
    else if (p === false) failed++;
  }
  const color = failed > 0
    ? "bg-red-500"
    : expectItems.length > 0 && passed === expectItems.length
      ? "bg-emerald-500"
      : "bg-zinc-300 dark:bg-zinc-600";
  return (
    <span className={`ml-1 inline-block w-1.5 h-1.5 rounded-full ${color}`} />
  );
}

function NodeContractContent({ node }: { node: C4Node }) {
  const updateNodeData = useUpdateNodeData();
  const data = node.data as C4NodeData;
  const raw = data.contract;
  const contract = { expect: raw?.expect ?? [], ask: raw?.ask ?? [], never: raw?.never ?? [] };

  const updateField = (field: keyof Contract, items: ContractItem[]) => {
    updateNodeData(node.id, { contract: { ...contract, [field]: items } });
  };

  return (
    <>
      <Section title="Expected">
        <ContractList
          items={contract.expect}
          onChange={(items) => updateField("expect", items)}
          placeholder="Expected to..."
        />
      </Section>
      <Divider />
      <Section title="Ask first">
        <ContractList
          items={contract.ask}
          onChange={(items) => updateField("ask", items)}
          placeholder="Confirm before..."
          showToggle={false}
        />
      </Section>
      <Divider />
      <Section title="Never">
        <ContractList
          items={contract.never}
          onChange={(items) => updateField("never", items)}
          placeholder="Must never..."
          showToggle={false}
        />
      </Section>
    </>
  );
}


/* ── Process tab content ──────────────────────────────────────────── */

function ProcessPropertiesContent({ node, mentionNames }: { node: C4Node; mentionNames: MentionItem[] }) {
  const updateNodeData = useUpdateNodeData();
  const data = node.data as C4NodeData;

  return (
    <>
      <Input
        variant="title"
        className="font-mono"
        value={data.name}
        placeholder="e.g. User Registration"
        onChange={(e) => updateNodeData(node.id, { name: e.target.value })}
      />
      <Divider />
      <Field
        label="Description"
        trailing={
          <span className={`text-[10px] tabular-nums ${data.description.length > 360 ? "text-amber-500" : "text-zinc-400/50 dark:text-zinc-500/50"}`}>
            {data.description.length}/400
          </span>
        }
      >
        <MentionTextarea
          value={data.description}
          onChange={(val) => updateNodeData(node.id, { description: val })}
          mentionNames={mentionNames}
          maxLength={400}
          rows={8}
          className="w-full rounded-md border border-zinc-200 dark:border-transparent bg-zinc-100/60 dark:bg-zinc-800/60 focus:bg-zinc-100 dark:focus:bg-zinc-700/60 px-2.5 py-2 text-xs text-zinc-700 dark:text-zinc-200 leading-relaxed outline-none resize-none transition-colors placeholder:text-zinc-400 dark:placeholder:text-zinc-500"
          placeholder="What does this process do? Use @[Name] to reference operations or other processes."
        />
      </Field>
      <Divider />
      <Field label="Status">
        <StatusBar value={data.status} onChange={(s) => updateNodeData(node.id, { status: s, statusReason: undefined })} />
        {data.statusReason && (
          <p className="mt-1 text-[11px] text-zinc-500 dark:text-zinc-400 leading-relaxed italic">{data.statusReason}</p>
        )}
      </Field>
    </>
  );
}

/* ── Model tab content ────────────────────────────────────────────── */

function ModelPropertiesContent({ node }: { node: C4Node }) {
  const updateNodeData = useUpdateNodeData();
  const data = node.data as C4NodeData;
  const properties = data.properties ?? [];

  const updateProperty = (index: number, updates: Partial<ModelProperty>) => {
    const next = properties.map((p: ModelProperty, i: number) => (i === index ? { ...p, ...updates } : p));
    updateNodeData(node.id, { properties: next });
  };

  const addProperty = () => {
    updateNodeData(node.id, { properties: [...properties, { label: "", description: "" }] });
  };

  const removeProperty = (index: number) => {
    updateNodeData(node.id, { properties: properties.filter((_: ModelProperty, i: number) => i !== index) });
  };

  return (
    <>
      <Input
        variant="title"
        className="font-mono"
        value={data.name}
        placeholder="e.g. UserProfile"
        onChange={(e) => updateNodeData(node.id, { name: sanitizeTypeName(e.target.value) })}
      />
      <Divider />
      <Field
        label="Description"
        trailing={
          <span className={`text-[10px] tabular-nums ${data.description.length > 180 ? "text-amber-500" : "text-zinc-400/50 dark:text-zinc-500/50"}`}>
            {data.description.length}/200
          </span>
        }
      >
        <Textarea
          rows={5}
          maxLength={200}
          value={data.description}
          placeholder="What does this model represent?"
          onChange={(e) => updateNodeData(node.id, { description: e.target.value })}
        />
      </Field>
      <Divider />
      <Section title="Properties" count={properties.length}>
        {properties.length > 0 && (
          <div className="flex flex-col gap-1">
            {properties.map((prop: ModelProperty, i: number) => (
              <div key={i} className="group flex items-start gap-1.5 border-l-2 border-zinc-200 dark:border-zinc-700 pl-2 py-0.5">
                <div className="flex-1 min-w-0 flex flex-col">
                  <Input
                    variant="inline"
                    className="!text-left !w-full !bg-transparent font-semibold font-mono text-xs"
                    value={prop.label}
                    placeholder="propertyName"
                    onChange={(e) => updateProperty(i, { label: sanitizeIdentifier(e.target.value) })}
                  />
                  <Input
                    variant="inline"
                    className="!text-left !w-full !bg-transparent text-[11px] !text-zinc-400 dark:!text-zinc-500"
                    value={prop.description}
                    placeholder="Description"
                    onChange={(e) => updateProperty(i, { description: e.target.value })}
                  />
                </div>
                <button
                  type="button"
                  className="shrink-0 mt-1 text-xs text-zinc-300 hover:text-red-400 dark:text-zinc-600 dark:hover:text-red-400 cursor-pointer opacity-0 group-hover:opacity-100"
                  title="Remove property"
                  onClick={() => removeProperty(i)}
                >
                  &times;
                </button>
              </div>
            ))}
          </div>
        )}
        <Button variant="link" onClick={addProperty}>
          + add property
        </Button>
      </Section>
      <Divider />
      <Field label="Status">
        <StatusBar value={data.status} onChange={(s) => updateNodeData(node.id, { status: s, statusReason: undefined })} />
        {data.statusReason && (
          <p className="mt-1 text-[11px] text-zinc-500 dark:text-zinc-400 leading-relaxed italic">{data.statusReason}</p>
        )}
      </Field>
    </>
  );
}

/* ── Tab builders ─────────────────────────────────────────────────── */

function getNodeTabs(
  node: C4Node,
  sourceLocations: SourceLocation[] | undefined,
  projectPath: string | undefined,
  onUpdateOperationData?: (fnId: string, data: Record<string, unknown>) => void,
  mentionNames?: MentionItem[],
): PanelTab[] {
  const tabs: PanelTab[] = [
    {
      id: "properties",
      label: "Properties",
      content: (
        <NodePropertiesContent
          node={node}
          sourceLocations={sourceLocations}
          projectPath={projectPath}
          mentionNames={mentionNames}
        />
      ),
    },
  ];

  if (node.data.kind === "component") {
    tabs.push({
      id: "members",
      label: "Members",
      content: <NodeChildrenContent node={node} onUpdateOperationData={onUpdateOperationData} />,
    });
  }

  if (node.data.kind !== "person" && !node.data.external) {
    tabs.push({
      id: "contract",
      label: "Contract",
      badge: <ContractTabBadge contract={(node.data as C4NodeData).contract as Contract | undefined} />,
      content: <NodeContractContent node={node} />,
    });
  }

  return tabs;
}

function getProcessTabs(
  node: C4Node,
  mentionNames: { name: string; kind: "operation" | "process" | "model"; ref?: boolean }[],
): PanelTab[] {
  return [
    {
      id: "properties",
      label: "Properties",
      content: <ProcessPropertiesContent node={node} mentionNames={mentionNames} />,
    },
    {
      id: "contract",
      label: "Contract",
      badge: <ContractTabBadge contract={(node.data as C4NodeData).contract as Contract | undefined} />,
      content: <NodeContractContent node={node} />,
    },
  ];
}

function getModelTabs(
  node: C4Node,
): PanelTab[] {
  return [
    {
      id: "properties",
      label: "Properties",
      content: <ModelPropertiesContent node={node} />,
    },
    {
      id: "contract",
      label: "Contract",
      badge: <ContractTabBadge contract={(node.data as C4NodeData).contract as Contract | undefined} />,
      content: <NodeContractContent node={node} />,
    },
  ];
}

/* ── Horizontal tab bar ───────────────────────────────────────────── */

function TabBar({ tabs, activeTab, onTabClick }: { tabs: PanelTab[]; activeTab: string; onTabClick: (id: string) => void }) {
  return (
    <div className="flex shrink-0 gap-0.5 px-2 py-1.5 border-b border-zinc-200 dark:border-zinc-700">
      {tabs.map((tab) => (
        <button
          key={tab.id}
          type="button"
          className={`px-1.5 py-0.5 text-[10px] uppercase tracking-wide font-semibold cursor-pointer transition-colors rounded ${
            activeTab === tab.id
              ? "text-zinc-700 bg-zinc-200 dark:text-zinc-200 dark:bg-zinc-700"
              : "text-zinc-500 hover:text-zinc-600 hover:bg-zinc-100 dark:text-zinc-500 dark:hover:text-zinc-300 dark:hover:bg-zinc-800"
          }`}
          onClick={() => onTabClick(tab.id)}
        >
          {tab.label}
          {tab.badge}
        </button>
      ))}
    </div>
  );
}

/* ── Panel shell ──────────────────────────────────────────────────── */

const panelBase = "shrink-0 border-l border-zinc-200 bg-zinc-50 dark:border-zinc-700 dark:bg-zinc-900";

/* ── Main panel ───────────────────────────────────────────────────── */

export function PropertiesPanel({ node, edge, onUpdateEdge, codeLevel, hints, onFixHint, onDismissHint, sourceLocations, projectPath, groups, onUpdateGroups, allNodes, onUpdateOperationData, processMentionNames, multiSelected, totalSelected, canGroup, groupKind, onCreateGroup, onAddToGroup, activeFlow }: {
  node: C4Node | null;
  edge: C4Edge | null;
  onUpdateEdge: (id: string, data: { label?: string; method?: string }) => void;
  codeLevel?: boolean;
  hints?: Hint[];
  onFixHint?: (hint: Hint) => void;
  onDismissHint?: (hint: Hint) => void;
  sourceLocations?: SourceLocation[];
  projectPath?: string;
  groups?: Group[];
  onUpdateGroups?: (fn: (prev: Group[]) => Group[]) => void;
  allNodes?: C4Node[];
  onUpdateOperationData?: (fnId: string, data: Record<string, unknown>) => void;
  processMentionNames?: { name: string; kind: "operation" | "process" | "model"; ref?: boolean }[];
  multiSelected?: string[];
  totalSelected?: number;
  canGroup?: boolean;
  groupKind?: "deployment" | "package";
  onCreateGroup?: (name: string, memberIds: string[]) => void;
  onAddToGroup?: (groupId: string, memberIds: string[]) => void;
  activeFlow?: Flow | null;
}) {
  const [activeTab, setActiveTab] = useState<string>("properties");

  const selectionId = node?.id ?? edge?.id ?? null;

  useEffect(() => {
    if (selectionId) {
      setActiveTab("properties");
    }
  }, [selectionId]);

  // Flow mode: no properties panel (step editing is inline)
  if (activeFlow) return null;

  // Multi-selection: never show properties panel. Show group panel only for 2+ nodes.
  if ((totalSelected ?? 0) >= 2) {
    if (multiSelected && multiSelected.length >= 2 && canGroup && onCreateGroup && onAddToGroup) {
      return (
        <div className={`${panelBase} w-80 flex flex-col`}>
          <div className="flex-1 min-h-0 p-4 flex flex-col overflow-y-auto">
            <MultiSelectionPanel
              selectedIds={multiSelected}
              groups={groups ?? []}
              groupKind={groupKind}
              onCreateGroup={onCreateGroup}
              onAddToGroup={onAddToGroup}
            />
          </div>
        </div>
      );
    }
    return null;
  }

  if (!node && !edge) return null;

  const isGroup = node?.type === "groupBox";
  const isProcess = node?.data?.kind === "process";
  const isModel = node?.data?.kind === "model";

  // Build tabs
  let tabs: PanelTab[] | null = null;
  if (isGroup && node && groups && onUpdateGroups) {
    tabs = [
      {
        id: "properties",
        label: "Properties",
        content: <GroupPropertiesContent node={node} groups={groups} onUpdateGroups={onUpdateGroups} allNodes={allNodes ?? []} />,
      },
      {
        id: "contract",
        label: "Contract",
        badge: <ContractTabBadge contract={groups.find((g) => g.id === node.id)?.contract} />,
        content: <GroupContractContent node={node} groups={groups} onUpdateGroups={onUpdateGroups} />,
      },
    ];
  } else if (isModel && node) {
    tabs = getModelTabs(node);
  } else if (isProcess && node) {
    tabs = getProcessTabs(node, processMentionNames ?? []);
  } else if (node && !isProcess && !isModel) {
    tabs = getNodeTabs(
      node,
      sourceLocations,
      projectPath,
      onUpdateOperationData,
      processMentionNames,
    );
  }

  const hasTabs = tabs != null && tabs.length > 1;

  // ── Tabbed panel ──
  if (hasTabs && tabs) {
    const activeTabObj = tabs.find((t) => t.id === activeTab) ?? tabs[0];
    return (
      <div className={`${panelBase} w-80 flex flex-col overflow-hidden`}>
        <TabBar
          tabs={tabs}
          activeTab={activeTabObj.id}
          onTabClick={(id) => setActiveTab(id)}
        />
        <div className="flex-1 min-h-0 p-4 flex flex-col overflow-y-auto">
          {activeTabObj.content}
        </div>
        <HintsFooter
          hints={hints ?? []}
          onFixHint={onFixHint ?? (() => {})}
          onDismissHint={onDismissHint ?? (() => {})}
        />
      </div>
    );
  }

  // ── Non-tabbed panel ──
  let content: ReactNode = null;
  if (!node && edge) {
    content = <EdgePanel edge={edge} onUpdate={(data) => onUpdateEdge(edge.id, data)} codeLevel={codeLevel} />;
  } else if (tabs && tabs.length === 1) {
    content = tabs[0].content;
  }

  return (
    <div className={`${panelBase} w-80 flex flex-col`}>
      <div className="flex-1 min-h-0 p-4 flex flex-col overflow-y-auto">
        {content}
      </div>
      <HintsFooter
        hints={hints ?? []}
        onFixHint={onFixHint ?? (() => {})}
        onDismissHint={onDismissHint ?? (() => {})}
      />
    </div>
  );
}

function HintsFooter({ hints, onFixHint, onDismissHint }: { hints: Hint[]; onFixHint: (hint: Hint) => void; onDismissHint: (hint: Hint) => void }) {
  if (hints.length === 0) return null;
  return (
    <div className="shrink-0 border-t border-zinc-200 dark:border-zinc-700 max-h-[40%] overflow-y-auto">
      <div className="px-4 py-2 flex flex-col gap-1.5">
        <Section title="Hints" count={hints.length}>
          <div className="flex flex-col gap-1.5">
            {hints.map((hint, i) => (
              <div
                key={i}
                className={`rounded-md border px-2 py-1.5 text-xs leading-relaxed ${
                  hint.severity === "warning"
                    ? "border-orange-200 bg-orange-50 text-orange-700 dark:border-orange-800 dark:bg-orange-950/40 dark:text-orange-300"
                    : "border-teal-200 bg-teal-50 text-teal-700 dark:border-teal-800 dark:bg-teal-950/40 dark:text-teal-300"
                }`}
              >
                <div className="flex justify-between items-start gap-1">
                  <span>{hint.message}</span>
                  <button
                    type="button"
                    className="shrink-0 text-xs opacity-40 hover:opacity-80 cursor-pointer leading-none"
                    onClick={() => onDismissHint(hint)}
                    title="Dismiss"
                  >
                    &times;
                  </button>
                </div>
                {hint.action && (
                  <button
                    type="button"
                    className="mt-1 block rounded bg-white/70 px-1.5 py-0.5 text-[11px] font-medium hover:bg-white cursor-pointer dark:bg-zinc-800/70 dark:hover:bg-zinc-800"
                    onClick={() => onFixHint(hint)}
                  >
                    Fix
                  </button>
                )}
              </div>
            ))}
          </div>
        </Section>
      </div>
    </div>
  );
}

function NotesList({ items, onChange }: { items: string[]; onChange: (items: string[]) => void }) {
  const [focusIdx, setFocusIdx] = useState<number | null>(null);
  const itemRefs = useRef<(HTMLSpanElement | null)[]>([]);

  // Focus newly added item
  useEffect(() => {
    if (focusIdx !== null && itemRefs.current[focusIdx]) {
      const el = itemRefs.current[focusIdx];
      el?.focus();
      // Place cursor at end
      const range = document.createRange();
      range.selectNodeContents(el!);
      range.collapse(false);
      const sel = window.getSelection();
      sel?.removeAllRanges();
      sel?.addRange(range);
      setFocusIdx(null);
    }
  }, [focusIdx, items.length]);

  const commit = (i: number, text: string) => {
    if (text !== items[i]) {
      onChange(items.map((x, j) => j === i ? text : x));
    }
  };

  const bullet = <span className="shrink-0 w-1.5 h-1.5 rounded-full bg-zinc-400 dark:bg-zinc-500 mt-[7px]" />;

  return (
    <div className="flex flex-col">
      {items.map((item, i) => (
        <div key={i} className="group flex items-start gap-1.5 min-h-[22px] px-1.5 py-1 rounded hover:bg-zinc-50 dark:hover:bg-zinc-800/50">
          {bullet}
          <span
            ref={(el) => { itemRefs.current[i] = el; }}
            contentEditable
            suppressContentEditableWarning
            className="flex-1 min-w-0 text-xs text-zinc-600 dark:text-zinc-300 caret-current outline-none leading-relaxed break-words"
            onBlur={(e) => commit(i, (e.target as HTMLSpanElement).textContent ?? "")}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                commit(i, (e.target as HTMLSpanElement).textContent ?? "");
                onChange([...items.slice(0, i + 1), "", ...items.slice(i + 1)]);
                setFocusIdx(i + 1);
              }
              if (e.key === "Backspace" && !(e.target as HTMLSpanElement).textContent) {
                e.preventDefault();
                onChange(items.filter((_, j) => j !== i));
                if (i > 0) setFocusIdx(i - 1);
              }
            }}
          >
            {item}
          </span>
          <button
            type="button"
            className="shrink-0 opacity-0 group-hover:opacity-100 text-zinc-400 hover:text-red-400 text-xs cursor-pointer transition-opacity mt-0.5"
            onClick={() => onChange(items.filter((_, j) => j !== i))}
          >
            &times;
          </button>
        </div>
      ))}
      {/* Ghost item */}
      <div className="flex items-start gap-1.5 min-h-[22px] px-1.5 py-1 rounded opacity-40 hover:opacity-70 focus-within:opacity-100 transition-opacity">
        {bullet}
        <span
          contentEditable
          suppressContentEditableWarning
          className="flex-1 min-w-0 text-xs text-zinc-400 dark:text-zinc-500 caret-current outline-none leading-relaxed break-words empty:before:content-[attr(data-placeholder)] empty:before:text-zinc-400 dark:empty:before:text-zinc-500"
          data-placeholder="Add note..."
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              const text = (e.target as HTMLSpanElement).textContent ?? "";
              if (text) {
                onChange([...items, text]);
                (e.target as HTMLSpanElement).textContent = "";
                setFocusIdx(items.length);
              }
            }
          }}
          onInput={(e) => {
            const text = (e.target as HTMLSpanElement).textContent ?? "";
            if (text && text.includes("\n")) {
              onChange([...items, text.replace(/\n/g, "")]);
              (e.target as HTMLSpanElement).textContent = "";
            }
          }}
        />
      </div>
    </div>
  );
}
