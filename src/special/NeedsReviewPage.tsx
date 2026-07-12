import { useState } from "react";
import { Check, GitCompare } from "lucide-react";
import { ConfirmPopover } from "../ConfirmPopover";
import type { ScryModel, Node, Responsibility, SchemaProperty, DriftScope } from "../viewmodel";
import { isNodeEmpty } from "../viewmodel";
import type { Editor } from "../editor";
import type { ModelHealthReport } from "../health";
import { ANCHOR_STATE_LABEL, collapseAnchors } from "../health";
import { kindIcon } from "../kindIcon";
import { respElementId, propElementId } from "../SourceSection";
import { BTN, BTN_AGENT, BTN_DANGER, BTN_GO, jumpTo, LINK, PageSection, WikiLink, WordDiffText } from "../pagekit";
import { SpecialBody, SpecialHeader } from "./shell";

// --- needs review ---------------------------------------------------------------

export interface ClaimRef {
  node: Node;
  resp: Responsibility;
}

/** One claim row: the statement (opens the claim on its own page, flashing it
 *  once rendered) and the node it sits on. Shared by Needs review and the
 *  Unmapped claims page so the two render claims identically. */
export function ClaimRow({
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
            className={LINK}
          >
            {claim.node.name || "Untitled"}
          </button>
        </span>
        {claim.resp.staleProposal && (
          <div className="mt-0.5 text-2xs text-[var(--text-muted)]">
            drift proposes:{" "}
            <span className="text-[var(--text-secondary)]">
              <WordDiffText from={claim.resp.statement} to={claim.resp.staleProposal} />
            </span>
          </div>
        )}
      </div>
      {actions}
    </li>
  );
}

interface PropRef {
  node: Node;
  prop: SchemaProperty;
}

/** One data-field row — the property-level mirror of {@link ClaimRow}. Opens the
 *  owning node and flashes the field; properties have no id, so it's addressed by
 *  (node, label). */
function PropRow({
  pref,
  onSelectNode,
  actions,
}: {
  pref: PropRef;
  onSelectNode: (id: string) => void;
  actions?: React.ReactNode;
}) {
  const goToProp = () => {
    onSelectNode(pref.node.id);
    window.setTimeout(() => jumpTo(propElementId(pref.node.id, pref.prop.label)), 250);
  };
  return (
    <li className="flex items-start gap-2 border-b border-[var(--border-subtle)] py-2 last:border-b-0">
      <div className="min-w-0 flex-1">
        <button
          type="button"
          onClick={goToProp}
          className="block w-full truncate text-left text-sm text-[var(--text-secondary)] hover:text-[var(--text)] hover:underline"
          title="Open on its page"
        >
          <span className="font-mono">{pref.prop.label || "field"}</span>
          {pref.prop.description && (
            <span className="text-[var(--text-muted)]"> — {pref.prop.description}</span>
          )}
        </button>
        <span className="text-2xs text-[var(--text-muted)]">
          on{" "}
          <button
            type="button"
            onClick={() => onSelectNode(pref.node.id)}
            className={LINK}
          >
            {pref.node.name || "Untitled"}
          </button>
        </span>
      </div>
      {actions}
    </li>
  );
}

