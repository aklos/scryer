/**
 * Right-hand infobox — structured at-a-glance metadata only (no restatement of
 * the page title/kind/status, which already lead the article). Reads by default;
 * its own [edit] toggle reveals the editable bits (technology, flags, icon).
 * Connections, status and boundary are derived/agent-owned, shown read-only.
 */

import { useRef, useState } from "react";
import { ImageIcon } from "lucide-react";
import type { ScryModel, Node } from "./viewmodel";
import type { Editor } from "./editor";
import { effectiveNodeStatus } from "./rollup";
import { kindIcon } from "./kindIcon";
import { IconPicker } from "./IconPicker";
import { Input, Toggle } from "./ui";
import { EditLink, StatusTag, WikiLink } from "./pagekit";

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex flex-col gap-1 px-4 py-2.5">
      <span className="text-[10px] font-semibold uppercase tracking-[0.14em] text-[var(--text-tertiary)]">
        {label}
      </span>
      <div className="text-[12.5px] text-[var(--text-secondary)]">{children}</div>
    </div>
  );
}

export function Infobox({
  model,
  node,
  editor,
  onSelectNode,
}: {
  model: ScryModel;
  node: Node;
  editor: Editor | undefined;
  onSelectNode: (id: string) => void;
}) {
  const [editing, setEditing] = useState(false);
  const [iconRect, setIconRect] = useState<DOMRect | null>(null);
  const iconBtnRef = useRef<HTMLButtonElement>(null);

  const status = effectiveNodeStatus(node);
  const outgoing = model.links.filter((l) => l.src === node.id);
  const incoming = model.links.filter((l) => l.dst === node.id);
  const boundary = model.boundaries?.[node.id] ?? [];
  const byId = (id: string) => model.nodes.find((n) => n.id === id);
  const patch = (p: Partial<Node>) => editor?.updateNode(node.id, p);
  const edit = editing && !!editor;

  return (
    <aside className="overflow-hidden rounded-md border border-[var(--border)] bg-[var(--surface-raised)] text-[var(--text)]">
      <div className="flex items-center justify-between border-b border-[var(--border-subtle)] px-4 py-2.5">
        <span className="text-[10px] font-semibold uppercase tracking-[0.14em] text-[var(--text-tertiary)]">
          Details
        </span>
        {editor && <EditLink editing={edit} onClick={() => setEditing((e) => !e)} />}
      </div>

      <div className="divide-y divide-[var(--border-subtle)]">
        <Row label="Technology">
          {edit ? (
            <Input
              variant="bordered"
              defaultValue={node.technology ?? ""}
              placeholder="e.g. PostgreSQL 16"
              onBlur={(e) => patch({ technology: e.currentTarget.value || undefined })}
              className="text-[12.5px]"
            />
          ) : node.technology ? (
            node.technology
          ) : (
            <span className="italic text-[var(--text-ghost)]">—</span>
          )}
        </Row>

        {status && status !== "implemented" && (
          <Row label="Status">
            <StatusTag status={status} />
          </Row>
        )}

        {(incoming.length > 0 || outgoing.length > 0) && (
          <Row label="Connections">
            <ul className="-mx-1 flex flex-col">
              {outgoing.map((l) => {
                const p = byId(l.dst);
                return p ? (
                  <li key={l.id} className="flex items-center gap-1">
                    <WikiLink name={p.name} Icon={kindIcon(p)} dir="out" onClick={() => onSelectNode(p.id)} />
                    {l.label && <span className="truncate text-[11px] text-[var(--text-muted)]">{l.label}</span>}
                  </li>
                ) : null;
              })}
              {incoming.map((l) => {
                const p = byId(l.src);
                return p ? (
                  <li key={l.id} className="flex items-center gap-1">
                    <WikiLink name={p.name} Icon={kindIcon(p)} dir="in" onClick={() => onSelectNode(p.id)} />
                    {l.label && <span className="truncate text-[11px] text-[var(--text-muted)]">{l.label}</span>}
                  </li>
                ) : null;
              })}
            </ul>
          </Row>
        )}

        {boundary.length > 0 && (
          <Row label="Boundary">
            <ul className="flex flex-col gap-1">
              {boundary.map((s, i) => (
                <li key={i} className="leading-relaxed">
                  <span className="font-mono text-[11px] text-[var(--text-secondary)]">{s.pattern}</span>
                  {s.comment && <span className="text-[11px] text-[var(--text-muted)]"> — {s.comment}</span>}
                </li>
              ))}
            </ul>
          </Row>
        )}

        {edit && (
          <>
            <Row label="Flags">
              <div className="flex flex-col gap-2">
                <label className="flex items-center justify-between gap-2">
                  <span className="text-[12px] text-[var(--text-secondary)]">
                    Visual <span className="text-[var(--text-ghost)]">— renders UI</span>
                  </span>
                  <Toggle value={!!node.visual} onChange={(v) => patch({ visual: v || undefined })} />
                </label>
                <label className="flex items-center justify-between gap-2">
                  <span className="text-[12px] text-[var(--text-secondary)]">
                    Deprecated <span className="text-[var(--text-ghost)]">— planned for removal</span>
                  </span>
                  <Toggle value={!!node.deprecated} onChange={(v) => patch({ deprecated: v || undefined })} />
                </label>
                <label className="flex items-center justify-between gap-2">
                  <span className="text-[12px] text-[var(--text-secondary)]">
                    Relocated <span className="text-[var(--text-ghost)]">— code needs to move</span>
                  </span>
                  <Toggle value={!!node.relocated} onChange={(v) => patch({ relocated: v || undefined })} />
                </label>
              </div>
            </Row>
            <Row label="Icon">
              <button
                ref={iconBtnRef}
                type="button"
                onClick={() => setIconRect(iconBtnRef.current?.getBoundingClientRect() ?? null)}
                className="flex items-center gap-2 rounded px-1.5 py-1 text-[12px] text-[var(--text-tertiary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text-secondary)] cursor-pointer"
              >
                {node.icon ? (
                  node.icon
                ) : (
                  <span className="inline-flex items-center gap-1">
                    <ImageIcon className="h-3.5 w-3.5" /> Set override
                  </span>
                )}
              </button>
            </Row>
          </>
        )}
      </div>

      {iconRect && (
        <IconPicker
          anchorRect={iconRect}
          current={node.icon}
          onPick={(name) => patch({ icon: name })}
          onClose={() => setIconRect(null)}
        />
      )}
    </aside>
  );
}
