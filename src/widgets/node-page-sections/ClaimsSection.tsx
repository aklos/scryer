/**
 * The node page's claims: a node's claims grouped by concern, each
 * a diff row against the base the checkout's changes are read from — a
 * + or ~ mark when it was added or reworded since, and with the changes
 * shown, the reword word-diffed and the claims removed since listed struck
 * through — its statement, and the test that states it, with its
 * recorded verdict and probe result. Edit mode is a list form, committed on
 * Done, with EARS lint and a live markup preview.
 */

import { useEffect, useState } from "react";
import {
  Check,
  CheckCheck,
  Circle,
  CircleDashed,
  Crosshair,
  type LucideIcon,
  Tag,
  Triangle,
  X,
} from "lucide-react";
import type { Claim, SourceLocation } from "../../entities/model/viewmodel";
import { STANDARD_CONCERNS } from "../../entities/model/viewmodel";
import { lookupIcon } from "../../features/search/IconPicker";
import type { Editor } from "../../entities/model/editor";
import type { AnchorState, ClaimProbeStatus, ClaimTestStatus } from "../../entities/model/health";
import { claimMark, claimMarkLabel, probeTitle, type ClaimMark } from "../../entities/model/health";
import { buildElementDiff, CHANGE_OF, ChangeGlyph, type ElementDiff } from "../../entities/model/diffkit";
import { ClaimTests, respElementId } from "../source-section/SourceSection";
import { ANCHOR_CALM, hasMarkup, lintEars, MarkupMirror, serializeEars, StatementText, stripMarkup } from "../../entities/model/markup";
import { usePageMenu, useCopyId, copyIdItem } from "../../features/search/pageMenu";
import { BTN, Editable, Empty, PageSection, SectionEditor, WordDiffText } from "../../shared/ui/pagekit";
import { RESP_ROW, STMT_HL_HOVER } from "./kit";
import { DecisionText } from "./DecisionsSection";
import { onAddClaimRequest } from "../../features/derived-tree/addClaim";
import type { DerivedClaimStatus } from "../../entities/model/derived";
import { CLAIM_TONE } from "../../entities/model/claimState";

// --- claims (the diff view) ---------------------------------------

/** How one claim diverges from the base: `added`, `reworded`, `deleted`, or
 *  `unchanged`. */
interface RespDiffRow extends Omit<ElementDiff<Claim>, "item"> {
  resp: Claim;
}

/** Build the diff rows for a node's claims: its claims in order (each
 *  tagged added / reworded / unchanged against its base copy), then, with
 *  the changes shown, the base's claims since removed. `base` is aligned
 *  (see `alignClaims`), so a reworded test keeps its pair. */
export function buildRespDiff(claims: Claim[], base: Claim[], showChanges = true): RespDiffRow[] {
  return buildElementDiff(claims, base, {
    key: (r) => r.id,
    // Statements compare stripped of display markup: a markup-only touch is
    // presentation, not a reword.
    changed: (prev, r) => stripMarkup(prev.statement) !== stripMarkup(r.statement),
  })
    .filter((row) => showChanges || row.kind !== "deleted")
    .map(({ item, ...row }) => ({ ...row, resp: item }));
}

const MARK_ICON: Record<ClaimMark, LucideIcon> = {
  proven: Check,
  probed: CheckCheck,
  hollow: Crosshair,
  failing: X,
  changed: Triangle,
  unrun: Circle,
  todo: CircleDashed,
};

const MARK_TONE: Record<ClaimMark, string> = {
  proven: CLAIM_TONE.proven,
  probed: CLAIM_TONE.proven,
  hollow: CLAIM_TONE.failing,
  failing: CLAIM_TONE.failing,
  changed: CLAIM_TONE.stale,
  unrun: CLAIM_TONE.unproven,
  todo: CLAIM_TONE.planned,
};

/** The mark in front of a claim: what its evidence says, before you read
 *  it, in the one hue its state has on every surface. The only colour on the
 *  row. Without a mark (no health report yet) it holds the column in a
 *  ghost ring. Sits centred on the statement's first line. */
