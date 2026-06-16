/**
 * The bottom-of-page Connections section — Wikipedia's "See also" for the
 * model graph. Every declared relationship gets a full-width row with room to
 * read: the peer as a wikilink, the link label as plain text, the protocol,
 * and the import-evidence verdict; beneath it, the peer node's own
 * description (the "annotation" Wikipedia puts after a See-also entry).
 * Grouped by direction — Uses (outgoing) / Used by (incoming) — plus the
 * code-suggested candidates with a one-click declare. The infobox keeps only
 * the compact name list; this section is where connections are understood
 * and edited.
 */

import { ArrowLeft, ArrowRight, Plus, Trash2 } from "lucide-react";
import type { ScryModel, Node, Link } from "./viewmodel";
import type { Editor } from "./editor";
import type { ModelHealthReport } from "./health";
import { linkEvidence } from "./health";
import { BTN, BTN_DANGER, BTN_GO, CTL, PageSection, SectionEditor } from "./pagekit";
import { wordDiff } from "./wordDiff";

/** How a declared link diverges from the committed model: `added` (new in the
 *  plan), `reworded` (label/method/endpoint changed), `deleted` (committed but
 *  dropped), or `unchanged`. */
type LinkDiffKind = "added" | "reworded" | "deleted" | "unchanged";

const LINK_MARK: Record<Exclude<LinkDiffKind, "unchanged">, { glyph: string; color: string }> = {
  added: { glyph: "+", color: "text-emerald-600 dark:text-emerald-400" },
  reworded: { glyph: "~", color: "text-amber-600 dark:text-amber-400" },
  deleted: { glyph: "−", color: "text-red-600 dark:text-red-400" },
};

/** Word-level add/remove highlight for a relabeled link. */
function WordDiffText({ from, to }: { from: string; to: string }) {
  return (
    <>
      {wordDiff(from, to).map((s, i) =>
        s.kind === "equal" ? (
          <span key={i}>{s.text}</span>
        ) : s.kind === "added" ? (
          <span key={i} className="rounded-[2px] bg-emerald-500/15 text-emerald-700 dark:text-emerald-300">
            {s.text}
          </span>
        ) : (
          <del key={i} className="text-[var(--text-muted)] decoration-red-400/60">
            {s.text}
          </del>
        ),
      )}
    </>
  );
}

/** Import evidence only exists between code-bearing nodes: a link to a person
 *  or an external system is asserted by nature, not suspicious. */
const codeBearing = (n: Node) => n.kind !== "person" && !n.external;

/** Import evidence, inline in the mono content lane: a dim `×N` when the code
 *  backs the link, and a muted tentative `· unverified` when no import witnesses
 *  it — never a verdict, since the resolver deliberately under-reports (runtime
 *  boundaries and cross-package deps have no import edge to find). */
function EvidenceTag({
  count,
  applicable,
}: {
  count: number;
  applicable: boolean;
}) {
  if (!applicable) return null;
  return count > 0 ? (
    <span
      className="shrink-0 italic text-[var(--text-ghost)]"
      title={`${count} import edge${count === 1 ? "" : "s"} back this link`}
    >
      &nbsp;(×{count})
    </span>
  ) : (
    <span
      className="shrink-0 italic text-[var(--text-ghost)]"
      title="No import in the code witnesses this link. Expected for runtime boundaries (IPC, HTTP, events) and cross-package / cross-crate dependencies the import resolver can't follow — absence of an edge is not a sign the link is wrong."
    >
      &nbsp;(unverified)
    </span>
  );
}

interface ConnDiffRow {
  link: Link;
  peer: Node;
  kind: LinkDiffKind;
  /** The committed copy, for word-diffing a relabeled link. */
  prev?: Link;
}

/** A code-suggested but undeclared edge to this node. */
interface SuggestedEdge {
  src: string;
  dst: string;
  count: number;
}

/** One declared relationship as a mono diff row — a change marker (+/~/−), the
 *  peer as a wikilink, label (word-diffed when reworded) and protocol inline,
 *  evidence dimmed. `removed` strikes the row (a pending deletion in edit mode);
 *  `control` is an optional action that rides in a reserved hover strip. */
