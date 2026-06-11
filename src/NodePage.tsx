/**
 * The main panel: a read-first wiki page for the selected node or group,
 * following Wikipedia's anatomy:
 *
 *  - header: breadcrumb trail, title, type line, page-level actions
 *  - maintenance banners (ambox): drift, stale claims, undescribed behaviour,
 *    empty symbols — each stating the problem with its verdict actions inline
 *  - lede: the description paragraph, no heading
 *  - infobox: structured summary, right-hand column
 *  - sections with per-section [edit] links, swapped to edit mode in place
 *  - Source: the read-through-to-code section. Claims cite source hunks like
 *    footnotes ([n] jumps down); hunks stack the claims they discharge and
 *    link back. Ranges shared by several claims render once.
 *
 * New items land as `proposed`. Mutations flow through the Editor intents.
 */

import { useEffect, useRef, useState } from "react";
import {
  Check,
  CircleDashed,
  Eye,
  Flag,
  GitCompare,
  Loader2,
  Plus,
  RefreshCw,
  Send,
  Sparkles,
  Trash2,
  Undo2,
  X,
} from "lucide-react";
import type {
  ScryModel,
  Node,
  Group,
  Responsibility,
  SchemaProperty,
  DriftScope,
} from "./viewmodel";
import { isDataShape, nextResponsibilityId } from "./viewmodel";
import type { Editor } from "./editor";
import type { ModelHealthReport } from "./health";
import type { Status } from "./statusColors";
import { FLAG_COLORS, PILL_BASE, STATUS_COLORS } from "./statusColors";
import { effectiveNodeStatus, isNodeEmpty } from "./rollup";
import { rollupStatus } from "./statusColors";
import { kindIcon, typeTag } from "./kindIcon";
import { lookupIcon } from "./IconPicker";
import { Infobox } from "./Infobox";
import { ConnectionsSection } from "./ConnectionsSection";
import {
  SourceSection,
  buildSourceIndex,
  hunkElementId,
  respElementId,
} from "./SourceSection";
import { Input, Textarea, Button, SegmentedControl, type SelectOption } from "./ui";
import {
  Banner,
  EditLink,
  Empty,
  EmptyFlag,
  isRedLink,
  jumpTo,
  PageSection,
  SectionEditor,
  StatusTag,
  USER_STATUSES,
  WikiLink,
  WikiText,
} from "./pagekit";

export interface VariationState {
  nodeId: string;
  prompt: string;
  status: "generating" | "ready";
  count: number;
  selectedIdx: number | null;
}

export type SpecialPage = "changes" | "review";

export type Selected =
  | { kind: "node"; id: string }
  | { kind: "group"; id: string }
  // Wiki special pages — Recent changes and Needs review (App routes these).
  | { kind: "special"; id: SpecialPage };

const STATUS_OPTIONS: SelectOption[] = USER_STATUSES.map((s) => ({
  value: s,
  label: STATUS_COLORS[s].label,
}));

interface PageProps {
  model: ScryModel;
  selected: Selected;
  report: ModelHealthReport | null;
  projectPath: string | null;
  editor: Editor | undefined;
  onSelectNode: (id: string) => void;
  onSelectGroup: (id: string) => void;
  onFill?: (nodeId: string) => void;
  onRender?: (nodeId: string) => void;
  variationState: VariationState | null;
  onStartVariation?: (nodeId: string, prompt: string, count?: number, baseVariationIdx?: number) => void;
  onAcceptVariation?: (nodeId: string, variationIdx: number) => void;
  onDiscardVariations?: (nodeId: string) => void;
  onSelectVariation?: (idx: number | null) => void;
  newRespIds: ReadonlySet<string>;
  onClearNewResp: (id: string) => void;
  /** Boundary-owning nodes whose code changed since the last reconcile —
   *  surfaced as a drift banner on the owning node's page. */
  driftScopes: DriftScope[];
  onCheckDrift?: () => void;
  onDismissDrift?: () => void;
}

/** Per-section edit toggles for one page. Edits inside a section accumulate
 *  in a local draft and persist only on Done; Cancel discards the draft —
 *  nothing reaches the model (or disk) until an explicit commit. */
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
  };
}

export function NodePage(props: PageProps) {
  const { model, selected } = props;
  if (selected.kind === "special") return null; // routed by App, never here
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
    <div className="flex flex-1 items-center justify-center text-xs text-[var(--text-muted)]">
      That page no longer exists.
    </div>
  );
}

// --- header -----------------------------------------------------------------

/** Root-first ancestor chain (excluding the node itself). */
function ancestorChain(model: ScryModel, startParentId: string | undefined | null) {
  const chain: Node[] = [];
  const seen = new Set<string>();
  let cur = startParentId ? model.nodes.find((n) => n.id === startParentId) : undefined;
  while (cur && !seen.has(cur.id)) {
    seen.add(cur.id);
    chain.unshift(cur);
    cur = cur.parentId ? model.nodes.find((n) => n.id === cur!.parentId) : undefined;
  }
  return chain;
}

function Crumbs({
  chain,
  onSelectNode,
}: {
  chain: Node[];
  onSelectNode: (id: string) => void;
}) {
  if (chain.length === 0) return null;
  return (
    <nav className="flex min-w-0 items-center gap-1 text-2xs text-[var(--text-muted)]">
      {chain.map((n, i) => (
        <span key={n.id} className="flex min-w-0 items-center gap-1">
          {i > 0 && <span className="text-[var(--text-ghost)]">›</span>}
          <button
            type="button"
            onClick={() => onSelectNode(n.id)}
            className="max-w-[200px] truncate rounded px-0.5 hover:bg-[var(--surface-hover)] hover:text-[var(--text-secondary)] hover:underline cursor-pointer"
          >
            {n.name || "Untitled"}
          </button>
        </span>
      ))}
    </nav>
  );
}