export function ClaimMarker({ mark, label, detail = "" }: { mark?: ClaimMark; label?: string; detail?: string }) {
  const Icon = mark ? MARK_ICON[mark] : Circle;
  const said = label ?? (mark ? mark : "no verdict yet");
  return (
    // One first-line box (text-sm, leading-relaxed): the marker centres on
    // the statement's first line whatever wraps below it.
    <span className="flex h-[22.75px] select-none items-center justify-end self-start pr-1.5" title={said + detail} aria-label={said}>
      {/* absoluteStrokeWidth: at 14px lucide would thin its 2px stroke;
          absolute keeps it full and pixel-aligned. */}
      <Icon absoluteStrokeWidth className={`h-3.5 w-3.5 ${mark ? MARK_TONE[mark] : "text-[var(--text-ghost)]"}`} />
    </span>
  );
}

/** A tagged claim's concerns, in the right-hand lane: each one's glyph
 *  (standard vocabulary → Tag) and name, muted, in tag order, in one column
 *  down the page so a concern reads vertically. An untagged claim leaves the
 *  lane empty. */
export function ConcernTag({ slugs }: { slugs?: string[] }) {
  if (!slugs?.length) return null;
  return (
    <span className="absolute right-1 top-[3px] flex max-w-[170px] select-none items-center gap-2 text-2xs text-[var(--text-tertiary)]">
      {slugs.map((slug) => {
        const standard = STANDARD_CONCERNS.get(slug);
        const Icon = lookupIcon(standard?.icon ?? "Tag") ?? Tag;
        return (
          <span
            key={slug}
            className="flex min-w-0 items-center gap-1"
            title={standard ? `${slug} — ${standard.description}` : slug}
          >
            <Icon className="h-3 w-3 shrink-0" />
            <span className="truncate">{slug}</span>
          </span>
        );
      })}
    </span>
  );
}

/** Presentation-only anatomy, shared by read and edit: concern groups A→Z
 *  (auth on top, always), then untagged core flow trailing in authored order.
 *  Stored order is never touched — this is how the list READS, the same
 *  arrangement on every page and in the editor. With `repeat`, a claim
 *  tagged with several concerns sits in each of their groups, so every
 *  concern's group is whole; without it (the editor, one input per claim),
 *  only in its first tag's. Each entry names the group it sits in. */
export function orderByConcern<T>(
  rows: T[],
  concernsOf: (row: T) => string[] | undefined,
  repeat: boolean,
): { row: T; group?: string }[] {
  const byConcern = new Map<string, T[]>();
  const flow: T[] = [];
  for (const row of rows) {
    const tags = concernsOf(row) ?? [];
    if (tags.length === 0) flow.push(row);
    for (const c of repeat ? tags : tags.slice(0, 1)) {
      const arr = byConcern.get(c) ?? [];
      arr.push(row);
      byConcern.set(c, arr);
    }
  }
  const slugs = [...byConcern.keys()].sort((a, b) => a.localeCompare(b));
  return [
    ...slugs.flatMap((group) => byConcern.get(group)!.map((row) => ({ row, group }))),
    ...flow.map((row) => ({ row })),
  ];
}

