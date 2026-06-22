/**
 * The bottom-of-page Connections section — Wikipedia's "See also" for the
 * model graph. Every declared relationship gets a full-width row with room to
 * read: the peer as a wikilink, the link label as plain text, the protocol,
 * and the import-evidence verdict; beneath it, the peer node's own
 * description (the "annotation" Wikipedia puts after a See-also entry).
 * Grouped by direction — Uses (outgoing) / Used by (incoming) — plus the
 * code-suggested candidates with a one-click declare. This is where
 * connections are read and edited.
 */

import { useMemo, useState } from "react";
import { ArrowLeft, ArrowRight, ChevronDown, ChevronRight, Plus, Trash2 } from "lucide-react";
import type { ScryModel, Node, Link } from "./viewmodel";
import { effectiveSourceMap } from "./viewmodel";
import type { Editor } from "./editor";
import type { ModelHealthReport, ImpliedConn, LinkPath } from "./health";
import { linkEvidence, impliedFor, impliedPaths, pathsForLink } from "./health";
import { kindIcon } from "./kindIcon";
import { BTN, BTN_DANGER, BTN_GO, CTL, Editable, PageSection, SectionEditor } from "./pagekit";
import { usePageMenu, useCopyId, copyIdItem } from "./pageMenu";
import { wordDiff } from "./wordDiff";

// Row grid: marker | index | content. The hover control (CTL) floats over the
// right edge as an absolute overlay (hence `relative`), taking no layout space.
const CONN_ROW = "relative grid grid-cols-[18px_22px_1fr] items-baseline";

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
  edit,
  expand,
  control,
  onSelectNode,
}: {
  node: Node;
  row: ConnDiffRow;
  evidence: Record<string, number>;
  hasAudit: boolean;
  removed?: boolean;
  /** When present (edit mode, row not staged for deletion), the label and
   *  protocol render as inline editable fields seeded from these values. */
  edit?: { onLabel: (t: string) => void; onMethod: (t: string) => void };
  /** When present, a disclosure chevron in the index lane toggles the link's
   *  underlying code-path ladder (read mode only). */
  expand?: { open: boolean; onToggle: () => void };
  control?: React.ReactNode;
  onSelectNode: (id: string) => void;
}) {
  const { link, peer, kind, prev } = row;
  const openMenu = usePageMenu();
  const copyId = useCopyId();
  // A link the plan drops (committed-but-removed) or one staged for deletion in
  // the editor reads red + struck — deletion is the only thing link colour now
  // encodes. Everything else is a plain blue wikilink to a real page.
  const struck = kind === "deleted" || removed;
  const mark = removed ? LINK_MARK.deleted : kind === "unchanged" ? null : LINK_MARK[kind];
  // A brand-new link reads green (the connection itself is new), a dropped one
  // red + struck — the same git-style added/removed signal the claim rows carry.
  const peerColor = struck
    ? "text-red-700 line-through decoration-red-500/60 dark:text-red-400"
    : kind === "added" && !removed
      ? "text-emerald-700 hover:text-emerald-800 dark:text-emerald-300 dark:hover:text-emerald-200"
      : "text-blue-700 dark:text-blue-400";
  // Inline editing only on live rows; a row staged for deletion keeps its
  // struck, read-only rendering so the removal still reads at a glance.
  const editable = edit && !struck;
  return (
    <li
      className={`group/erow ${CONN_ROW} py-[1.5px]`}
      onContextMenu={(e) => openMenu(e, [copyIdItem(link.id, copyId)])}
    >
      <span
        className={`select-none text-center font-mono text-xs font-bold ${mark?.color ?? "text-[var(--text-ghost)]"}`}
      >
        {mark?.glyph}
      </span>
      {expand ? (
        <button
          type="button"
          onClick={expand.onToggle}
          title={expand.open ? "Hide code paths" : "Show the code paths behind this link"}
          className="flex select-none items-center justify-center text-[var(--text-ghost)] hover:text-[var(--text-secondary)]"
        >
          {expand.open ? <ChevronDown className="h-3 w-3" /> : <ChevronRight className="h-3 w-3" />}
        </button>
      ) : (
        <span className="select-none" />
      )}
      <div className="flex min-w-0 items-baseline font-mono text-[12.5px] leading-[1.65]">
        <button
          type="button"
          onClick={() => onSelectNode(peer.id)}
          title={struck ? `${peer.name || "Untitled"} — slated for deletion in the plan` : peer.name}
          className={`shrink truncate text-left hover:underline ${peerColor}`}
        >
          {peer.name || "Untitled"}
        </button>
        {editable ? (
          // Always-present edit slots so a labelless link can gain one.
          <>
            <span className="shrink-0 text-[var(--text-tertiary)]">
              &nbsp;—{" "}
              <Editable initial={link.label ?? ""} placeholder="label" onInput={edit!.onLabel} />
            </span>
            <span className="shrink-0 text-[var(--text-ghost)]">
              &nbsp;·{" "}
              <Editable initial={link.method ?? ""} placeholder="method" onInput={edit!.onMethod} />
            </span>
          </>
        ) : (
          <>
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
          </>
        )}
        <EvidenceTag
          count={evidence[link.id] ?? 0}
          applicable={!struck && !editable && hasAudit && codeBearing(node) && codeBearing(peer)}
        />
      </div>
      {control && <span className={CTL}>{control}</span>}
    </li>
  );
}

