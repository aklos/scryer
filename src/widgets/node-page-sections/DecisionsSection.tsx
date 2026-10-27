/**
 * The decisions a later reader needs: why this shape, what was rejected,
 * what must stay true. They are the node's module doc after its first
 * paragraph (the system's `ARCHITECTURE.md` after its first), so they move
 * with the code they explain. A node also lists the decisions it inherits
 * from the nodes around it, nearest first, each a link to where it is
 * written.
 */

import type { ReactNode } from "react";
import type { Editor } from "../../entities/model/editor";
import type { Node, ScryModel } from "../../entities/model/viewmodel";
import { PageSection, SectionEditor } from "../../shared/ui/pagekit";

/** The ancestors' decisions, nearest first. */
export function inheritedDecisions(model: ScryModel, node: Node): { id: string; name: string; decisions: string }[] {
  const byId = new Map(model.nodes.map((n) => [n.id, n] as const));
  const out: { id: string; name: string; decisions: string }[] = [];
  for (let p = node.parentId ? byId.get(node.parentId) : undefined; p; p = p.parentId ? byId.get(p.parentId) : undefined)
    if (p.decisions) out.push({ id: p.id, name: p.name, decisions: p.decisions });
  return out;
}

/** The first line a decision text opens with, list marker dropped. */
export function firstLine(text: string): string {
  const line = text.split("\n").find((l) => l.trim() !== "") ?? "";
  return line.replace(/^\s*(?:[-*]|\d+\.)\s+/, "").replace(/^#+\s*/, "");
}

/** Inline `code` spans, the rest plain. A Rust intra-doc link's brackets
 *  (``[`name`]``) are dropped: the name is the reference. */
function inline(text: string): ReactNode[] {
  return text
    .replace(/\[(`[^`]+`)\]/g, "$1")
    .split(/(`[^`]+`)/)
    .map((part, i) =>
    part.startsWith("`") && part.endsWith("`") && part.length > 1 ? (
      <code key={i} className="rounded bg-[var(--surface-hover)] px-1 font-mono text-[0.9em]">
        {part.slice(1, -1)}
      </code>
    ) : (
      part
    ),
  );
}

/** Markdown as decisions are written: paragraphs and `-` lists, inline code. */
export function DecisionText({ text }: { text: string }) {
  const blocks = text.split(/\n\s*\n/).filter((b) => b.trim() !== "");
  return (
    <div className="flex flex-col gap-2 text-sm leading-relaxed text-[var(--text-secondary)]">
      {blocks.map((block, i) => {
        const lines = block.split("\n");
        const items = lines.every((l) => /^\s*[-*]\s+/.test(l) || /^\s{2,}\S/.test(l));
        if (items) {
          const merged = lines.reduce<string[]>((acc, l) => {
            if (/^\s*[-*]\s+/.test(l)) acc.push(l.replace(/^\s*[-*]\s+/, ""));
            else acc[acc.length - 1] += ` ${l.trim()}`;
            return acc;
          }, []);
          return (
            <ul key={i} className="ml-4 list-disc">
              {merged.map((item, j) => (
                <li key={j}>{inline(item)}</li>
              ))}
            </ul>
          );
        }
        return <p key={i}>{inline(lines.map((l) => l.trim()).join(" "))}</p>;
      })}
    </div>
  );
}

export function DecisionsSection({
  node,
  model,
  editor,
  editing,
  onToggle,
  onSelectNode,
}: {
  node: Node;
  model: ScryModel;
  editor: Editor | undefined;
  editing: boolean;
  onToggle: () => void;
  onSelectNode: (id: string) => void;
}) {
  const inherited = inheritedDecisions(model, node);
  const own = node.decisions ?? "";
  if (!own && inherited.length === 0 && !editor) return null;
  return (
    <PageSection
      title="Decisions"
      hint="Why this is shaped as it is, what was rejected, what must stay true. Written in the module doc after its first paragraph, so they move with the code."
      editable={!!editor}
      editing={editing}
      onToggleEdit={onToggle}
    >
      {editing && editor ? (
        <SectionEditor<string>
          initial={own}
          onCommit={(text) => editor.updateNode(node.id, { decisions: text })}
          onClose={onToggle}
        >
          {(draft, setDraft) => (
            <textarea
              autoFocus
              value={draft}
              onChange={(e) => setDraft(e.target.value)}
              rows={Math.max(4, draft.split("\n").length + 1)}
              placeholder="- Chose X over Y, because …"
              className="w-full resize-y rounded border border-[var(--border)] bg-[var(--surface-field)] px-2 py-1.5 font-mono text-xs leading-relaxed text-[var(--text)] focus:border-[var(--accent)] focus:outline-none"
            />
          )}
        </SectionEditor>
      ) : own ? (
        <DecisionText text={own} />
      ) : (
        <p className="text-sm italic text-[var(--text-muted)]">None written here.</p>
      )}
      {inherited.length > 0 && !editing && (
        <ul className="mt-3 flex flex-col gap-0.5 border-t border-[var(--border-subtle)] pt-2">
          {inherited.map((d) => (
            <li key={d.id} className="flex min-w-0 items-baseline gap-2 text-xs">
              <button
                type="button"
                onClick={() => onSelectNode(d.id)}
                className="shrink-0 text-[var(--text-muted)] hover:text-[var(--text)] hover:underline"
              >
                from {d.name}
              </button>
              <span className="min-w-0 truncate text-[var(--text-secondary)]">{inline(firstLine(d.decisions))}</span>
            </li>
          ))}
        </ul>
      )}
    </PageSection>
  );
}
