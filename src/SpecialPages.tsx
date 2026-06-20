/**
 * Wiki special pages — the cross-cutting surfaces that aren't model content:
 *
 *  - Recent changes: the session journal of external (agent) writes, newest
 *    first, with per-field before → after diffs. This is what you watch while
 *    planning in a terminal beside scryer.
 *  - Needs review: the maintenance-category index. Every observation awaiting
 *    a human verdict, grouped by kind, with the verdict actions inline. An
 *    empty page means the model is trustworthy.
 *  - Dark code: the inverse of coverage from the code's side — every file under
 *    a node's boundary that no claim reads into, grouped by the owning node.
 *    Where you eyeball how much is boilerplate versus something load-bearing the
 *    lens is missing.
 *  - Unmapped claims: the same gap from the model's side — committed leaf claims
 *    that say code exists but anchor to nothing. The list behind the coverage
 *    percentage; its complement.
 *
 * All are pages, not panels — reached from the status bar counters, left via
 * any link, exactly like Wikipedia's Special:RecentChanges and cleanup
 * categories.
 */

import { useState } from "react";
import { Check, FileClock, GitCompare, Sparkles } from "lucide-react";
import { ConfirmPopover } from "./ConfirmPopover";
import type { ChangeItem, ChangeRevision } from "./hooks/useModelStorage";
import type { ScryModel, Node, Responsibility, DriftScope } from "./viewmodel";
import type { Editor } from "./editor";
import type { ModelHealthReport } from "./health";
import { ANCHOR_STATE_LABEL, collapseAnchors, darkBoundaries } from "./health";
import { kindIcon } from "./kindIcon";
import { isNodeEmpty } from "./rollup";
import { respElementId } from "./SourceSection";
import { BTN, BTN_AGENT, jumpTo, PageSection, WikiLink } from "./pagekit";

// --- shared shell -------------------------------------------------------------

function SpecialHeader({ title, subtitle }: { title: string; subtitle: string }) {
  return (
    <header className="shrink-0 border-b border-[var(--border)] px-7 pb-3 pt-[18px]">
      <h1 className="text-[21px] font-semibold leading-tight text-[var(--text)]">{title}</h1>
      <div className="mt-[3px] text-xs text-[var(--text-tertiary)]">{subtitle}</div>
    </header>
  );
}

function SpecialBody({ children }: { children: React.ReactNode }) {
  return (
    <div className="min-h-0 flex-1 overflow-y-auto">
      <div className="max-w-[820px] px-7 pb-[50px] pt-[18px]">{children}</div>
    </div>
  );
}

// --- recent changes -----------------------------------------------------------

const OP_MARK: Record<string, { mark: string; title: string }> = {
  added: { mark: "+", title: "added" },
  changed: { mark: "±", title: "changed" },
  removed: { mark: "−", title: "removed" },
};

function timeLabel(at: number): string {
  return new Date(at).toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
}

/** One revision's diff rows. `context` is hidden when every item shares the
 *  same host (a per-node history, where "on X" would repeat on every line). */
