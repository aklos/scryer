/**
 * The main panel: a read-first wiki page for the selected node or group
 * (groups render through GroupPage), following Wikipedia's anatomy:
 *
 *  - header: breadcrumb trail, title, page-level actions
 *  - type line under the title: kind, technology, style and layer, the
 *    defining file, and the subtree's test gauge —
 *    structured metadata surfaced inline rather than in a separate column
 *  - maintenance banners (ambox): structural style violations, each
 *    expandable to where it is
 *  - lede: the description paragraph, no heading
 *  - sections with per-section [edit] links, swapped to edit mode in place:
 *    preview, claims with their test verdicts, the claims that exercise a
 *    symbol, properties,
 *    connections and implied connections
 *
 * Mutations flow through the Editor intents.
 */

import { useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Flag,
  Shapes,
} from "lucide-react";
import type { Node, ScryModel } from "../../entities/model/viewmodel";
import {
  effectiveSourceMap,
  effectiveTestMap,
  isDataShape,
  nextResponsibilityId,
} from "../../entities/model/viewmodel";
import {
  structuralBySubtree,
  noStyleNotice,
  unstyledContainers,
  structuralNotice,
  styleTable,
  testStatesOf,
  type StyleViolation,
} from "../../entities/model/health";
import { kindIcon, typeTag } from "../../entities/model/kindIcon";
import { lookupIcon } from "../../features/search/IconPicker";
import { ConnectionsSection, ImpliedConnectionsSection } from "../../widgets/connections/ConnectionsSection";
import { PageMenuProvider, usePageMenu, useCopyId, copyIdItem } from "../../features/search/pageMenu";
import {
  NAME_MAX,
  PAGE_COL,
  sanitizeIdentifier,
} from "../../shared/ui/pagekit";
import type { PageProps } from "../../widgets/node-page-sections/types";
import { TYPE_CHIP, useEditSections } from "../../widgets/node-page-sections/kit";
import { CommentsRail } from "../../widgets/comments-rail/CommentsRail";
import { useComments } from "../../features/comments/useComments";
import { CLAIM_LABEL, CLAIM_STATES, CLAIM_TONE, describeCounts } from "../../entities/model/claimState";
import {
  ancestorChain,
  Crumbs,
  PageHeader,
  Ambox,
  NOTICE_ACTION,
} from "../../widgets/node-page-sections/PageHeader";
import { GroupPageBody } from "./GroupPage";
import { DescriptionSection } from "../../widgets/node-page-sections/DescriptionSection";
import { plannedRespHosts, ResponsibilitiesSection } from "../../widgets/node-page-sections/ResponsibilitiesSection";
import { PropertiesSection } from "../../widgets/node-page-sections/PropertiesSection";
import { PreviewSection } from "../../widgets/node-page-sections/PreviewSection";
import { ExercisedBySection } from "../../widgets/node-page-sections/ExercisedBySection";
import { previewEntryFor } from "../../features/preview-client/usePreviewServer";
import { allowedImports, governingStyleDef, layerOf } from "../../entities/model/styles";

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

/** Plain words for each violation kind, as the list labels them. */
const VIOLATION_KIND: Record<StyleViolation["kind"], string> = {
  layer_violation: "layer",
  isolation_violation: "isolation",
  external_violation: "external import",
  misplaced: "misplaced file",
  unstyled: "no style",
  layerless: "no layer",
  cycle: "cycle",
  forbidden_link: "forbidden link",
  role_violation: "role",
  data_violation: "data shape",
};

/** Where the structural notice's violations are: grouped under the container
 *  whose style each breaks, each naming the component charged (and the one it
 *  reaches, for an import or link), the file the fix goes in, and the check's
 *  own one-line account. Names navigate to their pages. */