function PageHeader({
  crumbs,
  actions,
  name,
  typeLine,
  editor,
  editingName,
  onToggleName,
  onRename,
}: {
  crumbs: React.ReactNode;
  /** Page-level actions, right-aligned on the crumb line. */
  actions?: React.ReactNode;
  name: string;
  /** The line under the title: kind icon, type word, technology, status. */
  typeLine: React.ReactNode;
  editor: Editor | undefined;
  editingName: boolean;
  onToggleName: () => void;
  onRename: (v: string) => void;
}) {
  // Enter commits the rename, Esc or click-away cancels — one exit path
  // (blur), with the commit decision carried across it.
  const commitRef = useRef(false);
  return (
    <header className="shrink-0 border-b border-[var(--border-subtle)] px-8 pb-4 pt-4">
      <div className="mx-auto w-full max-w-[1080px]">
        <div className="flex min-h-[18px] items-center gap-1.5">
          {crumbs}
          <span className="flex-1" />
          {actions}
        </div>
        <div className="mt-1 flex items-baseline gap-3">
          <div className="min-w-0 flex-1">
            {editingName ? (
              <Input
                variant="title"
                autoFocus
                defaultValue={name}
                onKeyDown={(e) => {
                  if (e.key === "Enter") {
                    commitRef.current = true;
                    e.currentTarget.blur();
                  } else if (e.key === "Escape") {
                    commitRef.current = false;
                    e.currentTarget.blur();
                  }
                }}
                onBlur={(e) => {
                  if (commitRef.current) onRename(e.currentTarget.value);
                  commitRef.current = false;
                  onToggleName();
                }}
                className="w-full !text-xl font-semibold leading-tight"
              />
            ) : (
              <h1 className="truncate text-xl font-semibold leading-tight text-[var(--text)]">
                {name || "Untitled"}
              </h1>
            )}
          </div>
          {editor && <EditLink editing={editingName} onClick={onToggleName} />}
        </div>
        <div className="mt-1.5 flex items-center gap-2 text-2xs font-medium text-[var(--text-tertiary)]">
          {typeLine}
        </div>
      </div>
    </header>
  );
}

// --- node page --------------------------------------------------------------