function RevisionItems({
  items,
  showContext,
  onSelectNode,
}: {
  items: readonly ChangeItem[];
  showContext: boolean;
  onSelectNode: (id: string) => void;
}) {
  return (
    <ul className="flex flex-col gap-1">
      {items.map((it, j) => (
        <li key={j} className="flex items-start gap-2">
          <span
            title={OP_MARK[it.op].title}
            className="w-3 shrink-0 pt-px text-center font-mono text-xs text-[var(--text-muted)]"
          >
            {OP_MARK[it.op].mark}
          </span>
          <div className="min-w-0 flex-1">
            <div className="flex min-w-0 items-baseline gap-1.5 text-sm">
              <span className="shrink-0 text-2xs uppercase tracking-wide text-[var(--text-ghost)]">
                {it.what}
              </span>
              {/* Removals read as struck-through text (never a live link — the
                  thing is gone); additions/changes link to their node. */}
              {it.op !== "removed" && it.nodeId ? (
                <button
                  type="button"
                  onClick={() => onSelectNode(it.nodeId!)}
                  className="min-w-0 truncate text-left text-blue-700 hover:underline dark:text-blue-400"
                >
                  {it.label}
                </button>
              ) : (
                <span
                  className={`min-w-0 truncate ${
                    it.op === "removed"
                      ? "text-[var(--text-muted)] line-through decoration-[var(--text-ghost)]"
                      : "text-[var(--text-secondary)]"
                  }`}
                >
                  {it.label}
                </span>
              )}
              {showContext && it.context && (
                <span className="shrink-0 text-2xs text-[var(--text-muted)]">
                  on {it.context}
                </span>
              )}
            </div>
            {it.fields && it.fields.length > 0 && (
              <ul className="mt-0.5 flex flex-col gap-px pl-1">
                {it.fields.map((f) => (
                  <li key={f.field} className="text-2xs leading-relaxed">
                    <span className="text-[var(--text-muted)]">{f.field}: </span>
                    <del className="text-[var(--text-muted)] decoration-[var(--text-ghost)]">
                      {f.from}
                    </del>
                    <span className="text-[var(--text-ghost)]"> → </span>
                    <span className="text-[var(--text-secondary)]">{f.to}</span>
                  </li>
                ))}
              </ul>
            )}
          </div>
        </li>
      ))}
    </ul>
  );
}

/** Shared revision feed: time + attribution header per edit burst, then its
 *  diff rows. Drives both the global Recent-changes page and a single node's
 *  History tab. */
export function RevisionList({
  revisions,
  showContext = true,
  onSelectNode,
}: {
  revisions: readonly ChangeRevision[];
  /** Hide the "on X" host label — redundant in a per-node history. */
  showContext?: boolean;
  onSelectNode: (id: string) => void;
}) {
  return (
    <div className="flex flex-col">
      {revisions.map((rev, i) => (
        <section
          key={`${rev.at}-${i}`}
          className="border-b border-[var(--border-subtle)] py-3 last:border-b-0"
        >
          <div className="mb-1.5 flex items-center gap-2 font-mono text-2xs tabular-nums text-[var(--text-muted)]">
            {timeLabel(rev.at)}
            {/* Attribution — indigo is the agent's hue. */}
            <span
              className={
                rev.by === "agent"
                  ? "font-sans font-medium text-indigo-600 dark:text-indigo-400"
                  : "font-sans font-medium text-[var(--text-tertiary)]"
              }
            >
              {rev.by === "agent" ? "agent" : "you"}
            </span>
            <span className="text-[var(--text-ghost)]">
              {rev.items.length} change{rev.items.length === 1 ? "" : "s"}
            </span>
          </div>
          <RevisionItems items={rev.items} showContext={showContext} onSelectNode={onSelectNode} />
        </section>
      ))}
    </div>
  );
}

export function RecentChangesPage({
  changeLog,
  onSelectNode,
}: {
  changeLog: readonly ChangeRevision[];
  onSelectNode: (id: string) => void;
}) {
  return (
    <div className="flex min-w-0 flex-1 flex-col">
      <SpecialHeader
        title="Recent changes"
        subtitle="Every edit to the model this session — yours and the agent's — newest first"
      />
      <SpecialBody>
        {changeLog.length === 0 ? (
          <div className="flex flex-col items-center gap-3 px-6 py-16">
            <FileClock className="h-6 w-6 text-[var(--text-ghost)]" />
            <p className="text-xs text-[var(--text-muted)]">
              No changes this session. Your edits and the agent's writes will appear here as
              they land.
            </p>
          </div>
        ) : (
          <div className="pt-5">
            <RevisionList revisions={changeLog} onSelectNode={onSelectNode} />
          </div>
        )}
      </SpecialBody>
    </div>
  );
}

// --- needs review ---------------------------------------------------------------

interface ClaimRef {
  node: Node;
  resp: Responsibility;
}

/** One claim row: the statement (opens the claim on its own page, flashing it
 *  once rendered) and the node it sits on. Shared by Needs review and the
 *  Unmapped claims page so the two render claims identically. */
