/**
 * The main panel: a read-first wiki page for the selected node or group,
 * following Wikipedia's anatomy:
 *
 *  - header: breadcrumb trail, title, type line, page-level actions
 *  - maintenance banners (ambox): drift, stale claims, undescribed behaviour,
 *    empty symbols — each stating the problem with its verdict actions inline
 *  - lede: the description paragraph, no heading
 *  - type line under the title: kind, technology, status — structured metadata
 *    surfaced inline rather than in a separate column
 *  - sections with per-section [edit] links, swapped to edit mode in place
 *  - Source: the read-through-to-code section. Claims cite source hunks like
 *    footnotes ([n] jumps down); hunks stack the claims they discharge and
 *    link back. Ranges shared by several claims render once.
 *
 * New items land as `proposed`. Mutations flow through the Editor intents.
 */

import { Fragment, useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { invoke } from "@tauri-apps/api/core";
import {
  Check,
  CircleDashed,
  CornerDownRight,
  Eye,
  FileClock,
  Flag,
  GitCompare,
  Loader2,
  Moon,
  Plus,
  Send,
  Sparkles,
  Sun,
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
  SourceLocation,
  DriftScope,
} from "./viewmodel";
import { effectiveSourceMap, isDataShape, nextResponsibilityId } from "./viewmodel";
import { wordDiff } from "./wordDiff";
import type { Editor } from "./editor";
import type { ModelHealthReport } from "./health";
import { FLAG_COLORS } from "./statusColors";
import { isNodeEmpty } from "./rollup";
import { kindIcon, typeTag } from "./kindIcon";
import { lookupIcon } from "./IconPicker";
import { ConnectionsSection, ImpliedConnectionsSection } from "./ConnectionsSection";
import type { ChangeRevision } from "./hooks/useModelStorage";
import {
  EVENT_META,
  type HistoryEvent,
  markerColor,
  relativeTime,
} from "./history";
import { matchPreviewComponent, usePreviewServer } from "./hooks/usePreviewServer";
import { useDarkMode } from "./hooks/useDarkMode";
import { ClaimSource, respElementId } from "./SourceSection";
import { PageMenuProvider, usePageMenu, useCopyId, copyIdItem } from "./pageMenu";
import { Input } from "./ui";
import {
  BTN,
  BTN_AGENT,
  BTN_DANGER,
  BTN_GO,
  CTL,
  DESCRIPTION_MAX,
  Editable,
  EditLink,
  Empty,
  EmptyFlag,
  jumpTo,
  NAME_MAX,
  PageSection,
  sanitizeIdentifier,
  SectionEditor,
  SegField,
  WikiText,
} from "./pagekit";

// Row grid for the mono lanes: marker | index | content. The edit controls
// (CTL) float over the right edge as an absolute overlay (so `relative`), which
// takes no layout space — read↔edit stays the same width and never reflows.
const RESP_ROW = "relative grid grid-cols-[18px_22px_1fr] items-baseline";
const PROP_ROW = "relative grid grid-cols-[18px_22px_1fr] items-baseline";

// The mockup's `.ctl` overlay, scoped to a statement (`/srow`) or directive
// (`/drow`) line so each line reveals only its own controls on hover. Same
// gradient float as the shared CTL; `not-italic` keeps buttons upright on the
// italic directive rows.
const CTL_BASE =
  "pointer-events-none invisible absolute inset-y-0 -right-1 z-10 flex items-center gap-1.5 not-italic pl-9 pr-1 [background-image:linear-gradient(90deg,transparent,color-mix(in_srgb,var(--text)_4%,var(--surface-canvas))_28px)]";
const CTL_SROW = `${CTL_BASE} group-hover/srow:visible`;
const CTL_DROW = `${CTL_BASE} group-hover/drow:visible`;
// Full-cell field highlight: dim on line hover, brighter while focused.
const STMT_HL =
  "group-hover/srow:bg-[color-mix(in_srgb,var(--text)_6%,transparent)] focus:bg-[var(--surface-active)]";
const DIR_HL =
  "group-hover/drow:bg-[color-mix(in_srgb,var(--text)_6%,transparent)] focus:bg-[var(--surface-active)]";

export interface VariationState {
  nodeId: string;
  prompt: string;
  status: "generating" | "ready";
  count: number;
  selectedIdx: number | null;
}

export type SpecialPage = "changes" | "review" | "dark" | "unmapped";

export type Selected =
  | { kind: "node"; id: string }
  | { kind: "group"; id: string }
  // Wiki special pages — Recent changes, Needs review, Dark code, Unmapped
  // claims (App routes these).
  | { kind: "special"; id: SpecialPage };

interface PageProps {
  model: ScryModel;
  /** The committed model (`model.scry`) — the diff base. The Overview renders
   *  each claim as a diff of `model` (working/planned) against this. Null only
   *  in the brief window before the committed model loads. */
  committed: ScryModel | null;
  selected: Selected;
  report: ModelHealthReport | null;
  projectPath: string | null;
  editor: Editor | undefined;
  onSelectNode: (id: string) => void;
  onSelectGroup: (id: string) => void;
  /** B5 repair path: agent writes realistic fixture props after a failed
   *  deterministic render. */
  onFixture?: (nodeId: string, renderStatus: string, renderError: string | null) => void;
  variationState: VariationState | null;
  onStartVariation?: (nodeId: string, prompt: string, count?: number, baseVariationIdx?: number) => void;
  onAcceptVariation?: (nodeId: string, variationIdx: number) => void;
  onDiscardVariations?: (nodeId: string) => void;
  onSelectVariation?: (idx: number | null) => void;
  /** Session-local journal of every edit (yours and the agent's), newest
   *  first — filtered per node to drive the History tab. */
  changeLog: readonly ChangeRevision[];
  /** Durable committed-model timeline (`.scryer/history.jsonl`), oldest first —
   *  filtered per node to drive the History tab. */
  history: readonly HistoryEvent[];
  /** Boundary-owning nodes whose code changed since the last reconcile —
   *  surfaced as a drift banner on the owning node's page. */
  driftScopes: DriftScope[];
  onCheckDrift?: () => void;
  /** Reconcile drift for a node and its subtree (scoped Dismiss). */
  onDismissDrift?: (nodeId: string) => void;
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
    return (
      <PageMenuProvider>
        <GroupPageBody key={group.id} {...props} group={group} />
      </PageMenuProvider>
    );
  }
  const node = model.nodes.find((n) => n.id === selected.id);
  if (!node) return <Gone />;
  return (
    <PageMenuProvider>
      <NodePageBody key={node.id} {...props} node={node} />
    </PageMenuProvider>
  );
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
  // Inherits the header crumb row's font (mono) / size (11px) / color.
  return (
    <nav className="flex min-w-0 items-center gap-1">
      {chain.map((n, i) => (
        <span key={n.id} className="flex min-w-0 items-center gap-1">
          {i > 0 && <span className="text-[var(--text-ghost)]">/</span>}
          <button
            type="button"
            onClick={() => onSelectNode(n.id)}
            className="max-w-[200px] truncate hover:text-[var(--text-secondary)] hover:underline"
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
  tabs,
  editor,
  editingName,
  onToggleName,
  onRename,
  onCancel,
  nameMaxLength,
  nameSanitize,
}: {
  crumbs: React.ReactNode;
  /** Page-level actions, right-aligned on the crumb line. */
  actions?: React.ReactNode;
  name: string;
  /** The line under the title: kind icon, type word, technology, status. */
  typeLine: React.ReactNode;
  /** Article tabs (Overview · History), rendered as the header's last row. */
  tabs?: React.ReactNode;
  editor: Editor | undefined;
  editingName: boolean;
  onToggleName: () => void;
  onRename: (v: string) => void;
  /** Discard the title/type-line edits made this session and close (reverts to
   *  the snapshot taken when edit mode opened). Omit to hide the Cancel button. */
  onCancel?: () => void;
  /** Hard character cap on the title (omitted for symbol names, which are
   *  identifier-shaped rather than length-bound). */
  nameMaxLength?: number;
  /** Coerce the title's shape on input (symbol names → source identifiers). */
  nameSanitize?: (text: string) => string;
}) {
  return (
    <header className="shrink-0 border-b border-[var(--border)] px-7 pt-[13px]">
      <div className="flex min-h-[15px] items-center gap-1 font-mono text-[11px] text-[var(--text-tertiary)]">
        {crumbs}
        <span className="flex-1" />
        {actions}
      </div>
      <div className="mt-[5px] flex items-start gap-4">
        {editingName ? (
          // The title edits in place as a contentEditable span (same metrics as
          // the h1, no reflow), committing on blur. It's `inline-block` so it
          // grows with the text rather than spanning the header; the buttons are
          // pinned right by a flex spacer. Edit mode stays open across fields
          // until Done/Cancel — so you can edit the type line too.
          <div className="flex min-w-0 flex-1 items-baseline gap-2">
            <Editable
              initial={name}
              autoFocus
              maxLength={nameMaxLength}
              sanitize={nameSanitize}
              placeholder="Untitled"
              onCommit={(t) => onRename(t)}
              className="inline-block max-w-full text-[21px] font-semibold leading-tight text-[var(--text)]"
            />
            <span className="flex-1" />
            <span className="flex shrink-0 items-center gap-2">
              {onCancel && (
                <button type="button" onClick={onCancel} className={BTN}>
                  Cancel
                </button>
              )}
              <button type="button" onClick={onToggleName} className={BTN_GO}>
                Done
              </button>
            </span>
          </div>
        ) : (
          <div className="flex min-w-0 flex-1 items-baseline gap-3">
            <h1 className="min-w-0 flex-1 truncate text-[21px] font-semibold leading-tight text-[var(--text)]">
              {name || "Untitled"}
            </h1>
            {editor && <EditLink editing={false} onClick={onToggleName} />}
          </div>
        )}
      </div>
      <div className="mt-[3px] flex items-center gap-2 text-xs text-[var(--text-tertiary)]">
        {typeLine}
      </div>
      {tabs}
    </header>
  );
}

/** A maintenance notice (ambox) — a full-width banner stacked at the top of the
 *  article body: a tinted strip with a left accent rule, an icon, the message,
 *  and inline actions right-aligned. The classic wiki hatnote, not a header chip. */
function Ambox({
  tone,
  icon,
  children,
  actions,
}: {
  tone: "warning" | "danger" | "info";
  icon: React.ReactNode;
  children: React.ReactNode;
  actions?: React.ReactNode;
}) {
  const c =
    tone === "danger"
      ? "border-red-500/30 border-l-red-500/70 bg-red-500/10 text-red-700 dark:text-red-300"
      : tone === "info"
        ? "border-violet-500/30 border-l-violet-500/70 bg-violet-500/10 text-violet-700 dark:text-violet-300"
        : "border-orange-500/30 border-l-orange-500/70 bg-orange-500/10 text-orange-700 dark:text-orange-300";
  return (
    <div className={`flex items-center gap-2.5 rounded-md border border-l-[3px] px-3 py-2 text-xs ${c}`}>
      <span className="shrink-0 opacity-80">{icon}</span>
      <span className="min-w-0 flex-1">{children}</span>
      {actions && <span className="flex shrink-0 items-center gap-3">{actions}</span>}
    </div>
  );
}

/** Inline text action for an {@link Ambox} — terse, underlined, no chrome. */
const NOTICE_ACTION =
  "shrink-0 font-medium underline-offset-2 hover:underline";

// --- tabs -------------------------------------------------------------------

/** Article tabs (Overview · History) — the mockup's `.modes` underline tabs,
 *  rendered as the last row of the page header. */
function PageTabs({
  tab,
  onTab,
  historyCount,
}: {
  tab: "overview" | "history";
  onTab: (t: "overview" | "history") => void;
  historyCount: number;
}) {
  const tabClass = (active: boolean) =>
    `-mb-px mr-[18px] border-b-2 py-1.5 text-xs transition-colors ${
      active
        ? "border-[var(--text)] text-[var(--text)]"
        : "border-transparent text-[var(--text-tertiary)] hover:text-[var(--text-secondary)]"
    }`;
  return (
    <div className="mt-[11px] flex">
      <button type="button" onClick={() => onTab("overview")} className={tabClass(tab === "overview")}>
        Overview
      </button>
      <button type="button" onClick={() => onTab("history")} className={tabClass(tab === "history")}>
        History
        {historyCount > 0 && (
          <span className="ml-1.5 font-mono text-[10px] text-[var(--text-ghost)]">{historyCount}</span>
        )}
      </button>
    </div>
  );
}

/** The History tab body: this node's durable committed-model timeline — every
 *  fold, drift reconcile, move and birth, newest first. Each event reads as a
 *  rail dot coloured by kind, a driver badge, attribution, and its diff rows
 *  (with inline source peeks for the claims an `impl` discharged). */
function NodeHistory({
  events,
  projectPath,
}: {
  events: readonly HistoryEvent[];
  projectPath: string | null;
}) {
  if (events.length === 0) {
    return (
      <div className="flex flex-col items-center gap-3 px-6 py-16 text-center">
        <FileClock className="h-6 w-6 text-[var(--text-ghost)]" />
        <p className="max-w-sm text-xs text-[var(--text-muted)]">
          No committed history yet. When the agent implements, reconciles drift, moves, or builds
          this node, it lands here.
        </p>
      </div>
    );
  }
  // Stored oldest-first (append-only); the timeline reads newest-first.
  const ordered = [...events].reverse();
  return (
    <div className="pt-5">
      {ordered.map((ev, i) => {
        const meta = EVENT_META[ev.kind];
        const last = i === ordered.length - 1;
        return (
          <div
            key={`${ev.at}-${i}`}
            className={`relative ml-1.5 border-l pl-6 pb-6 ${
              last ? "border-transparent" : "border-[var(--border)]"
            }`}
          >
            <span
              className="absolute -left-[5px] top-1 h-2.5 w-2.5 rounded-full"
              style={{ background: meta.dot, boxShadow: "0 0 0 2px var(--surface)" }}
            />
            <div className="mb-2 flex flex-wrap items-baseline gap-2">
              <span className="font-mono text-2xs tabular-nums text-[var(--text-tertiary)]">
                {relativeTime(ev.at)}
              </span>
              <span
                className={`rounded border px-1.5 py-px font-mono text-[10px] uppercase tracking-wide ${meta.badge}`}
              >
                {meta.label}
              </span>
              <span className="text-2xs text-[var(--text-muted)]">
                <span className="text-violet-500 dark:text-violet-400">⌁</span> {ev.by} · {ev.driver}
              </span>
            </div>
            <div className="flex flex-col gap-1">
              {ev.rows.map((row, j) => (
                <div key={j} className="grid grid-cols-[16px_1fr] items-baseline gap-1">
                  <span
                    className={`text-center font-mono text-xs font-bold ${markerColor(row.marker)}`}
                  >
                    {row.marker}
                  </span>
                  <div className="min-w-0">
                    <span className="font-mono text-xs leading-relaxed text-[var(--text-secondary)]">
                      {row.text}
                    </span>
                    {row.source && (
                      <ClaimSource locations={[row.source]} projectPath={projectPath} />
                    )}
                  </div>
                </div>
              ))}
            </div>
          </div>
        );
      })}
    </div>
  );
}

// --- node page --------------------------------------------------------------

function NodePageBody(props: PageProps & { node: Node }) {
  const {
    model,
    committed,
    node,
    report,
    editor,
    projectPath,
    onSelectNode,
    onFixture,
    history,
    driftScopes,
    onCheckDrift,
    onDismissDrift,
  } = props;
  const ed = useEditSections();
  const openMenu = usePageMenu();
  const copyId = useCopyId();
  const [tab, setTab] = useState<"overview" | "history">("overview");
  // The header edits (name, technology) commit live on blur, so Cancel needs a
  // snapshot to revert to. Captured when the title editor opens, restored on
  // Cancel. See onToggleName / onCancel below.
  const titleSnapshot = useRef<{ name: string; technology?: string } | null>(null);
  const openTitleEdit = () => {
    if (!ed.isEditing("title")) {
      titleSnapshot.current = { name: node.name, technology: node.technology };
    }
    ed.toggle("title");
  };
  const cancelTitleEdit = () => {
    const snap = titleSnapshot.current;
    if (snap) editor?.updateNode(node.id, { name: snap.name, technology: snap.technology });
    titleSnapshot.current = null;
    ed.toggle("title");
  };
  // This node's slice of the durable committed-model timeline.
  const nodeEvents = useMemo(
    () => history.filter((e) => e.nodeId === node.id),
    [history, node.id],
  );
  const tag = typeTag(node);
  const KindIcon = lookupIcon(node.icon) ?? kindIcon(node);

  const sourceMap = effectiveSourceMap(committed, model);
  const dataShape = isDataShape(node);
  const resps = node.responsibilities ?? [];
  // The committed copy of this node's claims — the diff base for the Overview.
  const committedResps =
    committed?.nodes.find((n) => n.id === node.id)?.responsibilities ?? [];
  const definition = sourceMap[node.id] ?? [];
  const hasChildren = model.nodes.some((n) => n.parentId === node.id);

  // Leaf claims must read through to code; structural nodes discharge through
  // their subtree, so their claims are never "unmapped". Persons (actors) and
  // externals are out-of-system — their claims are never code-backed.
  const leafHost = !hasChildren && !node.external && node.kind !== "person";

  // The node's own definition anchor — its file, surfaced in the type line.
  const defFile = definition[0]?.pattern;

  const staleCount = resps.filter((r) => r.stale).length;
  const vagrantCount = resps.filter((r) => r.vagrant).length;
  const drift = driftScopes.find((s) => s.nodeId === node.id);

  // Maintenance notices — full-width amboxes stacked at the top of the article
  // body (the wiki hatnote pattern), not chips crammed beside the title.
  const bannerStack =
    drift || node.stale || staleCount > 0 || vagrantCount > 0 || isNodeEmpty(node) ? (
      <>
        {node.stale && editor && (
          <Ambox
            tone="danger"
            icon={<Flag className="h-3 w-3" />}
            actions={
              <>
                <button
                  type="button"
                  onClick={() => editor.reimplementNode(node.id)}
                  title="The model is right — rebuild this whole subtree. Becomes a to-do the agent implements."
                  className={NOTICE_ACTION}
                >
                  Re-implement
                </button>
                <button
                  type="button"
                  onClick={() => editor.dropNode(node.id)}
                  title="The code was removed on purpose — drop this node and its subtree from the model."
                  className={NOTICE_ACTION}
                >
                  Drop
                </button>
              </>
            }
          >
            Backing code removed — this node and its subtree have no code
          </Ambox>
        )}
        {drift && (
          <Ambox
            tone="warning"
            icon={<GitCompare className="h-3 w-3" />}
            actions={
              <>
                {onCheckDrift && (
                  <button
                    type="button"
                    onClick={onCheckDrift}
                    title="Run a semantic drift check across the whole project"
                    className={NOTICE_ACTION}
                  >
                    Check
                  </button>
                )}
                {onDismissDrift && (
                  <button
                    type="button"
                    onClick={() => onDismissDrift(node.id)}
                    title="Mark this node and its children reconciled, without a semantic check"
                    className={NOTICE_ACTION}
                  >
                    Dismiss
                  </button>
                )}
              </>
            }
          >
            Code changed ({drift.changedFiles.length} file
            {drift.changedFiles.length === 1 ? "" : "s"}) — claims may not hold
          </Ambox>
        )}
        {staleCount > 0 && (
          <Ambox
            tone="warning"
            icon={<Flag className="h-3 w-3" />}
            actions={
              <button
                type="button"
                onClick={() => {
                  const first = resps.find((r) => r.stale);
                  if (first) jumpTo(respElementId(first.id));
                }}
                className={NOTICE_ACTION}
              >
                Review
              </button>
            }
          >
            {staleCount} stale claim{staleCount === 1 ? "" : "s"}
          </Ambox>
        )}
        {vagrantCount > 0 && (
          <Ambox
            tone="danger"
            icon={<Flag className="h-3 w-3" />}
            actions={
              <button
                type="button"
                onClick={() => {
                  const first = resps.find((r) => r.vagrant);
                  if (first) jumpTo(respElementId(first.id));
                }}
                className={NOTICE_ACTION}
              >
                Review
              </button>
            }
          >
            {vagrantCount} undescribed behaviour{vagrantCount === 1 ? "" : "s"} in code
          </Ambox>
        )}
        {isNodeEmpty(node) && (
          <Ambox tone="warning" icon={<CircleDashed className="h-3 w-3" />}>
            Empty symbol — no responsibilities or properties
          </Ambox>
        )}
      </>
    ) : null;

  return (
    <div className="flex min-w-0 flex-1 flex-col">
      <PageHeader
        crumbs={<Crumbs chain={ancestorChain(model, node.parentId)} onSelectNode={onSelectNode} />}
        name={node.name}
        typeLine={
          <>
            <KindIcon className="h-3.5 w-3.5" />
            <span>{dataShape ? "Data type" : tag.type}</span>
            {/* Technology — editable in place when the header is in edit mode,
                committing on blur; otherwise shown only when set. */}
            {ed.isEditing("title") ? (
              <>
                <span className="text-[var(--text-ghost)]">·</span>
                <Editable
                  initial={node.technology ?? ""}
                  placeholder="technology"
                  onCommit={(t) => editor?.updateNode(node.id, { technology: t.trim() || undefined })}
                  className="font-mono text-[var(--text-secondary)]"
                />
              </>
            ) : (
              node.technology && (
                <>
                  <span className="text-[var(--text-ghost)]">·</span>
                  <span className="font-mono">{node.technology}</span>
                </>
              )
            )}
            {defFile && (
              <>
                <span className="text-[var(--text-ghost)]">·</span>
                <button
                  type="button"
                  onClick={() => void invoke("open_in_editor", { file: defFile, line: definition[0]?.line ?? null, projectPath })}
                  title="Open in editor"
                  className="font-mono text-[var(--text-tertiary)] hover:text-blue-600 hover:underline dark:hover:text-blue-400"
                >
                  {defFile}
                </button>
              </>
            )}
            {isNodeEmpty(node) && <EmptyFlag />}
          </>
        }
        editor={editor}
        editingName={ed.isEditing("title")}
        onToggleName={openTitleEdit}
        onCancel={cancelTitleEdit}
        onRename={(v) => editor?.updateNode(node.id, { name: v })}
        // Symbol names are bound to code identifiers (shape, not length); every
        // other kind is a human-authored title with a length cap.
        nameMaxLength={node.kind === "symbol" ? undefined : NAME_MAX}
        nameSanitize={node.kind === "symbol" ? sanitizeIdentifier : undefined}
        tabs={<PageTabs tab={tab} onTab={setTab} historyCount={nodeEvents.length} />}
      />

      <div className="min-h-0 flex-1 overflow-y-auto">
        {tab === "history" ? (
          <div className="max-w-[900px] px-7 pb-[50px] pt-[18px]">
            <NodeHistory events={nodeEvents} projectPath={projectPath} />
          </div>
        ) : (
          <div className="flex gap-8 px-7 pb-[50px] pt-[18px]">
            <article
              className="min-w-0 max-w-[900px] flex-1"
              onContextMenu={(e) => openMenu(e, [copyIdItem(node.id, copyId)])}
            >
              {bannerStack && (
                <div className="mb-5 flex flex-col gap-2">{bannerStack}</div>
              )}
              <DescriptionSection
                value={node.description}
                prevValue={committed?.nodes.find((n) => n.id === node.id)?.description}
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
                  sourceFile={
                    sourceMap[node.id]?.[0]?.pattern ??
                    node.responsibilities
                      ?.map((r) => sourceMap[r.id]?.[0]?.pattern)
                      .find(Boolean)
                  }
                  onFixture={onFixture}
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
                  prevResps={committedResps}
                  sourceMap={sourceMap}
                  projectPath={projectPath}
                  leafHost={leafHost}
                  editor={editor}
                  editing={ed.isEditing("responsibilities")}
                  onToggle={() => ed.toggle("responsibilities")}
                  onSelectNode={onSelectNode}
                />
              )}

              {node.kind === "symbol" && (
                <PropertiesSection
                  node={node}
                  prevProps={committed?.nodes.find((n) => n.id === node.id)?.properties ?? []}
                  model={model}
                  onSelectNode={onSelectNode}
                  editor={editor}
                  editing={ed.isEditing("properties")}
                  onToggle={() => ed.toggle("properties")}
                />
              )}

              <ConnectionsSection
                model={model}
                committed={committed}
                node={node}
                report={report}
                editor={editor}
                editing={ed.isEditing("connections")}
                onToggle={() => ed.toggle("connections")}
                onSelectNode={onSelectNode}
              />

              <ImpliedConnectionsSection
                model={model}
                node={node}
                report={report}
                onSelectNode={onSelectNode}
              />
            </article>
            <NotesGutter
              node={node}
              model={model}
              editor={editor}
              editing={ed.isEditing("notes")}
              onToggle={() => ed.toggle("notes")}
              onSelectNode={onSelectNode}
            />
          </div>
        )}
      </div>
    </div>
  );
}

// --- group page -------------------------------------------------------------

function GroupPageBody(props: PageProps & { group: Group }) {
  const { model, committed, group, editor, projectPath, onSelectNode } = props;
  const ed = useEditSections();
  const openMenu = usePageMenu();
  const copyId = useCopyId();
  // Snapshot the group name on open so Cancel can revert the live-committed edit.
  const titleSnapshot = useRef<{ name: string } | null>(null);
  const openTitleEdit = () => {
    if (!ed.isEditing("title")) titleSnapshot.current = { name: group.name };
    ed.toggle("title");
  };
  const cancelTitleEdit = () => {
    const snap = titleSnapshot.current;
    if (snap) editor?.updateGroup(group.id, { name: snap.name });
    titleSnapshot.current = null;
    ed.toggle("title");
  };
  const members = group.memberIds
    .map((id) => model.nodes.find((n) => n.id === id))
    .filter((n): n is Node => Boolean(n));

  const sourceMap = effectiveSourceMap(committed, model);
  const resps = group.responsibilities ?? [];
  const committedResps =
    committed?.groups.find((g) => g.id === group.id)?.responsibilities ?? [];

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
          </>
        }
        editor={editor}
        editingName={ed.isEditing("title")}
        onToggleName={openTitleEdit}
        onCancel={cancelTitleEdit}
        onRename={(v) => editor?.updateGroup(group.id, { name: v })}
        nameMaxLength={NAME_MAX}
      />

      <div className="min-h-0 flex-1 overflow-y-auto">
        <div
          className="max-w-[900px] px-7 pb-[50px] pt-[18px]"
          onContextMenu={(e) => openMenu(e, [copyIdItem(group.id, copyId)])}
        >
            <DescriptionSection
              value={group.description}
              prevValue={committed?.groups.find((g) => g.id === group.id)?.description}
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
              prevResps={committedResps}
              sourceMap={sourceMap}
              projectPath={projectPath}
              leafHost={false} // group claims discharge through members
              editor={editor}
              editing={ed.isEditing("responsibilities")}
              onToggle={() => ed.toggle("responsibilities")}
              onSelectNode={onSelectNode}
            />

            <PageSection
              title="Members"
              count={members.length}
              editable={!!editor && members.length > 0}
              editing={ed.isEditing("members")}
              onToggleEdit={() => ed.toggle("members")}
            >
              {ed.isEditing("members") && editor ? (
                <MembersEditor
                  members={members}
                  editor={editor}
                  onSelectNode={onSelectNode}
                  onClose={() => ed.toggle("members")}
                />
              ) : members.length === 0 ? (
                <Empty>No members yet. Add nodes to this group from the tree.</Empty>
              ) : (
                <ol className="-mx-2 flex flex-col">
                  {members.map((m) => (
                    <MemberRow
                      key={m.id}
                      member={m}
                      onSelectNode={onSelectNode}
                    />
                  ))}
                </ol>
              )}
            </PageSection>
        </div>
      </div>
    </div>
  );
}