// --- node references & tree scaffolding --------------------------------------

/** A node as an inline reference: its C4 kind icon + clickable name. The icon
 *  carries the altitude (container vs component vs symbol) at a glance; the name
 *  navigates. `muted` is for breadcrumb ancestors that frame, not lead. */
function NodeRef({
  node,
  onSelectNode,
  muted,
  plain,
}: {
  node: Node;
  onSelectNode: (id: string) => void;
  muted?: boolean;
  /** Render as static text, not a link — for endpoints the surrounding row's
   *  own wikilink already navigates to (the peer of a connection). */
  plain?: boolean;
}) {
  const Icon = kindIcon(node);
  const label = node.name || "Untitled";
  return (
    <span className="inline-flex min-w-0 items-center gap-1">
      <Icon className="h-3 w-3 shrink-0 text-[var(--text-ghost)]" />
      {plain ? (
        <span
          title={node.name}
          className={`truncate ${muted ? "text-[var(--text-tertiary)]" : "text-[var(--text-secondary)]"}`}
        >
          {label}
        </span>
      ) : (
        <button
          type="button"
          onClick={() => onSelectNode(node.id)}
          title={node.name}
          className={`truncate text-left hover:underline ${
            muted
              ? "text-[var(--text-tertiary)] hover:text-[var(--text-secondary)]"
              : "text-blue-700 dark:text-blue-400"
          }`}
        >
          {label}
        </button>
      )}
    </span>
  );
}

/** A symbol node's source file: its own definition anchor, else the first of
 *  its responsibilities' anchors. Undefined when nothing anchors it. */
function symbolFile(model: ScryModel, node: Node): string | undefined {
  const sm = model.sourceMap ?? {};
  return (
    sm[node.id]?.[0]?.pattern ??
    node.responsibilities?.map((r) => sm[r.id]?.[0]?.pattern).find(Boolean)
  );
}

/** When a symbol shares its name with a sibling symbol (same parent), the bare
 *  `Host › name` reads identically for two different functions — e.g. two `main`s
 *  in separate example files. Return a file basename to tell them apart; undefined
 *  in the common case, so unambiguous symbols stay clean. */
function symbolDiscriminator(model: ScryModel, node: Node): string | undefined {
  if (node.kind !== "symbol" || !node.parentId) return undefined;
  const collides = model.nodes.some(
    (n) =>
      n.id !== node.id &&
      n.parentId === node.parentId &&
      n.kind === "symbol" &&
      n.name === node.name,
  );
  if (!collides) return undefined;
  return symbolFile(model, node)?.split("/").pop();
}

/** Inline breadcrumb arrow — joins a host to its member (and an ancestor to its
 *  child). An arrow, not a chevron: the chevron is reserved for expand toggles,
 *  so a static separator never reads as a clickable control. */
function Sep() {
  return <span className="shrink-0 select-none px-0.5 text-[var(--text-ghost)]">→</span>;
}