function NodePageBody(props: PageProps & { node: Node }) {
  const {
    model,
    node,
    report,
    editor,
    projectPath,
    onSelectNode,
    onSelectGroup,
    onFill,
    onRender,
    newRespIds,
    onClearNewResp,
    driftScopes,
    onCheckDrift,
    onDismissDrift,
  } = props;
  const ed = useEditSections();
  const status = effectiveNodeStatus(node);
  const tag = typeTag(node);
  const KindIcon = lookupIcon(node.icon) ?? kindIcon(node);

  const sourceMap = model.sourceMap ?? {};
  const dataShape = isDataShape(node);
  const resps = node.responsibilities ?? [];
  const definition = sourceMap[node.id] ?? [];
  const hasChildren = model.nodes.some((n) => n.parentId === node.id);

  // "Model from code" is for nodes that are still undefined — a structural
  // node with no children and no claims of its own. A defined node never
  // shows it: there's nothing to fill.
  const fillable = node.kind !== "symbol" && node.kind !== "person" && !node.external;
  const undefinedNode = fillable && !hasChildren && resps.length === 0;

  // Leaf claims must read through to code; structural nodes discharge through
  // their subtree, so their claims are never "unmapped".
  const leafHost = !hasChildren && !node.external;

  const sourceIndex = buildSourceIndex(definition, dataShape ? [] : resps, sourceMap);

  const staleCount = resps.filter((r) => r.stale).length;
  const vagrantCount = resps.filter((r) => r.vagrant).length;
  const drift = driftScopes.find((s) => s.nodeId === node.id);

  return (
    <div className="flex min-w-0 flex-1 flex-col">
      <PageHeader
        crumbs={<Crumbs chain={ancestorChain(model, node.parentId)} onSelectNode={onSelectNode} />}
        actions={
          <>
            {undefinedNode && onFill && (
              <button
                type="button"
                onClick={() => onFill(node.id)}
                title="Have the agent model this node from the codebase — children, responsibilities, source mapping"
                className="inline-flex items-center gap-1 rounded px-1.5 py-0.5 text-2xs font-medium text-indigo-600 hover:bg-indigo-500/10 dark:text-indigo-400 cursor-pointer"
              >
                <Sparkles className="h-3 w-3" /> Model from code
              </button>
            )}
          </>
        }
        name={node.name}
        typeLine={
          <>
            <KindIcon className="h-3.5 w-3.5" />
            <span>{dataShape ? "Data type" : tag.type}</span>
            {node.technology && (
              <>
                <span className="text-[var(--text-ghost)]">·</span>
                <span>{node.technology}</span>
              </>
            )}
            {status && status !== "implemented" && (
              <>
                <span className="text-[var(--text-ghost)]">·</span>
                <StatusTag status={status} />
              </>
            )}
            {isNodeEmpty(node) && <EmptyFlag />}
            {node.deprecated && (
              <span
                className={`shrink-0 ${PILL_BASE} bg-red-500/10 text-red-700 ring-red-500/25 dark:bg-red-400/10 dark:text-red-300 dark:ring-red-400/25`}
              >
                deprecated
              </span>
            )}
          </>
        }
        editor={editor}
        editingName={ed.isEditing("title")}
        onToggleName={() => ed.toggle("title")}
        onRename={(v) => editor?.updateNode(node.id, { name: v })}
      />

      <div className="min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto w-full max-w-[1080px] px-8 pb-16">
          {/* Maintenance banners — the page states its own problems up top. */}
          {(drift || staleCount > 0 || vagrantCount > 0 || isNodeEmpty(node)) && (
            <div className="flex flex-col gap-2 pt-4">
              {drift && (
                <Banner
                  tone="warning"
                  icon={<GitCompare className="h-3.5 w-3.5" />}
                  actions={
                    <>
                      {onCheckDrift && (
                        <button
                          type="button"
                          onClick={onCheckDrift}
                          className="rounded px-1.5 py-0.5 text-2xs font-medium text-orange-600 hover:bg-[var(--surface-hover)] dark:text-orange-400 cursor-pointer"
                        >
                          Run drift check
                        </button>
                      )}
                      {onDismissDrift && (
                        <button
                          type="button"
                          onClick={onDismissDrift}
                          title="Mark reconciled without a semantic check"
                          className="rounded px-1.5 py-0.5 text-2xs font-medium text-[var(--text-tertiary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text)] cursor-pointer"
                        >
                          Dismiss
                        </button>
                      )}
                    </>
                  }
                >
                  Code in this node's boundary changed since the last reconcile (
                  {drift.changedFiles.length} file{drift.changedFiles.length === 1 ? "" : "s"}
                  ) — the claims below may no longer hold.
                </Banner>
              )}
              {staleCount > 0 && (
                <Banner
                  tone="warning"
                  icon={<Flag className="h-3.5 w-3.5" />}
                  actions={
                    <button
                      type="button"
                      onClick={() => {
                        const first = resps.find((r) => r.stale);
                        if (first) jumpTo(respElementId(first.id));
                      }}
                      className="rounded px-1.5 py-0.5 text-2xs font-medium text-orange-600 hover:bg-[var(--surface-hover)] dark:text-orange-400 cursor-pointer"
                    >
                      Review
                    </button>
                  }
                >
                  The drift check judged {staleCount} claim{staleCount === 1 ? "" : "s"} no
                  longer discharged by the code. Each needs a verdict: still valid, reword,
                  or drop.
                </Banner>
              )}
              {vagrantCount > 0 && (
                <Banner
                  tone="danger"
                  icon={<Flag className="h-3.5 w-3.5" />}
                  actions={
                    <button
                      type="button"
                      onClick={() => {
                        const first = resps.find((r) => r.vagrant);
                        if (first) jumpTo(respElementId(first.id));
                      }}
                      className="rounded px-1.5 py-0.5 text-2xs font-medium text-red-600 hover:bg-[var(--surface-hover)] dark:text-red-400 cursor-pointer"
                    >
                      Review
                    </button>
                  }
                >
                  {vagrantCount === 1 ? "A behaviour" : `${vagrantCount} behaviours`} found in
                  the code {vagrantCount === 1 ? "is" : "are"} not described by this page.
                  Adopt into the contract or reject.
                </Banner>
              )}
              {isNodeEmpty(node) && (
                <Banner tone="warning" icon={<CircleDashed className="h-3.5 w-3.5" />}>
                  This symbol carries no semantic content — no responsibilities, no
                  properties. Give it a business responsibility or remove it.
                </Banner>
              )}
            </div>
          )}

          <div className="flex flex-col gap-6 pt-5 lg:flex-row lg:gap-8">
            {/* Article column. */}
            <article className="min-w-0 flex-1">
              <DescriptionSection
                value={node.description}
                model={model}
                onSelectNode={onSelectNode}
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
                  variationState={
                    props.variationState?.nodeId === node.id ? props.variationState : null
                  }
                  onStartVariation={props.onStartVariation}
                  onAcceptVariation={props.onAcceptVariation}
                  onDiscardVariations={props.onDiscardVariations}
                  onSelectVariation={props.onSelectVariation}
                />
              )}

              {!dataShape && (
                <ResponsibilitiesSection
                  model={model}
                  host="node"
                  hostId={node.id}
                  resps={resps}
                  citations={sourceIndex.citations}
                  leafHost={leafHost}
                  editor={editor}
                  editing={ed.isEditing("responsibilities")}
                  onToggle={() => ed.toggle("responsibilities")}
                  onSelectNode={onSelectNode}
                  newRespIds={newRespIds}
                  onClearNewResp={onClearNewResp}
                />
              )}

              {node.kind === "symbol" && (
                <PropertiesSection
                  node={node}
                  editor={editor}
                  editing={ed.isEditing("properties")}
                  onToggle={() => ed.toggle("properties")}
                />
              )}

              {(sourceIndex.hunks.length > 0 || sourceIndex.wholeFiles.length > 0) && (
                <PageSection title="Source" count={sourceIndex.hunks.length}>
                  <SourceSection index={sourceIndex} projectPath={projectPath} />
                </PageSection>
              )}

              <ConnectionsSection
                model={model}
                node={node}
                report={report}
                editor={editor}
                onSelectNode={onSelectNode}
              />
            </article>

            {/* Infobox column — sticky beside the article, Wikipedia-right. */}
            <div className="w-full shrink-0 lg:sticky lg:top-4 lg:w-[300px] lg:self-start">
              <Infobox
                model={model}
                node={node}
                report={report}
                editor={editor}
                onSelectNode={onSelectNode}
                onSelectGroup={onSelectGroup}
              />
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