function ClaimRow({
  claim,
  onSelectNode,
  actions,
}: {
  claim: ClaimRef;
  onSelectNode: (id: string) => void;
  actions?: React.ReactNode;
}) {
  const goToClaim = () => {
    onSelectNode(claim.node.id);
    window.setTimeout(() => jumpTo(respElementId(claim.resp.id)), 250);
  };
  return (
    <li className="flex items-start gap-2 border-b border-[var(--border-subtle)] py-2 last:border-b-0">
      <div className="min-w-0 flex-1">
        <button
          type="button"
          onClick={goToClaim}
          className="block w-full truncate text-left text-sm text-[var(--text-secondary)] hover:text-[var(--text)] hover:underline"
          title="Open on its page"
        >
          {claim.resp.statement || "Untitled responsibility"}
        </button>
        <span className="text-2xs text-[var(--text-muted)]">
          on{" "}
          <button
            type="button"
            onClick={() => onSelectNode(claim.node.id)}
            className="text-blue-700 hover:underline dark:text-blue-400"
          >
            {claim.node.name || "Untitled"}
          </button>
        </span>
      </div>
      {actions}
    </li>
  );
}

export interface ReviewIndex {
  vagrant: ClaimRef[];
  stale: ClaimRef[];
  staleNodes: Node[];
  emptySymbols: Node[];
  unseenNodes: Node[];
  unseenClaims: ClaimRef[];
  total: number;
}

/** Gather everything awaiting a human verdict. Shared by the page and the
 *  status-bar counter so the number and the list can never disagree. */
export function buildReviewIndex(
  model: ScryModel,
  report: ModelHealthReport | null,
  driftScopes: DriftScope[],
  newNodeIds: ReadonlySet<string>,
  newRespIds: ReadonlySet<string>,
): ReviewIndex {
  const vagrant: ClaimRef[] = [];
  const stale: ClaimRef[] = [];
  const unseenClaims: ClaimRef[] = [];
  // Whole nodes whose backing code is gone — verdicted as a subtree, so their
  // own stale claims are subsumed (don't also list them as individual claims).
  const staleNodes = model.nodes.filter((n) => n.stale);
  const staleNodeIds = new Set(staleNodes.map((n) => n.id));
  for (const node of model.nodes) {
    for (const resp of node.responsibilities ?? []) {
      if (resp.vagrant) vagrant.push({ node, resp });
      if (resp.stale && !staleNodeIds.has(node.id)) stale.push({ node, resp });
      if (newRespIds.has(resp.id)) unseenClaims.push({ node, resp });
    }
  }
  const emptySymbols = model.nodes.filter(isNodeEmpty);
  const unseenNodes = model.nodes.filter((n) => newNodeIds.has(n.id));
  const total =
    vagrant.length +
    stale.length +
    staleNodes.length +
    emptySymbols.length +
    unseenNodes.length +
    unseenClaims.length +
    driftScopes.length +
    collapseAnchors(report?.anchors ?? []).length;
  return { vagrant, stale, staleNodes, emptySymbols, unseenNodes, unseenClaims, total };
}