/** One group member as a mono row — the node as a blue wikilink in the shared
 *  marker/number grid, so the Members list reads as the same diff sheet as
 *  everything else on the page. */
function MemberRow({
  member,
  onSelectNode,
  onRemove,
}: {
  member: Node;
  onSelectNode: (id: string) => void;
  onRemove?: () => void;
}) {
  return (
    <li className="group/conn grid grid-cols-[18px_22px_1fr] items-baseline py-[1.5px]">
      <span className="select-none" />
      <span className="select-none" />
      <div className="flex min-w-0 items-baseline font-mono text-[12.5px] leading-[1.65]">
        <button
          type="button"
          onClick={() => onSelectNode(member.id)}
          title={member.name}
          className="shrink truncate text-left text-blue-700 hover:underline dark:text-blue-400"
        >
          {member.name || "Untitled"}
        </button>
        {onRemove && (
          <button
            type="button"
            title="Remove from group"
            onClick={onRemove}
            className="invisible ml-2 shrink-0 rounded p-0.5 text-[var(--text-ghost)] hover:text-red-400 group-hover/conn:visible"
          >
            <X className="h-3 w-3" />
          </button>
        )}
      </div>
    </li>
  );
}

/** The Members form inside the shared {@link SectionEditor} shell — Cancel/Done
 *  ride in the section header. Removals accumulate in a draft (the dropped row
 *  reads struck) and only fire `setNodeGroup(null)` on Done. */