/** One endpoint of a code path — the host node (component/container, a real node
 *  → icon + link) and the symbol within it. When the symbol is itself a modeled
 *  node it links too; otherwise it's a bare code identifier. `relativeTo` is the
 *  current page node: an endpoint inside it renders abbreviated (host dropped),
 *  since repeating the page you're on is the noise that makes "is this me?"
 *  unanswerable. */
function EndpointRef({
  id,
  symbol,
  model,
  relativeTo,
  plainId,
  byId,
  onSelectNode,
}: {
  id: string;
  symbol: string;
  model: ScryModel;
  /** Current page node id — endpoints within it drop their redundant host. */
  relativeTo?: string;
  /** The row's already-linked peer node — a segment equal to it renders as static
   *  text (re-linking the header's target is noise); finer sub-nodes stay links. */
  plainId?: string;
  byId: (id: string) => Node | undefined;
  onSelectNode: (id: string) => void;
}) {
  const node = byId(id);
  if (!node) return <span className="truncate">{symbol}</span>;
  // A resolved symbol node: show its host (parent) then the symbol node itself,
  // plus a file discriminator when a same-named sibling would make it ambiguous.
  if (node.kind === "symbol") {
    const parent = node.parentId ? byId(node.parentId) : undefined;
    // Drop the host when it — or the symbol itself — is the current page node:
    // redundant, so the symbol reads bare as "yours".
    const host =
      parent && parent.id !== relativeTo && node.id !== relativeTo ? parent : undefined;
    const disc = symbolDiscriminator(model, node);
    return (
      <span className="inline-flex min-w-0 items-center gap-1">
        {host && (
          <>
            <NodeRef node={host} onSelectNode={onSelectNode} muted plain={host.id === plainId} />
            <Sep />
          </>
        )}
        <NodeRef node={node} onSelectNode={onSelectNode} plain={node.id === plainId} />
        {disc && <span className="shrink-0 italic text-[var(--text-ghost)]">&nbsp;({disc})</span>}
      </span>
    );
  }
  // Resolved to a component/container (boundary fallback): the symbol is a raw
  // code identifier, not a modeled node. When the host is the current page node,
  // show just the bare identifier — the page header already names the host.
  if (node.id === relativeTo) {
    return <span className="truncate text-[var(--text-tertiary)]">{symbol || node.name}</span>;
  }
  return (
    <span className="inline-flex min-w-0 items-center gap-1">
      <NodeRef node={node} onSelectNode={onSelectNode} plain={node.id === plainId} />
      {symbol && symbol !== node.name && (
        <>
          <Sep />
          <span className="truncate text-[var(--text-tertiary)]">{symbol}</span>
        </>
      )}
    </span>
  );
}

/** Collapse leaf paths into a ladder oriented around the CURRENT node: one
 *  branch per *self* endpoint (the side inside the page's node), its connected
 *  *peer* endpoints beneath. `selfSide` says which end of each edge is self —
 *  `src` for a Uses/outgoing connection, `dst` for a Used-by/incoming one. Paths
 *  that resolve to the same (node, symbol) pair are merged and counts summed. */
function groupPaths(
  paths: LinkPath[],
  selfSide: "src" | "dst",
): {
  selfId: string;
  selfSymbol: string;
  peers: { peerId: string; peerSymbol: string; count: number }[];
}[] {
  const sep = " ";
  const groups = new Map<
    string,
    {
      selfId: string;
      selfSymbol: string;
      peers: Map<string, { peerId: string; peerSymbol: string; count: number }>;
    }
  >();
  for (const p of paths) {
    const selfId = selfSide === "src" ? p.srcId : p.dstId;
    const selfSymbol = selfSide === "src" ? p.srcSymbol : p.dstSymbol;
    const peerId = selfSide === "src" ? p.dstId : p.srcId;
    const peerSymbol = selfSide === "src" ? p.dstSymbol : p.srcSymbol;
    const sk = `${selfId}${sep}${selfSymbol}`;
    let g = groups.get(sk);
    if (!g) {
      g = { selfId, selfSymbol, peers: new Map() };
      groups.set(sk, g);
    }
    const pk = `${peerId}${sep}${peerSymbol}`;
    const cur = g.peers.get(pk);
    if (cur) cur.count += p.count;
    else g.peers.set(pk, { peerId, peerSymbol, count: p.count });
  }
  return [...groups.values()].map((g) => ({
    selfId: g.selfId,
    selfSymbol: g.selfSymbol,
    peers: [...g.peers.values()].sort((a, b) => b.count - a.count),
  }));
}

