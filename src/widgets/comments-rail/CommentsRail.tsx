/**
 * The comments rail beside a node's page: threads people and agents left on
 * this node, open ones first, each worked like an issue — replies, then an
 * explicit close. Closed threads fold to one line until opened. Your own
 * comments delete in two clicks; the opening one takes its thread with it.
 */

import { useState } from "react";
import { CircleCheck, Trash2 } from "lucide-react";
import type { CommentEvent, Thread, ThreadStatus } from "../../entities/model/comments";
import { AgentMark, BTN, BTN_GO, EYEBROW } from "../../shared/ui/tokens";

// A boxed field: the composer and the reply line are inputs, and read as such.
const BOX =
  "block w-full resize-none rounded-md border border-[var(--border)] bg-[var(--surface-field)] px-2.5 py-1.5 text-sm leading-snug text-[var(--text)] outline-none transition-colors placeholder:text-[var(--text-ghost)] focus:border-[var(--accent)] focus:ring-1 focus:ring-[var(--accent)]";
// The reply line inside a card: no box of its own, the card's footer is it.
const LINE =
  "block w-full resize-none bg-transparent px-3 py-2 text-sm leading-snug text-[var(--text)] outline-none placeholder:text-[var(--text-ghost)]";
const CARD = "overflow-hidden rounded-md border border-[var(--border)]";

/** "just now", "12m ago", "3h ago", else the day. */
function ago(ms: number): string {
  const s = Math.max(0, (Date.now() - ms) / 1000);
  if (s < 60) return "just now";
  if (s < 3600) return `${Math.floor(s / 60)}m ago`;
  if (s < 86400) return `${Math.floor(s / 3600)}h ago`;
  return new Date(ms).toLocaleDateString([], { month: "short", day: "numeric" });
}

