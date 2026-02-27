import { useState, useEffect, useRef, useCallback, type ReactNode } from "react";
import { useReactFlow } from "@xyflow/react";
import { invoke } from "@tauri-apps/api/core";
import type { C4Node, C4NodeData, C4Edge, Hint, Status, SourceLocation, ModelProperty, Group, Contract, Flow, Attachment } from "./types";
import { ShapeIcon, resolveShape, defaultShapeForKind, ALL_SHAPES, SHAPE_LABELS } from "./shapes";
import { STATUS_COLORS } from "./statusColors";
import { Button, Input, Textarea, Section, Divider, KVRow, Toggle, PillToggle, Field } from "./ui";
import { MentionTextarea, type MentionItem } from "./MentionTextarea";

/** Sanitize to camelCase / snake_case: only [a-zA-Z0-9_], first char must be lowercase letter */
function sanitizeIdentifier(raw: string): string {
  const stripped = raw.replace(/[^a-zA-Z0-9_]/g, "");
  if (stripped.length === 0) return "";
  // Force first character to lowercase; drop if not a letter
  const first = stripped[0];
  if (/[a-zA-Z]/.test(first)) return first.toLowerCase() + stripped.slice(1);
  return stripped.slice(1); // strip leading digits/underscores
}

/* ── Tab type ─────────────────────────────────────────────────────── */

type PanelTab = { id: string; label: string; content: ReactNode };

/* ── Constants ────────────────────────────────────────────────────── */

const TECHNOLOGY_SUGGESTIONS: Record<string, string[]> = {
  container: ["TypeScript", "Rust", "Python", "Go", "Java", "C#", "PostgreSQL", "Redis"],
  component: ["React", "Next.js", "Express", "Spring", "Django", "Axum", "GraphQL", "gRPC"],
};

/* ── Sub-panels (edge, group — no tabs) ──────────────────────────── */

