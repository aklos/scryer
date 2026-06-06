/**
 * The main panel: a read-first, wiki-style page for the selected node or group.
 *
 * Default view is clean reading — no edit chrome. Each section carries a small
 * [edit] link that swaps only that section into edit mode in place (no modals);
 * [done] returns to reading. New items land as `proposed`. The mapped source for
 * each responsibility is rendered inline as the read-through-to-code. Structured
 * metadata lives in the right infobox. Theme tokens are unchanged — this is a
 * layout/interaction shape, not a restyle.
 */

import { useEffect, useRef, useState } from "react";
import { Plus, Trash2, FolderOpen, X, Eye, RefreshCw, Send, Check, Undo2, Loader2 } from "lucide-react";
import type { ScryModel, Node, Group, Responsibility, SourceLocation } from "./viewmodel";
import { isDataShape } from "./viewmodel";
import type { Editor } from "./editor";
import type { Status } from "./statusColors";
import { STATUS_COLORS } from "./statusColors";
import { effectiveNodeStatus } from "./rollup";
import { rollupStatus } from "./statusColors";
import { kindIcon, typeTag } from "./kindIcon";
import { lookupIcon } from "./IconPicker";
import { CodeBlock } from "./CodeBlock";
import { Infobox } from "./Infobox";
import { Input, Textarea, Select, Button, type SelectOption } from "./ui";
import {
  EditLink,
  Empty,
  PageSection,
  StatusTag,
  USER_STATUSES,
  WikiLink,
} from "./pagekit";

export interface VariationState {
  nodeId: string;
  prompt: string;
  status: "generating" | "ready";
  count: number;
  selectedIdx: number | null;
}

export type Selected =
  | { kind: "node"; id: string }
  | { kind: "group"; id: string };

const STATUS_OPTIONS: SelectOption[] = USER_STATUSES.map((s) => ({
  value: s,
  label: STATUS_COLORS[s].label,
}));

/** Per-section edit toggles for one page. */
function useEditSections() {
  const [open, setOpen] = useState<ReadonlySet<string>>(() => new Set());
  return {
    isEditing: (key: string) => open.has(key),
    toggle: (key: string) =>
      setOpen((prev) => {
        const next = new Set(prev);
        if (next.has(key)) next.delete(key);
        else next.add(key);
        return next;
      }),
    reset: () => setOpen(new Set()),
  };
}

export function NodePage(props: {
  model: ScryModel;
  selected: Selected;
  projectPath: string | null;
  editor: Editor | undefined;
  onSelectNode: (id: string) => void;
  onSelectGroup: (id: string) => void;
  onRender?: (nodeId: string) => void;
  variationState: VariationState | null;
  onStartVariation?: (nodeId: string, prompt: string, count?: number, baseVariationIdx?: number) => void;
  onAcceptVariation?: (nodeId: string, variationIdx: number) => void;
  onDiscardVariations?: (nodeId: string) => void;
  onSelectVariation?: (idx: number | null) => void;
  newRespIds: ReadonlySet<string>;
  onClearNewResp: (id: string) => void;
}) {
  const { model, selected } = props;
  // Key the page on the selection so edit toggles reset when you navigate away.
  if (selected.kind === "group") {
    const group = model.groups.find((g) => g.id === selected.id);
    if (!group) return <Gone />;
    return <GroupPageBody key={group.id} {...props} group={group} />;
  }
  const node = model.nodes.find((n) => n.id === selected.id);
  if (!node) return <Gone />;
  return <NodePageBody key={node.id} {...props} node={node} />;
}

function Gone() {
  return (
    <div className="flex flex-1 items-center justify-center text-[12px] text-[var(--text-muted)]">
      That page no longer exists.
    </div>
  );
}

// --- header -----------------------------------------------------------------