export function NeedsReviewPage({
  model,
  report,
  driftScopes,
  newNodeIds,
  newRespIds,
  editor,
  onSelectNode,
  onCheckDrift,
  onDismissDrift,
  onClearAllNew,
}: {
  model: ScryModel;
  report: ModelHealthReport | null;
  driftScopes: DriftScope[];
  newNodeIds: ReadonlySet<string>;
  newRespIds: ReadonlySet<string>;
  editor: Editor | undefined;
  onSelectNode: (id: string) => void;
  onCheckDrift?: () => void;
  onDismissDrift?: () => void;
  onClearAllNew: () => void;
}) {
  const idx = buildReviewIndex(model, report, driftScopes, newNodeIds, newRespIds);
  const anchors = collapseAnchors(report?.anchors ?? []);

  // Dropping a stale claim deletes an authored responsibility (and its anchors),
  // so it's confirmed inline rather than firing on a single click.
  const [confirmDrop, setConfirmDrop] = useState<{
    rect: DOMRect;
    label: string;
    run: () => void;
  } | null>(null);

  return (
    <div className="flex min-w-0 flex-1 flex-col">
      <SpecialHeader
        title="Needs review"
        subtitle={
          idx.total === 0
            ? "Nothing awaits a verdict"
            : `${idx.total} item${idx.total === 1 ? "" : "s"} awaiting a human verdict`
        }
      />
      <SpecialBody>
        {idx.total === 0 ? (
          <div className="flex flex-col items-center gap-3 px-6 py-16">
            <Check className="h-6 w-6 text-emerald-500 dark:text-emerald-400" />
            <p className="text-xs text-[var(--text-muted)]">
              Nothing needs review — the model is current with the code.
            </p>
          </div>
        ) : (
          <>
            {(idx.unseenNodes.length > 0 || idx.unseenClaims.length > 0) && (
              <PageSection
                title="Unreviewed agent changes"
                count={idx.unseenNodes.length + idx.unseenClaims.length}
                right={
                  <button
                    type="button"
                    onClick={onClearAllNew}
                    className={BTN}
                  >
                    <Check className="h-3 w-3" /> Mark all reviewed
                  </button>
                }
              >
                <p className="mb-2 text-2xs text-[var(--text-muted)]">
                  <Sparkles className="mr-1 inline h-3 w-3 text-indigo-500 dark:text-indigo-400" />
                  Landed from the agent and not yet looked at. Opening an item clears it; see
                  Recent changes for the field-level diffs.
                </p>
                <ul className="flex flex-col">
                  {idx.unseenNodes.map((n) => (
                    <li key={n.id} className="border-b border-[var(--border-subtle)] py-1 last:border-b-0">
                      <WikiLink
                        name={n.name}
                        Icon={kindIcon(n)}
                        onClick={() => onSelectNode(n.id)}
                      />
                    </li>
                  ))}
                  {idx.unseenClaims.map((ref) => (
                    <ClaimRow key={ref.resp.id} claim={ref} onSelectNode={onSelectNode} />
                  ))}
                </ul>
              </PageSection>
            )}

            {idx.vagrant.length > 0 && (
              <PageSection title="Undescribed behaviour" count={idx.vagrant.length}>
                <p className="mb-2 text-2xs text-[var(--text-muted)]">
                  Found in the code with no claim describing it. Adopt into the contract, or
                  reject to mark the code for deletion.
                </p>
                <ul className="flex flex-col">
                  {idx.vagrant.map((ref) => (
                    <ClaimRow
                      key={ref.resp.id}
                      claim={ref}
                      onSelectNode={onSelectNode}
                      actions={
                        editor && (
                          <span className="flex shrink-0 items-center gap-2 pt-0.5 text-2xs">
                            <button
                              type="button"
                              onClick={() => editor.adoptResponsibility(ref.resp.id)}
                              className="font-medium text-indigo-600 hover:underline dark:text-indigo-400"
                            >
                              Adopt
                            </button>
                            <button
                              type="button"
                              onClick={() => editor.rejectResponsibility(ref.resp.id)}
                              className="font-medium text-[var(--text-tertiary)] hover:text-red-500 hover:underline dark:hover:text-red-400"
                            >
                              Reject
                            </button>
                          </span>
                        )
                      }
                    />
                  ))}
                </ul>
              </PageSection>
            )}

            {idx.stale.length > 0 && (
              <PageSection title="Stale claims" count={idx.stale.length}>
                <p className="mb-2 text-2xs text-[var(--text-muted)]">
                  The model asserts these but the code stopped doing them. Re-implement to
                  rebuild the code, or drop the claim if the behaviour was removed on purpose.
                </p>
                <ul className="flex flex-col">
                  {idx.stale.map((ref) => (
                    <ClaimRow
                      key={ref.resp.id}
                      claim={ref}
                      onSelectNode={onSelectNode}
                      actions={
                        editor && (
                          <span className="flex shrink-0 items-center gap-2 pt-0.5 text-2xs">
                            <button
                              type="button"
                              onClick={() => editor.reimplementResponsibility(ref.resp.id)}
                              className="font-medium text-indigo-600 hover:underline dark:text-indigo-400"
                            >
                              Re-implement
                            </button>
                            <button
                              type="button"
                              onClick={(e) =>
                                setConfirmDrop({
                                  rect: e.currentTarget.getBoundingClientRect(),
                                  label: "Drop this claim?",
                                  run: () => editor.dropResponsibility(ref.resp.id),
                                })
                              }
                              className="font-medium text-[var(--text-tertiary)] hover:text-red-500 hover:underline dark:hover:text-red-400"
                            >
                              Drop
                            </button>
                          </span>
                        )
                      }
                    />
                  ))}
                </ul>
              </PageSection>
            )}

            {idx.staleNodes.length > 0 && (
              <PageSection title="Code removed" count={idx.staleNodes.length}>
                <p className="mb-2 text-2xs text-[var(--text-muted)]">
                  These nodes lost their backing code entirely (a deleted file or folder).
                  Re-implement to rebuild the whole subtree, or drop it from the model.
                </p>
                <ul className="flex flex-col">
                  {idx.staleNodes.map((n) => (
                    <li
                      key={n.id}
                      className="flex items-center gap-2 border-b border-[var(--border-subtle)] py-2 last:border-b-0"
                    >
                      <div className="min-w-0 flex-1">
                        <WikiLink name={n.name} Icon={kindIcon(n)} onClick={() => onSelectNode(n.id)} />
                      </div>
                      {editor && (
                        <span className="flex shrink-0 items-center gap-2 pt-0.5 text-2xs">
                          <button
                            type="button"
                            onClick={() => editor.reimplementNode(n.id)}
                            className="font-medium text-indigo-600 hover:underline dark:text-indigo-400"
                          >
                            Re-implement
                          </button>
                          <button
                            type="button"
                            onClick={(e) =>
                              setConfirmDrop({
                                rect: e.currentTarget.getBoundingClientRect(),
                                label: "Drop this node and its subtree?",
                                run: () => editor.dropNode(n.id),
                              })
                            }
                            className="font-medium text-[var(--text-tertiary)] hover:text-red-500 hover:underline dark:hover:text-red-400"
                          >
                            Drop
                          </button>
                        </span>
                      )}
                    </li>
                  ))}
                </ul>
              </PageSection>
            )}

            {driftScopes.length > 0 && (
              <PageSection
                title="Possible drift"
                count={driftScopes.length}
                right={
                  <span className="flex items-center gap-2">
                    {onCheckDrift && (
                      <button
                        type="button"
                        onClick={onCheckDrift}
                        className={BTN_AGENT}
                      >
                        <GitCompare className="h-3 w-3" /> Run drift check
                      </button>
                    )}
                    {onDismissDrift && (
                      <button
                        type="button"
                        onClick={onDismissDrift}
                        title="Mark reconciled without a semantic check"
                        className={BTN}
                      >
                        Dismiss
                      </button>
                    )}
                  </span>
                }
              >
                <p className="mb-2 text-2xs text-[var(--text-muted)]">
                  Code under these nodes changed since the last reconcile. The drift check
                  reads the changes and flags claims that stopped holding.
                </p>
                <ul className="flex flex-col">
                  {driftScopes.map((s) => (
                    <li
                      key={s.nodeId}
                      className="border-b border-[var(--border-subtle)] py-2 last:border-b-0"
                    >
                      <button
                        type="button"
                        onClick={() => onSelectNode(s.nodeId)}
                        className="text-sm text-blue-700 hover:underline dark:text-blue-400"
                      >
                        {s.nodeName}
                      </button>
                      <ul className="mt-0.5 flex flex-col gap-px">
                        {s.changedFiles.map((f) => (
                          <li key={f} className="truncate font-mono text-2xs text-[var(--text-muted)]">
                            {f}
                          </li>
                        ))}
                      </ul>
                    </li>
                  ))}
                </ul>
              </PageSection>
            )}

            {anchors.length > 0 && (
              <PageSection title="Out-of-date anchors" count={anchors.length}>
                <p className="mb-2 text-2xs text-[var(--text-muted)]">
                  The code under these source anchors changed since the model last reconciled
                  against it — the mapped spans may have moved or gone.
                </p>
                <ul className="flex flex-col">
                  {anchors.map((a) => (
                    <li
                      key={`${a.hostId}:${a.file}:${a.symbol ?? ""}:${a.state}`}
                      className="flex items-center gap-2 border-b border-[var(--border-subtle)] py-1.5 last:border-b-0"
                    >
                      <button
                        type="button"
                        onClick={() => onSelectNode(a.hostId)}
                        className="shrink-0 text-sm text-blue-700 hover:underline dark:text-blue-400"
                      >
                        {a.hostName}
                      </button>
                      <span className="min-w-0 flex-1 truncate font-mono text-2xs text-[var(--text-muted)]">
                        {a.symbol ?? a.file}
                      </span>
                      <span className="shrink-0 text-2xs text-orange-600 dark:text-orange-400">
                        {ANCHOR_STATE_LABEL[a.state]}
                      </span>
                    </li>
                  ))}
                </ul>
              </PageSection>
            )}

            {idx.emptySymbols.length > 0 && (
              <PageSection title="Empty symbols" count={idx.emptySymbols.length}>
                <p className="mb-2 text-2xs text-[var(--text-muted)]">
                  Symbols carrying no semantic content. Give each a business responsibility or
                  remove it.
                </p>
                <ul className="flex flex-col">
                  {idx.emptySymbols.map((n) => (
                    <li key={n.id} className="border-b border-[var(--border-subtle)] py-1 last:border-b-0">
                      <WikiLink
                        name={n.name}
                        Icon={kindIcon(n)}
                        onClick={() => onSelectNode(n.id)}
                      />
                    </li>
                  ))}
                </ul>
              </PageSection>
            )}
          </>
        )}
      </SpecialBody>
      {confirmDrop && (
        <ConfirmPopover
          anchorRect={confirmDrop.rect}
          label={confirmDrop.label}
          confirmLabel="Drop"
          onConfirm={() => {
            confirmDrop.run();
            setConfirmDrop(null);
          }}
          onCancel={() => setConfirmDrop(null)}
        />
      )}
    </div>
  );
}

