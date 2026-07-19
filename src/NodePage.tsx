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

import { useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Anchor,
  CircleDashed,
  FlaskConical,
  Flag,
  GitCompare,
} from "lucide-react";
import type { Node } from "./viewmodel";
import {
  effectiveSourceMap,
  effectiveVerifyMap,
  isDataShape,
  isNodeEmpty,
  nextResponsibilityId,
} from "./viewmodel";
import { completenessBadge, verifyStatesOf } from "./health";
import { kindIcon, typeTag } from "./kindIcon";
import { lookupIcon } from "./IconPicker";
import { ConnectionsSection, ImpliedConnectionsSection } from "./ConnectionsSection";
import { PageMenuProvider, usePageMenu, useCopyId, copyIdItem } from "./pageMenu";
import {
  Editable,
  EmptyFlag,
  NAME_MAX,
  PAGE_COL,
  sanitizeIdentifier,
  TECHNOLOGY_MAX,
} from "./pagekit";
import type { PageProps } from "./page/types";
import { GAUGE_CHIP, useEditSections } from "./page/kit";
import {
  ancestorChain,
  Crumbs,
  PageHeader,
  Ambox,
  NOTICE_ACTION,
  PageTabs,
  NodeHistory,
} from "./page/PageHeader";
import { GroupPageBody } from "./page/GroupPage";
import { DescriptionSection } from "./page/DescriptionSection";
import { DetailRail } from "./page/DetailRail";
import { plannedRespHosts, ResponsibilitiesSection } from "./page/ResponsibilitiesSection";
import { PropertiesSection } from "./page/PropertiesSection";
import { PreviewSection } from "./page/PreviewSection";
import { AppearanceWorkspace } from "./page/AppearanceWorkspace";