function PageHeader({
  eyebrow,
  name,
  status,
  editor,
  editingName,
  onToggleName,
  onRename,
}: {
  eyebrow: React.ReactNode;
  name: string;
  status: Status | null;
  editor: Editor | undefined;
  editingName: boolean;
  onToggleName: () => void;
  onRename: (v: string) => void;
}) {
  return (
    <header className="shrink-0 border-b border-[var(--border-subtle)] px-6 pb-4 pt-5">
      <div className="flex items-center gap-1.5 text-[11px] font-medium text-[var(--text-tertiary)]">
        {eyebrow}
      </div>
      <div className="mt-1.5 flex items-baseline gap-3">
        <div className="min-w-0 flex-1">
          {editingName ? (
            <Input
              variant="title"
              autoFocus
              defaultValue={name}
              onBlur={(e) => onRename(e.currentTarget.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  onRename(e.currentTarget.value);
                  onToggleName();
                } else if (e.key === "Escape") {
                  onToggleName();
                }
              }}
              className="w-full text-[22px] font-semibold leading-tight"
            />
          ) : (
            <h1 className="truncate text-[22px] font-semibold leading-tight text-[var(--text)]">
              {name || "Untitled"}
            </h1>
          )}
        </div>
        <StatusTag status={status} />
        {editor && <EditLink editing={editingName} onClick={onToggleName} />}
      </div>
    </header>
  );
}

// --- node page --------------------------------------------------------------

function NodePageBody({
  model,
  node,
  editor,
  projectPath,
  onSelectNode,
  onSelectGroup,
  onRender,
  variationState,
  onStartVariation,
  onAcceptVariation,
  onDiscardVariations,
  onSelectVariation,
  newRespIds,
  onClearNewResp,
}: {
  model: ScryModel;
  node: Node;
  editor: Editor | undefined;
  projectPath: string | null;
  onSelectNode: (id: string) => void;
  onSelectGroup: (id: string) => void;
  onRender?: (nodeId: string) => void;
  variationState: VariationState | null;
  onStartVariation?: (nodeId: string, prompt: string, count?: number, baseVariationIdx?: number) => void;
  onAcceptVariation?: (nodeId: string, variationIdx: number) => void;
  onDiscardVariations?: (nodeId: string) => void;
  onSelectVariation?: (idx: number | null) => void;
  newRespIds: ReadonlySet<string>;
  onClearNewResp: (id: string) => void;
}) {
  const ed = useEditSections();
  const status = effectiveNodeStatus(node);
  const tag = typeTag(node);
  const KindIcon = lookupIcon(node.icon) ?? kindIcon(node);

  const sourceMap = model.sourceMap ?? {};
  const dataShape = isDataShape(node);
  const isSymbol = node.kind === "symbol";
  const definition = sourceMap[node.id] ?? [];
  const group = model.groups.find((g) => g.memberIds.includes(node.id));

  return (
    <div className="flex min-w-0 flex-1 flex-col overflow-y-auto">
      <PageHeader
        eyebrow={
          <>
            <KindIcon className="h-3.5 w-3.5" />
            <span>{dataShape ? "Data type" : tag.type}</span>
            {group && (
              <>
                <span className="text-[var(--text-ghost)]">·</span>
                <button
                  type="button"
                  onClick={() => onSelectGroup(group.id)}
                  className="inline-flex items-center gap-1 rounded px-1 hover:bg-[var(--surface-hover)] hover:text-[var(--text-secondary)] cursor-pointer"
                >
                  <FolderOpen className="h-3 w-3" />
                  {group.name || "Group"}
                </button>
              </>
            )}
          </>
        }
        name={node.name}
        status={status}
        editor={editor}
        editingName={ed.isEditing("title")}
        onToggleName={() => ed.toggle("title")}
        onRename={(v) => editor?.updateNode(node.id, { name: v })}
      />

      {/* flow-root contains the floated infobox; sections (also flow-root) wrap
          beside it and reclaim full width below it, like a Wikipedia article. */}
      <div className="flow-root pb-12">
        <div className="float-right mb-4 ml-6 mr-6 mt-5 w-[300px]">
          <Infobox model={model} node={node} editor={editor} onSelectNode={onSelectNode} />
        </div>

        <DescriptionSection
          value={node.description}
          editor={editor}
          editing={ed.isEditing("description")}
          onToggle={() => ed.toggle("description")}
          onCommit={(v) => editor?.updateNode(node.id, { description: v || undefined })}
        />

        {node.visual && (
          <PreviewSection
            node={node}
            projectPath={projectPath}
            onRender={onRender}
            variationState={variationState?.nodeId === node.id ? variationState : null}
            onStartVariation={onStartVariation}
            onAcceptVariation={onAcceptVariation}
            onDiscardVariations={onDiscardVariations}
            onSelectVariation={onSelectVariation}
          />
        )}

        {!dataShape && (
          <ResponsibilitiesSection
            host="node"
            hostId={node.id}
            resps={node.responsibilities ?? []}
            sourceMap={sourceMap}
            projectPath={projectPath}
            editor={editor}
            editing={ed.isEditing("responsibilities")}
            onToggle={() => ed.toggle("responsibilities")}
            newRespIds={newRespIds}
            onClearNewResp={onClearNewResp}
          />
        )}

        {isSymbol && (
          <PropertiesSection
            node={node}
            editor={editor}
            editing={ed.isEditing("properties")}
            onToggle={() => ed.toggle("properties")}
          />
        )}

        {definition.length > 0 && (
          <PageSection title="Definition">
            <CodeList locations={definition} projectPath={projectPath} />
          </PageSection>
        )}
      </div>
    </div>
  );
}