/** The code paths behind one connection, oriented around the current node: each
 *  branch is one of *your* symbols (rendered bare via {@link EndpointRef}'s
 *  `relativeTo`), and beneath it the peer symbols it connects to, each prefixed
 *  with the relationship verb (`uses` / `used by`) so the direction reads as a
 *  sentence — "your `slice_container` is *used by* their `main`" — not as a
 *  containment tree. Shared by declared links and implied connections. */
function PathLadder({
  paths,
  model,
  nodeId,
  peerId,
  selfSide,
  verb,
  byId,
  onSelectNode,
}: {
  paths: LinkPath[];
  model: ScryModel;
  /** Current page node — your symbols render relative to it (host dropped). */
  nodeId: string;
  /** The connection's peer (the row header's wikilink) — rendered plain in the
   *  ladder, since re-linking it is redundant; sub-nodes below it keep links. */
  peerId: string;
  /** Which end of each edge is the self (page) side. */
  selfSide: "src" | "dst";
  /** Relationship read from your side: `uses` (outgoing) / `used by` (incoming). */
  verb: string;
  byId: (id: string) => Node | undefined;
  onSelectNode: (id: string) => void;
}) {
  const groups = groupPaths(paths, selfSide);
  return (
    <li className={`${CONN_ROW} pb-1.5`}>
      <span className="select-none" />
      <span className="select-none" />
      <ul className="flex flex-col gap-2 pl-1 font-mono text-[11.5px] leading-[1.55]">
        {groups.map((g, i) => (
          <li key={i} className="flex min-w-0 flex-col">
            <EndpointRef
              id={g.selfId}
              symbol={g.selfSymbol}
              model={model}
              relativeTo={nodeId}
              byId={byId}
              onSelectNode={onSelectNode}
            />
            <ul className="flex flex-col gap-px pl-4">
              {g.peers.map((p, j) => (
                <li key={j} className="flex min-w-0 items-center gap-1.5 py-px">
                  <span className="shrink-0 select-none text-[var(--text-ghost)]">{verb}</span>
                  <EndpointRef
                    id={p.peerId}
                    symbol={p.peerSymbol}
                    model={model}
                    plainId={peerId}
                    byId={byId}
                    onSelectNode={onSelectNode}
                  />
                  {p.count > 1 && (
                    <span className="shrink-0 italic text-[var(--text-ghost)]">(×{p.count})</span>
                  )}
                </li>
              ))}
            </ul>
          </li>
        ))}
      </ul>
    </li>
  );
}

/** A read-mode declared-link row with a disclosure: clicking the chevron expands
 *  the link into its backing code paths. Asserted-only links (no path) render as
 *  a plain row with no chevron. */