// --- dark code ------------------------------------------------------------------

export function DarkCodePage({
  model,
  report,
  onSelectNode,
}: {
  model: ScryModel;
  report: ModelHealthReport | null;
  onSelectNode: (id: string) => void;
}) {
  const { groups, total } = darkBoundaries(report);
  const nodeById = new Map(model.nodes.map((n) => [n.id, n] as const));

  return (
    <div className="flex min-w-0 flex-1 flex-col">
      <SpecialHeader
        title="Dark code"
        subtitle={
          total === 0
            ? "Every file under a boundary reads through to a claim"
            : `${total} file${total === 1 ? "" : "s"} under a node's boundary that no claim reads into`
        }
      />
      <SpecialBody>
        {total === 0 ? (
          <div className="flex flex-col items-center gap-3 px-6 py-16">
            <Check className="h-6 w-6 text-emerald-500 dark:text-emerald-400" />
            <p className="text-xs text-[var(--text-muted)]">
              No dark code — every file under a node's boundary is read by some claim.
            </p>
          </div>
        ) : (
          <>
            <p className="mb-4 mt-1 text-2xs text-[var(--text-muted)]">
              These files sit inside a node's boundary, but no claim in its subtree anchors to
              them — the lens can't see them. Most will be boilerplate (generated code, config,
              glue); scan for anything load-bearing the model is missing.
            </p>
            <div className="flex flex-col gap-5">
              {groups.map((g) => {
                const node = nodeById.get(g.nodeId);
                return (
                  <section key={g.nodeId}>
                    <div className="mb-1 flex items-baseline gap-2 border-b border-[var(--border-subtle)] pb-1">
                      <WikiLink
                        name={node?.name ?? g.nodeId}
                        Icon={node ? kindIcon(node) : undefined}
                        onClick={() => onSelectNode(g.nodeId)}
                      />
                      <span className="font-mono text-2xs text-[var(--text-ghost)]">
                        {g.files.length} dark
                      </span>
                    </div>
                    <ul className="flex flex-col gap-px pl-1">
                      {g.files.map((f) => (
                        <li
                          key={f}
                          className="truncate font-mono text-2xs text-[var(--text-muted)]"
                        >
                          {f}
                        </li>
                      ))}
                    </ul>
                  </section>
                );
              })}
            </div>
          </>
        )}
      </SpecialBody>
    </div>
  );
}