function MembersEditor({
  members,
  editor,
  onSelectNode,
  onClose,
}: {
  members: Node[];
  editor: Editor;
  onSelectNode: (id: string) => void;
  onClose: () => void;
}) {
  const initialIds = members.map((m) => m.id);
  return (
    <SectionEditor<string[]>
      initial={initialIds}
      onCommit={(keptIds) => {
        const kept = new Set(keptIds);
        for (const id of initialIds) if (!kept.has(id)) editor.setNodeGroup(id, null);
      }}
      onClose={onClose}
    >
      {(draft, setDraft) => {
        const kept = members.filter((m) => draft.includes(m.id));
        const dropped = members.filter((m) => !draft.includes(m.id));
        if (members.length === 0) return <Empty>No members.</Empty>;
        return (
          <ol className="-mx-2 flex flex-col">
            {kept.map((m) => (
              <MemberRow
                key={m.id}
                member={m}
                onSelectNode={onSelectNode}
                onRemove={() => setDraft((d) => d.filter((id) => id !== m.id))}
              />
            ))}
            {dropped.map((m) => (
              <li
                key={m.id}
                className="grid grid-cols-[18px_22px_1fr] items-baseline py-[1.5px]"
              >
                <span className="select-none text-center font-mono text-xs font-bold text-red-600 dark:text-red-400">
                  −
                </span>
                <span className="select-none" />
                <div className="flex min-w-0 items-baseline gap-2 font-mono text-[12.5px] leading-[1.65]">
                  <span className="truncate text-[var(--text-muted)] line-through decoration-red-400/50">
                    {m.name || "Untitled"}
                  </span>
                  <button
                    type="button"
                    onClick={() => setDraft((d) => [...d, m.id])}
                    className={BTN}
                  >
                    Undo
                  </button>
                </div>
              </li>
            ))}
          </ol>
        );
      }}
    </SectionEditor>
  );
}