function GroupPanel({ node, groups, onUpdateGroups, allNodes }: { node: C4Node; groups: Group[]; onUpdateGroups: (fn: (prev: Group[]) => Group[]) => void; allNodes: C4Node[] }) {
  const group = groups.find((g) => g.id === node.id);
  if (!group) return null;

  const removeMember = (memberId: string) => {
    onUpdateGroups((prev) => {
      const updated = prev.map((g) =>
        g.id === group.id ? { ...g, memberIds: g.memberIds.filter((id) => id !== memberId) } : g,
      );
      // Auto-delete empty groups
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

/* ── Attachments section ──────────────────────────────────────────── */

const MAX_IMAGE_DIM = 1280;

function resizeImage(file: File): Promise<{ base64: string; mimeType: string }> {
  return new Promise((resolve) => {
    const img = new Image();
    img.onload = () => {
      let { width, height } = img;
      if (width <= MAX_IMAGE_DIM && height <= MAX_IMAGE_DIM) {
        // Small enough — read as-is
        const reader = new FileReader();
        reader.onload = () => {
          const b64 = (reader.result as string).split(",")[1];
          if (b64) resolve({ base64: b64, mimeType: file.type || "image/png" });
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
      resolve({ base64: dataUrl.split(",")[1]!, mimeType: "image/jpeg" });
    };
    img.src = URL.createObjectURL(file);
  });
}

function AttachmentsSection({ nodeId, attachments }: { nodeId: string; attachments: Attachment[] }) {
  const { updateNodeData } = useReactFlow();
  const fileInputRef = useRef<HTMLInputElement>(null);
  const [expandedId, setExpandedId] = useState<string | null>(null);

  const handleFileSelect = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;
    resizeImage(file).then(({ base64, mimeType }) => {
      const attachment: Attachment = {
        id: crypto.randomUUID(),
        filename: file.name,
        mimeType,
        data: base64,
      };
      updateNodeData(nodeId, { attachments: [...attachments, attachment] });
    });
    // Reset so the same file can be re-selected
    e.target.value = "";
  }, [nodeId, attachments, updateNodeData]);

  const removeAttachment = useCallback((id: string) => {
    updateNodeData(nodeId, { attachments: attachments.filter((a) => a.id !== id) });
    if (expandedId === id) setExpandedId(null);
  }, [nodeId, attachments, updateNodeData, expandedId]);

  return (
    <Section title="Attachments" count={attachments.length || undefined}>
      {attachments.length > 0 && (
        <div className="flex flex-col gap-2">
          {attachments.map((att) => (
            <div key={att.id} className="group">
              <div className="flex items-center gap-1.5">
                <button
                  type="button"
                  className="flex-1 min-w-0 cursor-pointer"
                  onClick={() => setExpandedId(expandedId === att.id ? null : att.id)}
                >
                  <img
                    src={`data:${att.mimeType};base64,${att.data}`}
                    alt={att.filename}
                    className="w-full rounded border border-zinc-200 dark:border-zinc-700 object-cover"
                    style={{ maxHeight: expandedId === att.id ? "none" : "80px" }}
                  />
                </button>
                <button
                  type="button"
                  className="shrink-0 text-xs text-zinc-300 hover:text-red-400 dark:text-zinc-600 dark:hover:text-red-400 cursor-pointer opacity-0 group-hover:opacity-100"
                  title="Remove attachment"
                  onClick={() => removeAttachment(att.id)}
                >
                  &times;
                </button>
              </div>
              <span className="text-[10px] text-zinc-500 dark:text-zinc-500 truncate block mt-0.5">{att.filename}</span>
            </div>
          ))}
        </div>
      )}
      <input
        ref={fileInputRef}
        type="file"
        accept="image/*"
        className="hidden"
        onChange={handleFileSelect}
      />
      <Button variant="link" onClick={() => fileInputRef.current?.click()}>
        + add image
      </Button>
    </Section>
  );
}

/* ── Node tab content ─────────────────────────────────────────────── */

function NodePropertiesContent({ node, hints, onFixHint, onDismissHint, sourceLocations, projectPath, mentionNames }: { node: C4Node; hints: Hint[]; onFixHint: (hint: Hint) => void; onDismissHint: (hint: Hint) => void; sourceLocations?: SourceLocation[]; projectPath?: string; mentionNames?: MentionItem[] }) {
  const { updateNodeData } = useReactFlow();
  const { data } = node;
  const isCodeLevel = data.kind === "operation";
  const showTechnology = data.kind === "container" || data.kind === "component";
  const showShape = data.kind !== "person" && !isCodeLevel;
  const suggestions = TECHNOLOGY_SUGGESTIONS[data.kind] ?? [];
  const listId = `panel-tech-${node.id}`;

  return (
    <>
      {/* ── Name ── */}
      <Input
        variant="title"
        value={data.name}
        placeholder={isCodeLevel ? "e.g. handleLogin" : "Name..."}
        className={isCodeLevel ? "font-mono" : undefined}
        onChange={(e) => updateNodeData(node.id, { name: isCodeLevel ? sanitizeIdentifier(e.target.value) : e.target.value })}
      />

      {/* ── Identity — external toggle for systems ── */}
      {data.kind === "system" && (
        <>
          <Divider />
          <Section title="Identity">
            <KVRow label="External">
              <Toggle value={!!data.external} onChange={(v) => updateNodeData(node.id, { external: v || undefined, ...(v ? { status: undefined } : {}) })} />
            </KVRow>
          </Section>
        </>
      )}

      <Divider />

      {/* ── Description — prominent, right after identity ── */}
      <Field
        label="Description"
        trailing={!isCodeLevel ? (
          <span className={`text-[10px] tabular-nums ${data.description.length > 180 ? "text-amber-500" : "text-zinc-400/50 dark:text-zinc-500/50"}`}>
            {data.description.length}/200
          </span>
        ) : undefined}
      >
        {isCodeLevel && mentionNames ? (
          <MentionTextarea
            value={data.description}
            onChange={(val) => updateNodeData(node.id, { description: val })}
            mentionNames={mentionNames}
            rows={5}
            className="w-full rounded-md border border-zinc-200 dark:border-transparent bg-zinc-100/60 dark:bg-zinc-800/60 focus:bg-zinc-100 dark:focus:bg-zinc-700/60 px-2.5 py-2 text-xs text-zinc-700 dark:text-zinc-200 leading-relaxed outline-none resize-none transition-colors placeholder:text-zinc-400 dark:placeholder:text-zinc-500"
            placeholder="Describe this operation... Use @[Name] to reference sibling operations, processes, or models."
          />
        ) : (
          <Textarea
            rows={7}
            maxLength={200}
            value={data.description}
            placeholder="Describe this node..."
            onChange={(e) => updateNodeData(node.id, { description: e.target.value })}
          />
        )}
      </Field>

      {/* ── Decisions — rationale for why this node exists ── */}
      {!isCodeLevel && (
        <>
          <Divider />
          <Field label="Decision record">
            <Textarea
              rows={4}
              value={data.decisions ?? ""}
              placeholder="Why this node exists or is structured this way..."
              onChange={(e) => updateNodeData(node.id, { decisions: e.target.value || undefined })}
            />
          </Field>
        </>
      )}

      {/* ── Attachments (containers & components only) ── */}
      {(data.kind === "container" || data.kind === "component") && (
        <>
          <Divider />
          <AttachmentsSection nodeId={node.id} attachments={data.attachments ?? []} />
        </>
      )}

      {/* ── Details — technology ── */}
      {showTechnology && (
        <>
          <Divider />
          <Section title="Details">
            <KVRow label="Technology">
              <Input
                variant="inline"
                list={listId}
                value={data.technology ?? ""}
                placeholder="e.g. REST API"
                onChange={(e) => updateNodeData(node.id, { technology: e.target.value || undefined })}
              />
              <datalist id={listId}>
                {suggestions.map((s) => <option key={s} value={s} />)}
              </datalist>
            </KVRow>
          </Section>
        </>
      )}

      <Divider />

      {/* ── Implementation — status, source locations ── */}
      <Section title="Status">
        {data.kind !== "person" && !data.external && (
          <PillToggle<Status | undefined>
            options={[
              { value: undefined, label: "None" },
              { value: "implemented", label: "Implemented", variant: "success" },
              { value: "proposed", label: "Proposed", variant: "info" },
            { value: "changed", label: "Changed", variant: "warning" },
            ]}
            value={data.status}
            onChange={(s) => updateNodeData(node.id, { status: s })}
          />
        )}

        {isCodeLevel && sourceLocations && sourceLocations.length > 0 && (
          <div className="flex flex-col gap-0.5">
            {sourceLocations.map((loc, i) => (
              <button
                key={i}
                type="button"
                className="flex items-baseline gap-1 text-left text-xs text-blue-400 hover:text-blue-300 cursor-pointer truncate"
                title={loc.file + (loc.line ? `:${loc.line}` : "")}
                onClick={() => invoke("open_in_editor", { file: loc.file, line: loc.line, projectPath }).catch((e) => console.error("open_in_editor:", e))}
              >
                <span className="truncate">{loc.file}</span>
                {loc.line != null && <span className="shrink-0 text-zinc-500">:{loc.line}</span>}
              </button>
            ))}
          </div>
        )}

        {(data.kind === "person" || data.external) && !(isCodeLevel && sourceLocations && sourceLocations.length > 0) && (
          <span className="text-xs text-zinc-300 dark:text-zinc-600 italic">No implementation tracking</span>
        )}
      </Section>

      {/* ── Appearance — shape, collapsed by default ── */}
      {showShape && (
        <>
          <Divider />
          <Section title="Appearance">
            <div className="grid grid-cols-3 gap-1">
              {ALL_SHAPES.filter((s) => s !== "person").map((s) => {
                const effective = resolveShape(data.kind, data.shape);
                const isDefault = s === defaultShapeForKind(data.kind);
                return (
                  <div key={s} className="flex flex-col items-center gap-0.5">
                    <ShapeIcon
                      shape={s}
                      active={effective === s}
                      onClick={() =>
                        updateNodeData(node.id, { shape: isDefault ? undefined : s })
                      }
                    />
                    <span className="text-[10px] text-zinc-500 dark:text-zinc-500 leading-none">{SHAPE_LABELS[s]}</span>
                  </div>
                );
              })}
            </div>
          </Section>
        </>
      )}

      {/* ── Hints — open by default ── */}
      {hints.length > 0 && (
        <>
          <Divider />
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
              const sc = p.status ? STATUS_COLORS[p.status as keyof typeof STATUS_COLORS] : null;
              return (
              <div key={p.id} className="flex items-center gap-1.5 group rounded-md px-1.5 py-1 -mx-1.5 hover:bg-zinc-100/60 dark:hover:bg-zinc-800/60 transition-colors">
                <span
                  className="shrink-0 w-1.5 h-1.5 rounded-full"
                  style={{ backgroundColor: sc ? sc.hex : "#a1a1aa" }}
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
              const sc = m.status ? STATUS_COLORS[m.status as keyof typeof STATUS_COLORS] : null;
              return (
              <div key={m.id} className="flex items-center gap-1.5 group rounded-md px-1.5 py-1 -mx-1.5 hover:bg-zinc-100/60 dark:hover:bg-zinc-800/60 transition-colors">
                <span
                  className="shrink-0 w-1.5 h-1.5 rounded-full"
                  style={{ backgroundColor: sc ? sc.hex : "#a1a1aa" }}
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
              const sc = fn.status ? STATUS_COLORS[fn.status as keyof typeof STATUS_COLORS] : null;
              return (
                <div key={fn.id} className="flex items-center gap-1.5 group rounded-md px-1.5 py-1 -mx-1.5 hover:bg-zinc-100/60 dark:hover:bg-zinc-800/60 transition-colors">
                  <span className="shrink-0 w-1.5 h-1.5 rounded-full" style={{ backgroundColor: sc ? sc.hex : "#a1a1aa" }} />
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

/* ── String list editor (reusable) ────────────────────────────────── */

function BulletItem({ value, focused, placeholder, onCommit, onFocus, onBlur, onEnter, onDeleteEmpty, onRemove }: {
  value: string; focused: boolean; placeholder?: string;
  onCommit: (value: string) => void; onFocus: () => void; onBlur: () => void;
  onEnter: () => void; onDeleteEmpty: () => void; onRemove: () => void;
}) {
  const spanRef = useRef<HTMLSpanElement>(null);

  // Sync DOM text from props only when not focused (avoids cursor reset)
  useEffect(() => {
    if (!focused && spanRef.current && spanRef.current.textContent !== value) {
      spanRef.current.textContent = value;
    }
  }, [value, focused]);

  // Auto-focus when focused prop becomes true (e.g. after ghost promotion or Enter)
  useEffect(() => {
    if (focused && spanRef.current && document.activeElement !== spanRef.current) {
      const el = spanRef.current;
      el.focus();
      // Place cursor at end
      const range = document.createRange();
      range.selectNodeContents(el);
      range.collapse(false);
      const sel = window.getSelection();
      sel?.removeAllRanges();
      sel?.addRange(range);
    }
  }, [focused]);

  return (
    <div className={`flex items-baseline gap-1.5 group min-h-[22px] -mx-1.5 px-1.5 mb-1 last:mb-0 rounded ${focused ? "bg-zinc-100 dark:bg-zinc-800/80" : ""}`}>
      <span className="text-zinc-300 dark:text-zinc-600 text-xs select-none shrink-0">&bull;</span>
      <span
        ref={spanRef}
        contentEditable
        suppressContentEditableWarning
        className="flex-1 text-xs text-zinc-700 dark:text-zinc-200 caret-current outline-none leading-relaxed empty:before:content-[attr(data-placeholder)] empty:before:text-zinc-400 dark:empty:before:text-zinc-500"
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
      <button
        type="button"
        className="shrink-0 text-[10px] text-zinc-300 hover:text-red-400 dark:text-zinc-600 dark:hover:text-red-400 cursor-pointer opacity-0 group-hover:opacity-100"
        onClick={onRemove}
      >
        &times;
      </button>
    </div>
  );
}

function BulletList({ items, onChange, placeholder }: { items: string[]; onChange: (items: string[]) => void; placeholder?: string }) {
  const [focusedIndex, setFocusedIndex] = useState<number | null>(null);
  return (
    <div className="flex flex-col">
      {items.map((item, i) => (
        <BulletItem
          key={i}
          value={item}
          focused={focusedIndex === i}
          placeholder={placeholder}
          onCommit={(v) => { if (v !== item) onChange(items.map((x, j) => j === i ? v : x)); }}
          onFocus={() => setFocusedIndex(i)}
          onBlur={() => setFocusedIndex(null)}
          onEnter={() => onChange([...items.slice(0, i + 1), "", ...items.slice(i + 1)])}
          onDeleteEmpty={() => onChange(items.filter((_, j) => j !== i))}
          onRemove={() => onChange(items.filter((_, j) => j !== i))}
        />
      ))}
      {/* Ghost bullet — becomes a real item when typed into */}
      <div className="flex items-baseline gap-1.5 min-h-[22px] -mx-1.5 px-1.5 rounded opacity-40 hover:opacity-70 focus-within:opacity-100 transition-opacity">
        <span className="text-zinc-300 dark:text-zinc-600 text-xs select-none shrink-0">&bull;</span>
        <span
          contentEditable
          suppressContentEditableWarning
          className="flex-1 text-xs text-zinc-700 dark:text-zinc-200 caret-current outline-none leading-relaxed empty:before:content-[attr(data-placeholder)] empty:before:text-zinc-400 dark:empty:before:text-zinc-500"
          data-placeholder={placeholder ?? "Add..."}
          onInput={(e) => {
            const text = (e.target as HTMLSpanElement).textContent ?? "";
            if (text) {
              onChange([...items, text]);
              (e.target as HTMLSpanElement).textContent = "";
              setFocusedIndex(items.length); // focus the newly created real item
            }
          }}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              const text = (e.target as HTMLSpanElement).textContent ?? "";
              if (text) {
                onChange([...items, text]);
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

/* ── Node contract content (contract + accepts) ────────────────── */

function NodeContractContent({ node }: { node: C4Node }) {
  const { updateNodeData } = useReactFlow();
  const data = node.data as C4NodeData;
  const contract = data.contract ?? { expect: [], ask: [], never: [] };
  const accepts = data.accepts ?? [];

  const updateField = (field: keyof Contract, items: string[]) => {
    updateNodeData(node.id, { contract: { ...contract, [field]: items } });
  };

  return (
    <>
      <Section title="Expected">
        <BulletList
          items={contract.expect}
          onChange={(items) => updateField("expect", items)}
          placeholder="Expected to..."
        />
      </Section>
      <Divider />
      <Section title="Ask first">
        <BulletList
          items={contract.ask}
          onChange={(items) => updateField("ask", items)}
          placeholder="Confirm before..."
        />
      </Section>
      <Divider />
      <Section title="Never">
        <BulletList
          items={contract.never}
          onChange={(items) => updateField("never", items)}
          placeholder="Must never..."
        />
      </Section>
      <Divider />
      <Section title="Acceptance criteria">
        <BulletList
          items={accepts}
          onChange={(items) => updateNodeData(node.id, { accepts: items })}
        />
      </Section>
    </>
  );
}


/* ── Process tab content ──────────────────────────────────────────── */

function ProcessPropertiesContent({ node, mentionNames }: { node: C4Node; mentionNames: MentionItem[] }) {
  const { updateNodeData } = useReactFlow();
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
      <Field label="Description">
        <MentionTextarea
          value={data.description}
          onChange={(val) => updateNodeData(node.id, { description: val })}
          mentionNames={mentionNames}
          rows={5}
          className="w-full rounded-md border border-zinc-200 dark:border-transparent bg-zinc-100/60 dark:bg-zinc-800/60 focus:bg-zinc-100 dark:focus:bg-zinc-700/60 px-2.5 py-2 text-xs text-zinc-700 dark:text-zinc-200 leading-relaxed outline-none resize-none transition-colors placeholder:text-zinc-400 dark:placeholder:text-zinc-500"
          placeholder="What does this process do? Use @[Name] to reference operations or other processes."
        />
      </Field>
      <Divider />
      <Section title="Status">
        <PillToggle<Status | undefined>
          options={[
            { value: undefined, label: "None" },
            { value: "implemented", label: "Implemented", variant: "success" },
            { value: "proposed", label: "Proposed", variant: "info" },
            { value: "changed", label: "Changed", variant: "warning" },
          ]}
          value={data.status}
          onChange={(s) => updateNodeData(node.id, { status: s })}
        />
      </Section>
    </>
  );
}

/* ── Model tab content ────────────────────────────────────────────── */

function ModelPropertiesContent({ node }: { node: C4Node }) {
  const { updateNodeData } = useReactFlow();
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
        placeholder="e.g. userProfile"
        onChange={(e) => updateNodeData(node.id, { name: sanitizeIdentifier(e.target.value) })}
      />
      <Divider />
      <Field label="Description">
        <Textarea
          rows={5}
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
                    placeholder="Property name"
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
      <Section title="Status">
        <PillToggle<Status | undefined>
          options={[
            { value: undefined, label: "None" },
            { value: "implemented", label: "Implemented", variant: "success" },
            { value: "proposed", label: "Proposed", variant: "info" },
            { value: "changed", label: "Changed", variant: "warning" },
          ]}
          value={data.status}
          onChange={(s) => updateNodeData(node.id, { status: s })}
        />
      </Section>
    </>
  );
}

/* ── Tab builders ─────────────────────────────────────────────────── */

function getNodeTabs(
  node: C4Node,
  hints: Hint[],
  onFixHint: (hint: Hint) => void,
  onDismissHint: (hint: Hint) => void,
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
          hints={hints}
          onFixHint={onFixHint}
          onDismissHint={onDismissHint}
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

  if (node.data.kind === "system" || node.data.kind === "container" || node.data.kind === "component") {
    tabs.push({
      id: "contract",
      label: "Contract",
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
  ];
}

/* ── Horizontal tab bar ───────────────────────────────────────────── */

function TabBar({ tabs, activeTab, onTabClick }: { tabs: PanelTab[]; activeTab: string; onTabClick: (id: string) => void }) {
  return (
    <div className="flex shrink-0 gap-1 px-3 py-1.5 border-b border-zinc-200 dark:border-zinc-700">
      {tabs.map((tab) => (
        <button
          key={tab.id}
          type="button"
          className={`px-2 py-0.5 text-[10px] uppercase tracking-wider font-semibold cursor-pointer transition-colors rounded ${
            activeTab === tab.id
              ? "text-zinc-700 bg-zinc-200 dark:text-zinc-200 dark:bg-zinc-700"
              : "text-zinc-500 hover:text-zinc-600 hover:bg-zinc-100 dark:text-zinc-500 dark:hover:text-zinc-300 dark:hover:bg-zinc-800"
          }`}
          onClick={() => onTabClick(tab.id)}
        >
          {tab.label}
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
        <div className={`${panelBase} w-60 flex flex-col`}>
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
  if (isModel && node) {
    tabs = getModelTabs(node);
  } else if (isProcess && node) {
    tabs = getProcessTabs(node, processMentionNames ?? []);
  } else if (node && !isGroup && !isProcess && !isModel) {
    tabs = getNodeTabs(
      node,
      hints ?? [],
      onFixHint ?? (() => {}),
      onDismissHint ?? (() => {}),
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
      <div className={`${panelBase} w-60 flex flex-col`}>
        <TabBar
          tabs={tabs}
          activeTab={activeTabObj.id}
          onTabClick={(id) => setActiveTab(id)}
        />
        <div className="flex-1 min-h-0 p-4 flex flex-col overflow-y-auto">
          {activeTabObj.content}
        </div>
      </div>
    );
  }

  // ── Non-tabbed panel ──
  let content: ReactNode = null;
  if (isGroup && node && groups && onUpdateGroups) {
    content = <GroupPanel node={node} groups={groups} onUpdateGroups={onUpdateGroups} allNodes={allNodes ?? []} />;
  } else if (!node && edge) {
    content = <EdgePanel edge={edge} onUpdate={(data) => onUpdateEdge(edge.id, data)} codeLevel={codeLevel} />;
  } else if (tabs && tabs.length === 1) {
    content = tabs[0].content;
  }

  return (
    <div className={`${panelBase} w-60 flex flex-col`}>
      <div className="flex-1 min-h-0 p-4 flex flex-col overflow-y-auto">
        {content}
      </div>
    </div>
  );
}
