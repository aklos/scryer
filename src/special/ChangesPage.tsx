import { useMemo } from "react";
import { Check, GitCompare, X } from "lucide-react";
import type { ChangeRevision } from "../hooks/useModelStorage";
import type { ScryModel, Node } from "../viewmodel";
import type { Change, ElementChange, ModelDiff } from "../planDiff";
import { CHANGE_COLOR, type ChangeKind, collectPlanEntries, type LinkChange, MARK_META, type PlanEntry } from "../changeMarks";
import { DIFF_ANCHOR, DIFF_TINT, DiffRow } from "../diffkit";
import { ANCHOR_CALM, StatementText } from "../markup";
import { entryChanges } from "../ledger";
import { BTN, BTN_ICON, LINK, WordDiffText } from "../pagekit";
import { SpecialBody, SpecialHeader, timeLabel } from "./shell";

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
  /** Raw (marked) statements by responsibility id — the planDiff label is
   *  stripped for prose surfaces, so claim rows resolve their display markup
   *  through here instead. */
  statements: Map<string, string>;
  onSelectNode: (id: string) => void;
}

/** Every responsibility's raw statement, planned model first and committed
 *  filling in what the plan drops (a deleted claim's markup still renders). */
function buildStatementMap(model: ScryModel, committed: ScryModel | null): Map<string, string> {
  const map = new Map<string, string>();
  for (const m of [model, ...(committed ? [committed] : [])])
    for (const holder of [...m.nodes, ...m.groups])
      for (const r of holder.responsibilities ?? [])
        if (!map.has(r.id)) map.set(r.id, r.statement);
  return map;
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
  const text = (ec.kind === "responsibility" && ctx.statements.get(ec.id)) || ec.label;
  return (
    <DiffRow kind={kind} className="text-xs leading-relaxed">
      <div className="min-w-0 font-mono">
        {deleted ? (
          <span className={DIFF_TINT.delete}>
            <StatementText text={text} anchor={DIFF_ANCHOR.delete} />
          </span>
        ) : statement ? (
          <WordDiffText from={statement.from} to={statement.to} />
        ) : added ? (
          <span className={DIFF_TINT.add}>
            <StatementText text={text} anchor={DIFF_ANCHOR.add} />
          </span>
        ) : (
          <span className="text-[var(--text-secondary)]">
            <StatementText text={text} anchor={ANCHOR_CALM} />
          </span>
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
  onCloseChange,
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
  /** Close an EMPTY (stranded) change. Absent = read-only (agent writing). */
  onCloseChange?: (id: string) => void;
}) {
  const ctx = useMemo<RowCtx>(
    () => ({
      names: buildNameMap(model, committed),
      // Only elements still in the plan are selectable; dropped ones link to nothing.
      live: new Set<string>([...model.nodes.map((n) => n.id), ...model.groups.map((g) => g.id)]),
      statements: buildStatementMap(model, committed),
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
                onClose={onCloseChange && (() => onCloseChange(c.id))}
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

/** One change's partition of the pending queue: header (id chip, rationale,
 *  count, work-here toggle) + its entry cards. `id` null = the unfiled bucket.
 *  An EMPTY change also offers a ✕ — the hand-close for a stranded ledger
 *  (opened, but its work ended up filed elsewhere), which otherwise nothing
 *  ever closes. The backend refuses once entries exist, so the ✕ only shows
 *  while there is nothing to lose; the rationale survives in history. */
function ChangeSection({
  id,
  rationale,
  entries,
  ctx,
  active,
  onToggleActive,
  onClose,
}: {
  id: string | null;
  rationale: string;
  entries: DiffEntry[];
  ctx: RowCtx;
  active: boolean;
  onToggleActive?: () => void;
  onClose?: () => void;
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
        {onClose && entries.length === 0 && (
          <button
            type="button"
            className={BTN_ICON}
            title="Close this empty change — nothing was filed into it; the rationale is kept in the history log"
            onClick={onClose}
          >
            <X className="h-3 w-3" />
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