function ConnRow({
  node,
  row,
  evidence,
  hasAudit,
  removed,
  control,
  onSelectNode,
}: {
  node: Node;
  row: ConnDiffRow;
  evidence: Record<string, number>;
  hasAudit: boolean;
  removed?: boolean;
  control?: React.ReactNode;
  onSelectNode: (id: string) => void;
}) {
  const { link, peer, kind, prev } = row;
  // A link the plan drops (committed-but-removed) or one staged for deletion in
  // the editor reads red + struck — deletion is the only thing link colour now
  // encodes. Everything else is a plain blue wikilink to a real page.
  const struck = kind === "deleted" || removed;
  const mark = removed ? LINK_MARK.deleted : kind === "unchanged" ? null : LINK_MARK[kind];
  const peerColor = struck
    ? "text-red-700 line-through decoration-red-500/60 dark:text-red-400"
    : "text-blue-700 dark:text-blue-400";
  return (
    <li className="group/erow relative grid grid-cols-[18px_22px_1fr] items-baseline py-[1.5px]">
      <span
        className={`select-none text-center font-mono text-xs font-bold ${mark?.color ?? "text-[var(--text-ghost)]"}`}
      >
        {mark?.glyph}
      </span>
      <span className="select-none" />
      <div className="flex min-w-0 items-baseline font-mono text-[12.5px] leading-[1.65]">
        <button
          type="button"
          onClick={() => onSelectNode(peer.id)}
          title={struck ? `${peer.name || "Untitled"} — slated for deletion in the plan` : peer.name}
          className={`shrink truncate text-left hover:underline cursor-pointer ${peerColor}`}
        >
          {peer.name || "Untitled"}
        </button>
        {(link.label || (kind === "reworded" && prev?.label)) && (
          <span className="shrink-0 text-[var(--text-tertiary)]">
            &nbsp;—{" "}
            {kind === "reworded" && prev && prev.label !== link.label ? (
              <WordDiffText from={prev.label ?? ""} to={link.label ?? ""} />
            ) : (
              link.label
            )}
          </span>
        )}
        {link.method && (
          <span className="shrink-0 text-[var(--text-ghost)]">&nbsp;· {link.method}</span>
        )}
        <EvidenceTag
          count={evidence[link.id] ?? 0}
          applicable={!struck && hasAudit && codeBearing(node) && codeBearing(peer)}
        />
      </div>
      {control && <span className={CTL}>{control}</span>}
    </li>
  );
}

/** A code-suggested edge as a mono row — the direction arrow, the peer (muted,
 *  it's not declared), the import count, and an optional declare control. */
function SuggestedRow({
  edge,
  peer,
  out,
  declared,
  control,
  onSelectNode,
}: {
  edge: SuggestedEdge;
  peer: Node;
  /** Outgoing (this node imports the peer) vs incoming. */
  out: boolean;
  declared?: boolean;
  control?: React.ReactNode;
  onSelectNode: (id: string) => void;
}) {
  return (
    <li className="group/erow relative grid grid-cols-[18px_22px_1fr] items-baseline py-[1.5px]">
      <span
        className={`relative top-px flex select-none justify-center ${declared ? "text-emerald-600 dark:text-emerald-400" : "text-[var(--text-ghost)]"}`}
      >
        {declared ? (
          <Plus className="h-3.5 w-3.5" />
        ) : out ? (
          <ArrowRight className="h-3.5 w-3.5" />
        ) : (
          <ArrowLeft className="h-3.5 w-3.5" />
        )}
      </span>
      <span className="select-none" />
      <div className="flex min-w-0 items-baseline font-mono text-[12.5px] leading-[1.65]">
        <button
          type="button"
          onClick={() => onSelectNode(peer.id)}
          className={`shrink truncate text-left hover:underline cursor-pointer ${declared ? "text-emerald-700 dark:text-emerald-400" : "text-[var(--text-muted)]"}`}
        >
          {peer.name || "Untitled"}
        </button>
        <span
          className="shrink-0 italic text-[var(--text-ghost)]"
          title={`${edge.count} import edge${edge.count === 1 ? "" : "s"} connect these nodes, but no link declares the relationship`}
        >
          &nbsp;(×{edge.count})
        </span>
      </div>
      {control && <span className={CTL}>{control}</span>}
    </li>
  );
}

function ConnGroup({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div className="mt-3 first:mt-1">
      <h3 className="mb-0.5 pl-[40px] text-2xs font-semibold uppercase tracking-[0.12em] text-[var(--text-ghost)]">
        {title}
      </h3>
      <ul className="flex flex-col">{children}</ul>
    </div>
  );
}

