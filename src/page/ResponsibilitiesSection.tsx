import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { CornerDownRight, FlaskConical } from "lucide-react";
import type { ScryModel, Responsibility, SourceLocation } from "../viewmodel";
import type { Editor } from "../editor";
import type { AnchorState } from "../health";
import { FLAG_COLORS, VERIFY_PILLS } from "../statusColors";
import {
  buildElementDiff,
  CHANGE_OF,
  ChangeGlyph,
  DIFF_TINT,
  type ElementDiff,
  VerdictBar,
} from "../diffkit";
import { ClaimSource, respElementId } from "../SourceSection";
import { usePageMenu, useCopyId, copyIdItem } from "../pageMenu";
import {
  BTN,
  BTN_DANGER,
  BTN_GO,
  Editable,
  Empty,
  PageSection,
  SectionEditor,
  WordDiffText,
} from "../pagekit";
import { CTL_DROW, CTL_SROW, DIR_HL, RESP_ROW, STMT_HL } from "./kit";

// --- responsibilities (the diff view) ---------------------------------------

/** How one claim diverges from the committed model. The Overview reads as a
 *  diff: `added` (in the plan, not yet committed), `reworded` (statement or
 *  directives moved), `deleted` (committed but dropped from the plan, shown so
 *  it can be restored), `relocated` (committed here but moved to another host in
 *  the plan — shown for context, NOT restorable), `vagrant` (code does it, the
 *  model never claimed it — adopt or reject), or `unchanged`. */
interface RespDiffRow extends Omit<ElementDiff<Responsibility>, "item"> {
  resp: Responsibility;
}

/** Map every planned responsibility id to the display name of the node/group
 *  that holds it. Lets {@link buildRespDiff} tell a claim that *moved* to
 *  another host (id still present elsewhere in the plan) from one genuinely
 *  dropped — the former must not offer a Restore, which would re-add the id
 *  here and duplicate it across two hosts (both diff engines key by id, so one
 *  copy would silently vanish and corrupt the plan). */
export function plannedRespHosts(model: ScryModel): Map<string, string> {
  const hosts = new Map<string, string>();
  for (const n of model.nodes)
    for (const r of n.responsibilities ?? []) hosts.set(r.id, n.name || n.id);
  for (const g of model.groups)
    for (const r of g.responsibilities ?? []) hosts.set(r.id, g.name || "Group");
  return hosts;
}

/** Build the diff rows for a host's claims: planned claims in order (each tagged
 *  added / reworded / vagrant / unchanged against the committed copy), then any
 *  committed claims the plan dropped — as restorable `deleted` rows, or, when the
 *  id now lives on another host, as read-only `relocated` rows. `plannedHosts`
 *  (respId → owning host name across the whole plan) drives that distinction. */
function buildRespDiff(
  planned: Responsibility[],
  committed: Responsibility[],
  plannedHosts?: Map<string, string>,
): RespDiffRow[] {
  return buildElementDiff(planned, committed, {
    key: (r) => r.id,
    changed: (prev, r) =>
      prev.statement !== r.statement ||
      (prev.directives ?? []).join("\n") !== (r.directives ?? []).join("\n"),
    vagrant: (r) => !!r.vagrant,
    relocatedTo: (r) => plannedHosts?.get(r.id),
  }).map(({ item, ...row }) => ({ ...row, resp: item }));
}

/** Render text with word-level add/remove highlighting (a reworded claim). When
 *  `from`/`to` are equal it's just the plain text. */