// --- unmapped claims ------------------------------------------------------------

/** Unmapped claims — the list behind the coverage percentage's complement:
 *  committed leaf claims the model asserts but that read through to no code.
 *  Symmetric with dark code, which is the same blind spot seen from the code's
 *  side.
 *
 *  Computed over the COMMITTED model and its source map — NOT the working draft —
 *  exactly as Rust's `compute_health` is, so the count agrees with the
 *  powerline's "N% claims mapped". (A claim already re-anchored in the pending
 *  plan still shows here until the work folds in; the percentage is committed-
 *  side too, so the two stay consistent.)
 *
 *  Anchorable = a leaf, non-external, non-person node. Each contributes its
 *  responsibilities (keyed by resp id), plus — if it declares any properties —
 *  one data-shape claim keyed by the node id. */
export function findUnmappedClaims(committed: ScryModel | null): {
  claims: ClaimRef[];
  shapes: Node[];
} {
  const claims: ClaimRef[] = [];
  const shapes: Node[] = [];
  if (!committed) return { claims, shapes };
  const hasChildren = new Set(
    committed.nodes.map((n) => n.parentId).filter(Boolean) as string[],
  );
  const sourceMap = committed.sourceMap ?? {};
  const anchored = (id: string) => (sourceMap[id] ?? []).length > 0;
  for (const node of committed.nodes) {
    if (hasChildren.has(node.id) || node.external || node.kind === "person") continue;
    for (const resp of node.responsibilities ?? []) {
      if (!anchored(resp.id)) claims.push({ node, resp });
    }
    if ((node.properties?.length ?? 0) > 0 && !anchored(node.id)) shapes.push(node);
  }
  return { claims, shapes };
}

