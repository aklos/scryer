/**
 * Wiki special pages — the cross-cutting surfaces that aren't model content:
 *
 *  - Changes: the whole plan diff — every way `planned` diverges from the
 *    committed model — on one page, grouped per element with before → after
 *    field diffs. The global form of the tree's Changes lens; ordered by most
 *    recent session edit (timestamps borrowed from the session journal), then
 *    by tree position for anything pending from a prior session.
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

import { useMemo, useState } from "react";
import { Check, GitCompare } from "lucide-react";
import { ConfirmPopover } from "./ConfirmPopover";
import type { ChangeItem, ChangeRevision } from "./hooks/useModelStorage";
import type { ScryModel, Node, Responsibility, SchemaProperty, DriftScope } from "./viewmodel";
import type { Change, ElementChange, ModelDiff } from "./planDiff";
import { CHANGE_COLOR, type ChangeKind, collectPlanEntries, type LinkChange, MARK_META, type PlanEntry } from "./changeMarks";
import { DIFF_TINT, DiffRow } from "./diffkit";
import { entryChanges } from "./ledger";
import type { Editor } from "./editor";
import type { ModelHealthReport } from "./health";
import { ANCHOR_STATE_LABEL, collapseAnchors, darkBoundaries } from "./health";
import { kindIcon } from "./kindIcon";
import { isNodeEmpty } from "./rollup";
import { respElementId, propElementId } from "./SourceSection";
import { BTN, BTN_AGENT, BTN_DANGER, BTN_GO, jumpTo, LINK, PAGE_COL, PageSection, WikiLink, WordDiffText } from "./pagekit";

// --- shared shell -------------------------------------------------------------

function SpecialHeader({ title, subtitle }: { title: string; subtitle: string }) {
  return (
    <header className="shrink-0 border-b border-[var(--border)] pb-3 pt-[18px]">
      <div className={PAGE_COL}>
        <h1 className="text-xl font-semibold leading-tight text-[var(--text)]">{title}</h1>
        <div className="mt-[3px] text-xs text-[var(--text-tertiary)]">{subtitle}</div>
      </div>
    </header>
  );
}

function SpecialBody({ children }: { children: React.ReactNode }) {
  return (
    <div className="min-h-0 flex-1 overflow-y-auto">
      <div className={`${PAGE_COL} pb-[50px] pt-[18px]`}>
        <div className="max-w-[820px]">{children}</div>
      </div>
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
              <span className="shrink-0 text-2xs uppercase tracking-[0.07em] text-[var(--text-ghost)]">
                {it.what}
              </span>
              {/* Removals read as struck-through text (never a live link — the
                  thing is gone); additions/changes link to their node. */}
              {it.op !== "removed" && it.nodeId ? (
                <button
                  type="button"
                  onClick={() => onSelectNode(it.nodeId!)}
                  className={`min-w-0 truncate text-left ${LINK}`}
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

// --- changes (the whole plan diff) -------------------------------------------
//
// The Changes page is the entire plan — every way `planned` diverges from the
// committed model — on ONE page, grouped per element. It's the global form of
// the tree's Changes lens and a node's per-field diff. The diff itself is
// structural and carries no time, so we borrow timestamps from the session
// `changeLog`: anything edited this session floats to the top (newest first),
// and everything else (pending from a prior session) falls below in tree order.

/** One node's (or group's) pending divergence — a shared {@link PlanEntry}
 *  (the same computation the tree's gutter and lens count use, so the surfaces
 *  cannot disagree) decorated with the page's display/sort fields. */
interface DiffEntry extends PlanEntry {
  label: string;
  /** Last session touch, if any — the primary sort key. */
  at: number | null;
  by: "agent" | "user" | null;
  /** DFS tree index — the tie-break / fallback ordering for untouched entries. */
  order: number;
}

type Reworded = Extract<Change, { type: "reworded" }>;
type Moved = Extract<Change, { type: "moved" }>;
type Repointed = Extract<Change, { type: "repointed" }>;

/** id → display name, drawn from both layers so a dropped element (gone from
 *  `planned`) still resolves via the committed copy; planned names win. */
function buildNameMap(model: ScryModel, committed: ScryModel | null): Map<string, string> {
  const m = new Map<string, string>();
  const add = (mod: ScryModel | null) => {
    if (!mod) return;
    for (const n of mod.nodes) m.set(n.id, n.name);
    for (const g of mod.groups) m.set(g.id, g.name);
  };
  add(committed);
  add(model); // planned last so its names override
  return m;
}

const nameOf = (names: Map<string, string>, id: string | null): string =>
  id == null ? "root" : (names.get(id) ?? id);

/** node id → its position in a name-sorted DFS of the node tree — the same
 *  top-down spine the ModelTree shows, used to order untouched entries. */
function treeOrder(model: ScryModel): Map<string, number> {
  const byParent = new Map<string | null, Node[]>();
  for (const n of model.nodes) {
    const k = n.parentId ?? null;
    const arr = byParent.get(k);
    if (arr) arr.push(n);
    else byParent.set(k, [n]);
  }
  for (const arr of byParent.values()) arr.sort((a, b) => a.name.localeCompare(b.name));
  const order = new Map<string, number>();
  let i = 0;
  const walk = (parent: string | null) => {
    for (const n of byParent.get(parent) ?? []) {
      order.set(n.id, i++);
      walk(n.id);
    }
  };
  walk(null);
  return order;
}

/** id → its most recent session edit. `changeLog` is newest-first, so the first
 *  time an id appears is its latest touch; claim/link edits carry their host
 *  node's id, so they roll up to that node's entry. */
function lastTouched(
  changeLog: readonly ChangeRevision[],
): Map<string, { at: number; by: "agent" | "user" }> {
  const m = new Map<string, { at: number; by: "agent" | "user" }>();
  for (const rev of changeLog)
    for (const it of rev.items)
      if (it.nodeId && !m.has(it.nodeId)) m.set(it.nodeId, { at: rev.at, by: rev.by });
  return m;
}

function buildEntries(
  planDiff: ModelDiff,
  model: ScryModel,
  committed: ScryModel | null,
  changeLog: readonly ChangeRevision[],
): DiffEntry[] {
  const order = treeOrder(model);
  const touched = lastTouched(changeLog);
  const names = buildNameMap(model, committed);

  const entries: DiffEntry[] = collectPlanEntries(planDiff, model, committed).map((e) => {
    const t = touched.get(e.id);
    return {
      ...e,
      label: nameOf(names, e.id),
      at: t?.at ?? null,
      by: t?.by ?? null,
      order: order.get(e.id) ?? Number.MAX_SAFE_INTEGER,
    };
  });

  // Newest session edit first; untouched entries (at = null → -1) sink to the
  // bottom, where tree position decides the order.
  entries.sort((a, b) => (b.at ?? -1) - (a.at ?? -1) || a.order - b.order);
  return entries;
}

/** A node/group reference — a live link to it, or muted struck text when it's
 *  gone (a dropped element the plan no longer has). `root` for the tree root. */
function NodeRef({
  id,
  names,
  live,
  onSelectNode,
}: {
  id: string | null;
  names: Map<string, string>;
  live: ReadonlySet<string>;
  onSelectNode: (id: string) => void;
}) {
  if (id == null) return <span className="text-[var(--text-muted)]">root</span>;
  const name = nameOf(names, id);
  if (live.has(id))
    return (
      <button
        type="button"
        onClick={() => onSelectNode(id)}
        className={`rounded text-left ${LINK}`}
      >
        {name}
      </button>
    );
  return (
    <span
      title="no longer in the model"
      className="text-[var(--text-muted)] line-through decoration-[var(--text-ghost)]"
    >
      {name}
    </span>
  );
}

/** Shared props for the detail rows — everything they need to render node
 *  references as live links. */
interface RowCtx {
  names: Map<string, string>;
  live: ReadonlySet<string>;
  onSelectNode: (id: string) => void;
}

/** One of the element's own changes — a field reword, a move, a membership
 *  change. Whole-element add/delete is shown by the entry's mark, not a row. */
function OwnRow({ change, ctx }: { change: Change; ctx: RowCtx }) {
  switch (change.type) {
    case "added":
    case "deleted":
    case "repointed": // links carry these; rendered by LinkRow, never here
      return null;
    case "reworded":
      return (
        <div className="text-2xs leading-relaxed">
          <span className="text-[var(--text-muted)]">{change.field}: </span>
          {change.from ? (
            <WordDiffText from={change.from} to={change.to} />
          ) : (
            <span className={CHANGE_COLOR.add}>{change.to}</span>
          )}
        </div>
      );
    case "moved":
      return (
        <div className="flex flex-wrap items-baseline gap-1 text-2xs text-[var(--text-muted)]">
          moved: <NodeRef id={change.from} {...ctx} />
          <span className="text-[var(--text-ghost)]">→</span>
          <NodeRef id={change.to} {...ctx} />
        </div>
      );
    case "membersChanged":
      return (
        <div className="flex flex-wrap items-baseline gap-1 text-2xs text-[var(--text-muted)]">
          members:
          {change.added.map((m) => (
            <span key={`+${m}`} className={CHANGE_COLOR.add}>
              +<NodeRef id={m} {...ctx} />
            </span>
          ))}
          {change.removed.map((m) => (
            <span key={`-${m}`} className={CHANGE_COLOR.delete}>
              −<NodeRef id={m} {...ctx} />
            </span>
          ))}
        </div>
      );
  }
}

/** One owned responsibility/property change — a +/−/~ glyph and the statement,
 *  word-diffed when reworded, with any secondary field diffs beneath. */
function ChildRow({ ec, ctx }: { ec: ElementChange; ctx: RowCtx }) {
  const added = ec.changes.some((c) => c.type === "added");
  const deleted = ec.changes.some((c) => c.type === "deleted");
  const moved = ec.changes.find((c): c is Moved => c.type === "moved");
  const rewords = ec.changes.filter((c): c is Reworded => c.type === "reworded");
  const statement = rewords.find((r) => r.field === "statement");
  const secondary = rewords.filter((r) => r.field !== "statement");
  const kind: ChangeKind = added ? "add" : deleted ? "delete" : "modified";
  return (
    <DiffRow kind={kind} className="text-xs leading-relaxed">
      <div className="min-w-0 font-mono">
        {deleted ? (
          <span className={DIFF_TINT.delete}>{ec.label}</span>
        ) : statement ? (
          <WordDiffText from={statement.from} to={statement.to} />
        ) : added ? (
          <span className={DIFF_TINT.add}>{ec.label}</span>
        ) : (
          <span className="text-[var(--text-secondary)]">{ec.label}</span>
        )}
        {moved && (
          <span className="ml-1.5 inline-flex flex-wrap items-baseline gap-1 text-[var(--text-muted)]">
            in <NodeRef id={moved.from} {...ctx} />
            <span className="text-[var(--text-ghost)]">→</span>
            <NodeRef id={moved.to} {...ctx} />
          </span>
        )}
        {secondary.map((r) => (
          <div key={r.field} className="mt-px">
            <span className="text-[var(--text-muted)]">{r.field}: </span>
            <WordDiffText from={r.from} to={r.to} />
          </div>
        ))}
      </div>
    </DiffRow>
  );
}

/** One outgoing link change — a +/−/~ glyph, the relationship label, and an
 *  arrow to the (linked) target node, with a repointed before-target noted. */
function LinkRow({ link, ctx }: { link: LinkChange; ctx: RowCtx }) {
  const { ec, dst } = link;
  const added = ec.changes.some((c) => c.type === "added");
  const deleted = ec.changes.some((c) => c.type === "deleted");
  const repointed = ec.changes.find((c): c is Repointed => c.type === "repointed");
  const rewords = ec.changes.filter((c): c is Reworded => c.type === "reworded");
  const label = rewords.find((r) => r.field === "label");
  const method = rewords.find((r) => r.field === "method");
  const kind: ChangeKind = added ? "add" : deleted ? "delete" : "modified";
  return (
    <DiffRow kind={kind} className="text-xs leading-relaxed">
      <div className="min-w-0 font-mono">
        <span className="inline-flex flex-wrap items-baseline gap-1">
          <span className="text-2xs uppercase tracking-[0.07em] text-[var(--text-ghost)]">link</span>
          {deleted ? (
            <span className={DIFF_TINT.delete}>{ec.label}</span>
          ) : label ? (
            <WordDiffText from={label.from} to={label.to} />
          ) : added ? (
            <span className={DIFF_TINT.add}>{ec.label}</span>
          ) : (
            <span className="text-[var(--text-secondary)]">{ec.label}</span>
          )}
          <span className="text-[var(--text-ghost)]">→</span>
          <NodeRef id={dst} {...ctx} />
          {repointed && repointed.dstFrom !== repointed.dstTo && (
            <span className="text-[var(--text-muted)]">
              (was <NodeRef id={repointed.dstFrom} {...ctx} />)
            </span>
          )}
        </span>
        {method && (
          <div className="mt-px">
            <span className="text-[var(--text-muted)]">method: </span>
            <WordDiffText from={method.from} to={method.to} />
          </div>
        )}
      </div>
    </DiffRow>
  );
}

function EntryCard({ entry, ctx }: { entry: DiffEntry; ctx: RowCtx }) {
  const meta = MARK_META[entry.mark];
  const ownRows = entry.own.filter((c) => c.type === "reworded" || c.type === "moved" || c.type === "membersChanged");
  const hasBody = ownRows.length > 0 || entry.children.length > 0 || entry.links.length > 0;
  return (
    <li className="border-b border-[var(--border-subtle)] py-3 last:border-b-0">
      <div className="flex items-baseline gap-2">
        <span
          title={meta.label}
          className={`w-3 shrink-0 text-center font-mono text-xs font-semibold ${meta.color}`}
        >
          {entry.mark}
        </span>
        <span className="shrink-0 text-2xs uppercase tracking-[0.07em] text-[var(--text-ghost)]">
          {entry.kind}
        </span>
        {/* A dropped element has no page — struck text, never a dead link
            (the same convention as NodeRef in the detail rows). */}
        {ctx.live.has(entry.id) ? (
          <button
            type="button"
            onClick={() => ctx.onSelectNode(entry.id)}
            className={`min-w-0 truncate text-left text-sm ${LINK}`}
          >
            {entry.label}
          </button>
        ) : (
          <span className="min-w-0 truncate text-sm text-[var(--text-muted)] line-through decoration-[var(--text-ghost)]">
            {entry.label}
          </span>
        )}
        {entry.at != null && (
          <span className="ml-auto shrink-0 font-mono text-2xs tabular-nums text-[var(--text-muted)]">
            {timeLabel(entry.at)}
            <span
              className={
                entry.by === "agent"
                  ? "ml-1.5 font-sans font-medium text-indigo-600 dark:text-indigo-400"
                  : "ml-1.5 font-sans font-medium text-[var(--text-tertiary)]"
              }
            >
              {entry.by === "agent" ? "agent" : "you"}
            </span>
          </span>
        )}
      </div>
      {hasBody && (
        <div className="mt-1 flex flex-col gap-0.5 pl-5 font-mono">
          {ownRows.map((c, i) => (
            <OwnRow key={`o${i}`} change={c} ctx={ctx} />
          ))}
          {entry.children.map((ec) => (
            <ChildRow key={`${ec.kind}:${ec.id}`} ec={ec} ctx={ctx} />
          ))}
          {entry.links.map((l) => (
            <LinkRow key={`l:${l.ec.id}`} link={l} ctx={ctx} />
          ))}
        </div>
      )}
    </li>
  );
}

export function ChangesPage({
  planDiff,
  model,
  committed,
  changeLog,
  onSelectNode,
  activeChange,
  onSetActiveChange,
  onOpenChange,
}: {
  planDiff: ModelDiff;
  /** The planned model — element names, kinds, and tree order. */
  model: ScryModel;
  /** The committed model — resolves the names of elements the plan drops. */
  committed: ScryModel | null;
  /** Session edit journal — the page's only source of timestamps. */
  changeLog: readonly ChangeRevision[];
  onSelectNode: (id: string) => void;
  /** The ledger change canvas edits currently stamp into (null = unfiled). */
  activeChange?: string | null;
  /** Select/detach the active change. Absent = read-only (agent writing). */
  onSetActiveChange?: (id: string | null) => void;
  /** Open a new named change and make it active. Absent = read-only. */
  onOpenChange?: (rationale: string) => void;
}) {
  const ctx = useMemo<RowCtx>(
    () => ({
      names: buildNameMap(model, committed),
      // Only elements still in the plan are selectable; dropped ones link to nothing.
      live: new Set<string>([...model.nodes.map((n) => n.id), ...model.groups.map((g) => g.id)]),
      onSelectNode,
    }),
    [model, committed, onSelectNode],
  );
  const entries = useMemo(
    () => buildEntries(planDiff, model, committed, changeLog),
    [planDiff, model, committed, changeLog],
  );
  // Ledger partition: each entry lands under every change that tags any of
  // its parts (a carrier straddling two changes is shown in both — honest
  // about the overlap), untagged entries under "Unfiled". With no open
  // changes the page renders the flat list it always did.
  const registry = model.changes ?? [];
  const sections = useMemo(() => {
    const byChange = new Map<string, DiffEntry[]>(registry.map((c) => [c.id, []]));
    const unfiled: DiffEntry[] = [];
    for (const e of entries) {
      const tags = entryChanges(
        e.kind,
        e.id,
        [...e.children, ...e.links.map((l) => l.ec)],
        model.changeMap,
      );
      const known = [...tags].filter((t) => byChange.has(t));
      if (known.length === 0) unfiled.push(e);
      for (const t of known) byChange.get(t)?.push(e);
    }
    return { byChange, unfiled };
  }, [entries, registry, model.changeMap]);

  return (
    <div className="flex min-w-0 flex-1 flex-col">
      <SpecialHeader
        title="Changes"
        subtitle="Everything the plan changes against the committed model — most recently edited first"
      />
      <SpecialBody>
        {onOpenChange && <NewChangeForm onOpen={onOpenChange} />}
        {entries.length === 0 && registry.length === 0 ? (
          <div className="flex flex-col items-center gap-3 px-6 py-16">
            <GitCompare className="h-6 w-6 text-[var(--text-ghost)]" />
            <p className="text-xs text-[var(--text-muted)]">
              The plan matches the committed model — nothing pending.
            </p>
          </div>
        ) : registry.length === 0 ? (
          <ul className="flex flex-col" data-changes-list>
            {entries.map((e) => (
              <EntryCard key={`${e.kind}:${e.id}`} entry={e} ctx={ctx} />
            ))}
          </ul>
        ) : (
          <div className="flex flex-col">
            {registry.map((c) => (
              <ChangeSection
                key={c.id}
                id={c.id}
                rationale={c.rationale}
                entries={sections.byChange.get(c.id) ?? []}
                ctx={ctx}
                active={activeChange === c.id}
                onToggleActive={
                  onSetActiveChange &&
                  (() => onSetActiveChange(activeChange === c.id ? null : c.id))
                }
              />
            ))}
            {sections.unfiled.length > 0 && (
              <ChangeSection
                id={null}
                rationale="Unfiled — pending work belonging to no change"
                entries={sections.unfiled}
                ctx={ctx}
                active={false}
                onToggleActive={undefined}
              />
            )}
          </div>
        )}
      </SpecialBody>
    </div>
  );
}

/** Inline opener for a new ledger change: the rationale in one sentence, as
 *  the dev would say it — it becomes the change's durable identity. */
function NewChangeForm({ onOpen }: { onOpen: (rationale: string) => void }) {
  const [rationale, setRationale] = useState("");
  return (
    <form
      className="flex items-center gap-2 border-b border-[var(--border)] px-4 py-2"
      onSubmit={(e) => {
        e.preventDefault();
        if (!rationale.trim()) return;
        onOpen(rationale);
        setRationale("");
      }}
    >
      <input
        value={rationale}
        onChange={(e) => setRationale(e.target.value)}
        placeholder="Start a change — the task in one sentence"
        className="min-w-0 flex-1 rounded border border-[var(--border)] bg-[var(--surface-field)] px-2 py-1 text-xs text-[var(--text)] placeholder:text-[var(--text-ghost)] focus:outline-none focus:border-[var(--accent)] focus:ring-1 focus:ring-[var(--accent)]"
      />
      <button type="submit" className={BTN} disabled={!rationale.trim()}>
        Open
      </button>
    </form>
  );
}

/** One change's partition of the pending queue: header (id chip, rationale,
 *  count, work-here toggle) + its entry cards. `id` null = the unfiled bucket. */
function ChangeSection({
  id,
  rationale,
  entries,
  ctx,
  active,
  onToggleActive,
}: {
  id: string | null;
  rationale: string;
  entries: DiffEntry[];
  ctx: RowCtx;
  active: boolean;
  onToggleActive?: () => void;
}) {
  return (
    <section>
      <div className="sticky top-0 z-10 flex items-center gap-2 border-b border-[var(--border)] bg-[var(--surface)] px-4 py-1.5">
        {id && (
          <span className="rounded bg-[var(--surface-hover)] px-1.5 py-0.5 font-mono text-2xs text-[var(--text-secondary)]">
            {id}
          </span>
        )}
        <span
          className={`min-w-0 flex-1 truncate text-xs ${id ? "text-[var(--text)]" : "text-[var(--text-muted)]"}`}
          title={rationale}
        >
          {rationale}
        </span>
        <span className="text-2xs text-[var(--text-muted)]">
          {entries.length === 0
            ? "no entries yet"
            : `${entries.length} entr${entries.length === 1 ? "y" : "ies"}`}
        </span>
        {onToggleActive && (
          <button
            type="button"
            className={BTN}
            title={
              active
                ? "Canvas edits are stamping into this change — click to detach (edits go unfiled)"
                : "Stamp subsequent canvas edits into this change"
            }
            onClick={onToggleActive}
          >
            {active ? (
              <span className="flex items-center gap-1 text-[var(--accent)]">
                <Check className="h-3 w-3" /> working here
              </span>
            ) : (
              "work here"
            )}
          </button>
        )}
      </div>
      {entries.length > 0 && (
        <ul className="flex flex-col" data-changes-list>
          {entries.map((e) => (
            <EntryCard key={`${id ?? "unfiled"}:${e.kind}:${e.id}`} entry={e} ctx={ctx} />
          ))}
        </ul>
      )}
    </section>
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
export function findUnmappedClaims(committed: ScryModel | null, planned: ScryModel | null): {
  claims: ClaimRef[];
  shapes: Node[];
} {
  const claims: ClaimRef[] = [];
  const shapes: Node[] = [];
  if (!committed) return { claims, shapes };
  // Leafness spans the AUTHORED tree (committed + plan) — mirrors
  // compute_health: a design-ahead child makes its parent structural, so the
  // parent's claims discharge through the subtree-to-be instead of reading as
  // blind spots.
  const hasChildren = new Set(
    [...committed.nodes, ...(planned?.nodes ?? [])]
      .map((n) => n.parentId)
      .filter(Boolean) as string[],
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
  model,
  report,
  onSelectNode,
}: {
  committed: ScryModel | null;
  /** The planned layer — leafness spans both, like compute_health. */
  model: ScryModel | null;
  report: ModelHealthReport | null;
  onSelectNode: (id: string) => void;
}) {
  const { claims, shapes } = findUnmappedClaims(committed, model);
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