export function ConnectionsSection({
  model,
  committed,
  node,
  report,
  editor,
  editing,
  onToggle,
  onSelectNode,
}: {
  model: ScryModel;
  /** The committed model — the diff base for declared links. */
  committed: ScryModel | null;
  node: Node;
  report: ModelHealthReport | null;
  editor: Editor | undefined;
  editing: boolean;
  onToggle: () => void;
  onSelectNode: (id: string) => void;
}) {
  const byId = (id: string) => model.nodes.find((n) => n.id === id);
  const committedLinks = committed?.links ?? [];
  const committedById = new Map(committedLinks.map((l) => [l.id, l]));
  const plannedIds = new Set(model.links.map((l) => l.id));

  /** Tag a planned link against its committed copy. */
  const classify = (l: Link): { kind: LinkDiffKind; prev?: Link } => {
    const prev = committedById.get(l.id);
    if (!prev) return { kind: "added" };
    if (prev.label !== l.label || prev.method !== l.method || prev.src !== l.src || prev.dst !== l.dst)
      return { kind: "reworded", prev };
    return { kind: "unchanged", prev };
  };

  // Planned links in each direction, tagged; then committed links the plan
  // dropped, as `deleted` rows so the removal still reads on the page.
  const buildDir = (dir: "out" | "in"): ConnDiffRow[] => {
    const isMember = (l: Link) => (dir === "out" ? l.src === node.id : l.dst === node.id);
    const peerOf = (l: Link) => byId(dir === "out" ? l.dst : l.src);
    const rows: ConnDiffRow[] = [];
    for (const l of model.links.filter(isMember)) {
      const peer = peerOf(l);
      if (peer) rows.push({ link: l, peer, ...classify(l) });
    }
    for (const l of committedLinks.filter((l) => isMember(l) && !plannedIds.has(l.id))) {
      const peer = peerOf(l);
      if (peer) rows.push({ link: l, peer, kind: "deleted", prev: l });
    }
    return rows;
  };

  const outgoing = buildDir("out");
  const incoming = buildDir("in");
  // Candidate links the code shows but the model doesn't declare.
  const suggested = (report?.derived.unmodeled ?? []).filter(
    (e) => e.src === node.id || e.dst === node.id,
  );

  if (outgoing.length === 0 && incoming.length === 0 && suggested.length === 0) {
    return null;
  }

  const evidence = linkEvidence(report);
  const hasAudit = (report?.derived.linkAudit.length ?? 0) > 0;
  const declared =
    outgoing.filter((r) => r.kind !== "deleted").length +
    incoming.filter((r) => r.kind !== "deleted").length;

  // Read-mode rows carry no controls — editing is a proper section mode (Edit
  // → form → Cancel/Done in the header), not per-row affordances.
  const readRow = (r: ConnDiffRow) => (
    <ConnRow
      key={r.link.id}
      node={node}
      row={r}
      evidence={evidence}
      hasAudit={hasAudit}
      onSelectNode={onSelectNode}
    />
  );
  const suggestedRow = (e: SuggestedEdge, control?: React.ReactNode, declaredNow?: boolean) => {
    const out = e.src === node.id;
    const peer = byId(out ? e.dst : e.src);
    return peer ? (
      <SuggestedRow
        key={`${e.src}:${e.dst}`}
        edge={e}
        peer={peer}
        out={out}
        declared={declaredNow}
        control={control}
        onSelectNode={onSelectNode}
      />
    ) : null;
  };

  return (
    <PageSection
      title="Connections"
      count={declared}
      editable={!!editor}
      editing={editing}
      onToggleEdit={onToggle}
    >
      {editing && editor ? (
        <ConnectionsEditor
          node={node}
          outgoing={outgoing}
          incoming={incoming}
          suggested={suggested}
          byId={byId}
          evidence={evidence}
          hasAudit={hasAudit}
          editor={editor}
          onSelectNode={onSelectNode}
          onClose={onToggle}
        />
      ) : (
        <>
          {outgoing.length > 0 && <ConnGroup title="Uses">{outgoing.map(readRow)}</ConnGroup>}
          {incoming.length > 0 && <ConnGroup title="Used by">{incoming.map(readRow)}</ConnGroup>}
          {suggested.length > 0 && (
            <ConnGroup title="Suggested by the code">
              {suggested.map((e) => suggestedRow(e))}
            </ConnGroup>
          )}
        </>
      )}
    </PageSection>
  );
}