export function UnmappedClaimsPage({
  committed,
  report,
  onSelectNode,
}: {
  committed: ScryModel | null;
  report: ModelHealthReport | null;
  onSelectNode: (id: string) => void;
}) {
  const { claims, shapes } = findUnmappedClaims(committed);
  const total = claims.length + shapes.length;
  const totals = report?.health.totals;
  const coverage =
    totals && totals.anchorable > 0
      ? Math.round((totals.anchored / totals.anchorable) * 100)
      : null;

  return (
    <div className="flex min-w-0 flex-1 flex-col">
      <SpecialHeader
        title="Unmapped claims"
        subtitle={
          total === 0
            ? "Every claim reads through to code"
            : `${total} claim${total === 1 ? "" : "s"} that say code exists but anchor to nothing${
                coverage != null ? ` — ${coverage}% mapped` : ""
              }`
        }
      />
      <SpecialBody>
        {total === 0 ? (
          <div className="flex flex-col items-center gap-3 px-6 py-16">
            <Check className="h-6 w-6 text-emerald-500 dark:text-emerald-400" />
            <p className="text-xs text-[var(--text-muted)]">
              No unmapped claims — every committed claim on a leaf reads through to source.
            </p>
          </div>
        ) : (
          <>
            <p className="mb-4 mt-1 text-2xs text-[var(--text-muted)]">
              These claims say code exists but anchor to nothing — they can't be read through to
              source. Have the agent re-map, or fix the claim. A claim already re-anchored in a
              pending plan clears once the work is folded into the committed model.
            </p>
            <ul className="flex flex-col">
              {claims.map((ref) => (
                <ClaimRow key={ref.resp.id} claim={ref} onSelectNode={onSelectNode} />
              ))}
              {shapes.map((n) => (
                <li
                  key={n.id}
                  className="flex items-center gap-2 border-b border-[var(--border-subtle)] py-2 last:border-b-0"
                >
                  <WikiLink name={n.name} Icon={kindIcon(n)} onClick={() => onSelectNode(n.id)} />
                  <span className="text-2xs text-[var(--text-ghost)]">data shape</span>
                </li>
              ))}
            </ul>
          </>
        )}
      </SpecialBody>
    </div>
  );
}
