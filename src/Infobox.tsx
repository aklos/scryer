/**
 * The infobox — Wikipedia's right-hand structured summary card. At-a-glance
 * facts: kind, technology, status, position in the hierarchy, connected peers
 * (names only — labels, evidence, and editing live in the page's Connections
 * section), and the code boundary. Reads by default; [edit] opens the
 * editable facts (kind, technology, flags, icon) in the shared transactional
 * SectionEditor — nothing persists until Done.
 *
 * Kind is STRUCTURAL: with a parent, a node's kind is fully determined by the
 * hierarchy (child of a container can only be a component), so it renders
 * read-only; only a top-level childless node offers a real choice
 * (person vs system). You change a node's kind by moving it, not by editing
 * a field.
 */

import { useRef, useState } from "react";
import { ImageIcon } from "lucide-react";
import type { ScryModel, Node, Group, Kind } from "./viewmodel";
import { childKindFor } from "./viewmodel";
import type { Editor } from "./editor";
import type { ModelHealthReport } from "./health";
import { FLAG_COLORS, PILL_BASE, STATUS_COLORS } from "./statusColors";
import { kindIcon, typeTag } from "./kindIcon";
import { effectiveNodeStatus } from "./rollup";
import { IconPicker } from "./IconPicker";
import { Input, Select, Toggle, type SelectOption } from "./ui";
import { EditLink, isRedLink, SectionEditor, StatusTag, WikiLink } from "./pagekit";

const KIND_LABEL: Record<Kind, string> = {
  person: "Person",
  system: "System",
  container: "Container",
  component: "Component",
  symbol: "Symbol",
};