// --- group page -------------------------------------------------------------

function GroupPageBody(props: PageProps & { group: Group }) {
  const { model, group, editor, projectPath, onSelectNode, newRespIds, onClearNewResp } = props;
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

  const sourceMap = model.sourceMap ?? {};
  const resps = group.responsibilities ?? [];
  const sourceIndex = buildSourceIndex([], resps, sourceMap);

  const containerId =
    group.parentNodeId ??
    model.nodes.find((n) => n.id === (group.memberIds[0] ?? ""))?.parentId ??
    undefined;

  return (
    <div className="flex min-w-0 flex-1 flex-col">
      <PageHeader
        crumbs={
          <Crumbs
            chain={
              containerId
                ? [...ancestorChain(model, containerId), model.nodes.find((n) => n.id === containerId)!].filter(Boolean)
                : []
            }
            onSelectNode={onSelectNode}
          />
        }
        name={group.name || "Group"}
        typeLine={
          <>
            <span>Group</span>
            {members.length > 0 && (
              <>
                <span className="text-[var(--text-ghost)]">·</span>
                <span>
                  {members.length} member{members.length === 1 ? "" : "s"}
                </span>
              </>
            )}
            {status && status !== "implemented" && (
              <>
                <span className="text-[var(--text-ghost)]">·</span>
                <StatusTag status={status} />
              </>
            )}
          </>
        }
        editor={editor}
        editingName={ed.isEditing("title")}
        onToggleName={() => ed.toggle("title")}
        onRename={(v) => editor?.updateGroup(group.id, { name: v })}
      />

      <div className="min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto w-full max-w-[1080px] px-8 pb-16">
          <article className="min-w-0 max-w-[760px]">
            <DescriptionSection
              value={group.description}
              model={model}
              onSelectNode={onSelectNode}
              editor={editor}
              editing={ed.isEditing("description")}
              onToggle={() => ed.toggle("description")}
              onCommit={(v) => editor?.updateGroup(group.id, { description: v || undefined })}
            />

            <ResponsibilitiesSection
              model={model}
              host="group"
              hostId={group.id}
              resps={resps}
              citations={sourceIndex.citations}
              leafHost={false} // group claims discharge through members
              editor={editor}
              editing={ed.isEditing("responsibilities")}
              onToggle={() => ed.toggle("responsibilities")}
              onSelectNode={onSelectNode}
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
                        <WikiLink
                          name={m.name}
                          Icon={kindIcon(m)}
                          red={isRedLink(m)}
                          onClick={() => onSelectNode(m.id)}
                        />
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
              {editor && ed.isEditing("members") && (
                <div className="mt-3">
                  <Button variant="primary" size="sm" onClick={() => ed.toggle("members")}>
                    Done
                  </Button>
                </div>
              )}
            </PageSection>

            {(sourceIndex.hunks.length > 0 || sourceIndex.wholeFiles.length > 0) && (
              <PageSection title="Source" count={sourceIndex.hunks.length}>
                <SourceSection index={sourceIndex} projectPath={projectPath} />
              </PageSection>
            )}
          </article>
        </div>
      </div>
    </div>
  );
}

// --- description ------------------------------------------------------------

/** The lede — a title-less paragraph directly under the header, Wikipedia
 *  style, so the kind-specific hero (responsibilities / properties / preview)
 *  is the first titled section the eye lands on. Edits live in a draft and
 *  persist only on Save; Cancel/Esc discards. */
function DescriptionSection({
  value,
  model,
  onSelectNode,
  editor,
  editing,
  onToggle,
  onCommit,
}: {
  value: string | undefined;
  model: ScryModel;
  onSelectNode: (id: string) => void;
  editor: Editor | undefined;
  editing: boolean;
  onToggle: () => void;
  onCommit: (v: string) => void;
}) {
  if (editing && editor) {
    return (
      <SectionEditor<string> initial={value ?? ""} onCommit={onCommit} onClose={onToggle}>
        {(draft, setDraft) => (
          <Textarea
            autoFocus
            value={draft}
            rows={3}
            placeholder="Describe what this is. Link other nodes with [[Name]]."
            onChange={(e) => setDraft(e.target.value)}
            className="w-full text-sm leading-relaxed"
          />
        )}
      </SectionEditor>
    );
  }
  return (
    <div className="flow-root">
      <p
        className={`text-sm leading-relaxed ${
          value ? "text-[var(--text-secondary)]" : "italic text-[var(--text-muted)]"
        }`}
      >
        {value ? (
          <WikiText text={value} nodes={model.nodes} onSelectNode={onSelectNode} />
        ) : (
          "No description."
        )}
        {editor && (
          <span className="ml-2 align-baseline">
            <EditLink editing={false} onClick={onToggle} />
          </span>
        )}
      </p>
    </div>
  );
}


// --- responsibilities -------------------------------------------------------