export function StructuralViolationList({
  violations,
  model,
  onSelectNode,
}: {
  violations: readonly StyleViolation[];
  model: Pick<ScryModel, "nodes">;
  onSelectNode: (id: string) => void;
}) {
  const names = new Map(model.nodes.map((n) => [n.id, n.name] as const));
  const byContainer = new Map<string, StyleViolation[]>();
  for (const v of violations) {
    const list = byContainer.get(v.container);
    if (list) list.push(v);
    else byContainer.set(v.container, [v]);
  }
  const link = (id: string) => (
    <button
      type="button"
      onClick={() => onSelectNode(id)}
      className="font-medium text-[var(--text)] underline-offset-2 hover:underline"
    >
      {names.get(id) || id}
    </button>
  );
  return (
    <div
      data-structural-list
      className="flex flex-col gap-3 rounded-md border border-[var(--border)] px-3 py-2 text-xs"
    >
      {[...byContainer].map(([container, vs]) => (
        <div key={container} className="flex flex-col gap-1.5">
          <div className="flex items-baseline gap-2">
            {link(container)}
            <span className="text-[var(--text-ghost)]">
              {vs.length} violation{vs.length === 1 ? "" : "s"}
            </span>
          </div>
          <ul className="flex flex-col gap-1.5 border-l border-[var(--border)] pl-3">
            {vs.map((v, i) => (
              <li key={i} className="flex flex-col gap-0.5">
                <span className="flex flex-wrap items-baseline gap-x-1.5">
                  {v.node !== v.container && link(v.node)}
                  {v.other && (
                    <>
                      <span className="text-[var(--text-ghost)]">→</span>
                      {link(v.other)}
                    </>
                  )}
                  <span className="text-2xs uppercase tracking-wider text-[var(--text-ghost)]">
                    {VIOLATION_KIND[v.kind]}
                  </span>
                </span>
                <span className="font-mono text-2xs text-[var(--text-muted)]">{v.file}</span>
                <span className="text-[var(--text-secondary)]">{v.detail}</span>
              </li>
            ))}
          </ul>
        </div>
      ))}
    </div>
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
  } = props;
  const ed = useEditSections();
  const openMenu = usePageMenu();
  const copyId = useCopyId();
  // The header edits (name, technology) accumulate in this draft; the model is
  // written once, on Done — the same nothing-commits-until-Done contract as
  // every SectionEditor. Cancel (or navigating away) simply drops the draft.
  const titleDraft = useRef<{ name: string } | null>(null);
  const openTitleEdit = () => {
    if (ed.isEditing("title")) return;
    titleDraft.current = { name: node.name };
    ed.toggle("title");
  };
  const commitTitleEdit = () => {
    const d = titleDraft.current;
    titleDraft.current = null;
    ed.toggle("title");
    if (!d || !editor) return;
    // An emptied title keeps the old name.
    const name = d.name.trim() || node.name;
    if (name !== node.name) editor.updateNode(node.id, { name });
  };
  const cancelTitleEdit = () => {
    titleDraft.current = null;
    ed.toggle("title");
  };
  const tag = typeTag(node);

  const sourceMap = effectiveSourceMap(committed, model);
  const testMap = effectiveTestMap(committed, model);
  // Per-claim fingerprint state of the attached test (test: observations).
  const testStates = useMemo(() => testStatesOf(report), [report]);
  const structuralViolations = useMemo(
    () => structuralBySubtree(model, report ?? null).get(node.id) ?? [],
    [model, report, node.id],
  );
  const structural = structuralNotice(structuralViolations, node, model);
  const noStyle = noStyleNotice(unstyledContainers(report ?? null), node, model);
  // Comment threads on this node; the system page also keeps the ones whose
  // node no longer exists, so a moved module's threads are never lost.
  const comments = useComments(projectPath);
  const threads = comments.threads.filter(
    (t) => t.node === node.id || (node.kind === "system" && !model.nodes.some((n) => n.id === t.node)),
  );
  const anchor = [...ancestorChain(model, node.parentId), node].map((n) => n.name).join(" / ");
  const [structuralOpen, setStructuralOpen] = useState(false);
  const dataShape = isDataShape(node);
  const resps = node.responsibilities ?? [];
  // Each claim's derived status: its row's marker, and what a reworded one
  // last passed as.
  const claimStatuses = Object.fromEntries((report?.derivedStatus?.claims ?? []).map((c) => [c.respId, c]));
  // The committed copy of this node's claims — the diff base for the Overview.
  const committedResps =
    committed?.nodes.find((n) => n.id === node.id)?.responsibilities ?? [];
  const definition = sourceMap[node.id] ?? [];
  // Preview-ability is derived, never stored: the section exists iff the
  // sidecar reports a mountable export for the node's anchored source file.
  const previewSourceFile =
    sourceMap[node.id]?.[0]?.pattern ??
    node.responsibilities?.map((r) => sourceMap[r.id]?.[0]?.pattern).find(Boolean);
  const previewEntry = previewEntryFor(node, props.preview.components, previewSourceFile);
  const KindIcon = lookupIcon(node.icon) ?? kindIcon(node, previewEntry != null);

  // The node's own definition anchor — its file, surfaced in the type line.
  const defFile = definition[0]?.pattern;

  // Maintenance notices — full-width amboxes stacked at the top of the article
  // body (the wiki hatnote pattern), not chips crammed beside the title.
  const bannerStack = structural || noStyle ? (
    <>
      {structural && (
      <Ambox
        tone="danger"
        icon={<Flag className="h-3 w-3" />}
        actions={
          <button
            type="button"
            onClick={() => setStructuralOpen((o) => !o)}
            aria-expanded={structuralOpen}
            className={NOTICE_ACTION}
          >
            {structuralOpen ? "Hide" : "Show where"}
          </button>
        }
      >
        {structural}
      </Ambox>
      )}
      {structuralOpen && (
        <StructuralViolationList
          violations={structuralViolations}
          model={model}
          onSelectNode={onSelectNode}
        />
      )}
      {noStyle && (
        <Ambox tone="note" icon={<Shapes className="h-3 w-3" />}>
          {noStyle.text}
          {noStyle.containers.map((id, i) => (
            <span key={id}>
              {i > 0 && (i === noStyle.containers.length - 1 ? " and " : ", ")}
              <button type="button" onClick={() => onSelectNode(id)} className={NOTICE_ACTION}>
                {model.nodes.find((n) => n.id === id)?.name ?? id}
              </button>
            </span>
          ))}
        </Ambox>
      )}
    </>
  ) : null;

  // The horizontal axis for the badges below.
  const styleDef = governingStyleDef(model, node.id, styleTable(report ?? null));
  const layer = layerOf(model, node.id);

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
            {node.technology && (
              <>
                <span className="text-[var(--text-ghost)]">·</span>
                <span className="font-mono">{node.technology}</span>
              </>
            )}
            {/* Style and layer: the horizontal axis. A container names its
                style; a component its layer, with the one line the reader
                needs about it — what it may import. Symbols inherit. */}
            {node.style && (
              <span
                className={TYPE_CHIP}
                title={styleDef ? `${styleDef.name}: ${styleDef.description}` : `style: ${node.style}`}
              >
                <span className="text-[var(--text-muted)]">style</span>
                {node.style}
              </span>
            )}
            {layer && (
              <>
                <span
                  className={TYPE_CHIP}
                  title={
                    styleDef?.layers.find((l) => l.name === layer)?.description ??
                    `layer: ${layer}`
                  }
                >
                  <span className="text-[var(--text-muted)]">layer</span>
                  {layer}
                </span>
                {styleDef && node.kind === "component" && (
                  <span className="text-[var(--text-tertiary)]">
                    may import {allowedImports(styleDef, layer).join(", ") || "nothing"}
                  </span>
                )}
              </>
            )}
            {defFile && (
              <>
                <span className="text-[var(--text-ghost)]">·</span>
                <button
                  type="button"
                  onClick={() =>
                    void invoke("open_in_editor", {
                      file: defFile,
                      line: definition[0]?.line ?? null,
                      symbol: definition[0]?.symbol ?? null,
                      projectPath,
                    })
                  }
                  title="Open in editor"
                  className="font-mono text-[var(--text-tertiary)] hover:text-blue-600 hover:underline dark:hover:text-blue-400"
                >
                  {defFile}
                </button>
              </>
            )}
            {/* The claims inside this node, by state — the same counts and
                hues as the map's claim bar and the status line. */}
            {(() => {
              const c = report?.derivedStatus?.nodes[node.id]?.subtree;
              if (!c || c.claims === 0) return null;
              return (
                <span className={TYPE_CHIP} title={`Claims in this subtree: ${describeCounts(c)}`}>
                  <span className="text-[var(--text-muted)]">
                    {c.claims} claim{c.claims === 1 ? "" : "s"}:
                  </span>
                  {CLAIM_STATES.filter((k) => c[k] > 0).map((k, i) => (
                    <span key={k} className="flex items-center gap-1">
                      {i > 0 && <span className="text-[var(--text-ghost)]">·</span>}
                      <span className={CLAIM_TONE[k]}>
                        {c[k]} {CLAIM_LABEL[k]}
                      </span>
                    </span>
                  ))}
                </span>
              );
            })()}
          </>
        }
        // Only a container's display name lives outside the code, in
        // structure.toml; everything else is named by its code.
        editor={node.kind === "container" ? editor : undefined}
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
      />

      <div className="min-h-0 flex-1 overflow-y-auto">
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
                // The description is the module's doc comment: edited in the code.
                editor={undefined}
                editing={ed.isEditing("description")}
                onToggle={() => ed.toggle("description")}
                onCommit={(v) => editor?.updateNode(node.id, { description: v || undefined })}
              />

              {previewEntry && (
                <PreviewSection
                  node={node}
                  server={props.preview}
                  entry={previewEntry}
                  onFixture={onFixture}
                />
              )}

              {/* A symbol owns no claims by design; its section shows only
                  when a test is placed on it directly. */}
              {!dataShape && !(node.kind === "symbol" && resps.length === 0) && (
                <ResponsibilitiesSection
                  host="node"
                  hostId={node.id}
                  resps={resps}
                  prevResps={committedResps}
                  plannedHosts={plannedRespHosts(model)}
                  concerns={model.concerns ?? []}
                  testMap={testMap}
                  notes={model.notes}
                  testStates={testStates}
                  claimStatuses={claimStatuses}
                  testVerdicts={props.testVerdicts}
                  probeResults={props.probeResults}
                  projectPath={projectPath}
                  mintId={(draft) => nextResponsibilityId(draft, model, committed)}
                  editor={editor}
                  editing={ed.isEditing("responsibilities")}
                  onToggle={() => ed.toggle("responsibilities")}
                />
              )}

              {node.kind === "symbol" && (node.exercisedBy?.length ?? 0) > 0 && (
                <ExercisedBySection
                  claimIds={node.exercisedBy!}
                  model={model}
                  status={report?.derivedStatus ?? null}
                  onSelectNode={onSelectNode}
                />
              )}

              {node.kind === "symbol" && (
                <PropertiesSection
                  node={node}
                  prevProps={committed?.nodes.find((n) => n.id === node.id)?.properties ?? []}
                  editor={undefined}
                  editing={ed.isEditing("properties")}
                  onToggle={() => ed.toggle("properties")}
                />
              )}

              <ConnectionsSection
                model={model}
                committed={committed}
                node={node}
                report={report}
                // Links come from imports and structure.toml, not from here.
                editor={undefined}
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
            <CommentsRail
              threads={threads}
              onOpen={(text) => comments.open(node.id, anchor, text)}
              onReply={comments.reply}
            />
          </div>
      </div>
    </div>
  );
}