export interface ReviewIndex {
  vagrant: ClaimRef[];
  vagrantProps: PropRef[];
  stale: ClaimRef[];
  staleProps: PropRef[];
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
  const vagrantProps: PropRef[] = [];
  const stale: ClaimRef[] = [];
  const staleProps: PropRef[] = [];
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
    // Data fields drift the same way — a vagrant/stale property awaits the same
    // adopt/reject and re-implement/drop verdicts as a claim.
    for (const prop of node.properties ?? []) {
      if (prop.vagrant) vagrantProps.push({ node, prop });
      if (prop.stale && !staleNodeIds.has(node.id)) staleProps.push({ node, prop });
    }
  }
  const emptySymbols = model.nodes.filter(isNodeEmpty);
  const unseenNodes = model.nodes.filter((n) => newNodeIds.has(n.id));
  const total =
    vagrant.length +
    vagrantProps.length +
    stale.length +
    staleProps.length +
    staleNodes.length +
    emptySymbols.length +
    unseenNodes.length +
    unseenClaims.length +
    driftScopes.length +
    collapseAnchors(report?.anchors ?? []).length;
  return { vagrant, vagrantProps, stale, staleProps, staleNodes, emptySymbols, unseenNodes, unseenClaims, total };
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

            {idx.vagrant.length + idx.vagrantProps.length > 0 && (
              <PageSection
                title="Undescribed in code"
                count={idx.vagrant.length + idx.vagrantProps.length}
              >
                <p className="mb-2 text-2xs text-[var(--text-muted)]">
                  Found in the code with no claim or field describing it. Adopt into the contract,
                  or reject to mark the code for deletion.
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
                              className={BTN_GO}
                            >
                              Adopt
                            </button>
                            <button
                              type="button"
                              onClick={() => editor.rejectResponsibility(ref.resp.id)}
                              className={BTN_DANGER}
                            >
                              Reject
                            </button>
                          </span>
                        )
                      }
                    />
                  ))}
                  {idx.vagrantProps.map((ref) => (
                    <PropRow
                      key={`${ref.node.id}.${ref.prop.label}`}
                      pref={ref}
                      onSelectNode={onSelectNode}
                      actions={
                        editor && (
                          <span className="flex shrink-0 items-center gap-2 pt-0.5 text-2xs">
                            <button
                              type="button"
                              onClick={() => editor.adoptProperty(ref.node.id, ref.prop.label)}
                              className={BTN_GO}
                            >
                              Adopt
                            </button>
                            <button
                              type="button"
                              onClick={() => editor.rejectProperty(ref.node.id, ref.prop.label)}
                              className={BTN_DANGER}
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

            {idx.stale.length + idx.staleProps.length > 0 && (
              <PageSection
                title="Stale claims"
                count={idx.stale.length + idx.staleProps.length}
              >
                <p className="mb-2 text-2xs text-[var(--text-muted)]">
                  The model asserts these but the code no longer matches. Where drift proposes a
                  reword, accept it to bring the claim in line with the code (no rebuild). Otherwise
                  re-implement to rebuild the code, or drop the claim if the behaviour was removed.
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
                            {ref.resp.staleProposal && (
                              <button
                                type="button"
                                onClick={() =>
                                  editor.rewordResponsibility(ref.resp.id, ref.resp.staleProposal!)
                                }
                                className={BTN_GO}
                                title="The code changed what it does — accept drift's wording into the model. No rebuild: the code already does this."
                              >
                                Accept reword
                              </button>
                            )}
                            <button
                              type="button"
                              onClick={() => editor.reimplementResponsibility(ref.resp.id)}
                              className={ref.resp.staleProposal ? BTN : BTN_GO}
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
                              className={BTN_DANGER}
                            >
                              Drop
                            </button>
                          </span>
                        )
                      }
                    />
                  ))}
                  {idx.staleProps.map((ref) => (
                    <PropRow
                      key={`${ref.node.id}.${ref.prop.label}`}
                      pref={ref}
                      onSelectNode={onSelectNode}
                      actions={
                        editor && (
                          <span className="flex shrink-0 items-center gap-2 pt-0.5 text-2xs">
                            <button
                              type="button"
                              onClick={() => editor.reimplementProperty(ref.node.id, ref.prop.label)}
                              className={BTN_GO}
                            >
                              Re-implement
                            </button>
                            <button
                              type="button"
                              onClick={(e) =>
                                setConfirmDrop({
                                  rect: e.currentTarget.getBoundingClientRect(),
                                  label: "Drop this field?",
                                  run: () => editor.dropProperty(ref.node.id, ref.prop.label),
                                })
                              }
                              className={BTN_DANGER}
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
                            className={BTN_GO}
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
                            className={BTN_DANGER}
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
                        className={`text-sm ${LINK}`}
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
                        className={`shrink-0 text-sm ${LINK}`}
                      >
                        {a.hostName}
                      </button>
                      <span className="min-w-0 flex-1 truncate font-mono text-2xs text-[var(--text-muted)]">
                        {a.symbol ?? a.file}
                      </span>
                      {a.key.startsWith("verify:") && (
                        <span
                          className="shrink-0 text-2xs text-[var(--text-muted)]"
                          title="This is a claim's BACKING TEST, not its implementation — the test changed or vanished."
                        >
                          test
                        </span>
                      )}
                      <span className="shrink-0 text-2xs text-orange-700 dark:text-orange-400">
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