function ResponsibilitiesSection({
  model,
  host,
  hostId,
  resps,
  citations,
  leafHost,
  editor,
  editing,
  onToggle,
  onSelectNode,
  newRespIds,
  onClearNewResp,
}: {
  model: ScryModel;
  host: "node" | "group";
  hostId: string;
  resps: Responsibility[];
  /** respId → hunk citation numbers, for the footnote chips. */
  citations: Map<string, number[]>;
  /** Whether claims here must anchor to source (leaf node). Structural hosts
   *  discharge through their subtree and never flag "unmapped". */
  leafHost: boolean;
  editor: Editor | undefined;
  editing: boolean;
  onToggle: () => void;
  onSelectNode: (id: string) => void;
  newRespIds: ReadonlySet<string>;
  onClearNewResp: (id: string) => void;
}) {
  // "Add responsibility" from read mode opens the editor seeded with a fresh
  // row; nothing is written to the model until Done.
  const [seedNewRow, setSeedNewRow] = useState(false);

  return (
    <PageSection
      title="Responsibilities"
      count={resps.length}
      editable={!!editor}
      editing={editing}
      onToggleEdit={() => {
        setSeedNewRow(false);
        onToggle();
      }}
    >
      {editing && editor ? (
        <ResponsibilitiesEditor
          host={host}
          hostId={hostId}
          initial={resps}
          seedNewRow={seedNewRow}
          editor={editor}
          onClose={() => {
            setSeedNewRow(false);
            onToggle();
          }}
        />
      ) : resps.length === 0 ? (
        <Empty>No responsibilities.</Empty>
      ) : (
        <ol className="-mx-2 flex flex-col">
          {resps.map((r, i) => (
            <ResponsibilityRow
              key={r.id}
              model={model}
              index={i + 1}
              host={host}
              hostId={hostId}
              resp={r}
              cites={citations.get(r.id) ?? []}
              leafHost={leafHost}
              isNew={newRespIds.has(r.id)}
              onSeen={() => onClearNewResp(r.id)}
              onSelectNode={onSelectNode}
              editor={editor}
            />
          ))}
        </ol>
      )}
      {editor && !editing && (
        <button
          type="button"
          onClick={() => {
            setSeedNewRow(true);
            onToggle();
          }}
          className="mt-3 inline-flex items-center gap-1 rounded px-1.5 py-0.5 text-2xs font-medium text-[var(--text-tertiary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text-secondary)] cursor-pointer"
        >
          <Plus className="h-3 w-3" /> Add responsibility
        </button>
      )}
    </PageSection>
  );
}

/**
 * The responsibility-list form inside the shared {@link SectionEditor} shell.
 * Done commits: deletions go through the intent (so side-effects like
 * unlocking a relocated source still fire), then one bulk set writes the
 * final list. Untouched new rows vanish on commit.
 */
function ResponsibilitiesEditor({
  host,
  hostId,
  initial,
  seedNewRow,
  editor,
  onClose,
}: {
  host: "node" | "group";
  hostId: string;
  initial: Responsibility[];
  seedNewRow: boolean;
  editor: Editor;
  onClose: () => void;
}) {
  const seededId = seedNewRow ? nextResponsibilityId(initial) : null;
  const start: Responsibility[] = seededId
    ? [...initial, { id: seededId, statement: "", status: "proposed" }]
    : initial;

  const commit = (draft: Responsibility[]) => {
    const existingIds = new Set(initial.map((r) => r.id));
    const cleaned = draft
      .filter((r) => existingIds.has(r.id) || r.statement.trim() !== "")
      .map((r) => {
        const dirs = (r.directives ?? []).map((s) => s.trim()).filter(Boolean);
        return { ...r, statement: r.statement.trim(), directives: dirs.length ? dirs : undefined };
      });
    const keep = new Set(cleaned.map((r) => r.id));
    for (const r of initial)
      if (!keep.has(r.id)) editor.removeResponsibility(host, hostId, r.id);
    if (host === "node") editor.updateNode(hostId, { responsibilities: cleaned });
    else editor.updateGroup(hostId, { responsibilities: cleaned });
  };

  return (
    <SectionEditor<Responsibility[]>
      initial={start}
      onCommit={commit}
      onClose={onClose}
      footerExtra={(setDraft) => (
        <button
          type="button"
          onClick={() =>
            setDraft((d) => [
              ...d,
              { id: nextResponsibilityId(d), statement: "", status: "proposed" },
            ])
          }
          className="inline-flex items-center gap-1 rounded px-1.5 py-0.5 text-2xs font-medium text-[var(--text-tertiary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text-secondary)] cursor-pointer"
        >
          <Plus className="h-3 w-3" /> Add responsibility
        </button>
      )}
    >
      {(draft, setDraft) => {
        const patchRow = (id: string, patch: Partial<Responsibility>) =>
          setDraft((d) => d.map((r) => (r.id === id ? { ...r, ...patch } : r)));
        const removeRow = (id: string) => setDraft((d) => d.filter((r) => r.id !== id));
        return draft.length === 0 ? (
          <Empty>No responsibilities.</Empty>
        ) : (
          <ul className="flex flex-col gap-2">
            {draft.map((r) => (
              <ResponsibilityEditRow
                key={r.id}
                resp={r}
                // autoFocus fires at mount only — it lands on the seeded row
                // and on rows appended via "Add responsibility".
                autoFocus={r.statement === "" && r.id === draft[draft.length - 1].id}
                onPatch={patchRow}
                onRemove={removeRow}
              />
            ))}
          </ul>
        );
      }}
    </SectionEditor>
  );
}

/**
 * One numbered row of the responsibilities list. The statement carries
 * footnote-style citation chips that jump to the source hunks discharging it;
 * directives and flag verdicts render inline, always visible — the contract
 * reads in one pass, nothing hides behind selection.
 */