const stamp = (ms: number) =>
  new Date(ms).toLocaleString([], { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" });

/** Ctrl/Cmd+Enter sends. */
const onSend = (send: () => void) => (ev: React.KeyboardEvent) => {
  if (ev.key === "Enter" && (ev.metaKey || ev.ctrlKey)) {
    ev.preventDefault();
    send();
  }
};

function Event({
  e,
  deleteLabel,
  onDelete,
}: {
  e: CommentEvent;
  /** What deleting this event deletes, when you may; absent otherwise. */
  deleteLabel?: string;
  onDelete: () => Promise<void>;
}) {
  const [armed, setArmed] = useState(false);
  return (
    <li className="group/ev flex flex-col gap-1 px-3 py-2" onMouseLeave={() => setArmed(false)}>
      <span className="flex h-5 items-center gap-1.5 text-xs text-[var(--text-tertiary)]">
        {e.author === "agent" && <AgentMark />}
        <span className="font-medium text-[var(--text-secondary)]">{e.author === "human" ? "You" : "Agent"}</span>
        {e.status === "addressed" && <span className="text-violet-700 dark:text-violet-400">closed this</span>}
        {e.status === "open" && <span className="text-emerald-700 dark:text-emerald-400">reopened this</span>}
        <span aria-hidden>·</span>
        <span className="whitespace-nowrap" title={stamp(e.at)}>
          {ago(e.at)}
        </span>
        {deleteLabel &&
          (armed ? (
            <button
              type="button"
              onClick={() => void onDelete()}
              className="ml-auto rounded px-1 font-medium text-red-700 hover:bg-red-500/10 dark:text-red-400"
            >
              {deleteLabel}
            </button>
          ) : (
            <button
              type="button"
              title={deleteLabel}
              aria-label={deleteLabel}
              onClick={() => setArmed(true)}
              className="ml-auto rounded p-0.5 text-[var(--text-tertiary)] opacity-0 transition-opacity hover:bg-[var(--surface-hover)] hover:text-red-600 focus-visible:opacity-100 group-hover/ev:opacity-100 dark:hover:text-red-400"
            >
              <Trash2 className="h-3.5 w-3.5" />
            </button>
          ))}
      </span>
      <span className="whitespace-pre-wrap break-words text-sm leading-snug text-[var(--text)]">{e.text}</span>
    </li>
  );
}

function ThreadCard({
  thread,
  onReply,
  onDelete,
}: {
  thread: Thread;
  onReply: (text: string, status?: ThreadStatus) => Promise<void>;
  onDelete: (event: string) => Promise<void>;
}) {
  const closed = thread.status === "addressed";
  const [expanded, setExpanded] = useState(!closed);
  const [draft, setDraft] = useState("");
  const [busy, setBusy] = useState(false);
  const send = async (status?: ThreadStatus) => {
    if (!draft.trim() || busy) return;
    setBusy(true);
    try {
      await onReply(draft, status);
      setDraft("");
    } finally {
      setBusy(false);
    }
  };
  const first = thread.events[0];
  if (!expanded) {
    return (
      <button
        type="button"
        onClick={() => setExpanded(true)}
        className={`${CARD} flex items-center gap-2 px-3 py-2 text-left transition-colors hover:bg-[var(--surface-hover)]`}
      >
        <CircleCheck className="h-3.5 w-3.5 shrink-0 text-violet-700 dark:text-violet-400" aria-label="Addressed" />
        <span className="truncate text-sm text-[var(--text-muted)]">{first?.text}</span>
      </button>
    );
  }
  return (
    <div className={CARD}>
      {closed && (
        <div className="flex items-center gap-1.5 border-b border-[var(--border)] px-3 py-1.5 text-xs font-medium text-violet-700 dark:text-violet-400">
          <CircleCheck className="h-3.5 w-3.5" /> Addressed
          <button
            type="button"
            onClick={() => setExpanded(false)}
            className="ml-auto font-normal text-[var(--text-tertiary)] hover:text-[var(--text)]"
          >
            Fold
          </button>
        </div>
      )}
      <ul className="flex flex-col divide-y divide-[var(--border)]">
        {thread.events.map((e, i) => (
          <Event
            key={e.id || i}
            e={e}
            deleteLabel={
              e.author !== "human" ? undefined : i === 0 && thread.events.length > 1 ? "Delete thread" : "Delete"
            }
            onDelete={() => onDelete(e.id)}
          />
        ))}
      </ul>
      <div className="border-t border-[var(--border)]">
        <textarea
          rows={draft ? 3 : 1}
          value={draft}
          onChange={(ev) => setDraft(ev.target.value)}
          onKeyDown={onSend(() => void send())}
          placeholder={closed ? "Reopen with a reason…" : "Reply…"}
          className={LINE}
        />
        {draft.trim() && (
          <div className="flex justify-end gap-2 px-3 pb-2">
            {closed ? (
              <button type="button" disabled={busy} onClick={() => void send("open")} className={BTN}>
                Reopen
              </button>
            ) : (
              <>
                <button type="button" disabled={busy} onClick={() => void send("addressed")} className={BTN}>
                  Reply and close
                </button>
                <button type="button" disabled={busy} onClick={() => void send()} className={BTN_GO}>
                  Reply
                </button>
              </>
            )}
          </div>
        )}
      </div>
    </div>
  );
}

export function CommentsRail({
  threads,
  onOpen,
  onReply,
  onDelete,
}: {
  /** This node's threads, open ones first. */
  threads: Thread[];
  onOpen: (text: string) => Promise<void>;
  onReply: (thread: string, text: string, status?: ThreadStatus) => Promise<void>;
  onDelete: (thread: string, event: string) => Promise<void>;
}) {
  const [draft, setDraft] = useState("");
  const [busy, setBusy] = useState(false);
  const open = threads.filter((t) => t.status === "open").length;
  const start = async () => {
    if (!draft.trim() || busy) return;
    setBusy(true);
    try {
      await onOpen(draft);
      setDraft("");
    } finally {
      setBusy(false);
    }
  };
  return (
    // Beside the article in a wide page, a full-width section under it in a
    // narrow one (the page is the size container).
    <aside
      data-comments-rail
      className="flex w-full shrink-0 flex-col gap-3 @3xl:sticky @3xl:top-0 @3xl:w-[280px] @3xl:self-start"
    >
      {/* The page sections' heading: eyebrow over a rule. */}
      <h2 className={`${EYEBROW} border-b border-[var(--border)] pb-[5px]`}>
        Comments
        {open > 0 && (
          <span className="ml-1.5 font-mono text-2xs font-normal normal-case tracking-normal text-[var(--text-ghost)]">
            {open} open
          </span>
        )}
      </h2>
      <div className="flex flex-col gap-2">
        <textarea
          rows={draft ? 3 : 1}
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={onSend(() => void start())}
          placeholder="Ask an agent, or leave a decision…"
          title="Ask an agent, or leave a decision for whoever works here next"
          className={BOX}
        />
        {draft.trim() && (
          <button type="button" disabled={busy} onClick={() => void start()} className={`${BTN_GO} self-end`}>
            Comment
          </button>
        )}
      </div>
      {threads.map((t) => (
        <ThreadCard
          key={t.id}
          thread={t}
          onReply={(text, status) => onReply(t.id, text, status)}
          onDelete={(event) => onDelete(t.id, event)}
        />
      ))}
    </aside>
  );
}
