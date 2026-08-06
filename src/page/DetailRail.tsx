import { useEffect, useRef } from "react";
import { CornerDownRight } from "lucide-react";
import type { ScryModel, Node } from "../viewmodel";
import type { Editor } from "../editor";
import { DIFF_TINT } from "../diffkit";
import {
  BTN,
  BTN_DANGER,
  Editable,
  EditLink,
  Empty,
  EYEBROW,
  SectionEditor,
} from "../pagekit";
import { CTL_DROW, DIR_HL } from "./kit";

// --- notes gutter ------------------------------------------------------------

/** Render notes read-only: lines starting with `- ` (or `• `) group into a
 *  bullet list; everything else is a paragraph. */
function NotesRead({ text }: { text: string }) {
  const blocks: React.ReactNode[] = [];
  let bullets: string[] = [];
  const flush = () => {
    if (!bullets.length) return;
    const items = bullets;
    blocks.push(
      <ul key={blocks.length} className="list-disc space-y-0.5 pl-4">
        {items.map((b, i) => (
          <li key={i}>{b}</li>
        ))}
      </ul>,
    );
    bullets = [];
  };
  for (const line of text.split("\n")) {
    const m = line.match(/^\s*[-•]\s+(.*)$/);
    if (m) {
      bullets.push(m[1]);
    } else {
      flush();
      if (line.trim())
        blocks.push(
          <p key={blocks.length}>
            {line}
          </p>,
        );
    }
  }
  flush();
  return <div className="flex flex-col gap-1.5">{blocks}</div>;
}

/** Invisible in-place notes editor: a transparent, borderless, auto-growing
 *  textarea with the same metrics as the read view (so the swap doesn't
 *  reflow). Enter on a non-empty bullet line auto-continues the list with a new
 *  `- `; Shift+Enter is always a plain newline. */
function NotesEditable({
  initial,
  onInput,
}: {
  initial: string;
  onInput: (text: string) => void;
}) {
  const ref = useRef<HTMLTextAreaElement>(null);
  const grow = (el: HTMLTextAreaElement) => {
    el.style.height = "auto";
    el.style.height = `${el.scrollHeight}px`;
  };
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    grow(el);
    el.focus();
    el.setSelectionRange(el.value.length, el.value.length);
  }, []);
  return (
    <textarea
      ref={ref}
      defaultValue={initial}
      rows={2}
      placeholder="Notes to self. Start a line with “- ” for a bullet."
      onInput={(e) => {
        grow(e.currentTarget);
        onInput(e.currentTarget.value);
      }}
      onKeyDown={(e) => {
        if (e.key !== "Enter" || e.shiftKey) return;
        const ta = e.currentTarget;
        const lineStart = ta.value.lastIndexOf("\n", ta.selectionStart - 1) + 1;
        const line = ta.value.slice(lineStart, ta.selectionStart);
        const m = line.match(/^\s*[-•]\s+(.*)$/);
        if (m && m[1].trim() !== "") {
          e.preventDefault();
          ta.setRangeText("\n- ", ta.selectionStart, ta.selectionEnd, "end");
          grow(ta);
          onInput(ta.value);
        }
      }}
      // The same field treatment as every Editable: recessed field surface,
      // accent ring and caret, full-contrast text — an invisible textarea made
      // edit mode look like nothing had happened.
      className="-mx-1 w-full resize-none whitespace-pre-wrap rounded bg-[var(--surface-field)] px-1 text-sm leading-relaxed text-[var(--text)] caret-[var(--accent)] outline-none ring-1 ring-[var(--accent)] placeholder:text-[var(--text-muted)]"
    />
  );
}

/** A rail section header: the uppercase eyebrow on a rule, with a hover-revealed
 *  [Edit] toggle. `revealClass` must be a LITERAL Tailwind string (e.g.
 *  `"invisible group-hover/dir:visible"`) — Tailwind's JIT can't see classes
 *  built from template parts, so callers pass the whole class. */
function RailHeader({
  title,
  revealClass,
  editable,
  editing,
  onToggle,
}: {
  title: string;
  revealClass: string;
  editable: boolean;
  editing: boolean;
  onToggle: () => void;
}) {
  return (
    <div className="mb-2 flex items-end justify-between gap-2 border-b border-[var(--border)] pb-[5px]">
      <h2 className={EYEBROW}>
        {title}
      </h2>
      {editable && !editing && (
        <EditLink onClick={onToggle} className={revealClass} />
      )}
    </div>
  );
}