function ResponsibilityRow({
  model,
  index,
  host,
  hostId,
  resp,
  cites,
  leafHost,
  isNew,
  onSeen,
  onSelectNode,
  editor,
}: {
  model: ScryModel;
  index: number;
  host: "node" | "group";
  hostId: string;
  resp: Responsibility;
  cites: number[];
  leafHost: boolean;
  isNew: boolean;
  onSeen: () => void;
  onSelectNode: (id: string) => void;
  editor: Editor | undefined;
}) {
  const status: Status = resp.status ?? "proposed";
  const directives = resp.directives ?? [];
  const reviewable = resp.vagrant && host === "node" && editor;
  // A LEAF claim that says code exists but has no source anchor is a blind
  // spot in the lens — flag it. Structural hosts discharge through their
  // subtree; proposed claims naturally have no code yet.
  const unmapped =
    leafHost &&
    cites.length === 0 &&
    (status === "implemented" || status === "verified" || status === "changed");
  const relocTarget = resp.relocatedTo
    ? model.nodes.find((n) => n.id === resp.relocatedTo)
    : undefined;
  const relocSource = resp.relocatedFrom
    ? model.nodes.find((n) => n.id === resp.relocatedFrom)
    : undefined;
  // Implemented is the steady state and stays silent (StatusTag renders null),
  // so the meta line only exists when it has something to say.
  const hasMeta =
    status !== "implemented" ||
    resp.stale === true ||
    resp.vagrant === true ||
    unmapped ||
    !!relocTarget ||
    !!relocSource;

  return (
    <li
      id={respElementId(resp.id)}
      onMouseEnter={isNew ? onSeen : undefined}
      className={`border-b border-[var(--border-subtle)] px-2 py-2.5 last:border-b-0 ${
        isNew ? "bg-indigo-500/10" : ""
      }`}
    >
      <div className="flex items-start gap-3">
        <span className="w-6 shrink-0 pt-px text-right font-mono text-2xs leading-normal tabular-nums text-[var(--text-ghost)]">
          {index}.
        </span>
        <div className="min-w-0 flex-1">
          <p className="text-sm leading-normal text-[var(--text-secondary)]">
            {resp.statement ? (
              <WikiText text={resp.statement} nodes={model.nodes} onSelectNode={onSelectNode} />
            ) : (
              <span className="italic text-[var(--text-ghost)]">Untitled responsibility</span>
            )}
            {cites.map((n) => (
              <button
                key={n}
                type="button"
                onClick={() => jumpTo(hunkElementId(n))}
                title="Jump to the source for this claim"
                className="ml-1 align-super font-mono text-2xs text-blue-700 hover:underline dark:text-blue-400 cursor-pointer"
              >
                [{n}]
              </button>
            ))}
          </p>

          {directives.length > 0 && (
            <ul className="mt-1 flex flex-col gap-0.5">
              {directives.map((d, i) => (
                <li key={i} className="text-xs italic leading-snug text-[var(--text-muted)]">
                  → <WikiText text={d} nodes={model.nodes} onSelectNode={onSelectNode} />
                </li>
              ))}
            </ul>
          )}

          {hasMeta && (
          <span className="mt-1 flex flex-wrap items-center gap-x-2.5 gap-y-0.5 text-2xs">
            <StatusTag status={status} />
            {resp.vagrant && (
              <span
                className={FLAG_COLORS.vagrant.pill}
                title="Found in the code, not described by the contract — adopt or reject below."
              >
                vagrant
              </span>
            )}
            {resp.stale && (
              <span
                className={FLAG_COLORS.stale.pill}
                title="Drift check: the code no longer discharges this claim. Re-implement, reword, or drop it."
              >
                stale
              </span>
            )}
            {unmapped && (
              <span
                className={FLAG_COLORS.stale.pill}
                title="No source lines anchor this responsibility — the claim can't be read through to code."
              >
                unmapped
              </span>
            )}
            {relocTarget && (
              <button
                type="button"
                onClick={() => onSelectNode(relocTarget.id)}
                className={`${FLAG_COLORS.relocated.pill} cursor-pointer hover:underline`}
                title="This claim's code was relocated — jump to where it lives now"
              >
                moved to {relocTarget.name || "node"}
              </button>
            )}
            {relocSource && (
              <button
                type="button"
                onClick={() => onSelectNode(relocSource.id)}
                className={`${FLAG_COLORS.relocated.pill} cursor-pointer hover:underline`}
                title="This claim arrived from another node — jump to its origin"
              >
                moved from {relocSource.name || "node"}
              </button>
            )}
          </span>
          )}

          {/* Verdict actions, inline where the flag is. */}
          {resp.stale && editor && (
            <div className="mt-1.5 flex items-center gap-3 text-2xs">
              <button
                type="button"
                onClick={() =>
                  editor.updateResponsibility(host, hostId, resp.id, { stale: undefined })
                }
                className="font-medium text-[var(--text-tertiary)] hover:text-[var(--text)] hover:underline cursor-pointer"
                title="The claim still holds as written — clear the stale flag"
              >
                Still valid
              </button>
              <span className="text-[var(--text-ghost)]">or reword / delete it via [edit]</span>
            </div>
          )}
          {reviewable && (
            <div className="mt-1.5 flex items-center gap-3 text-2xs">
              <button
                type="button"
                onClick={() => editor!.updateResponsibility(host, hostId, resp.id, { vagrant: undefined })}
                className="font-medium text-indigo-600 dark:text-indigo-400 hover:underline cursor-pointer"
                title="Accept this discovered behaviour into the spec"
              >
                Adopt
              </button>
              <button
                type="button"
                onClick={() => editor!.removeResponsibility(host, hostId, resp.id)}
                className="font-medium text-[var(--text-tertiary)] hover:text-red-500 dark:hover:text-red-400 hover:underline cursor-pointer"
                title="Delete — the code it describes is not wanted behaviour"
              >
                Reject
              </button>
            </div>
          )}
        </div>
      </div>
    </li>
  );
}