const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? "" : "s"}`;

/** Compact relative timestamp for the "Last touched" fact. */
function relTime(unixSec: number): string {
  const d = Date.now() / 1000 - unixSec;
  if (d < 90) return "just now";
  if (d < 3600) return `${Math.round(d / 60)}m ago`;
  if (d < 86400 * 2) return `${Math.round(d / 3600)}h ago`;
  if (d < 86400 * 30) return `${Math.round(d / 86400)}d ago`;
  return new Date(unixSec * 1000).toLocaleDateString();
}

/** The kinds this node may legally hold, given its position in the tree. */
function validKinds(model: ScryModel, node: Node): Kind[] {
  if (node.parentId) {
    const parent = model.nodes.find((n) => n.id === node.parentId);
    return parent ? [childKindFor(parent.kind)] : [node.kind];
  }
  const hasChildren = model.nodes.some((n) => n.parentId === node.id);
  return hasChildren ? ["system"] : ["person", "system"];
}

interface InfoDraft {
  kind: Kind;
  technology: string;
  visual: boolean;
  deprecated: boolean;
  relocated: boolean;
  icon: string | null;
}

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex flex-col gap-1 px-3.5 py-2.5">
      <span className="text-2xs font-semibold uppercase tracking-[0.14em] text-[var(--text-tertiary)]">
        {label}
      </span>
      <div className="text-xs text-[var(--text-secondary)]">{children}</div>
    </div>
  );
}

export function Infobox({
  model,
  node,
  report,
  editor,
  onSelectNode,
  onSelectGroup,
}: {
  model: ScryModel;
  node: Node;
  /** Observability report — boundary coverage and last-touched facts.
   *  Those rows simply don't render without it. */
  report: ModelHealthReport | null;
  editor: Editor | undefined;
  onSelectNode: (id: string) => void;
  onSelectGroup: (id: string) => void;
}) {
  const [editing, setEditing] = useState(false);
  const [iconRect, setIconRect] = useState<DOMRect | null>(null);
  const iconBtnRef = useRef<HTMLButtonElement>(null);

  const outgoing = model.links.filter((l) => l.src === node.id);
  const incoming = model.links.filter((l) => l.dst === node.id);
  const boundary = model.boundaries?.[node.id] ?? [];
  const byId = (id: string) => model.nodes.find((n) => n.id === id);

  const parent = node.parentId ? byId(node.parentId) : undefined;
  const group: Group | undefined = model.groups.find((g) =>
    g.memberIds.includes(node.id),
  );
  const status = effectiveNodeStatus(node);
  const tag = typeTag(node);
  const kinds = validKinds(model, node);
  const kindOptions: SelectOption[] = kinds.map((k) => ({ value: k, label: KIND_LABEL[k] }));

  // --- derived at-a-glance facts ---
  const health = report?.health.nodes[node.id];

  // Contents: what's one level down (children by kind, plus anchored groups).
  const children = model.nodes.filter((n) => n.parentId === node.id);
  const childGroups = model.groups.filter((g) => g.parentNodeId === node.id);
  const contents = (["person", "system", "container", "component", "symbol"] as Kind[])
    .map((k) => ({ k, n: children.filter((c) => c.kind === k).length }))
    .filter((x) => x.n > 0)
    .map((x) => plural(x.n, KIND_LABEL[x.k].toLowerCase()));
  if (childGroups.length > 0) contents.push(plural(childGroups.length, "group"));

  // Claims: the node's OWN contract — subtree numbers live in the tree badges.
  const resps = node.responsibilities ?? [];
  const props = node.properties ?? [];
  const isPlanned = (s?: string) => s === "proposed" || s === "changed" || s === undefined;
  const respPlanned = resps.filter((r) => !r.vagrant && isPlanned(r.status)).length;
  const respFlagged = resps.filter((r) => r.vagrant || r.stale).length;
  const propPlanned = props.filter((p) => isPlanned(p.status)).length;

  // Code: where this node lives on disk. Symbols list their mapped files;
  // structural nodes show their boundary globs with lens coverage.
  const ownFiles = [
    ...new Set(
      [node.id, ...resps.map((r) => r.id)]
        .flatMap((k) => model.sourceMap?.[k] ?? [])
        .map((l) => l.pattern),
    ),
  ];
  const coverage = health?.boundary;

  const touched = Math.max(health?.own.lastTouchedAt ?? 0, health?.subtree.lastTouchedAt ?? 0);

  return (
    <aside className="overflow-hidden rounded-md border border-[var(--border)] bg-[var(--surface-raised)] text-[var(--text)]">
      <div className="flex items-center justify-between border-b border-[var(--border-subtle)] bg-[var(--surface)] px-3.5 py-2">
        <span className="truncate text-xs font-semibold text-[var(--text)]">
          {node.name || "Untitled"}
        </span>
        {editor && !editing && <EditLink editing={false} onClick={() => setEditing(true)} />}
      </div>

      <div className="divide-y divide-[var(--border-subtle)]">
        {editing && editor ? (
          <div className="px-3.5 py-2.5">
            <SectionEditor<InfoDraft>
              initial={{
                kind: node.kind,
                technology: node.technology ?? "",
                visual: !!node.visual,
                deprecated: !!node.deprecated,
                relocated: !!node.relocated,
                icon: node.icon ?? null,
              }}
              onCommit={(d) =>
                editor.updateNode(node.id, {
                  kind: d.kind,
                  technology: d.technology.trim() || undefined,
                  visual: d.visual || undefined,
                  deprecated: d.deprecated || undefined,
                  relocated: d.relocated || undefined,
                  icon: d.icon ?? undefined,
                })
              }
              onClose={() => setEditing(false)}
            >
              {(draft, setDraft) => (
                <div className="flex flex-col gap-3">
                  <label className="flex flex-col gap-1">
                    <span className="text-2xs font-semibold uppercase tracking-[0.14em] text-[var(--text-tertiary)]">
                      Kind
                    </span>
                    {kinds.length > 1 ? (
                      <Select
                        variant="bordered"
                        options={kindOptions}
                        value={draft.kind}
                        onChange={(v) => setDraft((d) => ({ ...d, kind: v as Kind }))}
                      />
                    ) : (
                      <span
                        className="text-xs text-[var(--text-secondary)]"
                        title="Kind is determined by the node's position in the hierarchy — move the node to change it."
                      >
                        {tag.type}
                        <span className="ml-1.5 text-[var(--text-muted)]">
                          — fixed by hierarchy
                        </span>
                      </span>
                    )}
                  </label>

                  <label className="flex flex-col gap-1">
                    <span className="text-2xs font-semibold uppercase tracking-[0.14em] text-[var(--text-tertiary)]">
                      Technology
                    </span>
                    <Input
                      variant="inline"
                      value={draft.technology}
                      placeholder="e.g. PostgreSQL 16"
                      onChange={(e) => setDraft((d) => ({ ...d, technology: e.target.value }))}
                    />
                  </label>

                  <div className="flex flex-col gap-1.5">
                    <span className="text-2xs font-semibold uppercase tracking-[0.14em] text-[var(--text-tertiary)]">
                      Flags
                    </span>
                    <label className="flex items-center justify-between gap-2">
                      <span className="text-xs text-[var(--text-secondary)]">
                        Visual <span className="text-[var(--text-muted)]">— renders UI</span>
                      </span>
                      <Toggle
                        value={draft.visual}
                        onChange={(v) => setDraft((d) => ({ ...d, visual: v }))}
                      />
                    </label>
                    <label className="flex items-center justify-between gap-2">
                      <span className="text-xs text-[var(--text-secondary)]">
                        Deprecated <span className="text-[var(--text-muted)]">— planned for removal</span>
                      </span>
                      <Toggle
                        value={draft.deprecated}
                        onChange={(v) => setDraft((d) => ({ ...d, deprecated: v }))}
                      />
                    </label>
                    <label className="flex items-center justify-between gap-2">
                      <span className="text-xs text-[var(--text-secondary)]">
                        Relocated <span className="text-[var(--text-muted)]">— code needs to move</span>
                      </span>
                      <Toggle
                        value={draft.relocated}
                        onChange={(v) => setDraft((d) => ({ ...d, relocated: v }))}
                      />
                    </label>
                  </div>

                  <label className="flex flex-col gap-1">
                    <span className="text-2xs font-semibold uppercase tracking-[0.14em] text-[var(--text-tertiary)]">
                      Icon
                    </span>
                    <button
                      ref={iconBtnRef}
                      type="button"
                      onClick={() =>
                        setIconRect(iconBtnRef.current?.getBoundingClientRect() ?? null)
                      }
                      className="flex w-fit items-center gap-2 rounded px-1.5 py-1 text-xs text-[var(--text-tertiary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text-secondary)] cursor-pointer"
                    >
                      {draft.icon ?? (
                        <span className="inline-flex items-center gap-1">
                          <ImageIcon className="h-3.5 w-3.5" /> Set override
                        </span>
                      )}
                    </button>
                  </label>

                  {iconRect && (
                    <IconPicker
                      anchorRect={iconRect}
                      current={draft.icon ?? undefined}
                      onPick={(name) => setDraft((d) => ({ ...d, icon: name ?? null }))}
                      onClose={() => setIconRect(null)}
                    />
                  )}
                </div>
              )}
            </SectionEditor>
          </div>
        ) : (
          <>
            <Row label="Kind">{tag.type}</Row>
            <Row label="Technology">
              {node.technology || <span className="italic text-[var(--text-ghost)]">—</span>}
            </Row>
            {status && status !== "implemented" && (
              <Row label="Status">
                <StatusTag status={status} />
              </Row>
            )}
            {(node.deprecated || node.relocated) && (
              <Row label="Flags">
                <div className="flex flex-wrap gap-1.5">
                  {node.deprecated && (
                    <span
                      className={`${PILL_BASE} bg-red-500/10 text-red-700 ring-red-500/25 dark:bg-red-400/10 dark:text-red-300 dark:ring-red-400/25`}
                      title="Planned for removal"
                    >
                      deprecated
                    </span>
                  )}
                  {node.relocated && (
                    <span className={FLAG_COLORS.relocated.pill} title="Code needs to move">
                      relocated
                    </span>
                  )}
                </div>
              </Row>
            )}
          </>
        )}

        {(parent || group) && (
          <Row label="Part of">
            <ul className="-mx-1 flex flex-col">
              {parent && (
                <li>
                  <WikiLink
                    name={parent.name}
                    Icon={kindIcon(parent)}
                    red={isRedLink(parent)}
                    onClick={() => onSelectNode(parent.id)}
                  />
                </li>
              )}
              {group && (
                <li>
                  <WikiLink
                    name={group.name || "Group"}
                    onClick={() => onSelectGroup(group.id)}
                  />
                </li>
              )}
            </ul>
          </Row>
        )}

        {contents.length > 0 && <Row label="Contents">{contents.join(" · ")}</Row>}

        {(resps.length > 0 || props.length > 0) && (
          <Row label="Claims">
            <div className="flex flex-col gap-1">
              {resps.length > 0 && (
                <span className="flex flex-wrap items-center gap-1.5">
                  {resps.length === 1 ? "1 responsibility" : `${resps.length} responsibilities`}
                  {respPlanned > 0 && (
                    <span className={STATUS_COLORS.proposed.pill} title="Proposed or changed — the code doesn't discharge these yet">
                      {respPlanned} planned
                    </span>
                  )}
                  {respFlagged > 0 && (
                    <span className={FLAG_COLORS.stale.pill} title="Vagrant or stale — awaiting a verdict">
                      {respFlagged} flagged
                    </span>
                  )}
                </span>
              )}
              {props.length > 0 && (
                <span className="flex flex-wrap items-center gap-1.5">
                  {plural(props.length, "field")}
                  {propPlanned > 0 && (
                    <span className={STATUS_COLORS.proposed.pill} title="Proposed or changed — the code doesn't declare these yet">
                      {propPlanned} planned
                    </span>
                  )}
                </span>
              )}
            </div>
          </Row>
        )}

        {(incoming.length > 0 || outgoing.length > 0) && (
          /* Names only, at a glance — labels, evidence, and editing live in
             the page's Connections section. */
          <Row label="Connections">
            <ul className="-mx-1 flex flex-col">
              {[...outgoing.map((l) => ({ l, out: true })), ...incoming.map((l) => ({ l, out: false }))].map(
                ({ l, out }) => {
                  const p = byId(out ? l.dst : l.src);
                  return p ? (
                    <li key={l.id}>
                      <WikiLink
                        name={p.name}
                        Icon={kindIcon(p)}
                        dir={out ? "out" : "in"}
                        red={isRedLink(p)}
                        onClick={() => onSelectNode(p.id)}
                      />
                    </li>
                  ) : null;
                },
              )}
            </ul>
          </Row>
        )}

        {(boundary.length > 0 || ownFiles.length > 0) && (
          <Row label="Code">
            <ul className="flex flex-col gap-1">
              {boundary.length > 0
                ? boundary.map((s, i) => (
                    <li key={i} className="leading-relaxed">
                      <span className="font-mono text-2xs text-[var(--text-secondary)]">{s.pattern}</span>
                      {s.comment && (
                        <span className="text-2xs text-[var(--text-muted)]"> — {s.comment}</span>
                      )}
                    </li>
                  ))
                : ownFiles.map((f) => (
                    <li key={f} className="leading-relaxed">
                      <span className="break-all font-mono text-2xs text-[var(--text-secondary)]" title={f}>
                        {f}
                      </span>
                    </li>
                  ))}
              {coverage && coverage.totalFiles > 0 && (
                <li className="text-2xs text-[var(--text-muted)]">
                  {plural(coverage.totalFiles, "file")} · {coverage.anchoredFiles} anchored{" "}
                  {coverage.darkFiles.length > 0 && (
                    <span
                      className={FLAG_COLORS.stale.pill}
                      title={`No anchor reads into:\n${coverage.darkFiles.slice(0, 12).join("\n")}${coverage.darkFiles.length > 12 ? "\n…" : ""}`}
                    >
                      {coverage.darkFiles.length} dark
                    </span>
                  )}
                </li>
              )}
            </ul>
          </Row>
        )}

        {touched > 0 && (
          <Row label="Last touched">
            <span title={new Date(touched * 1000).toLocaleString()}>{relTime(touched)}</span>
          </Row>
        )}
      </div>
    </aside>
  );
}