// --- description ------------------------------------------------------------

/** The lede — a title-less paragraph directly under the header, Wikipedia
 *  style, so the kind-specific hero (responsibilities / properties / preview)
 *  is the first titled section the eye lands on. Edits live in a draft and
 *  persist only on Save; Cancel/Esc discards. */
function DescriptionSection({
  value,
  prevValue,
  model,
  onSelectNode,
  editor,
  editing,
  onToggle,
  onCommit,
}: {
  value: string | undefined;
  /** The committed description — when it differs from `value`, the lede shows
   *  the reword inline (word-diff), like a claim. */
  prevValue?: string;
  model: ScryModel;
  onSelectNode: (id: string) => void;
  editor: Editor | undefined;
  editing: boolean;
  onToggle: () => void;
  onCommit: (v: string) => void;
}) {
  if (editing && editor) {
    // The lede edits in place: a contentEditable span with the SAME size/
    // leading/colour as the read paragraph, so the swap doesn't reflow.
    return (
      <SectionEditor<string> initial={value ?? ""} onCommit={onCommit} onClose={onToggle}>
        {(_draft, setDraft) => (
          <Editable
            initial={value ?? ""}
            autoFocus
            maxLength={DESCRIPTION_MAX}
            placeholder="Describe what this is. Link other nodes with [[Name]]."
            onInput={setDraft}
            className="block text-[13.5px] leading-[1.6] text-[var(--text-secondary)]"
          />
        )}
      </SectionEditor>
    );
  }
  // A reworded description (committed text present and differing) shows the
  // change inline; an added one (no committed text) reads plain — the node's
  // own diff marker already announces it's new.
  const reworded = !!value && prevValue !== undefined && prevValue !== "" && prevValue !== value;
  return (
    <div className="group/lede relative flow-root pr-16">
      <p
        className={`text-[13.5px] leading-[1.6] ${
          value ? "text-[var(--text-secondary)]" : "italic text-[var(--text-muted)]"
        }`}
      >
        {value ? (
          reworded ? (
            <WordDiffText from={prevValue!} to={value} />
          ) : (
            <WikiText text={value} nodes={model.nodes} onSelectNode={onSelectNode}/>
          )
        ) : (
          "No description."
        )}
      </p>
      {editor && (
        <EditLink
          editing={false}
          onClick={onToggle}
          className="invisible absolute right-0 top-0 group-hover/lede:visible"
        />
      )}
    </div>
  );
}

// --- notes gutter ------------------------------------------------------------

/** Render notes read-only: lines starting with `- ` (or `• `) group into a
 *  bullet list; everything else is a paragraph. Each line's prose runs through
 *  WikiText so `[[Name]]` links resolve. */
function NotesRead({
  text,
  nodes,
  onSelectNode,
}: {
  text: string;
  nodes: readonly Node[];
  onSelectNode: (id: string) => void;
}) {
  const blocks: React.ReactNode[] = [];
  let bullets: string[] = [];
  const flush = () => {
    if (!bullets.length) return;
    const items = bullets;
    blocks.push(
      <ul key={blocks.length} className="list-disc space-y-0.5 pl-4">
        {items.map((b, i) => (
          <li key={i}>
            <WikiText text={b} nodes={nodes} onSelectNode={onSelectNode} />
          </li>
        ))}
      </ul>,
    );
    bullets = [];
  };
  for (const line of text.split("\n")) {
    const m = line.match(/^\s*[-•]\s+(.*)$/);
    if (m) {
      bullets.push(m[1]);
    } else {
      flush();
      if (line.trim())
        blocks.push(
          <p key={blocks.length}>
            <WikiText text={line} nodes={nodes} onSelectNode={onSelectNode} />
          </p>,
        );
    }
  }
  flush();
  return <div className="flex flex-col gap-1.5">{blocks}</div>;
}

/** Invisible in-place notes editor: a transparent, borderless, auto-growing
 *  textarea with the same metrics as the read view (so the swap doesn't
 *  reflow). Enter on a non-empty bullet line auto-continues the list with a new
 *  `- `; Shift+Enter is always a plain newline. */
function NotesEditable({
  initial,
  onInput,
}: {
  initial: string;
  onInput: (text: string) => void;
}) {
  const ref = useRef<HTMLTextAreaElement>(null);
  const grow = (el: HTMLTextAreaElement) => {
    el.style.height = "auto";
    el.style.height = `${el.scrollHeight}px`;
  };
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    grow(el);
    el.focus();
    el.setSelectionRange(el.value.length, el.value.length);
  }, []);
  return (
    <textarea
      ref={ref}
      defaultValue={initial}
      rows={2}
      placeholder="Notes to self. Start a line with “- ” for a bullet. Link nodes with [[Name]]."
      onInput={(e) => {
        grow(e.currentTarget);
        onInput(e.currentTarget.value);
      }}
      onKeyDown={(e) => {
        if (e.key !== "Enter" || e.shiftKey) return;
        const ta = e.currentTarget;
        const lineStart = ta.value.lastIndexOf("\n", ta.selectionStart - 1) + 1;
        const line = ta.value.slice(lineStart, ta.selectionStart);
        const m = line.match(/^\s*[-•]\s+(.*)$/);
        if (m && m[1].trim() !== "") {
          e.preventDefault();
          ta.setRangeText("\n- ", ta.selectionStart, ta.selectionEnd, "end");
          grow(ta);
          onInput(ta.value);
        }
      }}
      className="w-full resize-none whitespace-pre-wrap bg-transparent text-[12.5px] leading-[1.6] text-[var(--text-secondary)] outline-none placeholder:text-[var(--text-muted)]"
    />
  );
}

