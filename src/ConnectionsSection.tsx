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

import { Plus, X } from "lucide-react";
import type { ScryModel, Node, Link } from "./viewmodel";
import type { Editor } from "./editor";
import type { ModelHealthReport } from "./health";
import { linkEvidence } from "./health";
import { PILL_BASE } from "./statusColors";
import { kindIcon } from "./kindIcon";
import { isRedLink, PageSection, WikiLink, WikiText } from "./pagekit";

/** Import evidence only exists between code-bearing nodes: a link to a person
 *  or an external system is asserted by nature, not suspicious. */
const codeBearing = (n: Node) => n.kind !== "person" && !n.external;

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
      className="shrink-0 font-mono text-2xs tabular-nums text-[var(--text-ghost)]"
      title={`${count} import edge${count === 1 ? "" : "s"} back this link`}
    >
      ×{count}
    </span>
  ) : (
    <span
      className={`shrink-0 ${PILL_BASE} bg-orange-500/10 text-orange-700 ring-orange-500/25 dark:bg-orange-400/10 dark:text-orange-300 dark:ring-orange-400/25`}
      title="No import in the code crosses this link. It may still be real (runtime calls, IPC, HTTP), or it may be a link to question."
    >
      no code evidence
    </span>
  );
}

function ConnRow({
  model,
  node,
  peer,
  link,
  evidence,
  hasAudit,
  editor,
  onSelectNode,
}: {
  model: ScryModel;
  node: Node;
  peer: Node;
  link: Link;
  evidence: Record<string, number>;
  hasAudit: boolean;
  editor: Editor | undefined;
  onSelectNode: (id: string) => void;
}) {
  return (
    <li className="group/conn border-b border-[var(--border-subtle)] py-2 last:border-b-0">
      <div className="flex items-center gap-2">
        <WikiLink
          name={peer.name}
          Icon={kindIcon(peer)}
          red={isRedLink(peer)}
          onClick={() => onSelectNode(peer.id)}
        />
        {link.label && (
          <span className="min-w-0 text-xs text-[var(--text-muted)]">— {link.label}</span>
        )}
        {link.method && (
          <span className="shrink-0 font-mono text-2xs text-[var(--text-ghost)]">
            {link.method}
          </span>
        )}
        <span className="flex-1" />
        <EvidenceTag
          count={evidence[link.id] ?? 0}
          applicable={hasAudit && codeBearing(node) && codeBearing(peer)}
        />
        {editor && (
          <button
            type="button"
            title="Delete this link"
            onClick={() => editor.deleteLink(link.id)}
            className="shrink-0 rounded p-0.5 text-[var(--text-ghost)] opacity-0 transition-opacity hover:text-red-400 group-hover/conn:opacity-100 cursor-pointer"
          >
            <X className="h-3 w-3" />
          </button>
        )}
      </div>
      {peer.description && (
        <p className="mt-0.5 pl-6 text-xs leading-relaxed text-[var(--text-tertiary)]">
          <WikiText text={peer.description} nodes={model.nodes} onSelectNode={onSelectNode} />
        </p>
      )}
    </li>
  );
}

function ConnGroup({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div className="mt-2 first:mt-0">
      <h3 className="text-2xs font-semibold uppercase tracking-[0.14em] text-[var(--text-tertiary)]">
        {title}
      </h3>
      <ul className="mt-1 flex flex-col">{children}</ul>
    </div>
  );
}

export function ConnectionsSection({
  model,
  node,
  report,
  editor,
  onSelectNode,
}: {
  model: ScryModel;
  node: Node;
  report: ModelHealthReport | null;
  editor: Editor | undefined;
  onSelectNode: (id: string) => void;
}) {
  const byId = (id: string) => model.nodes.find((n) => n.id === id);
  const outgoing = model.links
    .filter((l) => l.src === node.id)
    .map((l) => ({ link: l, peer: byId(l.dst) }))
    .filter((x): x is { link: Link; peer: Node } => !!x.peer);
  const incoming = model.links
    .filter((l) => l.dst === node.id)
    .map((l) => ({ link: l, peer: byId(l.src) }))
    .filter((x): x is { link: Link; peer: Node } => !!x.peer);
  // Candidate links the code shows but the model doesn't declare.
  const suggested = (report?.derived.unmodeled ?? []).filter(
    (e) => e.src === node.id || e.dst === node.id,
  );

  if (outgoing.length === 0 && incoming.length === 0 && suggested.length === 0) {
    return null;
  }

  const evidence = linkEvidence(report);
  const hasAudit = (report?.derived.linkAudit.length ?? 0) > 0;
  const row = (x: { link: Link; peer: Node }) => (
    <ConnRow
      key={x.link.id}
      model={model}
      node={node}
      peer={x.peer}
      link={x.link}
      evidence={evidence}
      hasAudit={hasAudit}
      editor={editor}
      onSelectNode={onSelectNode}
    />
  );

  return (
    <PageSection title="Connections" count={outgoing.length + incoming.length}>
      {outgoing.length > 0 && <ConnGroup title="Uses">{outgoing.map(row)}</ConnGroup>}
      {incoming.length > 0 && <ConnGroup title="Used by">{incoming.map(row)}</ConnGroup>}
      {suggested.length > 0 && (
        <ConnGroup title="Suggested by the code">
          {suggested.map((e) => {
            const out = e.src === node.id;
            const p = byId(out ? e.dst : e.src);
            return p ? (
              <li
                key={`${e.src}:${e.dst}`}
                className="flex items-center gap-2 border-b border-[var(--border-subtle)] py-2 last:border-b-0"
              >
                <WikiLink
                  name={p.name}
                  Icon={kindIcon(p)}
                  dir={out ? "out" : "in"}
                  muted
                  onClick={() => onSelectNode(p.id)}
                />
                <span className="flex-1" />
                <span
                  className="shrink-0 font-mono text-2xs tabular-nums text-[var(--text-ghost)]"
                  title={`${e.count} import edge${e.count === 1 ? "" : "s"} connect these nodes, but no link declares the relationship`}
                >
                  ×{e.count}
                </span>
                {editor && (
                  <button
                    type="button"
                    title="Declare this link in the model"
                    onClick={() => editor.addLink(e.src, e.dst)}
                    className="shrink-0 rounded p-0.5 text-[var(--text-ghost)] hover:bg-[var(--surface-hover)] hover:text-[var(--text-secondary)] cursor-pointer"
                  >
                    <Plus className="h-3 w-3" />
                  </button>
                )}
              </li>
            ) : null;
          })}
        </ConnGroup>
      )}
    </PageSection>
  );
}