/** Read-only directive bullets, rendered as plain text. */
function DirectiveList({ directives }: { directives: readonly string[] }) {
  return (
    <ul className="list-disc space-y-1.5 pl-4 text-sm leading-relaxed text-[var(--text-secondary)]">
      {directives.map((d, i) => (
        <li key={i}>{d}</li>
      ))}
    </ul>
  );
}

/** Own directives as a plan diff, one bullet per entry — additions tinted green,
 *  removals struck red, unchanged plain. Keeps the bulleted list and its spacing
 *  (an edit reads as the old line struck + the new line added). */
function DirectiveDiffList({ prev, next }: { prev: readonly string[]; next: readonly string[] }) {
  const prevSet = new Set(prev);
  const nextSet = new Set(next);
  const removed = prev.filter((d) => !nextSet.has(d));
  return (
    <ul className="list-disc space-y-1.5 pl-4 text-sm leading-relaxed text-[var(--text-secondary)]">
      {next.map((d, i) => (
        <li key={`n${i}`}>{prevSet.has(d) ? d : <span className={DIFF_TINT.add}>{d}</span>}</li>
      ))}
      {removed.map((d, i) => (
        <li key={`r${i}`} className="marker:text-red-400/60">
          <span className={DIFF_TINT.delete}>{d}</span>
        </li>
      ))}
    </ul>
  );
}

/** Top of the detail rail: the node's OWN node-level directives (user-authored
 *  HOW-constraints), editable here. Ancestors' directives still BIND this node
 *  — `inherited_directives` hands the agent the full set through orient/locate
 *  — but they are not shown: repeating an ancestor's constraint on every
 *  descendant page is noise the reader has to re-skip, and the page it belongs
 *  to is one click up the breadcrumb. Mirrors the Notes section's chrome.
 *  User-only; the agent reads directives but never authors them. */
function DirectivesSection({
  node,
  committed,
  editor,
  editing,
  onToggle,
}: {
  node: Node;
  committed: ScryModel | null;
  editor: Editor | undefined;
  editing: boolean;
  onToggle: () => void;
}) {
  const own = node.directives ?? [];
  // The own list shows its plan divergence inline, on the same joined string the
  // plan diff tracks: additions paint green, removals strike through, edits
  // word-diff — every case, not just edit-in-place. Only an unchanged list (or
  // one with no committed base yet) reads plain.
  const prevOwn = committed?.nodes.find((n) => n.id === node.id)?.directives ?? [];
  const ownJoined = own.join("\n");
  const prevJoined = prevOwn.join("\n");
  const changed = !!committed && prevJoined !== ownJoined;
  const nothing = !changed && own.length === 0;
  return (
    // In edit mode the section wears the same shell as PageSection — inset
    // surface + accent ring; the negative margins compensate the padding so
    // nothing moves when it toggles.
    <div
      className={`group/dir ${
        editing
          ? "-mx-3 -my-2 rounded-md bg-[var(--surface-inset)] px-3 py-2 ring-1 ring-[color-mix(in_srgb,var(--accent)_35%,transparent)]"
          : ""
      }`}
    >
      <RailHeader
        title="Node directives"
        revealClass="invisible group-hover/dir:visible"
        editable={!!editor}
        editing={editing}
        onToggle={onToggle}
      />
      {editing && editor ? (
        <SectionEditor<{ text: string; removed?: boolean }[]>
          initial={own.map((text) => ({ text }))}
          onCommit={(draft) => {
            const next = draft
              .filter((d) => !d.removed)
              .map((d) => d.text.trim())
              .filter(Boolean);
            editor.updateNode(node.id, { directives: next.length ? next : undefined });
          }}
          onClose={onToggle}
          footerExtra={(setDraft) => (
            <button type="button" onClick={() => setDraft((d) => [...d, { text: "" }])} className={BTN}>
              Add directive
            </button>
          )}
        >
          {(draft, setDraft) =>
            draft.length === 0 ? (
              <Empty>No directives — add one.</Empty>
            ) : (
              <div className="flex flex-col gap-0.5">
                {draft.map((d, i) =>
                  d.removed ? (
                    // Soft-deleted: struck with Undo, dropped only on Done.
                    <div
                      key={i}
                      className="flex items-baseline gap-1.5 text-sm leading-relaxed"
                    >
                      <CornerDownRight className="h-3 w-3 shrink-0 translate-y-px text-[var(--text-ghost)]" />
                      <span className="min-w-0 truncate text-[var(--text-muted)] line-through decoration-red-400/50">
                        {d.text.trim() || "(empty)"}
                      </span>
                      <button
                        type="button"
                        onClick={() =>
                          setDraft((arr) => arr.map((x, j) => (j === i ? { ...x, removed: false } : x)))
                        }
                        className={BTN}
                      >
                        Undo
                      </button>
                    </div>
                  ) : (
                    <div
                      key={i}
                      className="group/drow relative flex items-baseline gap-1.5 text-sm leading-relaxed text-[var(--text-secondary)]"
                    >
                      <CornerDownRight className="h-3 w-3 shrink-0 translate-y-px text-[var(--text-ghost)]" />
                      <Editable
                        initial={d.text}
                        autoFocus={d.text === ""}
                        placeholder={'must … / never …'}
                        onInput={(t) =>
                          setDraft((arr) => arr.map((x, j) => (j === i ? { ...x, text: t } : x)))
                        }
                        className={`block min-w-0 flex-1 rounded !pr-16 ${DIR_HL}`}
                      />
                      <span className={CTL_DROW}>
                        <button
                          type="button"
                          title="Remove directive"
                          onClick={() =>
                            setDraft((arr) => arr.map((x, j) => (j === i ? { ...x, removed: true } : x)))
                          }
                          className={BTN_DANGER}
                        >
                          Delete
                        </button>
                      </span>
                    </div>
                  ),
                )}
              </div>
            )
          }
        </SectionEditor>
      ) : (
        <div className="flex flex-col gap-3">
          {changed ? (
            <DirectiveDiffList prev={prevOwn} next={own} />
          ) : own.length > 0 ? (
            <DirectiveList directives={own} />
          ) : null}
          {/* Empty state: one quiet affordance instead of a placeholder that
              says nothing — read-only emptiness renders nothing at all (the
              rail collapses; see DetailRail). */}
          {nothing && editor && (
            <button
              type="button"
              onClick={onToggle}
              className="self-start text-sm text-[var(--text-ghost)] hover:text-[var(--text-secondary)]"
            >
              + Add a directive
            </button>
          )}
        </div>
      )}
    </div>
  );
}