export function ClaimsSection({
  nodeId,
  resps,
  prevResps,
  showChanges = false,
  testMap,
  testStates,
  claimStatuses,
  testVerdicts,
  probeResults,
  projectPath,
  mintId,
  editor,
  editing,
  onToggle,
}: {
  nodeId: string;
  resps: Claim[];
  /** This node's claims at the base, aligned to their current ids — the
   *  diff base for the rows. */
  prevResps: Claim[];
  /** Word-diff reworded claims and list removed ones; off, a changed row
   *  only carries its mark. */
  showChanges?: boolean;
  /** respId → attached-test locations (the test dimension). */
  testMap: Record<string, SourceLocation[]>;
  /** respId → fingerprint state of the attached test, when it regressed. */
  testStates: Record<string, AnchorState>;
  /** respId → the claim's derived status: its row's marker (proven,
   *  outdated, failing, todo), and the wording a reworded one passed as. */
  claimStatuses?: Record<string, DerivedClaimStatus>;
  /** respId → recorded test verdict (with re-verified staleness), from the
   *  `get_test_statuses` feed. Colors the test lane. */
  testVerdicts: Record<string, ClaimTestStatus>;
  /** respId → probe result; absent for a claim nobody has probed. */
  probeResults: Record<string, ClaimProbeStatus>;
  projectPath: string | null;
  /** Mint a fresh claim id clear of every node in both layers (plus the
   *  editor's own draft rows). */
  mintId: (draft: Claim[]) => string;
  editor: Editor | undefined;
  editing: boolean;
  onToggle: () => void;
}) {
  const diffRows = buildRespDiff(resps, prevResps, showChanges);
  // "Add claim" from read mode opens the editor seeded with a fresh
  // row; nothing is written to the model until Done.
  const [seedNewRow, setSeedNewRow] = useState(false);
  // "Add claim" from the tree opens this editor seeded with a fresh row.
  useEffect(
    () =>
      onAddClaimRequest(nodeId, () => {
        if (!editor || editing) return;
        setSeedNewRow(true);
        onToggle();
      }),
    [nodeId, editor, editing, onToggle],
  );

  return (
    <PageSection
      title="Claims"
      count={resps.length}
      editable={!!editor}
      editing={editing}
      onToggleEdit={() => {
        setSeedNewRow(false);
        onToggle();
      }}
    >
      {editing && editor ? (
        <ClaimsEditor
          nodeId={nodeId}
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
        <Empty>No tests state a claim here yet.</Empty>
      ) : (
        <ol className="-mx-2 flex flex-col">
          {orderByConcern(diffRows, (r) => r.resp.concerns, true).map(({ row, group }) => (
            <RespDiffRow
              key={`${group ?? ""}:${row.resp.id}`}
              row={row}
              showChanges={showChanges}
              testLocations={testMap[row.resp.id] ?? []}
              testState={testStates[row.resp.id] ?? null}
              claimStatus={claimStatuses?.[row.resp.id]}
              testVerdict={testVerdicts[row.resp.id] ?? null}
              probeResult={probeResults[row.resp.id] ?? null}
              projectPath={projectPath}
            />
          ))}
        </ol>
      )}
    </PageSection>
  );
}

/**
 * The claim-list form inside the shared {@link SectionEditor} shell.
 * Done writes the final list in one update. Untouched new rows vanish on
 * commit.
 */
function ClaimsEditor({
  nodeId,
  initial,
  seedNewRow,
  mintId,
  editor,
  onClose,
}: {
  nodeId: string;
  initial: Claim[];
  seedNewRow: boolean;
  mintId: (draft: Claim[]) => string;
  editor: Editor;
  onClose: () => void;
}) {
  const seededId = seedNewRow ? mintId(initial) : null;
  const start: Claim[] = seededId
    ? [...initial, { id: seededId, statement: "" }]
    : initial;

  const commit = (draft: Claim[]) => {
    // Blank statements drop out on commit, and blank concerns to untagged.
    const initialStmt = new Map(initial.map((r) => [r.id, r.statement]));
    const cleaned = draft
      .filter((r) => r.statement.trim() !== "")
      .map((r) => {
        // A statement the user actually touched and left marker-free gets its
        // display markup minted from the positional EARS pass — exactly what
        // the field's mirror previewed. Untouched rows pass through byte-
        // identical.
        let statement = r.statement.trim();
        if (statement !== (initialStmt.get(r.id) ?? "").trim() && !hasMarkup(statement))
          statement = serializeEars(statement);
        const concerns = (r.concerns ?? []).map((c) => c.trim()).filter(Boolean);
        return {
          ...r,
          statement,
          concerns: concerns.length > 0 ? concerns : undefined,
        };
      });
    editor.updateNode(nodeId, { claims: cleaned });
  };

  return (
    <SectionEditor<Claim[]>
      initial={start}
      onCommit={commit}
      onClose={onClose}
      footerExtra={(setDraft) => (
        <>
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
            Add claim
          </button>
          {/* The EARS forms (rule 21), one glance's worth: condition first,
              response last; no keyword means always-active, verb first. */}
          <span className="ml-auto select-none font-mono text-2xs text-[var(--text-ghost)]">
            <b className="font-semibold">When</b> trigger, … · <b className="font-semibold">While</b>{" "}
            state, … · <b className="font-semibold">If</b> failure,{" "}
            <b className="font-semibold">then</b> … · or verb-first
          </span>
        </>
      )}
    >
      {(draft, setDraft) => {
        const patchRow = (id: string, patch: Partial<Claim>) =>
          setDraft((d) => d.map((r) => (r.id === id ? { ...r, ...patch } : r)));
        return draft.length === 0 ? (
          <Empty>No tests state a claim here yet.</Empty>
        ) : (
          <ul className="-mx-2 flex flex-col">
            {orderByConcern(draft, (r) => r.concerns, false).map(({ row: r }, i) => (
              <ClaimEditRow
                key={r.id}
                resp={r}
                index={i + 1}
               
                // autoFocus fires at mount only — it lands on the seeded row
                // and on rows appended via "Add claim".
                autoFocus={r.statement === "" && r.id === draft[draft.length - 1].id}
                onPatch={patchRow}
              />
            ))}
          </ul>
        );
      }}
    </SectionEditor>
  );
}

