/**
 * The node page's claims: a node's or group's claims grouped by concern, each
 * a diff row against the committed tree — its marker, statement (word-diffed
 * when reworded), and the test that states it, with its
 * recorded verdict and probe result. Edit mode is a list form, committed on
 * Done, with EARS lint and a live markup preview.
 */

import { useEffect, useState } from "react";
import {
  CheckCheck,
  CircleDashed,
  CornerDownRight,
  Crosshair,
  FlaskConical,
  RotateCw,
  Tag,
  X,
} from "lucide-react";
import type { ConcernDef, ScryModel, Responsibility, SourceLocation } from "../../entities/model/viewmodel";
import { STANDARD_CONCERNS } from "../../entities/model/viewmodel";
import { lookupIcon } from "../../features/search/IconPicker";
import type { Editor } from "../../entities/model/editor";
import type { AnchorState, ClaimProbeStatus, ClaimTestStatus } from "../../entities/model/health";
import { probeTitle, testLaneGlyph, testLaneTitle, testLaneTone } from "../../entities/model/health";
import { FLAG_COLORS } from "../../shared/ui/statusColors";
import {
  buildElementDiff,
  CHANGE_OF,
  ChangeGlyph,
  DIFF_ANCHOR,
  DIFF_TINT,
  type ElementDiff,
  VerdictBar,
  DRIFT_RULE,
  DRIFT_HINT,
} from "../../entities/model/diffkit";
import { ClaimTests, respElementId } from "../source-section/SourceSection";
import { ANCHOR_CALM, earsTestable, hasMarkup, lintEars, MarkupMirror, serializeEars, StatementText, stripMarkup } from "../../entities/model/markup";
import { usePageMenu, useCopyId, copyIdItem } from "../../features/search/pageMenu";
import {
  BTN,
  BTN_DANGER,
  BTN_GO,
  Editable,
  Empty,
  PageSection,
  SectionEditor,
  WordDiffText,
} from "../../shared/ui/pagekit";
import { RESP_ROW, STMT_HL_HOVER } from "./kit";
import { onAddClaimRequest } from "../../features/derived-tree/addClaim";

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
    // Statements compare stripped of display markup: a markup-only touch is
    // presentation, not a reword.
    changed: (prev, r) =>
      stripMarkup(prev.statement) !== stripMarkup(r.statement) ||
      (prev.directives ?? []).join("\n") !== (r.directives ?? []).join("\n"),
    vagrant: (r) => !!r.vagrant,
    relocatedTo: (r) => plannedHosts?.get(r.id),
  }).map(({ item, ...row }) => ({ ...row, resp: item }));
}

/** The typed-slot prefix of a responsibility row — the element that tells you
 *  what a line is BEFORE you read it. A tagged claim leads with its concern's
 *  glyph (registry icon → standard vocabulary → Tag); untagged core flow leads
 *  with a muted dot. Replaces the old ordinal, which renumbered on every
 *  regroup and meant nothing. */
function ConcernGlyph({ slug, concerns }: { slug?: string; concerns: ConcernDef[] }) {
  const entry = slug ? concerns.find((c) => c.slug === slug) : undefined;
  const description = slug
    ? (entry?.description ?? STANDARD_CONCERNS.get(slug)?.description)
    : undefined;
  const title = slug
    ? `concern: ${slug}${description ? ` — ${description}` : ""}`
    : "core domain flow — no concern tag";
  if (!slug) {
    return (
      <span className="inline-flex select-none justify-end pr-1.5" title={title}>
        <CoreFlowDot />
      </span>
    );
  }
  const Icon = lookupIcon(entry?.icon ?? STANDARD_CONCERNS.get(slug)?.icon ?? "Tag") ?? Tag;
  return (
    <span className="inline-flex select-none justify-end pr-1.5 text-[var(--text)]" title={title}>
      {/* absoluteStrokeWidth: at 16px lucide would scale its 2px stroke down
          to a faint 1.33px — absolute keeps a full, pixel-aligned 2px so the
          glyph reads bold and crisp in the lane. */}
      <Icon absoluteStrokeWidth className="h-4 w-4 translate-y-[3px]" />
    </span>
  );
}

/** Untagged core flow's marker: a solid muted dot in the box a concern icon
 *  fills, so the lane reads as one system without looking like an empty slot. */