// --- group page -------------------------------------------------------------

function GroupPageBody({
  model,
  group,
  editor,
  projectPath,
  onSelectNode,
  newRespIds,
  onClearNewResp,
}: {
  model: ScryModel;
  group: Group;
  editor: Editor | undefined;
  projectPath: string | null;
  onSelectNode: (id: string) => void;
  newRespIds: ReadonlySet<string>;
  onClearNewResp: (id: string) => void;
}) {
  const ed = useEditSections();
  const members = group.memberIds
    .map((id) => model.nodes.find((n) => n.id === id))
    .filter((n): n is Node => Boolean(n));
  const memberStatuses = members
    .map((m) => effectiveNodeStatus(m))
    .filter((s): s is Status => Boolean(s));
  const ownStatuses = (group.responsibilities ?? []).map((r) => r.status ?? "proposed");
  const all = [...ownStatuses, ...memberStatuses];
  const status = all.length ? rollupStatus(all) : null;

  return (
    <div className="flex min-w-0 flex-1 flex-col">
      <PageHeader
        eyebrow={
          <>
            <FolderOpen className="h-3.5 w-3.5" />
            <span>Group</span>
          </>
        }
        name={group.name || "Group"}
        status={status}
        editor={editor}
        editingName={ed.isEditing("title")}
        onToggleName={() => ed.toggle("title")}
        onRename={(v) => editor?.updateGroup(group.id, { name: v })}
      />

      <div className="min-w-0 flex-1 overflow-y-auto">
        <DescriptionSection
          value={group.description}
          editor={editor}
          editing={ed.isEditing("description")}
          onToggle={() => ed.toggle("description")}
          onCommit={(v) => editor?.updateGroup(group.id, { description: v || undefined })}
        />

        <ResponsibilitiesSection
          host="group"
          hostId={group.id}
          resps={group.responsibilities ?? []}
          sourceMap={model.sourceMap ?? {}}
          projectPath={projectPath}
          editor={editor}
          editing={ed.isEditing("responsibilities")}
          onToggle={() => ed.toggle("responsibilities")}
          newRespIds={newRespIds}
          onClearNewResp={onClearNewResp}
        />

        <PageSection
          title="Members"
          count={members.length}
          editable={!!editor && members.length > 0}
          editing={ed.isEditing("members")}
          onToggleEdit={() => ed.toggle("members")}
        >
          {members.length === 0 ? (
            <Empty>No members yet. Add nodes to this group from the tree.</Empty>
          ) : (
            <ul className="-mx-1 flex flex-col">
              {members.map((m) => (
                <li key={m.id} className="flex items-center gap-1">
                  <span className="flex-1">
                    <WikiLink name={m.name} Icon={kindIcon(m)} onClick={() => onSelectNode(m.id)} />
                  </span>
                  <StatusTag status={effectiveNodeStatus(m)} />
                  {editor && ed.isEditing("members") && (
                    <button
                      type="button"
                      title="Remove from group"
                      onClick={() => editor.setNodeGroup(m.id, null)}
                      className="ml-1 rounded p-1 text-[var(--text-ghost)] hover:text-red-400 cursor-pointer"
                    >
                      <X className="h-3.5 w-3.5" />
                    </button>
                  )}
                </li>
              ))}
            </ul>
          )}
        </PageSection>
      </div>
    </div>
  );
}

// --- description ------------------------------------------------------------

function DescriptionSection({
  value,
  editor,
  editing,
  onToggle,
  onCommit,
}: {
  value: string | undefined;
  editor: Editor | undefined;
  editing: boolean;
  onToggle: () => void;
  onCommit: (v: string) => void;
}) {
  return (
    <PageSection
      title="Description"
      editable={!!editor}
      editing={editing}
      onToggleEdit={onToggle}
    >
      {editing ? (
        <Textarea
          autoFocus
          defaultValue={value ?? ""}
          rows={3}
          placeholder="Describe what this is."
          onBlur={(e) => onCommit(e.currentTarget.value)}
          className="w-full text-[13px] leading-relaxed"
        />
      ) : value ? (
        <p className="text-[13px] leading-relaxed text-[var(--text-secondary)]">{value}</p>
      ) : (
        <Empty>No description.</Empty>
      )}
    </PageSection>
  );
}