function ReadConnRow({
  node,
  row,
  evidence,
  hasAudit,
  report,
  model,
  byId,
  onSelectNode,
}: {
  node: Node;
  row: ConnDiffRow;
  evidence: Record<string, number>;
  hasAudit: boolean;
  report: ModelHealthReport | null;
  model: ScryModel;
  byId: (id: string) => Node | undefined;
  onSelectNode: (id: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const paths = useMemo(
    () => pathsForLink(report, model, row.link),
    [report, model, row.link],
  );
  // The page node is one end of the link; which end fixes how the ladder reads.
  const selfSide: "src" | "dst" = row.link.src === node.id ? "src" : "dst";
  const verb = selfSide === "src" ? "uses" : "used by";
  return (
    <>
      <ConnRow
        node={node}
        row={row}
        evidence={evidence}
        hasAudit={hasAudit}
        expand={paths.length > 0 ? { open, onToggle: () => setOpen((o) => !o) } : undefined}
        onSelectNode={onSelectNode}
      />
      {open && paths.length > 0 && (
        <PathLadder
          paths={paths}
          model={model}
          nodeId={node.id}
          peerId={row.peer.id}
          selfSide={selfSide}
          verb={verb}
          byId={byId}
          onSelectNode={onSelectNode}
        />
      )}
    </>
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
    <li className={`group/erow ${CONN_ROW} py-[1.5px]`}>
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
          className={`shrink truncate text-left hover:underline ${declared ? "text-emerald-700 dark:text-emerald-400" : "text-[var(--text-muted)]"}`}
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

/** A code-implied connection as an expandable row — the peer rendered as a
 *  breadcrumb of {@link NodeRef}s (each node its own C4 kind icon, so altitude
 *  reads per-node and an icon never sits orphaned beside an ancestor of a
 *  different kind), the summed import count, and a disclosure that expands into
 *  the same {@link PathLadder} a declared link uses — the leaf code paths behind
 *  the connection. Direction is carried by the section (Uses vs Used by), so
 *  there's no per-row arrow. Read-only — no declare control. */
function ImpliedRow({
  conn,
  peer,
  context,
  nodeId,
  model,
  report,
  byId,
  onSelectNode,
}: {
  conn: ImpliedConn;
  peer: Node;
  /** Peer's ancestors (container down to its parent), so its altitude and
   *  whereabouts read at a glance — "MCP Server › Model Observability Commands"
   *  vs a bare top-level "App Frontend". */
  context: Node[];
  nodeId: string;
  model: ScryModel;
  report: ModelHealthReport | null;
  byId: (id: string) => Node | undefined;
  onSelectNode: (id: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const paths = useMemo(
    () => impliedPaths(report, model, nodeId, conn),
    [report, model, nodeId, conn],
  );
  // `out` = your code reaches the peer (you use it); `in` = the peer reaches you.
  const selfSide: "src" | "dst" = conn.dir === "out" ? "src" : "dst";
  const verb = conn.dir === "out" ? "uses" : "used by";
  return (
    <>
      <li className={`group/erow ${CONN_ROW} py-[1.5px]`}>
        <span className="select-none" />
        {paths.length > 0 ? (
          <button
            type="button"
            onClick={() => setOpen((o) => !o)}
            title={open ? "Hide code paths" : "Show the code paths behind this connection"}
            className="flex select-none items-center justify-center text-[var(--text-ghost)] hover:text-[var(--text-secondary)]"
          >
            {open ? <ChevronDown className="h-3 w-3" /> : <ChevronRight className="h-3 w-3" />}
          </button>
        ) : (
          <span className="select-none" />
        )}
        <div className="flex min-w-0 items-center font-mono text-[12.5px] leading-[1.65]">
          {context.map((a) => (
            <span key={a.id} className="flex min-w-0 shrink items-center">
              <NodeRef node={a} onSelectNode={onSelectNode} muted />
              <Sep />
            </span>
          ))}
          <NodeRef node={peer} onSelectNode={onSelectNode} />
          <span
            className="ml-1.5 shrink-0 italic text-[var(--text-ghost)]"
            title={`${conn.count} import edge${conn.count === 1 ? "" : "s"} in the code, below the link altitude`}
          >
            (×{conn.count})
          </span>
        </div>
      </li>
      {open && paths.length > 0 && (
        <PathLadder
          paths={paths}
          model={model}
          nodeId={nodeId}
          peerId={conn.peerId}
          selfSide={selfSide}
          verb={verb}
          byId={byId}
          onSelectNode={onSelectNode}
        />
      )}
    </>
  );
}

/** Peer's ancestors from container level down to its parent, top-level system
 *  dropped (it's the same for every node, so it's noise). Root-first. */
function peerContext(model: ScryModel, peerId: string): Node[] {
  const byId = new Map(model.nodes.map((n) => [n.id, n]));
  const out: Node[] = [];
  const seen = new Set<string>();
  let cur = byId.get(peerId)?.parentId ? byId.get(byId.get(peerId)!.parentId!) : undefined;
  while (cur && !seen.has(cur.id)) {
    seen.add(cur.id);
    if (cur.kind !== "system") out.unshift(cur);
    cur = cur.parentId ? byId.get(cur.parentId) : undefined;
  }
  return out;
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
  model: rawModel,
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
  // Code-side anchors live in the committed model; the plan overlays only the
  // anchors it adds. Merge for display so symbol files resolve regardless of
  // which layer owns the anchor (the dedup invariant) — display-only, never
  // persisted back to the plan.
  const model = { ...rawModel, sourceMap: effectiveSourceMap(committed, rawModel) };
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
    <ReadConnRow
      key={r.link.id}
      node={node}
      row={r}
      evidence={evidence}
      hasAudit={hasAudit}
      report={report}
      model={model}
      byId={byId}
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
        // Same wrapper as the editor body so read↔edit never reflows.
        <div className="flex flex-col">
          {outgoing.length > 0 && <ConnGroup title="Uses">{outgoing.map(readRow)}</ConnGroup>}
          {incoming.length > 0 && <ConnGroup title="Used by">{incoming.map(readRow)}</ConnGroup>}
          {suggested.length > 0 && (
            <ConnGroup title="Suggested by the code">
              {suggested.map((e) => suggestedRow(e))}
            </ConnGroup>
          )}
        </div>
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
    <SectionEditor<{
      remove: string[];
      declare: string[];
      /** Per-link label/method edits, keyed by link id; only changed links appear. */
      patch: Record<string, { label: string; method: string }>;
    }>
      initial={{ remove: [], declare: [], patch: {} }}
      onClose={onClose}
      onCommit={({ remove, declare, patch }) => {
        for (const id of remove) editor.deleteLink(id);
        // A link staged for deletion ignores any label edits — deletion wins.
        for (const [id, p] of Object.entries(patch)) {
          if (remove.includes(id)) continue;
          editor.updateLink(id, { label: p.label.trim(), method: p.method.trim() });
        }
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
        // Record an edited field, seeding from the link's current values.
        const setField = (link: Link, field: "label" | "method", val: string) =>
          setDraft((d) => {
            const cur = d.patch[link.id] ?? {
              label: link.label ?? "",
              method: link.method ?? "",
            };
            return { ...d, patch: { ...d.patch, [link.id]: { ...cur, [field]: val } } };
          });
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
              edit={{
                onLabel: (t) => setField(r.link, "label", t),
                onMethod: (t) => setField(r.link, "method", t),
              }}
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
          <div className="flex flex-col">
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
          </div>
        );
      }}
    </SectionEditor>
  );
}

/**
 * Implied Connections — the read-only companion to {@link ConnectionsSection}.
 * Derived purely from the code's import graph: every cross-LEVEL reach of this
 * node's subtree (a symbol's callers, a component reaching into another
 * container), with the peer rolled up to its architectural node and counts
 * summed. Same-parent siblings are deliberately absent — those are candidate
 * same-level links, surfaced as "Suggested by the code" in the editable section.
 * Nothing here is editable: it's a fact about the code, not an authored claim.
 */
export function ImpliedConnectionsSection({
  model,
  node,
  report,
  onSelectNode,
}: {
  model: ScryModel;
  node: Node;
  report: ModelHealthReport | null;
  onSelectNode: (id: string) => void;
}) {
  const implied = impliedFor(report, model, node.id);
  if (implied.length === 0) return null;
  const byId = (id: string) => model.nodes.find((n) => n.id === id);
  const out = implied.filter((c) => c.dir === "out");
  const inc = implied.filter((c) => c.dir === "in");
  const row = (c: ImpliedConn) => {
    const peer = byId(c.peerId);
    return peer ? (
      <ImpliedRow
        key={`${c.dir}:${c.peerId}`}
        conn={c}
        peer={peer}
        context={peerContext(model, c.peerId)}
        nodeId={node.id}
        model={model}
        report={report}
        byId={byId}
        onSelectNode={onSelectNode}
      />
    ) : null;
  };
  return (
    <PageSection title="Implied Connections" count={implied.length}>
      <div className="flex flex-col">
        {out.length > 0 && <ConnGroup title="Uses">{out.map(row)}</ConnGroup>}
        {inc.length > 0 && <ConnGroup title="Used by">{inc.map(row)}</ConnGroup>}
      </div>
    </PageSection>
  );
}