/**
 * The right-margin gutter: the user's own freeform notes about this node —
 * self-context and traversal aids, NOT part of the spec (distinct from the
 * description and from a responsibility's directives). Supports `[[Name]]`
 * wikilinks like every prose field. User-only; the agent never authors it.
 * Node-only (groups carry no `notes`).
 */
function NotesGutter({
  node,
  model,
  editor,
  editing,
  onToggle,
  onSelectNode,
}: {
  node: Node;
  model: ScryModel;
  editor: Editor | undefined;
  editing: boolean;
  onToggle: () => void;
  onSelectNode: (id: string) => void;
}) {
  const notes = node.notes;
  return (
    <aside className="ml-auto hidden w-[240px] shrink-0 lg:block">
      <div className="group/gutter sticky top-0">
        <div className="mb-2 flex items-end justify-between gap-2 border-b border-[var(--border)] pb-[5px]">
          <h2 className="text-[11px] font-semibold uppercase tracking-[0.07em] text-[var(--text-tertiary)]">
            Notes
          </h2>
          {editor && !editing && (
            <EditLink
              editing={false}
              onClick={onToggle}
              className="invisible group-hover/gutter:visible"
            />
          )}
        </div>
        {editing && editor ? (
          <SectionEditor<string>
            initial={notes ?? ""}
            onCommit={(v) => editor.updateNode(node.id, { notes: v.trim() || undefined })}
            onClose={onToggle}
          >
            {(_draft, setDraft) => (
              <NotesEditable initial={notes ?? ""} onInput={setDraft} />
            )}
          </SectionEditor>
        ) : notes ? (
          <div className="text-[12.5px] leading-[1.6] text-[var(--text-secondary)]">
            <NotesRead text={notes} nodes={model.nodes} onSelectNode={onSelectNode} />
          </div>
        ) : (
          <p className="text-[12.5px] italic leading-[1.6] text-[var(--text-muted)]">
            No notes.
          </p>
        )}
      </div>
    </aside>
  );
}


// --- responsibilities (the diff view) ---------------------------------------

/** How one claim diverges from the committed model. The Overview reads as a
 *  diff: `added` (in the plan, not yet committed), `reworded` (statement or
 *  directives moved), `deleted` (committed but dropped from the plan, shown so
 *  it can be restored), `vagrant` (code does it, the model never claimed it —
 *  adopt or reject), or `unchanged`. */
type RespDiffKind = "added" | "reworded" | "deleted" | "vagrant" | "unchanged";

interface RespDiffRow {
  resp: Responsibility;
  kind: RespDiffKind;
  /** The committed version, for word-diffing a reworded statement/directives. */
  prev?: Responsibility;
  /** Display number — null for deleted rows (they're no longer in the list). */
  index: number | null;
}

const RESP_MARK: Record<Exclude<RespDiffKind, "unchanged">, { glyph: string; color: string }> = {
  added: { glyph: "+", color: "text-emerald-600 dark:text-emerald-400" },
  reworded: { glyph: "~", color: "text-amber-600 dark:text-amber-400" },
  deleted: { glyph: "−", color: "text-red-600 dark:text-red-400" },
  vagrant: { glyph: "?", color: "text-violet-600 dark:text-violet-400" },
};

/** Build the diff rows for a host's claims: planned claims in order (each tagged
 *  added / reworded / vagrant / unchanged against the committed copy), then any
 *  committed claims the plan dropped, as restorable `deleted` rows. */
function buildRespDiff(planned: Responsibility[], committed: Responsibility[]): RespDiffRow[] {
  const prevById = new Map(committed.map((r) => [r.id, r]));
  const liveIds = new Set(planned.map((r) => r.id));
  const rows: RespDiffRow[] = [];
  let n = 0;
  for (const r of planned) {
    const prev = prevById.get(r.id);
    let kind: RespDiffKind;
    if (r.vagrant) kind = "vagrant";
    else if (!prev) kind = "added";
    else if (
      prev.statement !== r.statement ||
      (prev.directives ?? []).join("\n") !== (r.directives ?? []).join("\n")
    )
      kind = "reworded";
    else kind = "unchanged";
    // Vagrant claims aren't part of the numbered contract yet (they await a
    // verdict); everything else takes the next sequence number.
    rows.push({ resp: r, kind, prev, index: kind === "vagrant" ? null : ++n });
  }
  for (const r of committed)
    if (!liveIds.has(r.id)) rows.push({ resp: r, kind: "deleted", index: null });
  return rows;
}

/** Render text with word-level add/remove highlighting (a reworded claim). When
 *  `from`/`to` are equal it's just the plain text. */
function WordDiffText({ from, to }: { from: string; to: string }) {
  const segs = wordDiff(from, to);
  return (
    <>
      {segs.map((s, i) => {
        // A substitution (removed word immediately followed by its replacement)
        // glues the two together — "forin" — because the tokenizer keeps the
        // surrounding spaces in the unchanged runs, leaving none between the
        // changed words. Reinsert that gap so the strike and the pill read apart.
        const sep = i > 0 && segs[i - 1].kind !== "equal" && s.kind !== "equal";
        return (
          <Fragment key={i}>
            {sep && " "}
            {s.kind === "equal" ? (
              <span>{s.text}</span>
            ) : s.kind === "added" ? (
              <span className="rounded-[2px] bg-emerald-500/15 px-px text-emerald-700 dark:text-emerald-300">
                {s.text}
              </span>
            ) : (
              <del className="text-red-600/90 decoration-red-400/60 dark:text-red-400/90">
                {s.text}
              </del>
            )}
          </Fragment>
        );
      })}
    </>
  );
}