// --- responsibilities -------------------------------------------------------

function ResponsibilitiesSection({
  host,
  hostId,
  resps,
  sourceMap,
  projectPath,
  editor,
  editing,
  onToggle,
  newRespIds,
  onClearNewResp,
}: {
  host: "node" | "group";
  hostId: string;
  resps: Responsibility[];
  sourceMap: Record<string, SourceLocation[]>;
  projectPath: string | null;
  editor: Editor | undefined;
  editing: boolean;
  onToggle: () => void;
  newRespIds: ReadonlySet<string>;
  onClearNewResp: (id: string) => void;
}) {
  return (
    <PageSection
      title="Responsibilities"
      count={resps.length}
      editable={!!editor}
      editing={editing}
      onToggleEdit={onToggle}
    >
      {resps.length === 0 && !editing ? (
        <Empty>No responsibilities.</Empty>
      ) : (
        <ul className={editing ? "flex flex-col gap-2" : "flex flex-col divide-y divide-[var(--border-subtle)]"}>
          {resps.map((r) =>
            editing && editor ? (
              <ResponsibilityEdit
                key={r.id}
                host={host}
                hostId={hostId}
                resp={r}
                editor={editor}
              />
            ) : (
              <ResponsibilityRead
                key={r.id}
                host={host}
                hostId={hostId}
                resp={r}
                locations={sourceMap[r.id] ?? []}
                projectPath={projectPath}
                editor={editor}
                isNew={newRespIds.has(r.id)}
                onSeen={() => onClearNewResp(r.id)}
              />
            ),
          )}
        </ul>
      )}
      {editing && editor && (
        <button
          type="button"
          onClick={() => editor.addResponsibility(host, hostId)}
          className="mt-3 inline-flex items-center gap-1 rounded px-1.5 py-0.5 text-[11px] font-medium text-[var(--text-tertiary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text-secondary)] cursor-pointer"
        >
          <Plus className="h-3 w-3" /> Add responsibility
        </button>
      )}
    </PageSection>
  );
}

function ResponsibilityRead({
  host,
  hostId,
  resp,
  locations,
  projectPath,
  editor,
  isNew,
  onSeen,
}: {
  host: "node" | "group";
  hostId: string;
  resp: Responsibility;
  locations: SourceLocation[];
  projectPath: string | null;
  editor: Editor | undefined;
  isNew: boolean;
  onSeen: () => void;
}) {
  const status: Status = resp.status ?? "proposed";
  const directives = resp.directives ?? [];
  const reviewable = resp.vagrant && host === "node" && editor;

  return (
    <li
      onClick={isNew ? onSeen : undefined}
      className={`flex flex-col gap-1.5 py-3 first:pt-0 last:pb-0 ${isNew ? "-mx-2 rounded-md bg-indigo-500/10 px-2" : ""}`}
    >
      <div className="flex items-baseline gap-3">
        <p className="min-w-0 flex-1 text-[13px] leading-normal text-[var(--text-secondary)]">
          {resp.statement || <span className="italic text-[var(--text-ghost)]">Untitled responsibility</span>}
          {resp.vagrant && (
            <span className="ml-1.5 align-middle text-[10px] font-medium uppercase tracking-wide text-amber-600 dark:text-amber-400">
              · drift
            </span>
          )}
          {(resp.relocatedTo || resp.relocatedFrom) && (
            <span className="ml-1.5 align-middle text-[10px] font-medium uppercase tracking-wide text-violet-600 dark:text-violet-400">
              · moved
            </span>
          )}
        </p>
        <StatusTag status={status} />
      </div>

      {directives.length > 0 && (
        <ul className="ml-3 flex flex-col gap-0.5">
          {directives.map((d, i) => (
            <li key={i} className="text-[12px] italic leading-snug text-[var(--text-muted)]">
              → {d}
            </li>
          ))}
        </ul>
      )}

      {locations.length > 0 && (
        <div className="mt-2 flex flex-col gap-2">
          <CodeList locations={locations} projectPath={projectPath} />
        </div>
      )}

      {reviewable && (
        <div className="mt-0.5 flex items-center gap-3 text-[11px]">
          <button
            type="button"
            onClick={() => editor!.updateResponsibility(host, hostId, resp.id, { vagrant: undefined })}
            className="font-medium text-emerald-600 dark:text-emerald-400 hover:underline cursor-pointer"
          >
            Adopt
          </button>
          <button
            type="button"
            onClick={() => editor!.removeResponsibility(host, hostId, resp.id)}
            className="font-medium text-[var(--text-tertiary)] hover:text-red-400 hover:underline cursor-pointer"
          >
            Reject
          </button>
        </div>
      )}
    </li>
  );
}