function CoreFlowDot({ className = "" }: { className?: string }) {
  return (
    <span className={`inline-flex h-4 w-4 translate-y-[3px] items-center justify-center ${className}`} aria-hidden="true">
      <span className="h-1.5 w-1.5 rounded-full bg-[var(--text-tertiary)]" />
    </span>
  );
}

/** Presentation-only anatomy, shared by read and edit: concern groups A→Z
 *  (auth on top, always), then untagged core flow trailing in authored order.
 *  Stored order is never touched — this is how the list READS, the same
 *  arrangement on every page and in the editor. */
function orderByConcern<T>(rows: T[], concernOf: (row: T) => string | undefined): T[] {
  const byConcern = new Map<string, T[]>();
  const flow: T[] = [];
  for (const row of rows) {
    const c = concernOf(row);
    if (c) {
      const arr = byConcern.get(c) ?? [];
      arr.push(row);
      byConcern.set(c, arr);
    } else flow.push(row);
  }
  const slugs = [...byConcern.keys()].sort((a, b) => a.localeCompare(b));
  return [...slugs.flatMap((s) => byConcern.get(s)!), ...flow];
}

function groupByConcern(rows: RespDiffRow[]): RespDiffRow[] {
  return orderByConcern(rows, (r) => r.resp.concern);
}

/** Render text with word-level add/remove highlighting (a reworded claim). When
 *  `from`/`to` are equal it's just the plain text. */