/**
 * One claim, rendered as a diff row against the base: a marker (+ added,
 * ~ reworded, − removed), its state marker, the statement (word-diffed when
 * reworded and the changes are shown), its decisions, and the test that
 * states it.
 */
function RespDiffRow({
  row,
  showChanges,
  testLocations,
  testState,
  claimStatus,
  testVerdict,
  probeResult,
  projectPath,
}: {
  row: RespDiffRow;
  showChanges: boolean;
  /** The claim's attached tests (test dimension). */
  testLocations: SourceLocation[];
  /** Fingerprint state of the attached test, when it regressed since reconcile. */
  testState: AnchorState | null;
  /** The claim's derived status, when the health report has one. */
  claimStatus?: DerivedClaimStatus;
  /** The claim's recorded test verdict, or null when no run was ingested. */
  testVerdict: ClaimTestStatus | null;
  /** The claim's recorded probe result, or null when nobody probed it. */
  probeResult: ClaimProbeStatus | null;
  projectPath: string | null;
}) {
  const { resp, kind, prev } = row;
  const openMenu = usePageMenu();
  const copyId = useCopyId();
  const deleted = kind === "deleted";
  // What a reworded claim last passed as: its statement reads as a diff
  // from that wording until it passes again.
  const provenAs = claimStatus?.provenAs;
  const mark = claimStatus && claimMark(claimStatus.state, testVerdict ?? undefined, probeResult ?? undefined, !!provenAs);
  const contentColor = deleted
    ? "text-[var(--text-muted)]"
    : kind === "unchanged"
      ? "text-[var(--text-secondary)]"
      : "text-[var(--text)]";

  return (
    <li
      id={respElementId(resp.id)}
      onContextMenu={(e) => openMenu(e, [copyIdItem(resp.id, copyId)])}
      className={`${RESP_ROW} rounded py-[1.5px] [&:not(:first-child)]:mt-2.5`}
    >
      {/* The marker carries the claim's state; a reworded claim's words
          diff in red and green beside it. */}
      {kind === "unchanged" ? <span /> : <ChangeGlyph kind={CHANGE_OF[kind]} />}
      <ClaimMarker
        mark={mark}
        label={mark && claimMarkLabel(mark, testVerdict ?? undefined, !!provenAs)}
        detail={probeTitle(probeResult ?? undefined)}
      />
      <div className="min-w-0 pr-[180px] font-mono text-sm leading-relaxed">
        <span className={contentColor}>
          {resp.statement ? (
            provenAs ? (
              <WordDiffText from={stripMarkup(provenAs)} to={stripMarkup(resp.statement)} />
            ) : kind === "reworded" && prev && showChanges ? (
              <WordDiffText from={stripMarkup(prev.statement)} to={stripMarkup(resp.statement)} />
            ) : kind === "added" ? (
              <StatementText text={resp.statement} />
            ) : deleted ? (
              <span className="line-through decoration-[var(--text-ghost)]">
                <StatementText text={resp.statement} />
              </span>
            ) : (
              <StatementText
                text={resp.statement}
                anchor={kind === "unchanged" ? ANCHOR_CALM : undefined}
              />
            )
          ) : (
            <span className="italic text-[var(--text-ghost)]">Untitled claim</span>
          )}
        </span>
        {/* The decisions and test hang in their OWN register — indented a
            clear step right of the claim's text edge, so the statement reads
            alone and the refs scan as annotation under it. The bleed undoes
            the row's gutters, 180px control lane and this 20px indent, so an
            open peek spans the article column. A todo claim shows them too:
            its todo test is real, and so is its why. */}
        <div className="pl-5">
          {resp.decisions && (
            <div className="my-1 font-sans" title="Written in the claim's test doc, after the claim">
              <DecisionText text={resp.decisions} />
            </div>
          )}
          {/* The claim's anchor is its test: flask-marked, peek and open
              included. */}
          <ClaimTests
            locations={testLocations}
            state={testState}
            projectPath={projectPath}
            deleted={deleted}
            bleed="-ml-[60px] -mr-[180px]"
          />
        </div>
      </div>

      {/* The concern lane: the 180px control gutter is empty in read mode,
          so its right edge is an aligned concern column. */}
      <ConcernTag slugs={resp.concerns} />
    </li>
  );
}