function ResponsibilityEdit({
  host,
  hostId,
  resp,
  editor,
}: {
  host: "node" | "group";
  hostId: string;
  resp: Responsibility;
  editor: Editor;
}) {
  const directives = resp.directives ?? [];
  const setDirective = (i: number, v: string) => {
    const next = directives.slice();
    if (v.trim() === "") next.splice(i, 1);
    else next[i] = v;
    editor.updateResponsibility(host, hostId, resp.id, {
      directives: next.length ? next : undefined,
    });
  };

  return (
    <li className="flex flex-col gap-1 rounded-md border border-[var(--border-subtle)] bg-[var(--surface-raised)] p-2">
      <div className="flex items-start gap-1.5">
        <Textarea
          defaultValue={resp.statement}
          rows={1}
          placeholder="Verb-led statement of accountability"
          onBlur={(e) =>
            editor.updateResponsibility(host, hostId, resp.id, { statement: e.currentTarget.value })
          }
          className="min-w-0 flex-1 !py-1.5 text-[13px] leading-snug"
        />
        {!resp.locked && (
          <button
            type="button"
            title="Delete responsibility"
            onClick={() => editor.removeResponsibility(host, hostId, resp.id)}
            className="mt-1.5 shrink-0 rounded p-0.5 text-[var(--text-ghost)] hover:text-red-400 cursor-pointer"
          >
            <Trash2 className="h-3.5 w-3.5" />
          </button>
        )}
      </div>
      <div className="flex items-center gap-3 text-[11px]">
        <div className="flex items-center gap-1.5">
          <span className="text-[var(--text-tertiary)]">Status</span>
          <div className="w-32">
            <Select
              variant="inline"
              options={STATUS_OPTIONS}
              value={resp.status ?? "proposed"}
              onChange={(v) =>
                editor.updateResponsibility(host, hostId, resp.id, { status: v as Status })
              }
            />
          </div>
        </div>
        <span className="text-[var(--text-ghost)]">·</span>
        <button
          type="button"
          onClick={() =>
            editor.updateResponsibility(host, hostId, resp.id, { directives: [...directives, " "] })
          }
          className="rounded px-1 py-0.5 text-[var(--text-tertiary)] hover:text-[var(--text-secondary)] cursor-pointer"
        >
          + directive
        </button>
      </div>
      {directives.length > 0 && (
        <div className="flex flex-col gap-0.5 pl-1">
          {directives.map((d, i) => (
            <Input
              key={i}
              variant="bordered"
              defaultValue={d}
              placeholder="(cleared if left empty)"
              onBlur={(e) => setDirective(i, e.currentTarget.value)}
              className="text-[12px]"
            />
          ))}
        </div>
      )}
    </li>
  );
}

// --- properties (data shapes) -----------------------------------------------