export function ResponsibilitiesSection({
  host,
  hostId,
  resps,
  prevResps,
  plannedHosts,
  concerns,
  testMap,
  notes,
  testStates,
  testVerdicts,
  probeResults,
  projectPath,
  codeBackedHost,
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
  /** The model's concern registry — resolves each claim's `concern` slug to its
   *  glyph/description, and feeds the editor's slug suggestions. */
  concerns: ConcernDef[];
  /** respId → attached-test locations (the test dimension). */
  testMap: Record<string, SourceLocation[]>;
  /** respId → the agent's progress note on a planned claim it left unfolded. */
  notes?: Record<string, string>;
  /** respId → fingerprint state of the attached test, when it regressed. */
  testStates: Record<string, AnchorState>;
  /** respId → recorded test verdict (with re-verified staleness), from the
   *  `get_test_statuses` feed. Colors the test lane. */
  testVerdicts: Record<string, ClaimTestStatus>;
  /** respId → probe result; absent for a claim nobody has probed. */
  probeResults: Record<string, ClaimProbeStatus>;
  projectPath: string | null;
  /** Whether this host's claims are backed by code at all (not a person, not
   *  an external). Structural hosts count — a structural When-claim backs onto
   *  an integration test (rule 22). Gates "untested". */
  codeBackedHost: boolean;
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
  // "Add claim" from the tree opens this editor seeded with a fresh row.
  useEffect(
    () =>
      onAddClaimRequest(hostId, () => {
        if (!editor || editing) return;
        setSeedNewRow(true);
        onToggle();
      }),
    [hostId, editor, editing, onToggle],
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
        <ResponsibilitiesEditor
          host={host}
          hostId={hostId}
          initial={resps}
          concerns={concerns}
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
          {groupByConcern(diffRows).map((row) => (
            <RespDiffRow
              key={row.resp.id}
              row={row}
              host={host}
              concerns={concerns}
              testLocations={testMap[row.resp.id] ?? []}
              note={notes?.[row.resp.id]}
              testState={testStates[row.resp.id] ?? null}
              testVerdict={testVerdicts[row.resp.id] ?? null}
              probeResult={probeResults[row.resp.id] ?? null}
              projectPath={projectPath}
              codeBackedHost={codeBackedHost}
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
  concerns,
  seedNewRow,
  mintId,
  editor,
  onClose,
}: {
  host: "node" | "group";
  hostId: string;
  initial: Responsibility[];
  /** The registry — feeds the concern input's slug suggestions. */
  concerns: ConcernDef[];
  seedNewRow: boolean;
  mintId: (draft: Responsibility[]) => string;
  editor: Editor;
  onClose: () => void;
}) {
  const seededId = seedNewRow ? mintId(initial) : null;
  const start: Responsibility[] = seededId
    ? [...initial, { id: seededId, statement: "" }]
    : initial;

  const commit = (draft: Responsibility[]) => {
    // Blank statements (invalid — the backend flags them) drop out on commit.
    // A blank concern drops to
    // untagged; slug normalization and registry minting happen at the write
    // chokepoint (`registerConcerns`).
    const initialStmt = new Map(initial.map((r) => [r.id, r.statement]));
    const cleaned = draft
      .filter((r) => r.statement.trim() !== "")
      .map((r) => {
        // A statement the user actually touched and left marker-free gets its
        // display markup minted from the positional EARS pass — exactly what
        // the field's mirror previewed. Untouched rows pass through byte-
        // identical (mass-minting would re-date every claim's patina).
        let statement = r.statement.trim();
        if (statement !== (initialStmt.get(r.id) ?? "").trim() && !hasMarkup(statement))
          statement = serializeEars(statement);
        const dirs = (r.directives ?? []).map((s) => s.trim()).filter(Boolean);
        const concern = (r.concern ?? "").trim();
        return {
          ...r,
          statement,
          directives: dirs.length ? dirs : undefined,
          concern: concern || undefined,
        };
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
        const patchRow = (id: string, patch: Partial<Responsibility>) =>
          setDraft((d) => d.map((r) => (r.id === id ? { ...r, ...patch } : r)));
        return draft.length === 0 ? (
          <Empty>No tests state a claim here yet.</Empty>
        ) : (
          <ul className="-mx-2 flex flex-col">
            {orderByConcern(draft, (r) => r.concern).map((r, i) => (
              <ResponsibilityEditRow
                key={r.id}
                resp={r}
                index={i + 1}
                concerns={concerns}
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
  concerns,
  testLocations,
  note,
  testState,
  testVerdict,
  probeResult,
  projectPath,
  codeBackedHost,
  onRestore,
  editor,
}: {
  row: RespDiffRow;
  host: "node" | "group";
  /** The model's concern registry — resolves the row's glyph. */
  concerns: ConcernDef[];
  /** The claim's attached tests (test dimension). */
  testLocations: SourceLocation[];
  /** The agent's progress note: what is built and what is left. */
  note?: string;
  /** Fingerprint state of the attached test, when it regressed since reconcile. */
  testState: AnchorState | null;
  /** The claim's recorded test verdict, or null when no run was ingested. */
  testVerdict: ClaimTestStatus | null;
  /** The claim's recorded probe result, or null when nobody probed it. */
  probeResult: ClaimProbeStatus | null;
  projectPath: string | null;
  /** See {@link ResponsibilitiesSection}: gates "untested", structural hosts included. */
  codeBackedHost: boolean;
  onRestore: () => void;
  editor: Editor | undefined;
}) {
  const { resp, kind, prev } = row;
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
  // A committed When/While/If claim with no attached test — demonstrable, but
  // no test is attached (rule 22). Plan-added and vagrant claims don't
  // nag: the test comes at the build checkpoint, and code-first claims await
  // a verdict. Not gated on leafHost — structural claims back onto
  // integration tests.
  const untested =
    codeBackedHost &&
    testLocations.length === 0 &&
    (kind === "unchanged" || kind === "reworded") &&
    earsTestable(resp.statement);
  // The glyph's tested backing: a live claim with a test attached. Added claims
  // never render their test dimension (see the bleed-spec comment below), and a
  // deleted/relocated claim is leaving — backing its glyph would dress the
  // departure as coverage.
  const tested = kind !== "added" && !deleted && !relocated && testLocations.length > 0;
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
      <ConcernGlyph slug={resp.concern} concerns={concerns} />
      <div className="min-w-0 pr-[180px] font-mono text-sm leading-relaxed">
        <span className={contentColor}>
          {resp.statement ? (
            kind === "reworded" && prev ? (
              <WordDiffText from={stripMarkup(prev.statement)} to={stripMarkup(resp.statement)} />
            ) : kind === "added" ? (
              <span className={DIFF_TINT.add}>
                <StatementText text={resp.statement} anchor={DIFF_ANCHOR.add} />
              </span>
            ) : deleted ? (
              <span className={DIFF_TINT.delete}>
                <StatementText text={resp.statement} anchor={DIFF_ANCHOR.delete} />
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
        {/* The chip only earns its place when the verdict block below can't
            render (read-only) — beside it, it would announce the same fact twice. */}
        {resp.stale && !editor && (
          <span
            className={`${FLAG_COLORS.stale.pill} ml-2 align-middle`}
            title="Drift check: the code no longer discharges this claim as written."
          >
            stale
          </span>
        )}
        {/* Bleed spec: undo the row's 18+22px gutter columns and the 180px
            control lane, so the open peek spans the article column.
            A plan-ADDED claim shows neither dimension: anchoring is the build
            checkpoint (rule 25), so until the claim folds, any recorded anchor
            or test entry is a promise, not a fact — rendering it would dress
            unbuilt work as built. The entries surface when the fold commits. */}
        {kind !== "added" && (
          // The anchor/test footnotes hang in their OWN register — indented a
          // clear step right of the claim's text edge, so the statement reads
          // alone and the refs scan as annotation under it, not more sentence.
          // The bleed grows by the same 20px so an open peek still spans the
          // article column.
          <div className="pl-5">
            {note && (
              <p className="my-1 text-sm text-[var(--text-muted)]" title="The agent's progress note on this unfinished claim">
                <span className="font-medium">Unfinished:</span> {note}
              </p>
            )}
            {/* The claim's anchor is its test: flask-marked, peek and open
                included. */}
            <ClaimTests
              locations={testLocations}
              state={testState}
              projectPath={projectPath}
              deleted={deleted || relocated}
              bleed="-ml-[60px] -mr-[180px]"
            />
          </div>
        )}

        {/* Verdict actions, inline where the row needs one — controls in their
            own lane, off the mono content. A stale claim is a two-way verdict:
            the model adheres to the code, or the code adheres to the model.
            Which pair shows depends on whether the behaviour diverged (drift
            wrote a reword proposal) or vanished (no proposal). Green is
            reserved for the no-cost accept; rebuilding is neutral (it files
            work); drop is red (it deletes from the model). */}
        {resp.stale && editor && (
          <div className={`mt-1.5 flex flex-wrap items-center gap-2 text-xs ${DRIFT_RULE}`}>
            {resp.staleProposal ? (
              <>
                <span className={DRIFT_HINT}>Drift proposes:</span>
                <span className="min-w-0 font-mono text-sm text-[var(--text-secondary)]">
                  <WordDiffText from={stripMarkup(resp.statement)} to={stripMarkup(resp.staleProposal)} />
                </span>
                <button
                  type="button"
                  onClick={() => editor.rewordResponsibility(resp.id, resp.staleProposal!)}
                  className={BTN_GO}
                  title="The code changed what it does — accept this wording into the model. No rebuild: the code already does this."
                >
                  Accept reword
                </button>
                <span className="text-[var(--text-tertiary)]">Or the model is right —</span>
                <button
                  type="button"
                  onClick={() => editor.reimplementResponsibility(resp.id)}
                  className={BTN}
                  title="Keep this claim as written and rebuild the behaviour in code — files a to-do the agent implements (folds back when done)."
                >
                  Rebuild code
                </button>
              </>
            ) : (
              <>
                <span className={DRIFT_HINT}>
                  Drift says the code no longer does this —
                </span>
                <button
                  type="button"
                  onClick={() => editor.dropResponsibility(resp.id)}
                  className={BTN_DANGER}
                  title="The behaviour was removed on purpose — drop the claim from the model."
                >
                  Drop claim
                </button>
                <button
                  type="button"
                  onClick={() => editor.reimplementResponsibility(resp.id)}
                  className={BTN}
                  title="Keep this claim and rebuild the behaviour in code — files a to-do the agent implements (folds back when done)."
                >
                  Rebuild code
                </button>
              </>
            )}
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
          <div className="mt-1.5 flex flex-wrap items-center gap-2 text-xs text-[var(--text-tertiary)]">
            <span>Moved to {row.movedTo} in the plan.</span>
          </div>
        )}
      </div>

      {/* The test lane — the 180px control gutter is empty in read mode, so
          its right edge is an aligned test column, scannable down the page.
          Two marks, two facts, never conflated: the flask + count means
          exactly one thing (tests are attached), and the verdict rides
          BESIDE it as its own glyph — a green check when the last run passed
          and still holds, an amber ↻ when the code moved past the verdict
          (re-run to refresh), a red ✗ when a current verdict is failing or
          errored, and nothing at all when no run was recorded. Green earns a
          mark of its own precisely because "passing" and "never run" are
          different states and must not read alike. The flask keeps reading
          weight while the verdict is fine and takes the tone only when
          something is wrong, so a page of green claims stays calm. The
          rule-22 nudge is deliberately NOT a flask — a dashed circle, the
          page's "expected but absent" mark — so fill-state never carries
          meaning again. Edit mode swaps the whole section for the editor, so
          the lane never collides with the hover controls. */}
      {tested &&
        (() => {
          const tone = testLaneTone(testVerdict ?? undefined);
          const glyph = testLaneGlyph(testVerdict ?? undefined, probeResult ?? undefined);
          const toneCls =
            tone === "failing"
              ? "text-red-600 dark:text-red-400"
              : tone === "stale"
                ? "text-orange-600 dark:text-orange-400"
                : "text-[var(--text-secondary)]";
          // At most one glyph after the count, and only for something worth a
          // look. Passing is assumed — nearly every verdict on screen is
          // green, so any mark for it would be wallpaper — and the flask
          // stays neutral; the tooltip is where "passing" vs "never run" is
          // spelled out. Marks are for what is WRONG: stale, failing, or a
          // probe survivor (red crosshair — a test the verdict calls green did
          // NOT catch a deliberate break). The one positive mark is the green
          // double check: probed, every deliberate break caught — verified,
          // as far as a sample can verify. Nothing for a claim never probed
          // or whose probe went stale: absence is "nobody measured this".
          const Glyph =
            glyph === "probed"
              ? CheckCheck
              : glyph === "hollow"
                ? Crosshair
                : glyph === "stale"
                  ? RotateCw
                  : glyph === "failing"
                    ? X
                    : null;
          const glyphCls =
            glyph === "probed"
              ? "text-emerald-600 dark:text-emerald-400"
              : glyph === "hollow"
                ? "text-red-600 dark:text-red-400"
                : "";
          const glyphLabel =
            glyph === "stale"
              ? "Unverified — re-run"
              : glyph === "failing"
                ? "Tests failing"
                : glyph === "probed"
                  ? "Probed — every deliberate break was caught"
                  : "A deliberate break went uncaught — this test does not hold the claim";
          return (
            <span
              className={`absolute right-1 top-[3px] flex items-center gap-px font-mono text-2xs ${toneCls}`}
              title={
                testLaneTitle(testLocations.length, testVerdict ?? undefined) +
                probeTitle(probeResult ?? undefined)
              }
            >
              <FlaskConical className="h-3 w-3" aria-label="Tests attached" />
              {testLocations.length}
              {Glyph && <Glyph className={`ml-0.5 h-3 w-3 ${glyphCls}`} aria-label={glyphLabel} />}
            </span>
          );
        })()}
      {untested && (
        <span
          className="absolute right-1 top-[3px] flex items-center text-[var(--text-ghost)]"
          title="This claim names a trigger, state, or failure a test could exercise — but no test is attached (rule 22)."
        >
          <CircleDashed className="h-3 w-3" aria-label="No test attached" />
        </span>
      )}
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
  concerns,
  autoFocus,
  onPatch,
}: {
  resp: Responsibility;
  index: number;
  /** The concern registry — feeds the in-place concern picker. */
  concerns: ConcernDef[];
  autoFocus: boolean;
  onPatch: (id: string, patch: Partial<Responsibility>) => void;
}) {
  const lints = lintEars(stripMarkup(resp.statement));

  // In-place edit row: each line is a contentEditable span flowing in the same
  // content cell as the read diff row, with the SAME font/size/line-height, so
  // read↔edit stays the same width. The concern glyph sits in the read view's
  // marker lane — clickable here to tag/untag in place. The statement and each
  // directive are their own hover-scoped line (`/srow`, `/drow`): the full cell
  // highlights on hover and its controls (CTL) float over the right edge.
  return (
    <li data-erow={resp.id} className={`group/erow ${RESP_ROW} py-[1.5px] [&:not(:first-child)]:mt-2.5`}>
      <span className="select-none text-center font-mono text-2xs tabular-nums text-[var(--text-ghost)]">
        {index}
      </span>
      {/* The concern is the test's trailing [tag]: shown, kept on reword,
          changed in the code. */}
      <ConcernGlyph slug={resp.concern} concerns={concerns} />
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
              it, tagging it or adding a directive happens in the code. */}
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