/**
 * The connections form inside the shared {@link SectionEditor} shell — Cancel /
 * Done ride in the section header. Deletions and declarations accumulate in a
 * draft (a removed link reads struck, a declared candidate reads as `+`) and
 * only hit the model on Done.
 */
function ConnectionsEditor({
  node,
  outgoing,
  incoming,
  suggested,
  byId,
  evidence,
  hasAudit,
  editor,
  onSelectNode,
  onClose,
}: {
  node: Node;
  outgoing: ConnDiffRow[];
  incoming: ConnDiffRow[];
  suggested: SuggestedEdge[];
  byId: (id: string) => Node | undefined;
  evidence: Record<string, number>;
  hasAudit: boolean;
  editor: Editor;
  onSelectNode: (id: string) => void;
  onClose: () => void;
}) {
  const edgeKey = (e: SuggestedEdge) => `${e.src}:${e.dst}`;
  return (
    <SectionEditor<{ remove: string[]; declare: string[] }>
      initial={{ remove: [], declare: [] }}
      onClose={onClose}
      onCommit={({ remove, declare }) => {
        for (const id of remove) editor.deleteLink(id);
        for (const key of declare) {
          const [src, dst] = key.split(":");
          editor.addLink(src, dst);
        }
      }}
    >
      {(draft, setDraft) => {
        const isRemoved = (id: string) => draft.remove.includes(id);
        const toggleRemove = (id: string) =>
          setDraft((d) => ({
            ...d,
            remove: d.remove.includes(id) ? d.remove.filter((x) => x !== id) : [...d.remove, id],
          }));
        const isDeclared = (key: string) => draft.declare.includes(key);
        const toggleDeclare = (key: string) =>
          setDraft((d) => ({
            ...d,
            declare: d.declare.includes(key)
              ? d.declare.filter((x) => x !== key)
              : [...d.declare, key],
          }));

        const editRow = (r: ConnDiffRow) =>
          r.kind === "deleted" ? (
            // Already dropped from the plan — not re-deletable here.
            <ConnRow
              key={r.link.id}
              node={node}
              row={r}
              evidence={evidence}
              hasAudit={hasAudit}
              onSelectNode={onSelectNode}
            />
          ) : (
            <ConnRow
              key={r.link.id}
              node={node}
              row={r}
              evidence={evidence}
              hasAudit={hasAudit}
              removed={isRemoved(r.link.id)}
              control={
                <button
                  type="button"
                  onClick={() => toggleRemove(r.link.id)}
                  className={isRemoved(r.link.id) ? BTN_GO : BTN_DANGER}
                >
                  {isRemoved(r.link.id) ? (
                    "Restore"
                  ) : (
                    <>
                      <Trash2 className="h-3 w-3" /> Delete
                    </>
                  )}
                </button>
              }
              onSelectNode={onSelectNode}
            />
          );

        return (
          <>
            {outgoing.length > 0 && <ConnGroup title="Uses">{outgoing.map(editRow)}</ConnGroup>}
            {incoming.length > 0 && <ConnGroup title="Used by">{incoming.map(editRow)}</ConnGroup>}
            {suggested.length > 0 && (
              <ConnGroup title="Suggested by the code">
                {suggested.map((e) => {
                  const out = e.src === node.id;
                  const peer = byId(out ? e.dst : e.src);
                  if (!peer) return null;
                  const key = edgeKey(e);
                  const declared = isDeclared(key);
                  return (
                    <SuggestedRow
                      key={key}
                      edge={e}
                      peer={peer}
                      out={out}
                      declared={declared}
                      control={
                        <button
                          type="button"
                          onClick={() => toggleDeclare(key)}
                          className={declared ? BTN : BTN_GO}
                        >
                          {declared ? (
                            "Undo"
                          ) : (
                            <>
                              <Plus className="h-3 w-3" /> Declare
                            </>
                          )}
                        </button>
                      }
                      onSelectNode={onSelectNode}
                    />
                  );
                })}
              </ConnGroup>
            )}
          </>
        );
      }}
    </SectionEditor>
  );
}
