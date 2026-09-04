import { useEffect, useRef, useState } from "react";
import { Check, Crosshair, GitCompare, PenLine, X } from "lucide-react";
import { ConfirmPopover } from "../ConfirmPopover";
import type { ScryModel, Node, Responsibility, SchemaProperty, DriftScope } from "../viewmodel";
import { isNodeEmpty } from "../viewmodel";
import type { Editor } from "../editor";
import type { ClaimProbeStatus, ClaimTestStatus, ModelHealthReport, TestFinding } from "../health";
import { ANCHOR_STATE_LABEL, collapseAnchors, testFindings } from "../health";
import { kindIcon } from "../kindIcon";
import { respElementId, propElementId } from "../SourceSection";
import { BTN, BTN_AGENT, BTN_DANGER, BTN_GO, jumpTo, LINK, PageSection, WikiLink, WordDiffText } from "../pagekit";
import { DRIFT_HINT, DRIFT_RULE } from "../diffkit";
import { ANCHOR_CALM, serializeEars, StatementText, stripMarkup } from "../markup";
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
  detail,
}: {
  claim: ClaimRef;
  onSelectNode: (id: string) => void;
  actions?: React.ReactNode;
  /** Extra lines under the host link — e.g. what a probe found. */
  detail?: React.ReactNode;
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
          className="block w-full truncate text-left font-mono text-sm text-[var(--text-secondary)] hover:text-[var(--text)] hover:underline"
          title="Open on its page"
        >
          {claim.resp.statement ? (
            <StatementText text={claim.resp.statement} anchor={ANCHOR_CALM} />
          ) : (
            "Untitled responsibility"
          )}
        </button>
        <span className="text-xs text-[var(--text-muted)]">
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
          <div className={`mt-0.5 text-xs ${DRIFT_RULE}`}>
            <span className={DRIFT_HINT}>drift proposes:</span>{" "}
            <span className="font-mono text-sm text-[var(--text-secondary)]">
              <WordDiffText from={stripMarkup(claim.resp.statement)} to={stripMarkup(claim.resp.staleProposal)} />
            </span>
          </div>
        )}
        {detail}
      </div>
      {actions}
    </li>
  );
}

/** Inline reword: a textarea seeded with the current wording, Save / Cancel.
 *  Shared by the amendment rows here and the inbox cards, so "reword" is one
 *  affordance everywhere. Enter saves, Escape cancels. */
export function RewordEditor({
  initial,
  onSave,
  onCancel,
}: {
  initial: string;
  onSave: (text: string) => void;
  onCancel: () => void;
}) {
  const [text, setText] = useState(initial);
  const ref = useRef<HTMLTextAreaElement>(null);
  useEffect(() => {
    ref.current?.focus();
    ref.current?.select();
  }, []);
  const save = () => {
    const t = text.trim();
    if (t) onSave(t);
  };
  return (
    <div className="flex w-full flex-col gap-1.5">
      <textarea
        ref={ref}
        value={text}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && !e.shiftKey) {
            e.preventDefault();
            save();
          } else if (e.key === "Escape") {
            e.preventDefault();
            onCancel();
          }
        }}
        rows={2}
        className="w-full resize-y rounded border border-[var(--border-strong)] bg-[var(--surface)] px-2 py-1 font-mono text-sm text-[var(--text)] outline-none focus:border-[var(--accent)]"
      />
      <div className="flex items-center gap-2 text-xs">
        <button type="button" onClick={save} className={BTN_GO} disabled={!text.trim()}>
          Save wording
        </button>
        <button type="button" onClick={onCancel} className={BTN}>
          Cancel
        </button>
        <span className="text-[var(--text-ghost)]">Enter saves · Esc cancels</span>
      </div>
    </div>
  );
}

/** One post-sign-off amendment / addition awaiting a verdict: the approved
 *  text against the amended text (an addition has none — "not in the
 *  signed-off plan"), with adopt / reject / reword inline. */