function ResponsibilitiesSection({
  model,
  host,
  hostId,
  resps,
  prevResps,
  sourceMap,
  projectPath,
  leafHost,
  editor,
  editing,
  onToggle,
  onSelectNode,
}: {
  model: ScryModel;
  host: "node" | "group";
  hostId: string;
  resps: Responsibility[];
  /** The committed copy of this host's claims — the diff base for the rows. */
  prevResps: Responsibility[];
  /** respId → source locations, for the inline `↳ file:range` peeks per claim. */
  sourceMap: Record<string, SourceLocation[]>;
  projectPath: string | null;
  /** Whether claims here must anchor to source (leaf node). Structural hosts
   *  discharge through their subtree and never flag "unmapped". */
  leafHost: boolean;
  editor: Editor | undefined;
  editing: boolean;
  onToggle: () => void;
  onSelectNode: (id: string) => void;
}) {
  const diffRows = buildRespDiff(resps, prevResps);
  /** Restore a dropped claim by putting the committed copy back into the plan. */
  const restore = (r: Responsibility) => {
    if (!editor) return;
    const next = [...resps, r];
    if (host === "node") editor.updateNode(hostId, { responsibilities: next });
    else editor.updateGroup(hostId, { responsibilities: next });
  };
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
      ) : diffRows.length === 0 ? (
        <Empty>No responsibilities.</Empty>
      ) : (
        <ol className="-mx-2 flex flex-col">
          {diffRows.map((row) => (
            <RespDiffRow
              key={row.resp.id}
              model={model}
              row={row}
              host={host}
              locations={sourceMap[row.resp.id] ?? []}
              projectPath={projectPath}
              leafHost={leafHost}
              onSelectNode={onSelectNode}
              onRestore={() => restore(row.resp)}
              editor={editor}
            />
          ))}
        </ol>
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
    ? [...initial, { id: seededId, statement: "" }]
    : initial;

  const commit = (draft: Responsibility[]) => {
    // A blank statement is invalid (the backend flags it), so drop any empty
    // row on commit — new or existing. Clearing a statement deletes the claim;
    // the removal loop below calls removeResponsibility for dropped existing rows.
    const cleaned = draft
      .filter((r) => r.statement.trim() !== "")
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
              { id: nextResponsibilityId(d), statement: "" },
            ])
          }
          className={BTN}
        >
          Add responsibility
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
          <ul className="-mx-2 flex flex-col">
            {draft.map((r, i) => (
              <ResponsibilityEditRow
                key={r.id}
                resp={r}
                index={i + 1}
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
 * One claim, rendered as a diff row against the committed model: a marker
 * (+ added / ~ reworded / − deleted / ? vagrant / blank unchanged), the
 * sequence number, the statement (word-diffed when reworded), its directives
 * (new ones flagged +), footnote citation chips, and the inline verdict actions
 * the row's kind calls for (adopt/reject a vagrant, restore a deletion, clear a
 * stale flag). No status pill — the marker carries the lifecycle now.
 */
function RespDiffRow({
  model,
  row,
  host,
  locations,
  projectPath,
  leafHost,
  onSelectNode,
  onRestore,
  editor,
}: {
  model: ScryModel;
  row: RespDiffRow;
  host: "node" | "group";
  /** This claim's source locations — rendered inline with expandable peeks. */
  locations: SourceLocation[];
  projectPath: string | null;
  leafHost: boolean;
  onSelectNode: (id: string) => void;
  onRestore: () => void;
  editor: Editor | undefined;
}) {
  const { resp, kind, prev, index } = row;
  const openMenu = usePageMenu();
  const copyId = useCopyId();
  const mark = kind === "unchanged" ? null : RESP_MARK[kind];
  const deleted = kind === "deleted";
  const directives = resp.directives ?? [];
  const prevDirs = prev?.directives ?? [];
  // Directives the plan dropped from a kept claim — shown struck so the removal
  // is visible, like a deleted line.
  const removedDirs =
    kind === "reworded" || kind === "unchanged" ? prevDirs.filter((d) => !directives.includes(d)) : [];
  const reviewable = kind === "vagrant" && host === "node" && editor;
  // A LEAF claim that's believed code-backed (committed: unchanged or reworded)
  // but anchors to no source is a blind spot. Added/vagrant claims are plan-only
  // or code-first, so they're never "unmapped".
  const unmapped = leafHost && locations.length === 0 && (kind === "unchanged" || kind === "reworded");
  const hasMeta = resp.stale === true || unmapped;

  const contentColor = deleted
    ? "text-[var(--text-muted)]"
    : kind === "unchanged"
      ? "text-[var(--text-secondary)]"
      : "text-[var(--text)]";

  return (
    <>
    <li
      id={respElementId(resp.id)}
      onContextMenu={(e) => openMenu(e, [copyIdItem(resp.id, copyId)])}
      className={`${RESP_ROW} rounded-sm py-[1.5px] [&:not(:first-child)]:mt-2.5`}
    >
      <span
        className={`select-none text-center font-mono text-xs font-bold ${mark?.color ?? "text-[var(--text-ghost)]"}`}
      >
        {mark?.glyph}
      </span>
      <span className="select-none pr-2.5 text-right font-mono text-[11px] tabular-nums text-[var(--text-ghost)]">
        {index}
      </span>
      <div className="min-w-0 pr-[180px] font-mono text-[12.5px] leading-[1.65]">
        <span className={contentColor}>
          {resp.statement ? (
            kind === "reworded" && prev ? (
              <WordDiffText from={prev.statement} to={resp.statement} />
            ) : (
              <WikiText text={resp.statement} nodes={model.nodes} onSelectNode={onSelectNode}/>
            )
          ) : (
            <span className="italic text-[var(--text-ghost)]">Untitled responsibility</span>
          )}
        </span>

        <ClaimSource locations={locations} projectPath={projectPath} deleted={deleted} />

        {hasMeta && (
          <span className="mt-1 flex flex-wrap items-center gap-x-2.5 gap-y-0.5 text-2xs">
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
          </span>
        )}

        {/* Verdict actions, inline where the row needs one — controls in their
            own lane, off the mono content. */}
        {resp.stale && editor && (
          <div className="mt-1.5 flex flex-wrap items-center gap-2 text-[11px]">
            <span className="text-[var(--text-tertiary)]">Drift says the code no longer does this —</span>
            <button
              type="button"
              onClick={() => editor.reimplementResponsibility(resp.id)}
              className={BTN_GO}
              title="The model is right — rebuild the code. Becomes a to-do the agent implements (folds back when done)."
            >
              Re-implement
            </button>
            <button
              type="button"
              onClick={() => editor.dropResponsibility(resp.id)}
              className={BTN_DANGER}
              title="The behaviour was removed on purpose — drop the claim from the model."
            >
              Drop
            </button>
          </div>
        )}
        {reviewable && (
          <div className="mt-1.5 flex flex-wrap items-center gap-2 text-[11px]">
            <span className="text-[var(--text-tertiary)]">In the code, not in the model —</span>
            <button
              type="button"
              onClick={() => editor!.adoptResponsibility(resp.id)}
              className={BTN_GO}
              title="Accept this discovered behaviour into the contract — commits it to the model now (the code already exists)"
            >
              Adopt
            </button>
            <button
              type="button"
              onClick={() => editor!.rejectResponsibility(resp.id)}
              className={BTN_DANGER}
              title="Mark the code for deletion — this behaviour is not wanted (folds it in, then schedules its removal)"
            >
              Reject
            </button>
          </div>
        )}
        {deleted && editor && (
          <div className="mt-1.5 flex flex-wrap items-center gap-2 text-[11px]">
            <span className="text-[var(--text-tertiary)]">Removed from the plan —</span>
            <button type="button" onClick={onRestore} className={BTN_GO} title="Put this committed claim back into the plan">
              Restore
            </button>
          </div>
        )}
      </div>
    </li>

      {/* Directives — each its own grid row so the +/− marker aligns in the
          page's marker lane, like the mockup's `.row.dir`. */}
      {directives.map((d, i) => {
        const added = !!prev && !prevDirs.includes(d) && !deleted;
        return (
          <li key={`d${i}`} className={`${RESP_ROW} py-[0.5px]`}>
            <span
              className={`select-none text-center font-mono text-xs font-bold ${
                added ? "text-emerald-600 dark:text-emerald-400" : "text-[var(--text-ghost)]"
              }`}
            >
              {added ? "+" : ""}
            </span>
            <span className="select-none" />
            <div
              className={`flex min-w-0 items-baseline gap-1.5 pr-[180px] font-mono text-[12.5px] leading-[1.65] ${
                added ? "text-emerald-600 dark:text-emerald-400" : "text-[var(--text-tertiary)]"
              }`}
            >
              <CornerDownRight className="h-3 w-3 shrink-0 translate-y-px not-italic text-[var(--text-ghost)]" />
              <span className="min-w-0">
                <WikiText text={d} nodes={model.nodes} onSelectNode={onSelectNode}/>
              </span>
            </div>
          </li>
        );
      })}
      {removedDirs.map((d, i) => (
        <li key={`rd${i}`} className={`${RESP_ROW} py-[0.5px]`}>
          <span className="select-none text-center font-mono text-xs font-bold text-red-600 dark:text-red-400">
            −
          </span>
          <span className="select-none" />
          <div className="flex min-w-0 items-baseline gap-1.5 pr-[180px] font-mono text-[12.5px] leading-[1.65] text-[var(--text-tertiary)]">
            <CornerDownRight className="h-3 w-3 shrink-0 translate-y-px not-italic text-[var(--text-ghost)]" />
            <del className="min-w-0 decoration-red-400/50">
              <WikiText text={d} nodes={model.nodes} onSelectNode={onSelectNode}/>
            </del>
          </div>
        </li>
      ))}
    </>
  );
}

/** One draft row of the responsibilities form — fully controlled; every
 *  change lands in the section draft, never directly in the model. */
function ResponsibilityEditRow({
  resp,
  index,
  autoFocus,
  onPatch,
  onRemove,
}: {
  resp: Responsibility;
  index: number;
  autoFocus: boolean;
  onPatch: (id: string, patch: Partial<Responsibility>) => void;
  onRemove: (id: string) => void;
}) {
  const directives = resp.directives ?? [];
  const setDirectives = (next: string[]) => onPatch(resp.id, { directives: next });

  // In-place edit row: each line is a contentEditable span flowing in the same
  // content cell as the read diff row, with the SAME font/size/line-height, so
  // read↔edit stays the same width. The statement and each directive are their
  // own hover-scoped line (`/srow`, `/drow`): the full cell highlights on hover
  // and its controls (CTL) float over the right edge with a gradient fade.
  return (
    <li className={`group/erow ${RESP_ROW} py-[1.5px] [&:not(:first-child)]:mt-2.5`}>
      <span className="select-none text-center font-mono text-xs" />
      <span className="select-none pr-2.5 text-right font-mono text-[11px] tabular-nums text-[var(--text-ghost)]">
        {index}
      </span>
      <div className="min-w-0 font-mono text-[12.5px] leading-[1.65]">
        <div className="group/srow relative">
          <Editable
            initial={resp.statement}
            autoFocus={autoFocus}
            placeholder="Verb-led statement of accountability"
            onInput={(t) => onPatch(resp.id, { statement: t })}
            className={`block !pr-[180px] text-[var(--text)] ${STMT_HL}`}
          />
          <span className={CTL_SROW}>
            <button
              type="button"
              onClick={() => setDirectives([...directives, ""])}
              className={BTN}
            >
              Directive
            </button>
            <button
              type="button"
              title="Delete responsibility"
              onClick={() => onRemove(resp.id)}
              className={BTN_DANGER}
            >
              Delete
            </button>
          </span>
        </div>

        {directives.length > 0 && (
          <div className="mt-0.5 flex flex-col gap-0.5">
            {directives.map((d, i) => (
              <div
                key={i}
                className="group/drow relative flex items-baseline gap-1.5 text-[var(--text-tertiary)]"
              >
                <CornerDownRight className="h-3 w-3 shrink-0 translate-y-px not-italic text-[var(--text-ghost)]" />
                <Editable
                  initial={d}
                  autoFocus={d === ""}
                  placeholder={'directive — "must …" / "never …"'}
                  onInput={(t) => {
                    const next = directives.slice();
                    next[i] = t;
                    setDirectives(next);
                  }}
                  className={`block min-w-0 flex-1 !pr-[180px] ${DIR_HL}`}
                />
                <span className={CTL_DROW}>
                  <button
                    type="button"
                    title="Remove directive"
                    onClick={() => {
                      const next = directives.slice();
                      next.splice(i, 1);
                      setDirectives(next);
                    }}
                    className={BTN_DANGER}
                  >
                    Delete
                  </button>
                </span>
              </div>
            ))}
          </div>
        )}
      </div>
    </li>
  );
}

// --- properties (data shapes) -----------------------------------------------

/** A property's divergence from the committed model, matched by label (props
 *  carry no stable id): `added`, `reworded` (description changed), `deleted`
 *  (committed but dropped), or `unchanged`. */
type PropDiffKind = "added" | "reworded" | "deleted" | "unchanged";

interface PropDiffRow {
  prop: SchemaProperty;
  kind: PropDiffKind;
  prev?: SchemaProperty;
  index: number | null;
}

const propKey = (label: string) => label.trim().toLowerCase();

function buildPropDiff(planned: SchemaProperty[], committed: SchemaProperty[]): PropDiffRow[] {
  const prevByKey = new Map(committed.map((p) => [propKey(p.label), p]));
  const liveKeys = new Set(planned.map((p) => propKey(p.label)));
  const rows: PropDiffRow[] = [];
  let n = 0;
  for (const p of planned) {
    const prev = prevByKey.get(propKey(p.label));
    let kind: PropDiffKind;
    if (!prev) kind = "added";
    else if (prev.description !== p.description) kind = "reworded";
    else kind = "unchanged";
    rows.push({ prop: p, kind, prev, index: ++n });
  }
  for (const p of committed)
    if (!liveKeys.has(propKey(p.label))) rows.push({ prop: p, kind: "deleted", index: null });
  return rows;
}

/** One property as a mono diff row — the field name, then its description after
 *  an em-dash (word-diffed when reworded), keyed to the shared marker/number
 *  grid so it reads as part of the same diff sheet as the responsibilities. */
function PropDiffRow({
  row,
  model,
  onSelectNode,
}: {
  row: PropDiffRow;
  model: ScryModel;
  onSelectNode: (id: string) => void;
}) {
  const { prop, kind, prev, index } = row;
  const mark = kind === "unchanged" ? null : RESP_MARK[kind];
  const deleted = kind === "deleted";
  const desc = prop.description ?? "";
  const contentColor = deleted
    ? "text-[var(--text-muted)]"
    : kind === "unchanged"
      ? "text-[var(--text-secondary)]"
      : "text-[var(--text)]";
  return (
    <li className={`${PROP_ROW} rounded-sm py-[1.5px]`}>
      <span
        className={`select-none text-center font-mono text-xs font-bold ${mark?.color ?? "text-[var(--text-ghost)]"}`}
      >
        {mark?.glyph}
      </span>
      <span className="select-none pr-2.5 text-right font-mono text-[11px] tabular-nums text-[var(--text-ghost)]">
        {index}
      </span>
      <div className="min-w-0 font-mono text-[12.5px] leading-[1.65]">
        <span className={`font-medium ${contentColor}`}>{prop.label || "field"}</span>
        {(desc || (kind === "reworded" && prev?.description)) && (
          <span className="text-[var(--text-tertiary)]">
            {" "}—{" "}
            {kind === "reworded" && prev ? (
              <WordDiffText from={prev.description ?? ""} to={desc} />
            ) : (
              <WikiText text={desc} nodes={model.nodes} onSelectNode={onSelectNode}/>
            )}
          </span>
        )}
      </div>
    </li>
  );
}

/** One draft row of the properties form — field + description as flush ghost
 *  fields in the mono content lane, with a hover-revealed delete in its own
 *  control lane (identical grid to {@link PropDiffRow}, so read↔edit doesn't
 *  reflow). */
function PropertyEditRow({
  prop,
  index,
  autoFocus,
  onPatch,
  onRemove,
}: {
  prop: SchemaProperty;
  index: number;
  autoFocus: boolean;
  onPatch: (patch: Partial<SchemaProperty>) => void;
  onRemove: () => void;
}) {
  const FIELD_HL =
    "group-hover/erow:bg-[color-mix(in_srgb,var(--text)_6%,transparent)] focus:bg-[var(--surface-active)]";
  // Mirrors PropDiffRow's content cell — `label — description` inline in the
  // mono lane — with the two fields as contentEditable spans and the delete in
  // the row's reserved trailing control column (CTL).
  return (
    <li className={`group/erow ${PROP_ROW} py-[1.5px]`}>
      <span className="select-none text-center font-mono text-xs" />
      <span className="select-none pr-2.5 text-right font-mono text-[11px] tabular-nums text-[var(--text-ghost)]">
        {index}
      </span>
      <div className="min-w-0 font-mono text-[12.5px] leading-[1.65]">
        <Editable
          initial={prop.label}
          autoFocus={autoFocus}
          placeholder="field"
          onInput={(t) => onPatch({ label: t })}
          className={`font-medium text-[var(--text)] ${FIELD_HL}`}
        />
        <span className="text-[var(--text-tertiary)]"> — </span>
        <Editable
          initial={prop.description ?? ""}
          placeholder="description"
          onInput={(t) => onPatch({ description: t })}
          className={`text-[var(--text-tertiary)] ${FIELD_HL}`}
        />
      </div>
      <span className={CTL}>
        <button type="button" title="Delete property" onClick={onRemove} className={BTN_DANGER}>
          <Trash2 className="h-3 w-3" /> Delete
        </button>
      </span>
    </li>
  );
}

function PropertiesSection({
  node,
  prevProps,
  model,
  onSelectNode,
  editor,
  editing,
  onToggle,
}: {
  node: Node;
  /** The committed copy of this symbol's properties — the diff base. */
  prevProps: SchemaProperty[];
  model: ScryModel;
  onSelectNode: (id: string) => void;
  editor: Editor | undefined;
  editing: boolean;
  onToggle: () => void;
}) {
  const properties = node.properties ?? [];
  const diffRows = buildPropDiff(properties, prevProps);

  return (
    <PageSection
      title="Properties"
      count={properties.length}
      editable={!!editor}
      editing={editing}
      onToggleEdit={onToggle}
    >
      {editing && editor ? (
        <SectionEditor<SchemaProperty[]>
          initial={properties}
          onCommit={(draft) => {
            const cleaned = draft
              .filter((p) => p.label.trim() !== "" || (p.description ?? "").trim() !== "")
              .map((p) => ({ ...p, label: p.label.trim() }));
            editor.updateNode(node.id, { properties: cleaned });
          }}
          onClose={onToggle}
          footerExtra={(setDraft) => (
            <button
              type="button"
              onClick={() =>
                setDraft((d) => [...d, { label: "", description: "" }])
              }
              className={BTN}
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
              <ul className="-mx-2 flex flex-col">
                {draft.map((p, i) => (
                  <PropertyEditRow
                    key={i}
                    prop={p}
                    index={i + 1}
                    autoFocus={p.label === "" && i === draft.length - 1}
                    onPatch={(patch) => patchRow(i, patch)}
                    onRemove={() => setDraft((d) => d.filter((_, j) => j !== i))}
                  />
                ))}
              </ul>
            );
          }}
        </SectionEditor>
      ) : diffRows.length === 0 ? (
        <Empty>No properties.</Empty>
      ) : (
        <ol className="-mx-2 flex flex-col">
          {diffRows.map((row, i) => (
            <PropDiffRow key={i} row={row} model={model} onSelectNode={onSelectNode} />
          ))}
        </ol>
      )}
    </PageSection>
  );
}

// --- visual preview ---------------------------------------------------------

function PreviewSection({
  node,
  projectPath,
  sourceFile,
  onFixture,
  variationState,
  onStartVariation,
  onAcceptVariation,
  onDiscardVariations,
  onSelectVariation,
}: {
  node: Node;
  projectPath: string | null;
  /** The node's anchored source file (from the source map), used to pick the
   *  matching component export on the preview server. */
  sourceFile?: string;
  onFixture?: (nodeId: string, renderStatus: string, renderError: string | null) => void;
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

  // Deterministic render: the shared dev server serves any component as a
  // virtual entry with synthesized props — no agent, no per-component build.
  const server = usePreviewServer(projectPath);
  const entry = server.components
    ? matchPreviewComponent(server.components, node.name, sourceFile)
    : null;
  // Scryer's chrome theme drives the checkerboard backdrop (the `canvas` param).
  const isDark = useDarkMode();
  // The previewed component has its OWN theme, decoupled from scryer's chrome:
  // it defaults to the chrome theme when the page opens, then is independent so
  // toggling scryer no longer drags the component with it. Best-effort — see the
  // preview server's `previewHtml` for what this can and can't theme.
  const [componentDark, setComponentDark] = useState(isDark);
  const theme = componentDark ? "dark" : "light";
  const canvasTheme = isDark ? "dark" : "light";

  // An accepted variation (design intent, status `changed`) overrides the
  // live component until the real code catches up.
  const accepted = node.appearance?.distPath?.endsWith(".tsx")
    ? node.appearance.distPath
    : null;
  const previewUrl = (file: string, exportName: string, fixture?: string) =>
    `${server.url}/__preview?file=${encodeURIComponent(file)}&export=${encodeURIComponent(exportName)}` +
    (fixture ? `&fixture=${encodeURIComponent(fixture)}` : "") +
    `&theme=${theme}` +
    `&canvas=${canvasTheme}`;

  const watched: { file: string; exportName: string } | null = accepted
    ? { file: accepted, exportName: "default" }
    : entry
      ? { file: entry.file, exportName: entry.exportName }
      : null;
  const iframeSrc =
    server.url && watched
      ? previewUrl(watched.file, watched.exportName, accepted ? undefined : `.scryer/preview/fixtures/${node.id}.tsx`)
      : null;

  // The preview entry posts its render verdict (ok/empty/error) to the parent
  // window — this drives the B5 "generate preview data" repair path.
  const [report, setReport] = useState<{ status: string; error: string | null; hasFixture: boolean } | null>(null);
  useEffect(() => setReport(null), [watched?.file, watched?.exportName]);
  useEffect(() => {
    const onMessage = (e: MessageEvent) => {
      const d = e.data;
      if (d?.type !== "scryer-render" || !watched || d.file !== watched.file || d.exportName !== watched.exportName) return;
      setReport({ status: d.status, error: d.error ?? null, hasFixture: !!d.hasFixture });
    };
    window.addEventListener("message", onMessage);
    return () => window.removeEventListener("message", onMessage);
  }, [watched?.file, watched?.exportName]);

  const placeholder =
    server.status === "error"
      ? `Preview server failed: ${server.error}`
      : server.status === "starting" || !server.components
        ? "Starting preview server…"
        : "Can't preview this yet — only web (React/TSX) components render for now.";

  const canEdit = iframeSrc && onStartVariation;
  const needsRepair = report != null && (report.status === "empty" || report.status === "error");
  // No real data behind the preview (no per-node fixture, no type-keyed shared
  // fixture) — offer to generate one even when the render is "ok". Skipped for
  // accepted variations, which render from their own module without fixtures.
  const noFixture = report != null && report.hasFixture === false && !accepted;

  const variationSrcFor = (idx: number) =>
    previewUrl(`.scryer/preview/variations/${node.id}/${idx}.tsx`, "default");

  return (
    <PageSection
      title="Preview"
      editable={!!canEdit}
      editing={modalOpen}
      onToggleEdit={() => setModalOpen(!modalOpen)}
      right={
        iframeSrc ? (
          <button
            type="button"
            onClick={() => setComponentDark((d) => !d)}
            title={`Preview the component in ${componentDark ? "light" : "dark"} mode`}
            className="rounded p-1 text-[var(--text-tertiary)] hover:bg-[var(--surface-hover)]"
          >
            {componentDark ? <Sun className="h-3.5 w-3.5" /> : <Moon className="h-3.5 w-3.5" />}
          </button>
        ) : undefined
      }
    >
      {iframeSrc ? (
        <div className="flex flex-col gap-2">
          <div className="relative overflow-hidden rounded-md border border-[var(--border)]">
            <iframe
              src={iframeSrc}
              title={`Preview: ${node.name}`}
              className="h-[400px] w-full border-0"
              sandbox="allow-scripts allow-same-origin"
            />
            {/* The dev server compiles the component (and its deps) on the
                first hit — 5–10s cold. Cover the blank iframe until the entry
                posts its first render verdict. */}
            {report == null && (
              <div className="absolute inset-0 flex flex-col items-center justify-center gap-2 bg-[var(--surface-canvas)]">
                <Loader2 className="h-5 w-5 animate-spin text-violet-500 dark:text-violet-400" />
                <span className="text-2xs text-[var(--text-muted)]">Loading preview…</span>
              </div>
            )}
          </div>
          {(needsRepair || noFixture) && onFixture && (
            <div className="flex items-center gap-3 self-start">
              <span className="text-2xs text-[var(--text-muted)]">
                {report!.status === "empty"
                  ? "Rendered empty with placeholder props."
                  : report!.status === "error"
                    ? "Render failed with placeholder props."
                    : "Showing placeholder props — no fixture data yet."}
              </span>
              <button
                type="button"
                onClick={() => onFixture(node.id, report!.status, report!.error)}
                className={BTN_AGENT}
              >
                <Sparkles className="h-3 w-3" /> Generate preview data
              </button>
            </div>
          )}
        </div>
      ) : (
        <div className="flex flex-col items-center gap-3 rounded-md border border-dashed border-[var(--border)] bg-[var(--surface-raised)] px-6 py-10">
          <Eye className="h-6 w-6 text-[var(--text-ghost)]" />
          <p className="text-xs text-[var(--text-muted)]">{placeholder}</p>
        </div>
      )}

      {modalOpen && iframeSrc && (
        <VariationModal
          node={node}
          currentSrc={iframeSrc}
          variationSrc={variationSrcFor}
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
  currentSrc,
  variationSrc,
  variationState,
  onStartVariation,
  onAcceptVariation,
  onDiscardVariations,
  onSelectVariation,
  onClose,
}: {
  node: Node;
  /** Live deterministic preview of the component as it exists now. */
  currentSrc: string;
  /** Preview URL for variation `idx` on the shared dev server. */
  variationSrc: (idx: number) => string;
  variationState: VariationState | null;
  onStartVariation: (nodeId: string, prompt: string, count?: number, baseVariationIdx?: number) => void;
  onAcceptVariation?: (nodeId: string, variationIdx: number) => void;
  onDiscardVariations?: (nodeId: string) => void;
  onSelectVariation?: (idx: number | null) => void;
  onClose: () => void;
}) {
  const [prompt, setPrompt] = useState("");
  const [count, setCount] = useState<1 | 3>(3);
  const generating = variationState?.status === "generating";
  const ready = variationState?.status === "ready";
  const selectedIdx = variationState?.selectedIdx ?? null;

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const handleSubmit = () => {
    const value = prompt.trim();
    if (!value || generating) return;
    onStartVariation(
      node.id,
      value,
      count,
      ready && selectedIdx != null ? selectedIdx : undefined,
    );
    setPrompt("");
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

  return createPortal(
    <div className="fixed inset-0 z-[1000] flex items-center justify-center">
      <div className="absolute inset-0 bg-black/55 backdrop-blur-[3px]" onClick={onClose} />
      <div className="relative flex max-h-[90vh] w-[90vw] max-w-[1200px] flex-col overflow-hidden rounded-lg border border-[var(--border-strong)] bg-[var(--surface)] shadow-2xl">
        {/* Header */}
        <div className="flex items-center justify-between border-b border-[var(--border)] px-4 py-3">
          <span className="text-sm font-medium text-[var(--text)]">
            Plan visual changes — {node.name}
          </span>
          <button
            type="button"
            onClick={onClose}
            className="rounded p-0.5 text-[var(--text-tertiary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text)]"
            aria-label="Close"
          >
            <X className="h-4 w-4" />
          </button>
        </div>

        {/* Body */}
        <div className="flex-1 overflow-y-auto">
          {/* Original preview — always visible */}
          <div className="border-b border-[var(--border-subtle)] px-5 py-4">
            <p className="mb-2 text-[11px] font-semibold uppercase tracking-[0.07em] text-[var(--text-tertiary)]">Current</p>
            <div className="overflow-hidden rounded-md border border-[var(--border-subtle)]">
              <iframe
                src={currentSrc}
                title={`Current: ${node.name}`}
                className="h-[350px] w-full border-0"
                sandbox="allow-scripts allow-same-origin"
              />
            </div>
          </div>

          {/* Prompt bar */}
          <div className="flex items-center gap-2 border-b border-[var(--border-subtle)] px-5 py-3">
            <Input
              variant="bordered"
              className="min-w-0 flex-1 disabled:opacity-50"
              type="text"
              placeholder={ready && selectedIdx != null ? "Refine the selected variation…" : "Describe visual changes…"}
              value={prompt}
              disabled={generating}
              onChange={(e) => setPrompt(e.target.value)}
              onKeyDown={(e) => { if (e.key === "Enter") handleSubmit(); }}
            />
            <SegField<1 | 3>
              options={[
                { value: 1, label: "1" },
                { value: 3, label: "3" },
              ]}
              value={count}
              disabled={generating}
              onChange={setCount}
            />
            <button
              type="button"
              disabled={generating}
              onClick={handleSubmit}
              className={`${BTN_AGENT} disabled:opacity-50`}
            >
              <Send className="h-3.5 w-3.5" />
              {ready ? "Iterate" : "Generate"}
            </button>
          </div>

          {/* Variations */}
          {generating && (
            <div className="flex flex-col items-center gap-3 px-5 py-12">
              <Loader2 className="h-6 w-6 animate-spin text-violet-500 dark:text-violet-400" />
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
                    <button type="button" onClick={handleAccept} className={BTN_GO}>
                      <Check className="h-3.5 w-3.5" /> Accept
                    </button>
                  )}
                  <button type="button" onClick={handleDiscard} className={BTN}>
                    <Undo2 className="h-3.5 w-3.5" /> Discard
                  </button>
                </div>
              </div>
              <div className={`grid gap-3 ${varCount === 1 ? "grid-cols-1 max-w-[600px]" : "grid-cols-3"}`}>
                {Array.from({ length: varCount }, (_, i) => (
                  <button
                    key={i}
                    type="button"
                    onClick={() => onSelectVariation?.(selectedIdx === i ? null : i)}
                    className={`flex flex-col gap-1.5 rounded-lg border-2 p-1 transition-colors ${
                      selectedIdx === i
                        ? "border-violet-500 bg-violet-500/5"
                        : "border-[var(--border-subtle)] hover:border-[var(--border-strong)]"
                    }`}
                  >
                    <div className="overflow-hidden rounded-md">
                      <iframe
                        src={variationSrc(i)}
                        title={`Variation ${i + 1}`}
                        className="pointer-events-none h-[280px] w-full border-0"
                        sandbox="allow-scripts allow-same-origin"
                      />
                    </div>
                    <span className={`text-2xs font-medium ${
                      selectedIdx === i ? "text-violet-500 dark:text-violet-400" : "text-[var(--text-tertiary)]"
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
    </div>,
    document.body,
  );
}