function PropertiesSection({
  node,
  editor,
  editing,
  onToggle,
}: {
  node: Node;
  editor: Editor | undefined;
  editing: boolean;
  onToggle: () => void;
}) {
  const properties = node.properties ?? [];
  return (
    <PageSection
      title="Properties"
      count={properties.length}
      editable={!!editor}
      editing={editing}
      onToggleEdit={onToggle}
    >
      {properties.length === 0 && !editing ? (
        <Empty>No properties.</Empty>
      ) : editing && editor ? (
        <div className="flex flex-col gap-2">
          {properties.map((p, i) => (
            <div key={i} className="flex items-center gap-2">
              <Input
                variant="bordered"
                defaultValue={p.label}
                placeholder="field"
                onBlur={(e) => editor.updateProperty(node.id, i, { label: e.currentTarget.value })}
                className="w-40 font-mono text-[12px]"
              />
              <Input
                variant="bordered"
                defaultValue={p.description ?? ""}
                placeholder="description"
                onBlur={(e) =>
                  editor.updateProperty(node.id, i, { description: e.currentTarget.value || undefined })
                }
                className="flex-1 text-[12px]"
              />
              <div className="w-32">
                <Select
                  variant="inline"
                  options={STATUS_OPTIONS}
                  value={p.status ?? "proposed"}
                  onChange={(v) => editor.updateProperty(node.id, i, { status: v as Status })}
                />
              </div>
              <button
                type="button"
                title="Delete property"
                onClick={() => editor.removeProperty(node.id, i)}
                className="rounded p-1 text-[var(--text-ghost)] hover:text-red-400 cursor-pointer"
              >
                <Trash2 className="h-3.5 w-3.5" />
              </button>
            </div>
          ))}
          <button
            type="button"
            onClick={() => editor.addProperty(node.id)}
            className="mt-1 inline-flex items-center gap-1 self-start rounded px-1.5 py-0.5 text-[11px] font-medium text-[var(--text-tertiary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text-secondary)] cursor-pointer"
          >
            <Plus className="h-3 w-3" /> Add property
          </button>
        </div>
      ) : (
        <table className="w-full border-collapse text-[12.5px]">
          <thead>
            <tr className="border-b border-[var(--border-subtle)] text-left text-[10px] font-semibold uppercase tracking-[0.12em] text-[var(--text-tertiary)]">
              <th className="py-1 pr-3 font-semibold">Field</th>
              <th className="py-1 pr-3 font-semibold">Description</th>
              <th className="py-1 font-semibold">Status</th>
            </tr>
          </thead>
          <tbody>
            {properties.map((p, i) => (
              <tr key={i} className="border-b border-[var(--border-subtle)] align-top">
                <td className="py-1.5 pr-3 font-mono font-medium text-[var(--text-secondary)]">
                  {p.label}
                </td>
                <td className="py-1.5 pr-3 text-[var(--text-muted)]">{p.description || "—"}</td>
                <td className="py-1.5">
                  <StatusTag status={p.status} />
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </PageSection>
  );
}

// --- visual preview ---------------------------------------------------------

function previewSrc(nodeId: string, projectPath: string) {
  const isDark = document.documentElement.classList.contains("dark");
  return `preview://${nodeId}/index.html?project=${encodeURIComponent(projectPath)}&theme=${isDark ? "dark" : "light"}`;
}

function variationSrc(nodeId: string, varIdx: number, projectPath: string) {
  const isDark = document.documentElement.classList.contains("dark");
  return `preview://${nodeId}__v${varIdx}/index.html?project=${encodeURIComponent(projectPath)}&theme=${isDark ? "dark" : "light"}`;
}

function PreviewSection({
  node,
  projectPath,
  onRender,
  variationState,
  onStartVariation,
  onAcceptVariation,
  onDiscardVariations,
  onSelectVariation,
}: {
  node: Node;
  projectPath: string | null;
  onRender?: (nodeId: string) => void;
  variationState: VariationState | null;
  onStartVariation?: (nodeId: string, prompt: string, count?: number, baseVariationIdx?: number) => void;
  onAcceptVariation?: (nodeId: string, variationIdx: number) => void;
  onDiscardVariations?: (nodeId: string) => void;
  onSelectVariation?: (idx: number | null) => void;
}) {
  const [modalOpen, setModalOpen] = useState(false);
  const prevVarStatus = useRef<string | null>(null);
  useEffect(() => {
    if (variationState?.status === "ready" && prevVarStatus.current === "generating") {
      setModalOpen(true);
    }
    prevVarStatus.current = variationState?.status ?? null;
  }, [variationState?.status]);

  const preview = node.appearance;
  const hasPreview = preview?.distPath;
  const iframeSrc = hasPreview && projectPath
    ? previewSrc(node.id, projectPath)
    : null;

  const canEdit = hasPreview && onStartVariation;

  return (
    <PageSection
      title="Visual"
      right={preview?.status ? <StatusTag status={preview.status} /> : undefined}
      editable={!!canEdit}
      editing={modalOpen}
      onToggleEdit={() => setModalOpen(!modalOpen)}
    >
      {iframeSrc ? (
        <div className="flex flex-col gap-2">
          <div className="overflow-hidden rounded-md border border-[var(--border-subtle)]">
            <iframe
              src={iframeSrc}
              title={`Preview: ${node.name}`}
              className="h-[400px] w-full border-0"
              sandbox="allow-scripts allow-same-origin"
            />
          </div>
          {onRender && (
            <button
              type="button"
              onClick={() => onRender(node.id)}
              className="inline-flex items-center gap-1.5 self-start rounded px-2 py-1 text-[11px] font-medium text-[var(--text-tertiary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text-secondary)] cursor-pointer"
            >
              <RefreshCw className="h-3 w-3" /> Re-render
            </button>
          )}
        </div>
      ) : (
        <div className="flex flex-col items-center gap-3 rounded-md border border-dashed border-[var(--border-subtle)] bg-[var(--surface-raised)] px-6 py-10">
          <Eye className="h-6 w-6 text-[var(--text-ghost)]" />
          <p className="text-[12px] text-[var(--text-muted)]">
            No render yet.
          </p>
          {onRender && (
            <button
              type="button"
              onClick={() => onRender(node.id)}
              className="inline-flex items-center gap-1.5 rounded-md bg-[var(--color-blue-500)] px-3 py-1.5 text-[12px] font-medium text-white hover:bg-[var(--color-blue-600)] cursor-pointer"
            >
              <Eye className="h-3.5 w-3.5" /> Render component
            </button>
          )}
        </div>
      )}

      {modalOpen && projectPath && (
        <VariationModal
          node={node}
          projectPath={projectPath}
          variationState={variationState}
          onStartVariation={onStartVariation!}
          onAcceptVariation={onAcceptVariation}
          onDiscardVariations={onDiscardVariations}
          onSelectVariation={onSelectVariation}
          onClose={() => setModalOpen(false)}
        />
      )}
    </PageSection>
  );
}

function VariationModal({
  node,
  projectPath,
  variationState,
  onStartVariation,
  onAcceptVariation,
  onDiscardVariations,
  onSelectVariation,
  onClose,
}: {
  node: Node;
  projectPath: string;
  variationState: VariationState | null;
  onStartVariation: (nodeId: string, prompt: string, count?: number, baseVariationIdx?: number) => void;
  onAcceptVariation?: (nodeId: string, variationIdx: number) => void;
  onDiscardVariations?: (nodeId: string) => void;
  onSelectVariation?: (idx: number | null) => void;
  onClose: () => void;
}) {
  const inputRef = useRef<HTMLInputElement>(null);
  const [count, setCount] = useState<1 | 3>(3);
  const generating = variationState?.status === "generating";
  const ready = variationState?.status === "ready";
  const selectedIdx = variationState?.selectedIdx ?? null;

  const handleSubmit = () => {
    const value = inputRef.current?.value.trim();
    if (!value || generating) return;
    onStartVariation(
      node.id,
      value,
      count,
      ready && selectedIdx != null ? selectedIdx : undefined,
    );
    if (inputRef.current) inputRef.current.value = "";
  };

  const handleAccept = () => {
    if (selectedIdx == null || !onAcceptVariation) return;
    onAcceptVariation(node.id, selectedIdx);
    onClose();
  };

  const handleDiscard = () => {
    onDiscardVariations?.(node.id);
  };

  const varCount = variationState?.count ?? 0;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
      <div className="flex max-h-[90vh] w-[90vw] max-w-[1200px] flex-col overflow-hidden rounded-xl border border-[var(--border-strong)] bg-[var(--surface-canvas)] shadow-2xl">
        {/* Header */}
        <div className="flex items-center justify-between border-b border-[var(--border-subtle)] px-5 py-3">
          <h3 className="text-[14px] font-semibold text-[var(--text)]">
            Plan visual changes — {node.name}
          </h3>
          <button
            type="button"
            onClick={onClose}
            className="rounded p-1 text-[var(--text-tertiary)] hover:bg-[var(--surface-hover)] cursor-pointer"
          >
            <X className="h-4 w-4" />
          </button>
        </div>

        {/* Body */}
        <div className="flex-1 overflow-y-auto">
          {/* Original preview — always visible */}
          <div className="border-b border-[var(--border-subtle)] px-5 py-4">
            <p className="mb-2 text-[11px] font-medium text-[var(--text-tertiary)]">Current</p>
            <div className="overflow-hidden rounded-md border border-[var(--border-subtle)]">
              <iframe
                src={previewSrc(node.id, projectPath)}
                title={`Current: ${node.name}`}
                className="h-[350px] w-full border-0"
                sandbox="allow-scripts allow-same-origin"
              />
            </div>
          </div>

          {/* Prompt bar */}
          <div className="flex items-center gap-2 border-b border-[var(--border-subtle)] px-5 py-3">
            <input
              ref={inputRef}
              type="text"
              placeholder={ready && selectedIdx != null ? "Refine the selected variation…" : "Describe visual changes…"}
              disabled={generating}
              onKeyDown={(e) => { if (e.key === "Enter") handleSubmit(); }}
              className="min-w-0 flex-1 rounded-md border border-[var(--border-subtle)] bg-[var(--surface-raised)] px-3 py-1.5 text-[13px] text-[var(--text)] placeholder:text-[var(--text-ghost)] focus:border-blue-500 focus:outline-none disabled:opacity-50"
            />
            <div className="flex items-center rounded-md border border-[var(--border-subtle)]">
              {([1, 3] as const).map((n) => (
                <button
                  key={n}
                  type="button"
                  disabled={generating}
                  onClick={() => setCount(n)}
                  className={`px-2 py-1 text-[11px] font-medium transition-colors cursor-pointer disabled:opacity-50 ${
                    count === n
                      ? "bg-[var(--surface-hover)] text-[var(--text)]"
                      : "text-[var(--text-tertiary)] hover:text-[var(--text-secondary)]"
                  } ${n === 1 ? "rounded-l-md" : "rounded-r-md border-l border-[var(--border-subtle)]"}`}
                >
                  {n}
                </button>
              ))}
            </div>
            <Button
              variant="primary"
              size="sm"
              disabled={generating}
              onClick={handleSubmit}
            >
              <Send className="h-3.5 w-3.5" />
              {ready ? "Iterate" : "Generate"}
            </Button>
          </div>

          {/* Variations */}
          {generating && (
            <div className="flex flex-col items-center gap-3 px-5 py-12">
              <Loader2 className="h-6 w-6 animate-spin text-[var(--text-muted)]" />
              <p className="text-[13px] text-[var(--text-muted)]">
                Generating {variationState!.count} variation{variationState!.count > 1 ? "s" : ""}…
              </p>
              <p className="text-[11px] text-[var(--text-ghost)]">
                "{variationState!.prompt}"
              </p>
            </div>
          )}

          {ready && (
            <div className="px-5 py-4">
              <div className="mb-3 flex items-baseline justify-between">
                <p className="text-[12px] text-[var(--text-muted)]">
                  "{variationState!.prompt}" — click to select
                </p>
                <div className="flex items-center gap-2">
                  {ready && selectedIdx != null && (
                    <Button
                      variant="primary"
                      color="accent"
                      size="sm"
                      onClick={handleAccept}
                    >
                      <Check className="h-3.5 w-3.5" /> Accept
                    </Button>
                  )}
                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={handleDiscard}
                  >
                    <Undo2 className="h-3.5 w-3.5" /> Discard
                  </Button>
                </div>
              </div>
              <div className={`grid gap-3 ${varCount === 1 ? "grid-cols-1 max-w-[600px]" : "grid-cols-3"}`}>
                {Array.from({ length: varCount }, (_, i) => (
                  <button
                    key={i}
                    type="button"
                    onClick={() => onSelectVariation?.(selectedIdx === i ? null : i)}
                    className={`flex flex-col gap-1.5 rounded-lg border-2 p-1 transition-colors cursor-pointer ${
                      selectedIdx === i
                        ? "border-blue-500 bg-blue-500/5"
                        : "border-[var(--border-subtle)] hover:border-[var(--border-strong)]"
                    }`}
                  >
                    <div className="overflow-hidden rounded-md">
                      <iframe
                        src={variationSrc(node.id, i, projectPath)}
                        title={`Variation ${i + 1}`}
                        className="pointer-events-none h-[280px] w-full border-0"
                        sandbox="allow-scripts allow-same-origin"
                      />
                    </div>
                    <span className={`text-[11px] font-medium ${
                      selectedIdx === i ? "text-blue-500" : "text-[var(--text-tertiary)]"
                    }`}>
                      {selectedIdx === i ? "✓ " : ""}Variation {i + 1}
                    </span>
                  </button>
                ))}
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

// --- shared -----------------------------------------------------------------

function CodeList({
  locations,
  projectPath,
}: {
  locations: SourceLocation[];
  projectPath: string | null;
}) {
  return (
    <>
      {locations.map((loc, i) => (
        <CodeBlock
          key={i}
          projectPath={projectPath}
          pattern={loc.pattern}
          symbol={loc.symbol}
          line={loc.line}
          endLine={loc.endLine}
        />
      ))}
    </>
  );
}