function AmendmentRow({
  claim,
  onSelectNode,
  editor,
}: {
  claim: ClaimRef;
  onSelectNode: (id: string) => void;
  editor: Editor | undefined;
}) {
  const [rewording, setRewording] = useState(false);
  const resp = claim.resp;
  const addition = resp.vagrantOrigin === "addition";
  return (
    <ClaimRow
      claim={claim}
      onSelectNode={onSelectNode}
      detail={
        <div className="mt-0.5 border-l-2 border-violet-500/30 pl-3 text-xs dark:border-violet-400/30">
          {addition ? (
            <span className="italic text-violet-700/80 dark:text-violet-400/80">not in the signed-off plan</span>
          ) : (
            <>
              <span className="text-violet-700/80 dark:text-violet-400/80">approved → amended:</span>{" "}
              <span className="font-mono text-sm text-[var(--text-secondary)]">
                <WordDiffText
                  from={stripMarkup(resp.approvedStatement ?? "")}
                  to={stripMarkup(resp.statement)}
                />
              </span>
            </>
          )}
          {rewording && editor && (
            <div className="mt-1.5">
              <RewordEditor
                initial={stripMarkup(resp.statement)}
                onSave={(t) => {
                  editor.rewordResponsibility(resp.id, serializeEars(t));
                  setRewording(false);
                }}
                onCancel={() => setRewording(false)}
              />
            </div>
          )}
        </div>
      }
      actions={
        editor && (
          <span className="flex shrink-0 items-center gap-2 pt-0.5 text-xs">
            <button
              type="button"
              onClick={() => editor.adoptResponsibility(resp.id)}
              className={BTN_GO}
              title={
                addition
                  ? "The added claim becomes intent — it folds once built and verified, else stays pending"
                  : "The amended text becomes the intent — it folds once built and verified, else stays pending"
              }
            >
              Adopt
            </button>
            <button
              type="button"
              onClick={() => editor.rejectResponsibility(resp.id)}
              className={BTN_DANGER}
              title={
                addition
                  ? "Remove the claim the plan never approved"
                  : "Restore the approved text — the agent built something else; the work stays open"
              }
            >
              Reject
            </button>
            <button
              type="button"
              onClick={() => setRewording((r) => !r)}
              className={BTN}
              title="Replace both with your own wording"
            >
              <PenLine className="h-3 w-3" /> Reword
            </button>
          </span>
        )
      }
    />
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
        <span className="text-xs text-[var(--text-muted)]">
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
  /** Claims the AGENT reworded or added after the developer signed off their
   *  change (`vagrantOrigin`) — proposals awaiting adopt / reject / reword.
   *  Listed apart from code-discovered vagrants: "what the agent changed" vs
   *  "what the code does that I never said". */
  amendments: ClaimRef[];
  /** Code-discovered vagrant claims (amendments excluded). */
  vagrant: ClaimRef[];
  vagrantProps: PropRef[];
  stale: ClaimRef[];
  staleProps: PropRef[];
  staleNodes: Node[];
  emptySymbols: Node[];
  unseenNodes: Node[];
  unseenClaims: ClaimRef[];
  /** Architecture nodes wired to no relationship link — edgeless on the canvas
   *  (from the health report; symbols exempt). */
  disconnected: Node[];
  /** Tests not holding: a current failing verdict, or a probe that found a
   *  break the test did not catch. Counted in `total` — each is wrong now. */
  testsNotHolding: TestFinding[];
  /** Committed testable claims with no test attached. Listed, but NOT in
   *  `total`: a standing gap to close, not a verdict awaiting a human, and
   *  it would swamp the counter on any model built before rule 22. */
  untested: TestFinding[];
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
  tests: {
    committed: ScryModel | null;
    verdicts: Record<string, ClaimTestStatus>;
    probes: Record<string, ClaimProbeStatus>;
  } = { committed: null, verdicts: {}, probes: {} },
): ReviewIndex {
  const amendments: ClaimRef[] = [];
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
      if (resp.vagrantOrigin) amendments.push({ node, resp });
      else if (resp.vagrant) vagrant.push({ node, resp });
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
  // Edgeless architecture nodes, from the health report. A node whose code is
  // gone (stale) is already surfaced under "Code removed" — don't list it twice.
  const disconnectedIds = new Set(report?.health.disconnected ?? []);
  const disconnected = model.nodes.filter(
    (n) => disconnectedIds.has(n.id) && !staleNodeIds.has(n.id),
  );
  const findings = testFindings(model, tests.committed, tests.verdicts, tests.probes);
  const testsNotHolding = findings.filter((f) => f.kind !== "untested");
  const untested = findings.filter((f) => f.kind === "untested");
  const total =
    testsNotHolding.length +
    amendments.length +
    vagrant.length +
    vagrantProps.length +
    stale.length +
    staleProps.length +
    staleNodes.length +
    emptySymbols.length +
    unseenNodes.length +
    unseenClaims.length +
    disconnected.length +
    driftScopes.length +
    collapseAnchors(report?.anchors ?? []).length;
  return { amendments, vagrant, vagrantProps, stale, staleProps, staleNodes, emptySymbols, unseenNodes, unseenClaims, disconnected, testsNotHolding, untested, total };
}

export function NeedsReviewPage({
  model,
  report,
  driftScopes,
  newNodeIds,
  newRespIds,
  committed = null,
  testVerdicts = {},
  probeResults = {},
  editor,
  onSelectNode,
  onCheckDrift,
  onDismissDrift,
  onClearAllNew,
}: {
  model: ScryModel;
  report: ModelHealthReport | null;
  committed?: ScryModel | null;
  testVerdicts?: Record<string, ClaimTestStatus>;
  probeResults?: Record<string, ClaimProbeStatus>;
  driftScopes: DriftScope[];
  newNodeIds: ReadonlySet<string>;
  newRespIds: ReadonlySet<string>;
  editor: Editor | undefined;
  onSelectNode: (id: string) => void;
  onCheckDrift?: () => void;
  onDismissDrift?: () => void;
  onClearAllNew: () => void;
}) {
  const idx = buildReviewIndex(model, report, driftScopes, newNodeIds, newRespIds, {
    committed,
    verdicts: testVerdicts,
    probes: probeResults,
  });
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
        {idx.total === 0 && (
          <div className="flex flex-col items-center gap-3 px-6 py-16">
            <Check className="h-6 w-6 text-emerald-500 dark:text-emerald-400" />
            <p className="text-xs text-[var(--text-muted)]">
              Nothing needs review — the model is current with the code.
            </p>
          </div>
        )}
        {idx.total > 0 && (
          <>
            {idx.testsNotHolding.length > 0 && (
              <PageSection title="Tests not holding" hint={"A failing test, or a test that stayed green while its claim's code was deliberately broken. Either way the claim is not held \u2014 fix the code, or strengthen the test and re-run for a fresh verdict."} count={idx.testsNotHolding.length}>
                <ul className="flex flex-col">
                  {idx.testsNotHolding.map((f) => (
                    <ClaimRow
                      key={f.resp.id}
                      claim={f}
                      onSelectNode={onSelectNode}
                      actions={
                        <span className="flex shrink-0 items-center gap-1 pt-0.5 font-mono text-xs text-red-600 dark:text-red-400">
                          {f.kind === "failing" ? (
                            <>
                              <X className="h-3 w-3" /> failing
                            </>
                          ) : (
                            <>
                              <Crosshair className="h-3 w-3" /> {f.survivors?.length ?? 0} uncaught
                            </>
                          )}
                        </span>
                      }
                      detail={
                        f.kind === "hollow" && f.survivors && f.survivors.length > 0 ? (
                          <ul className="mt-0.5 flex flex-col gap-px text-xs text-[var(--text-muted)]">
                            {f.survivors.map((sv, i) => (
                              <li key={i} className="truncate" title={sv}>
                                • {sv}
                              </li>
                            ))}
                          </ul>
                        ) : undefined
                      }
                    />
                  ))}
                </ul>
              </PageSection>
            )}

            {(idx.unseenNodes.length > 0 || idx.unseenClaims.length > 0) && (
              <PageSection
                title="Unreviewed agent changes" hint={"Landed from the agent and not yet looked at. Opening an item clears it; see Recent changes for the field-level diffs."}
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

            {idx.amendments.length > 0 && (
              <PageSection title="Changed after sign-off" hint={"The agent reworded or added these after you signed off their change. Each is a proposal, not intent: adopt it (it folds once built and verified), reject it (the approved text comes back and the work stays open), or reword it yourself."} count={idx.amendments.length}>
                <ul className="flex flex-col">
                  {idx.amendments.map((ref) => (
                    <AmendmentRow key={ref.resp.id} claim={ref} onSelectNode={onSelectNode} editor={editor} />
                  ))}
                </ul>
              </PageSection>
            )}

            {idx.vagrant.length + idx.vagrantProps.length > 0 && (
              <PageSection
                title="Undescribed in code" hint={"Found in the code with no claim or field describing it. Adopt into the contract, or reject to mark the code for deletion."}
                count={idx.vagrant.length + idx.vagrantProps.length}
              >
                <ul className="flex flex-col">
                  {idx.vagrant.map((ref) => (
                    <ClaimRow
                      key={ref.resp.id}
                      claim={ref}
                      onSelectNode={onSelectNode}
                      actions={
                        editor && (
                          <span className="flex shrink-0 items-center gap-2 pt-0.5 text-xs">
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
                          <span className="flex shrink-0 items-center gap-2 pt-0.5 text-xs">
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
                title="Stale claims" hint={"The model asserts these but the code no longer matches. Where drift proposes a reword, accept it to bring the claim in line with the code (no rebuild) \u2014 or keep the claim and rebuild the code. Where the behaviour vanished outright, drop the claim or rebuild the code."}
                count={idx.stale.length + idx.staleProps.length}
              >
                <ul className="flex flex-col">
                  {idx.stale.map((ref) => (
                    <ClaimRow
                      key={ref.resp.id}
                      claim={ref}
                      onSelectNode={onSelectNode}
                      actions={
                        editor && (
                          <span className="flex shrink-0 items-center gap-2 pt-0.5 text-xs">
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
                              className={BTN}
                              title="Keep this claim as written and rebuild the behaviour in code — files a to-do the agent implements (folds back when done)."
                            >
                              Rebuild code
                            </button>
                            {!ref.resp.staleProposal && (
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
                                title="The behaviour was removed on purpose — drop the claim from the model."
                              >
                                Drop
                              </button>
                            )}
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
                          <span className="flex shrink-0 items-center gap-2 pt-0.5 text-xs">
                            <button
                              type="button"
                              onClick={() => editor.reimplementProperty(ref.node.id, ref.prop.label)}
                              className={BTN}
                              title="Keep this field and rebuild its backing code — files a to-do the agent implements."
                            >
                              Rebuild code
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
              <PageSection title="Code removed" hint={"These nodes lost their backing code entirely (a deleted file or folder). Rebuild the whole subtree in code, or drop it from the model."} count={idx.staleNodes.length}>
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
                        <span className="flex shrink-0 items-center gap-2 pt-0.5 text-xs">
                          <button
                            type="button"
                            onClick={() => editor.reimplementNode(n.id)}
                            className={BTN}
                            title="Keep this node and rebuild its whole subtree in code — files a to-do the agent implements."
                          >
                            Rebuild code
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
                title="Possible drift" hint={"Code under these nodes changed since the last reconcile. The drift check reads the changes and flags claims that stopped holding."}
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
                          <li key={f} className="truncate font-mono text-xs text-[var(--text-muted)]">
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
              <PageSection title="Out-of-date anchors" hint={"The code under these source anchors changed since the model last reconciled against it \u2014 the mapped spans may have moved or gone."} count={anchors.length}>
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
                      <span className="min-w-0 flex-1 truncate font-mono text-xs text-[var(--text-muted)]">
                        {a.symbol ?? a.file}
                      </span>
                      {a.key.startsWith("test:") && (
                        <span
                          className="shrink-0 text-xs text-[var(--text-muted)]"
                          title="This is a claim's BACKING TEST, not its implementation — the test changed or vanished."
                        >
                          test
                        </span>
                      )}
                      <span className="shrink-0 text-xs text-orange-700 dark:text-orange-400">
                        {ANCHOR_STATE_LABEL[a.state]}
                      </span>
                    </li>
                  ))}
                </ul>
              </PageSection>
            )}

            {idx.emptySymbols.length > 0 && (
              <PageSection title="Empty symbols" hint={"Symbols carrying no semantic content. Give each a business responsibility or remove it."} count={idx.emptySymbols.length}>
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

            {idx.disconnected.length > 0 && (
              <PageSection title="Disconnected nodes" hint={"No relationship links these to anything, so they float edgeless on the canvas. Wire each into the relationship it performs, or confirm it belongs on its own."} count={idx.disconnected.length}>
                <ul className="flex flex-col">
                  {idx.disconnected.map((n) => (
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
        {idx.untested.length > 0 && (
          <PageSection title="Untested claims" hint={"Committed When/While/If claims with no test attached \u2014 each names a trigger to arrange and a response to assert, so the test is already specified. Not counted above: a standing gap to close, not a verdict to give."} count={idx.untested.length}>
            <ul className="flex flex-col">
              {idx.untested.map((f) => (
                <ClaimRow key={f.resp.id} claim={f} onSelectNode={onSelectNode} />
              ))}
            </ul>
          </PageSection>
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