export type { VariationState, SpecialPage, Selected } from "./page/types";

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
  // Appearance takeover: the overview content row swaps for the visual-edit
  // workspace. Leaving the tab leaves the mode.
  const [editingAppearance, setEditingAppearance] = useState(false);
  // The header edits (name, technology) accumulate in this draft; the model is
  // written once, on Done — the same nothing-commits-until-Done contract as
  // every SectionEditor. Cancel (or navigating away) simply drops the draft.
  const titleDraft = useRef<{ name: string; technology: string } | null>(null);
  const openTitleEdit = () => {
    if (ed.isEditing("title")) return;
    titleDraft.current = { name: node.name, technology: node.technology ?? "" };
    ed.toggle("title");
  };
  const commitTitleEdit = () => {
    const d = titleDraft.current;
    titleDraft.current = null;
    ed.toggle("title");
    if (!d || !editor) return;
    // An emptied title keeps the old name; an emptied technology clears it.
    const name = d.name.trim() || node.name;
    const technology = d.technology.trim() || undefined;
    if (name !== node.name || technology !== node.technology) {
      editor.updateNode(node.id, { name, technology });
    }
  };
  const cancelTitleEdit = () => {
    titleDraft.current = null;
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
  const verifyMap = effectiveVerifyMap(committed, model);
  // Per-claim fingerprint state of the backing test (verify: observations).
  const verifyStates = useMemo(() => verifyStatesOf(report), [report]);
  const dataShape = isDataShape(node);
  const resps = node.responsibilities ?? [];
  // The committed copy of this node's claims — the diff base for the Overview.
  const committedResps =
    committed?.nodes.find((n) => n.id === node.id)?.responsibilities ?? [];
  const definition = sourceMap[node.id] ?? [];
  // The node's anchored source file, for matching its preview component.
  const previewSourceFile =
    sourceMap[node.id]?.[0]?.pattern ??
    node.responsibilities?.map((r) => sourceMap[r.id]?.[0]?.pattern).find(Boolean);

  // Leaf claims must read through to code; structural nodes discharge through
  // their subtree, so their claims are never "unmapped". Leafness spans the
  // AUTHORED tree (committed + plan) — the same union compute_health and the
  // Unmapped page use — so the pill and the counters always agree, and a
  // design-ahead child discharges the parent's claims everywhere at once.
  // Persons (actors) and externals are out-of-system — never code-backed.
  const hasAuthoredChildren =
    model.nodes.some((n) => n.parentId === node.id) ||
    (committed?.nodes ?? []).some((n) => n.parentId === node.id);
  const leafHost = !hasAuthoredChildren && !node.external && node.kind !== "person";

  // The node's own definition anchor — its file, surfaced in the type line.
  const defFile = definition[0]?.pattern;

  // Drift counts span both claims and data fields — a vagrant/stale property
  // feeds the same review notices as a responsibility.
  const driftProps = node.properties ?? [];
  const staleCount =
    resps.filter((r) => r.stale).length + driftProps.filter((p) => p.stale).length;
  const vagrantCount =
    resps.filter((r) => r.vagrant).length + driftProps.filter((p) => p.vagrant).length;
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
                  title="Keep this node and rebuild its whole subtree in code — files a to-do the agent implements."
                  className={NOTICE_ACTION}
                >
                  Rebuild code
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
          <div data-drift-banner>
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
          </div>
        )}
        {staleCount > 0 && (
          <Ambox tone="warning" icon={<Flag className="h-3 w-3" />}>
            {staleCount} stale claim{staleCount === 1 ? "" : "s"} to review below
          </Ambox>
        )}
        {vagrantCount > 0 && (
          <Ambox tone="warning" icon={<Flag className="h-3 w-3" />}>
            {vagrantCount} undescribed in code to review below
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
                accumulating in the title draft (committed by Done); otherwise
                shown only when set. */}
            {ed.isEditing("title") ? (
              <>
                <span className="text-[var(--text-ghost)]">·</span>
                <Editable
                  initial={node.technology ?? ""}
                  placeholder="technology"
                  maxLength={TECHNOLOGY_MAX}
                  onInput={(t) => {
                    if (titleDraft.current) titleDraft.current.technology = t;
                  }}
                  onEnter={commitTitleEdit}
                  onEscape={cancelTitleEdit}
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
            {/* Ground-truth gauges follow the identity run as bordered mono
                chips — instruments, not prose, but in the same reading line
                (right-aligned they float contextless at wide widths). */}
            {(() => {
              const badge = completenessBadge(report?.completeness[node.id]);
              if (!badge) return null;
              return (
                <span
                  className={GAUGE_CHIP}
                  title={
                    badge.measured
                      ? `${badge.label} of this node's claims read through to code`
                      : "No leaf claims yet — nothing to measure"
                  }
                >
                  <Anchor className={`h-3 w-3 ${badge.grounded ? "" : "opacity-40"}`} />
                  {badge.label}
                </span>
              );
            })()}
            {(() => {
              const h = report?.health.nodes[node.id]?.subtree;
              if (!h?.verified) return null;
              return (
                <span
                  className={GAUGE_CHIP}
                  title={`${h.verified} of ${h.responsibilities} claim${h.responsibilities === 1 ? "" : "s"} in this subtree backed by a test`}
                >
                  <FlaskConical className="h-3 w-3" />
                  {h.verified}/{h.responsibilities}
                </span>
              );
            })()}
            {isNodeEmpty(node) && <EmptyFlag />}
          </>
        }
        editor={editor}
        editingName={ed.isEditing("title")}
        onToggleName={openTitleEdit}
        onDone={commitTitleEdit}
        onCancel={cancelTitleEdit}
        onNameInput={(v) => {
          if (titleDraft.current) titleDraft.current.name = v;
        }}
        // Symbol names are bound to code identifiers (shape, not length); every
        // other kind is a human-authored title with a length cap.
        nameMaxLength={node.kind === "symbol" ? undefined : NAME_MAX}
        nameSanitize={node.kind === "symbol" ? sanitizeIdentifier : undefined}
        tabs={
          <PageTabs
            tab={tab}
            onTab={(t) => {
              setEditingAppearance(false);
              setTab(t);
            }}
            historyCount={nodeEvents.length}
          />
        }
      />

      <div
        className={`min-h-0 flex-1 ${
          editingAppearance && tab === "overview" ? "overflow-hidden" : "overflow-y-auto"
        }`}
      >
        {tab === "history" ? (
          <div className={`${PAGE_COL} pb-[50px] pt-[18px]`}>
            <div className="max-w-[900px]">
              <NodeHistory events={nodeEvents} projectPath={projectPath} />
            </div>
          </div>
        ) : editingAppearance && node.visual ? (
          <AppearanceWorkspace
            node={node}
            projectPath={projectPath}
            sourceFile={previewSourceFile}
            variationState={
              props.variationState?.nodeId === node.id ? props.variationState : null
            }
            onStartVariation={props.onStartVariation}
            onAcceptVariation={props.onAcceptVariation}
            onDiscardVariations={props.onDiscardVariations}
            onSelectVariation={props.onSelectVariation}
            onClose={() => setEditingAppearance(false)}
          />
        ) : (
          <div className={`${PAGE_COL} flex gap-8 pb-[50px] pt-[18px]`}>
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
                editor={editor}
                editing={ed.isEditing("description")}
                onToggle={() => ed.toggle("description")}
                onCommit={(v) => editor?.updateNode(node.id, { description: v || undefined })}
              />

              {node.visual && (
                <PreviewSection
                  node={node}
                  projectPath={projectPath}
                  sourceFile={previewSourceFile}
                  onFixture={onFixture}
                  variationsReady={
                    props.variationState?.nodeId === node.id &&
                    props.variationState.status === "ready"
                  }
                  onEditAppearance={
                    props.onStartVariation ? () => setEditingAppearance(true) : undefined
                  }
                />
              )}

              {!dataShape && (
                <ResponsibilitiesSection
                  host="node"
                  hostId={node.id}
                  resps={resps}
                  prevResps={committedResps}
                  plannedHosts={plannedRespHosts(model)}
                  concerns={model.concerns ?? []}
                  sourceMap={sourceMap}
                  verifyMap={verifyMap}
                  verifyStates={verifyStates}
                  projectPath={projectPath}
                  leafHost={leafHost}
                  codeBackedHost={!node.external && node.kind !== "person"}
                  mintId={(draft) => nextResponsibilityId(draft, model, committed)}
                  editor={editor}
                  editing={ed.isEditing("responsibilities")}
                  onToggle={() => ed.toggle("responsibilities")}
                />
              )}

              {node.kind === "symbol" && (
                <PropertiesSection
                  node={node}
                  prevProps={committed?.nodes.find((n) => n.id === node.id)?.properties ?? []}
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
            <DetailRail
              node={node}
              model={model}
              committed={committed}
              editor={editor}
              notesEditing={ed.isEditing("notes")}
              onToggleNotes={() => ed.toggle("notes")}
              dirEditing={ed.isEditing("directives")}
              onToggleDir={() => ed.toggle("directives")}
              onSelectNode={onSelectNode}
            />
          </div>
        )}
      </div>
    </div>
  );
}