/** One draft row of the responsibilities form — fully controlled; every
 *  change lands in the section draft, never directly in the model. */
function ResponsibilityEditRow({
  resp,
  autoFocus,
  onPatch,
  onRemove,
}: {
  resp: Responsibility;
  autoFocus: boolean;
  onPatch: (id: string, patch: Partial<Responsibility>) => void;
  onRemove: (id: string) => void;
}) {
  const directives = resp.directives ?? [];
  const setDirectives = (next: string[]) => onPatch(resp.id, { directives: next });

  return (
    <li className="flex flex-col gap-2 rounded-md border border-[var(--border)] bg-[var(--surface-raised)] p-3">
      <Textarea
        autoFocus={autoFocus}
        value={resp.statement}
        rows={1}
        placeholder="Verb-led statement of accountability"
        onChange={(e) => onPatch(resp.id, { statement: e.target.value })}
        className="text-sm leading-snug"
      />

      {directives.length > 0 && (
        <div className="flex flex-col gap-1">
          {directives.map((d, i) => (
            <div key={i} className="flex items-center gap-1.5 pl-2">
              <span className="shrink-0 text-xs text-[var(--text-ghost)]">→</span>
              <Input
                variant="inline"
                autoFocus={d === ""}
                value={d}
                placeholder={'directive — "must …" / "never …"'}
                onChange={(e) => {
                  const next = directives.slice();
                  next[i] = e.target.value;
                  setDirectives(next);
                }}
                className="flex-1 italic"
              />
              <button
                type="button"
                title="Remove directive"
                onClick={() => {
                  const next = directives.slice();
                  next.splice(i, 1);
                  setDirectives(next);
                }}
                className="shrink-0 rounded p-1 text-[var(--text-ghost)] hover:text-red-400 cursor-pointer"
              >
                <X className="h-3.5 w-3.5" />
              </button>
            </div>
          ))}
        </div>
      )}

      <div className="flex items-center gap-3">
        <div className="w-fit min-w-[220px]">
          <SegmentedControl
            options={STATUS_OPTIONS}
            value={resp.status ?? "proposed"}
            onChange={(v) => onPatch(resp.id, { status: v as Status })}
          />
        </div>
        <button
          type="button"
          onClick={() => setDirectives([...directives, ""])}
          className="rounded px-1.5 py-0.5 text-2xs font-medium text-[var(--text-tertiary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text-secondary)] cursor-pointer"
        >
          <span className="inline-flex items-center gap-1">
            <Plus className="h-3 w-3" /> Directive
          </span>
        </button>
        <span className="flex-1" />
        {!resp.locked && (
          <button
            type="button"
            title="Delete responsibility"
            onClick={() => onRemove(resp.id)}
            className="shrink-0 rounded p-1 text-[var(--text-ghost)] hover:text-red-400 cursor-pointer"
          >
            <Trash2 className="h-3.5 w-3.5" />
          </button>
        )}
      </div>
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
  // "Add property" from read mode opens the editor seeded with a fresh row.
  const [seedNewRow, setSeedNewRow] = useState(false);
  const close = () => {
    setSeedNewRow(false);
    onToggle();
  };

  return (
    <PageSection
      title="Properties"
      count={properties.length}
      editable={!!editor}
      editing={editing}
      onToggleEdit={() => {
        setSeedNewRow(false);
        onToggle();
      }}
    >
      {editing && editor ? (
        <SectionEditor<SchemaProperty[]>
          initial={
            seedNewRow
              ? [...properties, { label: "", description: "", status: "proposed" }]
              : properties
          }
          onCommit={(draft) => {
            const cleaned = draft
              .filter((p) => p.label.trim() !== "" || (p.description ?? "").trim() !== "")
              .map((p) => ({ ...p, label: p.label.trim() }));
            editor.updateNode(node.id, { properties: cleaned });
          }}
          onClose={close}
          footerExtra={(setDraft) => (
            <button
              type="button"
              onClick={() =>
                setDraft((d) => [...d, { label: "", description: "", status: "proposed" }])
              }
              className="inline-flex items-center gap-1 rounded px-1.5 py-0.5 text-2xs font-medium text-[var(--text-tertiary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text-secondary)] cursor-pointer"
            >
              <Plus className="h-3 w-3" /> Add property
            </button>
          )}
        >
          {(draft, setDraft) => {
            const patchRow = (i: number, patch: Partial<SchemaProperty>) =>
              setDraft((d) => d.map((p, j) => (j === i ? { ...p, ...patch } : p)));
            return draft.length === 0 ? (
              <Empty>No properties.</Empty>
            ) : (
              <div className="grid grid-cols-[minmax(120px,160px)_1fr_auto_auto] items-center gap-x-2 gap-y-1.5">
                <span className="text-2xs font-semibold uppercase tracking-[0.12em] text-[var(--text-tertiary)]">
                  Field
                </span>
                <span className="text-2xs font-semibold uppercase tracking-[0.12em] text-[var(--text-tertiary)]">
                  Description
                </span>
                <span className="text-2xs font-semibold uppercase tracking-[0.12em] text-[var(--text-tertiary)]">
                  Status
                </span>
                <span />
                {draft.map((p, i) => (
                  <div key={i} className="col-span-4 grid grid-cols-subgrid items-center">
                    <Input
                      variant="inline"
                      autoFocus={p.label === "" && i === draft.length - 1}
                      value={p.label}
                      placeholder="field"
                      onChange={(e) => patchRow(i, { label: e.target.value })}
                      className="font-mono"
                    />
                    <Input
                      variant="inline"
                      value={p.description ?? ""}
                      placeholder="description"
                      onChange={(e) => patchRow(i, { description: e.target.value })}
                    />
                    <SegmentedControl
                      options={STATUS_OPTIONS}
                      value={p.status ?? "proposed"}
                      onChange={(v) => patchRow(i, { status: v as Status })}
                    />
                    <button
                      type="button"
                      title="Delete property"
                      onClick={() => setDraft((d) => d.filter((_, j) => j !== i))}
                      className="rounded p-1 text-[var(--text-ghost)] hover:text-red-400 cursor-pointer"
                    >
                      <Trash2 className="h-3.5 w-3.5" />
                    </button>
                  </div>
                ))}
              </div>
            );
          }}
        </SectionEditor>
      ) : properties.length === 0 ? (
        <Empty>No properties.</Empty>
      ) : (
        <table className="w-full border-collapse text-sm">
          <thead>
            <tr className="border-b border-[var(--border-subtle)] text-left text-2xs font-semibold uppercase tracking-[0.12em] text-[var(--text-tertiary)]">
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
                  <StatusTag status={p.status ?? "proposed"} />
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      {editor && !editing && (
        <button
          type="button"
          onClick={() => {
            setSeedNewRow(true);
            onToggle();
          }}
          className="mt-3 inline-flex items-center gap-1 rounded px-1.5 py-0.5 text-2xs font-medium text-[var(--text-tertiary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text-secondary)] cursor-pointer"
        >
          <Plus className="h-3 w-3" /> Add property
        </button>
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
  const iframeSrc = hasPreview && projectPath ? previewSrc(node.id, projectPath) : null;

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
          <div className="overflow-hidden rounded-md border border-[var(--border)]">
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
              className="inline-flex items-center gap-1.5 self-start rounded px-2 py-1 text-2xs font-medium text-[var(--text-tertiary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text-secondary)] cursor-pointer"
            >
              <RefreshCw className="h-3 w-3" /> Re-render
            </button>
          )}
        </div>
      ) : (
        <div className="flex flex-col items-center gap-3 rounded-md border border-dashed border-[var(--border)] bg-[var(--surface-raised)] px-6 py-10">
          <Eye className="h-6 w-6 text-[var(--text-ghost)]" />
          <p className="text-xs text-[var(--text-muted)]">No render yet.</p>
          {onRender && (
            <Button variant="primary" size="md" onClick={() => onRender(node.id)}>
              <Eye className="h-3.5 w-3.5" /> Render component
            </Button>
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
          <h3 className="text-sm font-semibold text-[var(--text)]">
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
            <p className="mb-2 text-2xs font-medium text-[var(--text-tertiary)]">Current</p>
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
              className="min-w-0 flex-1 rounded-md border border-[var(--border)] bg-[var(--surface-raised)] px-3 py-1.5 text-sm text-[var(--text)] placeholder:text-[var(--text-ghost)] focus:border-[var(--border-strong)] focus:outline-none disabled:opacity-50"
            />
            <div className="flex items-center rounded-md border border-[var(--border)]">
              {([1, 3] as const).map((n) => (
                <button
                  key={n}
                  type="button"
                  disabled={generating}
                  onClick={() => setCount(n)}
                  className={`px-2 py-1 text-2xs font-medium transition-colors cursor-pointer disabled:opacity-50 ${
                    count === n
                      ? "bg-[var(--surface-hover)] text-[var(--text)]"
                      : "text-[var(--text-tertiary)] hover:text-[var(--text-secondary)]"
                  } ${n === 1 ? "rounded-l-md" : "rounded-r-md border-l border-[var(--border-subtle)]"}`}
                >
                  {n}
                </button>
              ))}
            </div>
            <Button variant="primary" size="sm" disabled={generating} onClick={handleSubmit}>
              <Send className="h-3.5 w-3.5" />
              {ready ? "Iterate" : "Generate"}
            </Button>
          </div>

          {/* Variations */}
          {generating && (
            <div className="flex flex-col items-center gap-3 px-5 py-12">
              <Loader2 className="h-6 w-6 animate-spin text-indigo-500 dark:text-indigo-400" />
              <p className="text-sm text-[var(--text-muted)]">
                Generating {variationState!.count} variation{variationState!.count > 1 ? "s" : ""}…
              </p>
              <p className="text-2xs text-[var(--text-ghost)]">
                "{variationState!.prompt}"
              </p>
            </div>
          )}

          {ready && (
            <div className="px-5 py-4">
              <div className="mb-3 flex items-baseline justify-between">
                <p className="text-xs text-[var(--text-muted)]">
                  "{variationState!.prompt}" — click to select
                </p>
                <div className="flex items-center gap-2">
                  {ready && selectedIdx != null && (
                    <Button variant="primary" color="accent" size="sm" onClick={handleAccept}>
                      <Check className="h-3.5 w-3.5" /> Accept
                    </Button>
                  )}
                  <Button variant="ghost" size="sm" onClick={handleDiscard}>
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
                        ? "border-indigo-500 bg-indigo-500/5"
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
                    <span className={`text-2xs font-medium ${
                      selectedIdx === i ? "text-indigo-500 dark:text-indigo-400" : "text-[var(--text-tertiary)]"
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