export function ResponsibilitiesSection({
  host,
  hostId,
  resps,
  prevResps,
  plannedHosts,
  sourceMap,
  verifyMap,
  verifyStates,
  projectPath,
  leafHost,
  mintId,
  editor,
  editing,
  onToggle,
}: {
  host: "node" | "group";
  hostId: string;
  resps: Responsibility[];
  /** The committed copy of this host's claims — the diff base for the rows. */
  prevResps: Responsibility[];
  /** respId → owning host name across the whole plan; distinguishes a relocated
   *  claim from a deleted one so we never offer a duplicating Restore. */
  plannedHosts: Map<string, string>;
  /** respId → source locations, for the inline `↳ file:range` peeks per claim. */
  sourceMap: Record<string, SourceLocation[]>;
  /** respId → backing-test locations (the verify dimension). */
  verifyMap: Record<string, SourceLocation[]>;
  /** respId → fingerprint state of the backing test, when it regressed. */
  verifyStates: Record<string, AnchorState>;
  projectPath: string | null;
  /** Whether claims here must anchor to source (leaf node). Structural hosts
   *  discharge through their subtree and never flag "unmapped". */
  leafHost: boolean;
  /** Mint a fresh claim id clear of every host in BOTH layers (plus the
   *  editor's own draft rows) — claim ids are globally unique, and a per-host
   *  mint can duplicate one across hosts, silently corrupting the diff. */
  mintId: (draft: Responsibility[]) => string;
  editor: Editor | undefined;
  editing: boolean;
  onToggle: () => void;
}) {
  const diffRows = buildRespDiff(resps, prevResps, plannedHosts);
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
          mintId={mintId}
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
              row={row}
              host={host}
              locations={sourceMap[row.resp.id] ?? []}
              verifyLocations={verifyMap[row.resp.id] ?? []}
              verifyState={verifyStates[row.resp.id] ?? null}
              projectPath={projectPath}
              leafHost={leafHost}
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
  mintId,
  editor,
  onClose,
}: {
  host: "node" | "group";
  hostId: string;
  initial: Responsibility[];
  seedNewRow: boolean;
  mintId: (draft: Responsibility[]) => string;
  editor: Editor;
  onClose: () => void;
}) {
  const seededId = seedNewRow ? mintId(initial) : null;
  const start: RespDraftRow[] = seededId
    ? [...initial, { id: seededId, statement: "" }]
    : initial;

  const commit = (draft: RespDraftRow[]) => {
    // Deleted rows (draft-only `removed` marker) and blank statements (invalid —
    // the backend flags them) drop out on commit; the removal loop below calls
    // removeResponsibility for dropped existing rows.
    const cleaned = draft
      .filter((r) => !r.removed && r.statement.trim() !== "")
      .map(({ removed: _removed, ...r }) => {
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
    <SectionEditor<RespDraftRow[]>
      initial={start}
      onCommit={commit}
      onClose={onClose}
      footerExtra={(setDraft) => (
        <button
          type="button"
          onClick={() =>
            setDraft((d) => [
              ...d,
              { id: mintId(d), statement: "" },
            ])
          }
          className={BTN}
        >
          Add responsibility
        </button>
      )}
    >
      {(draft, setDraft) => {
        const patchRow = (id: string, patch: Partial<RespDraftRow>) =>
          setDraft((d) => d.map((r) => (r.id === id ? { ...r, ...patch } : r)));
        // Delete marks the row (struck, with Undo) instead of splicing it —
        // a stray click on the hover-revealed Delete can't destroy typed work.
        const removeRow = (id: string) => patchRow(id, { removed: true });
        const restoreRow = (id: string) => patchRow(id, { removed: false });
        return draft.length === 0 ? (
          <Empty>No responsibilities.</Empty>
        ) : (
          <ul className="-mx-2 flex flex-col">
            {draft.map((r, i) =>
              r.removed ? (
                <RemovedRespRow key={r.id} resp={r} index={i + 1} onUndo={() => restoreRow(r.id)} />
              ) : (
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
              ),
            )}
          </ul>
        );
      }}
    </SectionEditor>
  );
}

/** A responsibility in the edit draft: `removed` is the soft-delete marker —
 *  the row stays visible (struck, with Undo) and drops out only on Done. */
type RespDraftRow = Responsibility & { removed?: boolean };

/** A soft-deleted claim row in the editor — struck through with an inline Undo,
 *  the same recipe as {@link MembersEditor}'s dropped rows. */
function RemovedRespRow({
  resp,
  index,
  onUndo,
}: {
  resp: RespDraftRow;
  index: number;
  onUndo: () => void;
}) {
  return (
    <li className={`${RESP_ROW} py-[1.5px] [&:not(:first-child)]:mt-2.5`}>
      <ChangeGlyph kind="delete" />
      <span className="select-none pr-2.5 text-right font-mono text-2xs tabular-nums text-[var(--text-ghost)]">
        {index}
      </span>
      <div className="flex min-w-0 items-baseline gap-2 font-mono text-sm leading-relaxed">
        <span className="min-w-0 truncate text-[var(--text-muted)] line-through decoration-red-400/50">
          {resp.statement.trim() || "(empty)"}
        </span>
        <button type="button" onClick={onUndo} className={BTN}>
          Undo
        </button>
      </div>
    </li>
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
  row,
  host,
  locations,
  verifyLocations,
  verifyState,
  projectPath,
  leafHost,
  onRestore,
  editor,
}: {
  row: RespDiffRow;
  host: "node" | "group";
  /** This claim's source locations — rendered inline with expandable peeks. */
  locations: SourceLocation[];
  /** The claim's backing tests (verify dimension). */
  verifyLocations: SourceLocation[];
  /** Fingerprint state of the backing test, when it regressed since reconcile. */
  verifyState: AnchorState | null;
  projectPath: string | null;
  leafHost: boolean;
  onRestore: () => void;
  editor: Editor | undefined;
}) {
  const { resp, kind, prev, index } = row;
  const openMenu = usePageMenu();
  const copyId = useCopyId();
  const deleted = kind === "deleted";
  const relocated = kind === "relocated";
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
  // The verification pill shows on live claims only — a deleted/relocated
  // row's test link is context, not a badge.
  const tested = !deleted && !relocated && verifyLocations.length > 0;
  const hasMeta = resp.stale === true || unmapped || tested;
  // Drift's reword proposal: `null` = showing the accept/edit affordance, a
  // string = editing the wording before accepting.
  const [rewordDraft, setRewordDraft] = useState<string | null>(null);

  const contentColor = deleted || relocated
    ? "text-[var(--text-muted)]"
    : kind === "unchanged"
      ? "text-[var(--text-secondary)]"
      : "text-[var(--text)]";

  return (
    <>
    <li
      id={respElementId(resp.id)}
      onContextMenu={(e) => openMenu(e, [copyIdItem(resp.id, copyId)])}
      className={`${RESP_ROW} rounded py-[1.5px] [&:not(:first-child)]:mt-2.5`}
    >
      {kind === "unchanged" ? <span /> : <ChangeGlyph kind={CHANGE_OF[kind]} />}
      <span className="select-none pr-2.5 text-right font-mono text-2xs tabular-nums text-[var(--text-ghost)]">
        {index}
      </span>
      <div className="min-w-0 pr-[180px] font-mono text-sm leading-relaxed">
        <span className={contentColor}>
          {resp.statement ? (
            kind === "reworded" && prev ? (
              <WordDiffText from={prev.statement} to={resp.statement} />
            ) : kind === "added" ? (
              <span className={DIFF_TINT.add}>
                {resp.statement}
              </span>
            ) : deleted ? (
              <span className={DIFF_TINT.delete}>
                {resp.statement}
              </span>
            ) : (
              resp.statement
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
            {tested && (() => {
              const t = verifyLocations[0];
              const where = `${t.pattern}${t.symbol ? ` \`${t.symbol}\`` : ""}`;
              if (verifyState === "broken" || verifyState === "fileMissing") {
                return (
                  <span
                    className={VERIFY_PILLS.gone}
                    title={`The backing test is gone: ${where} no longer resolves. Re-link the claim to a live test, or clear the entry.`}
                  >
                    test gone
                  </span>
                );
              }
              const open = () =>
                void invoke("open_in_editor", {
                  file: t.pattern,
                  line: t.line ?? null,
                  projectPath,
                });
              if (verifyState === "changed") {
                return (
                  <button
                    type="button"
                    onClick={open}
                    className={FLAG_COLORS.stale.pill}
                    title={`The backing test changed since the last reconcile: ${where}. Check it still demonstrates this claim.`}
                  >
                    test changed
                  </button>
                );
              }
              return (
                <button
                  type="button"
                  onClick={open}
                  className={VERIFY_PILLS.tested}
                  title={`Backed by ${where} — click to open`}
                >
                  <FlaskConical className="h-2.5 w-2.5" />
                  tested
                </button>
              );
            })()}
          </span>
        )}

        {/* Verdict actions, inline where the row needs one — controls in their
            own lane, off the mono content. */}
        {resp.stale && editor && (
          <div className="mt-1.5 flex flex-col gap-1.5 text-2xs">
            {/* Reword: drift judged the behaviour DIVERGED, not vanished, and
                proposed wording that matches the code now. The recommended path —
                accept it (or edit first) to fold the new wording in with no
                rebuild, since the code already does it. */}
            {resp.staleProposal &&
              (rewordDraft === null ? (
                <div className="flex flex-wrap items-center gap-2">
                  <span className="text-[var(--text-tertiary)]">Drift proposes:</span>
                  <span className="min-w-0 font-mono text-sm">
                    <WordDiffText from={resp.statement} to={resp.staleProposal} />
                  </span>
                  <button
                    type="button"
                    onClick={() => editor.rewordResponsibility(resp.id, resp.staleProposal!)}
                    className={BTN_GO}
                    title="The code changed what it does — accept this wording into the model. No rebuild: the code already does this."
                  >
                    Accept reword
                  </button>
                  <button
                    type="button"
                    onClick={() => setRewordDraft(resp.staleProposal ?? "")}
                    className={BTN}
                    title="Adjust the wording before accepting"
                  >
                    Edit
                  </button>
                </div>
              ) : (
                <div className="flex flex-wrap items-center gap-2">
                  <Editable
                    initial={rewordDraft}
                    autoFocus
                    placeholder="Verb-led statement of accountability"
                    onInput={setRewordDraft}
                    onEnter={() => {
                      if (rewordDraft.trim()) editor.rewordResponsibility(resp.id, rewordDraft);
                    }}
                    onEscape={() => setRewordDraft(null)}
                    className={`min-w-[12rem] flex-1 ${STMT_HL}`}
                  />
                  <button
                    type="button"
                    disabled={!rewordDraft.trim()}
                    onClick={() => editor.rewordResponsibility(resp.id, rewordDraft)}
                    className={`${BTN_GO} disabled:opacity-40`}
                  >
                    Save
                  </button>
                  <button type="button" onClick={() => setRewordDraft(null)} className={BTN}>
                    Cancel
                  </button>
                </div>
              ))}
            <div className="flex flex-wrap items-center gap-2">
              <span className="text-[var(--text-tertiary)]">
                {resp.staleProposal
                  ? "Or —"
                  : "Drift says the code no longer does this —"}
              </span>
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
          </div>
        )}
        {reviewable && (
          <VerdictBar hint="In the code, not in the model" className="-mr-[180px]">
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
              data-act="reject"
              onClick={() => editor!.rejectResponsibility(resp.id)}
              className={BTN_DANGER}
              title="Mark the code for deletion — this behaviour is not wanted (folds it in, then schedules its removal)"
            >
              Reject
            </button>
          </VerdictBar>
        )}
        {deleted && editor && (
          <VerdictBar hint="Removed from the plan">
            <button type="button" onClick={onRestore} className={BTN_GO} title="Put this committed claim back into the plan">
              Restore
            </button>
          </VerdictBar>
        )}
        {/* Relocated: the claim moved to another host in the plan. It still lives
            there under the same id, so there's nothing to restore — a Restore
            here would re-add the id and duplicate it across two hosts. */}
        {relocated && (
          <div className="mt-1.5 flex flex-wrap items-center gap-2 text-2xs text-[var(--text-tertiary)]">
            <span>Moved to {row.movedTo} in the plan.</span>
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
            {added ? <ChangeGlyph kind="add" /> : <span className="select-none" />}
            <span className="select-none" />
            <div className="flex min-w-0 items-baseline gap-1.5 pr-[180px] font-mono text-sm leading-relaxed text-[var(--text-tertiary)]">
              <CornerDownRight className="h-3 w-3 shrink-0 translate-y-px not-italic text-[var(--text-ghost)]" />
              <span className={`min-w-0 ${added ? DIFF_TINT.add : ""}`}>{d}</span>
            </div>
          </li>
        );
      })}
      {removedDirs.map((d, i) => (
        <li key={`rd${i}`} className={`${RESP_ROW} py-[0.5px]`}>
          <ChangeGlyph kind="delete" />
          <span className="select-none" />
          <div className="flex min-w-0 items-baseline gap-1.5 pr-[180px] font-mono text-sm leading-relaxed text-[var(--text-tertiary)]">
            <CornerDownRight className="h-3 w-3 shrink-0 translate-y-px not-italic text-[var(--text-ghost)]" />
            <span className={`min-w-0 ${DIFF_TINT.delete}`}>{d}</span>
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
    <li data-erow={resp.id} className={`group/erow ${RESP_ROW} py-[1.5px] [&:not(:first-child)]:mt-2.5`}>
      <span className="select-none text-center font-mono text-xs" />
      <span className="select-none pr-2.5 text-right font-mono text-2xs tabular-nums text-[var(--text-ghost)]">
        {index}
      </span>
      <div className="min-w-0 font-mono text-sm leading-relaxed">
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
              data-act="add-directive"
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
                data-drow={resp.id}
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