/** One draft row of the claims form — fully controlled; every
 *  change lands in the section draft, never directly in the model. */
function ClaimEditRow({
  resp,
  index,
  autoFocus,
  onPatch,
}: {
  resp: Claim;
  index: number;
  autoFocus: boolean;
  onPatch: (id: string, patch: Partial<Claim>) => void;
}) {
  const lints = lintEars(stripMarkup(resp.statement));

  // In-place edit row: each line is a contentEditable span flowing in the same
  // content cell as the read diff row, with the SAME font/size/line-height, so
  // read↔edit stays the same width. The concern glyph sits in the read view's
  // marker lane — clickable here to tag/untag in place. The statement is its
  // own hover-scoped line (`/srow`): the full cell highlights on hover and its controls (CTL) float over the right edge.
  return (
    <li data-erow={resp.id} className={`group/erow ${RESP_ROW} py-[1.5px] [&:not(:first-child)]:mt-2.5`}>
      <span className="select-none text-center font-mono text-2xs tabular-nums text-[var(--text-ghost)]">
        {index}
      </span>
      <span />
      <div className="min-w-0 font-mono text-sm leading-relaxed">
        <div className="group/srow relative">
          {/* Markered source with the styling visible in place: the mirror
              previews authored markup (markers ghosted) or, on marker-free
              text, the positional EARS pass that commit will mint — WYSIWYG
              either way. Cmd/Ctrl+B / I wrap the selection in markers. */}
          <Editable
            initial={resp.statement}
            autoFocus={autoFocus}
            placeholder="Verb-led statement of accountability"
            onInput={(t) => onPatch(resp.id, { statement: t })}
            mirror={(t) => <MarkupMirror text={t} />}
            containerClassName={STMT_HL_HOVER}
            className="block !pr-[180px]"
          />
          {/* Only the wording edits here: a claim is a test, so deleting
              it, tagging it or writing its decisions happens in the code. The
              concerns are the test's trailing [tags]: shown, kept on reword. */}
          <ConcernTag slugs={resp.concerns} />
        </div>

        {lints.length > 0 && (
          <div className="mt-0.5 flex flex-col gap-px text-xs leading-snug text-[var(--text-tertiary)]">
            {lints.map((l) => (
              <div key={l.code}>
                <span className="text-[var(--text-ghost)]">“{l.excerpt}”</span> {l.message}
              </div>
            ))}
          </div>
        )}

      </div>
    </li>
  );
}