/**
 * The right-margin detail rail. Top: this node's OWN directives (editable) —
 * an ancestor's directives bind too, but they live on the ancestor's page, not
 * repeated here. Below: the user's own freeform notes — self-context and
 * traversal aids, NOT part of the spec. User-only; the agent authors neither.
 * Node-only (groups carry no `notes` or node-level `directives`).
 */
export function DetailRail({
  node,
  committed,
  editor,
  notesEditing,
  onToggleNotes,
  dirEditing,
  onToggleDir,
}: {
  node: Node;
  committed: ScryModel | null;
  editor: Editor | undefined;
  notesEditing: boolean;
  onToggleNotes: () => void;
  dirEditing: boolean;
  onToggleDir: () => void;
}) {
  const notes = node.notes;
  // With no editor and nothing to say, the rail says nothing — a column of
  // "No directives / No notes" is dead space, not information.
  const own = node.directives ?? [];
  const prevOwn = committed?.nodes.find((n) => n.id === node.id)?.directives ?? [];
  const hasDirectives =
    own.length > 0 || (!!committed && prevOwn.join("\n") !== own.join("\n"));
  if (!editor && !hasDirectives && !notes) return null;
  return (
    <aside className="ml-auto hidden w-[300px] shrink-0 lg:block">
      <div className="sticky top-0 flex flex-col gap-8">
        <DirectivesSection
          node={node}
          committed={committed}
          editor={editor}
          editing={dirEditing}
          onToggle={onToggleDir}
        />
        <div
          className={`group/notes ${
            notesEditing
              ? "-mx-3 -my-2 rounded-md bg-[var(--surface-inset)] px-3 py-2 ring-1 ring-[color-mix(in_srgb,var(--accent)_35%,transparent)]"
              : ""
          }`}
        >
          <RailHeader
            title="Notes"
            revealClass="invisible group-hover/notes:visible"
            editable={!!editor}
            editing={notesEditing}
            onToggle={onToggleNotes}
          />
          {notesEditing && editor ? (
            <SectionEditor<string>
              initial={notes ?? ""}
              onCommit={(v) => editor.updateNode(node.id, { notes: v.trim() || undefined })}
              onClose={onToggleNotes}
            >
              {(_draft, setDraft) => (
                <NotesEditable initial={notes ?? ""} onInput={setDraft} />
              )}
            </SectionEditor>
          ) : notes ? (
            <div className="text-sm leading-relaxed text-[var(--text-secondary)]">
              <NotesRead text={notes} />
            </div>
          ) : (
            editor && (
              <button
                type="button"
                onClick={onToggleNotes}
                className="block text-left text-sm text-[var(--text-ghost)] hover:text-[var(--text-secondary)]"
              >
                + Add a note
              </button>
            )
          )}
        </div>
      </div>
    </aside>
  );
}
